use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use slopos_core::{capability::Availability, ipc::IpcObjectPath};
use thiserror::Error;
use tokio::{sync::mpsc, time::timeout};
use zbus::{
    zvariant::{OwnedObjectPath, OwnedValue},
    Connection, Proxy,
};

const SERVICE: &str = "org.bluez";
const ROOT: &str = "/";
const OBJECT_MANAGER: &str = "org.freedesktop.DBus.ObjectManager";
const ADAPTER: &str = "org.bluez.Adapter1";
const DEVICE: &str = "org.bluez.Device1";
const OPERATION_TIMEOUT: Duration = Duration::from_secs(5);
const PAIRING_CANCEL_RETRY_TIMEOUT: Duration = Duration::from_secs(4);
const PAIRING_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BluetoothAdapter {
    pub path: String,
    pub name: String,
    pub address: String,
    pub powered: bool,
    pub discovering: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BluetoothDevice {
    pub path: String,
    pub name: String,
    pub address: String,
    pub adapter_path: String,
    pub paired: bool,
    pub trusted: bool,
    pub connected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BluetoothSnapshot {
    pub revision: u64,
    pub availability: Availability,
    pub adapters: Vec<BluetoothAdapter>,
    pub devices: Vec<BluetoothDevice>,
}

#[derive(Debug, Error)]
pub enum BluetoothError {
    #[error("Bluetooth operation timed out")]
    Timeout,
    #[error("BlueZ service is unavailable: {0}")]
    MissingService(#[source] zbus::Error),
    #[error("Bluetooth operation denied: {0}")]
    PermissionDenied(#[source] zbus::Error),
    #[error("Bluetooth adapter is unavailable: {0}")]
    AdapterUnavailable(String),
    #[error("Bluetooth device is unavailable: {0}")]
    DeviceUnavailable(String),
    #[error("Bluetooth device is not currently pairing: {0}")]
    PairingNotActive(String),
    #[error("Bluetooth pairing is already in progress for: {0}")]
    PairingInProgress(String),
    #[error("Bluetooth pairing was cancelled: {0}")]
    PairingCancelled(String),
    #[error("pairing timed out and BlueZ cancellation failed: {0}")]
    PairingCancellationFailed(#[source] Box<BluetoothError>),
    #[error("invalid BlueZ object path: {0}")]
    InvalidObjectPath(String),
    #[error("invalid BlueZ property: {0}")]
    InvalidProperty(String),
    #[error("BlueZ D-Bus error: {0}")]
    Bus(#[source] zbus::Error),
}

impl BluetoothError {
    pub fn availability(&self) -> Availability {
        match self {
            Self::Timeout => Availability::Failed,
            Self::MissingService(_) => Availability::MissingService,
            Self::PermissionDenied(_) => Availability::PermissionDenied,
            Self::AdapterUnavailable(_) | Self::DeviceUnavailable(_) => {
                Availability::UnsupportedHardware
            }
            Self::PairingNotActive(_) | Self::PairingInProgress(_) | Self::PairingCancelled(_) => {
                Availability::Disconnected
            }
            Self::PairingCancellationFailed(_)
            | Self::InvalidObjectPath(_)
            | Self::InvalidProperty(_)
            | Self::Bus(_) => Availability::Failed,
        }
    }
}

impl From<zbus::Error> for BluetoothError {
    fn from(error: zbus::Error) -> Self {
        match &error {
            zbus::Error::MethodError(name, message, _)
                if name.as_str() == "org.freedesktop.DBus.Error.ServiceUnknown"
                    || name.as_str() == "org.freedesktop.DBus.Error.NameHasNoOwner"
                    || message.as_deref().is_some_and(|message| {
                        message.contains("org.freedesktop.DBus.Error.ServiceUnknown")
                            || message.contains("org.freedesktop.DBus.Error.NameHasNoOwner")
                    }) =>
            {
                Self::MissingService(error)
            }
            zbus::Error::MethodError(name, message, _)
                if name.as_str() == "org.freedesktop.DBus.Error.AccessDenied"
                    || name.as_str() == "org.bluez.Error.NotAuthorized"
                    || message.as_deref().is_some_and(|message| {
                        message.contains("org.freedesktop.DBus.Error.AccessDenied")
                            || message.contains("org.bluez.Error.NotAuthorized")
                    }) =>
            {
                Self::PermissionDenied(error)
            }
            zbus::Error::MethodError(name, message, _)
                if name.as_str() == "org.bluez.Error.AuthenticationCanceled" =>
            {
                Self::PairingCancelled(message.as_deref().unwrap_or_default().to_owned())
            }
            _ => Self::Bus(error),
        }
    }
}

#[derive(Clone)]
pub struct BluetoothClient {
    connection: Connection,
    revision: Arc<AtomicU64>,
    pairings: Arc<std::sync::Mutex<HashMap<String, Arc<PairingControl>>>>,
}

struct PairingControl {
    in_flight: AtomicBool,
    cancellation_confirmed: AtomicBool,
    pairing_completed: AtomicBool,
    cancellation: tokio::sync::Mutex<()>,
}

impl PairingControl {
    fn new() -> Self {
        Self {
            in_flight: AtomicBool::new(false),
            cancellation_confirmed: AtomicBool::new(false),
            pairing_completed: AtomicBool::new(false),
            cancellation: tokio::sync::Mutex::new(()),
        }
    }
}

struct PairingCancelGuard {
    client: BluetoothClient,
    device_path: String,
    runtime: tokio::runtime::Handle,
    pairings: Arc<std::sync::Mutex<HashMap<String, Arc<PairingControl>>>>,
    control: Arc<PairingControl>,
    registered: bool,
    armed: bool,
}

impl PairingCancelGuard {
    fn new(client: BluetoothClient, device_path: &str) -> Self {
        Self {
            pairings: client.pairings.clone(),
            control: Arc::new(PairingControl::new()),
            client,
            device_path: device_path.to_owned(),
            runtime: tokio::runtime::Handle::current(),
            registered: false,
            armed: false,
        }
    }

    fn register(&mut self) -> bool {
        let mut pairings = self.pairings.lock().unwrap();
        if pairings.contains_key(&self.device_path) {
            return false;
        }
        self.control.in_flight.store(true, Ordering::Relaxed);
        pairings.insert(self.device_path.clone(), self.control.clone());
        self.registered = true;
        self.registered
    }

    fn arm(&mut self) {
        self.armed = true;
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for PairingCancelGuard {
    fn drop(&mut self) {
        self.control.in_flight.store(false, Ordering::Relaxed);
        if self.armed {
            let client = self.client.clone();
            let device_path = self.device_path.clone();
            let control = self.control.clone();
            self.runtime.spawn(async move {
                let result = client
                    .cancel_pairing_with_retry(&device_path, &control, true, true)
                    .await;
                match &result {
                    Ok(())
                    | Err(BluetoothError::DeviceUnavailable(_))
                    | Err(BluetoothError::MissingService(_)) => {
                        client.remove_pairing_registration(&device_path, &control);
                    }
                    Err(_) => {}
                }
                if let Err(error) = result {
                    eprintln!("BlueZ pairing cancellation after caller drop failed: {error}");
                }
            });
        } else if self.registered {
            self.client
                .remove_pairing_registration(&self.device_path, &self.control);
        }
    }
}

pub struct BluetoothEventStream {
    receiver: mpsc::Receiver<Result<BluetoothSnapshot, BluetoothError>>,
}

impl BluetoothEventStream {
    pub async fn next(&mut self) -> Option<Result<BluetoothSnapshot, BluetoothError>> {
        self.receiver.recv().await
    }
}

impl BluetoothClient {
    pub async fn connect() -> Result<Self, BluetoothError> {
        let connection = timeout(OPERATION_TIMEOUT, Connection::system())
            .await
            .map_err(|_| BluetoothError::Timeout)??;
        Ok(Self {
            connection,
            revision: Arc::new(AtomicU64::new(0)),
            pairings: Arc::new(std::sync::Mutex::new(HashMap::new())),
        })
    }

    pub async fn snapshot(&self) -> Result<BluetoothSnapshot, BluetoothError> {
        timeout(OPERATION_TIMEOUT, self.snapshot_inner())
            .await
            .map_err(|_| BluetoothError::Timeout)?
    }

    pub fn watch(&self) -> BluetoothEventStream {
        let client = self.clone();
        let (sender, receiver) = mpsc::channel(4);
        tokio::spawn(async move {
            let mut previous: Option<BluetoothSnapshot> = None;
            let mut unavailable = false;
            let mut tick = tokio::time::interval(Duration::from_secs(2));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tick.tick().await;
                let result = client.snapshot().await;
                let changed = match (&previous, &result) {
                    (_, Err(_)) => !unavailable,
                    (None, Ok(_)) => true,
                    (Some(old), Ok(new)) => {
                        unavailable
                            || old.adapters != new.adapters
                            || old.devices != new.devices
                            || old.availability != new.availability
                    }
                };
                if changed {
                    unavailable = result.is_err();
                    if let Ok(snapshot) = &result {
                        previous = Some(snapshot.clone());
                    }
                    if sender.send(result).await.is_err() {
                        break;
                    }
                }
            }
        });
        BluetoothEventStream { receiver }
    }

    pub async fn set_powered(
        &self,
        adapter_path: &str,
        powered: bool,
    ) -> Result<BluetoothSnapshot, BluetoothError> {
        self.set_adapter_property(adapter_path, "Powered", powered)
            .await?;
        self.wait_adapter(adapter_path, |adapter| adapter.powered == powered)
            .await
    }

    pub async fn set_discovery(
        &self,
        adapter_path: &str,
        enabled: bool,
    ) -> Result<BluetoothSnapshot, BluetoothError> {
        timeout(OPERATION_TIMEOUT, async {
            let adapter = self.adapter_proxy(adapter_path).await?;
            let method = if enabled {
                "StartDiscovery"
            } else {
                "StopDiscovery"
            };
            adapter.call::<_, _, ()>(method, &()).await?;
            loop {
                let snapshot = self.snapshot_inner().await?;
                if snapshot
                    .adapters
                    .iter()
                    .any(|item| item.path == adapter_path && item.discovering == enabled)
                {
                    return Ok(snapshot);
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .map_err(|_| BluetoothError::Timeout)?
    }

    pub async fn pair_device(
        &self,
        device_path: &str,
    ) -> Result<BluetoothSnapshot, BluetoothError> {
        self.pair_device_with_timeout(device_path, PAIRING_TIMEOUT)
            .await
    }

    async fn pair_device_with_timeout(
        &self,
        device_path: &str,
        pairing_timeout: Duration,
    ) -> Result<BluetoothSnapshot, BluetoothError> {
        let mut cancellation = PairingCancelGuard::new(self.clone(), device_path);
        let result = timeout(pairing_timeout, async {
            let (device, snapshot) = self.device_proxy(device_path).await?;
            if snapshot
                .devices
                .iter()
                .any(|item| item.path == device_path && item.paired)
            {
                return Ok(snapshot);
            }
            if !cancellation.register() {
                return Err(BluetoothError::PairingInProgress(device_path.to_owned()));
            }
            cancellation.arm();
            if let Err(error) = device.call::<_, _, ()>("Pair", &()).await {
                cancellation.disarm();
                return Err(error.into());
            }
            self.wait_device(device_path, |item| item.paired).await
        })
        .await;
        match result {
            Ok(Ok(snapshot)) => {
                cancellation.disarm();
                Ok(snapshot)
            }
            Err(_) | Ok(Err(BluetoothError::Timeout)) => {
                match self
                    .cancel_pairing_with_retry(device_path, &cancellation.control, true, true)
                    .await
                {
                    Ok(()) => {
                        cancellation.disarm();
                        match self.snapshot().await {
                            Ok(snapshot)
                                if snapshot
                                    .devices
                                    .iter()
                                    .any(|item| item.path == device_path && item.paired) =>
                            {
                                Ok(snapshot)
                            }
                            _ => Err(BluetoothError::Timeout),
                        }
                    }
                    Err(BluetoothError::DeviceUnavailable(_))
                    | Err(BluetoothError::MissingService(_)) => {
                        cancellation.disarm();
                        Err(BluetoothError::Timeout)
                    }
                    Err(error) => Err(BluetoothError::PairingCancellationFailed(Box::new(error))),
                }
            }
            Ok(Err(error)) => {
                cancellation.disarm();
                Err(error)
            }
        }
    }

    pub async fn cancel_pairing(&self, device_path: &str) -> Result<(), BluetoothError> {
        validate_object_path(device_path)?;
        let (control, retry) = {
            let mut pairings = self.pairings.lock().unwrap();
            if let Some(control) = pairings.get(device_path) {
                (control.clone(), true)
            } else {
                let control = Arc::new(PairingControl::new());
                pairings.insert(device_path.to_owned(), control.clone());
                (control, false)
            }
        };
        let result = self
            .cancel_pairing_with_retry(device_path, &control, false, retry)
            .await;
        if !control.in_flight.load(Ordering::Relaxed)
            && (result.is_ok()
                || control.cancellation_confirmed.load(Ordering::Relaxed)
                || control.pairing_completed.load(Ordering::Relaxed)
                || matches!(
                    &result,
                    Err(BluetoothError::DeviceUnavailable(_))
                        | Err(BluetoothError::MissingService(_))
                )
                || (!retry && matches!(&result, Err(BluetoothError::PairingNotActive(_)))))
        {
            self.remove_pairing_registration(device_path, &control);
        }
        result
    }

    async fn cancel_pairing_with_retry(
        &self,
        device_path: &str,
        control: &Arc<PairingControl>,
        pairing_already_completed_is_success: bool,
        retry_startup: bool,
    ) -> Result<(), BluetoothError> {
        timeout(OPERATION_TIMEOUT, async {
            let _cancellation = control.cancellation.lock().await;
            if control.cancellation_confirmed.load(Ordering::Relaxed) {
                return Ok(());
            }
            if control.pairing_completed.load(Ordering::Relaxed) {
                return if pairing_already_completed_is_success {
                    Ok(())
                } else {
                    Err(BluetoothError::PairingNotActive(device_path.to_owned()))
                };
            }
            let (device, snapshot) = self.device_proxy(device_path).await?;
            if snapshot
                .devices
                .iter()
                .any(|item| item.path == device_path && item.paired)
            {
                control.pairing_completed.store(true, Ordering::Relaxed);
                return if pairing_already_completed_is_success {
                    Ok(())
                } else {
                    Err(BluetoothError::PairingNotActive(device_path.to_owned()))
                };
            }
            let retry_deadline = tokio::time::Instant::now() + PAIRING_CANCEL_RETRY_TIMEOUT;
            loop {
                match device.call::<_, _, ()>("CancelPairing", &()).await {
                    Ok(()) => {
                        control
                            .cancellation_confirmed
                            .store(true, Ordering::Relaxed);
                        return Ok(());
                    }
                    Err(error) if pairing_does_not_exist(&error) => {
                        if self
                            .snapshot_inner()
                            .await?
                            .devices
                            .iter()
                            .any(|item| item.path == device_path && item.paired)
                        {
                            control.pairing_completed.store(true, Ordering::Relaxed);
                            return if pairing_already_completed_is_success {
                                Ok(())
                            } else {
                                Err(BluetoothError::PairingNotActive(device_path.to_owned()))
                            };
                        }
                        if !retry_startup || tokio::time::Instant::now() >= retry_deadline {
                            return Err(BluetoothError::PairingNotActive(device_path.to_owned()));
                        }
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    Err(error) => return Err(error.into()),
                }
            }
        })
        .await
        .map_err(|_| BluetoothError::Timeout)?
    }

    fn remove_pairing_registration(&self, device_path: &str, control: &Arc<PairingControl>) {
        let mut pairings = self.pairings.lock().unwrap();
        if pairings
            .get(device_path)
            .is_some_and(|current| Arc::ptr_eq(current, control))
        {
            pairings.remove(device_path);
        }
    }

    pub async fn connect_device(
        &self,
        device_path: &str,
    ) -> Result<BluetoothSnapshot, BluetoothError> {
        timeout(OPERATION_TIMEOUT, async {
            let (device, snapshot) = self.device_proxy(device_path).await?;
            if snapshot
                .devices
                .iter()
                .any(|item| item.path == device_path && item.connected)
            {
                return Ok(snapshot);
            }
            device.call::<_, _, ()>("Connect", &()).await?;
            self.wait_device(device_path, |item| item.connected).await
        })
        .await
        .map_err(|_| BluetoothError::Timeout)?
    }

    pub async fn disconnect_device(
        &self,
        device_path: &str,
    ) -> Result<BluetoothSnapshot, BluetoothError> {
        timeout(OPERATION_TIMEOUT, async {
            let (device, snapshot) = self.device_proxy(device_path).await?;
            if snapshot
                .devices
                .iter()
                .all(|item| item.path != device_path || !item.connected)
            {
                return Ok(snapshot);
            }
            device.call::<_, _, ()>("Disconnect", &()).await?;
            self.wait_device(device_path, |item| !item.connected).await
        })
        .await
        .map_err(|_| BluetoothError::Timeout)?
    }

    pub async fn set_device_trusted(
        &self,
        device_path: &str,
        trusted: bool,
    ) -> Result<BluetoothSnapshot, BluetoothError> {
        timeout(OPERATION_TIMEOUT, async {
            let (device, snapshot) = self.device_proxy(device_path).await?;
            if snapshot
                .devices
                .iter()
                .any(|item| item.path == device_path && item.trusted == trusted)
            {
                return Ok(snapshot);
            }
            device
                .set_property("Trusted", trusted)
                .await
                .map_err(zbus::Error::from)?;
            self.wait_device(device_path, |item| item.trusted == trusted)
                .await
        })
        .await
        .map_err(|_| BluetoothError::Timeout)?
    }

    /// Forget a device: BlueZ drops its pairing keys and the device object.
    pub async fn remove_device(
        &self,
        adapter_path: &str,
        device_path: &str,
    ) -> Result<BluetoothSnapshot, BluetoothError> {
        timeout(OPERATION_TIMEOUT, async {
            let (_, snapshot) = self.device_proxy(device_path).await?;
            if !snapshot
                .devices
                .iter()
                .any(|item| item.path == device_path && item.adapter_path == adapter_path)
            {
                return Err(BluetoothError::DeviceUnavailable(device_path.to_owned()));
            }
            let adapter = self.adapter_proxy(adapter_path).await?;
            let path = zbus::zvariant::ObjectPath::try_from(device_path)
                .map_err(|error| BluetoothError::InvalidObjectPath(error.to_string()))?;
            adapter.call::<_, _, ()>("RemoveDevice", &(path,)).await?;
            loop {
                let snapshot = self.snapshot_inner().await?;
                if snapshot.devices.iter().all(|item| item.path != device_path) {
                    return Ok(snapshot);
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .map_err(|_| BluetoothError::Timeout)?
    }

    async fn set_adapter_property(
        &self,
        path: &str,
        name: &str,
        value: bool,
    ) -> Result<(), BluetoothError> {
        timeout(OPERATION_TIMEOUT, async {
            let proxy = self.adapter_proxy(path).await?;
            proxy
                .set_property(name, value)
                .await
                .map_err(zbus::Error::from)?;
            Ok(())
        })
        .await
        .map_err(|_| BluetoothError::Timeout)?
    }

    async fn wait_adapter(
        &self,
        path: &str,
        matches: impl Fn(&BluetoothAdapter) -> bool,
    ) -> Result<BluetoothSnapshot, BluetoothError> {
        timeout(OPERATION_TIMEOUT, async {
            loop {
                let snapshot = self.snapshot_inner().await?;
                if snapshot
                    .adapters
                    .iter()
                    .any(|adapter| adapter.path == path && matches(adapter))
                {
                    return Ok(snapshot);
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .map_err(|_| BluetoothError::Timeout)?
    }

    async fn wait_device(
        &self,
        path: &str,
        matches: impl Fn(&BluetoothDevice) -> bool,
    ) -> Result<BluetoothSnapshot, BluetoothError> {
        timeout(OPERATION_TIMEOUT, async {
            loop {
                let snapshot = self.snapshot_inner().await?;
                if snapshot
                    .devices
                    .iter()
                    .any(|device| device.path == path && matches(device))
                {
                    return Ok(snapshot);
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .map_err(|_| BluetoothError::Timeout)?
    }

    async fn adapter_proxy<'a>(&'a self, path: &'a str) -> Result<Proxy<'a>, BluetoothError> {
        validate_object_path(path)?;
        let snapshot = self.snapshot_inner().await?;
        if !snapshot.adapters.iter().any(|adapter| adapter.path == path) {
            return Err(BluetoothError::AdapterUnavailable(path.to_owned()));
        }
        Ok(Proxy::new(&self.connection, SERVICE, path, ADAPTER).await?)
    }

    async fn device_proxy<'a>(
        &'a self,
        path: &'a str,
    ) -> Result<(Proxy<'a>, BluetoothSnapshot), BluetoothError> {
        validate_object_path(path)?;
        let snapshot = self.snapshot_inner().await?;
        if !snapshot.devices.iter().any(|device| device.path == path) {
            return Err(BluetoothError::DeviceUnavailable(path.to_owned()));
        }
        Ok((
            Proxy::new(&self.connection, SERVICE, path, DEVICE).await?,
            snapshot,
        ))
    }

    async fn snapshot_inner(&self) -> Result<BluetoothSnapshot, BluetoothError> {
        let proxy = Proxy::new(&self.connection, SERVICE, ROOT, OBJECT_MANAGER).await?;
        let managed: HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>> =
            proxy.call("GetManagedObjects", &()).await?;
        let mut adapters = Vec::new();
        let mut devices = Vec::new();
        for (path, interfaces) in managed {
            if let Some(properties) = interfaces.get(ADAPTER) {
                adapters.push(BluetoothAdapter {
                    path: path.to_string(),
                    name: string_property(properties, "Alias")?
                        .or(string_property(properties, "Name")?)
                        .unwrap_or_default(),
                    address: string_property(properties, "Address")?.unwrap_or_default(),
                    powered: bool_property(properties, "Powered")?,
                    discovering: bool_property(properties, "Discovering")?,
                });
            }
            if let Some(properties) = interfaces.get(DEVICE) {
                devices.push(BluetoothDevice {
                    path: path.to_string(),
                    name: string_property(properties, "Alias")?
                        .or(string_property(properties, "Name")?)
                        .unwrap_or_default(),
                    address: string_property(properties, "Address")?.unwrap_or_default(),
                    adapter_path: path_property(properties, "Adapter")?
                        .ok_or_else(|| BluetoothError::InvalidProperty("Adapter".into()))?,
                    paired: bool_property(properties, "Paired")?,
                    trusted: bool_property(properties, "Trusted")?,
                    connected: bool_property(properties, "Connected")?,
                });
            }
        }
        adapters.sort_by(|left, right| left.path.cmp(&right.path));
        devices.sort_by(|left, right| left.path.cmp(&right.path));
        let availability = if adapters.is_empty() {
            Availability::UnsupportedHardware
        } else {
            Availability::Available
        };
        Ok(BluetoothSnapshot {
            revision: self.revision.fetch_add(1, Ordering::Relaxed) + 1,
            availability,
            adapters,
            devices,
        })
    }
}

fn validate_object_path(path: &str) -> Result<(), BluetoothError> {
    IpcObjectPath::new(path.to_owned())
        .map(|_| ())
        .map_err(|error| BluetoothError::InvalidObjectPath(error.to_string()))
}

fn pairing_does_not_exist(error: &zbus::Error) -> bool {
    matches!(
        error,
        zbus::Error::MethodError(name, _, _) if name.as_str() == "org.bluez.Error.DoesNotExist"
    )
}

fn string_property(
    properties: &HashMap<String, OwnedValue>,
    name: &str,
) -> Result<Option<String>, BluetoothError> {
    properties
        .get(name)
        .map(|value| {
            value
                .try_clone()
                .and_then(String::try_from)
                .map_err(|_| BluetoothError::InvalidProperty(name.to_owned()))
        })
        .transpose()
}

fn bool_property(
    properties: &HashMap<String, OwnedValue>,
    name: &str,
) -> Result<bool, BluetoothError> {
    properties
        .get(name)
        .map(bool::try_from)
        .transpose()
        .map_err(|_| BluetoothError::InvalidProperty(name.to_owned()))?
        .ok_or_else(|| BluetoothError::InvalidProperty(name.to_owned()))
}

fn path_property(
    properties: &HashMap<String, OwnedValue>,
    name: &str,
) -> Result<Option<String>, BluetoothError> {
    properties
        .get(name)
        .map(|value| {
            value
                .try_clone()
                .and_then(OwnedObjectPath::try_from)
                .map(|path| path.to_string())
                .map_err(|_| BluetoothError::InvalidProperty(name.to_owned()))
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::BufRead,
        process::{Child, Command, Stdio},
    };

    struct BusDaemon(Child);

    impl Drop for BusDaemon {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    struct BluezFixture {
        removed: Arc<std::sync::atomic::AtomicBool>,
        powered: Arc<std::sync::atomic::AtomicBool>,
        discovering: Arc<std::sync::atomic::AtomicBool>,
        paired: Arc<std::sync::atomic::AtomicBool>,
        trusted: Arc<std::sync::atomic::AtomicBool>,
        connected: Arc<std::sync::atomic::AtomicBool>,
    }

    #[zbus::interface(name = "org.freedesktop.DBus.ObjectManager")]
    impl BluezFixture {
        fn get_managed_objects(
            &self,
        ) -> HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>> {
            let mut properties = HashMap::new();
            properties.insert(
                "Address".into(),
                OwnedValue::from(zbus::zvariant::Str::from("00:11:22:33:44:55")),
            );
            properties.insert(
                "Alias".into(),
                OwnedValue::from(zbus::zvariant::Str::from("Fixture Adapter")),
            );
            properties.insert(
                "Powered".into(),
                OwnedValue::from(self.powered.load(Ordering::Relaxed)),
            );
            properties.insert(
                "Discovering".into(),
                OwnedValue::from(self.discovering.load(Ordering::Relaxed)),
            );
            let adapter_path = OwnedObjectPath::try_from("/org/bluez/hci0").unwrap();
            let device_path =
                OwnedObjectPath::try_from("/org/bluez/hci0/dev_00_11_22_33_44_55").unwrap();
            let mut device_properties = HashMap::new();
            device_properties.insert(
                "Address".into(),
                OwnedValue::from(zbus::zvariant::Str::from("00:11:22:33:44:55")),
            );
            device_properties.insert(
                "Alias".into(),
                OwnedValue::from(zbus::zvariant::Str::from("Fixture Device")),
            );
            device_properties.insert(
                "Adapter".into(),
                OwnedValue::try_from(zbus::zvariant::Value::from(adapter_path.clone())).unwrap(),
            );
            device_properties.insert(
                "Paired".into(),
                OwnedValue::from(self.paired.load(Ordering::Relaxed)),
            );
            device_properties.insert(
                "Trusted".into(),
                OwnedValue::from(self.trusted.load(Ordering::Relaxed)),
            );
            device_properties.insert(
                "Connected".into(),
                OwnedValue::from(self.connected.load(Ordering::Relaxed)),
            );
            let mut objects =
                HashMap::from([(adapter_path, HashMap::from([(ADAPTER.into(), properties)]))]);
            if !self.removed.load(Ordering::Relaxed) {
                objects.insert(
                    device_path,
                    HashMap::from([(DEVICE.into(), device_properties)]),
                );
            }
            objects
        }
    }

    #[tokio::test]
    async fn bluetooth_snapshot_reads_adapter_and_device_state_from_private_bluez_bus() {
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
        let powered = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let removed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let discovering = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let denied = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let paired = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let trusted = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let connected = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let denied_trust = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let pairing_active = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let pairing_cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let pair_call_pending = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let pairing_cancel_signal = Arc::new(tokio::sync::Notify::new());
        let complete_pairing = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let fail_pairing_cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let pairing_cancel_attempts = Arc::new(AtomicU64::new(0));
        let delay_pairing_start = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let pairing_start_ready = Arc::new(tokio::sync::Notify::new());
        let pairing_start_continue = Arc::new(tokio::sync::Notify::new());
        let service = zbus::connection::Builder::address(address.trim())
            .unwrap()
            .name(SERVICE)
            .unwrap()
            .serve_at(
                ROOT,
                BluezFixture {
                    removed: removed.clone(),
                    powered: powered.clone(),
                    discovering: discovering.clone(),
                    paired: paired.clone(),
                    trusted: trusted.clone(),
                    connected: connected.clone(),
                },
            )
            .unwrap()
            .serve_at(
                "/org/bluez/hci0",
                AdapterFixture {
                    removed: removed.clone(),
                    powered: powered.clone(),
                    discovering: discovering.clone(),
                    denied: denied.clone(),
                },
            )
            .unwrap()
            .serve_at(
                "/org/bluez/hci0/dev_00_11_22_33_44_55",
                DeviceFixture {
                    paired: paired.clone(),
                    trusted: trusted.clone(),
                    connected: connected.clone(),
                    denied_trust: denied_trust.clone(),
                    pairing_active: pairing_active.clone(),
                    pairing_cancelled: pairing_cancelled.clone(),
                    pair_call_pending: pair_call_pending.clone(),
                    pairing_cancel_signal: pairing_cancel_signal.clone(),
                    complete_pairing: complete_pairing.clone(),
                    fail_pairing_cancel: fail_pairing_cancel.clone(),
                    pairing_cancel_attempts: pairing_cancel_attempts.clone(),
                    delay_pairing_start: delay_pairing_start.clone(),
                    pairing_start_ready: pairing_start_ready.clone(),
                    pairing_start_continue: pairing_start_continue.clone(),
                },
            )
            .unwrap()
            .build()
            .await
            .unwrap();
        let client = BluetoothClient {
            connection: zbus::connection::Builder::address(address.trim())
                .unwrap()
                .build()
                .await
                .unwrap(),
            revision: Arc::new(AtomicU64::new(0)),
            pairings: Arc::new(std::sync::Mutex::new(HashMap::new())),
        };
        let snapshot = client.snapshot().await.unwrap();
        assert_eq!(snapshot.availability, Availability::Available);
        assert_eq!(snapshot.adapters.len(), 1);
        assert_eq!(snapshot.adapters[0].path, "/org/bluez/hci0");
        assert_eq!(snapshot.adapters[0].name, "Fixture Adapter");
        assert_eq!(snapshot.adapters[0].address, "00:11:22:33:44:55");
        assert!(snapshot.adapters[0].powered);
        assert!(!snapshot.adapters[0].discovering);
        assert_eq!(snapshot.devices.len(), 1);
        assert_eq!(snapshot.devices[0].name, "Fixture Device");
        assert_eq!(snapshot.devices[0].adapter_path, "/org/bluez/hci0");
        assert!(!snapshot.devices[0].paired);
        assert!(!snapshot.devices[0].trusted);
        assert!(!snapshot.devices[0].connected);
        assert!(matches!(
            client
                .connect_device("org/bluez/hci0/dev_00_11_22_33_44_55")
                .await,
            Err(BluetoothError::InvalidObjectPath(_))
        ));
        assert!(matches!(
            client
                .disconnect_device("/org/bluez/hci0/dev_00_11_22_33_44_66")
                .await,
            Err(BluetoothError::DeviceUnavailable(_))
        ));
        assert!(matches!(
            client.set_powered("org/bluez/hci0", false).await,
            Err(BluetoothError::InvalidObjectPath(_))
        ));
        let changed = client.set_powered("/org/bluez/hci0", false).await.unwrap();
        assert!(!changed.adapters[0].powered);
        let changed = client.set_discovery("/org/bluez/hci0", true).await.unwrap();
        assert!(changed.adapters[0].discovering);
        let changed = client
            .pair_device("/org/bluez/hci0/dev_00_11_22_33_44_55")
            .await
            .unwrap();
        assert!(changed.devices[0].paired);

        paired.store(false, Ordering::Relaxed);
        connected.store(false, Ordering::Relaxed);
        complete_pairing.store(false, Ordering::Relaxed);
        assert!(matches!(
            client
                .pair_device_with_timeout(
                    "/org/bluez/hci0/dev_00_11_22_33_44_55",
                    std::time::Duration::from_millis(100),
                )
                .await,
            Err(BluetoothError::Timeout)
        ));
        assert!(pairing_cancelled.load(Ordering::Relaxed));
        assert!(!pairing_active.load(Ordering::Relaxed));

        pairing_cancelled.store(false, Ordering::Relaxed);
        complete_pairing.store(false, Ordering::Relaxed);
        delay_pairing_start.store(true, Ordering::Relaxed);
        paired.store(false, Ordering::Relaxed);
        let attempts_before_cancel = pairing_cancel_attempts.load(Ordering::Relaxed);
        let pairing_client = client.clone();
        let pairing = tokio::spawn(async move {
            pairing_client
                .pair_device_with_timeout(
                    "/org/bluez/hci0/dev_00_11_22_33_44_55",
                    Duration::from_secs(30),
                )
                .await
        });
        timeout(Duration::from_secs(1), pairing_start_ready.notified())
            .await
            .unwrap();

        let release_pairing = pairing_start_continue.clone();
        let cancel_attempts = pairing_cancel_attempts.clone();
        let release = tokio::spawn(async move {
            timeout(Duration::from_secs(1), async {
                while cancel_attempts.load(Ordering::Relaxed) == attempts_before_cancel {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            release_pairing.notify_one();
        });
        let (first_cancel, second_cancel) = tokio::join!(
            client.cancel_pairing("/org/bluez/hci0/dev_00_11_22_33_44_55"),
            client.cancel_pairing("/org/bluez/hci0/dev_00_11_22_33_44_55")
        );
        first_cancel.unwrap();
        second_cancel.unwrap();
        release.await.unwrap();
        assert!(matches!(
            pairing.await.unwrap(),
            Err(BluetoothError::PairingCancelled(_))
        ));
        assert!(pairing_cancelled.load(Ordering::Relaxed));
        assert!(!pairing_active.load(Ordering::Relaxed));

        pairing_cancelled.store(false, Ordering::Relaxed);
        let attempts_before_drop = pairing_cancel_attempts.load(Ordering::Relaxed);
        let pairing_client = client.clone();
        let pairing = tokio::spawn(async move {
            pairing_client
                .pair_device_with_timeout(
                    "/org/bluez/hci0/dev_00_11_22_33_44_55",
                    Duration::from_secs(30),
                )
                .await
        });
        timeout(Duration::from_secs(1), pairing_start_ready.notified())
            .await
            .unwrap();
        pairing.abort();
        assert!(pairing.await.unwrap_err().is_cancelled());
        timeout(Duration::from_secs(1), async {
            while pairing_cancel_attempts.load(Ordering::Relaxed) == attempts_before_drop {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        pairing_start_continue.notify_one();
        timeout(Duration::from_secs(1), async {
            while !pairing_cancelled.load(Ordering::Relaxed) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(!pairing_active.load(Ordering::Relaxed));
        delay_pairing_start.store(false, Ordering::Relaxed);
        timeout(Duration::from_secs(1), async {
            while client
                .pairings
                .lock()
                .unwrap()
                .contains_key("/org/bluez/hci0/dev_00_11_22_33_44_55")
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();

        pairing_active.store(true, Ordering::Relaxed);
        pairing_cancelled.store(false, Ordering::Relaxed);
        client
            .cancel_pairing("/org/bluez/hci0/dev_00_11_22_33_44_55")
            .await
            .unwrap();
        assert!(pairing_cancelled.load(Ordering::Relaxed));
        assert!(matches!(
            client
                .cancel_pairing("/org/bluez/hci0/dev_00_11_22_33_44_55")
                .await,
            Err(BluetoothError::PairingNotActive(_))
        ));

        pairing_cancelled.store(false, Ordering::Relaxed);
        fail_pairing_cancel.store(true, Ordering::Relaxed);
        let attempts_before_failed_cleanup = pairing_cancel_attempts.load(Ordering::Relaxed);
        assert!(matches!(
            client
                .pair_device_with_timeout(
                    "/org/bluez/hci0/dev_00_11_22_33_44_55",
                    std::time::Duration::from_millis(100),
                )
                .await,
            Err(BluetoothError::PairingCancellationFailed(_))
        ));
        timeout(Duration::from_secs(1), async {
            while pairing_cancel_attempts.load(Ordering::Relaxed)
                < attempts_before_failed_cleanup + 2
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(matches!(
            client
                .pair_device("/org/bluez/hci0/dev_00_11_22_33_44_55")
                .await,
            Err(BluetoothError::PairingInProgress(_))
        ));
        fail_pairing_cancel.store(false, Ordering::Relaxed);
        client
            .cancel_pairing("/org/bluez/hci0/dev_00_11_22_33_44_55")
            .await
            .unwrap();
        let changed = client
            .disconnect_device("/org/bluez/hci0/dev_00_11_22_33_44_55")
            .await
            .unwrap();
        assert!(!changed.devices[0].connected);
        let changed = client
            .connect_device("/org/bluez/hci0/dev_00_11_22_33_44_55")
            .await
            .unwrap();
        assert!(changed.devices[0].connected);
        let changed = client
            .set_device_trusted("/org/bluez/hci0/dev_00_11_22_33_44_55", true)
            .await
            .unwrap();
        assert!(changed.devices[0].trusted);
        denied_trust.store(true, Ordering::Relaxed);
        assert!(matches!(
            client
                .set_device_trusted("/org/bluez/hci0/dev_00_11_22_33_44_55", false)
                .await,
            Err(BluetoothError::PermissionDenied(_))
        ));
        assert!(trusted.load(Ordering::Relaxed));
        denied.store(true, Ordering::Relaxed);
        let denied_result = client.set_powered("/org/bluez/hci0", true).await;
        assert!(
            matches!(denied_result, Err(BluetoothError::PermissionDenied(_))),
            "{denied_result:?}"
        );
        assert!(!powered.load(Ordering::Relaxed));
        assert!(matches!(
            client
                .remove_device("/org/bluez/hci1", "/org/bluez/hci0/dev_00_11_22_33_44_55")
                .await,
            Err(BluetoothError::DeviceUnavailable(_))
        ));
        let forgotten = client
            .remove_device("/org/bluez/hci0", "/org/bluez/hci0/dev_00_11_22_33_44_55")
            .await
            .unwrap();
        assert!(forgotten.devices.is_empty() && removed.load(Ordering::Relaxed));
        service.close().await.unwrap();
        let missing = client.snapshot().await.unwrap_err();
        assert_eq!(
            missing.availability(),
            Availability::MissingService,
            "{missing:?}"
        );
    }

    #[tokio::test]
    #[ignore = "reads the guest system bus and live BlueZ service state"]
    async fn bluetooth_provider_reads_live_service_or_reports_unavailable() {
        match BluetoothClient::connect().await {
            Ok(client) => match client.snapshot().await {
                Ok(snapshot) => assert!(matches!(
                    snapshot.availability,
                    Availability::Available | Availability::UnsupportedHardware
                )),
                Err(error) => assert!(matches!(
                    error.availability(),
                    Availability::MissingService
                        | Availability::PermissionDenied
                        | Availability::Failed
                )),
            },
            Err(error) => assert!(matches!(
                error.availability(),
                Availability::MissingService
                    | Availability::PermissionDenied
                    | Availability::Failed
            )),
        }
    }

    struct AdapterFixture {
        removed: Arc<std::sync::atomic::AtomicBool>,
        powered: Arc<std::sync::atomic::AtomicBool>,
        discovering: Arc<std::sync::atomic::AtomicBool>,
        denied: Arc<std::sync::atomic::AtomicBool>,
    }

    #[zbus::interface(name = "org.bluez.Adapter1")]
    impl AdapterFixture {
        #[zbus(property)]
        fn powered(&self) -> bool {
            self.powered.load(Ordering::Relaxed)
        }

        #[zbus(property)]
        fn set_powered(&self, value: bool) -> zbus::Result<()> {
            if self.denied.load(Ordering::Relaxed) {
                return Err(zbus::fdo::Error::AccessDenied("fixture denied".into()).into());
            }
            self.powered.store(value, Ordering::Relaxed);
            Ok(())
        }

        #[zbus(property)]
        fn discovering(&self) -> bool {
            self.discovering.load(Ordering::Relaxed)
        }

        fn start_discovery(&self) {
            self.discovering.store(true, Ordering::Relaxed);
        }

        fn stop_discovery(&self) {
            self.discovering.store(false, Ordering::Relaxed);
        }

        fn remove_device(&self, device: OwnedObjectPath) -> zbus::fdo::Result<()> {
            if device.as_str() != "/org/bluez/hci0/dev_00_11_22_33_44_55" {
                return Err(zbus::fdo::Error::InvalidArgs("unknown device".into()));
            }
            self.removed.store(true, Ordering::Relaxed);
            Ok(())
        }
    }

    struct DeviceFixture {
        paired: Arc<std::sync::atomic::AtomicBool>,
        trusted: Arc<std::sync::atomic::AtomicBool>,
        connected: Arc<std::sync::atomic::AtomicBool>,
        denied_trust: Arc<std::sync::atomic::AtomicBool>,
        pairing_active: Arc<std::sync::atomic::AtomicBool>,
        pairing_cancelled: Arc<std::sync::atomic::AtomicBool>,
        pair_call_pending: Arc<std::sync::atomic::AtomicBool>,
        pairing_cancel_signal: Arc<tokio::sync::Notify>,
        pairing_cancel_attempts: Arc<AtomicU64>,
        complete_pairing: Arc<std::sync::atomic::AtomicBool>,
        fail_pairing_cancel: Arc<std::sync::atomic::AtomicBool>,
        delay_pairing_start: Arc<std::sync::atomic::AtomicBool>,
        pairing_start_ready: Arc<tokio::sync::Notify>,
        pairing_start_continue: Arc<tokio::sync::Notify>,
    }

    #[derive(Debug, zbus::DBusError)]
    #[zbus(prefix = "org.bluez.Error")]
    enum BluezFixtureError {
        DoesNotExist(String),
        Failed(String),
        AuthenticationCanceled(String),
        #[zbus(error)]
        ZBus(zbus::Error),
    }

    #[zbus::interface(name = "org.bluez.Device1")]
    impl DeviceFixture {
        async fn pair(&self) -> Result<(), BluezFixtureError> {
            self.pair_call_pending.store(true, Ordering::Relaxed);
            if self.delay_pairing_start.load(Ordering::Relaxed) {
                self.pairing_start_ready.notify_one();
                self.pairing_start_continue.notified().await;
            }
            self.pairing_active.store(true, Ordering::Relaxed);
            if self.complete_pairing.load(Ordering::Relaxed) {
                self.paired.store(true, Ordering::Relaxed);
                self.connected.store(true, Ordering::Relaxed);
                self.pairing_active.store(false, Ordering::Relaxed);
                self.pair_call_pending.store(false, Ordering::Relaxed);
                Ok(())
            } else {
                self.pairing_cancel_signal.notified().await;
                self.pair_call_pending.store(false, Ordering::Relaxed);
                Err(BluezFixtureError::AuthenticationCanceled(
                    "fixture pairing cancelled".into(),
                ))
            }
        }

        fn cancel_pairing(&self) -> Result<(), BluezFixtureError> {
            self.pairing_cancel_attempts.fetch_add(1, Ordering::Relaxed);
            if self.fail_pairing_cancel.load(Ordering::Relaxed) {
                return Err(BluezFixtureError::Failed(
                    "fixture cancellation failed".into(),
                ));
            }
            if !self.pairing_active.swap(false, Ordering::Relaxed) {
                return Err(BluezFixtureError::DoesNotExist("no active pairing".into()));
            }
            self.pairing_cancelled.store(true, Ordering::Relaxed);
            if self.pair_call_pending.swap(false, Ordering::Relaxed) {
                self.pairing_cancel_signal.notify_one();
            }
            Ok(())
        }

        fn connect(&self) {
            self.connected.store(true, Ordering::Relaxed);
        }

        fn disconnect(&self) {
            self.connected.store(false, Ordering::Relaxed);
        }

        #[zbus(property)]
        fn trusted(&self) -> bool {
            self.trusted.load(Ordering::Relaxed)
        }

        #[zbus(property)]
        fn set_trusted(&self, value: bool) -> zbus::Result<()> {
            if self.denied_trust.load(Ordering::Relaxed) {
                return Err(zbus::fdo::Error::AccessDenied("fixture denied".into()).into());
            }
            self.trusted.store(value, Ordering::Relaxed);
            Ok(())
        }
    }
}
