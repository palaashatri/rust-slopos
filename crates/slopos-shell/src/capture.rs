//! Screenshot (PNG) and screen recording (ffmpeg) helpers.
//!
//! Screenshot tries, in order: ImageMagick `import`, then `ffmpeg` x11grab
//! one frame. Recording uses `ffmpeg` x11grab and tracks a single child
//! process for start/stop.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("no capture tool found (need import or ffmpeg)")]
    ToolNotFound,
    #[error("capture command failed: {0}")]
    CommandFailed(String),
    #[error("recording already in progress")]
    AlreadyRecording,
    #[error("no active recording")]
    NotRecording,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

struct RecordingState {
    child: Child,
    path: PathBuf,
}

static RECORDING: Mutex<Option<RecordingState>> = Mutex::new(None);

/// Preferred output directory: `~/Pictures` when present, else `/tmp`.
pub fn capture_output_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        let pictures = PathBuf::from(&home).join("Pictures");
        if pictures.is_dir() {
            return pictures;
        }
        // Create Pictures when HOME is set and writable.
        if std::fs::create_dir_all(&pictures).is_ok() {
            return pictures;
        }
    }
    PathBuf::from("/tmp")
}

fn timestamp_slug() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

fn display_spec() -> String {
    std::env::var("DISPLAY").unwrap_or_else(|_| ":0.0".to_string())
}

fn command_exists(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Capture the root window to a PNG under Pictures (or /tmp).
pub fn take_screenshot() -> Result<PathBuf, CaptureError> {
    let dir = capture_output_dir();
    let path = dir.join(format!("SLOPOS-I-Screenshot-{}.png", timestamp_slug()));
    take_screenshot_to(&path)?;
    Ok(path)
}

/// Capture into an explicit path (used by tests with parsers only; still real tools).
pub fn take_screenshot_to(path: &Path) -> Result<(), CaptureError> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    if command_exists("import") {
        let output = Command::new("import")
            .args(["-window", "root"])
            .arg(path)
            .output()?;
        if output.status.success() && path.exists() {
            return Ok(());
        }
        tracing::debug!("import failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    if command_exists("ffmpeg") {
        let display = display_spec();
        let output = Command::new("ffmpeg")
            .args([
                "-y",
                "-loglevel",
                "error",
                "-f",
                "x11grab",
                "-i",
                &display,
                "-frames:v",
                "1",
                "-update",
                "1",
            ])
            .arg(path)
            .output()?;
        if output.status.success() && path.exists() {
            return Ok(());
        }
        return Err(CaptureError::CommandFailed(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }

    // Last resort on systems with xwd only: write XWD then hope convert exists.
    if command_exists("xwd") {
        let xwd_path = path.with_extension("xwd");
        let output = Command::new("xwd")
            .args(["-root", "-out"])
            .arg(&xwd_path)
            .output()?;
        if !output.status.success() {
            return Err(CaptureError::CommandFailed(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }
        if command_exists("convert") {
            let conv = Command::new("convert").arg(&xwd_path).arg(path).output()?;
            let _ = std::fs::remove_file(&xwd_path);
            if conv.status.success() && path.exists() {
                return Ok(());
            }
        }
        // Leave the .xwd if we cannot convert; surface as failure for PNG contract.
        let _ = std::fs::remove_file(&xwd_path);
    }

    Err(CaptureError::ToolNotFound)
}

/// Start ffmpeg x11grab recording. Returns the output file path.
pub fn start_recording() -> Result<PathBuf, CaptureError> {
    let mut guard = RECORDING.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_some() {
        return Err(CaptureError::AlreadyRecording);
    }
    if !command_exists("ffmpeg") {
        return Err(CaptureError::ToolNotFound);
    }

    let dir = capture_output_dir();
    let path = dir.join(format!("SLOPOS-I-Recording-{}.mp4", timestamp_slug()));
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let display = display_spec();
    let child = Command::new("ffmpeg")
        .args([
            "-y",
            "-loglevel",
            "error",
            "-f",
            "x11grab",
            "-i",
            &display,
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| CaptureError::CommandFailed(e.to_string()))?;

    *guard = Some(RecordingState {
        child,
        path: path.clone(),
    });
    Ok(path)
}

/// Stop the active recording, if any. Returns the file path when one was running.
pub fn stop_recording() -> Result<PathBuf, CaptureError> {
    let mut guard = RECORDING.lock().unwrap_or_else(|e| e.into_inner());
    let Some(mut state) = guard.take() else {
        return Err(CaptureError::NotRecording);
    };

    // Graceful stop: SIGINT lets ffmpeg finalize the container.
    #[cfg(unix)]
    {
        let _ = send_signal(state.child.id(), libc_sigint());
        // Give ffmpeg a moment; fall through to kill if needed.
        std::thread::sleep(std::time::Duration::from_millis(300));
    }

    match state.child.try_wait() {
        Ok(Some(_)) => {}
        Ok(None) => {
            let _ = state.child.kill();
            let _ = state.child.wait();
        }
        Err(_) => {
            let _ = state.child.kill();
            let _ = state.child.wait();
        }
    }

    Ok(state.path)
}

/// Whether a recording child is tracked.
pub fn is_recording() -> bool {
    RECORDING
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .is_some()
}

#[cfg(unix)]
fn libc_sigint() -> i32 {
    2 // SIGINT
}

#[cfg(unix)]
fn send_signal(pid: u32, sig: i32) -> std::io::Result<()> {
    // Avoid libc crate: use kill(2) via nix-less raw command.
    let status = Command::new("kill")
        .args([format!("-{sig}"), pid.to_string()])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("kill failed"))
    }
}

/// Build a default screenshot filename (pure helper for tests).
pub fn screenshot_filename(timestamp_secs: u64) -> String {
    format!("SLOPOS-I-Screenshot-{timestamp_secs}.png")
}

/// Build a default recording filename (pure helper for tests).
pub fn recording_filename(timestamp_secs: u64) -> String {
    format!("SLOPOS-I-Recording-{timestamp_secs}.mp4")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filenames() {
        assert_eq!(screenshot_filename(123), "SLOPOS-I-Screenshot-123.png");
        assert_eq!(recording_filename(456), "SLOPOS-I-Recording-456.mp4");
    }

    #[test]
    fn output_dir_is_absolute() {
        let dir = capture_output_dir();
        assert!(dir.is_absolute() || dir == std::path::Path::new("/tmp") || dir.starts_with("/"));
    }

    #[test]
    fn not_recording_stop_errors() {
        // Ensure no leftover recording from parallel tests in this process.
        if is_recording() {
            let _ = stop_recording();
        }
        assert!(!is_recording());
        assert!(matches!(stop_recording(), Err(CaptureError::NotRecording)));
    }
}
