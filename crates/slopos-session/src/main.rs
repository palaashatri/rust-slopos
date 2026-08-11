//! SLOPOS-I X11 Session Supervisor
//!
//! Supervises the X11 desktop session:
//! - Ensures X11 display connection.
//! - Launches Openbox stacking window manager.
//! - Launches SLOPOS desktop shell (`slopos-shell`).
//! - Manages session lifecycle (lock, logout, suspend, reboot, shutdown).

use std::env;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

static RUNNING: AtomicBool = AtomicBool::new(true);

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("Starting SLOPOS-I X11 Session Supervisor");

    // Ensure DISPLAY is set
    let display = env::var("DISPLAY").unwrap_or_else(|_| {
        log::warn!("DISPLAY not set, defaulting to :0");
        ":0".to_string()
    });
    env::set_var("DISPLAY", &display);

    // Setup signal handlers
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc_setup(r);

    // Step 1: Start Window Manager (Openbox)
    let mut wm_child = spawn_service("openbox", &["--replace"]);
    log::info!("Spawned Openbox window manager");

    // Step 2: Start SLOPOS Shell
    let shell_exe = get_shell_executable();
    let mut shell_child = spawn_service(&shell_exe, &[]);
    log::info!("Spawned SLOPOS Shell ({})", shell_exe);

    // Main supervisor loop
    while RUNNING.load(Ordering::SeqCst) && running.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(500));

        // Check if WM process died, restart if running
        if let Some(ref mut child) = wm_child {
            if let Ok(Some(status)) = child.try_wait() {
                log::warn!("Openbox exited with status {:?}, restarting...", status);
                wm_child = spawn_service("openbox", &["--replace"]);
            }
        }

        // Check if shell died, restart if running
        if let Some(ref mut child) = shell_child {
            if let Ok(Some(status)) = child.try_wait() {
                log::warn!("slopos-shell exited with status {:?}, restarting...", status);
                shell_child = spawn_service(&shell_exe, &[]);
            }
        }
    }

    log::info!("Shutting down SLOPOS-I Session");
    if let Some(mut child) = shell_child {
        let _ = child.kill();
    }
    if let Some(mut child) = wm_child {
        let _ = child.kill();
    }
}

fn spawn_service(cmd: &str, args: &[&str]) -> Option<Child> {
    match Command::new(cmd).args(args).spawn() {
        Ok(child) => Some(child),
        Err(e) => {
            log::error!("Failed to spawn {}: {}", cmd, e);
            None
        }
    }
}

fn get_shell_executable() -> String {
    if let Ok(exe) = env::current_exe() {
        if let Some(parent) = exe.parent() {
            let candidate = parent.join("slopos-shell");
            if candidate.exists() {
                return candidate.to_string_lossy().to_string();
            }
        }
    }
    "slopos-shell".to_string()
}

fn ctrlc_setup(running: Arc<AtomicBool>) {
    unsafe {
        libc::signal(libc::SIGINT, sig_handler as *const () as usize);
        libc::signal(libc::SIGTERM, sig_handler as *const () as usize);
    }
    let _ = running;
}

extern "C" fn sig_handler(_sig: libc::c_int) {
    RUNNING.store(false, Ordering::SeqCst);
}
