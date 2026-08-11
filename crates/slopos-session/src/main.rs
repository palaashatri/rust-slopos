//! SLOPOS-I X11 session supervisor.

use std::env;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

static RUNNING: AtomicBool = AtomicBool::new(true);

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("Starting SLOPOS-I X11 session supervisor");

    let display = env::var("DISPLAY").unwrap_or_else(|_| ":0".to_string());
    env::set_var("DISPLAY", &display);

    unsafe {
        libc::signal(libc::SIGINT, sig_handler as *const () as usize);
        libc::signal(libc::SIGTERM, sig_handler as *const () as usize);
    }

    let openbox_config = resolve_openbox_config();
    let shell_exe = resolve_sibling("slopos-shell").unwrap_or_else(|| PathBuf::from("slopos-shell"));

    let mut wm_child = spawn_openbox(openbox_config.as_deref());
    let mut shell_child = spawn_path(&shell_exe, &[]);

    while RUNNING.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(750));

        let restart_wm = match wm_child.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(Some(_))),
            None => true,
        };
        if restart_wm {
            log::warn!("Openbox is not running; restarting");
            wm_child = spawn_openbox(openbox_config.as_deref());
        }

        let restart_shell = match shell_child.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(Some(_))),
            None => true,
        };
        if restart_shell {
            log::warn!("SLOPOS shell is not running; restarting");
            shell_child = spawn_path(&shell_exe, &[]);
        }
    }

    log::info!("Stopping SLOPOS-I session");
    if let Some(mut child) = shell_child { let _ = child.kill(); }
    if let Some(mut child) = wm_child { let _ = child.kill(); }
}

fn spawn_openbox(config: Option<&Path>) -> Option<Child> {
    let mut cmd = Command::new("openbox");
    cmd.arg("--replace");
    if let Some(path) = config {
        cmd.arg("--config-file").arg(path);
    }
    match cmd.spawn() {
        Ok(child) => {
            log::info!("Spawned Openbox{}", config.map(|p| format!(" with {}", p.display())).unwrap_or_default());
            Some(child)
        }
        Err(err) => {
            log::error!("Failed to spawn Openbox: {err}");
            None
        }
    }
}

fn spawn_path(path: &Path, args: &[&str]) -> Option<Child> {
    match Command::new(path).args(args).spawn() {
        Ok(child) => Some(child),
        Err(err) => {
            log::error!("Failed to spawn {}: {err}", path.display());
            None
        }
    }
}

fn resolve_sibling(name: &str) -> Option<PathBuf> {
    let exe = env::current_exe().ok()?;
    let candidate = exe.parent()?.join(name);
    candidate.exists().then_some(candidate)
}

fn resolve_openbox_config() -> Option<PathBuf> {
    if let Ok(value) = env::var("SLOPOS_OPENBOX_CONFIG") {
        let path = PathBuf::from(value);
        if path.exists() { return Some(path); }
    }

    let candidates = [
        PathBuf::from("assets/config/openbox/rc.xml"),
        PathBuf::from("/usr/local/share/slopos-i/openbox/rc.xml"),
        PathBuf::from("/usr/share/slopos-i/openbox/rc.xml"),
    ];
    candidates.into_iter().find(|path| path.exists())
}

extern "C" fn sig_handler(_sig: libc::c_int) {
    RUNNING.store(false, Ordering::SeqCst);
}
