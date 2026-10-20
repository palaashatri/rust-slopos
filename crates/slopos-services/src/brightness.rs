use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use slopos_core::capability::Availability;
use thiserror::Error;
use tokio::{
    sync::{mpsc, Mutex},
    task::JoinHandle,
    time::{interval, timeout},
};
use zbus::{Connection, Proxy};

const LOGIN1: &str = "org.freedesktop.login1";
const LOGIN1_PATH: &str = "/org/freedesktop/login1";
const LOGIN1_MANAGER: &str = "org.freedesktop.login1.Manager";
const LOGIN1_SESSION: &str = "org.freedesktop.login1.Session";
const OPERATION_TIMEOUT: Duration = Duration::from_secs(5);
const WATCH_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrightnessDevice {
    pub name: String,
    pub current: u32,
    pub minimum: u32,
    pub maximum: u32,
}

impl BrightnessDevice {
    pub fn percent(&self) -> u8 {
        let range = u64::from(self.maximum - self.minimum);
        if range == 0 {
            return 0;
        }
        (u64::from(self.current.saturating_sub(self.minimum)) * 100 / range) as u8
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrightnessSnapshot {
    pub revision: u64,
    pub availability: Availability,
    pub devices: Vec<BrightnessDevice>,
}

pub struct BrightnessEventStream {
    receiver: mpsc::Receiver<Result<BrightnessSnapshot, BrightnessError>>,
    task: Option<JoinHandle<()>>,
}

impl BrightnessEventStream {
    pub async fn next(&mut self) -> Option<Result<BrightnessSnapshot, BrightnessError>> {
        self.receiver.recv().await
    }

    pub async fn close(mut self) {
        self.receiver.close();
        if let Some(task) = self.task.take() {
            task.abort();
            let _ = task.await;
        }
    }
}

impl Drop for BrightnessEventStream {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

#[derive(Debug, Error)]
pub enum BrightnessError {
    #[error("brightness operation timed out")]
    Timeout,
    #[error("brightness device is unavailable: {0}")]
    DeviceUnavailable(String),
    #[error("brightness value is outside the device range")]
    OutOfRange,
    #[error("brightness device data is invalid: {0}")]
    InvalidDevice(String),
    #[error("logind denied the brightness change: {0}")]
    PermissionDenied(String),
    #[error("logind is unavailable: {0}")]
    Service(String),
    #[error("brightness I/O failed: {0}")]
    Io(#[from] io::Error),
}

impl BrightnessError {
    pub fn availability(&self) -> Availability {
        match self {
            Self::DeviceUnavailable(_) => Availability::UnsupportedHardware,
            Self::PermissionDenied(_) => Availability::PermissionDenied,
            Self::Service(_) => Availability::MissingService,
            Self::Timeout | Self::OutOfRange | Self::InvalidDevice(_) | Self::Io(_) => {
                Availability::Failed
            }
        }
    }
}

#[derive(Clone)]
pub struct BrightnessClient {
    root: PathBuf,
    revision: Arc<AtomicU64>,
    mutation_lock: Arc<Mutex<()>>,
}

impl Default for BrightnessClient {
    fn default() -> Self {
        Self::new()
    }
}

impl BrightnessClient {
    pub fn new() -> Self {
        Self::with_root("/sys/class/backlight")
    }

    fn with_root(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            revision: Arc::new(AtomicU64::new(0)),
            mutation_lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn snapshot(&self) -> Result<BrightnessSnapshot, BrightnessError> {
        let devices = read_devices(&self.root)?;
        let availability = if devices.is_empty() {
            Availability::UnsupportedHardware
        } else {
            Availability::Available
        };
        Ok(BrightnessSnapshot {
            revision: self.revision.fetch_add(1, Ordering::Relaxed) + 1,
            availability,
            devices,
        })
    }

    pub fn watch(&self) -> BrightnessEventStream {
        let (sender, receiver) = mpsc::channel(4);
        let client = self.clone();
        let task = tokio::spawn(async move {
            let mut timer = interval(WATCH_INTERVAL);
            let mut previous = None;
            loop {
                timer.tick().await;
                match client.snapshot() {
                    Ok(snapshot) if previous.as_ref() != Some(&snapshot.devices) => {
                        previous = Some(snapshot.devices.clone());
                        if sender.send(Ok(snapshot)).await.is_err() {
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(error) => {
                        if sender.send(Err(error)).await.is_err() {
                            break;
                        }
                        previous = None;
                    }
                }
            }
        });
        BrightnessEventStream {
            receiver,
            task: Some(task),
        }
    }

    pub async fn set(&self, name: &str, value: u32) -> Result<BrightnessSnapshot, BrightnessError> {
        let _guard = self.mutation_lock.lock().await;
        let device = read_devices(&self.root)?
            .into_iter()
            .find(|device| device.name == name)
            .ok_or_else(|| BrightnessError::DeviceUnavailable(name.to_owned()))?;
        if value < device.minimum || value > device.maximum {
            return Err(BrightnessError::OutOfRange);
        }

        timeout(OPERATION_TIMEOUT, async {
            let connection = Connection::system()
                .await
                .map_err(|error| BrightnessError::Service(error.to_string()))?;
            let manager = Proxy::new(&connection, LOGIN1, LOGIN1_PATH, LOGIN1_MANAGER)
                .await
                .map_err(|error| BrightnessError::Service(error.to_string()))?;
            let session: zbus::zvariant::OwnedObjectPath = manager
                .call("GetSessionByPID", &(std::process::id()))
                .await
                .map_err(|error| map_dbus_error(error.to_string()))?;
            let session_proxy = Proxy::new(&connection, LOGIN1, session, LOGIN1_SESSION)
                .await
                .map_err(|error| BrightnessError::Service(error.to_string()))?;
            session_proxy
                .call::<_, _, ()>("SetBrightness", &("backlight", name, value))
                .await
                .map_err(|error| map_dbus_error(error.to_string()))?;

            let updated = read_devices(&self.root)?
                .into_iter()
                .find(|item| item.name == name)
                .ok_or_else(|| BrightnessError::DeviceUnavailable(name.to_owned()))?;
            if updated.current != value {
                return Err(BrightnessError::InvalidDevice(format!(
                    "read-back for {name} was {}, requested {value}",
                    updated.current
                )));
            }
            self.snapshot()
        })
        .await
        .map_err(|_| BrightnessError::Timeout)?
    }

    pub async fn adjust(
        &self,
        name: &str,
        delta: i32,
    ) -> Result<BrightnessSnapshot, BrightnessError> {
        let device = read_devices(&self.root)?
            .into_iter()
            .find(|device| device.name == name)
            .ok_or_else(|| BrightnessError::DeviceUnavailable(name.to_owned()))?;
        let value = (i64::from(device.current) + i64::from(delta))
            .clamp(i64::from(device.minimum), i64::from(device.maximum)) as u32;
        self.set(name, value).await
    }
}

fn read_devices(root: &Path) -> Result<Vec<BrightnessDevice>, BrightnessError> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut devices = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| BrightnessError::InvalidDevice("non-UTF-8 backlight name".to_owned()))?;
        if name.is_empty() || name == "." || name == ".." {
            return Err(BrightnessError::InvalidDevice(name));
        }
        let path = entry.path();
        let current = read_number(&path.join("brightness"), &name)?;
        let maximum = read_number(&path.join("max_brightness"), &name)?;
        let minimum = match fs::read_to_string(path.join("min_brightness")) {
            Ok(text) => parse_number(&text, &name)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => 0,
            Err(error) => return Err(error.into()),
        };
        if minimum >= maximum || current < minimum || current > maximum {
            return Err(BrightnessError::InvalidDevice(name));
        }
        devices.push(BrightnessDevice {
            name,
            current,
            minimum,
            maximum,
        });
    }
    devices.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(devices)
}

fn read_number(path: &Path, name: &str) -> Result<u32, BrightnessError> {
    parse_number(&fs::read_to_string(path)?, name)
}

fn parse_number(text: &str, name: &str) -> Result<u32, BrightnessError> {
    text.trim()
        .parse()
        .map_err(|_| BrightnessError::InvalidDevice(name.to_owned()))
}

fn map_dbus_error(error: String) -> BrightnessError {
    if error.contains("AccessDenied") || error.contains("NotAuthorized") {
        BrightnessError::PermissionDenied(error)
    } else {
        BrightnessError::Service(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn detects_no_backlight_without_fabricating_a_device() {
        let root = tempdir().unwrap();
        let snapshot = BrightnessClient::with_root(root.path()).snapshot().unwrap();
        assert_eq!(snapshot.availability, Availability::UnsupportedHardware);
        assert!(snapshot.devices.is_empty());
    }

    #[test]
    fn discovers_bounds_and_normalizes_real_sysfs_values() {
        let root = tempdir().unwrap();
        let device = root.path().join("panel0");
        fs::create_dir(&device).unwrap();
        fs::write(device.join("brightness"), "40\n").unwrap();
        fs::write(device.join("max_brightness"), "80\n").unwrap();
        let snapshot = BrightnessClient::with_root(root.path()).snapshot().unwrap();
        assert_eq!(snapshot.availability, Availability::Available);
        assert_eq!(snapshot.devices[0].percent(), 50);
        assert_eq!(snapshot.devices[0].maximum, 80);
    }

    #[test]
    fn rejects_inconsistent_kernel_device_values() {
        let root = tempdir().unwrap();
        let device = root.path().join("panel0");
        fs::create_dir(&device).unwrap();
        fs::write(device.join("brightness"), "90\n").unwrap();
        fs::write(device.join("max_brightness"), "80\n").unwrap();
        assert!(matches!(
            BrightnessClient::with_root(root.path()).snapshot(),
            Err(BrightnessError::InvalidDevice(_))
        ));
    }

    #[tokio::test]
    async fn watch_reports_state_and_backlight_hotplug() {
        let root = tempdir().unwrap();
        let client = BrightnessClient::with_root(root.path());
        let mut events = client.watch();
        let initial = tokio::time::timeout(Duration::from_secs(1), events.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(initial.availability, Availability::UnsupportedHardware);

        let device = root.path().join("panel0");
        fs::create_dir(&device).unwrap();
        fs::write(device.join("brightness"), "40\n").unwrap();
        fs::write(device.join("max_brightness"), "80\n").unwrap();
        let changed = tokio::time::timeout(Duration::from_secs(3), events.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(changed.devices[0].name, "panel0");
        events.close().await;
    }
}
