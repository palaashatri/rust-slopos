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
    /// Bus that carries logind for writes. `None` means the system bus; tests
    /// point it at a private bus.
    bus_address: Option<String>,
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
            bus_address: None,
            revision: Arc::new(AtomicU64::new(0)),
            mutation_lock: Arc::new(Mutex::new(())),
        }
    }

    async fn logind_connection(&self) -> zbus::Result<Connection> {
        match &self.bus_address {
            Some(address) => {
                zbus::connection::Builder::address(address.as_str())?
                    .build()
                    .await
            }
            None => Connection::system().await,
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
            let connection = self
                .logind_connection()
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
    use std::{
        fs,
        io::BufRead,
        process::{Child, Command, Stdio},
        sync::atomic::AtomicBool,
    };
    use tempfile::tempdir;

    const SESSION_PATH: &str = "/org/freedesktop/login1/session/_31";

    struct BusDaemon(Child);

    impl Drop for BusDaemon {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn start_bus() -> (BusDaemon, String) {
        let mut daemon = BusDaemon(
            Command::new("dbus-daemon")
                .args(["--session", "--nofork", "--print-address=1"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let mut address = String::new();
        std::io::BufReader::new(daemon.0.stdout.as_mut().unwrap())
            .read_line(&mut address)
            .unwrap();
        (daemon, address.trim().to_owned())
    }

    struct ManagerFixture;

    #[zbus::interface(name = "org.freedesktop.login1.Manager")]
    impl ManagerFixture {
        #[zbus(name = "GetSessionByPID")]
        fn get_session_by_pid(&self, _pid: u32) -> zbus::zvariant::OwnedObjectPath {
            zbus::zvariant::OwnedObjectPath::try_from(SESSION_PATH).unwrap()
        }
    }

    /// Stands in for the logind session that accepts brightness changes. A
    /// refused change is answered with AccessDenied; an ignored change is
    /// accepted but never reaches the backlight.
    struct SessionFixture {
        root: PathBuf,
        refuse: Arc<AtomicBool>,
        ignore_writes: Arc<AtomicBool>,
    }

    #[zbus::interface(name = "org.freedesktop.login1.Session")]
    impl SessionFixture {
        fn set_brightness(&self, subsystem: &str, name: &str, value: u32) -> zbus::fdo::Result<()> {
            if subsystem != "backlight" {
                return Err(zbus::fdo::Error::InvalidArgs(subsystem.to_owned()));
            }
            if self.refuse.load(Ordering::Relaxed) {
                return Err(zbus::fdo::Error::AccessDenied(
                    "fixture refuses brightness changes".into(),
                ));
            }
            if self.ignore_writes.load(Ordering::Relaxed) {
                return Ok(());
            }
            fs::write(
                self.root.join(name).join("brightness"),
                format!("{value}\n"),
            )
            .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))
        }
    }

    /// Serves logind's manager and one session on the private bus.
    async fn serve_logind(
        address: &str,
        root: &Path,
        refuse: Arc<AtomicBool>,
        ignore_writes: Arc<AtomicBool>,
    ) -> zbus::Connection {
        zbus::connection::Builder::address(address)
            .unwrap()
            .name("org.freedesktop.login1")
            .unwrap()
            .serve_at("/org/freedesktop/login1", ManagerFixture)
            .unwrap()
            .serve_at(
                SESSION_PATH,
                SessionFixture {
                    root: root.to_path_buf(),
                    refuse,
                    ignore_writes,
                },
            )
            .unwrap()
            .build()
            .await
            .unwrap()
    }

    fn client_with_logind(root: &Path, address: &str) -> BrightnessClient {
        let mut client = BrightnessClient::with_root(root);
        client.bus_address = Some(address.to_owned());
        client
    }

    fn panel(root: &Path, current: &str) -> PathBuf {
        let device = root.join("panel0");
        fs::create_dir_all(&device).unwrap();
        fs::write(device.join("brightness"), format!("{current}\n")).unwrap();
        fs::write(device.join("max_brightness"), "80\n").unwrap();
        device
    }

    #[tokio::test]
    async fn set_writes_through_logind_and_reports_the_value_read_back() {
        let root = tempdir().unwrap();
        let device = panel(root.path(), "40");
        let (_daemon, address) = start_bus();
        let _logind = serve_logind(
            &address,
            root.path(),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
        .await;
        let client = client_with_logind(root.path(), &address);
        let snapshot = client.set("panel0", 60).await.unwrap();
        assert_eq!(snapshot.devices[0].current, 60);
        assert_eq!(
            fs::read_to_string(device.join("brightness"))
                .unwrap()
                .trim(),
            "60"
        );
    }

    #[tokio::test]
    async fn logind_refusal_is_permission_denied_and_leaves_the_backlight_unchanged() {
        let root = tempdir().unwrap();
        let device = panel(root.path(), "40");
        let (_daemon, address) = start_bus();
        let _logind = serve_logind(
            &address,
            root.path(),
            Arc::new(AtomicBool::new(true)),
            Arc::new(AtomicBool::new(false)),
        )
        .await;
        let client = client_with_logind(root.path(), &address);
        let error = client.set("panel0", 60).await.unwrap_err();
        assert!(
            matches!(error, BrightnessError::PermissionDenied(_)),
            "{error:?}"
        );
        assert_eq!(error.availability(), Availability::PermissionDenied);
        assert_eq!(
            fs::read_to_string(device.join("brightness"))
                .unwrap()
                .trim(),
            "40"
        );
        // Reads keep working while writes are refused.
        assert_eq!(client.snapshot().unwrap().devices[0].current, 40);
    }

    #[tokio::test]
    async fn accepted_write_that_the_hardware_ignores_is_reported_as_failed() {
        let root = tempdir().unwrap();
        let device = panel(root.path(), "40");
        let (_daemon, address) = start_bus();
        let _logind = serve_logind(
            &address,
            root.path(),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(true)),
        )
        .await;
        let client = client_with_logind(root.path(), &address);
        let error = client.set("panel0", 60).await.unwrap_err();
        assert!(
            matches!(error, BrightnessError::InvalidDevice(_)),
            "{error:?}"
        );
        assert_eq!(error.availability(), Availability::Failed);
        assert_eq!(
            fs::read_to_string(device.join("brightness"))
                .unwrap()
                .trim(),
            "40"
        );
    }

    #[tokio::test]
    async fn absent_logind_is_missing_service_for_backlight_writes() {
        let root = tempdir().unwrap();
        let device = panel(root.path(), "40");
        let (_daemon, address) = start_bus();
        let client = client_with_logind(root.path(), &address);
        let error = client.set("panel0", 60).await.unwrap_err();
        assert_eq!(error.availability(), Availability::MissingService);
        assert_eq!(
            fs::read_to_string(device.join("brightness"))
                .unwrap()
                .trim(),
            "40"
        );
    }

    #[tokio::test]
    async fn set_on_a_missing_backlight_reports_unsupported_hardware() {
        let root = tempdir().unwrap();
        let client = BrightnessClient::with_root(root.path());
        let error = client.set("panel0", 60).await.unwrap_err();
        assert!(
            matches!(&error, BrightnessError::DeviceUnavailable(name) if name == "panel0"),
            "{error:?}"
        );
        assert_eq!(error.availability(), Availability::UnsupportedHardware);
    }

    #[tokio::test]
    async fn out_of_range_values_are_rejected_before_logind_is_called() {
        let root = tempdir().unwrap();
        let device = panel(root.path(), "40");
        // No bus is configured: reaching logind would fail with MissingService.
        let client = BrightnessClient::with_root(root.path());
        let error = client.set("panel0", 90).await.unwrap_err();
        assert!(matches!(error, BrightnessError::OutOfRange), "{error:?}");
        assert_eq!(error.availability(), Availability::Failed);
        assert_eq!(
            fs::read_to_string(device.join("brightness"))
                .unwrap()
                .trim(),
            "40"
        );
    }

    #[tokio::test]
    async fn watch_reports_an_invalid_backlight_then_the_recovered_value() {
        let root = tempdir().unwrap();
        let device = panel(root.path(), "40");
        let mut events = BrightnessClient::with_root(root.path()).watch();
        let first = timeout(Duration::from_secs(3), events.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(first.devices[0].current, 40);

        // 90 is above the device maximum of 80.
        fs::write(device.join("brightness"), "90\n").unwrap();
        let error = timeout(Duration::from_secs(6), async {
            loop {
                match events.next().await {
                    Some(Err(error)) => break error,
                    Some(Ok(_)) => continue,
                    None => panic!("watch stream ended without reporting the fault"),
                }
            }
        })
        .await
        .unwrap();
        assert!(
            matches!(error, BrightnessError::InvalidDevice(_)),
            "{error:?}"
        );
        assert_eq!(error.availability(), Availability::Failed);

        fs::write(device.join("brightness"), "50\n").unwrap();
        let recovered = timeout(Duration::from_secs(6), async {
            loop {
                match events.next().await {
                    Some(Ok(snapshot)) => break snapshot,
                    Some(Err(error)) => panic!("watch still failing after recovery: {error:?}"),
                    None => panic!("watch stream ended before recovery"),
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(recovered.devices[0].current, 50);
        events.close().await;
    }

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
