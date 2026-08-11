//! SLOPOS-I X11 desktop shell entry point.

mod app_finder;
mod dock;
mod launcher;
mod notifications;
mod topbar;

use dock::Dock;
use gtk::prelude::*;
use gtk::{CssProvider, StyleContext};
use launcher::Launcher;
use notifications::NotificationServer;
use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use topbar::TopBar;

static TOGGLE_LAUNCHER: AtomicBool = AtomicBool::new(false);

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
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
            "dialog-information-symbolic",
        );
    }

    gtk::main();
}

fn install_launcher_signal_bridge(launcher: Rc<Launcher>) {
    unsafe {
        libc::signal(
            libc::SIGUSR1,
            launcher_signal_handler as *const () as usize,
        );
    }

    glib::timeout_add_seconds_local(1, move || {
        if TOGGLE_LAUNCHER.swap(false, Ordering::SeqCst) {
            launcher.toggle();
        }
        glib::ControlFlow::Continue
    });
}

extern "C" fn launcher_signal_handler(_sig: libc::c_int) {
    TOGGLE_LAUNCHER.store(true, Ordering::SeqCst);
}

fn load_css_theme() {
    let css_paths = [
        "assets/config/gtk-3.0/gtk.css",
        "/etc/slopos-i/gtk-3.0/gtk.css",
        "/usr/local/share/themes/slopos-gtk/gtk-3.0/gtk.css",
        "/usr/share/themes/slopos-gtk/gtk-3.0/gtk.css",
    ];

    for path in css_paths {
        if !Path::new(path).exists() { continue; }
        let provider = CssProvider::new();
        if provider.load_from_path(path).is_ok() {
            if let Some(screen) = gdk::Screen::default() {
                StyleContext::add_provider_for_screen(
                    &screen,
                    &provider,
                    gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
                );
                log::info!("Loaded SLOPOS GTK CSS from {path}");
                return;
            }
        }
    }

    log::warn!("SLOPOS GTK CSS was not found; falling back to host GTK theme");
}
