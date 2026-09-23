//! Settings CSS theme loader and visual token integration.

use gtk::prelude::*;
use gtk::{CssProvider, StyleContext};
use std::path::PathBuf;

const CONTROL_PANEL_PARITY_CSS: &[u8] = br#"
.slopos-folder-caption {
  min-height: 17px;
  padding: 1px 3px 3px 3px;
  font-weight: bold;
  border-bottom: 1px solid #858585;
}

.slopos-icon-grid {
  padding: 10px 8px 8px 8px;
  background-color: #ffffff;
  border: 1px solid #111111;
}

/* These are folder objects, not modern dashboard cards.  Override every
 * inherited Platinum push-button edge so idle and unavailable panels sit
 * directly on the white icon field like files in a classic folder window. */
button.slopos-control-panel-icon,
button.slopos-control-panel-icon:disabled {
  min-width: 86px;
  min-height: 72px;
  padding: 4px 3px;
  margin: 0;
  background-image: none;
  background-color: transparent;
  border-style: none;
  border-width: 0;
  border-radius: 0;
  box-shadow: none;
  text-shadow: none;
  outline-width: 0;
}

button.slopos-control-panel-icon:hover,
button.slopos-control-panel-icon:focus,
button.slopos-control-panel-icon:active,
button.slopos-control-panel-icon:checked {
  color: #ffffff;
  background-image: none;
  background-color: #000080;
  border-style: none;
  border-width: 0;
  box-shadow: none;
  text-shadow: none;
  outline-width: 0;
}

button.slopos-control-panel-icon:hover label,
button.slopos-control-panel-icon:focus label,
button.slopos-control-panel-icon:active label,
button.slopos-control-panel-icon:checked label {
  color: #ffffff;
  text-shadow: none;
}

button.slopos-control-panel-icon:disabled {
  opacity: 0.46;
}

button.slopos-control-panel-icon image {
  min-width: 32px;
  min-height: 32px;
  margin-bottom: 1px;
}

button.slopos-control-panel-icon label {
  padding: 0 2px;
  font-size: 10px;
  font-weight: normal;
  color: #111111;
  text-shadow: none;
}
"#;

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
        let app_user_css = match appearance {
            "oled" => config_home.join("gtk-3.0/gtk-oled.css"),
            "graphite" => config_home.join("gtk-3.0/gtk-graphite.css"),
            "classic" => config_home.join("gtk-3.0/gtk-classic.css"),
            _ => config_home.join("gtk-3.0/gtk.css"),
        };
        if app_user_css.exists() {
            css_paths.push(app_user_css);
        }
    }
    css_paths.extend([
        PathBuf::from(format!(
            "/usr/share/themes/{installed_theme}/gtk-3.0/gtk.css"
        )),
        PathBuf::from(format!(
            "/usr/local/share/themes/{installed_theme}/gtk-3.0/gtk.css"
        )),
        PathBuf::from(source_css),
        PathBuf::from(format!("/etc/slopos-i/gtk-3.0/{installed_theme}.css")),
    ]);
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
            continue;
        };
        if provider.load_from_path(path_text).is_ok() {
            if let Some(screen) = gdk::Screen::default() {
                StyleContext::add_provider_for_screen(
                    &screen,
                    &provider,
                    gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
                );
            }
            break;
        }
    }

    // Settings has one additional application-owned layer for the icon-grid
    // Control Panels presentation. Keeping it here avoids changing the global
    // GTK theme semantics for unrelated upstream applications.
    let parity_provider = CssProvider::new();
    if parity_provider.load_from_data(CONTROL_PANEL_PARITY_CSS).is_ok() {
        if let Some(screen) = gdk::Screen::default() {
            StyleContext::add_provider_for_screen(
                &screen,
                &parity_provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
            );
        }
    }
}
