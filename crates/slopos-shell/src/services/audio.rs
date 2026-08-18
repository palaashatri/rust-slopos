//! Audio status adapter.

use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioState {
    pub volume_percent: u8,
    pub is_muted: bool,
}

pub fn query_audio_state() -> Option<AudioState> {
    // 1. Try wpctl (PipeWire / WirePlumber)
    if let Ok(output) = Command::new("wpctl")
        .args(["get-volume", "@DEFAULT_AUDIO_SINK@"])
        .output()
    {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            // Example format: "Volume: 0.65 [MUTED]" or "Volume: 0.65"
            let is_muted = text.contains("[MUTED]");
            if let Some(vol_str) = text.split("Volume:").nth(1) {
                let clean = vol_str.replace("[MUTED]", "").trim().to_string();
                if let Ok(vol_float) = clean.parse::<f32>() {
                    let volume_percent = (vol_float * 100.0).round().clamp(0.0, 150.0) as u8;
                    return Some(AudioState {
                        volume_percent,
                        is_muted,
                    });
                }
            }
        }
    }

    // 2. Try pactl (PulseAudio / PipeWire-Pulse)
    if let Ok(output) = Command::new("pactl")
        .args(["get-sink-volume", "@DEFAULT_SINK@"])
        .output()
    {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            // Example format: "... /  65% / ..."
            for part in text.split('/') {
                let trimmed = part.trim();
                if let Some(pct_str) = trimmed.strip_suffix('%') {
                    if let Ok(pct) = pct_str.trim().parse::<u8>() {
                        let is_muted = Command::new("pactl")
                            .args(["get-sink-mute", "@DEFAULT_SINK@"])
                            .output()
                            .ok()
                            .map(|o| String::from_utf8_lossy(&o.stdout).contains("yes"))
                            .unwrap_or(false);

                        return Some(AudioState {
                            volume_percent: pct,
                            is_muted,
                        });
                    }
                }
            }
        }
    }

    None
}

pub fn audio_label_text(state: Option<&AudioState>) -> String {
    match state {
        Some(s) if s.is_muted => "Mute".to_string(),
        Some(s) => format!("{}%", s.volume_percent),
        None => "—".to_string(),
    }
}
