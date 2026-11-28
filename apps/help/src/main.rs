//! SLOPOS Help: offline topics built from the repository's documentation.
//!
//! `slopos-help [TOPIC]` opens the window on TOPIC (`readme`, `architecture`
//! or `development`); other applications use it for deep links.

mod app;
mod topics;

use slopos_ui::platform::{self, WindowOptions};

const APP_ID: &str = "slopos-help";

fn main() {
    let topic = std::env::args().nth(1);
    let options = WindowOptions::new("Help", APP_ID, app::WIDTH, app::HEIGHT);
    let window = app::HelpApp::new(topic.as_deref());
    if let Err(error) = platform::run(options, window, |_proxy| {}) {
        eprintln!("slopos-help: {error}");
        std::process::exit(1);
    }
}
