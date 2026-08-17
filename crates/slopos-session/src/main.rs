//! SLOPOS-I X11 session supervisor.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

static RUNNING: AtomicBool = AtomicBool::new(true);
const HEALTHY_RUNTIME: Duration = Duration::from_secs(30);
const MAX_FAST_FAILURES: u32 = 5;

struct ManagedChild {
    name: &'static str,
    child: Option<Child>,
    launched_at: Instant,
    fast_failures: u32,
    restart_after: Instant,
}

impl ManagedChild {
    fn new(name: &'static str, child: Option<Child>) -> Self {
        Self {
            name,
            child,
            launched_at: Instant::now(),
            fast_failures: 0,
            restart_after: Instant::now(),
        }
    }

    fn poll(&mut self) -> Result<bool, String> {
        if let Some(child) = self.child.as_mut() {
            match child.try_wait() {
                Ok(None) => return Ok(false),
                Ok(Some(status)) => self.record_exit(status)?,
                Err(error) => {
                    self.child = None;
                    self.record_failure(format!("wait failed: {error}"))?;
                }
            }
        }
        Ok(Instant::now() >= self.restart_after)
    }

    fn replace(&mut self, child: Option<Child>) -> Result<(), String> {
        self.launched_at = Instant::now();
        self.child = child;
        if self.child.is_none() {
            self.record_failure("spawn failed".to_string())?;
        }
        Ok(())
    }

    fn record_exit(&mut self, status: ExitStatus) -> Result<(), String> {
        self.child = None;
        self.record_failure(format!("exited with {status}"))
    }

    fn record_failure(&mut self, reason: String) -> Result<(), String> {
        if self.launched_at.elapsed() >= HEALTHY_RUNTIME {
            self.fast_failures = 0;
        }
        self.fast_failures = self.fast_failures.saturating_add(1);
        if self.fast_failures >= MAX_FAST_FAILURES {
            return Err(format!(
                "{} failed {} times in quick succession ({reason})",
                self.name, self.fast_failures
            ));
        }

        let delay_ms = 250_u64.saturating_mul(1_u64 << self.fast_failures.min(4));
        self.restart_after = Instant::now() + Duration::from_millis(delay_ms);
        log::warn!(
            "{} {reason}; restart {} of {} in {}ms",
            self.name,
            self.fast_failures,
            MAX_FAST_FAILURES,
            delay_ms
        );
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("Starting SLOPOS-I X11 session supervisor");

    configure_session_environment();
    install_signal_handlers();
    apply_desktop_fallback();

    let shell_exe =
        resolve_sibling("slopos-shell").unwrap_or_else(|| PathBuf::from("slopos-shell"));
    let initial_openbox_config = resolve_openbox_config();
    let mut wm = ManagedChild::new("Openbox", spawn_openbox(initial_openbox_config.as_deref()));
    thread::sleep(Duration::from_millis(150));
    apply_desktop_fallback();
    let mut shell = ManagedChild::new("SLOPOS shell", spawn_path(&shell_exe, &[]));

    while RUNNING.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(250));

        match wm.poll() {
            Ok(true) => {
                // Resolve again after every restart. `slopos-appearance`
                // intentionally restarts Openbox so the new Platinum/Graphite
                // window chrome is picked up without ending the session.
                let openbox_config = resolve_openbox_config();
                if let Err(error) = wm.replace(spawn_openbox(openbox_config.as_deref())) {
                    log::error!("{error}");
                    break;
                }
                thread::sleep(Duration::from_millis(100));
                apply_desktop_fallback();
            }
            Ok(false) => {}
            Err(error) => {
                log::error!("{error}");
                break;
            }
        }

        match shell.poll() {
            Ok(true) => {
                if let Err(error) = shell.replace(spawn_path(&shell_exe, &[])) {
                    log::error!("{error}");
                    break;
                }
            }
            Ok(false) => {}
            Err(error) => {
                log::error!("{error}");
                break;
            }
        }
    }

    log::info!("Stopping SLOPOS-I session");
    shell.stop();
    wm.stop();
}

fn configure_session_environment() {
    let display = env::var("DISPLAY").unwrap_or_else(|_| ":0".to_string());
    env::set_var("DISPLAY", display);
    env::set_var("XDG_SESSION_TYPE", "x11");
    env::set_var("XDG_CURRENT_DESKTOP", "SLOPOS");
    env::set_var("XDG_SESSION_DESKTOP", "slopos");
    env::set_var("DESKTOP_SESSION", "slopos");
    env::set_var("SLOPOS_SESSION_MANAGED", "1");
    configure_install_prefix_environment();

    let _ = Command::new("dbus-update-activation-environment")
        .args([
            "--systemd",
            "DISPLAY",
            "XDG_SESSION_TYPE",
            "XDG_CURRENT_DESKTOP",
            "XDG_SESSION_DESKTOP",
            "DESKTOP_SESSION",
        ])
        .status();
}

fn install_signal_handlers() {
    unsafe {
        libc::signal(libc::SIGINT, sig_handler as *const () as usize);
        libc::signal(libc::SIGTERM, sig_handler as *const () as usize);
    }
}

fn appearance() -> String {
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    if let Some(config_home) = config_home {
        if let Ok(value) = fs::read_to_string(config_home.join("slopos-i/appearance")) {
            let v = value.trim();
            if v.eq_ignore_ascii_case("oled") {
                return "oled".to_string();
            }
            if v.eq_ignore_ascii_case("graphite") {
                return "graphite".to_string();
            }
            if v.eq_ignore_ascii_case("classic") {
                return "classic".to_string();
            }
        }
    }
    "platinum".to_string()
}

fn apply_desktop_fallback() {
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));

    if let Some(config_home) = &config_home {
        let wallpaper_file = config_home.join("slopos-i/wallpaper");
        if let Ok(path) = fs::read_to_string(wallpaper_file) {
            let path = path.trim();
            if Path::new(path).is_file()
                && Command::new("feh")
                    .args(["--bg-fill", path])
                    .status()
                    .is_ok()
            {
                return;
            }
        }
    }

    let color = match appearance().as_str() {
        "oled" => "#000000",
        "graphite" => "#25272B",
        "classic" => "#808080",
        _ => "#758090",
    };
    if let Err(error) = Command::new("xsetroot").args(["-solid", color]).status() {
        log::debug!("xsetroot is unavailable: {error}");
    }
}

fn sync_openbox_menu() {
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    if let Some(config_home) = config_home {
        let target_dir = config_home.join("openbox");
        let target = target_dir.join("menu.xml");
        if !target.exists() {
            if let Some(source) = resolve_openbox_menu() {
                let _ = std::fs::create_dir_all(&target_dir);
                let _ = std::fs::copy(source, target);
            }
        }
    }
}

fn resolve_openbox_menu() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(share_dir) = env::var("SLOPOS_SHARE_DIR") {
        candidates.push(
            PathBuf::from(share_dir)
                .join("slopos-i/openbox")
                .join("menu.xml"),
        );
    }
    if let Ok(executable) = env::current_exe() {
        if let Some(prefix) = executable.parent().and_then(Path::parent) {
            candidates.push(prefix.join("share/slopos-i/openbox").join("menu.xml"));
        }
    }
    candidates.extend([
        PathBuf::from("assets/config/openbox/menu.xml"),
        PathBuf::from("/usr/local/share/slopos-i/openbox/menu.xml"),
        PathBuf::from("/usr/share/slopos-i/openbox/menu.xml"),
    ]);
    candidates.into_iter().find(|path| path.exists())
}

fn spawn_openbox(config: Option<&Path>) -> Option<Child> {
    sync_openbox_menu();
    let mut command = Command::new("openbox");
    command.arg("--replace");
    if let Some(path) = config {
        command.arg("--config-file").arg(path);
    }
    match command.spawn() {
        Ok(child) => {
            let suffix = config
                .map(|path| format!(" with {}", path.display()))
                .unwrap_or_default();
            log::info!("Spawned Openbox{suffix}");
            Some(child)
        }
        Err(error) => {
            log::error!("Failed to spawn Openbox: {error}");
            None
        }
    }
}

fn spawn_path(path: &Path, args: &[&str]) -> Option<Child> {
    match Command::new(path).args(args).spawn() {
        Ok(child) => Some(child),
        Err(error) => {
            log::error!("Failed to spawn {}: {error}", path.display());
            None
        }
    }
}

fn resolve_sibling(name: &str) -> Option<PathBuf> {
    let executable = env::current_exe().ok()?;
    let candidate = executable.parent()?.join(name);
    candidate.exists().then_some(candidate)
}

fn resolve_openbox_config() -> Option<PathBuf> {
    if let Ok(value) = env::var("SLOPOS_OPENBOX_CONFIG") {
        let path = PathBuf::from(value);
        if path.exists() {
            return Some(path);
        }
    }

    let file_name = match appearance().as_str() {
        "oled" => "rc-oled.xml",
        "graphite" => "rc-graphite.xml",
        "classic" => "rc-classic.xml",
        _ => "rc.xml",
    };
    let mut candidates = Vec::new();
    if let Ok(share_dir) = env::var("SLOPOS_SHARE_DIR") {
        candidates.push(
            PathBuf::from(share_dir)
                .join("slopos-i/openbox")
                .join(file_name),
        );
    }
    if let Ok(executable) = env::current_exe() {
        if let Some(prefix) = executable.parent().and_then(Path::parent) {
            candidates.push(prefix.join("share/slopos-i/openbox").join(file_name));
        }
    }
    candidates.extend([
        PathBuf::from("assets/config/openbox").join(file_name),
        PathBuf::from("/usr/local/share/slopos-i/openbox").join(file_name),
        PathBuf::from("/usr/share/slopos-i/openbox").join(file_name),
    ]);
    candidates.into_iter().find(|path| path.exists())
}

fn configure_install_prefix_environment() {
    let Some(executable) = env::current_exe().ok() else {
        return;
    };
    let Some(bin_dir) = executable.parent() else {
        return;
    };
    let Some(prefix) = bin_dir.parent() else {
        return;
    };
    let share_dir = prefix.join("share");
    if !share_dir.is_dir() {
        return;
    }

    prepend_path("PATH", bin_dir);
    if env::var_os("SLOPOS_SHARE_DIR").is_none() {
        env::set_var("SLOPOS_SHARE_DIR", &share_dir);
    }
    prepend_env_path("XDG_DATA_DIRS", &share_dir, "/usr/local/share:/usr/share");
    prepend_env_path("XDG_CONFIG_DIRS", &share_dir.join("slopos-i"), "/etc/xdg");
}

fn prepend_path(variable: &str, directory: &Path) {
    prepend_env_path(variable, directory, "");
}

fn prepend_env_path(variable: &str, directory: &Path, default: &str) {
    let directory = directory.to_string_lossy();
    let current = env::var(variable).unwrap_or_else(|_| default.to_string());
    let prefix = format!(":{current}:");
    if prefix.contains(&format!(":{directory}:")) {
        return;
    }
    let value = if current.is_empty() {
        directory.to_string()
    } else {
        format!("{directory}:{current}")
    };
    env::set_var(variable, value);
}

extern "C" fn sig_handler(_sig: libc::c_int) {
    RUNNING.store(false, Ordering::SeqCst);
}
