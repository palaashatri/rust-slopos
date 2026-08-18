//! Network status adapter using NetworkManager D-Bus and sysfs fallback.

use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkStatus {
    ConnectedEthernet,
    ConnectedWifi(Option<String>),
    ConnectedOther(String),
    Connecting,
    Disconnected,
}

pub fn query_network_status() -> NetworkStatus {
    // 1. Try NetworkManager D-Bus
    if let Some(status) = query_networkmanager_dbus() {
        return status;
    }

    // 2. Try sysfs /sys/class/net
    if let Some(status) = query_sysfs_net() {
        return status;
    }

    NetworkStatus::Disconnected
}

fn query_networkmanager_dbus() -> Option<NetworkStatus> {
    let connection = zbus::blocking::Connection::system().ok()?;
    let proxy = zbus::blocking::Proxy::new(
        &connection,
        "org.freedesktop.NetworkManager",
        "/org/freedesktop/NetworkManager",
        "org.freedesktop.NetworkManager",
    )
    .ok()?;

    let state: u32 = proxy.get_property("State").ok()?;
    // NM_STATE_CONNECTED_GLOBAL = 70, NM_STATE_CONNECTED_SITE = 60, NM_STATE_CONNECTED_LOCAL = 50
    if state >= 50 {
        let conn_type: String = proxy
            .get_property("PrimaryConnectionType")
            .unwrap_or_default();
        if conn_type.contains("wireless") || conn_type.contains("wifi") {
            return Some(NetworkStatus::ConnectedWifi(None));
        } else if conn_type.contains("ethernet") {
            return Some(NetworkStatus::ConnectedEthernet);
        } else if !conn_type.is_empty() {
            return Some(NetworkStatus::ConnectedOther(conn_type));
        }
        return Some(NetworkStatus::ConnectedEthernet);
    } else if state == 40 {
        return Some(NetworkStatus::Connecting);
    }

    Some(NetworkStatus::Disconnected)
}

fn query_sysfs_net() -> Option<NetworkStatus> {
    let net_dir = Path::new("/sys/class/net");
    if !net_dir.is_dir() {
        return None;
    }

    let entries = fs::read_dir(net_dir).ok()?;
    for entry in entries.flatten() {
        let iface = entry.file_name().to_string_lossy().to_string();
        if iface == "lo" {
            continue;
        }

        let operstate_file = entry.path().join("operstate");
        if let Ok(state) = fs::read_to_string(operstate_file) {
            if state.trim().eq_ignore_ascii_case("up") {
                if iface.starts_with("wl") || iface.starts_with("wlan") {
                    return Some(NetworkStatus::ConnectedWifi(Some(iface)));
                }
                return Some(NetworkStatus::ConnectedEthernet);
            }
        }
    }
    None
}

pub fn network_label_text(status: &NetworkStatus) -> String {
    match status {
        NetworkStatus::ConnectedEthernet => "LAN".to_string(),
        NetworkStatus::ConnectedWifi(_) => "Wi-Fi".to_string(),
        NetworkStatus::ConnectedOther(name) => name.clone(),
        NetworkStatus::Connecting => "…".to_string(),
        NetworkStatus::Disconnected => "Offline".to_string(),
    }
}
