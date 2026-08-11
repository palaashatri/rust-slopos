//! SLOPOS-I X11 Desktop Shell Main Entry Point

mod app_finder;
mod dock;
mod launcher;
mod notifications;
mod topbar;

use dock::Dock;
use gtk::prelude::*;
use launcher::Launcher;
use notifications::NotificationServer;
use topbar::TopBar;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("Starting SLOPOS-I Desktop Shell (X11)");

    gtk::init().expect("Failed to initialize GTK3");

    // Initialize Notification Server
    NotificationServer::start();

    // Initialize Spotlight Launcher Window
    let launcher = Launcher::new();

    // Initialize Top System Bar
    let topbar = TopBar::new(launcher.clone());

    // Initialize Bottom Dock
    let _dock = Dock::new(launcher.clone());

    // Welcome notification
    NotificationServer::show_toast(
        "Welcome to SLOPOS-I",
        "Press Super+Space or click Search to open Spotlight Launcher.",
        "emblem-favorite",
    );

    gtk::main();
}
