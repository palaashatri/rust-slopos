use std::{
    collections::HashMap,
    net::Ipv6Addr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use futures::StreamExt;
use slopos_core::capability::Availability;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinSet;
use tokio::time::timeout;
use zbus::{Connection, Proxy};

const SERVICE: &str = "org.freedesktop.NetworkManager";
const MANAGER_PATH: &str = "/org/freedesktop/NetworkManager";
const MANAGER_INTERFACE: &str = "org.freedesktop.NetworkManager";
const DEVICE_INTERFACE: &str = "org.freedesktop.NetworkManager.Device";
const WIRELESS_INTERFACE: &str = "org.freedesktop.NetworkManager.Device.Wireless";
const ACCESS_POINT_INTERFACE: &str = "org.freedesktop.NetworkManager.AccessPoint";
const ACTIVE_CONNECTION_INTERFACE: &str = "org.freedesktop.NetworkManager.Connection.Active";
const IP4_CONFIG_INTERFACE: &str = "org.freedesktop.NetworkManager.IP4Config";
const IP6_CONFIG_INTERFACE: &str = "org.freedesktop.NetworkManager.IP6Config";
const SETTINGS_PATH: &str = "/org/freedesktop/NetworkManager/Settings";
const SETTINGS_INTERFACE: &str = "org.freedesktop.NetworkManager.Settings";
const SETTINGS_CONNECTION_INTERFACE: &str = "org.freedesktop.NetworkManager.Settings.Connection";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkDeviceType {
    Ethernet,
    Wifi,
    Bluetooth,
    Other(u32),
}

impl NetworkDeviceType {
    pub fn from_raw(value: u32) -> Self {
        match value {
            1 => Self::Ethernet,
            2 => Self::Wifi,
            5 => Self::Bluetooth,
            other => Self::Other(other),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkDeviceState {
    Unknown,
    Unmanaged,
    Unavailable,
    Disconnected,
    Prepare,
    Config,
    NeedAuth,
    IpConfig,
    IpCheck,
    Secondaries,
    Activated,
    Deactivating,
    Failed,
    Other(u32),
}

impl NetworkDeviceState {
    pub fn from_raw(value: u32) -> Self {
        match value {
            0 => Self::Unknown,
            10 => Self::Unmanaged,
            20 => Self::Unavailable,
            30 => Self::Disconnected,
            40 => Self::Prepare,
            50 => Self::Config,
            60 => Self::NeedAuth,
            70 => Self::IpConfig,
            80 => Self::IpCheck,
            90 => Self::Secondaries,
            100 => Self::Activated,
            110 => Self::Deactivating,
            120 => Self::Failed,
            other => Self::Other(other),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkDevice {
    pub object_path: String,
    pub interface: String,
    pub ip_interface: String,
    pub kind: NetworkDeviceType,
    pub state: NetworkDeviceState,
    pub active_access_point: Option<String>,
    pub access_points: Vec<NetworkAccessPoint>,
    pub active_connection: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkAccessPoint {
    pub object_path: String,
    pub ssid: Vec<u8>,
    pub strength: u8,
    pub frequency_mhz: u32,
    pub wpa_flags: u32,
    pub rsn_flags: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveNetworkConnection {
    pub object_path: String,
    pub id: String,
    pub uuid: String,
    pub connection_type: String,
    pub state: u32,
    pub default_ipv4: bool,
    pub default_ipv6: bool,
    pub is_vpn: bool,
    pub ipv4: Option<NetworkIpConfig>,
    pub ipv6: Option<NetworkIpConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedConnection {
    pub object_path: String,
    pub id: String,
    pub uuid: String,
    pub connection_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedVpnConnection {
    pub object_path: String,
    pub id: String,
    pub uuid: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkIpConfig {
    pub addresses: Vec<NetworkIpAddress>,
    pub gateway: Option<String>,
    pub nameservers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkIpAddress {
    pub address: String,
    pub prefix: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkSnapshot {
    pub revision: u64,
    pub networking_enabled: bool,
    pub wireless_enabled: bool,
    pub wireless_hardware_enabled: bool,
    pub devices: Vec<NetworkDevice>,
    pub active_connections: Vec<ActiveNetworkConnection>,
}

type SettingsMap = HashMap<String, HashMap<String, zbus::zvariant::OwnedValue>>;

/// NetworkManager settings for a new Wi-Fi profile. WPA passphrases must be
/// 8–63 characters (or 64 hex digits), checked before anything is sent.
pub fn wifi_settings(
    name: &str,
    ssid: &[u8],
    password: Option<&str>,
) -> Result<SettingsMap, NetworkError> {
    use zbus::zvariant::{OwnedValue, Value};
    let owned = |value: Value<'_>| {
        OwnedValue::try_from(value).map_err(|e| NetworkError::InvalidProperty(e.to_string()))
    };
    if ssid.is_empty() || ssid.len() > 32 {
        return Err(NetworkError::InvalidProperty(
            "a Wi-Fi name is 1–32 bytes".into(),
        ));
    }
    let mut settings = SettingsMap::new();
    settings.insert(
        "connection".into(),
        HashMap::from([
            ("id".to_owned(), owned(Value::from(name))?),
            ("type".to_owned(), owned(Value::from("802-11-wireless"))?),
        ]),
    );
    settings.insert(
        "802-11-wireless".into(),
        HashMap::from([
            ("ssid".to_owned(), owned(Value::from(ssid.to_vec()))?),
            ("mode".to_owned(), owned(Value::from("infrastructure"))?),
        ]),
    );
    if let Some(password) = password {
        let hex_key = password.len() == 64 && password.bytes().all(|b| b.is_ascii_hexdigit());
        if !(8..=63).contains(&password.chars().count()) && !hex_key {
            return Err(NetworkError::InvalidProperty(
                "a Wi-Fi password has 8 to 63 characters".into(),
            ));
        }
        settings.insert(
            "802-11-wireless-security".into(),
            HashMap::from([
                ("key-mgmt".to_owned(), owned(Value::from("wpa-psk"))?),
                ("psk".to_owned(), owned(Value::from(password))?),
            ]),
        );
    }
    Ok(settings)
}

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("NetworkManager query timed out")]
    Timeout,
    #[error("NetworkManager service disconnected")]
    Disconnected,
    #[error("NetworkManager is unavailable: {0}")]
    MissingService(#[source] zbus::Error),
    #[error("NetworkManager access denied: {0}")]
    PermissionDenied(#[source] zbus::Error),
    #[error("Invalid NetworkManager property: {0}")]
    InvalidProperty(String),
    #[error("Invalid NetworkManager object path: {0}")]
    InvalidObjectPath(String),
    #[error("NetworkManager Wi-Fi device not found: {0}")]
    WirelessDeviceNotFound(String),
    #[error("VPN connection profile not found: {0}")]
    VpnProfileNotFound(String),
    #[error("no active non-VPN default connection is available as a VPN base")]
    NoVpnBaseConnection,
    #[error("NetworkManager D-Bus error: {0}")]
    Bus(zbus::Error),
}

impl From<zbus::Error> for NetworkError {
    fn from(error: zbus::Error) -> Self {
        let availability = availability_from_zbus_error(&error);
        match availability {
            Availability::MissingService => Self::MissingService(error),
            Availability::PermissionDenied => Self::PermissionDenied(error),
            _ => Self::Bus(error),
        }
    }
}

fn availability_from_zbus_error(error: &zbus::Error) -> Availability {
    match error {
        zbus::Error::MethodError(name, _, _) => availability_from_dbus_error(name.as_str()),
        zbus::Error::FDO(error) => match error.as_ref() {
            zbus::fdo::Error::ServiceUnknown(_) | zbus::fdo::Error::NameHasNoOwner(_) => {
                Availability::MissingService
            }
            zbus::fdo::Error::AccessDenied(_) => Availability::PermissionDenied,
            zbus::fdo::Error::ZBus(zbus::Error::MethodError(_, Some(message), _)) => {
                availability_from_dbus_error(message.split(':').next().unwrap_or_default())
            }
            zbus::fdo::Error::ZBus(error) => availability_from_zbus_error(error),
            _ => Availability::Failed,
        },
        _ => Availability::Failed,
    }
}

impl NetworkError {
    pub fn availability(&self) -> Availability {
        match self {
            Self::Timeout => Availability::Failed,
            Self::Disconnected => Availability::Disconnected,
            Self::MissingService(_) => Availability::MissingService,
            Self::PermissionDenied(_) => Availability::PermissionDenied,
            Self::Bus(_) => Availability::Failed,
            Self::InvalidProperty(_) => Availability::Failed,
            Self::InvalidObjectPath(_) => Availability::Failed,
            Self::WirelessDeviceNotFound(_)
            | Self::VpnProfileNotFound(_)
            | Self::NoVpnBaseConnection => Availability::Failed,
        }
    }
}

pub fn availability_from_dbus_error(name: &str) -> Availability {
    match name {
        "org.freedesktop.DBus.Error.ServiceUnknown"
        | "org.freedesktop.DBus.Error.NameHasNoOwner" => Availability::MissingService,
        "org.freedesktop.DBus.Error.AccessDenied" => Availability::PermissionDenied,
        _ => Availability::Failed,
    }
}

#[derive(Debug, Clone)]
pub struct NetworkManagerClient {
    connection: Connection,
    revision: Arc<AtomicU64>,
}

pub struct NetworkEventStream {
    receiver: mpsc::Receiver<Result<NetworkSnapshot, NetworkError>>,
}

impl NetworkEventStream {
    pub async fn next(&mut self) -> Option<Result<NetworkSnapshot, NetworkError>> {
        self.receiver.recv().await
    }
}

impl NetworkManagerClient {
    pub async fn connect() -> Result<Self, NetworkError> {
        let connection = timeout(Duration::from_secs(3), Connection::system())
            .await
            .map_err(|_| NetworkError::Timeout)??;
        Ok(Self {
            connection,
            revision: Arc::new(AtomicU64::new(0)),
        })
    }

    pub async fn snapshot(&self) -> Result<NetworkSnapshot, NetworkError> {
        let mut snapshot = timeout(Duration::from_secs(5), self.snapshot_inner())
            .await
            .map_err(|_| NetworkError::Timeout)??;
        snapshot.revision = self.revision.fetch_add(1, Ordering::Relaxed) + 1;
        Ok(snapshot)
    }

    pub async fn request_wifi_scan(&self, device_path: &str) -> Result<(), NetworkError> {
        let device_path = zbus::zvariant::OwnedObjectPath::try_from(device_path.to_owned())
            .map_err(|_| NetworkError::InvalidObjectPath(device_path.to_owned()))?;
        timeout(Duration::from_secs(5), async {
            let snapshot = self.snapshot().await?;
            if !snapshot.devices.iter().any(|device| {
                device.object_path == device_path.as_str() && device.kind == NetworkDeviceType::Wifi
            }) {
                return Err(NetworkError::WirelessDeviceNotFound(
                    device_path.to_string(),
                ));
            }
            let wireless = Proxy::new(
                &self.connection,
                SERVICE,
                device_path.as_str(),
                WIRELESS_INTERFACE,
            )
            .await?;
            wireless
                .call::<_, _, ()>(
                    "RequestScan",
                    &(HashMap::<String, zbus::zvariant::OwnedValue>::new(),),
                )
                .await?;
            Ok(())
        })
        .await
        .map_err(|_| NetworkError::Timeout)?
    }

    pub async fn saved_connections(&self) -> Result<Vec<SavedConnection>, NetworkError> {
        timeout(Duration::from_secs(5), async {
            let settings =
                Proxy::new(&self.connection, SERVICE, SETTINGS_PATH, SETTINGS_INTERFACE).await?;
            let paths: Vec<zbus::zvariant::OwnedObjectPath> =
                settings.call("ListConnections", &()).await?;
            let mut profiles = Vec::new();
            for path in paths {
                let profile = Proxy::new(
                    &self.connection,
                    SERVICE,
                    path.as_str(),
                    SETTINGS_CONNECTION_INTERFACE,
                )
                .await?;
                let settings: HashMap<String, HashMap<String, zbus::zvariant::OwnedValue>> =
                    profile.call("GetSettings", &()).await?;
                let Some(connection) = settings.get("connection") else {
                    return Err(NetworkError::InvalidProperty(
                        "missing connection settings".into(),
                    ));
                };
                profiles.push(SavedConnection {
                    object_path: path.to_string(),
                    id: string_field(connection, "id")?,
                    uuid: string_field(connection, "uuid")?,
                    connection_type: string_field(connection, "type")?,
                });
            }
            Ok(profiles)
        })
        .await
        .map_err(|_| NetworkError::Timeout)?
    }

    pub async fn saved_vpn_connections(&self) -> Result<Vec<SavedVpnConnection>, NetworkError> {
        Ok(self
            .saved_connections()
            .await?
            .into_iter()
            .filter(|profile| profile.connection_type == "vpn")
            .map(|profile| SavedVpnConnection {
                object_path: profile.object_path,
                id: profile.id,
                uuid: profile.uuid,
            })
            .collect())
    }

    pub async fn set_networking_enabled(
        &self,
        enabled: bool,
    ) -> Result<NetworkSnapshot, NetworkError> {
        timeout(Duration::from_secs(5), async {
            let manager =
                Proxy::new(&self.connection, SERVICE, MANAGER_PATH, MANAGER_INTERFACE).await?;
            manager.call::<_, _, ()>("Enable", &(enabled,)).await?;
            loop {
                let snapshot = self.snapshot().await?;
                if snapshot.networking_enabled == enabled {
                    return Ok(snapshot);
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .map_err(|_| NetworkError::Timeout)?
    }

    pub async fn set_wireless_enabled(
        &self,
        enabled: bool,
    ) -> Result<NetworkSnapshot, NetworkError> {
        timeout(Duration::from_secs(5), async {
            let manager =
                Proxy::new(&self.connection, SERVICE, MANAGER_PATH, MANAGER_INTERFACE).await?;
            manager
                .set_property("WirelessEnabled", &enabled)
                .await
                .map_err(|error| NetworkError::from(zbus::Error::FDO(Box::new(error))))?;
            loop {
                let snapshot = self.snapshot().await?;
                if snapshot.wireless_enabled == enabled {
                    return Ok(snapshot);
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .map_err(|_| NetworkError::Timeout)?
    }

    pub async fn deactivate_connection(
        &self,
        active_connection: &str,
    ) -> Result<NetworkSnapshot, NetworkError> {
        let active_path =
            zbus::zvariant::OwnedObjectPath::try_from(active_connection.to_owned())
                .map_err(|_| NetworkError::InvalidObjectPath(active_connection.to_owned()))?;
        timeout(Duration::from_secs(5), async {
            let manager =
                Proxy::new(&self.connection, SERVICE, MANAGER_PATH, MANAGER_INTERFACE).await?;
            manager
                .call::<_, _, ()>("DeactivateConnection", &(active_path.clone(),))
                .await?;
            loop {
                let snapshot = self.snapshot().await?;
                if !snapshot
                    .active_connections
                    .iter()
                    .any(|connection| connection.object_path == active_path.as_str())
                {
                    return Ok(snapshot);
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .map_err(|_| NetworkError::Timeout)?
    }

    pub async fn activate_connection(
        &self,
        connection: &str,
        device: &str,
        specific_object: &str,
    ) -> Result<NetworkSnapshot, NetworkError> {
        let parse_path = |path: &str| {
            zbus::zvariant::OwnedObjectPath::try_from(path.to_owned())
                .map_err(|_| NetworkError::InvalidObjectPath(path.to_owned()))
        };
        let connection_path = parse_path(connection)?;
        let device_path = parse_path(device)?;
        let specific_path = parse_path(specific_object)?;
        timeout(Duration::from_secs(5), async {
            let manager =
                Proxy::new(&self.connection, SERVICE, MANAGER_PATH, MANAGER_INTERFACE).await?;
            let active_path: zbus::zvariant::OwnedObjectPath = manager
                .call(
                    "ActivateConnection",
                    &(connection_path, device_path, specific_path),
                )
                .await?;
            loop {
                let snapshot = self.snapshot().await?;
                if snapshot
                    .active_connections
                    .iter()
                    .any(|connection| connection.object_path == active_path.as_str())
                {
                    return Ok(snapshot);
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .map_err(|_| NetworkError::Timeout)?
    }

    /// Join the Wi-Fi network `ssid` seen at `access_point` on `device`. A
    /// saved profile for it is reused; otherwise NetworkManager stores a
    /// new one, WPA-PSK when `password` is given and open without. Returns
    /// once NetworkManager accepted the request; progress and failure
    /// (a wrong password ends the activation) arrive through `watch`.
    pub async fn join_wifi(
        &self,
        device: &str,
        access_point: &str,
        ssid: &[u8],
        password: Option<&str>,
    ) -> Result<(), NetworkError> {
        let parse_path = |path: &str| {
            zbus::zvariant::OwnedObjectPath::try_from(path.to_owned())
                .map_err(|_| NetworkError::InvalidObjectPath(path.to_owned()))
        };
        let device_path = parse_path(device)?;
        let ap_path = parse_path(access_point)?;
        let name = String::from_utf8_lossy(ssid).into_owned();
        // An existing wireless profile with this name is joined as is.
        if password.is_none() {
            if let Some(profile) = self
                .saved_connections()
                .await?
                .into_iter()
                .find(|p| p.connection_type == "802-11-wireless" && p.id == name)
            {
                self.activate_connection(&profile.object_path, device, access_point)
                    .await?;
                return Ok(());
            }
        }
        let settings = wifi_settings(&name, ssid, password)?;
        timeout(Duration::from_secs(10), async {
            let manager =
                Proxy::new(&self.connection, SERVICE, MANAGER_PATH, MANAGER_INTERFACE).await?;
            let _: (
                zbus::zvariant::OwnedObjectPath,
                zbus::zvariant::OwnedObjectPath,
            ) = manager
                .call(
                    "AddAndActivateConnection",
                    &(settings, device_path, ap_path),
                )
                .await?;
            Ok(())
        })
        .await
        .map_err(|_| NetworkError::Timeout)?
    }

    /// Delete a saved connection profile (Forget This Network).
    pub async fn forget_connection(&self, profile: &str) -> Result<(), NetworkError> {
        let path = zbus::zvariant::OwnedObjectPath::try_from(profile.to_owned())
            .map_err(|_| NetworkError::InvalidObjectPath(profile.to_owned()))?;
        timeout(Duration::from_secs(5), async {
            let connection = Proxy::new(
                &self.connection,
                SERVICE,
                path.as_str(),
                SETTINGS_CONNECTION_INTERFACE,
            )
            .await?;
            connection.call::<_, _, ()>("Delete", &()).await?;
            Ok(())
        })
        .await
        .map_err(|_| NetworkError::Timeout)?
    }

    pub async fn activate_vpn_connection(
        &self,
        profile_path: &str,
    ) -> Result<NetworkSnapshot, NetworkError> {
        let profile_path = zbus::zvariant::OwnedObjectPath::try_from(profile_path.to_owned())
            .map_err(|_| NetworkError::InvalidObjectPath(profile_path.to_owned()))?;
        let profiles = self.saved_vpn_connections().await?;
        let profile = profiles
            .into_iter()
            .find(|profile| profile.object_path == profile_path.as_str())
            .ok_or_else(|| NetworkError::VpnProfileNotFound(profile_path.to_string()))?;

        let snapshot = self.snapshot().await?;
        let base_connection = snapshot
            .active_connections
            .iter()
            .find(|connection| connection.default_ipv4 && !connection.is_vpn)
            .or_else(|| {
                snapshot
                    .active_connections
                    .iter()
                    .find(|connection| connection.default_ipv6 && !connection.is_vpn)
            })
            .ok_or(NetworkError::NoVpnBaseConnection)?;

        let activated = self
            .activate_connection(profile_path.as_str(), "/", &base_connection.object_path)
            .await?;
        if !activated.active_connections.iter().any(|connection| {
            connection.is_vpn
                && connection.connection_type == "vpn"
                && connection.uuid == profile.uuid
        }) {
            return Err(NetworkError::InvalidProperty(
                "VPN activation did not read back an active VPN connection".into(),
            ));
        }
        Ok(activated)
    }

    pub fn watch(&self) -> NetworkEventStream {
        let client = self.clone();
        let (sender, receiver) = mpsc::channel(16);
        tokio::spawn(async move { client.watch_task(sender).await });
        NetworkEventStream { receiver }
    }

    async fn watch_task(self, sender: mpsc::Sender<Result<NetworkSnapshot, NetworkError>>) {
        let manager =
            match Proxy::new(&self.connection, SERVICE, MANAGER_PATH, MANAGER_INTERFACE).await {
                Ok(proxy) => proxy,
                Err(error) => {
                    let _ = sender.send(Err(error.into())).await;
                    return;
                }
            };
        let mut devices = manager
            .receive_property_changed::<Vec<zbus::zvariant::OwnedObjectPath>>("Devices")
            .await;
        let mut active_connections = manager
            .receive_property_changed::<Vec<zbus::zvariant::OwnedObjectPath>>("ActiveConnections")
            .await;
        let mut networking_enabled = manager
            .receive_property_changed::<bool>("NetworkingEnabled")
            .await;
        let mut wireless_enabled = manager
            .receive_property_changed::<bool>("WirelessEnabled")
            .await;
        let mut wireless_hardware_enabled = manager
            .receive_property_changed::<bool>("WirelessHardwareEnabled")
            .await;
        let mut owner_changes = match manager.receive_owner_changed().await {
            Ok(stream) => stream,
            Err(error) => {
                let _ = sender.send(Err(error.into())).await;
                return;
            }
        };
        let (change_sender, mut changes) = mpsc::channel(16);
        let mut device_watchers = JoinSet::new();
        if !self
            .refresh_topology(&sender, &mut device_watchers, &change_sender)
            .await
        {
            return;
        }
        loop {
            tokio::select! {
                _ = sender.closed() => break,
                event = devices.next() => {
                    if event.is_none() || !self.refresh_topology(&sender, &mut device_watchers, &change_sender).await { break; }
                }
                event = active_connections.next() => {
                    if event.is_none() || !self.refresh_topology(&sender, &mut device_watchers, &change_sender).await { break; }
                }
                event = networking_enabled.next() => {
                    if event.is_none() || !send_snapshot(&self, &sender).await { break; }
                }
                event = wireless_enabled.next() => {
                    if event.is_none() || !send_snapshot(&self, &sender).await { break; }
                }
                event = wireless_hardware_enabled.next() => {
                    if event.is_none() || !send_snapshot(&self, &sender).await { break; }
                }
                event = changes.recv() => {
                    if event.is_none() || !self.refresh_topology(&sender, &mut device_watchers, &change_sender).await { break; }
                }
                owner = owner_changes.next() => {
                    match owner {
                        Some(Some(_)) => {
                            if !self.refresh_topology(&sender, &mut device_watchers, &change_sender).await { break; }
                        }
                        Some(None) => if sender.send(Err(NetworkError::Disconnected)).await.is_err() { break; },
                        None => break,
                    }
                }
            }
        }
        device_watchers.abort_all();
        while device_watchers.join_next().await.is_some() {}
    }

    async fn refresh_topology(
        &self,
        sender: &mpsc::Sender<Result<NetworkSnapshot, NetworkError>>,
        watchers: &mut JoinSet<()>,
        change_sender: &mpsc::Sender<()>,
    ) -> bool {
        let mut snapshot = self.snapshot().await;
        for _ in 0..10 {
            if !snapshot
                .as_ref()
                .err()
                .is_some_and(|error| error.availability() == Availability::MissingService)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
            snapshot = self.snapshot().await;
        }
        if let Ok(snapshot) = &snapshot {
            replace_watchers(self, snapshot, watchers, change_sender).await;
        }
        sender.send(snapshot).await.is_ok()
    }

    async fn snapshot_inner(&self) -> Result<NetworkSnapshot, NetworkError> {
        let manager =
            Proxy::new(&self.connection, SERVICE, MANAGER_PATH, MANAGER_INTERFACE).await?;
        let networking_enabled = manager.get_property("NetworkingEnabled").await?;
        let wireless_enabled = manager.get_property("WirelessEnabled").await?;
        let wireless_hardware_enabled = manager.get_property("WirelessHardwareEnabled").await?;
        let paths: Vec<zbus::zvariant::OwnedObjectPath> = manager.get_property("Devices").await?;
        let active_paths: Vec<zbus::zvariant::OwnedObjectPath> =
            manager.get_property("ActiveConnections").await?;
        let mut devices = Vec::with_capacity(paths.len());
        for path in paths {
            let device =
                Proxy::new(&self.connection, SERVICE, path.as_str(), DEVICE_INTERFACE).await?;
            let kind = NetworkDeviceType::from_raw(device.get_property("DeviceType").await?);
            let (active_access_point, access_points) = if kind == NetworkDeviceType::Wifi {
                let wireless =
                    Proxy::new(&self.connection, SERVICE, path.as_str(), WIRELESS_INTERFACE)
                        .await?;
                let active: zbus::zvariant::OwnedObjectPath =
                    wireless.get_property("ActiveAccessPoint").await?;
                let active = optional_object_path(&active);
                let access_point_paths: Vec<zbus::zvariant::OwnedObjectPath> =
                    wireless.call("GetAllAccessPoints", &()).await?;
                let mut access_points = Vec::with_capacity(access_point_paths.len());
                for access_point_path in access_point_paths {
                    let access_point = Proxy::new(
                        &self.connection,
                        SERVICE,
                        access_point_path.as_str(),
                        ACCESS_POINT_INTERFACE,
                    )
                    .await?;
                    access_points.push(NetworkAccessPoint {
                        object_path: access_point_path.to_string(),
                        ssid: access_point.get_property("Ssid").await?,
                        strength: access_point.get_property("Strength").await?,
                        frequency_mhz: access_point.get_property("Frequency").await?,
                        wpa_flags: access_point.get_property("WpaFlags").await?,
                        rsn_flags: access_point.get_property("RsnFlags").await?,
                    });
                }
                (active, access_points)
            } else {
                (None, Vec::new())
            };
            let active_connection: zbus::zvariant::OwnedObjectPath =
                device.get_property("ActiveConnection").await?;
            devices.push(NetworkDevice {
                object_path: path.to_string(),
                interface: device.get_property("Interface").await?,
                ip_interface: device.get_property("IpInterface").await?,
                kind,
                state: NetworkDeviceState::from_raw(device.get_property("State").await?),
                active_access_point,
                access_points,
                active_connection: optional_object_path(&active_connection),
            });
        }
        let mut active_connections = Vec::with_capacity(active_paths.len());
        for path in active_paths {
            let active = Proxy::new(
                &self.connection,
                SERVICE,
                path.as_str(),
                ACTIVE_CONNECTION_INTERFACE,
            )
            .await?;
            let ipv4_path: zbus::zvariant::OwnedObjectPath =
                active.get_property("Ip4Config").await?;
            let ipv6_path: zbus::zvariant::OwnedObjectPath =
                active.get_property("Ip6Config").await?;
            active_connections.push(ActiveNetworkConnection {
                object_path: path.to_string(),
                id: active.get_property("Id").await?,
                uuid: active.get_property("Uuid").await?,
                connection_type: active.get_property("Type").await?,
                state: active.get_property("State").await?,
                default_ipv4: active.get_property("Default").await?,
                default_ipv6: active.get_property("Default6").await?,
                is_vpn: active.get_property("Vpn").await?,
                ipv4: read_ip_config(&self.connection, &ipv4_path, IP4_CONFIG_INTERFACE).await?,
                ipv6: read_ip_config(&self.connection, &ipv6_path, IP6_CONFIG_INTERFACE).await?,
            });
        }
        Ok(NetworkSnapshot {
            revision: 0,
            networking_enabled,
            wireless_enabled,
            wireless_hardware_enabled,
            devices,
            active_connections,
        })
    }
}

async fn send_snapshot(
    client: &NetworkManagerClient,
    sender: &mpsc::Sender<Result<NetworkSnapshot, NetworkError>>,
) -> bool {
    sender.send(client.snapshot().await).await.is_ok()
}

async fn replace_watchers(
    client: &NetworkManagerClient,
    snapshot: &NetworkSnapshot,
    watchers: &mut JoinSet<()>,
    changes: &mpsc::Sender<()>,
) {
    watchers.abort_all();
    while watchers.join_next().await.is_some() {}
    for device in &snapshot.devices {
        let (ready, registered) = oneshot::channel();
        watchers.spawn(watch_device(
            client.clone(),
            device.clone(),
            changes.clone(),
            ready,
        ));
        let _ = timeout(Duration::from_secs(2), registered).await;
        if device.kind == NetworkDeviceType::Wifi {
            let (ready, registered) = oneshot::channel();
            watchers.spawn(watch_access_points(
                client.clone(),
                device.object_path.clone(),
                changes.clone(),
                ready,
            ));
            let _ = timeout(Duration::from_secs(2), registered).await;
        }
    }
    for connection in &snapshot.active_connections {
        let (ready, registered) = oneshot::channel();
        watchers.spawn(watch_active_connection(
            client.clone(),
            connection.object_path.clone(),
            changes.clone(),
            ready,
        ));
        let _ = timeout(Duration::from_secs(2), registered).await;
    }
}

async fn watch_device(
    client: NetworkManagerClient,
    device: NetworkDevice,
    changes: mpsc::Sender<()>,
    ready: oneshot::Sender<()>,
) {
    let proxy = match Proxy::new(
        &client.connection,
        SERVICE,
        device.object_path.as_str(),
        DEVICE_INTERFACE,
    )
    .await
    {
        Ok(proxy) => proxy,
        Err(_) => {
            let _ = ready.send(());
            return;
        }
    };
    let mut states = proxy.receive_property_changed::<u32>("State").await;
    let mut active_connections = proxy
        .receive_property_changed::<zbus::zvariant::OwnedObjectPath>("ActiveConnection")
        .await;
    let _ = ready.send(());
    loop {
        tokio::select! {
            _ = changes.closed() => break,
            event = states.next() => {
                if event.is_none() || changes.send(()).await.is_err() { break; }
            }
            event = active_connections.next() => {
                if event.is_none() || changes.send(()).await.is_err() { break; }
            }
        }
    }
}

async fn watch_access_points(
    client: NetworkManagerClient,
    device_path: String,
    changes: mpsc::Sender<()>,
    ready: oneshot::Sender<()>,
) {
    let proxy = match Proxy::new(&client.connection, SERVICE, device_path, WIRELESS_INTERFACE).await
    {
        Ok(proxy) => proxy,
        Err(_) => {
            let _ = ready.send(());
            return;
        }
    };
    let mut added = match proxy.receive_signal("AccessPointAdded").await {
        Ok(stream) => stream,
        Err(_) => {
            let _ = ready.send(());
            return;
        }
    };
    let mut removed = match proxy.receive_signal("AccessPointRemoved").await {
        Ok(stream) => stream,
        Err(_) => {
            let _ = ready.send(());
            return;
        }
    };
    let _ = ready.send(());
    loop {
        tokio::select! {
            _ = changes.closed() => break,
            event = added.next() => {
                if event.is_none() || changes.send(()).await.is_err() { break; }
            }
            event = removed.next() => {
                if event.is_none() || changes.send(()).await.is_err() { break; }
            }
        }
    }
}

async fn watch_active_connection(
    client: NetworkManagerClient,
    path: String,
    changes: mpsc::Sender<()>,
    ready: oneshot::Sender<()>,
) {
    let proxy = match Proxy::new(
        &client.connection,
        SERVICE,
        path,
        ACTIVE_CONNECTION_INTERFACE,
    )
    .await
    {
        Ok(proxy) => proxy,
        Err(_) => {
            let _ = ready.send(());
            return;
        }
    };
    let mut states = proxy.receive_property_changed::<u32>("State").await;
    let mut ipv4 = proxy
        .receive_property_changed::<zbus::zvariant::OwnedObjectPath>("Ip4Config")
        .await;
    let mut ipv6 = proxy
        .receive_property_changed::<zbus::zvariant::OwnedObjectPath>("Ip6Config")
        .await;
    let _ = ready.send(());
    loop {
        tokio::select! {
            _ = changes.closed() => break,
            event = states.next() => {
                if event.is_none() || changes.send(()).await.is_err() { break; }
            }
            event = ipv4.next() => {
                if event.is_none() || changes.send(()).await.is_err() { break; }
            }
            event = ipv6.next() => {
                if event.is_none() || changes.send(()).await.is_err() { break; }
            }
        }
    }
}

async fn read_ip_config(
    connection: &Connection,
    path: &zbus::zvariant::OwnedObjectPath,
    interface: &str,
) -> Result<Option<NetworkIpConfig>, NetworkError> {
    if path.as_str() == "/" {
        return Ok(None);
    }
    let proxy = Proxy::new(connection, SERVICE, path.as_str(), interface).await?;
    let data: Vec<HashMap<String, zbus::zvariant::OwnedValue>> =
        proxy.get_property("AddressData").await?;
    let addresses = data
        .iter()
        .map(|item| {
            Ok(NetworkIpAddress {
                address: string_field(item, "address")?,
                prefix: u32_field(item, "prefix")?,
            })
        })
        .collect::<Result<_, NetworkError>>()?;
    let nameservers = if interface == IP6_CONFIG_INTERFACE {
        let raw: Vec<Vec<u8>> = proxy.get_property("Nameservers").await?;
        raw.iter()
            .map(|bytes| ipv6_string(bytes))
            .collect::<Result<_, _>>()?
    } else {
        let data: Vec<HashMap<String, zbus::zvariant::OwnedValue>> =
            proxy.get_property("NameserverData").await?;
        data.iter()
            .map(|item| string_field(item, "address"))
            .collect::<Result<_, _>>()?
    };
    let gateway: String = proxy.get_property("Gateway").await?;
    Ok(Some(NetworkIpConfig {
        addresses,
        gateway: (!gateway.is_empty()).then_some(gateway),
        nameservers,
    }))
}

fn ipv6_string(bytes: &[u8]) -> Result<String, NetworkError> {
    let octets: [u8; 16] = bytes.try_into().map_err(|_| {
        NetworkError::InvalidProperty("IPv6 nameserver must contain 16 bytes".into())
    })?;
    Ok(Ipv6Addr::from(octets).to_string())
}

fn string_field(
    fields: &HashMap<String, zbus::zvariant::OwnedValue>,
    name: &str,
) -> Result<String, NetworkError> {
    fields
        .get(name)
        .ok_or_else(|| NetworkError::InvalidProperty(format!("missing {name}")))?
        .try_clone()
        .and_then(String::try_from)
        .map_err(|error| NetworkError::InvalidProperty(format!("{name}: {error}")))
}

fn u32_field(
    fields: &HashMap<String, zbus::zvariant::OwnedValue>,
    name: &str,
) -> Result<u32, NetworkError> {
    fields
        .get(name)
        .ok_or_else(|| NetworkError::InvalidProperty(format!("missing {name}")))?
        .try_clone()
        .and_then(u32::try_from)
        .map_err(|error| NetworkError::InvalidProperty(format!("{name}: {error}")))
}

fn optional_object_path(path: &zbus::zvariant::OwnedObjectPath) -> Option<String> {
    (path.as_str() != "/").then(|| path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::BufRead,
        process::{Child, Command, Stdio},
        sync::atomic::{AtomicBool, AtomicU32},
    };

    #[test]
    fn network_manager_device_states_keep_known_and_unknown_values() {
        assert_eq!(
            NetworkDeviceState::from_raw(10),
            NetworkDeviceState::Unmanaged
        );
        assert_eq!(
            NetworkDeviceState::from_raw(30),
            NetworkDeviceState::Disconnected
        );
        assert_eq!(
            NetworkDeviceState::from_raw(100),
            NetworkDeviceState::Activated
        );
        assert_eq!(
            NetworkDeviceState::from_raw(120),
            NetworkDeviceState::Failed
        );
        assert_eq!(
            NetworkDeviceState::from_raw(130),
            NetworkDeviceState::Other(130)
        );
    }

    #[test]
    fn network_manager_device_types_keep_unknown_values() {
        assert_eq!(NetworkDeviceType::from_raw(1), NetworkDeviceType::Ethernet);
        assert_eq!(NetworkDeviceType::from_raw(2), NetworkDeviceType::Wifi);
        assert_eq!(NetworkDeviceType::from_raw(5), NetworkDeviceType::Bluetooth);
        assert_eq!(
            NetworkDeviceType::from_raw(31),
            NetworkDeviceType::Other(31)
        );
    }

    #[test]
    fn dbus_errors_map_to_truthful_availability() {
        assert_eq!(
            availability_from_dbus_error("org.freedesktop.DBus.Error.ServiceUnknown"),
            Availability::MissingService
        );
        assert_eq!(
            availability_from_dbus_error("org.freedesktop.DBus.Error.NameHasNoOwner"),
            Availability::MissingService
        );
        assert_eq!(
            availability_from_dbus_error("org.freedesktop.DBus.Error.AccessDenied"),
            Availability::PermissionDenied
        );
        assert_eq!(
            availability_from_dbus_error("org.freedesktop.DBus.Error.Failed"),
            Availability::Failed
        );
    }

    #[test]
    fn direct_dbus_access_denial_becomes_a_typed_permission_error() {
        let call = zbus::Message::method("/org/example", "Probe")
            .unwrap()
            .build(&())
            .unwrap();
        let message = zbus::Message::method_error(&call, "org.freedesktop.DBus.Error.AccessDenied")
            .unwrap()
            .build(&"denied")
            .unwrap();
        let error = zbus::Error::MethodError(
            "org.freedesktop.DBus.Error.AccessDenied"
                .try_into()
                .unwrap(),
            Some("denied".to_owned()),
            message,
        );
        let error = NetworkError::from(error);
        assert_eq!(error.availability(), Availability::PermissionDenied);
        assert!(matches!(error, NetworkError::PermissionDenied(_)));
    }

    #[test]
    fn empty_dbus_object_path_is_not_reported_as_an_active_access_point() {
        let absent = zbus::zvariant::OwnedObjectPath::try_from("/").unwrap();
        let present = zbus::zvariant::OwnedObjectPath::try_from("/org/example/ap").unwrap();
        assert_eq!(optional_object_path(&absent), None);
        assert_eq!(
            optional_object_path(&present),
            Some("/org/example/ap".to_owned())
        );
    }

    #[test]
    fn ip_property_decoding_preserves_address_prefix_and_rejects_missing_fields() {
        let properties = HashMap::from([
            (
                "address".to_owned(),
                zbus::zvariant::OwnedValue::from(zbus::zvariant::Str::from("192.0.2.14")),
            ),
            (
                "prefix".to_owned(),
                zbus::zvariant::OwnedValue::from(24_u32),
            ),
        ]);
        assert_eq!(string_field(&properties, "address").unwrap(), "192.0.2.14");
        assert_eq!(u32_field(&properties, "prefix").unwrap(), 24);
        assert!(matches!(
            string_field(&properties, "gateway"),
            Err(NetworkError::InvalidProperty(_))
        ));
    }

    #[test]
    fn ipv6_nameserver_decoding_checks_address_width() {
        assert_eq!(ipv6_string(&[0; 16]).unwrap(), "::");
        assert!(matches!(
            ipv6_string(&[0; 15]),
            Err(NetworkError::InvalidProperty(_))
        ));
    }

    #[tokio::test]
    async fn fixture_bus_reports_available_networking_without_adapters() {
        let mut daemon = BusDaemon(
            Command::new("dbus-daemon")
                .args(["--session", "--nofork", "--print-address=1"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .expect("dbus-daemon must be installed for provider fixture tests"),
        );
        let mut address = String::new();
        std::io::BufReader::new(daemon.0.stdout.as_mut().unwrap())
            .read_line(&mut address)
            .expect("private bus must provide its address");
        let devices = Arc::new(tokio::sync::RwLock::new(Vec::new()));
        let networking_enabled = Arc::new(AtomicBool::new(true));
        let wireless_state = Arc::new(AtomicBool::new(false));
        let device_error = Arc::new(AtomicU32::new(0));
        let device_path = owned_path("/org/freedesktop/NetworkManager/Devices/2");
        let service = zbus::connection::Builder::address(address.trim())
            .unwrap()
            .name(SERVICE)
            .unwrap()
            .serve_at(
                MANAGER_PATH,
                NetworkManagerFixture {
                    devices: devices.clone(),
                    active_connections: Arc::new(tokio::sync::RwLock::new(Vec::new())),
                    activation_path: None,
                    vpn_activation_path: None,
                    networking_enabled: networking_enabled.clone(),
                    wireless_enabled: wireless_state.clone(),
                    device_error: device_error.clone(),
                },
            )
            .unwrap()
            .serve_at(
                device_path.as_str(),
                DeviceFixture {
                    active_connection: owned_path("/"),
                    interface: "eth0".to_owned(),
                    device_type: 1,
                    state: Arc::new(AtomicU32::new(30)),
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
        let client = NetworkManagerClient {
            connection: client_connection,
            revision: Arc::new(AtomicU64::new(0)),
        };

        let mut updates = client.watch();
        let initial = tokio::time::timeout(Duration::from_secs(2), updates.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(initial.revision, 1);
        assert!(initial.networking_enabled);
        assert!(!initial.wireless_enabled);
        assert!(!initial.wireless_hardware_enabled);
        assert!(initial.devices.is_empty());
        assert!(initial.active_connections.is_empty());

        let wireless_on = client.set_wireless_enabled(true).await.unwrap();
        assert!(wireless_on.wireless_enabled);
        let enabled_event = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let update = updates.next().await.unwrap().unwrap();
                if update.wireless_enabled {
                    break update;
                }
            }
        })
        .await
        .unwrap();
        assert!(enabled_event.wireless_enabled);
        let wireless_disabled = client.set_wireless_enabled(false).await.unwrap();
        assert!(!wireless_disabled.wireless_enabled);
        let disabled_event = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let update = updates.next().await.unwrap().unwrap();
                if !update.wireless_enabled {
                    break update;
                }
            }
        })
        .await
        .unwrap();
        assert!(!disabled_event.wireless_enabled);

        let manager = Proxy::new(&client.connection, SERVICE, MANAGER_PATH, MANAGER_INTERFACE)
            .await
            .unwrap();
        manager
            .call::<_, _, ()>("SetDevices", &(vec![device_path.clone()],))
            .await
            .unwrap();
        let added = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let update = updates.next().await.unwrap().unwrap();
                if update.devices.len() == 1 {
                    break update;
                }
            }
        })
        .await
        .unwrap();
        assert!(added.revision > initial.revision);
        assert_eq!(added.devices.len(), 1);
        assert_eq!(added.devices[0].interface, "eth0");
        assert_eq!(added.devices[0].state, NetworkDeviceState::Disconnected);

        manager
            .call::<_, _, ()>(
                "SetDevices",
                &(Vec::<zbus::zvariant::OwnedObjectPath>::new(),),
            )
            .await
            .unwrap();
        let removed = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let update = updates.next().await.unwrap().unwrap();
                if update.devices.is_empty() {
                    break update;
                }
            }
        })
        .await
        .unwrap();
        assert!(removed.revision > added.revision);
        assert!(removed.devices.is_empty());

        manager
            .call::<_, _, ()>("SetDeviceError", &(1_u32,))
            .await
            .unwrap();
        let denied = client.snapshot().await.unwrap_err();
        assert_eq!(denied.availability(), Availability::PermissionDenied);
        assert!(matches!(denied, NetworkError::PermissionDenied(_)));
        let denied_mutation = client.set_networking_enabled(false).await.unwrap_err();
        assert_eq!(
            denied_mutation.availability(),
            Availability::PermissionDenied
        );
        assert!(matches!(denied_mutation, NetworkError::PermissionDenied(_)));
        let denied_wireless = client.set_wireless_enabled(true).await.unwrap_err();
        assert_eq!(
            denied_wireless.availability(),
            Availability::PermissionDenied,
            "{denied_wireless:?}"
        );
        assert!(matches!(denied_wireless, NetworkError::PermissionDenied(_)));

        manager
            .call::<_, _, ()>("SetDeviceError", &(2_u32,))
            .await
            .unwrap();
        let failed = client.snapshot().await.unwrap_err();
        assert_eq!(failed.availability(), Availability::Failed);

        manager
            .call::<_, _, ()>("SetDeviceError", &(0_u32,))
            .await
            .unwrap();
        let restored = client.snapshot().await.unwrap();
        assert!(restored.revision > removed.revision);
        assert!(!restored.wireless_enabled);

        manager
            .call::<_, _, ()>("SetDeviceError", &(3_u32,))
            .await
            .unwrap();
        assert!(matches!(
            client.set_networking_enabled(false).await,
            Err(NetworkError::Timeout)
        ));
        assert!(matches!(
            client.set_wireless_enabled(true).await,
            Err(NetworkError::Timeout)
        ));
        assert!(client.snapshot().await.unwrap().networking_enabled);
        manager
            .call::<_, _, ()>("SetDeviceError", &(4_u32,))
            .await
            .unwrap();
        let cancel_client = client.clone();
        let pending_mutation =
            tokio::spawn(async move { cancel_client.set_networking_enabled(false).await });
        tokio::time::sleep(Duration::from_millis(20)).await;
        pending_mutation.abort();
        assert!(pending_mutation.await.unwrap_err().is_cancelled());
        manager
            .call::<_, _, ()>("SetDeviceError", &(0_u32,))
            .await
            .unwrap();

        let disabled = client.set_networking_enabled(false).await.unwrap();
        assert!(!disabled.networking_enabled);

        drop(service);
        loop {
            let update = tokio::time::timeout(Duration::from_secs(2), updates.next())
                .await
                .expect("service owner loss must be reported")
                .expect("watcher remains open");
            if matches!(update, Err(NetworkError::Disconnected)) {
                break;
            }
        }
        let replacement = zbus::connection::Builder::address(address.trim())
            .unwrap()
            .name(SERVICE)
            .unwrap()
            .serve_at(
                MANAGER_PATH,
                NetworkManagerFixture {
                    devices: devices.clone(),
                    active_connections: Arc::new(tokio::sync::RwLock::new(Vec::new())),
                    activation_path: None,
                    vpn_activation_path: None,
                    networking_enabled,
                    wireless_enabled: wireless_state,
                    device_error,
                },
            )
            .unwrap()
            .serve_at(
                device_path.as_str(),
                DeviceFixture {
                    active_connection: owned_path("/"),
                    interface: "eth0".to_owned(),
                    device_type: 1,
                    state: Arc::new(AtomicU32::new(30)),
                },
            )
            .unwrap()
            .build()
            .await
            .unwrap();
        let recovered = loop {
            match tokio::time::timeout(Duration::from_secs(2), updates.next())
                .await
                .expect("new service owner must be observed")
                .expect("watcher remains open")
            {
                Ok(snapshot) => break snapshot,
                Err(NetworkError::Disconnected) => continue,
                Err(error) if error.availability() == Availability::MissingService => continue,
                Err(error) => panic!("unexpected watcher error: {error}"),
            }
        };
        assert!(recovered.revision > disabled.revision);
        assert!(!recovered.networking_enabled);

        drop(client);
        drop(replacement);
    }

    #[tokio::test]
    async fn fixture_bus_reads_wifi_connection_and_ip_state() {
        let mut daemon = BusDaemon(
            Command::new("dbus-daemon")
                .args(["--session", "--nofork", "--print-address=1"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .expect("dbus-daemon must be installed for provider fixture tests"),
        );
        let mut address = String::new();
        std::io::BufReader::new(daemon.0.stdout.as_mut().unwrap())
            .read_line(&mut address)
            .expect("private bus must provide its address");

        let device_path = owned_path("/org/freedesktop/NetworkManager/Devices/1");
        let ethernet_path = owned_path("/org/freedesktop/NetworkManager/Devices/2");
        let scan_calls = Arc::new(AtomicU64::new(0));
        let scan_error = Arc::new(AtomicU32::new(0));
        let access_point_path = owned_path("/org/freedesktop/NetworkManager/AccessPoint/1");
        let active_path = owned_path("/org/freedesktop/NetworkManager/ActiveConnection/1");
        let next_active_path = owned_path("/org/freedesktop/NetworkManager/ActiveConnection/2");
        let vpn_active_path = owned_path("/org/freedesktop/NetworkManager/ActiveConnection/3");
        let ipv4_path = owned_path("/org/freedesktop/NetworkManager/IP4Config/1");
        let ipv6_path = owned_path("/org/freedesktop/NetworkManager/IP6Config/1");
        let service = zbus::connection::Builder::address(address.trim())
            .unwrap()
            .name(SERVICE)
            .unwrap()
            .serve_at(
                MANAGER_PATH,
                NetworkManagerFixture {
                    devices: Arc::new(tokio::sync::RwLock::new(vec![device_path.clone()])),
                    active_connections: Arc::new(tokio::sync::RwLock::new(vec![
                        active_path.clone()
                    ])),
                    activation_path: Some(next_active_path.clone()),
                    vpn_activation_path: Some(vpn_active_path.clone()),
                    networking_enabled: Arc::new(AtomicBool::new(true)),
                    wireless_enabled: Arc::new(AtomicBool::new(false)),
                    device_error: Arc::new(AtomicU32::new(0)),
                },
            )
            .unwrap()
            .serve_at(
                SETTINGS_PATH,
                SettingsFixture(vec![
                    owned_path("/org/freedesktop/NetworkManager/Settings/1"),
                    owned_path("/org/freedesktop/NetworkManager/Settings/2"),
                ]),
            )
            .unwrap()
            .serve_at(
                "/org/freedesktop/NetworkManager/Settings/1",
                SettingsConnectionFixture::new("Lab Wi-Fi", "802-11-wireless", "wifi-uuid"),
            )
            .unwrap()
            .serve_at(
                "/org/freedesktop/NetworkManager/Settings/2",
                SettingsConnectionFixture::new(
                    "Work VPN",
                    "vpn",
                    "00000000-0000-0000-0000-000000000002",
                ),
            )
            .unwrap()
            .serve_at(
                device_path.as_str(),
                DeviceFixture {
                    active_connection: active_path.clone(),
                    interface: "wlan0".to_owned(),
                    device_type: 2,
                    state: Arc::new(AtomicU32::new(100)),
                },
            )
            .unwrap()
            .serve_at(
                device_path.as_str(),
                WirelessFixture {
                    active_access_point: access_point_path.clone(),
                    access_points: Arc::new(tokio::sync::RwLock::new(vec![
                        access_point_path.clone()
                    ])),
                    scan_calls: scan_calls.clone(),
                    scan_error: scan_error.clone(),
                },
            )
            .unwrap()
            .serve_at(
                ethernet_path.as_str(),
                DeviceFixture {
                    active_connection: owned_path("/"),
                    interface: "eth0".to_owned(),
                    device_type: 1,
                    state: Arc::new(AtomicU32::new(30)),
                },
            )
            .unwrap()
            .serve_at(access_point_path.as_str(), AccessPointFixture)
            .unwrap()
            .serve_at(
                "/org/freedesktop/NetworkManager/AccessPoint/2",
                AccessPointFixture,
            )
            .unwrap()
            .serve_at(
                active_path.as_str(),
                ActiveConnectionFixture {
                    ipv4: ipv4_path.clone(),
                    ipv6: ipv6_path.clone(),
                    is_vpn: false,
                },
            )
            .unwrap()
            .serve_at(
                next_active_path.as_str(),
                ActiveConnectionFixture {
                    ipv4: ipv4_path.clone(),
                    ipv6: ipv6_path.clone(),
                    is_vpn: false,
                },
            )
            .unwrap()
            .serve_at(
                vpn_active_path.as_str(),
                ActiveConnectionFixture {
                    ipv4: owned_path("/"),
                    ipv6: owned_path("/"),
                    is_vpn: true,
                },
            )
            .unwrap()
            .serve_at(ipv4_path.as_str(), Ipv4Fixture)
            .unwrap()
            .serve_at(ipv6_path.as_str(), Ipv6Fixture)
            .unwrap()
            .build()
            .await
            .unwrap();
        let client_connection = zbus::connection::Builder::address(address.trim())
            .unwrap()
            .build()
            .await
            .unwrap();
        let client = NetworkManagerClient {
            connection: client_connection,
            revision: Arc::new(AtomicU64::new(0)),
        };

        let snapshot = client.snapshot().await.unwrap();
        assert_eq!(snapshot.devices.len(), 1);
        assert_eq!(snapshot.devices[0].kind, NetworkDeviceType::Wifi);
        assert_eq!(snapshot.devices[0].state, NetworkDeviceState::Activated);
        assert_eq!(snapshot.devices[0].access_points[0].ssid, b"Lab Wi-Fi");
        assert_eq!(snapshot.devices[0].access_points[0].strength, 83);
        client
            .request_wifi_scan(device_path.as_str())
            .await
            .unwrap();
        assert_eq!(scan_calls.load(Ordering::Relaxed), 1);
        assert!(matches!(
            client.request_wifi_scan("not/a/path").await,
            Err(NetworkError::InvalidObjectPath(_))
        ));
        assert!(matches!(
            client
                .request_wifi_scan("/org/freedesktop/NetworkManager/Devices/404")
                .await,
            Err(NetworkError::WirelessDeviceNotFound(_))
        ));
        scan_error.store(1, Ordering::Relaxed);
        assert!(matches!(
            client.request_wifi_scan(device_path.as_str()).await,
            Err(NetworkError::PermissionDenied(_))
        ));
        scan_error.store(0, Ordering::Relaxed);
        let manager = Proxy::new(&client.connection, SERVICE, MANAGER_PATH, MANAGER_INTERFACE)
            .await
            .unwrap();
        manager
            .call::<_, _, ()>(
                "SetDevices",
                &(vec![device_path.clone(), ethernet_path.clone()],),
            )
            .await
            .unwrap();
        assert!(matches!(
            client.request_wifi_scan(ethernet_path.as_str()).await,
            Err(NetworkError::WirelessDeviceNotFound(_))
        ));
        assert_eq!(
            snapshot.devices[0].active_connection.as_deref(),
            Some(active_path.as_str())
        );
        assert_eq!(snapshot.active_connections.len(), 1);
        assert_eq!(snapshot.active_connections[0].id, "Lab Wi-Fi");
        assert_eq!(
            snapshot.active_connections[0]
                .ipv4
                .as_ref()
                .unwrap()
                .addresses[0]
                .address,
            "192.0.2.20"
        );
        assert_eq!(
            snapshot.active_connections[0]
                .ipv4
                .as_ref()
                .unwrap()
                .gateway
                .as_deref(),
            Some("192.0.2.1")
        );
        assert_eq!(
            snapshot.active_connections[0]
                .ipv4
                .as_ref()
                .unwrap()
                .nameservers,
            ["192.0.2.53"]
        );
        assert_eq!(
            snapshot.active_connections[0]
                .ipv6
                .as_ref()
                .unwrap()
                .addresses[0]
                .prefix,
            64
        );
        assert_eq!(
            snapshot.active_connections[0]
                .ipv6
                .as_ref()
                .unwrap()
                .nameservers,
            ["2001:db8::53"]
        );

        let mut updates = client.watch();
        let initial = tokio::time::timeout(Duration::from_secs(2), updates.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(initial.devices[0].access_points.len(), 1);
        let wireless = Proxy::new(
            &client.connection,
            SERVICE,
            device_path.as_str(),
            WIRELESS_INTERFACE,
        )
        .await
        .unwrap();
        wireless
            .call::<_, _, ()>(
                "AddAccessPoint",
                &(owned_path("/org/freedesktop/NetworkManager/AccessPoint/2"),),
            )
            .await
            .unwrap();
        let with_second_ap = loop {
            let update = tokio::time::timeout(Duration::from_secs(2), updates.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            if update
                .devices
                .first()
                .is_some_and(|device| device.access_points.len() == 2)
            {
                break update;
            }
        };
        assert!(with_second_ap.revision > initial.revision);
        wireless
            .call::<_, _, ()>(
                "RemoveAccessPoint",
                &(owned_path("/org/freedesktop/NetworkManager/AccessPoint/2"),),
            )
            .await
            .unwrap();
        let without_second_ap = loop {
            let update = tokio::time::timeout(Duration::from_secs(2), updates.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            if update
                .devices
                .first()
                .is_some_and(|device| device.access_points.len() == 1)
            {
                break update;
            }
        };
        assert!(without_second_ap.revision > with_second_ap.revision);
        let device = Proxy::new(
            &client.connection,
            SERVICE,
            device_path.as_str(),
            DEVICE_INTERFACE,
        )
        .await
        .unwrap();
        device
            .call::<_, _, ()>("ChangeState", &(120_u32,))
            .await
            .unwrap();
        let failed = loop {
            let update = tokio::time::timeout(Duration::from_secs(2), updates.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            if update.devices[0].state == NetworkDeviceState::Failed {
                break update;
            }
        };
        assert!(failed.revision > initial.revision);

        let activated = client
            .activate_connection(
                "/org/freedesktop/NetworkManager/Settings/1",
                device_path.as_str(),
                access_point_path.as_str(),
            )
            .await
            .unwrap();
        assert!(activated
            .active_connections
            .iter()
            .any(|connection| connection.object_path == next_active_path.as_str()));

        let invalid_path = client
            .deactivate_connection("not-an-object-path")
            .await
            .unwrap_err();
        assert!(matches!(invalid_path, NetworkError::InvalidObjectPath(_)));
        let unknown_connection = client
            .deactivate_connection("/org/freedesktop/NetworkManager/ActiveConnection/99")
            .await
            .unwrap_err();
        assert_eq!(unknown_connection.availability(), Availability::Failed);
        let disconnected = client
            .deactivate_connection(next_active_path.as_str())
            .await
            .unwrap();
        assert_eq!(disconnected.active_connections.len(), 1);
        let disconnected = client
            .deactivate_connection(active_path.as_str())
            .await
            .unwrap();
        assert!(disconnected.active_connections.is_empty());

        assert!(matches!(
            client.activate_vpn_connection("invalid-path").await,
            Err(NetworkError::InvalidObjectPath(_))
        ));
        assert!(matches!(
            client
                .activate_vpn_connection("/org/freedesktop/NetworkManager/Settings/99")
                .await,
            Err(NetworkError::VpnProfileNotFound(_))
        ));
        assert!(matches!(
            client
                .activate_vpn_connection("/org/freedesktop/NetworkManager/Settings/2")
                .await,
            Err(NetworkError::NoVpnBaseConnection)
        ));
        client
            .activate_connection(
                "/org/freedesktop/NetworkManager/Settings/1",
                device_path.as_str(),
                access_point_path.as_str(),
            )
            .await
            .unwrap();
        let manager = Proxy::new(&client.connection, SERVICE, MANAGER_PATH, MANAGER_INTERFACE)
            .await
            .unwrap();
        manager
            .call::<_, _, ()>(
                "SetActiveConnections",
                &(vec![vpn_active_path.clone(), next_active_path.clone()],),
            )
            .await
            .unwrap();
        let vpn_snapshot = client
            .activate_vpn_connection("/org/freedesktop/NetworkManager/Settings/2")
            .await
            .unwrap();
        assert!(vpn_snapshot.active_connections.iter().any(|connection| {
            connection.object_path == vpn_active_path.as_str()
                && connection.connection_type == "vpn"
                && connection.is_vpn
        }));

        drop(client);
        drop(service);
    }

    struct BusDaemon(Child);

    impl Drop for BusDaemon {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn owned_path(path: &str) -> zbus::zvariant::OwnedObjectPath {
        zbus::zvariant::OwnedObjectPath::try_from(path).unwrap()
    }

    #[tokio::test]
    async fn saved_vpn_query_lists_vpn_profiles_without_requesting_secrets() {
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
        let vpn_path = owned_path("/org/freedesktop/NetworkManager/Settings/1");
        let wifi_path = owned_path("/org/freedesktop/NetworkManager/Settings/2");
        let service = zbus::connection::Builder::address(address.trim())
            .unwrap()
            .name(SERVICE)
            .unwrap()
            .serve_at(
                SETTINGS_PATH,
                SettingsFixture(vec![vpn_path.clone(), wifi_path.clone()]),
            )
            .unwrap()
            .serve_at(
                vpn_path.as_str(),
                SettingsConnectionFixture::new("Work VPN", "vpn", "vpn-uuid"),
            )
            .unwrap()
            .serve_at(
                wifi_path.as_str(),
                SettingsConnectionFixture::new("Office Wi-Fi", "802-11-wireless", "wifi-uuid"),
            )
            .unwrap()
            .build()
            .await
            .unwrap();
        let client = NetworkManagerClient {
            connection: zbus::connection::Builder::address(address.trim())
                .unwrap()
                .build()
                .await
                .unwrap(),
            revision: Arc::new(AtomicU64::new(0)),
        };

        assert_eq!(
            client.saved_vpn_connections().await.unwrap(),
            vec![SavedVpnConnection {
                object_path: vpn_path.to_string(),
                id: "Work VPN".into(),
                uuid: "vpn-uuid".into(),
            }]
        );
        assert_eq!(
            client.saved_connections().await.unwrap(),
            vec![
                SavedConnection {
                    object_path: vpn_path.to_string(),
                    id: "Work VPN".into(),
                    uuid: "vpn-uuid".into(),
                    connection_type: "vpn".into(),
                },
                SavedConnection {
                    object_path: wifi_path.to_string(),
                    id: "Office Wi-Fi".into(),
                    uuid: "wifi-uuid".into(),
                    connection_type: "802-11-wireless".into(),
                },
            ]
        );
        drop(client);
        drop(service);
    }

    struct SettingsFixture(Vec<zbus::zvariant::OwnedObjectPath>);

    #[zbus::interface(name = "org.freedesktop.NetworkManager.Settings")]
    impl SettingsFixture {
        fn list_connections(&self) -> Vec<zbus::zvariant::OwnedObjectPath> {
            self.0.clone()
        }
    }

    struct SettingsConnectionFixture(String, String, String);

    impl SettingsConnectionFixture {
        fn new(id: &str, kind: &str, uuid: &str) -> Self {
            Self(id.into(), kind.into(), uuid.into())
        }
    }

    #[zbus::interface(name = "org.freedesktop.NetworkManager.Settings.Connection")]
    impl SettingsConnectionFixture {
        fn get_settings(&self) -> HashMap<String, HashMap<String, zbus::zvariant::OwnedValue>> {
            let connection = HashMap::from([
                (
                    "id".into(),
                    zbus::zvariant::OwnedValue::from(zbus::zvariant::Str::from(self.0.as_str())),
                ),
                (
                    "type".into(),
                    zbus::zvariant::OwnedValue::from(zbus::zvariant::Str::from(self.1.as_str())),
                ),
                (
                    "uuid".into(),
                    zbus::zvariant::OwnedValue::from(zbus::zvariant::Str::from(self.2.as_str())),
                ),
            ]);
            HashMap::from([("connection".into(), connection)])
        }
    }

    struct NetworkManagerFixture {
        devices: Arc<tokio::sync::RwLock<Vec<zbus::zvariant::OwnedObjectPath>>>,
        active_connections: Arc<tokio::sync::RwLock<Vec<zbus::zvariant::OwnedObjectPath>>>,
        activation_path: Option<zbus::zvariant::OwnedObjectPath>,
        vpn_activation_path: Option<zbus::zvariant::OwnedObjectPath>,
        networking_enabled: Arc<AtomicBool>,
        wireless_enabled: Arc<AtomicBool>,
        device_error: Arc<AtomicU32>,
    }

    #[zbus::interface(name = "org.freedesktop.NetworkManager")]
    impl NetworkManagerFixture {
        #[zbus(property)]
        fn networking_enabled(&self) -> bool {
            self.networking_enabled.load(Ordering::Relaxed)
        }

        #[zbus(property)]
        fn wireless_enabled(&self) -> bool {
            self.wireless_enabled.load(Ordering::Relaxed)
        }

        #[zbus(property)]
        async fn set_wireless_enabled(&self, enabled: bool) -> zbus::Result<()> {
            match self.device_error.load(Ordering::Relaxed) {
                1 => {
                    return Err(zbus::Error::FDO(Box::new(zbus::fdo::Error::AccessDenied(
                        "fixture denied".into(),
                    ))));
                }
                3 => return Ok(()),
                4 => tokio::time::sleep(Duration::from_millis(250)).await,
                _ => {}
            }
            self.wireless_enabled.store(enabled, Ordering::Relaxed);
            Ok(())
        }

        #[zbus(property)]
        fn wireless_hardware_enabled(&self) -> bool {
            false
        }

        #[zbus(property)]
        async fn devices(&self) -> zbus::fdo::Result<Vec<zbus::zvariant::OwnedObjectPath>> {
            match self.device_error.load(Ordering::Relaxed) {
                1 => Err(zbus::fdo::Error::AccessDenied("fixture denied".into())),
                2 => Err(zbus::fdo::Error::Failed("fixture failed".into())),
                _ => Ok(self.devices.read().await.clone()),
            }
        }

        #[zbus(property)]
        async fn active_connections(&self) -> Vec<zbus::zvariant::OwnedObjectPath> {
            self.active_connections.read().await.clone()
        }

        async fn deactivate_connection(
            &self,
            path: zbus::zvariant::OwnedObjectPath,
            #[zbus(signal_context)] emitter: zbus::object_server::SignalContext<'_>,
        ) -> zbus::fdo::Result<()> {
            let mut connections = self.active_connections.write().await;
            let previous_length = connections.len();
            connections.retain(|connection| connection != &path);
            if connections.len() == previous_length {
                return Err(zbus::fdo::Error::Failed("unknown active connection".into()));
            }
            drop(connections);
            self.active_connections_changed(&emitter)
                .await
                .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))
        }

        async fn set_active_connections(
            &self,
            connections: Vec<zbus::zvariant::OwnedObjectPath>,
            #[zbus(signal_context)] emitter: zbus::object_server::SignalContext<'_>,
        ) -> zbus::fdo::Result<()> {
            *self.active_connections.write().await = connections;
            self.active_connections_changed(&emitter)
                .await
                .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))
        }

        async fn activate_connection(
            &self,
            connection: zbus::zvariant::OwnedObjectPath,
            device: zbus::zvariant::OwnedObjectPath,
            specific_object: zbus::zvariant::OwnedObjectPath,
            #[zbus(signal_context)] emitter: zbus::object_server::SignalContext<'_>,
        ) -> zbus::fdo::Result<zbus::zvariant::OwnedObjectPath> {
            let path = match (
                connection.as_str(),
                device.as_str(),
                specific_object.as_str(),
            ) {
                (
                    "/org/freedesktop/NetworkManager/Settings/1",
                    "/org/freedesktop/NetworkManager/Devices/1",
                    "/org/freedesktop/NetworkManager/AccessPoint/1",
                ) => self.activation_path.clone(),
                (
                    "/org/freedesktop/NetworkManager/Settings/2",
                    "/",
                    "/org/freedesktop/NetworkManager/ActiveConnection/2",
                ) => self.vpn_activation_path.clone(),
                _ => {
                    return Err(zbus::fdo::Error::Failed(format!(
                        "unexpected activation target {connection}, {device}, {specific_object}"
                    )));
                }
            }
            .ok_or_else(|| zbus::fdo::Error::Failed("activation unavailable".into()))?;
            self.active_connections.write().await.push(path.clone());
            self.active_connections_changed(&emitter)
                .await
                .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))?;
            Ok(path)
        }

        async fn set_devices(
            &self,
            devices: Vec<zbus::zvariant::OwnedObjectPath>,
            #[zbus(signal_context)] emitter: zbus::object_server::SignalContext<'_>,
        ) -> zbus::fdo::Result<()> {
            *self.devices.write().await = devices;
            self.devices_changed(&emitter)
                .await
                .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))
        }

        async fn enable(
            &self,
            enabled: bool,
            #[zbus(signal_context)] emitter: zbus::object_server::SignalContext<'_>,
        ) -> zbus::fdo::Result<()> {
            match self.device_error.load(Ordering::Relaxed) {
                1 => return Err(zbus::fdo::Error::AccessDenied("fixture denied".into())),
                3 => return Ok(()),
                4 => tokio::time::sleep(Duration::from_millis(250)).await,
                _ => {}
            }
            self.networking_enabled.store(enabled, Ordering::Relaxed);
            self.networking_enabled_changed(&emitter)
                .await
                .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))
        }

        fn set_device_error(&self, error: u32) -> zbus::fdo::Result<()> {
            self.device_error.store(error, Ordering::Relaxed);
            Ok(())
        }
    }

    struct DeviceFixture {
        active_connection: zbus::zvariant::OwnedObjectPath,
        interface: String,
        device_type: u32,
        state: Arc<AtomicU32>,
    }

    #[zbus::interface(name = "org.freedesktop.NetworkManager.Device")]
    impl DeviceFixture {
        #[zbus(property)]
        fn interface(&self) -> &str {
            &self.interface
        }
        #[zbus(property)]
        fn ip_interface(&self) -> &str {
            &self.interface
        }
        #[zbus(property)]
        fn device_type(&self) -> u32 {
            self.device_type
        }
        #[zbus(property)]
        fn state(&self) -> u32 {
            self.state.load(Ordering::Relaxed)
        }
        #[zbus(property)]
        fn active_connection(&self) -> zbus::zvariant::OwnedObjectPath {
            self.active_connection.clone()
        }

        async fn change_state(
            &self,
            state: u32,
            #[zbus(signal_context)] context: zbus::object_server::SignalContext<'_>,
        ) -> zbus::fdo::Result<()> {
            self.state.store(state, Ordering::Relaxed);
            self.state_changed(&context)
                .await
                .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))
        }
    }

    struct WirelessFixture {
        active_access_point: zbus::zvariant::OwnedObjectPath,
        access_points: Arc<tokio::sync::RwLock<Vec<zbus::zvariant::OwnedObjectPath>>>,
        scan_calls: Arc<AtomicU64>,
        scan_error: Arc<AtomicU32>,
    }

    #[zbus::interface(name = "org.freedesktop.NetworkManager.Device.Wireless")]
    impl WirelessFixture {
        async fn request_scan(
            &self,
            options: HashMap<String, zbus::zvariant::OwnedValue>,
        ) -> zbus::fdo::Result<()> {
            assert!(options.is_empty());
            self.scan_calls.fetch_add(1, Ordering::Relaxed);
            if self.scan_error.load(Ordering::Relaxed) == 1 {
                return Err(zbus::fdo::Error::AccessDenied("fixture denied".into()));
            }
            Ok(())
        }
        #[zbus(property)]
        fn active_access_point(&self) -> zbus::zvariant::OwnedObjectPath {
            self.active_access_point.clone()
        }
        async fn get_all_access_points(&self) -> Vec<zbus::zvariant::OwnedObjectPath> {
            self.access_points.read().await.clone()
        }
        async fn add_access_point(
            &self,
            access_point: zbus::zvariant::OwnedObjectPath,
            #[zbus(signal_context)] context: zbus::object_server::SignalContext<'_>,
        ) -> zbus::fdo::Result<()> {
            self.access_points.write().await.push(access_point.clone());
            Self::access_point_added(&context, &access_point)
                .await
                .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))
        }
        async fn remove_access_point(
            &self,
            access_point: zbus::zvariant::OwnedObjectPath,
            #[zbus(signal_context)] context: zbus::object_server::SignalContext<'_>,
        ) -> zbus::fdo::Result<()> {
            self.access_points
                .write()
                .await
                .retain(|current| current != &access_point);
            Self::access_point_removed(&context, &access_point)
                .await
                .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))
        }
        #[zbus(signal)]
        async fn access_point_added(
            context: &zbus::object_server::SignalContext<'_>,
            access_point: &zbus::zvariant::ObjectPath<'_>,
        ) -> zbus::Result<()>;
        #[zbus(signal)]
        async fn access_point_removed(
            context: &zbus::object_server::SignalContext<'_>,
            access_point: &zbus::zvariant::ObjectPath<'_>,
        ) -> zbus::Result<()>;
    }

    struct AccessPointFixture;

    #[zbus::interface(name = "org.freedesktop.NetworkManager.AccessPoint")]
    impl AccessPointFixture {
        #[zbus(property)]
        fn ssid(&self) -> Vec<u8> {
            b"Lab Wi-Fi".to_vec()
        }
        #[zbus(property)]
        fn strength(&self) -> u8 {
            83
        }
        #[zbus(property)]
        fn frequency(&self) -> u32 {
            5180
        }
        #[zbus(property)]
        fn wpa_flags(&self) -> u32 {
            1
        }
        #[zbus(property)]
        fn rsn_flags(&self) -> u32 {
            2
        }
    }

    struct ActiveConnectionFixture {
        ipv4: zbus::zvariant::OwnedObjectPath,
        ipv6: zbus::zvariant::OwnedObjectPath,
        is_vpn: bool,
    }

    #[zbus::interface(name = "org.freedesktop.NetworkManager.Connection.Active")]
    impl ActiveConnectionFixture {
        #[zbus(property)]
        fn id(&self) -> &str {
            if self.is_vpn {
                "Work VPN"
            } else {
                "Lab Wi-Fi"
            }
        }
        #[zbus(property)]
        fn uuid(&self) -> &str {
            if self.is_vpn {
                "00000000-0000-0000-0000-000000000002"
            } else {
                "00000000-0000-0000-0000-000000000001"
            }
        }
        #[zbus(property, name = "Type")]
        fn r#type(&self) -> &str {
            if self.is_vpn {
                "vpn"
            } else {
                "802-11-wireless"
            }
        }
        #[zbus(property)]
        fn state(&self) -> u32 {
            2
        }
        #[zbus(property)]
        fn default(&self) -> bool {
            true
        }
        #[zbus(property)]
        fn default6(&self) -> bool {
            true
        }
        #[zbus(property)]
        fn vpn(&self) -> bool {
            self.is_vpn
        }
        #[zbus(property)]
        fn ip4_config(&self) -> zbus::zvariant::OwnedObjectPath {
            self.ipv4.clone()
        }
        #[zbus(property)]
        fn ip6_config(&self) -> zbus::zvariant::OwnedObjectPath {
            self.ipv6.clone()
        }
    }

    struct Ipv4Fixture;

    #[zbus::interface(name = "org.freedesktop.NetworkManager.IP4Config")]
    impl Ipv4Fixture {
        #[zbus(property)]
        fn address_data(&self) -> Vec<HashMap<String, zbus::zvariant::OwnedValue>> {
            vec![HashMap::from([
                (
                    "address".to_owned(),
                    zbus::zvariant::OwnedValue::from(zbus::zvariant::Str::from("192.0.2.20")),
                ),
                (
                    "prefix".to_owned(),
                    zbus::zvariant::OwnedValue::from(24_u32),
                ),
            ])]
        }
        #[zbus(property)]
        fn gateway(&self) -> &str {
            "192.0.2.1"
        }
        #[zbus(property)]
        fn nameserver_data(&self) -> Vec<HashMap<String, zbus::zvariant::OwnedValue>> {
            vec![HashMap::from([(
                "address".to_owned(),
                zbus::zvariant::OwnedValue::from(zbus::zvariant::Str::from("192.0.2.53")),
            )])]
        }
    }

    struct Ipv6Fixture;

    #[zbus::interface(name = "org.freedesktop.NetworkManager.IP6Config")]
    impl Ipv6Fixture {
        #[zbus(property)]
        fn address_data(&self) -> Vec<HashMap<String, zbus::zvariant::OwnedValue>> {
            vec![HashMap::from([
                (
                    "address".to_owned(),
                    zbus::zvariant::OwnedValue::from(zbus::zvariant::Str::from("2001:db8::20")),
                ),
                (
                    "prefix".to_owned(),
                    zbus::zvariant::OwnedValue::from(64_u32),
                ),
            ])]
        }
        #[zbus(property)]
        fn gateway(&self) -> &str {
            "2001:db8::1"
        }
        #[zbus(property)]
        fn nameservers(&self) -> Vec<Vec<u8>> {
            vec!["2001:db8::53"
                .parse::<Ipv6Addr>()
                .unwrap()
                .octets()
                .to_vec()]
        }
    }

    #[test]
    fn wifi_profiles_are_built_and_validated_before_sending() {
        let open = wifi_settings("Cafe", b"Cafe", None).unwrap();
        assert!(open.contains_key("802-11-wireless"));
        assert!(!open.contains_key("802-11-wireless-security"));
        let secured = wifi_settings("Home", b"Home", Some("correct horse")).unwrap();
        let security = &secured["802-11-wireless-security"];
        assert_eq!(<&str>::try_from(&*security["key-mgmt"]).unwrap(), "wpa-psk");
        assert!(wifi_settings("Home", b"Home", Some("short")).is_err());
        assert!(wifi_settings("Home", b"Home", Some(&"z".repeat(64))).is_err());
        assert!(wifi_settings("Home", b"Home", Some(&"ab".repeat(32))).is_ok());
        assert!(wifi_settings("", b"", None).is_err());
        assert!(wifi_settings("x", &[b'x'; 33], None).is_err());
    }
}
