//! Appearance tokens and runtime theme selection for SLOPOS-I.

use gtk::prelude::*;
use gtk::{CssProvider, StyleContext};
use std::path::PathBuf;

pub fn current_appearance() -> &'static str {
    if let Ok(env_app) = std::env::var("SLOPOS_APPEARANCE") {
        let v = env_app.trim();
        if v.eq_ignore_ascii_case("custom") {
            return "custom";
        }
        if v.eq_ignore_ascii_case("oled") {
            return "oled";
        }
        if v.eq_ignore_ascii_case("graphite") {
            return "graphite";
        }
        if v.eq_ignore_ascii_case("classic") {
            return "classic";
        }
        if v.eq_ignore_ascii_case("platinum") {
            return "platinum";
        }
    }
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    if let Some(config_home) = config_home {
        if let Ok(value) = std::fs::read_to_string(config_home.join("slopos-i/appearance")) {
            let v = value.trim();
            if v.eq_ignore_ascii_case("custom") {
                return "custom";
            }
            if v.eq_ignore_ascii_case("oled") {
                return "oled";
            }
            if v.eq_ignore_ascii_case("graphite") {
                return "graphite";
            }
            if v.eq_ignore_ascii_case("classic") {
                return "classic";
            }
        }
    }
    "platinum"
}

pub fn load_css_theme() {
    let appearance = current_appearance();
    let installed_theme = match appearance {
        "oled" => "slopos-gtk-oled",
        "graphite" => "slopos-gtk-graphite",
        "classic" => "slopos-gtk-classic",
        _ => "slopos-gtk",
    };
    let source_css = match appearance {
        "oled" => "assets/config/gtk-3.0/gtk-oled.css",
        "graphite" => "assets/config/gtk-3.0/gtk-graphite.css",
        "classic" => "assets/config/gtk-3.0/gtk-classic.css",
        _ => "assets/config/gtk-3.0/gtk.css",
    };

    let mut css_paths = Vec::new();
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    if let Some(ref config_home) = config_home {
        let user_css = config_home.join("gtk-3.0/gtk.css");
        if user_css.exists() {
            css_paths.push(user_css);
        }
    }
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
                        gtk::STYLE_PROVIDER_PRIORITY_USER,
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
