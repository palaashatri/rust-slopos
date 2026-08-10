//! Power / battery status via UPower (D-Bus) with `/sys` BAT0 fallback.

/// Where battery data was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerSource {
    UPower,
    Sysfs,
    Unavailable,
}

/// Charging / discharge state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatteryState {
    Unknown,
    Charging,
    Discharging,
    Empty,
    FullyCharged,
    PendingCharge,
    PendingDischarge,
    Unavailable,
}

impl BatteryState {
    /// Map UPower `BatteryState` integer.
    pub fn from_upower_u32(value: u32) -> Self {
        match value {
            1 => Self::Charging,
            2 => Self::Discharging,
            3 => Self::Empty,
            4 => Self::FullyCharged,
            5 => Self::PendingCharge,
            6 => Self::PendingDischarge,
            _ => Self::Unknown,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::Charging => "Charging",
            Self::Discharging => "Discharging",
            Self::Empty => "Empty",
            Self::FullyCharged => "Fully charged",
            Self::PendingCharge => "Pending charge",
            Self::PendingDischarge => "Pending discharge",
            Self::Unavailable => "Unavailable",
        }
    }

    pub fn is_on_battery(self) -> bool {
        matches!(
            self,
            Self::Discharging | Self::Empty | Self::PendingDischarge
        )
    }
}

/// Battery snapshot for About / status / Settings.
#[derive(Debug, Clone, PartialEq)]
pub struct BatteryInfo {
    pub percentage: Option<u8>,
    pub on_battery: bool,
    pub state: BatteryState,
    pub source: PowerSource,
    /// True when a battery (or DisplayDevice) was present.
    pub present: bool,
}

impl BatteryInfo {
    pub fn unavailable() -> Self {
        Self {
            percentage: None,
            on_battery: false,
            state: BatteryState::Unavailable,
            source: PowerSource::Unavailable,
            present: false,
        }
    }

    pub fn summary_line(&self) -> String {
        match self.percentage {
            Some(pct) => format!("Battery: {}% ({})", pct, self.state.as_str()),
            None => "Battery: Not available".to_string(),
        }
    }
}

/// Best-effort battery info: UPower first (Linux), then `/sys` BAT0.
pub fn battery_info() -> BatteryInfo {
    #[cfg(target_os = "linux")]
    {
        if let Ok(info) = query_upower() {
            return info;
        }
    }
    if let Some(info) = query_sysfs_bat0() {
        return info;
    }
    BatteryInfo::unavailable()
}

/// Read charge level 0–100, if a battery is present.
pub fn battery_percentage() -> Option<u8> {
    battery_info().percentage
}

/// `true` when running on battery power (discharging).
pub fn is_on_battery() -> bool {
    battery_info().on_battery
}

/// Parse a sysfs capacity string (`"87\n"` → 87).
pub fn parse_sysfs_capacity(raw: &str) -> Option<u8> {
    let v = raw.trim().parse::<u32>().ok()?;
    Some(v.min(100) as u8)
}

/// Parse sysfs status text into [`BatteryState`].
pub fn parse_sysfs_status(raw: &str) -> BatteryState {
    match raw.trim().to_ascii_lowercase().as_str() {
        "charging" => BatteryState::Charging,
        "discharging" => BatteryState::Discharging,
        "full" | "not charging" => BatteryState::FullyCharged,
        "unknown" => BatteryState::Unknown,
        _ => BatteryState::Unknown,
    }
}

fn query_sysfs_bat0() -> Option<BatteryInfo> {
    let capacity_raw = std::fs::read_to_string("/sys/class/power_supply/BAT0/capacity").ok()?;
    let percentage = parse_sysfs_capacity(&capacity_raw)?;
    let status_raw =
        std::fs::read_to_string("/sys/class/power_supply/BAT0/status").unwrap_or_default();
    let state = parse_sysfs_status(&status_raw);
    Some(BatteryInfo {
        percentage: Some(percentage),
        on_battery: state.is_on_battery(),
        state,
        source: PowerSource::Sysfs,
        present: true,
    })
}

#[cfg(target_os = "linux")]
fn query_upower() -> Result<BatteryInfo, Box<dyn std::error::Error>> {
    use zbus::blocking::Connection;

    let conn = Connection::system()?;
    // DisplayDevice aggregates battery state when present.
    let device = zbus::blocking::Proxy::new(
        &conn,
        "org.freedesktop.UPower",
        "/org/freedesktop/UPower/devices/DisplayDevice",
        "org.freedesktop.UPower.Device",
    )?;

    let is_present: bool = device.get_property("IsPresent").unwrap_or(false);
    if !is_present {
        return Err("UPower DisplayDevice not present".into());
    }

    let percentage_f: f64 = device.get_property("Percentage").unwrap_or(0.0);
    let state_u: u32 = device.get_property("State").unwrap_or(0);
    let percentage = if percentage_f.is_finite() && percentage_f >= 0.0 {
        Some(percentage_f.round().clamp(0.0, 100.0) as u8)
    } else {
        None
    };
    let state = BatteryState::from_upower_u32(state_u);

    Ok(BatteryInfo {
        percentage,
        on_battery: state.is_on_battery(),
        state,
        source: PowerSource::UPower,
        present: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upower_state_mapping() {
        assert_eq!(BatteryState::from_upower_u32(1), BatteryState::Charging);
        assert_eq!(BatteryState::from_upower_u32(2), BatteryState::Discharging);
        assert_eq!(BatteryState::from_upower_u32(4), BatteryState::FullyCharged);
        assert_eq!(BatteryState::from_upower_u32(0), BatteryState::Unknown);
    }

    #[test]
    fn sysfs_capacity_parser() {
        assert_eq!(parse_sysfs_capacity("87\n"), Some(87));
        assert_eq!(parse_sysfs_capacity("100"), Some(100));
        assert_eq!(parse_sysfs_capacity("150"), Some(100));
        assert_eq!(parse_sysfs_capacity("nope"), None);
    }

    #[test]
    fn sysfs_status_parser() {
        assert_eq!(
            parse_sysfs_status("Discharging\n"),
            BatteryState::Discharging
        );
        assert_eq!(parse_sysfs_status("Charging"), BatteryState::Charging);
        assert_eq!(parse_sysfs_status("Full"), BatteryState::FullyCharged);
        assert!(parse_sysfs_status("Discharging").is_on_battery());
        assert!(!parse_sysfs_status("Charging").is_on_battery());
    }

    #[test]
    fn battery_info_safe_on_host() {
        let info = battery_info();
        let _ = info.summary_line();
        // On macOS without BAT0/UPower we expect Unavailable.
        #[cfg(not(target_os = "linux"))]
        {
            if !std::path::Path::new("/sys/class/power_supply/BAT0/capacity").exists() {
                assert_eq!(info.source, PowerSource::Unavailable);
                assert_eq!(info.percentage, None);
            }
        }
    }

    #[test]
    fn unavailable_summary() {
        assert_eq!(
            BatteryInfo::unavailable().summary_line(),
            "Battery: Not available"
        );
        let charged = BatteryInfo {
            percentage: Some(42),
            on_battery: true,
            state: BatteryState::Discharging,
            source: PowerSource::Sysfs,
            present: true,
        };
        assert_eq!(charged.summary_line(), "Battery: 42% (Discharging)");
    }
}
