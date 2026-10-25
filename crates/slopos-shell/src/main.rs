//! SLOPOS-I X11 desktop shell entry point.

pub mod app_finder;
pub mod app_index;
pub mod desktop_files;
pub mod launcher;
pub mod menu;
pub mod notifications;
pub mod services;
pub mod shortcuts;
pub mod theme;
pub mod topbar;
pub mod x11;

use app_index::AppIndex;
use launcher::Launcher;
use notifications::NotificationServer;
use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::rc::Rc;
use topbar::TopBar;
use x11::X11EventBus;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let _instance_guard = match acquire_instance_guard() {
        Ok(guard) => guard,
        Err(error) => {
            log::error!("Refusing to start a second SLOPOS shell: {error}");
            return;
        }
    };

    log::info!("Starting SLOPOS-I desktop shell (X11)");
    gtk::init().expect("Failed to initialize GTK3");
    theme::load_css_theme();

    NotificationServer::start();
    let launcher = Launcher::new();
    install_launcher_signal_bridge(launcher.clone());

    let topbar = TopBar::new(launcher.clone());
    shortcuts::install_system_menu_shortcut();

    // Initialize the persistent background application index worker.
    #[allow(deprecated)]
    let (app_index_sender, app_index_receiver) =
        glib::MainContext::channel(glib::Priority::default());
    let launcher_c = launcher.clone();
    app_index_receiver.attach(None, move |update| {
        launcher_c.update_index(update);
        glib::ControlFlow::Continue
    });

    let _app_index = AppIndex::start(move |update| {
        let _ = app_index_sender.send(update);
    });

    // Initialize the persistent event-driven D-Bus and audio monitor.
    #[allow(deprecated)]
    let (status_sender, status_receiver) = glib::MainContext::channel(glib::Priority::default());
    let topbar_status = topbar.clone();
    status_receiver.attach(None, move |status: services::SystemStatus| {
        if let Some(warning) = status.power_notification {
            let (summary, icon) = if warning.critical {
                ("Battery critically low", "battery-low-symbolic")
            } else {
                ("Battery low", "battery-low-symbolic")
            };
            NotificationServer::show_toast(
                summary,
                &format!("Battery has {}% charge remaining.", warning.percentage),
                icon,
            );
        }
        topbar_status.update_system_status(&status);
        glib::ControlFlow::Continue
    });

    let _system_monitor = services::SystemMonitor::start(move |status| {
        let _ = status_sender.send(status);
    });

    // Initialize the event-driven X11 integration layer.
    #[allow(deprecated)]
    let (event_sender, event_receiver) = glib::MainContext::channel(glib::Priority::default());
    let topbar_c = topbar.clone();

    event_receiver.attach(None, move |event| {
        topbar_c.handle_x11_event(&event);
        glib::ControlFlow::Continue
    });

    let _event_bus = match X11EventBus::start(move |event| {
        let _ = event_sender.send(event);
    }) {
        Ok(bus) => Some(bus),
        Err(error) => {
            log::warn!("Failed to initialize long-lived X11 event bus: {error}");
            None
        }
    };

    if std::env::var_os("SLOPOS_QA_NO_WELCOME").is_none() && first_login() {
        NotificationServer::show_toast(
            "Welcome to SLOPOS-I",
            "Press Super+Space or choose Search to find applications.",
            "",
        );
    }

    gtk::main();
}

fn acquire_instance_guard() -> Result<File, String> {
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&runtime_dir)
        .map_err(|error| format!("create runtime directory: {error}"))?;
    let display_suffix = std::env::var("DISPLAY")
        .unwrap_or_default()
        .replace(':', "_");
    let lock_path = runtime_dir.join(format!("slopos-shell{display_suffix}.lock"));
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|error| format!("open {}: {error}", lock_path.display()))?;

    let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if result != 0 {
        return Err(format!("{} is already locked", lock_path.display()));
    }
    Ok(file)
}

/// `SIGUSR1` toggles the launcher (Openbox binds Super+Space to
/// `pkill -USR1 -x slopos-shell`).
fn install_launcher_signal_bridge(launcher: Rc<Launcher>) {
    glib::unix_signal_add_local(libc::SIGUSR1, move || {
        launcher.toggle();
        glib::ControlFlow::Continue
    });
}

/// True the first time the shell starts for this user; records that it has.
fn first_login() -> bool {
    let Some(marker) = slopos_core::xdg::XdgPaths::from_environment()
        .ok()
        .map(|paths| paths.state.join("slopos-i/welcomed"))
    else {
        return false;
    };
    if marker.exists() {
        return false;
    }
    if let Some(parent) = marker.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&marker, "").is_ok()
}
