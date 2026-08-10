use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionAction {
    Login,
    Logout,
    Lock,
    Unlock,
    Shutdown,
    Restart,
    Sleep,
}

/// High-level state of the user session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// No user is logged in.
    LoggedOut,
    /// A user is logged in and the desktop is accessible.
    Active,
    /// A user is logged in but the screen is locked.
    Locked,
}

pub struct SessionManager {
    pub logged_in: bool,
    pub username: String,
    pub autologin: bool,
    pub restore_windows: bool,
    pub locked: bool,
    pub pending_action: Option<SessionAction>,
    pub session_state: HashMap<String, String>,
    /// Structured session state enum derived from `logged_in` and `locked`.
    pub state: SessionState,
    /// Unix timestamp (seconds since epoch) recorded at login.
    pub login_timestamp: Option<u64>,
    lock_on_sleep: bool,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            logged_in: false,
            username: String::new(),
            autologin: false,
            restore_windows: true,
            locked: false,
            pending_action: None,
            session_state: HashMap::new(),
            state: SessionState::LoggedOut,
            login_timestamp: None,
            lock_on_sleep: true,
        }
    }

    pub fn login(&mut self, username: &str) {
        self.logged_in = true;
        self.locked = false;
        self.state = SessionState::Active;
        self.pending_action = Some(SessionAction::Login);
        self.username = username.to_string();
        self.login_timestamp = Some(unix_now());
        self.save_state();
    }

    /// Transition the session to the Locked state without ending it.
    /// A separate authentication step is required to return to Active.
    pub fn lock_screen(&mut self) {
        if self.logged_in {
            self.locked = true;
            self.state = SessionState::Locked;
            self.pending_action = Some(SessionAction::Lock);
            self.save_state();
        }
    }

    pub fn logout(&mut self) {
        self.logout_without_exit();
        std::process::exit(0);
    }

    /// Clear session state without terminating the process (caller may exit).
    pub fn logout_without_exit(&mut self) {
        self.logged_in = false;
        self.locked = false;
        self.state = SessionState::LoggedOut;
        self.login_timestamp = None;
        self.pending_action = Some(SessionAction::Logout);
        self.save_state();
    }

    pub fn lock(&mut self) {
        self.lock_screen();
    }

    pub fn unlock(&mut self) {
        self.locked = false;
        self.state = if self.logged_in {
            SessionState::Active
        } else {
            SessionState::LoggedOut
        };
        self.pending_action = Some(SessionAction::Unlock);
    }

    pub fn sleep(&mut self) {
        if self.lock_on_sleep {
            self.lock_screen();
        }
        self.pending_action = Some(SessionAction::Sleep);
    }

    pub fn shutdown(&mut self) {
        self.state = SessionState::LoggedOut;
        self.pending_action = Some(SessionAction::Shutdown);
        self.save_state();
    }

    pub fn restart(&mut self) {
        self.state = SessionState::LoggedOut;
        self.pending_action = Some(SessionAction::Restart);
        self.save_state();
    }

    pub fn save_state(&mut self) {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let config_dir = std::path::PathBuf::from(home).join(".config/slopos-i");
        let _ = std::fs::create_dir_all(&config_dir);
        let path = config_dir.join("session.toml");

        let mut content = String::new();
        content.push_str("[session]\n");
        content.push_str(&format!(
            "username = \"{}\"\n",
            escape_toml_string(&self.username)
        ));
        content.push_str(&format!("logged_in = {}\n", self.logged_in));
        content.push_str(&format!("locked = {}\n", self.locked));
        content.push_str(&format!("restore_windows = {}\n", self.restore_windows));
        if let Some(ts) = self.login_timestamp {
            content.push_str(&format!("login_timestamp = {}\n", ts));
        }
        for (k, v) in &self.session_state {
            content.push_str(&format!("{} = \"{}\"\n", k, escape_toml_string(v)));
        }
        let _ = std::fs::write(path, content);
    }

    pub fn restore_state(&mut self) {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let path = std::path::PathBuf::from(home).join(".config/slopos-i/session.toml");
        if let Ok(content) = std::fs::read_to_string(path) {
            for line in content.lines() {
                if let Some(pos) = line.find('=') {
                    let key = line[..pos].trim();
                    let val = line[pos + 1..].trim().trim_matches('"');
                    if key == "username" {
                        self.username = val.to_string();
                    } else if key == "logged_in" {
                        self.logged_in = val.parse().unwrap_or(false);
                    } else if key == "locked" {
                        self.locked = val.parse().unwrap_or(false);
                    } else if key == "restore_windows" {
                        self.restore_windows = val.parse().unwrap_or(true);
                    } else if key == "login_timestamp" {
                        self.login_timestamp = val.parse().ok();
                    } else if key != "[session]" {
                        self.session_state.insert(key.to_string(), val.to_string());
                    }
                }
            }
        }
        // Reconstruct the SessionState enum from the persisted boolean fields
        self.state = match (self.logged_in, self.locked) {
            (true, true) => SessionState::Locked,
            (true, false) => SessionState::Active,
            _ => SessionState::LoggedOut,
        };
    }
}

/// Returns the current time as seconds since the Unix epoch.
fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn escape_toml_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Read the battery charge level (0–100) from the system.
/// Returns `None` on desktop machines, VMs, or any system without a battery.
///
/// Prefers UPower (Linux) then `/sys` BAT0 — see [`crate::power::battery_info`].
pub fn battery_percentage() -> Option<u8> {
    crate::power::battery_percentage()
}

/// Returns `true` when the system is running on battery (i.e. not plugged in).
pub fn is_on_battery() -> bool {
    crate::power::is_on_battery()
}

/// Returns the machine hostname, falling back to `"slopos-i"` if unavailable.
pub fn hostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|h| h.trim().to_string())
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "slopos-i".to_string())
}

/// Returns the system uptime in whole seconds by reading `/proc/uptime`.
/// Returns 0 when the file is not available (non-Linux systems).
pub fn uptime_seconds() -> u64 {
    std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|content| {
            content
                .split_whitespace()
                .next()
                .and_then(|first| first.parse::<f64>().ok())
        })
        .map(|secs| secs as u64)
        .unwrap_or(0)
}

/// Returns `(used_kb, total_kb)` by parsing `/proc/meminfo`.
/// Both values are 0 when the file is not available.
pub fn memory_usage() -> (u64, u64) {
    let content = match std::fs::read_to_string("/proc/meminfo") {
        Ok(c) => c,
        Err(_) => return (0, 0),
    };

    let mut total_kb: u64 = 0;
    let mut free_kb: u64 = 0;
    let mut buffers_kb: u64 = 0;
    let mut cached_kb: u64 = 0;
    let mut sreclaimable_kb: u64 = 0;

    for line in content.lines() {
        let mut parts = line.split_whitespace();
        let key = parts.next().unwrap_or("");
        let value: u64 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
        match key {
            "MemTotal:" => total_kb = value,
            "MemFree:" => free_kb = value,
            "Buffers:" => buffers_kb = value,
            "Cached:" => cached_kb = value,
            "SReclaimable:" => sreclaimable_kb = value,
            _ => {}
        }
    }

    let used_kb = total_kb
        .saturating_sub(free_kb)
        .saturating_sub(buffers_kb)
        .saturating_sub(cached_kb)
        .saturating_sub(sreclaimable_kb);

    (used_kb, total_kb)
}
