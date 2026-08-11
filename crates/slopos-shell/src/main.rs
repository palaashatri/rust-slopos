//! SLOPOS-I X11 Desktop Shell Main Entry Point

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
use topbar::TopBar;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("Starting SLOPOS-I Desktop Shell (X11)");

    gtk::init().expect("Failed to initialize GTK3");

    // Load GTK CSS stylesheet
    load_css_theme();

    // Initialize Notification Server
    NotificationServer::start();

    // Initialize Spotlight Launcher Window
    let launcher = Launcher::new();

    // Initialize Top System Bar with Global Menu
    let _topbar = TopBar::new(launcher.clone());

    // Initialize Bottom Glass Dock
    let _dock = Dock::new(launcher.clone());

    // Welcome notification
    NotificationServer::show_toast(
        "Welcome to SLOPOS-I",
        "Press Super+Space or click Search to open Spotlight Launcher.",
        "dialog-information",
    );

    gtk::main();
}

fn load_css_theme() {
    let css_paths = vec![
        "assets/config/gtk-3.0/gtk.css",
        "/etc/slopos-i/gtk-3.0/gtk.css",
        "/usr/share/themes/slopos-gtk/gtk-3.0/gtk.css",
    ];

    for path in css_paths {
        if Path::new(path).exists() {
            let provider = CssProvider::new();
            if provider.load_from_path(path).is_ok() {
                if let Some(screen) = gdk::Screen::default() {
                    StyleContext::add_provider_for_screen(
                        &screen,
                        &provider,
                        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
                    );
                    log::info!("Loaded SLOPOS GTK CSS theme from {}", path);
                    return;
                }
            }
        }
    }
}
