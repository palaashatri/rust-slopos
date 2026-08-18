//! Battery and power state adapter using UPower D-Bus and sysfs fallback.

use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct BatteryState {
    pub percentage: u8,
    pub is_charging: bool,
    pub is_plugged: bool,
}

pub fn query_battery_state() -> Option<BatteryState> {
    // 1. Try reading directly from sysfs (in-process, fastest on Linux)
    if let Some(state) = query_sysfs_battery() {
        return Some(state);
    }

    // 2. Try UPower D-Bus via zbus if available
    if let Some(state) = query_upower_dbus() {
        return Some(state);
    }

    None
}

fn query_sysfs_battery() -> Option<BatteryState> {
    let power_supply = Path::new("/sys/class/power_supply");
    if !power_supply.is_dir() {
        return None;
    }

    let entries = fs::read_dir(power_supply).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("BAT") {
            let path = entry.path();
            let capacity_file = path.join("capacity");
            let status_file = path.join("status");

            if let Ok(cap_text) = fs::read_to_string(&capacity_file) {
                if let Ok(percentage) = cap_text.trim().parse::<u8>() {
                    let status = fs::read_to_string(&status_file).unwrap_or_default();
                    let is_charging = status.trim().eq_ignore_ascii_case("Charging");
                    let is_plugged = is_charging || status.trim().eq_ignore_ascii_case("Full");

                    return Some(BatteryState {
                        percentage: percentage.min(100),
                        is_charging,
                        is_plugged,
                    });
                }
            }
        }
    }
    None
}

fn query_upower_dbus() -> Option<BatteryState> {
    let connection = zbus::blocking::Connection::system().ok()?;
    let proxy = zbus::blocking::Proxy::new(
        &connection,
        "org.freedesktop.UPower",
        "/org/freedesktop/UPower/devices/DisplayDevice",
        "org.freedesktop.UPower.Device",
    )
    .ok()?;

    let is_present: bool = proxy.get_property("IsPresent").ok()?;
    if !is_present {
        return None;
    }

    let percentage: f64 = proxy.get_property("Percentage").ok()?;
    let state_num: u32 = proxy.get_property("State").ok().unwrap_or(0);
    // 1: Charging, 2: Discharging, 4: Fully charged
    let is_charging = state_num == 1;
    let is_plugged = state_num == 1 || state_num == 4;

    Some(BatteryState {
        percentage: (percentage.round() as u8).min(100),
        is_charging,
        is_plugged,
    })
}

pub fn battery_label_text(state: Option<&BatteryState>) -> Option<String> {
    state.map(|s| {
        if s.is_charging {
            format!("⚡{}%", s.percentage)
        } else {
            format!("{}%", s.percentage)
        }
    })
}
