use parking_lot::Mutex;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

static CLIPBOARD_CONTENT: Mutex<String> = Mutex::new(String::new());

pub struct Clipboard;

impl Clipboard {
    pub fn copy(text: &str) {
        let content = text.to_string();
        {
            let mut guard = CLIPBOARD_CONTENT.lock();
            *guard = content.clone();
        }

        // Write to file-based clipboard.
        if let Some(path) = clipboard_path() {
            if let Some(parent) = path.parent() {
                if let Err(err) = fs::create_dir_all(parent) {
                    log::warn!("failed to create clipboard directory: {err}");
                    return;
                }
            }
            if let Err(err) = fs::write(path, &content) {
                log::warn!("failed to write clipboard content: {err}");
            }
        }

        // Also push to system clipboard via xclip, xsel, or wl-copy.
        // Failures are silent — these tools may simply not be installed.
        if !try_write_xclip(text) && !try_write_xsel(text) {
            try_write_wl_copy(text);
        }
    }

    pub fn paste() -> String {
        // Try system clipboard tools first; use first non-empty result.
        if let Some(s) = try_read_xclip()
            .or_else(try_read_xsel)
            .or_else(try_read_wl_paste)
            .filter(|s| !s.is_empty())
        {
            let mut guard = CLIPBOARD_CONTENT.lock();
            *guard = s.clone();
            return s;
        }

        // Fall back to file-based clipboard.
        if let Some(content) = clipboard_path().and_then(|path| fs::read_to_string(path).ok()) {
            let mut guard = CLIPBOARD_CONTENT.lock();
            *guard = content.clone();
            return content;
        }

        CLIPBOARD_CONTENT.lock().clone()
    }

    pub fn clear() {
        {
            let mut guard = CLIPBOARD_CONTENT.lock();
            guard.clear();
        }

        if let Some(path) = clipboard_path() {
            if let Err(err) = fs::remove_file(path) {
                if err.kind() != std::io::ErrorKind::NotFound {
                    log::warn!("failed to clear clipboard content: {err}");
                }
            }
        }
    }

    /// Returns the names of system clipboard backends found in PATH.
    /// Useful for diagnostics / bug reports.
    pub fn available_backends() -> Vec<&'static str> {
        let candidates: &[&str] = &["xclip", "xsel", "wl-copy", "wl-paste"];
        candidates
            .iter()
            .copied()
            .filter(|tool| {
                Command::new("which")
                    .arg(tool)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false)
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Internal helpers: write to system clipboard
// ---------------------------------------------------------------------------

fn try_write_xclip(text: &str) -> bool {
    match Command::new("xclip")
        .args(["-selection", "clipboard"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(mut child) => {
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(text.as_bytes());
            }
            child.wait().map(|s| s.success()).unwrap_or(false)
        }
        Err(e) => {
            log::debug!("xclip not available: {e}");
            false
        }
    }
}

fn try_write_xsel(text: &str) -> bool {
    match Command::new("xsel")
        .args(["--clipboard", "--input"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(mut child) => {
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(text.as_bytes());
            }
            child.wait().map(|s| s.success()).unwrap_or(false)
        }
        Err(e) => {
            log::debug!("xsel not available: {e}");
            false
        }
    }
}

fn try_write_wl_copy(text: &str) -> bool {
    match Command::new("wl-copy")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(mut child) => {
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(text.as_bytes());
            }
            child.wait().map(|s| s.success()).unwrap_or(false)
        }
        Err(e) => {
            log::debug!("wl-copy not available: {e}");
            false
        }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers: read from system clipboard
// ---------------------------------------------------------------------------

fn try_read_xclip() -> Option<String> {
    Command::new("xclip")
        .args(["-selection", "clipboard", "-o"])
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
}

fn try_read_xsel() -> Option<String> {
    Command::new("xsel")
        .args(["--clipboard", "--output"])
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
}

fn try_read_wl_paste() -> Option<String> {
    Command::new("wl-paste")
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
}

fn clipboard_path() -> Option<PathBuf> {
    std::env::var_os("SLOPOS_CLIPBOARD_PATH")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("XDG_RUNTIME_DIR").map(|runtime| {
                PathBuf::from(runtime)
                    .join("slopos-i")
                    .join("clipboard.txt")
            })
        })
        .or_else(|| {
            std::env::var_os("TMPDIR")
                .map(PathBuf::from)
                .map(|tmp| tmp.join("slopos-i-clipboard.txt"))
        })
        .or_else(|| Some(std::env::temp_dir().join("slopos-i-clipboard.txt")))
}

#[cfg(test)]
mod tests {
    use super::Clipboard;
    use parking_lot::Mutex;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    // Serialize tests that mutate the SLOPOS_CLIPBOARD_PATH env var,
    // since std::env::set_var is not thread-safe across parallel test threads.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn unique_path() -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("slopos-i_clipboard_{unique}.txt"))
    }

    #[test]
    fn clipboard_persists_to_runtime_path() {
        let _guard = ENV_LOCK.lock();
        let path = unique_path();
        std::env::set_var("SLOPOS_CLIPBOARD_PATH", &path);

        Clipboard::clear();
        Clipboard::copy("hello from another app");

        assert_eq!(fs::read_to_string(&path).unwrap(), "hello from another app");
        Clipboard::clear();
        assert!(!path.exists());

        std::env::remove_var("SLOPOS_CLIPBOARD_PATH");
    }

    /// Verify that the file-based round-trip works when system tools are absent
    /// or not consulted (we force a dedicated temp path so the test is hermetic).
    #[test]
    fn file_based_round_trip() {
        let _guard = ENV_LOCK.lock();
        let path = unique_path();
        std::env::set_var("SLOPOS_CLIPBOARD_PATH", &path);

        Clipboard::clear();
        Clipboard::copy("round-trip test");

        // Read via file directly to confirm persistence independent of in-memory state.
        let on_disk = fs::read_to_string(&path).expect("clipboard file must exist after copy");
        assert_eq!(on_disk, "round-trip test");

        Clipboard::clear();
        std::env::remove_var("SLOPOS_CLIPBOARD_PATH");
    }

    /// available_backends() must always return a Vec (possibly empty on CI).
    #[test]
    fn available_backends_returns_vec() {
        let backends = Clipboard::available_backends();
        // Just ensure the call succeeds and the result is a valid (possibly empty) list.
        for name in &backends {
            assert!(!name.is_empty(), "backend name should be non-empty");
        }
        // No panic == success.
        let _ = backends;
    }
}
