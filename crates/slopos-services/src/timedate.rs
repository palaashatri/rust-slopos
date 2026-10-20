use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use slopos_core::capability::Availability;
use thiserror::Error;
#[cfg(test)]
use tokio::sync::RwLock;
use tokio::{sync::mpsc, time::timeout};
use zbus::{Connection, Proxy};

const SERVICE: &str = "org.freedesktop.timedate1";
const PATH: &str = "/org/freedesktop/timedate1";
const INTERFACE: &str = "org.freedesktop.timedate1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimedateSnapshot {
    pub revision: u64,
    pub timezone: String,
    pub ntp_available: bool,
    pub ntp_enabled: bool,
    pub ntp_synchronized: bool,
    pub time_unix_usec: u64,
}

#[derive(Debug, Error)]
pub enum TimedateError {
    #[error("timedate operation timed out")]
    Timeout,
    #[error("system timedate service is unavailable: {0}")]
    MissingService(#[source] zbus::Error),
    #[error("timedate permission denied: {0}")]
    PermissionDenied(#[source] zbus::Error),
    #[error("network time synchronization is unavailable")]
    NtpUnavailable,
    #[error("unknown system timezone: {0}")]
    UnknownTimezone(String),
    #[error("requested time is outside the D-Bus signed microsecond range")]
    InvalidTime,
    #[error("invalid timedate property: {0}")]
    InvalidProperty(String),
    #[error("timedate D-Bus error: {0}")]
    Bus(#[source] zbus::Error),
}

impl TimedateError {
    pub fn availability(&self) -> Availability {
        match self {
            Self::Timeout => Availability::Failed,
            Self::MissingService(_) => Availability::MissingService,
            Self::PermissionDenied(_) => Availability::PermissionDenied,
            Self::NtpUnavailable
            | Self::UnknownTimezone(_)
            | Self::InvalidTime
            | Self::InvalidProperty(_)
            | Self::Bus(_) => Availability::Failed,
        }
    }
}

impl From<zbus::Error> for TimedateError {
    fn from(error: zbus::Error) -> Self {
        match availability_from_error(&error) {
            Availability::MissingService => Self::MissingService(error),
            Availability::PermissionDenied => Self::PermissionDenied(error),
            _ => Self::Bus(error),
        }
    }
}

fn availability_from_error(error: &zbus::Error) -> Availability {
    match error {
        zbus::Error::MethodError(name, _, _) => match name.as_str() {
            "org.freedesktop.DBus.Error.ServiceUnknown"
            | "org.freedesktop.DBus.Error.NameHasNoOwner" => Availability::MissingService,
            "org.freedesktop.DBus.Error.AccessDenied" => Availability::PermissionDenied,
            _ => Availability::Failed,
        },
        zbus::Error::FDO(error) => match error.as_ref() {
            zbus::fdo::Error::ServiceUnknown(_) | zbus::fdo::Error::NameHasNoOwner(_) => {
                Availability::MissingService
            }
            zbus::fdo::Error::AccessDenied(_) => Availability::PermissionDenied,
            zbus::fdo::Error::ZBus(error) => availability_from_error(error),
            _ => Availability::Failed,
        },
        _ => Availability::Failed,
    }
}

#[derive(Debug, Clone)]
pub struct TimedateClient {
    connection: Connection,
    revision: Arc<AtomicU64>,
}

pub struct TimedateEventStream {
    receiver: mpsc::Receiver<Result<TimedateSnapshot, TimedateError>>,
}

impl TimedateEventStream {
    pub async fn next(&mut self) -> Option<Result<TimedateSnapshot, TimedateError>> {
        self.receiver.recv().await
    }
}

impl TimedateClient {
    pub async fn connect() -> Result<Self, TimedateError> {
        let connection = timeout(Duration::from_secs(3), Connection::system())
            .await
            .map_err(|_| TimedateError::Timeout)??;
        Ok(Self {
            connection,
            revision: Arc::new(AtomicU64::new(0)),
        })
    }

    pub async fn snapshot(&self) -> Result<TimedateSnapshot, TimedateError> {
        let mut snapshot = timeout(Duration::from_secs(5), self.snapshot_inner())
            .await
            .map_err(|_| TimedateError::Timeout)??;
        snapshot.revision = self.revision.fetch_add(1, Ordering::Relaxed) + 1;
        Ok(snapshot)
    }

    pub fn watch(&self) -> TimedateEventStream {
        let client = self.clone();
        let (sender, receiver) = mpsc::channel(4);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                if sender.send(client.snapshot().await).await.is_err() {
                    break;
                }
            }
        });
        TimedateEventStream { receiver }
    }

    pub async fn list_timezones(&self) -> Result<Vec<String>, TimedateError> {
        timeout(Duration::from_secs(5), async {
            let proxy = self.proxy().await?;
            Ok(proxy.call("ListTimezones", &()).await?)
        })
        .await
        .map_err(|_| TimedateError::Timeout)?
    }

    pub async fn set_timezone(&self, timezone: &str) -> Result<TimedateSnapshot, TimedateError> {
        timeout(Duration::from_secs(5), async {
            let proxy = self.proxy().await?;
            let timezones: Vec<String> = proxy.call("ListTimezones", &()).await?;
            if !timezones.iter().any(|zone| zone == timezone) {
                return Err(TimedateError::UnknownTimezone(timezone.to_owned()));
            }
            proxy
                .call::<_, _, ()>("SetTimezone", &(timezone, true))
                .await?;
            loop {
                let snapshot = self.snapshot_inner().await?;
                if snapshot.timezone == timezone {
                    return Ok(snapshot);
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .map_err(|_| TimedateError::Timeout)?
    }

    pub async fn set_ntp(&self, enabled: bool) -> Result<TimedateSnapshot, TimedateError> {
        timeout(Duration::from_secs(5), async {
            let proxy = self.proxy().await?;
            let available: bool = proxy.get_property("CanNTP").await?;
            if !available {
                return Err(TimedateError::NtpUnavailable);
            }
            proxy.call::<_, _, ()>("SetNTP", &(enabled, true)).await?;
            loop {
                let snapshot = self.snapshot_inner().await?;
                if snapshot.ntp_enabled == enabled {
                    return Ok(snapshot);
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .map_err(|_| TimedateError::Timeout)?
    }

    pub async fn set_time_usec(&self, unix_usec: u64) -> Result<(), TimedateError> {
        let unix_usec = i64::try_from(unix_usec).map_err(|_| TimedateError::InvalidTime)?;
        timeout(Duration::from_secs(5), async {
            let proxy = self.proxy().await?;
            proxy
                .call::<_, _, ()>("SetTime", &(unix_usec, false, true))
                .await?;
            Ok(())
        })
        .await
        .map_err(|_| TimedateError::Timeout)?
    }

    async fn snapshot_inner(&self) -> Result<TimedateSnapshot, TimedateError> {
        let proxy = self.proxy().await?;
        Ok(TimedateSnapshot {
            revision: 0,
            timezone: proxy.get_property("Timezone").await?,
            ntp_available: proxy.get_property("CanNTP").await?,
            ntp_enabled: proxy.get_property("NTP").await?,
            ntp_synchronized: proxy.get_property("NTPSynchronized").await?,
            time_unix_usec: proxy.get_property("TimeUSec").await?,
        })
    }

    async fn proxy(&self) -> Result<Proxy<'_>, TimedateError> {
        Ok(Proxy::new(&self.connection, SERVICE, PATH, INTERFACE).await?)
    }
}

#[cfg(test)]
struct TimedateFixture {
    timezone: Arc<RwLock<String>>,
    ntp_available: Arc<std::sync::atomic::AtomicBool>,
    ntp_enabled: Arc<std::sync::atomic::AtomicBool>,
    ntp_synchronized: Arc<std::sync::atomic::AtomicBool>,
    time_usec: Arc<AtomicU64>,
    denied: Arc<std::sync::atomic::AtomicBool>,
}

#[zbus::interface(name = "org.freedesktop.timedate1")]
#[cfg(test)]
impl TimedateFixture {
    #[zbus(property)]
    async fn timezone(&self) -> String {
        self.timezone.read().await.clone()
    }

    #[zbus(property)]
    fn can_n_t_p(&self) -> bool {
        self.ntp_available.load(Ordering::Relaxed)
    }

    #[zbus(property)]
    fn n_t_p(&self) -> bool {
        self.ntp_enabled.load(Ordering::Relaxed)
    }

    #[zbus(property)]
    fn n_t_p_synchronized(&self) -> bool {
        self.ntp_synchronized.load(Ordering::Relaxed)
    }

    #[zbus(property)]
    fn time_u_sec(&self) -> u64 {
        self.time_usec.load(Ordering::Relaxed)
    }

    fn list_timezones(&self) -> Vec<String> {
        vec!["Etc/UTC".into(), "Europe/Berlin".into()]
    }

    async fn set_timezone(&self, timezone: String, interactive: bool) -> zbus::fdo::Result<()> {
        if !interactive {
            return Err(zbus::fdo::Error::Failed(
                "interactive auth not requested".into(),
            ));
        }
        self.ensure_allowed()?;
        *self.timezone.write().await = timezone;
        Ok(())
    }

    fn set_n_t_p(&self, enabled: bool, interactive: bool) -> zbus::fdo::Result<()> {
        if !interactive {
            return Err(zbus::fdo::Error::Failed(
                "interactive auth not requested".into(),
            ));
        }
        self.ensure_allowed()?;
        self.ntp_enabled.store(enabled, Ordering::Relaxed);
        Ok(())
    }

    fn set_time(&self, unix_usec: i64, relative: bool, interactive: bool) -> zbus::fdo::Result<()> {
        if relative || !interactive || unix_usec < 0 {
            return Err(zbus::fdo::Error::Failed(
                "invalid fixture time request".into(),
            ));
        }
        self.ensure_allowed()?;
        self.time_usec.store(unix_usec as u64, Ordering::Relaxed);
        Ok(())
    }
}

#[cfg(test)]
impl TimedateFixture {
    fn ensure_allowed(&self) -> zbus::fdo::Result<()> {
        if self.denied.load(Ordering::Relaxed) {
            Err(zbus::fdo::Error::AccessDenied("fixture denied".into()))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::BufRead,
        process::{Child, Command, Stdio},
        sync::{
            atomic::{AtomicBool, AtomicU64, Ordering},
            Arc,
        },
    };

    use tokio::sync::RwLock;

    struct BusDaemon(Child);

    impl Drop for BusDaemon {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[tokio::test]
    async fn timedate_provider_reads_and_authorizes_clock_configuration() {
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
        let timezone = Arc::new(RwLock::new("Etc/UTC".to_owned()));
        let ntp_available = Arc::new(AtomicBool::new(true));
        let ntp_enabled = Arc::new(AtomicBool::new(false));
        let ntp_synchronized = Arc::new(AtomicBool::new(false));
        let time_usec = Arc::new(AtomicU64::new(1_700_000_000_000_000));
        let denied = Arc::new(AtomicBool::new(false));
        let service = zbus::connection::Builder::address(address.trim())
            .unwrap()
            .name(SERVICE)
            .unwrap()
            .serve_at(
                PATH,
                TimedateFixture {
                    timezone: timezone.clone(),
                    ntp_available: ntp_available.clone(),
                    ntp_enabled: ntp_enabled.clone(),
                    ntp_synchronized: ntp_synchronized.clone(),
                    time_usec: time_usec.clone(),
                    denied: denied.clone(),
                },
            )
            .unwrap()
            .build()
            .await
            .unwrap();
        let client = TimedateClient {
            connection: zbus::connection::Builder::address(address.trim())
                .unwrap()
                .build()
                .await
                .unwrap(),
            revision: Arc::new(AtomicU64::new(0)),
        };

        let initial = client.snapshot().await.unwrap();
        assert_eq!(initial.timezone, "Etc/UTC");
        assert!(initial.ntp_available);
        assert!(!initial.ntp_enabled);
        assert!(!initial.ntp_synchronized);
        assert_eq!(initial.time_unix_usec, 1_700_000_000_000_000);
        assert!(client
            .list_timezones()
            .await
            .unwrap()
            .contains(&"Europe/Berlin".into()));

        let mut updates = client.watch();
        let first_update = tokio::time::timeout(Duration::from_secs(2), updates.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(first_update.revision > initial.revision);
        let changed = client.set_timezone("Europe/Berlin").await.unwrap();
        assert_eq!(changed.timezone, "Europe/Berlin");
        let timezone_update = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let update = updates.next().await.unwrap().unwrap();
                if update.timezone == "Europe/Berlin" {
                    break update;
                }
            }
        })
        .await
        .unwrap();
        assert!(timezone_update.revision > first_update.revision);
        assert!(matches!(
            client.set_timezone("Not/A_Real_Zone").await,
            Err(TimedateError::UnknownTimezone(_))
        ));
        assert_eq!(client.snapshot().await.unwrap().timezone, "Europe/Berlin");

        let enabled = client.set_ntp(true).await.unwrap();
        assert!(enabled.ntp_enabled);
        ntp_available.store(false, Ordering::Relaxed);
        assert!(matches!(
            client.set_ntp(false).await,
            Err(TimedateError::NtpUnavailable)
        ));
        assert!(ntp_enabled.load(Ordering::Relaxed));
        ntp_available.store(true, Ordering::Relaxed);

        let requested_time = 1_800_000_000_000_000;
        client.set_time_usec(requested_time).await.unwrap();
        assert_eq!(time_usec.load(Ordering::Relaxed), requested_time);
        assert!(matches!(
            client.set_time_usec(i64::MAX as u64 + 1).await,
            Err(TimedateError::InvalidTime)
        ));

        denied.store(true, Ordering::Relaxed);
        assert!(matches!(
            client.set_timezone("Etc/UTC").await,
            Err(TimedateError::PermissionDenied(_))
        ));
        denied.store(false, Ordering::Relaxed);
        drop(service);
        let missing = client.snapshot().await.unwrap_err();
        assert_eq!(missing.availability(), Availability::MissingService);
        let replacement = zbus::connection::Builder::address(address.trim())
            .unwrap()
            .name(SERVICE)
            .unwrap()
            .serve_at(
                PATH,
                TimedateFixture {
                    timezone,
                    ntp_available,
                    ntp_enabled,
                    ntp_synchronized,
                    time_usec,
                    denied,
                },
            )
            .unwrap()
            .build()
            .await
            .unwrap();
        let restored = client.snapshot().await.unwrap();
        assert_eq!(restored.timezone, "Europe/Berlin");
        drop(client);
        drop(replacement);
    }

    #[tokio::test]
    #[ignore = "requires the guest system bus and systemd-timedated"]
    async fn timedate_provider_reads_the_system_service() {
        let client = TimedateClient::connect().await.unwrap();
        let snapshot = client.snapshot().await.unwrap();
        assert!(!snapshot.timezone.is_empty());
        assert!(snapshot.time_unix_usec > 0);
    }
}
