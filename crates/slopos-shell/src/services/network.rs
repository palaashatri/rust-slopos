//! Network status for the menu bar, derived from the shared NetworkManager
//! provider in `slopos-services`.
//!
//! The shell does not probe interfaces itself: everything shown here comes
//! from a NetworkManager snapshot, and a missing NetworkManager is reported as
//! such instead of guessing from `/sys/class/net`.

use slopos_services::network::{
    ActiveNetworkConnection, NetworkDeviceType, NetworkManagerClient, NetworkSnapshot,
};
use std::thread;
use std::time::Duration;

/// NetworkManager `NMActiveConnectionState` values.
const ACTIVE_ACTIVATING: u32 = 1;
const ACTIVE_ACTIVATED: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkSummary {
    /// Short menu-bar text.
    pub label: String,
    /// Freedesktop symbolic icon name.
    pub icon: &'static str,
    /// Lines for the network menu, most important first.
    pub details: Vec<String>,
    /// Whether NetworkManager answered at all.
    pub service_available: bool,
    /// Whether the system has a Wi-Fi device, and whether its radio is on.
    pub wifi: Option<bool>,
}

impl NetworkSummary {
    /// Shown until NetworkManager has answered for the first time.
    pub fn checking() -> Self {
        Self {
            label: "…".to_owned(),
            icon: "network-offline-symbolic",
            details: vec!["Checking network status…".to_owned()],
            service_available: false,
            wifi: None,
        }
    }

    pub fn unavailable() -> Self {
        Self {
            label: "—".to_owned(),
            icon: "network-offline-symbolic",
            details: vec!["NetworkManager is not running".to_owned()],
            service_available: false,
            wifi: None,
        }
    }
}

fn kind_of(connection: &ActiveNetworkConnection) -> &'static str {
    if connection.is_vpn {
        return "VPN";
    }
    match connection.connection_type.as_str() {
        "802-11-wireless" => "Wi-Fi",
        "802-3-ethernet" => "Wired",
        "wireguard" | "vpn" => "VPN",
        "bluetooth" => "Bluetooth",
        "gsm" | "cdma" => "Mobile broadband",
        "bridge" | "bond" | "vlan" => "Wired",
        _ => "Network",
    }
}

fn icon_for(kind: &str) -> &'static str {
    match kind {
        "Wi-Fi" => "network-wireless-symbolic",
        "VPN" => "network-vpn-symbolic",
        "Mobile broadband" => "network-cellular-symbolic",
        _ => "network-wired-symbolic",
    }
}

fn shorten(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let mut short: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    short.push('…');
    short
}

pub fn summarize(snapshot: &NetworkSnapshot) -> NetworkSummary {
    let wifi = snapshot
        .devices
        .iter()
        .any(|device| device.kind == NetworkDeviceType::Wifi)
        .then_some(snapshot.wireless_enabled && snapshot.wireless_hardware_enabled);

    if !snapshot.networking_enabled {
        return NetworkSummary {
            label: "Off".to_owned(),
            icon: "network-offline-symbolic",
            details: vec!["Networking is turned off".to_owned()],
            service_available: true,
            wifi,
        };
    }

    let activated: Vec<&ActiveNetworkConnection> = snapshot
        .active_connections
        .iter()
        .filter(|connection| connection.state == ACTIVE_ACTIVATED)
        .collect();
    let primary = activated
        .iter()
        .copied()
        .find(|connection| connection.default_ipv4 || connection.default_ipv6)
        .or_else(|| activated.first().copied());

    let mut details = Vec::new();
    for connection in &activated {
        details.push(format!("{} — {}", connection.id, kind_of(connection)));
        if let Some(ipv4) = &connection.ipv4 {
            for address in &ipv4.addresses {
                details.push(format!("   IPv4 {}/{}", address.address, address.prefix));
            }
        }
    }
    if wifi == Some(false) {
        details.push("Wi-Fi is turned off".to_owned());
    }

    if let Some(primary) = primary {
        let kind = kind_of(primary);
        let label = if kind == "Wi-Fi" {
            shorten(&primary.id, 16)
        } else {
            kind.to_owned()
        };
        return NetworkSummary {
            label,
            icon: icon_for(kind),
            details,
            service_available: true,
            wifi,
        };
    }

    let connecting = snapshot
        .active_connections
        .iter()
        .find(|connection| connection.state == ACTIVE_ACTIVATING);
    if let Some(connection) = connecting {
        details.insert(0, format!("Connecting to {}…", connection.id));
        return NetworkSummary {
            label: "…".to_owned(),
            icon: icon_for(kind_of(connection)),
            details,
            service_available: true,
            wifi,
        };
    }

    details.insert(0, "Not connected".to_owned());
    NetworkSummary {
        label: "Offline".to_owned(),
        icon: "network-offline-symbolic",
        details,
        service_available: true,
        wifi,
    }
}

/// Follow NetworkManager on a background thread and call `on_change` with each
/// new summary. Reconnects when NetworkManager restarts.
pub fn watch<F>(on_change: F)
where
    F: Fn(NetworkSummary) + Send + 'static,
{
    let spawned = thread::Builder::new()
        .name("slopos-network-status".to_owned())
        .spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                on_change(NetworkSummary::unavailable());
                return;
            };
            runtime.block_on(async move {
                let mut last: Option<NetworkSummary> = None;
                let mut publish = |summary: NetworkSummary| {
                    if last.as_ref() != Some(&summary) {
                        last = Some(summary.clone());
                        on_change(summary);
                    }
                };
                loop {
                    match NetworkManagerClient::connect().await {
                        Ok(client) => {
                            if let Ok(snapshot) = client.snapshot().await {
                                publish(summarize(&snapshot));
                            }
                            let mut events = client.watch();
                            while let Some(update) = events.next().await {
                                match update {
                                    Ok(snapshot) => publish(summarize(&snapshot)),
                                    Err(_) => break,
                                }
                            }
                            publish(NetworkSummary::unavailable());
                        }
                        Err(_) => publish(NetworkSummary::unavailable()),
                    }
                    // NetworkManager is gone; check again later.
                    tokio::time::sleep(Duration::from_secs(10)).await;
                }
            });
        });
    if let Err(error) = spawned {
        log::warn!("Could not start the network status worker: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slopos_services::network::{
        NetworkDevice, NetworkDeviceState, NetworkIpAddress, NetworkIpConfig,
    };

    fn connection(id: &str, kind: &str, state: u32, default: bool) -> ActiveNetworkConnection {
        ActiveNetworkConnection {
            object_path: format!("/ac/{id}"),
            id: id.to_owned(),
            uuid: format!("uuid-{id}"),
            connection_type: kind.to_owned(),
            state,
            default_ipv4: default,
            default_ipv6: false,
            is_vpn: false,
            ipv4: Some(NetworkIpConfig {
                addresses: vec![NetworkIpAddress {
                    address: "192.0.2.10".to_owned(),
                    prefix: 24,
                }],
                gateway: None,
                nameservers: vec![],
            }),
            ipv6: None,
        }
    }

    fn wifi_device() -> NetworkDevice {
        NetworkDevice {
            object_path: "/dev/1".to_owned(),
            interface: "wlan0".to_owned(),
            ip_interface: "wlan0".to_owned(),
            kind: NetworkDeviceType::Wifi,
            state: NetworkDeviceState::Activated,
            active_access_point: None,
            access_points: vec![],
            active_connection: None,
        }
    }

    fn snapshot(connections: Vec<ActiveNetworkConnection>) -> NetworkSnapshot {
        NetworkSnapshot {
            revision: 1,
            networking_enabled: true,
            wireless_enabled: true,
            wireless_hardware_enabled: true,
            devices: vec![],
            active_connections: connections,
        }
    }

    #[test]
    fn wired_default_route_is_labelled_wired_with_its_address() {
        let summary = summarize(&snapshot(vec![connection(
            "Wired connection 1",
            "802-3-ethernet",
            ACTIVE_ACTIVATED,
            true,
        )]));
        assert_eq!(summary.label, "Wired");
        assert_eq!(summary.icon, "network-wired-symbolic");
        assert!(summary
            .details
            .iter()
            .any(|line| line.contains("192.0.2.10/24")));
    }

    #[test]
    fn wifi_shows_the_network_name_and_prefers_the_default_route() {
        let summary = summarize(&snapshot(vec![
            connection("docker0", "bridge", ACTIVE_ACTIVATED, false),
            connection("Home Network", "802-11-wireless", ACTIVE_ACTIVATED, true),
        ]));
        assert_eq!(summary.label, "Home Network");
        assert_eq!(summary.icon, "network-wireless-symbolic");
    }

    #[test]
    fn nothing_activated_is_offline_not_lan() {
        let summary = summarize(&snapshot(vec![]));
        assert_eq!(summary.label, "Offline");
        assert_eq!(summary.details[0], "Not connected");
    }

    #[test]
    fn an_activating_connection_is_reported_as_connecting() {
        let summary = summarize(&snapshot(vec![connection(
            "Cafe",
            "802-11-wireless",
            ACTIVE_ACTIVATING,
            false,
        )]));
        assert_eq!(summary.label, "…");
        assert_eq!(summary.details[0], "Connecting to Cafe…");
    }

    #[test]
    fn disabled_networking_and_wifi_radio_are_explained() {
        let mut off = snapshot(vec![]);
        off.networking_enabled = false;
        assert_eq!(summarize(&off).label, "Off");

        let mut radio_off = snapshot(vec![]);
        radio_off.devices.push(wifi_device());
        radio_off.wireless_enabled = false;
        let summary = summarize(&radio_off);
        assert_eq!(summary.wifi, Some(false));
        assert!(summary
            .details
            .iter()
            .any(|line| line == "Wi-Fi is turned off"));
    }

    #[test]
    fn long_network_names_are_shortened_for_the_menu_bar() {
        let summary = summarize(&snapshot(vec![connection(
            "A Very Long Wireless Network Name",
            "802-11-wireless",
            ACTIVE_ACTIVATED,
            true,
        )]));
        assert_eq!(summary.label.chars().count(), 16);
        assert!(summary.label.ends_with('…'));
    }
}
