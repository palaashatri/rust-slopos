use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use slopos_core::capability::Availability;
use thiserror::Error;
use tokio::{
    sync::mpsc,
    task::JoinHandle,
    time::{interval, timeout},
};
use zbus::{Connection, Proxy};

const UPOWER: &str = "org.freedesktop.UPower";
const UPOWER_PATH: &str = "/org/freedesktop/UPower";
const UPOWER_INTERFACE: &str = "org.freedesktop.UPower";
const DISPLAY_DEVICE: &str = "/org/freedesktop/UPower/devices/DisplayDevice";
const DEVICE_INTERFACE: &str = "org.freedesktop.UPower.Device";
const LOGIN1: &str = "org.freedesktop.login1";
const LOGIN1_PATH: &str = "/org/freedesktop/login1";
const LOGIN1_MANAGER: &str = "org.freedesktop.login1.Manager";
const QUERY_TIMEOUT: Duration = Duration::from_secs(5);
const WATCH_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatteryStatus {
    Unknown,
    Charging,
    Discharging,
    Empty,
    Full,
    PendingCharge,
    PendingDischarge,
}

impl BatteryStatus {
    fn from_raw(value: u32) -> Self {
        match value {
            1 => Self::Charging,
            2 => Self::Discharging,
            3 => Self::Empty,
            4 => Self::Full,
            5 => Self::PendingCharge,
            6 => Self::PendingDischarge,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatteryWarning {
    None,
    Low,
    Critical,
    Unknown,
}

impl BatteryWarning {
    fn from_raw(value: u32) -> Self {
        match value {
            1 => Self::None,
            3 => Self::Low,
            4 | 5 => Self::Critical,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerSource {
    Battery,
    External,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuspendCapability {
    Available,
    Unavailable,
    RequiresAuthorization,
    Unknown,
}

impl SuspendCapability {
    fn from_logind(value: &str) -> Self {
        match value {
            "yes" => Self::Available,
            "no" | "na" => Self::Unavailable,
            "challenge" => Self::RequiresAuthorization,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Battery {
    pub percentage: f64,
    pub status: BatteryStatus,
    pub warning: BatteryWarning,
    pub energy_rate_watts: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PowerSnapshot {
    pub revision: u64,
    pub availability: Availability,
    pub battery: Option<Battery>,
    pub source: PowerSource,
    pub suspend: SuspendCapability,
    pub lid_present: bool,
    pub lid_closed: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PowerEvent {
    Changed(PowerSnapshot),
    LowBattery { percentage: f64 },
    CriticalBattery { percentage: f64 },
}

#[derive(Debug, Error)]
pub enum PowerError {
    #[error("power query timed out")]
    Timeout,
    #[error("UPower or logind disconnected")]
    Disconnected,
    #[error("power service is unavailable: {0}")]
    MissingService(String),
    #[error("power service access denied: {0}")]
    PermissionDenied(String),
    #[error("suspend is unavailable")]
    SuspendUnavailable,
    #[error("invalid power service data: {0}")]
    InvalidProperty(String),
    #[error("power D-Bus operation failed: {0}")]
    Bus(String),
}

impl PowerError {
    pub fn availability(&self) -> Availability {
        match self {
            Self::Timeout | Self::InvalidProperty(_) | Self::Bus(_) => Availability::Failed,
            Self::Disconnected => Availability::Disconnected,
            Self::MissingService(_) => Availability::MissingService,
            Self::PermissionDenied(_) => Availability::PermissionDenied,
            Self::SuspendUnavailable => Availability::UnsupportedHardware,
        }
    }
}

#[derive(Clone)]
pub struct PowerClient {
    connection: Connection,
    revision: Arc<AtomicU64>,
}

pub struct PowerEventStream {
    receiver: mpsc::Receiver<Result<PowerEvent, PowerError>>,
    task: Option<JoinHandle<()>>,
}

impl PowerEventStream {
    pub async fn next(&mut self) -> Option<Result<PowerEvent, PowerError>> {
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

impl Drop for PowerEventStream {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

impl PowerClient {
    pub async fn connect() -> Result<Self, PowerError> {
        let connection = timeout(QUERY_TIMEOUT, Connection::system())
            .await
            .map_err(|_| PowerError::Timeout)?
            .map_err(map_bus_error)?;
        Ok(Self::from_connection(connection))
    }

    fn from_connection(connection: Connection) -> Self {
        Self {
            connection,
            revision: Arc::new(AtomicU64::new(0)),
        }
    }

    pub async fn snapshot(&self) -> Result<PowerSnapshot, PowerError> {
        timeout(QUERY_TIMEOUT, self.snapshot_inner())
            .await
            .map_err(|_| PowerError::Timeout)?
    }

    pub async fn request_suspend(&self) -> Result<(), PowerError> {
        timeout(QUERY_TIMEOUT, async {
            let login1 = Proxy::new(&self.connection, LOGIN1, LOGIN1_PATH, LOGIN1_MANAGER)
                .await
                .map_err(map_bus_error)?;
            let capability: String = login1
                .call("CanSuspend", &())
                .await
                .map_err(map_bus_error)?;
            if !matches!(capability.as_str(), "yes" | "challenge") {
                return Err(PowerError::SuspendUnavailable);
            }
            login1
                .call::<_, _, ()>("Suspend", &(true,))
                .await
                .map_err(map_bus_error)
        })
        .await
        .map_err(|_| PowerError::Timeout)?
    }

    pub fn watch(&self) -> PowerEventStream {
        let (sender, receiver) = mpsc::channel(8);
        let client = self.clone();
        let task = tokio::spawn(async move {
            // ponytail: two-second polling simplifies reconnection; use UPower signals if this latency becomes a problem.
            let mut timer = interval(WATCH_INTERVAL);
            let mut previous: Option<PowerSnapshot> = None;
            let mut recovering = false;
            loop {
                timer.tick().await;
                match client.snapshot().await {
                    Ok(snapshot) => {
                        let changed = recovering
                            || previous.as_ref().is_none_or(|previous| {
                                previous.availability != snapshot.availability
                                    || previous.battery != snapshot.battery
                                    || previous.source != snapshot.source
                                    || previous.suspend != snapshot.suspend
                                    || previous.lid_present != snapshot.lid_present
                                    || previous.lid_closed != snapshot.lid_closed
                            });
                        if changed {
                            let warnings = warning_events(&previous, &snapshot);
                            previous = Some(snapshot.clone());
                            recovering = false;
                            if sender
                                .send(Ok(PowerEvent::Changed(snapshot)))
                                .await
                                .is_err()
                            {
                                break;
                            }
                            for warning in warnings {
                                if sender.send(Ok(warning)).await.is_err() {
                                    return;
                                }
                            }
                        }
                    }
                    Err(error) => {
                        recovering = true;
                        if sender.send(Err(error)).await.is_err() {
                            break;
                        }
                    }
                }
            }
        });
        PowerEventStream {
            receiver,
            task: Some(task),
        }
    }

    async fn snapshot_inner(&self) -> Result<PowerSnapshot, PowerError> {
        let upower_result = async {
            match Proxy::new(&self.connection, UPOWER, UPOWER_PATH, UPOWER_INTERFACE).await {
                Ok(upower) => {
                    let device =
                        Proxy::new(&self.connection, UPOWER, DISPLAY_DEVICE, DEVICE_INTERFACE)
                            .await
                            .map_err(map_bus_error)?;
                    let present: bool = device
                        .get_property("IsPresent")
                        .await
                        .map_err(map_bus_error)?;
                    let battery = if present {
                        let percentage: f64 = device
                            .get_property("Percentage")
                            .await
                            .map_err(map_bus_error)?;
                        if !percentage.is_finite() || !(0.0..=100.0).contains(&percentage) {
                            return Err(PowerError::InvalidProperty(format!(
                                "battery percentage out of range: {percentage}"
                            )));
                        }
                        let state: u32 =
                            device.get_property("State").await.map_err(map_bus_error)?;
                        let warning: u32 = device
                            .get_property("WarningLevel")
                            .await
                            .map_err(map_bus_error)?;
                        let energy_rate_watts = match device.get_property::<f64>("EnergyRate").await
                        {
                            Ok(value) if value.is_finite() && value >= 0.0 => Some(value),
                            Ok(_) => {
                                return Err(PowerError::InvalidProperty("EnergyRate".to_owned()))
                            }
                            Err(error) if is_unknown_property(&error) => None,
                            Err(error) => return Err(map_bus_error(error)),
                        };
                        Some(Battery {
                            percentage,
                            status: BatteryStatus::from_raw(state),
                            warning: BatteryWarning::from_raw(warning),
                            energy_rate_watts,
                        })
                    } else {
                        None
                    };
                    let on_battery: bool = upower
                        .get_property("OnBattery")
                        .await
                        .map_err(map_bus_error)?;
                    let lid_present: bool = upower
                        .get_property("LidIsPresent")
                        .await
                        .map_err(map_bus_error)?;
                    let lid_closed = if lid_present {
                        Some(
                            upower
                                .get_property("LidIsClosed")
                                .await
                                .map_err(map_bus_error)?,
                        )
                    } else {
                        None
                    };
                    let availability = if battery.is_some() {
                        Availability::Available
                    } else {
                        Availability::UnsupportedHardware
                    };
                    Ok((
                        battery,
                        if on_battery {
                            PowerSource::Battery
                        } else {
                            PowerSource::External
                        },
                        lid_present,
                        lid_closed,
                        availability,
                    ))
                }
                Err(error) => Err(map_bus_error(error)),
            }
        }
        .await;
        let (battery, source, lid_present, lid_closed, availability) = match upower_result {
            Ok(result) => result,
            Err(PowerError::MissingService(_)) => (
                None,
                PowerSource::Unknown,
                false,
                None,
                Availability::MissingService,
            ),
            Err(error) => return Err(error),
        };
        let login1 = Proxy::new(&self.connection, LOGIN1, LOGIN1_PATH, LOGIN1_MANAGER)
            .await
            .map_err(map_bus_error)?;
        let can_suspend: String = login1
            .call("CanSuspend", &())
            .await
            .map_err(map_bus_error)?;
        Ok(PowerSnapshot {
            revision: self.revision.fetch_add(1, Ordering::Relaxed) + 1,
            availability,
            battery,
            source,
            suspend: SuspendCapability::from_logind(&can_suspend),
            lid_present,
            lid_closed,
        })
    }
}

fn warning_events(previous: &Option<PowerSnapshot>, current: &PowerSnapshot) -> Vec<PowerEvent> {
    let Some(battery) = &current.battery else {
        return Vec::new();
    };
    let before = previous
        .as_ref()
        .and_then(|snapshot| snapshot.battery.as_ref())
        .map(|battery| battery.warning);
    match (before, battery.warning) {
        (Some(BatteryWarning::Critical), _) => Vec::new(),
        (Some(BatteryWarning::Low), BatteryWarning::Critical) => {
            vec![PowerEvent::CriticalBattery {
                percentage: battery.percentage,
            }]
        }
        (Some(BatteryWarning::Low), _) => Vec::new(),
        (_, BatteryWarning::Critical) => vec![PowerEvent::CriticalBattery {
            percentage: battery.percentage,
        }],
        (_, BatteryWarning::Low) => vec![PowerEvent::LowBattery {
            percentage: battery.percentage,
        }],
        _ => Vec::new(),
    }
}

fn map_bus_error(error: zbus::Error) -> PowerError {
    match &error {
        zbus::Error::MethodError(name, _, _) => match name.as_str() {
            "org.freedesktop.DBus.Error.ServiceUnknown"
            | "org.freedesktop.DBus.Error.NameHasNoOwner" => {
                PowerError::MissingService(error.to_string())
            }
            "org.freedesktop.DBus.Error.AccessDenied" => {
                PowerError::PermissionDenied(error.to_string())
            }
            _ => PowerError::Bus(error.to_string()),
        },
        zbus::Error::FDO(error) => match error.as_ref() {
            zbus::fdo::Error::ServiceUnknown(_) | zbus::fdo::Error::NameHasNoOwner(_) => {
                PowerError::MissingService(error.to_string())
            }
            zbus::fdo::Error::AccessDenied(_) => PowerError::PermissionDenied(error.to_string()),
            _ => PowerError::Bus(error.to_string()),
        },
        _ => PowerError::Bus(error.to_string()),
    }
}

fn is_unknown_property(error: &zbus::Error) -> bool {
    matches!(
        error,
        zbus::Error::MethodError(name, _, _) if name.as_str() == "org.freedesktop.DBus.Error.UnknownProperty"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::BufRead,
        process::{Child, Command, Stdio},
        sync::{atomic::AtomicBool, Mutex},
    };

    struct BusDaemon(Child);

    impl Drop for BusDaemon {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[derive(Clone, Copy)]
    struct BatteryFixture {
        present: bool,
        percentage: f64,
        state: u32,
        warning: u32,
        energy_rate: f64,
    }

    struct UPowerFixture {
        on_battery: AtomicBool,
        lid_present: AtomicBool,
        lid_closed: AtomicBool,
    }

    #[zbus::interface(name = "org.freedesktop.UPower")]
    impl UPowerFixture {
        #[zbus(property)]
        fn on_battery(&self) -> bool {
            self.on_battery.load(Ordering::SeqCst)
        }

        #[zbus(property)]
        fn lid_is_present(&self) -> bool {
            self.lid_present.load(Ordering::SeqCst)
        }

        #[zbus(property)]
        fn lid_is_closed(&self) -> bool {
            self.lid_closed.load(Ordering::SeqCst)
        }
    }

    struct DeviceFixture(Arc<Mutex<BatteryFixture>>);

    #[zbus::interface(name = "org.freedesktop.UPower.Device")]
    impl DeviceFixture {
        #[zbus(property)]
        fn is_present(&self) -> bool {
            self.0.lock().unwrap().present
        }

        #[zbus(property)]
        fn percentage(&self) -> f64 {
            self.0.lock().unwrap().percentage
        }

        #[zbus(property)]
        fn state(&self) -> u32 {
            self.0.lock().unwrap().state
        }

        #[zbus(property)]
        fn warning_level(&self) -> u32 {
            self.0.lock().unwrap().warning
        }

        #[zbus(property)]
        fn energy_rate(&self) -> f64 {
            self.0.lock().unwrap().energy_rate
        }
    }

    struct LogindFixture {
        capability: &'static str,
        requested_interactive: Arc<AtomicBool>,
    }

    #[zbus::interface(name = "org.freedesktop.login1.Manager")]
    impl LogindFixture {
        fn can_suspend(&self) -> String {
            self.capability.to_owned()
        }

        fn suspend(&self, interactive: bool) {
            self.requested_interactive
                .store(interactive, Ordering::SeqCst);
        }
    }

    async fn fixture(
        present: bool,
        percentage: f64,
        warning: u32,
        suspend: &'static str,
    ) -> (
        BusDaemon,
        Connection,
        Connection,
        PowerClient,
        Arc<Mutex<BatteryFixture>>,
        Arc<AtomicBool>,
    ) {
        let mut daemon = BusDaemon(
            Command::new("dbus-daemon")
                .args(["--session", "--nofork", "--print-address=1"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .expect("dbus-daemon must be installed for isolated power tests"),
        );
        let mut address = String::new();
        std::io::BufReader::new(daemon.0.stdout.as_mut().unwrap())
            .read_line(&mut address)
            .unwrap();
        let battery = Arc::new(Mutex::new(BatteryFixture {
            present,
            percentage,
            state: 2,
            warning,
            energy_rate: 12.5,
        }));
        let upower_connection = zbus::connection::Builder::address(address.trim())
            .unwrap()
            .name(UPOWER)
            .unwrap()
            .serve_at(
                UPOWER_PATH,
                UPowerFixture {
                    on_battery: AtomicBool::new(true),
                    lid_present: AtomicBool::new(true),
                    lid_closed: AtomicBool::new(false),
                },
            )
            .unwrap()
            .serve_at(DISPLAY_DEVICE, DeviceFixture(battery.clone()))
            .unwrap()
            .build()
            .await
            .unwrap();
        let suspend_interactive = Arc::new(AtomicBool::new(false));
        let logind_connection = zbus::connection::Builder::address(address.trim())
            .unwrap()
            .name(LOGIN1)
            .unwrap()
            .serve_at(
                LOGIN1_PATH,
                LogindFixture {
                    capability: suspend,
                    requested_interactive: suspend_interactive.clone(),
                },
            )
            .unwrap()
            .build()
            .await
            .unwrap();
        let client_connection = zbus::connection::Builder::address(address.trim())
            .unwrap()
            .build()
            .await
            .unwrap();
        let client = PowerClient::from_connection(client_connection);
        (
            daemon,
            upower_connection,
            logind_connection,
            client,
            battery,
            suspend_interactive,
        )
    }

    #[test]
    fn maps_upower_states_and_logind_suspend_capability() {
        assert_eq!(BatteryStatus::from_raw(1), BatteryStatus::Charging);
        assert_eq!(BatteryStatus::from_raw(2), BatteryStatus::Discharging);
        assert_eq!(BatteryStatus::from_raw(4), BatteryStatus::Full);
        assert_eq!(BatteryStatus::from_raw(99), BatteryStatus::Unknown);
        assert_eq!(BatteryWarning::from_raw(3), BatteryWarning::Low);
        assert_eq!(BatteryWarning::from_raw(5), BatteryWarning::Critical);
        assert_eq!(
            SuspendCapability::from_logind("yes"),
            SuspendCapability::Available
        );
        assert_eq!(
            SuspendCapability::from_logind("challenge"),
            SuspendCapability::RequiresAuthorization
        );
        assert_eq!(
            SuspendCapability::from_logind("na"),
            SuspendCapability::Unavailable
        );
    }

    #[test]
    fn critical_battery_escalation_is_emitted_once() {
        let snapshot = |warning| PowerSnapshot {
            revision: 1,
            availability: Availability::Available,
            battery: Some(Battery {
                percentage: 8.0,
                status: BatteryStatus::Discharging,
                warning,
                energy_rate_watts: None,
            }),
            source: PowerSource::Battery,
            suspend: SuspendCapability::Available,
            lid_present: false,
            lid_closed: None,
        };
        let low = snapshot(BatteryWarning::Low);
        let critical = snapshot(BatteryWarning::Critical);
        assert_eq!(
            warning_events(&Some(low), &critical),
            vec![PowerEvent::CriticalBattery { percentage: 8.0 }]
        );
        assert!(warning_events(&Some(critical.clone()), &critical).is_empty());
    }

    #[tokio::test]
    async fn fixture_reads_battery_lid_power_source_and_suspend() {
        let (_daemon, _upower, _logind, client, _, _) = fixture(true, 63.5, 1, "yes").await;
        let snapshot = client.snapshot().await.unwrap();
        assert_eq!(snapshot.availability, Availability::Available);
        assert_eq!(snapshot.battery.as_ref().unwrap().percentage, 63.5);
        assert_eq!(
            snapshot.battery.as_ref().unwrap().status,
            BatteryStatus::Discharging
        );
        assert_eq!(
            snapshot.battery.as_ref().unwrap().energy_rate_watts,
            Some(12.5)
        );
        assert_eq!(snapshot.source, PowerSource::Battery);
        assert_eq!(snapshot.suspend, SuspendCapability::Available);
        assert_eq!(snapshot.lid_closed, Some(false));
    }

    #[tokio::test]
    async fn suspend_request_allows_authorized_interaction() {
        let (_daemon, _upower, _logind, client, _, requested_interactive) =
            fixture(false, 0.0, 0, "challenge").await;
        client.request_suspend().await.unwrap();
        assert!(requested_interactive.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn unavailable_suspend_is_not_requested() {
        let (_daemon, _upower, _logind, client, _, requested_interactive) =
            fixture(false, 0.0, 0, "no").await;
        assert!(matches!(
            client.request_suspend().await,
            Err(PowerError::SuspendUnavailable)
        ));
        assert!(!requested_interactive.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn no_battery_is_reported_without_fabricated_charge() {
        let (_daemon, _upower, _logind, client, _, _) = fixture(false, 0.0, 0, "no").await;
        let snapshot = client.snapshot().await.unwrap();
        assert_eq!(snapshot.availability, Availability::UnsupportedHardware);
        assert!(snapshot.battery.is_none());
        assert_eq!(snapshot.suspend, SuspendCapability::Unavailable);
    }

    #[tokio::test]
    async fn missing_upower_preserves_logind_suspend_capability() {
        let (_daemon, upower, _logind, client, _, _) = fixture(false, 0.0, 0, "yes").await;
        drop(upower);
        let snapshot = client.snapshot().await.unwrap();
        assert_eq!(snapshot.availability, Availability::MissingService);
        assert!(snapshot.battery.is_none());
        assert_eq!(snapshot.source, PowerSource::Unknown);
        assert_eq!(snapshot.suspend, SuspendCapability::Available);
    }

    #[tokio::test]
    #[ignore = "requires the guest system bus with UPower absent and logind available"]
    async fn live_logind_capability_remains_available_without_upower() {
        let client = PowerClient::connect().await.unwrap();
        let snapshot = client.snapshot().await.unwrap();
        assert_eq!(snapshot.availability, Availability::MissingService);
        assert!(snapshot.battery.is_none());
        assert_eq!(snapshot.source, PowerSource::Unknown);
        assert_ne!(snapshot.suspend, SuspendCapability::Unknown);
    }

    #[tokio::test]
    async fn snapshot_rejects_invalid_upower_percentage() {
        let (_daemon, _upower, _logind, client, _, _) = fixture(true, 110.0, 1, "yes").await;
        assert!(matches!(
            client.snapshot().await,
            Err(PowerError::InvalidProperty(_))
        ));
    }

    #[tokio::test]
    async fn watch_emits_low_battery_once_on_warning_transition() {
        let (_daemon, _upower, _logind, client, battery, _) = fixture(true, 20.0, 1, "yes").await;
        let mut events = client.watch();
        assert!(matches!(
            tokio::time::timeout(Duration::from_secs(2), events.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap(),
            PowerEvent::Changed(_)
        ));
        battery.lock().unwrap().warning = 3;
        assert!(matches!(
            tokio::time::timeout(Duration::from_secs(3), events.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap(),
            PowerEvent::Changed(_)
        ));
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), events.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap(),
            PowerEvent::LowBattery { percentage: 20.0 }
        );
        events.close().await;
    }
}
