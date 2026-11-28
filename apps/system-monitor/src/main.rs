//! SLOPOS System Monitor: processes, CPU, memory, disk and GPU, sampled from
//! `/proc` once a second while the window is visible.

mod app;
mod format;
mod gpu;
mod model;
mod policy;
mod procfs;
mod source;

use slopos_ui::platform::{self, WindowOptions};

const APP_ID: &str = "slopos-system-monitor";
const WIDTH: u32 = 760;
const HEIGHT: u32 = 540;

fn main() {
    let options = WindowOptions::new("System Monitor", APP_ID, WIDTH, HEIGHT);
    let app = app::SystemMonitor::new(source::LinuxSource::default(), WIDTH, HEIGHT);
    if let Err(error) = platform::run(options, app, |_proxy| {}) {
        eprintln!("slopos-system-monitor: {error}");
        std::process::exit(1);
    }
}
