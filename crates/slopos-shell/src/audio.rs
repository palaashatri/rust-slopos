//! System volume get/set via PipeWire (`wpctl`) or PulseAudio (`pactl`) CLI.
//!
//! Designed for reliability inside Docker/desktop Linux without a hard
//! D-Bus dependency. Pure parsers are unit-tested on every host.

use std::process::Command;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("no volume control tool found (need pactl or wpctl)")]
    ToolNotFound,
    #[error("volume tool failed: {0}")]
    CommandFailed(String),
    #[error("could not parse volume from tool output")]
    ParseError,
}

/// Backend used for volume control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioBackend {
    Pactl,
    Wpctl,
}

/// Detect an available CLI backend (`pactl` preferred, then `wpctl`).
pub fn detect_backend() -> Option<AudioBackend> {
    if command_exists("pactl") {
        Some(AudioBackend::Pactl)
    } else if command_exists("wpctl") {
        Some(AudioBackend::Wpctl)
    } else {
        None
    }
}

fn command_exists(name: &str) -> bool {
    Command::new(name)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or_else(|_| {
            // Some tools exit non-zero for --version; presence of the binary is enough.
            Command::new("which")
                .arg(name)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        })
}

/// Current default sink volume as 0–100 percent.
pub fn get_volume() -> Result<u8, AudioError> {
    match detect_backend() {
        Some(AudioBackend::Pactl) => {
            let output = run_capture("pactl", &["get-sink-volume", "@DEFAULT_SINK@"])?;
            parse_pactl_volume(&output).ok_or(AudioError::ParseError)
        }
        Some(AudioBackend::Wpctl) => {
            let output = run_capture("wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"])?;
            parse_wpctl_volume(&output).ok_or(AudioError::ParseError)
        }
        None => Err(AudioError::ToolNotFound),
    }
}

/// Pure argv for `pactl set-sink-volume` (unit-tested; not executed).
pub fn volume_pactl_set_plan(percent: u8) -> Vec<String> {
    let percent = percent.min(100);
    vec![
        "pactl".into(),
        "set-sink-volume".into(),
        "@DEFAULT_SINK@".into(),
        format!("{percent}%"),
    ]
}

/// Pure argv for `wpctl set-volume` (unit-tested; not executed).
pub fn volume_wpctl_set_plan(percent: u8) -> Vec<String> {
    let percent = percent.min(100);
    let linear = f64::from(percent) / 100.0;
    vec![
        "wpctl".into(),
        "set-volume".into(),
        "@DEFAULT_AUDIO_SINK@".into(),
        format!("{linear:.2}"),
    ]
}

/// Pure argv for `pactl get-sink-volume` (unit-tested; not executed).
pub fn volume_pactl_get_plan() -> Vec<String> {
    vec![
        "pactl".into(),
        "get-sink-volume".into(),
        "@DEFAULT_SINK@".into(),
    ]
}

/// Pure argv for `wpctl get-volume` (unit-tested; not executed).
pub fn volume_wpctl_get_plan() -> Vec<String> {
    vec![
        "wpctl".into(),
        "get-volume".into(),
        "@DEFAULT_AUDIO_SINK@".into(),
    ]
}

/// Compact menu-bar / status label for volume (pure).
///
/// `None` → unavailable placeholder; `Some(p)` → `"🔊 p%"`.
pub fn volume_status_label(percent: Option<u8>) -> String {
    match percent {
        Some(p) => format!("🔊 {}%", p.min(100)),
        None => "🔊 —".to_string(),
    }
}

/// Set default sink volume to `percent` (clamped to 0–100).
///
/// Uses the same argv as [`volume_pactl_set_plan`] / [`volume_wpctl_set_plan`].
pub fn set_volume(percent: u8) -> Result<(), AudioError> {
    let percent = percent.min(100);
    match detect_backend() {
        Some(AudioBackend::Pactl) => {
            let plan = volume_pactl_set_plan(percent);
            let args: Vec<&str> = plan[1..].iter().map(String::as_str).collect();
            run_status(&plan[0], &args)
        }
        Some(AudioBackend::Wpctl) => {
            let plan = volume_wpctl_set_plan(percent);
            let args: Vec<&str> = plan[1..].iter().map(String::as_str).collect();
            run_status(&plan[0], &args)
        }
        None => Err(AudioError::ToolNotFound),
    }
}

fn run_capture(cmd: &str, args: &[&str]) -> Result<String, AudioError> {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| AudioError::CommandFailed(e.to_string()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AudioError::CommandFailed(stderr.trim().to_string()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn run_status(cmd: &str, args: &[&str]) -> Result<(), AudioError> {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| AudioError::CommandFailed(e.to_string()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AudioError::CommandFailed(stderr.trim().to_string()));
    }
    Ok(())
}

/// Parse `pactl get-sink-volume` output.
///
/// Example line:
/// `Volume: front-left: 32768 /  50% / -18.06 dB,   front-right: 32768 /  50% / -18.06 dB`
///
/// Returns the first percentage found (left channel), clamped to 0–100.
pub fn parse_pactl_volume(output: &str) -> Option<u8> {
    for token in output.split_whitespace() {
        let token = token.trim_end_matches(',');
        if let Some(num) = token.strip_suffix('%') {
            if let Ok(v) = num.parse::<u32>() {
                return Some(v.min(100) as u8);
            }
        }
    }
    None
}

/// Parse `wpctl get-volume` output.
///
/// Examples:
/// - `Volume: 0.50`
/// - `Volume: 0.50 [MUTED]`
/// - `Volume: 1.00`
pub fn parse_wpctl_volume(output: &str) -> Option<u8> {
    let line = output.lines().next()?.trim();
    let rest = line
        .strip_prefix("Volume:")
        .or_else(|| line.strip_prefix("volume:"))?
        .trim();
    let first = rest.split_whitespace().next()?;
    let frac: f64 = first.parse().ok()?;
    if !frac.is_finite() || frac < 0.0 {
        return None;
    }
    let pct = (frac * 100.0).round();
    Some(pct.clamp(0.0, 100.0) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pactl_typical() {
        let sample = "Volume: front-left: 32768 /  50% / -18.06 dB,   front-right: 32768 /  50% / -18.06 dB\n\
                      balance 0.00\n";
        assert_eq!(parse_pactl_volume(sample), Some(50));
    }

    #[test]
    fn parse_pactl_100() {
        let sample =
            "Volume: front-left: 65536 / 100% / 0.00 dB,   front-right: 65536 / 100% / 0.00 dB\n";
        assert_eq!(parse_pactl_volume(sample), Some(100));
    }

    #[test]
    fn parse_pactl_zero() {
        let sample = "Volume: front-left: 0 /   0% / -inf dB,   front-right: 0 /   0% / -inf dB\n";
        assert_eq!(parse_pactl_volume(sample), Some(0));
    }

    #[test]
    fn parse_pactl_empty() {
        assert_eq!(parse_pactl_volume(""), None);
        assert_eq!(parse_pactl_volume("no percents here"), None);
    }

    #[test]
    fn parse_wpctl_typical() {
        assert_eq!(parse_wpctl_volume("Volume: 0.50\n"), Some(50));
        assert_eq!(parse_wpctl_volume("Volume: 0.50 [MUTED]\n"), Some(50));
        assert_eq!(parse_wpctl_volume("Volume: 1.00\n"), Some(100));
        assert_eq!(parse_wpctl_volume("Volume: 0.00\n"), Some(0));
        assert_eq!(parse_wpctl_volume("Volume: 0.255\n"), Some(26));
    }

    #[test]
    fn parse_wpctl_invalid() {
        assert_eq!(parse_wpctl_volume(""), None);
        assert_eq!(parse_wpctl_volume("Volume:\n"), None);
        assert_eq!(parse_wpctl_volume("Volume: abc\n"), None);
        assert_eq!(parse_wpctl_volume("not volume\n"), None);
    }

    #[test]
    fn get_volume_without_tools_is_err() {
        // On macOS CI there is typically no pactl/wpctl; ensure we fail cleanly.
        if detect_backend().is_none() {
            assert!(matches!(get_volume(), Err(AudioError::ToolNotFound)));
            assert!(matches!(set_volume(50), Err(AudioError::ToolNotFound)));
        }
    }

    #[test]
    fn volume_set_plans_match_shipped_backends() {
        let pactl = volume_pactl_set_plan(42);
        assert_eq!(
            pactl,
            vec!["pactl", "set-sink-volume", "@DEFAULT_SINK@", "42%"]
        );
        assert_eq!(
            volume_pactl_set_plan(200).last().map(String::as_str),
            Some("100%")
        );

        let wpctl = volume_wpctl_set_plan(50);
        assert_eq!(
            wpctl,
            vec!["wpctl", "set-volume", "@DEFAULT_AUDIO_SINK@", "0.50"]
        );
        assert_eq!(
            volume_wpctl_set_plan(0).last().map(String::as_str),
            Some("0.00")
        );
    }

    #[test]
    fn volume_get_plans_are_stable() {
        assert_eq!(
            volume_pactl_get_plan(),
            vec!["pactl", "get-sink-volume", "@DEFAULT_SINK@"]
        );
        assert_eq!(
            volume_wpctl_get_plan(),
            vec!["wpctl", "get-volume", "@DEFAULT_AUDIO_SINK@"]
        );
    }

    #[test]
    fn volume_status_label_pure() {
        assert_eq!(volume_status_label(Some(75)), "🔊 75%");
        assert_eq!(volume_status_label(Some(0)), "🔊 0%");
        assert_eq!(volume_status_label(Some(150)), "🔊 100%");
        assert_eq!(volume_status_label(None), "🔊 —");
    }
}
