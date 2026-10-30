//! SLOPOS-I X11 session supervisor.
//!
//! Starts and supervises Openbox, an optional compositor and the SLOPOS shell
//! for one X display. `slopos-session --logout` ends the running session.

mod environment;
mod supervisor;

use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::FileExt;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use supervisor::{Child, Policy};

static RUNNING: AtomicBool = AtomicBool::new(true);

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    match env::args().nth(1).as_deref() {
        Some("--logout") => return logout(),
        Some("--paint-desktop") => {
            // Repaint after an appearance or wallpaper change; no session lock.
            environment::apply_root_background();
            return ExitCode::SUCCESS;
        }
        Some("--version") => {
            println!("slopos-session {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Some("--help" | "-h") => {
            println!(
                "Usage: slopos-session [--logout | --paint-desktop | --version]\n\n\
                 Without arguments, supervise Openbox and the SLOPOS shell on $DISPLAY.\n\
                 --logout         end the SLOPOS session running on $DISPLAY\n\
                 --paint-desktop  repaint the desktop picture on $DISPLAY"
            );
            return ExitCode::SUCCESS;
        }
        Some(other) => {
            eprintln!("slopos-session: unknown argument {other:?} (try --help)");
            return ExitCode::from(2);
        }
        None => {}
    }

    let _session_lock = match SessionLock::acquire() {
        Ok(lock) => lock,
        Err(error) => {
            log::error!("{error}");
            return ExitCode::FAILURE;
        }
    };
    log::info!("Starting SLOPOS-I X11 session supervisor");

    environment::configure();
    install_signal_handlers();
    restore_settings();
    environment::apply_root_background();

    let mut wm = Child::new("Openbox", Policy::Required, spawn_openbox);
    let mut compositor = Child::new("X11 compositor", Policy::Optional, spawn_compositor);
    // Give Openbox a moment to own the screen before the shell maps its bar.
    thread::sleep(Duration::from_millis(150));
    // The desktop icons sit on the root picture the session painted above.
    let mut desktop = env::var_os("SLOPOS_NO_DESKTOP")
        .is_none()
        .then(|| Child::new("SLOPOS desktop", Policy::Optional, spawn_desktop));
    let mut shell = Child::new("SLOPOS shell", Policy::Important, spawn_shell);
    let mut dock = env::var_os("SLOPOS_NO_DOCK")
        .is_none()
        .then(|| Child::new("SLOPOS Dock", Policy::Important, spawn_dock));

    while RUNNING.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(250));
        if let Err(error) = wm.supervise() {
            // Without a window manager the X session is unusable; end it so
            // the display manager can offer a fresh login.
            log::error!("{error}");
            break;
        }
        if let Err(error) = compositor.supervise() {
            log::warn!("{error}; continuing without a compositor");
        }
        if let Err(error) = shell.supervise() {
            // Keep the session (and the user's open applications) alive. The
            // Openbox root menu still offers Log Out.
            log::error!("{error}; continuing without the shell");
        }
        if let Some(Err(error)) = dock.as_mut().map(Child::supervise) {
            log::error!("{error}; continuing without the Dock");
        }
        if let Some(Err(error)) = desktop.as_mut().map(Child::supervise) {
            log::warn!("{error}; the desktop picture stays, without icons");
        }
    }

    log::info!("Stopping SLOPOS-I session");
    if let Some(dock) = dock.as_mut() {
        dock.stop();
    }
    if let Some(desktop) = desktop.as_mut() {
        desktop.stop();
    }
    shell.stop();
    compositor.stop();
    wm.stop();
    ExitCode::SUCCESS
}

fn spawn_openbox() -> Option<std::process::Child> {
    let config = environment::openbox_config();
    let mut command = std::process::Command::new("openbox");
    command.arg("--replace");
    if let Some(path) = &config {
        command.arg("--config-file").arg(path);
    }
    match command.spawn() {
        Ok(child) => {
            match &config {
                Some(path) => log::info!("Started Openbox with {}", path.display()),
                None => log::info!("Started Openbox with its default configuration"),
            }
            Some(child)
        }
        Err(error) => {
            log::error!("Could not start Openbox: {error}");
            None
        }
    }
}

/// Put back the display layout, keyboard and mouse settings kept in
/// Control Panels, before anything sizes itself to the screen. Missing or
/// slow, the session goes on.
fn restore_settings() {
    let path = slopos_core::process::resolve_program("slopos-control-panels")
        .unwrap_or_else(|| PathBuf::from("slopos-control-panels"));
    let mut child = match std::process::Command::new(&path).arg("--restore").spawn() {
        Ok(child) => child,
        Err(error) => {
            log::info!("Not restoring display and input settings: {error}");
            return;
        }
    };
    let (sender, receiver) = std::sync::mpsc::channel();
    let id = child.id();
    thread::spawn(move || {
        let _ = sender.send(child.wait());
    });
    match receiver.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(status)) if status.success() => {}
        Ok(Ok(status)) => log::warn!("Restoring display and input settings failed ({status})"),
        Ok(Err(error)) => log::warn!("Restoring display and input settings failed: {error}"),
        Err(_) => {
            log::warn!("Restoring display and input settings took too long; going on");
            // SAFETY: plain kill(2) on the child we started.
            unsafe { libc::kill(id as libc::pid_t, libc::SIGKILL) };
        }
    }
}

fn spawn_shell() -> Option<std::process::Child> {
    spawn_component("slopos-topbar")
}

fn spawn_desktop() -> Option<std::process::Child> {
    spawn_component("slopos-desktop")
}

fn spawn_dock() -> Option<std::process::Child> {
    spawn_component("slopos-dock")
}

/// Start a SLOPOS component, preferring the copy next to this binary.
fn spawn_component(name: &str) -> Option<std::process::Child> {
    let path = slopos_core::process::resolve_program(name).unwrap_or_else(|| PathBuf::from(name));
    match std::process::Command::new(&path).spawn() {
        Ok(child) => Some(child),
        Err(error) => {
            log::error!("Could not start {}: {error}", path.display());
            None
        }
    }
}

/// Compositors in preference order. A compositor is optional: Openbox and the
/// shell work without one, so a missing compositor is not a failure.
fn spawn_compositor() -> Option<std::process::Child> {
    if env::var_os("SLOPOS_NO_COMPOSITOR").is_some() {
        return None;
    }
    let candidates: &[(&str, &[&str])] = &[
        (
            "picom",
            &[
                "--backend",
                "xrender",
                "--shadow",
                "--shadow-radius",
                "10",
                "--shadow-opacity",
                "0.22",
                "--shadow-offset-x",
                "-5",
                "--shadow-offset-y",
                "-5",
                "--shadow-exclude",
                "window_type = 'dock' || window_type = 'desktop' || class_g = 'slopos-topbar'",
            ],
        ),
        ("xcompmgr", &["-c"]),
    ];
    let (path, args) = slopos_core::process::first_available(candidates)?;
    match std::process::Command::new(&path).args(args).spawn() {
        Ok(child) => {
            log::info!("Started compositor {}", path.display());
            Some(child)
        }
        Err(error) => {
            log::warn!("Could not start compositor {}: {error}", path.display());
            None
        }
    }
}

/// One supervisor per X display. The lock file also records the supervisor's
/// pid so `--logout` can find it.
struct SessionLock {
    _file: File,
    path: PathBuf,
}

impl SessionLock {
    fn acquire() -> Result<Self, String> {
        let path = session_pid_path();
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)
                .map_err(|error| format!("create {}: {error}", dir.display()))?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|error| format!("open {}: {error}", path.display()))?;
        // SAFETY: flock on an owned, open descriptor.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(format!(
                "a SLOPOS session is already running on this display ({})",
                path.display()
            ));
        }
        file.set_len(0)
            .and_then(|()| writeln!(file, "{}", std::process::id()))
            .map_err(|error| format!("write {}: {error}", path.display()))?;
        Ok(Self { _file: file, path })
    }
}

impl Drop for SessionLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn session_pid_path() -> PathBuf {
    let runtime = env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .unwrap_or_else(env::temp_dir);
    let display = env::var("DISPLAY").unwrap_or_default();
    runtime.join(format!("slopos-session{}.pid", display_suffix(&display)))
}

fn display_suffix(display: &str) -> String {
    display
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// True when a session holds the lock on `file`. A successful non-blocking
/// acquire means the file is no longer locked, so it is released immediately.
fn lock_is_held(file: &File) -> bool {
    // SAFETY: flock on an owned, open descriptor.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        // SAFETY: this descriptor owns the lock we just acquired.
        unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) };
        return false;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EWOULDBLOCK)
}

fn session_pid(file: &File) -> Option<i32> {
    let mut buffer = [0_u8; 32];
    let length = file.read_at(&mut buffer, 0).ok()?;
    std::str::from_utf8(&buffer[..length])
        .ok()?
        .trim()
        .parse::<i32>()
        .ok()
        .filter(|pid| *pid > 1)
}

fn logout() -> ExitCode {
    let path = session_pid_path();
    let Ok(file) = OpenOptions::new().read(true).open(&path) else {
        eprintln!(
            "slopos-session: no SLOPOS session is running on this display ({})",
            path.display()
        );
        return ExitCode::FAILURE;
    };
    if !lock_is_held(&file) {
        eprintln!(
            "slopos-session: no SLOPOS session is running on this display ({})",
            path.display()
        );
        return ExitCode::FAILURE;
    }
    let Some(pid) = session_pid(&file) else {
        eprintln!(
            "slopos-session: {} does not record a session pid",
            path.display()
        );
        return ExitCode::FAILURE;
    };
    // Pin the process before signalling, so a recycled pid cannot receive the
    // signal. pidfd_open is available on every kernel SLOPOS targets; libc
    // exposes the syscall number rather than a wrapper.
    // SAFETY: pidfd_open takes (pid, flags); the kernel reports errors through
    // the -1 return value and errno.
    let pidfd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid as libc::c_long, 0) };
    if pidfd < 0 {
        eprintln!(
            "slopos-session: could not open a handle for session {pid}: {}",
            std::io::Error::last_os_error()
        );
        return ExitCode::FAILURE;
    }
    let pidfd = pidfd as libc::c_int;
    // The session may have exited between the lock check and pidfd_open, in
    // which case the pid could now belong to an unrelated process. Confirm
    // that the same session still holds the lock before sending anything.
    if !lock_is_held(&file) || session_pid(&file) != Some(pid) {
        // SAFETY: close an owned descriptor.
        unsafe { libc::close(pidfd) };
        eprintln!("slopos-session: session {pid} exited before it could be signalled");
        return ExitCode::FAILURE;
    }
    // SAFETY: pidfd refers to the pinned session process.
    let sent = unsafe {
        libc::syscall(
            libc::SYS_pidfd_send_signal,
            pidfd as libc::c_long,
            libc::SIGTERM as libc::c_long,
            std::ptr::null_mut::<libc::siginfo_t>(),
            0,
        )
    };
    let error = std::io::Error::last_os_error();
    // SAFETY: close an owned descriptor.
    unsafe { libc::close(pidfd) };
    if sent != 0 {
        eprintln!("slopos-session: could not signal session {pid}: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn install_signal_handlers() {
    // SAFETY: the handler only stores to an atomic, which is async-signal-safe.
    unsafe {
        for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
            libc::signal(signal, stop_handler as *const () as usize);
        }
    }
}

extern "C" fn stop_handler(_signal: libc::c_int) {
    RUNNING.store(false, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_names_become_safe_file_suffixes() {
        assert_eq!(display_suffix(":0"), "_0");
        assert_eq!(display_suffix("localhost:10.0"), "localhost_10_0");
        assert_eq!(display_suffix("../x"), "___x");
    }

    #[test]
    fn session_lock_detection_tracks_the_holder() {
        let path = std::env::temp_dir().join(format!(
            "slopos-session-lock-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_file(&path);
        let mut writer = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&path)
            .unwrap();
        writeln!(writer, "4242").unwrap();
        let reader = OpenOptions::new().read(true).open(&path).unwrap();
        assert!(!lock_is_held(&reader));
        assert_eq!(session_pid(&reader), Some(4242));

        // SAFETY: flock on the descriptor that owns the test lock.
        assert_eq!(
            unsafe { libc::flock(writer.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
            0
        );
        assert!(lock_is_held(&reader));

        drop(writer);
        assert!(!lock_is_held(&reader));
        let _ = fs::remove_file(&path);
    }
}
