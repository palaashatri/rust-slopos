//! SLOPOS Archive Utility: list and extract archives on the SLOPOS toolkit.
//!
//! `slopos-archive-utility [ARCHIVE]` opens the window with ARCHIVE listed.
//! The archive model lives in [`archive`]; the window in [`app`].

mod app;
mod archive;

use std::sync::{Arc, OnceLock};

use slopos_ui::platform::{self, WindowOptions};

const APP_ID: &str = "slopos-archive-utility";
pub const WIDTH: u32 = 640;
pub const HEIGHT: u32 = 440;

fn main() {
    let initial = std::env::args().nth(1).unwrap_or_default();
    // The window's proxy exists only once the loop starts, so the worker
    // threads reach it through this slot.
    let slot: Arc<OnceLock<platform::Proxy>> = Arc::new(OnceLock::new());
    let waker_slot = Arc::clone(&slot);
    let waker: app::Waker = Arc::new(move || {
        if let Some(proxy) = waker_slot.get() {
            proxy.wake();
        }
    });
    let mut options = WindowOptions::new("Archive Utility", APP_ID, WIDTH, HEIGHT);
    options.resizable = false;
    let app = app::ArchiveApp::new(&initial, waker);
    let on_proxy = move |proxy: platform::Proxy| {
        let _ = slot.set(proxy);
    };
    if let Err(error) = platform::run(options, app, on_proxy) {
        eprintln!("slopos-archive-utility: {error}");
        std::process::exit(1);
    }
}
