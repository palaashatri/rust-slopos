//! The Wi-Fi, Bluetooth and clock popovers: the lines each one shows, built
//! only from `slopos-services` provider answers. A missing provider is said
//! to be unavailable with its reason; nothing is filled in. Plain logic,
//! tested without X. The bar turns the lines into its popup menu.

use slopos_services::bluetooth::BluetoothSnapshot;
use slopos_services::network::{NetworkDeviceType, NetworkSnapshot};
use slopos_services::power::{BatteryStatus, BatteryWarning, PowerSnapshot, PowerSource};
use slopos_services::timedate::TimedateSnapshot;

use crate::history::History;
use crate::notifications::Toast;
use crate::status::State;

/// The most nearby networks the Wi-Fi popover lists.
const NEARBY: usize = 5;
/// Longest name shown in a popover line, in characters.
const NAME_CHARS: usize = 32;
/// Longest notification line in the history popover, in characters.
const HISTORY_CHARS: usize = 72;
/// The most notifications the history popover lists before "N earlier".
const HISTORY_SHOWN: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    /// A status line; it cannot be chosen.
    Info(String),
    Separator,
    /// Opens a Control Panel with this argument. Disabled when no Control Panels
    /// program is installed.
    Panel {
        label: &'static str,
        argument: &'static str,
        enabled: bool,
    },
    /// Clears the notification history. Disabled when there is nothing to clear.
    Clear {
        enabled: bool,
    },
}

fn info(text: impl Into<String>) -> Line {
    Line::Info(text.into())
}

fn panel(label: &'static str, argument: &'static str, enabled: bool) -> Line {
    Line::Panel {
        label,
        argument,
        enabled,
    }
}

/// A name cut to `NAME_CHARS` characters, with an ellipsis when it was cut.
fn short(name: &str) -> String {
    cut(name, NAME_CHARS)
}

/// `text` cut to `chars` characters, with an ellipsis when it was cut.
fn cut(text: &str, chars: usize) -> String {
    if text.chars().count() <= chars {
        text.to_owned()
    } else {
        let head: String = text.chars().take(chars - 1).collect();
        format!("{head}…")
    }
}

/// The Wi-Fi popover: whether Wi-Fi is on, the network it is joined to, and the
/// nearby networks it can see.
pub fn wifi_lines(state: &State<NetworkSnapshot>, panels: bool) -> Vec<Line> {
    let mut lines = Vec::new();
    match state {
        Err(why) => lines.push(info(format!("Network unavailable: {why}"))),
        Ok(snapshot) if !snapshot.networking_enabled => {
            lines.push(info("Networking is off"));
        }
        Ok(snapshot) => {
            let wifi: Vec<_> = snapshot
                .devices
                .iter()
                .filter(|device| device.kind == NetworkDeviceType::Wifi)
                .collect();
            if wifi.is_empty() {
                lines.push(info("No Wi-Fi adapter"));
            } else if !snapshot.wireless_hardware_enabled {
                lines.push(info("Wi-Fi is switched off in hardware"));
            } else if !snapshot.wireless_enabled {
                lines.push(info("Wi-Fi is off"));
            } else {
                let joined = wifi.iter().find_map(|device| {
                    let path = device.active_access_point.as_ref()?;
                    device
                        .access_points
                        .iter()
                        .find(|point| &point.object_path == path)
                        .map(|point| (point, device))
                });
                match joined {
                    Some((point, _)) => {
                        lines.push(info(format!(
                            "Connected to {}",
                            short(&ssid_text(&point.ssid))
                        )));
                        lines.push(info(format!("Signal {}%", point.strength)));
                    }
                    None => lines.push(info("Not connected to Wi-Fi")),
                }
                let joined_name = joined.map(|(point, _)| ssid_text(&point.ssid));
                let mut nearby: Vec<(String, u8)> = Vec::new();
                for point in wifi.iter().flat_map(|device| &device.access_points) {
                    let name = ssid_text(&point.ssid);
                    if name.is_empty() || Some(&name) == joined_name.as_ref() {
                        continue;
                    }
                    match nearby.iter_mut().find(|(seen, _)| *seen == name) {
                        Some(entry) => entry.1 = entry.1.max(point.strength),
                        None => nearby.push((name, point.strength)),
                    }
                }
                nearby.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                if !nearby.is_empty() {
                    lines.push(Line::Separator);
                    for (name, strength) in nearby.into_iter().take(NEARBY) {
                        lines.push(info(format!("{} — {strength}%", short(&name))));
                    }
                }
            }
        }
    }
    lines.push(Line::Separator);
    lines.push(panel("Network Preferences…", "--network", panels));
    lines
}

fn ssid_text(ssid: &[u8]) -> String {
    String::from_utf8_lossy(ssid).trim().to_owned()
}

/// The Bluetooth item's text, or `None` before the first answer.
pub fn bluetooth_text(state: Option<&State<BluetoothSnapshot>>) -> String {
    match state {
        None => "…".into(),
        Some(Err(_)) => "Bluetooth unavailable".into(),
        Some(Ok(snapshot)) => {
            let Some(adapter) = snapshot.adapters.first() else {
                return "No adapter".into();
            };
            if !adapter.powered {
                return "Off".into();
            }
            let connected: Vec<_> = snapshot
                .devices
                .iter()
                .filter(|device| device.connected)
                .collect();
            match connected.as_slice() {
                [] => "On".into(),
                [one] => short(&one.name),
                many => format!("{} connected", many.len()),
            }
        }
    }
}

/// The Bluetooth popover: the adapter's state, the devices connected now and
/// the paired devices that are not.
pub fn bluetooth_lines(state: Option<&State<BluetoothSnapshot>>, panels: bool) -> Vec<Line> {
    let mut lines = Vec::new();
    match state {
        None => lines.push(info("Reading Bluetooth…")),
        Some(Err(why)) => lines.push(info(format!("Bluetooth unavailable: {why}"))),
        Some(Ok(snapshot)) => match snapshot.adapters.first() {
            None => lines.push(info("No Bluetooth adapter")),
            Some(adapter) => {
                lines.push(info(if adapter.powered {
                    "Bluetooth is on"
                } else {
                    "Bluetooth is off"
                }));
                lines.push(info(short(&adapter.name)));
                if adapter.powered {
                    let connected: Vec<_> = snapshot
                        .devices
                        .iter()
                        .filter(|device| device.connected)
                        .collect();
                    let paired = snapshot
                        .devices
                        .iter()
                        .filter(|device| device.paired && !device.connected)
                        .count();
                    if !connected.is_empty() {
                        lines.push(Line::Separator);
                        for device in connected {
                            lines.push(info(format!("{} — connected", short(&device.name))));
                        }
                    }
                    if paired > 0 {
                        lines.push(info(format!("{paired} paired, not connected")));
                    }
                }
            }
        },
    }
    lines.push(Line::Separator);
    lines.push(panel("Bluetooth Preferences…", "--bluetooth", panels));
    lines
}

/// The clock popover: the date, the time zone and whether network time is in use.
pub fn clock_lines(date: &str, state: Option<&State<TimedateSnapshot>>, panels: bool) -> Vec<Line> {
    let mut lines = vec![info(date)];
    match state {
        None => lines.push(info("Reading time settings…")),
        Some(Err(why)) => lines.push(info(format!("Time settings unavailable: {why}"))),
        Some(Ok(snapshot)) => {
            lines.push(info(format!("Time zone: {}", snapshot.timezone)));
            lines.push(info(ntp_text(snapshot)));
        }
    }
    lines.push(Line::Separator);
    lines.push(panel("Date & Time Preferences…", "--datetime", panels));
    lines
}

fn ntp_text(snapshot: &TimedateSnapshot) -> String {
    if !snapshot.ntp_available {
        "Network time: unavailable".into()
    } else if !snapshot.ntp_enabled {
        "Network time: off".into()
    } else if snapshot.ntp_synchronized {
        "Network time: synchronized".into()
    } else {
        "Network time: not synchronized".into()
    }
}

/// The battery popover: the charge, what the battery is doing, its energy rate,
/// its warning level and the power source. A power provider that cannot answer is
/// said to be unavailable; a machine without a battery says so.
pub fn battery_lines(state: Option<&State<PowerSnapshot>>, panels: bool) -> Vec<Line> {
    let mut lines = Vec::new();
    match state {
        None => lines.push(info("Reading power…")),
        Some(Err(why)) => lines.push(info(format!("Battery unavailable: {why}"))),
        Some(Ok(snapshot)) => match &snapshot.battery {
            None => lines.push(info("No battery")),
            Some(battery) => {
                lines.push(info(format!(
                    "Charge {:.0}%",
                    battery.percentage.clamp(0.0, 100.0)
                )));
                lines.push(info(battery_state_text(battery.status)));
                if let Some(text) = energy_rate_text(battery.status, battery.energy_rate_watts) {
                    lines.push(info(text));
                }
                match battery.warning {
                    BatteryWarning::Low => lines.push(info("Low battery")),
                    BatteryWarning::Critical => lines.push(info("Critically low battery")),
                    BatteryWarning::None | BatteryWarning::Unknown => {}
                }
                match snapshot.source {
                    PowerSource::Battery => lines.push(info("Running on battery power")),
                    PowerSource::External => lines.push(info("Plugged in")),
                    PowerSource::Unknown => {}
                }
            }
        },
    }
    lines.push(Line::Separator);
    lines.push(panel("Display Preferences…", "--display", panels));
    lines
}

fn battery_state_text(status: BatteryStatus) -> &'static str {
    match status {
        BatteryStatus::Unknown => "Battery state unknown",
        BatteryStatus::Charging => "Charging",
        BatteryStatus::Discharging => "Discharging",
        BatteryStatus::Empty => "Empty",
        BatteryStatus::Full => "Fully charged",
        BatteryStatus::PendingCharge => "Waiting to charge",
        BatteryStatus::PendingDischarge => "Waiting to discharge",
    }
}

/// The energy rate, when the provider gives a non-zero one while charging or
/// discharging.
fn energy_rate_text(status: BatteryStatus, watts: Option<f64>) -> Option<String> {
    let watts = watts.filter(|watts| *watts > 0.0)?;
    match status {
        BatteryStatus::Charging => Some(format!("Charging at {watts:.1} W")),
        BatteryStatus::Discharging => Some(format!("Using {watts:.1} W")),
        _ => None,
    }
}

/// The notification history popover: the newest notices, a count of the older
/// ones, and Clear All, which is disabled when there is nothing to clear.
pub fn notification_lines(history: &History) -> Vec<Line> {
    let mut lines = Vec::new();
    if history.is_empty() {
        lines.push(info("No notifications"));
    } else {
        for toast in history.iter().take(HISTORY_SHOWN) {
            lines.push(info(cut(&notice_text(toast), HISTORY_CHARS)));
        }
        let earlier = history.len().saturating_sub(HISTORY_SHOWN);
        if earlier > 0 {
            lines.push(info(format!("{earlier} earlier")));
        }
    }
    lines.push(Line::Separator);
    lines.push(Line::Clear {
        enabled: !history.is_empty(),
    });
    lines
}

/// "App: summary", or whichever of the two is there; a notice with no text at all
/// is still listed.
fn notice_text(toast: &Toast) -> String {
    let (app, summary) = (toast.app.trim(), toast.summary.trim());
    let text = match (app.is_empty(), summary.is_empty()) {
        (false, false) => format!("{app}: {summary}"),
        (true, false) => summary.to_owned(),
        (false, true) => app.to_owned(),
        (true, true) => toast.body.trim().to_owned(),
    };
    if text.is_empty() {
        "Notification without text".into()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slopos_core::capability::Availability;
    use slopos_services::bluetooth::{BluetoothAdapter, BluetoothDevice};
    use slopos_services::network::{NetworkAccessPoint, NetworkDevice, NetworkDeviceState};

    fn texts(lines: &[Line]) -> Vec<String> {
        lines
            .iter()
            .filter_map(|line| match line {
                Line::Info(text) => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    fn point(path: &str, ssid: &str, strength: u8) -> NetworkAccessPoint {
        NetworkAccessPoint {
            object_path: path.into(),
            ssid: ssid.as_bytes().to_vec(),
            strength,
            frequency_mhz: 2412,
            wpa_flags: 0,
            rsn_flags: 0,
        }
    }

    fn wifi_device(active: Option<&str>, points: Vec<NetworkAccessPoint>) -> NetworkDevice {
        NetworkDevice {
            object_path: "/dev/wifi".into(),
            interface: "wlan0".into(),
            ip_interface: "wlan0".into(),
            kind: NetworkDeviceType::Wifi,
            state: NetworkDeviceState::Activated,
            active_access_point: active.map(str::to_owned),
            access_points: points,
            active_connection: None,
        }
    }

    fn network(devices: Vec<NetworkDevice>) -> NetworkSnapshot {
        NetworkSnapshot {
            revision: 1,
            networking_enabled: true,
            wireless_enabled: true,
            wireless_hardware_enabled: true,
            devices,
            active_connections: Vec::new(),
        }
    }

    #[test]
    fn a_missing_network_provider_says_unavailable_with_the_reason() {
        let lines = wifi_lines(&Err("no NetworkManager".into()), true);
        assert_eq!(texts(&lines), ["Network unavailable: no NetworkManager"]);
        assert!(matches!(
            lines.last(),
            Some(Line::Panel {
                argument: "--network",
                enabled: true,
                ..
            })
        ));
    }

    #[test]
    fn the_wifi_popover_shows_the_joined_network_and_nearby_ones() {
        let snapshot = network(vec![wifi_device(
            Some("/ap/home"),
            vec![
                point("/ap/home", "Home", 70),
                point("/ap/cafe", "Cafe", 40),
                point("/ap/cafe2", "Cafe", 55),
                point("/ap/hidden", "", 90),
            ],
        )]);
        let lines = wifi_lines(&Ok(snapshot), true);
        assert_eq!(
            texts(&lines),
            ["Connected to Home", "Signal 70%", "Cafe — 55%",]
        );
    }

    #[test]
    fn the_wifi_popover_says_what_is_off_and_never_invents_a_network() {
        let mut off = network(vec![wifi_device(None, vec![point("/ap/x", "Near", 50)])]);
        off.wireless_enabled = false;
        assert_eq!(
            texts(&wifi_lines(&Ok(off.clone()), false)),
            ["Wi-Fi is off"]
        );

        off.wireless_enabled = true;
        off.wireless_hardware_enabled = false;
        assert_eq!(
            texts(&wifi_lines(&Ok(off), false)),
            ["Wi-Fi is switched off in hardware"]
        );

        let mut no_networking = network(Vec::new());
        no_networking.networking_enabled = false;
        assert_eq!(
            texts(&wifi_lines(&Ok(no_networking), false)),
            ["Networking is off"]
        );

        let none = network(Vec::new());
        assert_eq!(texts(&wifi_lines(&Ok(none), false)), ["No Wi-Fi adapter"]);

        let idle = network(vec![wifi_device(None, Vec::new())]);
        assert_eq!(
            texts(&wifi_lines(&Ok(idle), false)),
            ["Not connected to Wi-Fi"]
        );
    }

    #[test]
    fn a_long_network_name_is_cut_with_an_ellipsis() {
        let long = "x".repeat(60);
        let joined = network(vec![wifi_device(
            Some("/ap/long"),
            vec![point("/ap/long", &long, 60)],
        )]);
        let line = texts(&wifi_lines(&Ok(joined), false))[0].clone();
        assert_eq!(line.chars().count(), "Connected to ".len() + NAME_CHARS);
        assert!(line.ends_with('…'));
    }

    fn bluetooth(powered: bool, devices: Vec<BluetoothDevice>) -> BluetoothSnapshot {
        BluetoothSnapshot {
            revision: 1,
            availability: Availability::Available,
            adapters: vec![BluetoothAdapter {
                path: "/org/bluez/hci0".into(),
                name: "Radio".into(),
                address: "00:11:22:33:44:55".into(),
                powered,
                discovering: false,
            }],
            devices,
        }
    }

    fn device(name: &str, paired: bool, connected: bool) -> BluetoothDevice {
        BluetoothDevice {
            path: format!("/dev/{name}"),
            name: name.into(),
            address: "AA:BB".into(),
            adapter_path: "/org/bluez/hci0".into(),
            paired,
            trusted: paired,
            connected,
        }
    }

    #[test]
    fn the_bluetooth_item_and_popover_read_the_adapter_and_devices() {
        let on = bluetooth(
            true,
            vec![
                device("Keyboard", true, true),
                device("Speaker", true, false),
            ],
        );
        assert_eq!(bluetooth_text(Some(&Ok(on.clone()))), "Keyboard");
        assert_eq!(
            texts(&bluetooth_lines(Some(&Ok(on)), true)),
            [
                "Bluetooth is on",
                "Radio",
                "Keyboard — connected",
                "1 paired, not connected",
            ]
        );
        let off = bluetooth(false, Vec::new());
        assert_eq!(bluetooth_text(Some(&Ok(off.clone()))), "Off");
        assert_eq!(
            texts(&bluetooth_lines(Some(&Ok(off)), true)),
            ["Bluetooth is off", "Radio"]
        );
    }

    #[test]
    fn a_missing_bluetooth_provider_is_unavailable_and_not_a_device_count() {
        assert_eq!(
            bluetooth_text(Some(&Err("no BlueZ".into()))),
            "Bluetooth unavailable"
        );
        assert_eq!(bluetooth_text(None), "…");
        assert_eq!(
            texts(&bluetooth_lines(Some(&Err("no BlueZ".into())), true)),
            ["Bluetooth unavailable: no BlueZ"]
        );
        assert_eq!(texts(&bluetooth_lines(None, true)), ["Reading Bluetooth…"]);
        let none = BluetoothSnapshot {
            revision: 1,
            availability: Availability::UnsupportedHardware,
            adapters: Vec::new(),
            devices: Vec::new(),
        };
        assert_eq!(bluetooth_text(Some(&Ok(none.clone()))), "No adapter");
        assert_eq!(
            texts(&bluetooth_lines(Some(&Ok(none)), true)),
            ["No Bluetooth adapter"]
        );
    }

    #[test]
    fn the_clock_popover_shows_zone_and_network_time_or_why_not() {
        let synced = TimedateSnapshot {
            revision: 1,
            timezone: "Europe/Berlin".into(),
            ntp_available: true,
            ntp_enabled: true,
            ntp_synchronized: true,
            time_unix_usec: 0,
        };
        assert_eq!(
            texts(&clock_lines(
                "Mon 1 Jan, 12:00",
                Some(&Ok(synced.clone())),
                true
            )),
            [
                "Mon 1 Jan, 12:00",
                "Time zone: Europe/Berlin",
                "Network time: synchronized",
            ]
        );
        let mut unsynced = synced.clone();
        unsynced.ntp_synchronized = false;
        assert_eq!(ntp_text(&unsynced), "Network time: not synchronized");
        unsynced.ntp_enabled = false;
        assert_eq!(ntp_text(&unsynced), "Network time: off");
        unsynced.ntp_available = false;
        assert_eq!(ntp_text(&unsynced), "Network time: unavailable");

        assert_eq!(
            texts(&clock_lines(
                "Mon",
                Some(&Err("no timedated".into())),
                false
            )),
            ["Mon", "Time settings unavailable: no timedated"]
        );
        assert_eq!(
            texts(&clock_lines("Mon", None, false)),
            ["Mon", "Reading time settings…"]
        );
    }

    fn battery_snapshot(
        percentage: f64,
        status: BatteryStatus,
        warning: BatteryWarning,
        watts: Option<f64>,
    ) -> PowerSnapshot {
        PowerSnapshot {
            revision: 1,
            availability: Availability::Available,
            battery: Some(slopos_services::power::Battery {
                percentage,
                status,
                warning,
                energy_rate_watts: watts,
            }),
            source: PowerSource::Battery,
            suspend: slopos_services::power::SuspendCapability::Available,
            lid_present: false,
            lid_closed: None,
        }
    }

    #[test]
    fn the_battery_popover_shows_the_provider_state_and_nothing_more() {
        let discharging = battery_snapshot(
            42.4,
            BatteryStatus::Discharging,
            BatteryWarning::None,
            Some(8.4),
        );
        assert_eq!(
            texts(&battery_lines(Some(&Ok(discharging)), false)),
            [
                "Charge 42%",
                "Discharging",
                "Using 8.4 W",
                "Running on battery power"
            ]
        );
        let charging = PowerSnapshot {
            source: PowerSource::External,
            ..battery_snapshot(100.0, BatteryStatus::Full, BatteryWarning::None, Some(0.0))
        };
        // A zero or missing rate is not shown, and "Full" has no rate line.
        assert_eq!(
            texts(&battery_lines(Some(&Ok(charging)), false)),
            ["Charge 100%", "Fully charged", "Plugged in"]
        );
        let low = battery_snapshot(
            9.0,
            BatteryStatus::Charging,
            BatteryWarning::Critical,
            Some(15.0),
        );
        let lines = texts(&battery_lines(Some(&Ok(low)), false));
        assert!(lines.contains(&"Charging at 15.0 W".to_string()));
        assert!(lines.contains(&"Critically low battery".to_string()));
    }

    #[test]
    fn a_missing_power_provider_is_unavailable_and_a_machine_without_a_battery_says_so() {
        assert_eq!(
            texts(&battery_lines(
                Some(&Err("UPower is not running".into())),
                false
            )),
            ["Battery unavailable: UPower is not running"]
        );
        assert_eq!(texts(&battery_lines(None, false)), ["Reading power…"]);
        let no_battery = PowerSnapshot {
            revision: 1,
            availability: Availability::Available,
            battery: None,
            source: PowerSource::External,
            suspend: slopos_services::power::SuspendCapability::Available,
            lid_present: false,
            lid_closed: None,
        };
        assert_eq!(
            texts(&battery_lines(Some(&Ok(no_battery)), false)),
            ["No battery"]
        );
    }

    #[test]
    fn the_battery_popover_ends_with_the_display_panel_entry() {
        let lines = battery_lines(Some(&Err("gone".into())), true);
        assert_eq!(
            lines.last(),
            Some(&Line::Panel {
                label: "Display Preferences…",
                argument: "--display",
                enabled: true,
            })
        );
    }

    fn notice(id: u32, summary: &str) -> Toast {
        Toast {
            id,
            app: "Mail".into(),
            summary: summary.into(),
            body: String::new(),
            actions: Vec::new(),
            timeout_ms: Some(1000),
        }
    }

    #[test]
    fn an_empty_history_says_so_and_has_nothing_to_clear() {
        let lines = notification_lines(&History::new());
        assert_eq!(
            lines,
            [
                Line::Info("No notifications".into()),
                Line::Separator,
                Line::Clear { enabled: false },
            ]
        );
    }

    #[test]
    fn the_history_lists_the_newest_notices_counts_the_rest_and_clears_all() {
        let mut history = History::with_limit(20);
        for id in 1..=10 {
            history.record(notice(id, &format!("message {id}")));
        }
        let lines = notification_lines(&history);
        // Newest first: message 10 is the first line, eight are listed.
        assert_eq!(lines[0], Line::Info("Mail: message 10".into()));
        assert_eq!(lines[7], Line::Info("Mail: message 3".into()));
        assert_eq!(lines[8], Line::Info("2 earlier".into()));
        assert_eq!(lines[9], Line::Separator);
        assert_eq!(lines[10], Line::Clear { enabled: true });
        assert_eq!(lines.len(), 11);
    }

    #[test]
    fn a_notice_without_an_app_or_summary_is_still_listed_and_long_text_is_cut() {
        let mut history = History::with_limit(5);
        history.record(Toast {
            app: String::new(),
            summary: String::new(),
            body: "only the body".into(),
            ..notice(1, "")
        });
        history.record(notice(2, &"x".repeat(300)));
        let shown = texts(&notification_lines(&history));
        // Newest first: the long notice, then the one that has only its body.
        assert_eq!(shown.len(), 2);
        assert!(shown[0].starts_with("Mail: xxx"));
        assert_eq!(shown[0].chars().count(), HISTORY_CHARS);
        assert!(shown[0].ends_with('…'));
        assert_eq!(shown[1], "only the body");
    }
}
