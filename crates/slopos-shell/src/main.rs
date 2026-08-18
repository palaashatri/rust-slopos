//! SLOPOS-I X11 desktop shell entry point.

pub mod app_finder;
pub mod dock;
pub mod gmenu;
pub mod launcher;
pub mod menu;
pub mod notifications;
pub mod services;
pub mod shortcuts;
pub mod theme;
pub mod topbar;
pub mod x11;

use dock::Dock;
use launcher::Launcher;
use notifications::NotificationServer;
use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use topbar::TopBar;
use x11::X11EventBus;

static TOGGLE_LAUNCHER: AtomicBool = AtomicBool::new(false);

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
    let dock = Dock::new(launcher);

    // Initialize the event-driven X11 integration layer
    #[allow(deprecated)]
    let (event_sender, event_receiver) = glib::MainContext::channel(glib::Priority::default());
    let topbar_c = topbar.clone();
    let dock_c = dock.clone();

    event_receiver.attach(None, move |event| {
        topbar_c.handle_x11_event(&event);
        dock_c.handle_x11_event(&event);
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

    if std::env::var_os("SLOPOS_QA_NO_WELCOME").is_none() {
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

fn install_launcher_signal_bridge(launcher: Rc<Launcher>) {
    unsafe {
        libc::signal(libc::SIGUSR1, launcher_signal_handler as *const () as usize);
    }

    glib::timeout_add_local(Duration::from_millis(80), move || {
        if TOGGLE_LAUNCHER.swap(false, Ordering::SeqCst) {
            launcher.toggle();
        }
        glib::ControlFlow::Continue
    });
}

extern "C" fn launcher_signal_handler(_sig: libc::c_int) {
    TOGGLE_LAUNCHER.store(true, Ordering::SeqCst);
}
