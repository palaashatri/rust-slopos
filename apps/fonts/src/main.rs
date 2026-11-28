//! SLOPOS Fonts: browse the fonts fontconfig knows, preview them, and install
//! or remove the user's own fonts.

mod app;
mod catalog;

use std::path::PathBuf;

use slopos_ui::platform::{self, WindowOptions};

const APP_ID: &str = "slopos-fonts";

/// The user's font folder, `$XDG_DATA_HOME/fonts` or `~/.local/share/fonts`.
/// Without a home directory the path is one that cannot be created, so
/// installing reports an error instead of writing somewhere unexpected.
fn user_font_dir() -> PathBuf {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")));
    match data {
        Some(data) => data.join("fonts"),
        None => PathBuf::from("/nonexistent-slopos-user-fonts"),
    }
}

fn main() {
    let user_dir = user_font_dir();
    let source = catalog::Fontconfig {
        user_dir: user_dir.clone(),
    };
    let window = app::FontsApp::new(Box::new(source), user_dir);
    let mut options = WindowOptions::new("Fonts", APP_ID, app::WIDTH, app::HEIGHT);
    options.resizable = false;
    if let Err(error) = platform::run(options, window, |_proxy| {}) {
        eprintln!("slopos-fonts: {error}");
        std::process::exit(1);
    }
}
