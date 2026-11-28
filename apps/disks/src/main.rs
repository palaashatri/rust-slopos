//! SLOPOS Disks: drives and volumes from the UDisks2 storage provider.
//! Read-only in this version.

mod app;
mod model;

use std::sync::{Arc, OnceLock};

use slopos_ui::platform::{self, WindowOptions};

const APP_ID: &str = "slopos-disks";

fn main() {
    // The window's proxy exists only once the loop starts; the reader thread
    // wakes the window through this slot.
    let slot: Arc<OnceLock<platform::Proxy>> = Arc::new(OnceLock::new());
    let waker_slot = Arc::clone(&slot);
    let waker: app::Waker = Arc::new(move || {
        if let Some(proxy) = waker_slot.get() {
            proxy.wake();
        }
    });
    let mut options = WindowOptions::new("Disks", APP_ID, app::WIDTH, app::HEIGHT);
    options.resizable = false;
    let window = app::DisksApp::new(waker);
    let on_proxy = move |proxy: platform::Proxy| {
        let _ = slot.set(proxy);
    };
    if let Err(error) = platform::run(options, window, on_proxy) {
        eprintln!("slopos-disks: {error}");
        std::process::exit(1);
    }
}
