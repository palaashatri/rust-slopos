//! Native Appearance Settings Panel.

use crate::providers::availability::resolve_program_path;
use crate::theme::current_appearance;
use gdk_pixbuf::Pixbuf;
use gtk::atk::prelude::AtkObjectExt;
use gtk::prelude::*;
use gtk::{
    Box as GtkBox, CheckButton, Dialog, DialogFlags, FontButton, Image, Label, Orientation,
    RadioButton, ResponseType, Window,
};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn show_appearance_dialog(parent: &Window) {
    let dialog = Dialog::with_buttons(
        Some("Appearance"),
        Some(parent),
        DialogFlags::MODAL | DialogFlags::DESTROY_WITH_PARENT,
        &[
            ("Cancel", ResponseType::Cancel),
            ("Apply", ResponseType::Accept),
        ],
    );
    dialog.set_default_response(ResponseType::Accept);
    dialog.set_default_size(560, 480);
    set_accessible_name(&dialog, "SLOPOS appearance settings");

    let content = dialog.content_area();
    content.set_spacing(10);
    content.set_margin_start(14);
    content.set_margin_end(14);
    content.set_margin_top(12);
    content.set_margin_bottom(12);

    let heading = Label::new(Some("Choose a SLOPOS-I appearance"));
    heading.set_xalign(0.0);
    heading.style_context().add_class("slopos-control-title");
    content.pack_start(&heading, false, false, 0);

    let explanation = Label::new(Some(
        "Every preset uses the same SLOPOS component language. Changes apply to the shell and supported GTK applications.",
    ));
    explanation.set_xalign(0.0);
    explanation.set_line_wrap(true);
    explanation
        .style_context()
        .add_class("slopos-secondary-text");
    content.pack_start(&explanation, false, false, 0);

    let platinum = RadioButton::with_label("Platinum Light");
    let graphite = RadioButton::with_label_from_widget(&platinum, "Graphite Dark");
    let oled = RadioButton::with_label_from_widget(&platinum, "OLED Dark");
    let classic = RadioButton::with_label_from_widget(&platinum, "Classic Contrast");

    let presets = [
        (
            "platinum",
            "Soft neutral surfaces, rounded controls and the canonical SLOPOS-I light palette",
            platinum.clone(),
        ),
        (
            "graphite",
            "A dark neutral version of the same SLOPOS-I component system",
            graphite.clone(),
        ),
        (
            "oled",
            "Pure-black surfaces for OLED displays and maximum dark contrast",
            oled.clone(),
        ),
        (
            "classic",
            "A deliberately sharp high-contrast monochrome accessibility/legacy style",
            classic.clone(),
        ),
    ];

    let presets_box = GtkBox::new(Orientation::Vertical, 7);
    for (id, description, radio) in presets {
        let row = GtkBox::new(Orientation::Horizontal, 10);
        row.style_context().add_class("slopos-section");
        if let Some(preview) = load_theme_preview(id) {
            row.pack_start(&preview, false, false, 0);
        }
        let labels = GtkBox::new(Orientation::Vertical, 2);
        labels.pack_start(&radio, false, false, 0);
        let description_label = Label::new(Some(description));
        description_label.set_xalign(0.0);
        description_label.set_line_wrap(true);
        description_label
            .style_context()
            .add_class("slopos-secondary-text");
        labels.pack_start(&description_label, false, false, 0);
        row.pack_start(&labels, true, true, 0);
        presets_box.pack_start(&row, false, false, 0);
    }
    content.pack_start(&presets_box, true, true, 0);

    match current_appearance() {
        "graphite" => graphite.set_active(true),
        "oled" => oled.set_active(true),
        "classic" => classic.set_active(true),
        _ => platinum.set_active(true),
    }

    let typography = GtkBox::new(Orientation::Horizontal, 8);
    let font_label = Label::new(Some("Interface font:"));
    font_label.set_xalign(0.0);
    let font = FontButton::new();
    font.set_font(&current_font());
    typography.pack_start(&font_label, false, false, 0);
    typography.pack_start(&font, true, true, 0);
    content.pack_start(&typography, false, false, 0);

    let dodge = CheckButton::with_label(
        "Hide the Application Strip when a maximized window needs the space",
    );
    dodge.set_active(is_dock_dodge_enabled());
    content.pack_start(&dodge, false, false, 0);

    dialog.show_all();
    if dialog.run() == ResponseType::Accept {
        let mode = if graphite.is_active() {
            "graphite"
        } else if oled.is_active() {
            "oled"
        } else if classic.is_active() {
            "classic"
        } else {
            "platinum"
        };

        if let Some(font_name) = font.font() {
            save_font(font_name.as_str());
        }
        set_dock_dodge_enabled(dodge.is_active());

        if let Some(helper) = resolve_program_path("slopos-appearance") {
            if let Err(error) = Command::new(helper).arg(mode).spawn() {
                log::warn!("Failed to apply appearance: {error}");
            }
        } else {
            log::warn!("slopos-appearance helper is unavailable");
        }
    }
    dialog.close();
}

fn load_theme_preview(name: &str) -> Option<Image> {
    let mut candidates = Vec::new();
    if let Ok(share) = env::var("SLOPOS_SHARE_DIR") {
        candidates.push(format!("{share}/slopos-i/previews/{name}.png"));
    }
    candidates.extend([
        format!("assets/previews/{name}.png"),
        format!("/usr/local/share/slopos-i/previews/{name}.png"),
        format!("/usr/share/slopos-i/previews/{name}.png"),
    ]);
    for path in candidates {
        if Path::new(&path).exists() {
            if let Ok(pixbuf) = Pixbuf::from_file_at_scale(&path, 72, 48, false) {
                return Some(Image::from_pixbuf(Some(&pixbuf)));
            }
        }
    }
    None
}

fn current_font() -> String {
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
    if let Some(config_home) = config_home {
        let settings_ini = config_home.join("gtk-3.0/settings.ini");
        if let Ok(content) = fs::read_to_string(settings_ini) {
            for line in content.lines() {
                if let Some(font) = line.strip_prefix("gtk-font-name = ") {
                    return font.trim().to_string();
                }
            }
        }
    }
    "Geneva 9".to_string()
}

fn save_font(font: &str) {
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
    let Some(config_home) = config_home else {
        return;
    };
    let settings_ini = config_home.join("gtk-3.0/settings.ini");
    if let Ok(content) = fs::read_to_string(&settings_ini) {
        let mut new_lines = Vec::new();
        let mut replaced = false;
        for line in content.lines() {
            if line.starts_with("gtk-font-name") {
                new_lines.push(format!("gtk-font-name = {font}"));
                replaced = true;
            } else {
                new_lines.push(line.to_string());
            }
        }
        if !replaced {
            new_lines.push(format!("gtk-font-name = {font}"));
        }
        let _ = fs::write(&settings_ini, new_lines.join("\n"));
    }
}

pub fn is_dock_dodge_enabled() -> bool {
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
    if let Some(config_home) = config_home {
        let flag_file = config_home.join("slopos-i/dock_dodge");
        if let Ok(content) = fs::read_to_string(flag_file) {
            let t = content.trim();
            return t == "1" || t.eq_ignore_ascii_case("true") || t.eq_ignore_ascii_case("yes");
        }
    }
    false
}

pub fn set_dock_dodge_enabled(enabled: bool) {
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
    let Some(config_home) = config_home else {
        return;
    };
    let dir = config_home.join("slopos-i");
    let _ = fs::create_dir_all(&dir);
    let _ = fs::write(dir.join("dock_dodge"), if enabled { "1\n" } else { "0\n" });
}

fn set_accessible_name<W>(widget: &W, name: &str)
where
    W: IsA<gtk::Widget>,
{
    let Some(accessible) = widget.accessible() else {
        return;
    };
    let Ok(accessible) = accessible.downcast::<gtk::atk::Object>() else {
        return;
    };
    accessible.set_name(name);
}
