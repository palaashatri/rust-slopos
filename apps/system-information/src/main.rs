//! SLOPOS System Information: the About window. Every value is read from the
//! system when the window opens; a value that cannot be read says so.

mod app;
mod facts;

use app::{HEIGHT, WIDTH};
use facts::{gather, LinuxSource};
use slopos_ui::platform::{self, App, WindowOptions};
use slopos_ui::render::RasterSurface;

const APP_ID: &str = "slopos-system-information";

fn main() {
    let facts = gather(&LinuxSource);
    if let Some(path) = snapshot_path() {
        // `--snapshot FILE`: render one frame headless (for QA and docs).
        let scale = platform::scale_from_environment();
        let mut app = app::SystemInformationApp::new(facts);
        let surface = RasterSurface::new(WIDTH, HEIGHT, scale).expect("surface");
        app.paint(&surface);
        surface.save_png(&path).expect("write snapshot");
        return;
    }
    let mut options = WindowOptions::new("System Information", APP_ID, WIDTH, HEIGHT);
    options.resizable = false;
    let app = app::SystemInformationApp::new(facts);
    if let Err(error) = platform::run(options, app, |_proxy| {}) {
        eprintln!("slopos-system-information: {error}");
        std::process::exit(1);
    }
}

fn snapshot_path() -> Option<std::path::PathBuf> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--snapshot" {
            return args.next().map(Into::into);
        }
    }
    None
}
