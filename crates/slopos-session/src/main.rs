//! SLOPOS-I X11 session supervisor.

use std::env;
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

    let openbox_config = resolve_openbox_config();
    let shell_exe =
        resolve_sibling("slopos-shell").unwrap_or_else(|| PathBuf::from("slopos-shell"));

    let mut wm = ManagedChild::new("Openbox", spawn_openbox(openbox_config.as_deref()));
    // Give the WM a small head start so shell windows are managed from their
    // first map rather than racing Openbox startup.
    thread::sleep(Duration::from_millis(150));
    // Openbox may reset the root window while it starts. Re-apply the
    // canonical SLOPOS desktop colour after the WM owns the display so the
    // shipping desktop never falls back to a black root background.
    apply_desktop_fallback();
    let mut shell = ManagedChild::new("SLOPOS shell", spawn_path(&shell_exe, &[]));

    while RUNNING.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(250));

        match wm.poll() {
            Ok(true) => {
                if let Err(error) = wm.replace(spawn_openbox(openbox_config.as_deref())) {
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

    // Export only the interoperable desktop/session identity to activation
    // services. SLOPOS_SESSION_MANAGED is intentionally private to SLOPOS
    // child processes and must not leak into unrelated D-Bus activations.
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

fn apply_desktop_fallback() {
    if let Err(error) = Command::new("xsetroot")
        .args(["-solid", "#758090"])
        .status()
    {
        log::debug!("xsetroot is unavailable: {error}");
    }
}

fn spawn_openbox(config: Option<&Path>) -> Option<Child> {
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

    let mut candidates = Vec::new();
    if let Ok(share_dir) = env::var("SLOPOS_SHARE_DIR") {
        candidates.push(PathBuf::from(share_dir).join("slopos-i/openbox/rc.xml"));
    }
    if let Ok(executable) = env::current_exe() {
        if let Some(prefix) = executable.parent().and_then(Path::parent) {
            candidates.push(prefix.join("share/slopos-i/openbox/rc.xml"));
        }
    }
    candidates.extend([
        PathBuf::from("assets/config/openbox/rc.xml"),
        PathBuf::from("/usr/local/share/slopos-i/openbox/rc.xml"),
        PathBuf::from("/usr/share/slopos-i/openbox/rc.xml"),
    ]);
    candidates.into_iter().find(|path| path.exists())
}

/// Display managers execute slopos-session directly, so the session cannot
/// rely on start-slopos-i to expose a custom prefix. Derive that prefix from
/// the installed supervisor path and make its wrapper, desktop entries and
/// MIME defaults discoverable to shell children.
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
