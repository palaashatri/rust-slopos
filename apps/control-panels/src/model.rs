//! What each panel shows, computed from provider snapshots without any
//! D-Bus or window: the truth rules of AGENTS.md live here and are tested.
//!
//! A panel shows either a provider's real snapshot or why it has none
//! ([`Unavailable`]); it never invents a device, network or level.

use slopos_core::capability::Availability;
use slopos_services::audio::{AudioDevice, AudioSnapshot};
use slopos_services::bluetooth::{BluetoothDevice, BluetoothSnapshot};
use slopos_services::brightness::BrightnessSnapshot;
use slopos_services::network::{NetworkDeviceState, NetworkDeviceType, NetworkSnapshot};
use slopos_services::timedate::TimedateSnapshot;

/// Why a provider has nothing to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unavailable {
    pub availability: Availability,
    pub detail: String,
}

impl Unavailable {
    pub fn new(availability: Availability, detail: impl Into<String>) -> Self {
        Self {
            availability,
            detail: detail.into(),
        }
    }

    /// One sentence for the panel.
    pub fn message(&self, service: &str) -> String {
        let reason = match self.availability {
            Availability::Loading => return format!("Checking {service}…"),
            Availability::MissingService => format!("{service} is not running on this computer"),
            Availability::UnsupportedHardware => {
                "this computer has no supported hardware".to_owned()
            }
            Availability::PermissionDenied => "SLOPOS is not allowed to change this".to_owned(),
            Availability::Disconnected => format!("the connection to {service} was lost"),
            Availability::Failed | Availability::Available => {
                format!("{service} reported an error")
            }
        };
        if self.detail.is_empty() {
            format!("Unavailable: {reason}.")
        } else {
            format!("Unavailable: {reason} ({}).", self.detail)
        }
    }
}

/// A provider's latest state.
pub type State<T> = Result<T, Unavailable>;

pub fn loading<T>() -> State<T> {
    Err(Unavailable::new(Availability::Loading, ""))
}

/// Average channel volume in percent.
pub fn volume_percent(device: &AudioDevice) -> u32 {
    if device.volume_millipercent.is_empty() {
        return 0;
    }
    let sum: u64 = device
        .volume_millipercent
        .iter()
        .map(|v| u64::from(*v))
        .sum();
    (sum / device.volume_millipercent.len() as u64 / 1000) as u32
}

pub fn default_output(snapshot: &AudioSnapshot) -> Option<&AudioDevice> {
    snapshot
        .outputs
        .iter()
        .find(|device| device.is_default)
        .or_else(|| snapshot.outputs.first())
}

pub fn default_input(snapshot: &AudioSnapshot) -> Option<&AudioDevice> {
    snapshot
        .inputs
        .iter()
        .find(|device| device.is_default)
        .or_else(|| snapshot.inputs.first())
}

/// "Firefox — Playback" for the application pop-up.
pub fn stream_label(stream: &slopos_services::audio::AudioStream) -> String {
    match &stream.media {
        Some(media) if media != &stream.application => {
            format!("{} — {media}", stream.application)
        }
        _ => stream.application.clone(),
    }
}

/// Average channel volume of an application stream, in percent.
pub fn stream_percent(stream: &slopos_services::audio::AudioStream) -> u32 {
    let channels = &stream.volume_millipercent;
    if channels.is_empty() {
        return 0;
    }
    (channels.iter().map(|v| u64::from(*v)).sum::<u64>() / channels.len() as u64 / 1000) as u32
}

pub fn device_name(device: &AudioDevice) -> String {
    device
        .description
        .clone()
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| device.name.clone())
}

fn device_state(state: NetworkDeviceState) -> &'static str {
    match state {
        NetworkDeviceState::Activated => "connected",
        NetworkDeviceState::Disconnected => "disconnected",
        NetworkDeviceState::Unavailable => "unavailable",
        NetworkDeviceState::Unmanaged => "not managed",
        NetworkDeviceState::Failed => "failed",
        _ => "connecting",
    }
}

fn device_kind(kind: NetworkDeviceType) -> &'static str {
    match kind {
        NetworkDeviceType::Ethernet => "Ethernet",
        NetworkDeviceType::Wifi => "Wi-Fi",
        _ => "Other",
    }
}

/// Lines describing the network state.
pub fn network_lines(snapshot: &NetworkSnapshot) -> Vec<String> {
    let mut lines = Vec::new();
    let primary = snapshot
        .active_connections
        .iter()
        .find(|c| c.default_ipv4 || c.default_ipv6)
        .or_else(|| snapshot.active_connections.first());
    lines.push(match primary {
        Some(connection) => {
            let address = connection
                .ipv4
                .as_ref()
                .and_then(|ip| ip.addresses.first())
                .map(|a| format!(" — {}/{}", a.address, a.prefix))
                .unwrap_or_default();
            format!("Connected to “{}”{address}", connection.id)
        }
        None if !snapshot.networking_enabled => "Networking is turned off".to_owned(),
        None => "Not connected".to_owned(),
    });
    for device in &snapshot.devices {
        if matches!(
            device.kind,
            NetworkDeviceType::Ethernet | NetworkDeviceType::Wifi
        ) {
            lines.push(format!(
                "{} {}: {}",
                device_kind(device.kind),
                device.interface,
                device_state(device.state)
            ));
        }
    }
    if !snapshot.wireless_hardware_enabled
        && snapshot
            .devices
            .iter()
            .any(|d| d.kind == NetworkDeviceType::Wifi)
    {
        lines.push("Wi-Fi is switched off by a hardware switch".to_owned());
    }
    lines
}

/// Wi-Fi networks in range, strongest first, de-duplicated by name.
pub fn wifi_networks(snapshot: &NetworkSnapshot) -> Vec<(String, u8, bool)> {
    let mut seen = std::collections::HashMap::<String, (u8, bool)>::new();
    for device in snapshot
        .devices
        .iter()
        .filter(|d| d.kind == NetworkDeviceType::Wifi)
    {
        for ap in &device.access_points {
            let name = String::from_utf8_lossy(&ap.ssid).into_owned();
            if name.is_empty() {
                continue;
            }
            let secured = ap.wpa_flags != 0 || ap.rsn_flags != 0;
            let entry = seen.entry(name).or_insert((0, secured));
            entry.0 = entry.0.max(ap.strength);
        }
    }
    let mut networks: Vec<(String, u8, bool)> = seen
        .into_iter()
        .map(|(name, (s, sec))| (name, s, sec))
        .collect();
    networks.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    networks
}

/// What joining network `name` needs: its strongest access point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiTarget {
    pub device: String,
    pub access_point: String,
    pub ssid: Vec<u8>,
    pub secured: bool,
    /// The active connection when this network is the one in use.
    pub active_connection: Option<String>,
}

pub fn wifi_target(snapshot: &NetworkSnapshot, name: &str) -> Option<WifiTarget> {
    snapshot
        .devices
        .iter()
        .filter(|d| d.kind == NetworkDeviceType::Wifi)
        .flat_map(|device| device.access_points.iter().map(move |ap| (device, ap)))
        .filter(|(_, ap)| String::from_utf8_lossy(&ap.ssid) == name)
        .max_by_key(|(_, ap)| ap.strength)
        .map(|(device, ap)| {
            let in_use = snapshot.devices.iter().any(|d| {
                d.kind == NetworkDeviceType::Wifi
                    && d.active_access_point.as_ref().is_some_and(|active| {
                        d.access_points
                            .iter()
                            .any(|p| &p.object_path == active && p.ssid == ap.ssid)
                    })
            });
            WifiTarget {
                device: device.object_path.clone(),
                access_point: ap.object_path.clone(),
                ssid: ap.ssid.clone(),
                secured: ap.wpa_flags != 0 || ap.rsn_flags != 0,
                active_connection: if in_use {
                    device.active_connection.clone()
                } else {
                    None
                },
            }
        })
}

pub fn bluetooth_lines(snapshot: &BluetoothSnapshot) -> Vec<String> {
    if snapshot.adapters.is_empty() {
        return vec!["No Bluetooth adapter was found.".to_owned()];
    }
    let mut lines: Vec<String> = snapshot
        .adapters
        .iter()
        .map(|adapter| {
            format!(
                "{} ({}) — {}{}",
                adapter.name,
                adapter.address,
                if adapter.powered { "on" } else { "off" },
                if adapter.discovering {
                    ", searching"
                } else {
                    ""
                }
            )
        })
        .collect();
    let paired = snapshot.devices.iter().filter(|d| d.paired).count();
    lines.push(match (snapshot.devices.len(), paired) {
        (0, _) => "No devices found yet.".to_owned(),
        (1, paired) => format!("1 device, {paired} paired"),
        (found, paired) => format!("{found} devices, {paired} paired"),
    });
    lines
}

/// Where Control Panels keeps the display layout, in the settings folder.
pub const DISPLAYS_FILE: &str = "displays.conf";

/// "HDMI-1" with "(built-in)" for laptop panels.
pub fn output_label(output: &slopos_services::display::DisplayOutput) -> String {
    let builtin = ["eDP", "LVDS", "DSI"]
        .iter()
        .any(|prefix| output.name.starts_with(prefix));
    if builtin {
        format!("{} (built-in)", output.name)
    } else {
        output.name.clone()
    }
}

pub fn size_label(width: u16, height: u16) -> String {
    format!("{width} × {height}")
}

/// "60 Hz", "59.94 Hz"; "Unknown" for modes without timing.
pub fn refresh_label(millihz: u32) -> String {
    if millihz == 0 {
        "Unknown".to_owned()
    } else if millihz % 1000 < 5 || millihz % 1000 > 995 {
        format!("{} Hz", (millihz + 500) / 1000)
    } else {
        format!("{:.2} Hz", f64::from(millihz) / 1000.0)
    }
}

pub fn rotation_label(rotation: slopos_services::display::Rotation) -> &'static str {
    use slopos_services::display::Rotation;
    match rotation {
        Rotation::Normal => "Standard",
        Rotation::Left => "90° (portrait, left)",
        Rotation::Inverted => "180° (upside down)",
        Rotation::Right => "270° (portrait, right)",
    }
}

pub fn placement_label(placement: slopos_services::display::Placement, other: &str) -> String {
    use slopos_services::display::Placement;
    match placement {
        Placement::RightOf => format!("Right of {other}"),
        Placement::LeftOf => format!("Left of {other}"),
        Placement::Above => format!("Above {other}"),
        Placement::Below => format!("Below {other}"),
        Placement::SameAs => format!("Mirror {other}"),
    }
}

pub fn revert_message(seconds: u32) -> String {
    format!(
        "If the screen looks wrong, wait: the previous settings come back in {seconds} second{}.",
        if seconds == 1 { "" } else { "s" }
    )
}

/// The Keyboard & Mouse summary lines.
pub fn input_lines(
    snapshot: &slopos_services::input::InputSnapshot,
    layouts: &[(String, String)],
) -> Vec<String> {
    let settings = &snapshot.settings;
    let layout = layouts
        .iter()
        .find(|(code, _)| *code == settings.layout)
        .map_or(settings.layout.as_str(), |(_, name)| name.as_str());
    let mut lines = vec![if settings.variant.is_empty() {
        format!("Keyboard: {layout}")
    } else {
        format!("Keyboard: {layout} ({})", settings.variant)
    }];
    lines.push(if snapshot.tunable_pointers == 0 {
        "No mouse or touchpad here takes speed settings.".to_owned()
    } else {
        format!(
            "{} pointing device{} found.",
            snapshot.tunable_pointers,
            if snapshot.tunable_pointers == 1 {
                ""
            } else {
                "s"
            }
        )
    });
    lines
}

/// Devices for the list: connected first, then paired, then by name.
pub fn bluetooth_devices(snapshot: &BluetoothSnapshot) -> Vec<BluetoothDevice> {
    let mut devices = snapshot.devices.clone();
    devices.sort_by(|a, b| {
        (b.connected, b.paired)
            .cmp(&(a.connected, a.paired))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.address.cmp(&b.address))
    });
    devices
}

/// A device's state as the list shows it.
pub fn bluetooth_state(device: &BluetoothDevice) -> &'static str {
    if device.connected {
        "Connected"
    } else if device.paired {
        "Paired"
    } else {
        "Not paired"
    }
}

pub fn brightness_percent(snapshot: &BrightnessSnapshot) -> Option<(String, u32)> {
    let device = snapshot.devices.first()?;
    let range = device.maximum.saturating_sub(device.minimum).max(1);
    let percent =
        (u64::from(device.current.saturating_sub(device.minimum)) * 100 / u64::from(range)) as u32;
    Some((device.name.clone(), percent))
}

/// The raw brightness value for `percent` of `device`'s range.
pub fn brightness_value(snapshot: &BrightnessSnapshot, percent: u32) -> Option<(String, u32)> {
    let device = snapshot.devices.first()?;
    let range = device.maximum.saturating_sub(device.minimum);
    let value = device.minimum + (u64::from(range) * u64::from(percent.min(100)) / 100) as u32;
    Some((device.name.clone(), value))
}

pub fn timedate_lines(snapshot: &TimedateSnapshot) -> Vec<String> {
    vec![
        format!(
            "Time zone: {}",
            if snapshot.timezone.is_empty() {
                "not set"
            } else {
                &snapshot.timezone
            }
        ),
        if !snapshot.ntp_available {
            "Network time is not available".to_owned()
        } else if snapshot.ntp_enabled {
            format!(
                "Network time is on{}",
                if snapshot.ntp_synchronized {
                    " and synchronised"
                } else {
                    ", not yet synchronised"
                }
            )
        } else {
            "Network time is off".to_owned()
        },
    ]
}

/// The appearances `slopos-appearance` accepts.
pub const APPEARANCES: &[(&str, &str)] = &[
    ("platinum", "Platinum"),
    ("classic", "Classic"),
    ("graphite", "Graphite"),
    ("oled", "OLED Graphite"),
];

/// The saved appearance from `~/.config/slopos-i/appearance`, if any.
pub fn current_appearance(config_home: &std::path::Path) -> Option<String> {
    std::fs::read_to_string(config_home.join("slopos-i/appearance"))
        .ok()
        .map(|text| text.trim().to_owned())
        .filter(|name| APPEARANCES.iter().any(|(id, _)| id == name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use slopos_services::audio::AudioDirection;
    use slopos_services::bluetooth::{BluetoothAdapter, BluetoothDevice};
    use slopos_services::brightness::BrightnessDevice;
    use slopos_services::network::{
        ActiveNetworkConnection, NetworkAccessPoint, NetworkDevice, NetworkIpAddress,
        NetworkIpConfig,
    };

    fn output(volume: Vec<u32>, default: bool) -> AudioDevice {
        AudioDevice {
            index: 1,
            name: "alsa_output.pci".into(),
            description: Some("Built-in Audio".into()),
            direction: AudioDirection::Output,
            muted: false,
            volume_millipercent: volume,
            is_default: default,
        }
    }

    #[test]
    fn unavailable_providers_say_why_and_never_invent_values() {
        let missing = Unavailable::new(
            Availability::MissingService,
            "org.freedesktop.NetworkManager",
        );
        assert_eq!(
            missing.message("NetworkManager"),
            "Unavailable: NetworkManager is not running on this computer (org.freedesktop.NetworkManager)."
        );
        assert_eq!(
            loading::<()>().unwrap_err().message("PipeWire"),
            "Checking PipeWire…"
        );
        let denied = Unavailable::new(Availability::PermissionDenied, "");
        assert_eq!(
            denied.message("BlueZ"),
            "Unavailable: SLOPOS is not allowed to change this."
        );
    }

    #[test]
    fn audio_levels_and_defaults_come_from_the_snapshot() {
        let snapshot = AudioSnapshot {
            revision: 1,
            availability: Availability::Available,
            default_output: Some("alsa_output.pci".into()),
            default_input: None,
            outputs: vec![output(vec![40_000, 60_000], true)],
            inputs: Vec::new(),
            streams: Vec::new(),
        };
        let device = default_output(&snapshot).unwrap();
        assert_eq!(volume_percent(device), 50);
        assert_eq!(device_name(device), "Built-in Audio");
        assert!(default_input(&snapshot).is_none());
        assert_eq!(volume_percent(&output(Vec::new(), false)), 0);
    }

    #[test]
    fn network_lines_describe_real_connections_and_devices() {
        let mut snapshot = NetworkSnapshot {
            revision: 1,
            networking_enabled: true,
            wireless_enabled: true,
            wireless_hardware_enabled: false,
            devices: vec![NetworkDevice {
                object_path: "/d/1".into(),
                interface: "wlan0".into(),
                ip_interface: "wlan0".into(),
                kind: NetworkDeviceType::Wifi,
                state: NetworkDeviceState::Disconnected,
                active_access_point: None,
                access_points: vec![
                    NetworkAccessPoint {
                        object_path: "/a/1".into(),
                        ssid: b"Home".to_vec(),
                        strength: 40,
                        frequency_mhz: 2412,
                        wpa_flags: 0,
                        rsn_flags: 8,
                    },
                    NetworkAccessPoint {
                        object_path: "/a/2".into(),
                        ssid: b"Home".to_vec(),
                        strength: 70,
                        frequency_mhz: 5180,
                        wpa_flags: 0,
                        rsn_flags: 8,
                    },
                    NetworkAccessPoint {
                        object_path: "/a/3".into(),
                        ssid: b"Cafe".to_vec(),
                        strength: 55,
                        frequency_mhz: 2437,
                        wpa_flags: 0,
                        rsn_flags: 0,
                    },
                    NetworkAccessPoint {
                        object_path: "/a/4".into(),
                        ssid: Vec::new(),
                        strength: 99,
                        frequency_mhz: 2437,
                        wpa_flags: 0,
                        rsn_flags: 0,
                    },
                ],
                active_connection: None,
            }],
            active_connections: Vec::new(),
        };
        let lines = network_lines(&snapshot);
        assert_eq!(lines[0], "Not connected");
        assert!(lines.contains(&"Wi-Fi wlan0: disconnected".to_owned()));
        assert!(lines.last().unwrap().contains("hardware switch"));
        assert_eq!(
            wifi_networks(&snapshot),
            vec![
                ("Home".to_owned(), 70, true),
                ("Cafe".to_owned(), 55, false)
            ]
        );
        // Joining uses the strongest access point; nothing is in use yet.
        let home = wifi_target(&snapshot, "Home").unwrap();
        assert_eq!((home.access_point.as_str(), home.secured), ("/a/2", true));
        assert_eq!(home.device, "/d/1");
        assert_eq!(home.active_connection, None);
        assert!(!wifi_target(&snapshot, "Cafe").unwrap().secured);
        assert!(wifi_target(&snapshot, "Elsewhere").is_none());
        snapshot.devices[0].active_access_point = Some("/a/1".into());
        snapshot.devices[0].active_connection = Some("/ac/9".into());
        assert_eq!(
            wifi_target(&snapshot, "Home")
                .unwrap()
                .active_connection
                .as_deref(),
            Some("/ac/9")
        );
        assert_eq!(
            wifi_target(&snapshot, "Cafe").unwrap().active_connection,
            None
        );
        snapshot.devices[0].active_access_point = None;
        snapshot.devices[0].active_connection = None;
        snapshot.active_connections.push(ActiveNetworkConnection {
            object_path: "/c/1".into(),
            id: "Wired connection 1".into(),
            uuid: "u".into(),
            connection_type: "802-3-ethernet".into(),
            state: 2,
            default_ipv4: true,
            default_ipv6: false,
            is_vpn: false,
            ipv4: Some(NetworkIpConfig {
                addresses: vec![NetworkIpAddress {
                    address: "10.0.2.15".into(),
                    prefix: 24,
                }],
                gateway: Some("10.0.2.2".into()),
                nameservers: Vec::new(),
            }),
            ipv6: None,
        });
        assert_eq!(
            network_lines(&snapshot)[0],
            "Connected to “Wired connection 1” — 10.0.2.15/24"
        );
        snapshot.active_connections.clear();
        snapshot.networking_enabled = false;
        assert_eq!(network_lines(&snapshot)[0], "Networking is turned off");
    }

    #[test]
    fn brightness_and_bluetooth_and_time() {
        let brightness = BrightnessSnapshot {
            revision: 1,
            availability: Availability::Available,
            devices: vec![BrightnessDevice {
                name: "intel_backlight".into(),
                current: 600,
                minimum: 0,
                maximum: 1200,
            }],
        };
        assert_eq!(
            brightness_percent(&brightness),
            Some(("intel_backlight".into(), 50))
        );
        assert_eq!(
            brightness_value(&brightness, 25),
            Some(("intel_backlight".into(), 300))
        );
        assert_eq!(brightness_value(&brightness, 400).unwrap().1, 1200);
        let empty = BrightnessSnapshot {
            revision: 1,
            availability: Availability::UnsupportedHardware,
            devices: Vec::new(),
        };
        assert_eq!(brightness_percent(&empty), None);
        let bluetooth = BluetoothSnapshot {
            revision: 1,
            availability: Availability::Available,
            adapters: vec![BluetoothAdapter {
                path: "/hci0".into(),
                name: "slopos".into(),
                address: "AA:BB".into(),
                powered: true,
                discovering: false,
            }],
            devices: vec![BluetoothDevice {
                path: "/d".into(),
                name: "Headphones".into(),
                address: "CC".into(),
                adapter_path: "/hci0".into(),
                paired: true,
                trusted: true,
                connected: false,
            }],
        };
        assert_eq!(
            bluetooth_lines(&bluetooth),
            vec![
                "slopos (AA:BB) — on".to_owned(),
                "1 device, 1 paired".to_owned()
            ]
        );
        let mut more = bluetooth.clone();
        let device = |name: &str, paired, connected| BluetoothDevice {
            path: format!("/{name}"),
            name: name.into(),
            address: name.into(),
            adapter_path: "/hci0".into(),
            paired,
            trusted: paired,
            connected,
        };
        more.devices.push(device("keyboard", false, false));
        more.devices.push(device("Mouse", true, true));
        let names: Vec<_> = bluetooth_devices(&more)
            .into_iter()
            .map(|d| (d.name.clone(), bluetooth_state(&d)))
            .collect();
        assert_eq!(
            names,
            vec![
                ("Mouse".to_owned(), "Connected"),
                ("Headphones".to_owned(), "Paired"),
                ("keyboard".to_owned(), "Not paired")
            ]
        );
        let none = BluetoothSnapshot {
            revision: 1,
            availability: Availability::Available,
            adapters: Vec::new(),
            devices: Vec::new(),
        };
        assert_eq!(
            bluetooth_lines(&none),
            vec!["No Bluetooth adapter was found.".to_owned()]
        );
        let time = TimedateSnapshot {
            revision: 1,
            timezone: "Etc/UTC".into(),
            ntp_available: true,
            ntp_enabled: true,
            ntp_synchronized: false,
            time_unix_usec: 0,
        };
        assert_eq!(
            timedate_lines(&time),
            vec![
                "Time zone: Etc/UTC".to_owned(),
                "Network time is on, not yet synchronised".to_owned()
            ]
        );
    }

    #[test]
    fn appearance_is_read_from_config_and_validated() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(current_appearance(dir.path()), None);
        std::fs::create_dir_all(dir.path().join("slopos-i")).unwrap();
        std::fs::write(dir.path().join("slopos-i/appearance"), "graphite\n").unwrap();
        assert_eq!(current_appearance(dir.path()).as_deref(), Some("graphite"));
        std::fs::write(dir.path().join("slopos-i/appearance"), "../../etc").unwrap();
        assert_eq!(current_appearance(dir.path()), None);
    }
}
