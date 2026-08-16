//! SLOPOS-I X11 desktop shell entry point.

mod app_finder;
mod appmenu;
mod dock;
mod launcher;
mod notifications;
mod topbar;

use dock::Dock;
use gtk::prelude::*;
use gtk::{CssProvider, StyleContext};
use launcher::Launcher;
use notifications::NotificationServer;
use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use topbar::TopBar;

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
    load_css_theme();

    NotificationServer::start();
    let launcher = Launcher::new();
    install_launcher_signal_bridge(launcher.clone());

    let _topbar = TopBar::new(launcher.clone());
    let _dock = Dock::new(launcher);

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
    let lock_path = runtime_dir.join("slopos-shell.lock");
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

fn current_appearance() -> &'static str {
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    if let Some(config_home) = config_home {
        if let Ok(value) = std::fs::read_to_string(config_home.join("slopos-i/appearance")) {
            if value.trim().eq_ignore_ascii_case("graphite") {
                return "graphite";
            }
        }
    }
    "platinum"
}

fn load_css_theme() {
    let graphite = current_appearance() == "graphite";
    let installed_theme = if graphite {
        "slopos-gtk-graphite"
    } else {
        "slopos-gtk"
    };
    let source_css = if graphite {
        "assets/config/gtk-3.0/gtk-graphite.css"
    } else {
        "assets/config/gtk-3.0/gtk.css"
    };

    let mut css_paths = Vec::new();
    if let Ok(share_dir) = std::env::var("SLOPOS_SHARE_DIR") {
        css_paths.push(
            PathBuf::from(share_dir)
                .join("themes")
                .join(installed_theme)
                .join("gtk-3.0/gtk.css"),
        );
    }
    css_paths.extend([
        PathBuf::from(source_css),
        PathBuf::from(format!("/etc/slopos-i/gtk-3.0/{installed_theme}.css")),
        PathBuf::from(format!(
            "/usr/local/share/themes/{installed_theme}/gtk-3.0/gtk.css"
        )),
        PathBuf::from(format!(
            "/usr/share/themes/{installed_theme}/gtk-3.0/gtk.css"
        )),
    ]);

    for path in css_paths {
        if !path.exists() {
            continue;
        }
        let provider = CssProvider::new();
        let Some(path_text) = path.to_str() else {
            log::error!("SLOPOS GTK CSS path is not valid UTF-8: {}", path.display());
            return;
        };
        match provider.load_from_path(path_text) {
            Ok(()) => {
                if let Some(screen) = gdk::Screen::default() {
                    StyleContext::add_provider_for_screen(
                        &screen,
                        &provider,
                        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
                    );
                    log::info!(
                        "Loaded SLOPOS {} GTK CSS from {}",
                        current_appearance(),
                        path.display()
                    );
                    return;
                }
                log::error!("GTK has no default screen while loading {}", path.display());
                return;
            }
            Err(error) => {
                log::error!(
                    "Failed to parse SLOPOS GTK CSS at {}: {error}",
                    path.display()
                );
                return;
            }
        }
    }

    log::warn!("SLOPOS GTK CSS was not found; falling back to host GTK theme");
}
