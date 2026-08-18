//! Native Desktop and Wallpaper Settings Panel.

use crate::providers::availability::{command_exists, resolve_program_path};
use gdk_pixbuf::Pixbuf;
use gtk::atk::prelude::AtkObjectExt;
use gtk::prelude::*;
use gtk::{
    Box as GtkBox, Button, ComboBoxText, Dialog, DialogFlags, FileChooserAction, FileChooserDialog,
    FileFilter, Image, Label, Orientation, RadioButton, ResponseType, ScrolledWindow, Window,
};
use std::cell::RefCell;
use std::env;
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;

pub fn show_wallpaper_dialog(parent: &Window) {
    let dialog = Dialog::with_buttons(
        Some("Desktop & Wallpaper"),
        Some(parent),
        DialogFlags::MODAL | DialogFlags::DESTROY_WITH_PARENT,
        &[
            ("Advanced…", ResponseType::Other(1)),
            ("Cancel", ResponseType::Cancel),
            ("Apply", ResponseType::Accept),
        ],
    );
    dialog.set_default_response(ResponseType::Accept);
    dialog.set_default_size(560, 530);
    set_accessible_name(&dialog, "SLOPOS wallpaper settings");

    let content = dialog.content_area();
    content.set_spacing(9);
    content.set_margin_start(14);
    content.set_margin_end(14);
    content.set_margin_top(12);
    content.set_margin_bottom(12);

    let heading = Label::new(Some("Desktop background"));
    heading.set_xalign(0.0);
    heading.style_context().add_class("slopos-control-title");
    content.pack_start(&heading, false, false, 0);

    let explanation = Label::new(Some(
        "Choose a bundled SLOPOS-I background or select an image from your computer.",
    ));
    explanation.set_xalign(0.0);
    explanation
        .style_context()
        .add_class("slopos-secondary-text");
    content.pack_start(&explanation, false, false, 0);

    let scrolled = ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
    scrolled.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scrolled.set_min_content_height(310);

    let rows = GtkBox::new(Orientation::Vertical, 6);
    let choices = [
        (
            "01_classic_system_gray.png",
            "Classic Gray",
            "Neutral monochrome dither",
        ),
        (
            "02_platinum_cool_slate.png",
            "Platinum Slate",
            "Cool slate grid",
        ),
        ("03_slate_blue.png", "Slate Blue", "Deep blue woven pattern"),
        (
            "04_retro_teal_grid.png",
            "Teal Grid",
            "Geometric teal desktop pattern",
        ),
        (
            "05_oled_pure_dark.png",
            "OLED Pure Dark",
            "Pure black with subtle points",
        ),
    ];

    let radio_choices: Rc<RefCell<Vec<(RadioButton, String)>>> = Rc::new(RefCell::new(Vec::new()));
    let custom_path: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let mut first_radio: Option<RadioButton> = None;

    for (index, (file, name, description)) in choices.iter().enumerate() {
        let row = GtkBox::new(Orientation::Horizontal, 10);
        row.style_context().add_class("slopos-section");

        if let Some(path) = find_wallpaper_path(file) {
            if let Ok(pixbuf) = Pixbuf::from_file_at_scale(path, 86, 54, true) {
                row.pack_start(&Image::from_pixbuf(Some(&pixbuf)), false, false, 0);
            }
        }

        let radio = if let Some(ref first) = first_radio {
            RadioButton::with_label_from_widget(first, "")
        } else {
            let value = RadioButton::with_label("");
            first_radio = Some(value.clone());
            value
        };
        if index == 1 {
            radio.set_active(true);
        }

        let labels = GtkBox::new(Orientation::Vertical, 1);
        let name_label = Label::new(Some(name));
        name_label.set_xalign(0.0);
        name_label.style_context().add_class("slopos-result-title");
        labels.pack_start(&name_label, false, false, 0);
        let description_label = Label::new(Some(description));
        description_label.set_xalign(0.0);
        description_label
            .style_context()
            .add_class("slopos-secondary-text");
        labels.pack_start(&description_label, false, false, 0);

        row.pack_start(&radio, false, false, 0);
        row.pack_start(&labels, true, true, 0);
        radio_choices
            .borrow_mut()
            .push((radio, (*file).to_string()));
        rows.pack_start(&row, false, false, 0);
    }

    let custom_radio = RadioButton::with_label_from_widget(
        first_radio
            .as_ref()
            .expect("bundled wallpaper choices must not be empty"),
        "Custom image",
    );
    let custom_label = Label::new(Some("No custom image selected"));
    custom_label.set_xalign(0.0);
    custom_label
        .style_context()
        .add_class("slopos-secondary-text");
    let browse = Button::with_label("Choose Image…");
    let dialog_parent = dialog.clone();
    let custom_radio_ref = custom_radio.clone();
    let custom_label_ref = custom_label.clone();
    let custom_path_ref = custom_path.clone();
    browse.connect_clicked(move |_| {
        let chooser = FileChooserDialog::with_buttons(
            Some("Choose Wallpaper Image"),
            Some(&dialog_parent),
            FileChooserAction::Open,
            &[
                ("Cancel", ResponseType::Cancel),
                ("Open", ResponseType::Accept),
            ],
        );
        let filter = FileFilter::new();
        filter.set_name(Some("Images"));
        for mime in [
            "image/png",
            "image/jpeg",
            "image/bmp",
            "image/svg+xml",
            "image/webp",
        ] {
            filter.add_mime_type(mime);
        }
        for pattern in ["*.png", "*.jpg", "*.jpeg", "*.bmp", "*.svg", "*.webp"] {
            filter.add_pattern(pattern);
        }
        chooser.add_filter(filter);
        if chooser.run() == ResponseType::Accept {
            if let Some(path) = chooser.filename() {
                custom_label_ref.set_text(&path.to_string_lossy());
                *custom_path_ref.borrow_mut() = Some(path.to_string_lossy().to_string());
                custom_radio_ref.set_active(true);
            }
        }
        chooser.close();
    });

    let custom_row = GtkBox::new(Orientation::Horizontal, 8);
    custom_row.style_context().add_class("slopos-section");
    custom_row.pack_start(&custom_radio, false, false, 0);
    custom_row.pack_start(&custom_label, true, true, 0);
    custom_row.pack_start(&browse, false, false, 0);
    radio_choices
        .borrow_mut()
        .push((custom_radio, "custom".to_string()));
    rows.pack_start(&custom_row, false, false, 0);

    scrolled.add(&rows);
    content.pack_start(&scrolled, true, true, 0);

    let mode_row = GtkBox::new(Orientation::Horizontal, 8);
    let mode_label = Label::new(Some("Fit:"));
    let mode = ComboBoxText::new();
    mode.append(Some("fill"), "Fill");
    mode.append(Some("max"), "Fit");
    mode.append(Some("center"), "Center");
    mode.append(Some("tile"), "Tile");
    mode.set_active_id(Some("fill"));
    mode_row.pack_start(&mode_label, false, false, 0);
    mode_row.pack_start(&mode, true, true, 0);
    content.pack_start(&mode_row, false, false, 0);

    // Dock & Application Strip Preferences
    let dock_frame = gtk::Frame::new(Some("Application Strip (Dock)"));
    dock_frame.style_context().add_class("slopos-section");
    let dock_box = GtkBox::new(Orientation::Vertical, 6);
    dock_box.set_margin_start(10);
    dock_box.set_margin_end(10);
    dock_box.set_margin_top(6);
    dock_box.set_margin_bottom(6);

    let dock_pos_row = GtkBox::new(Orientation::Horizontal, 8);
    dock_pos_row.pack_start(&Label::new(Some("Screen Position:")), false, false, 0);
    let dock_pos_combo = ComboBoxText::new();
    dock_pos_combo.append(Some("bottom"), "Bottom (Horizontal)");
    dock_pos_combo.append(Some("left"), "Left Screen Edge (Vertical)");
    dock_pos_combo.append(Some("right"), "Right Screen Edge (Vertical)");

    let cur_pos = current_saved_dock_position();
    dock_pos_combo.set_active_id(Some(&cur_pos));
    dock_pos_row.pack_start(&dock_pos_combo, true, true, 0);
    dock_box.pack_start(&dock_pos_row, false, false, 0);

    let dock_align_row = GtkBox::new(Orientation::Horizontal, 8);
    dock_align_row.pack_start(&Label::new(Some("Alignment:")), false, false, 0);
    let dock_align_combo = ComboBoxText::new();
    dock_align_combo.append(Some("center"), "Center (macOS Style)");
    dock_align_combo.append(Some("start"), "Start (Left / Top)");
    dock_align_combo.append(Some("end"), "End (Right / Bottom)");

    let cur_align = current_saved_dock_alignment();
    dock_align_combo.set_active_id(Some(&cur_align));
    dock_align_row.pack_start(&dock_align_combo, true, true, 0);
    dock_box.pack_start(&dock_align_row, false, false, 0);

    let dock_dodge_check =
        gtk::CheckButton::with_label("Auto-hide dock when windows maximize (Dock Dodge)");
    dock_dodge_check.set_active(current_saved_dock_dodge());
    dock_box.pack_start(&dock_dodge_check, false, false, 0);

    dock_frame.add(&dock_box);
    content.pack_start(&dock_frame, false, false, 0);

    dialog.show_all();
    match dialog.run() {
        ResponseType::Accept => {
            let mut selected = "02_platinum_cool_slate.png".to_string();
            for (radio, value) in radio_choices.borrow().iter() {
                if !radio.is_active() {
                    continue;
                }
                selected = if value == "custom" {
                    custom_path
                        .borrow()
                        .clone()
                        .unwrap_or_else(|| selected.clone())
                } else {
                    value.clone()
                };
                break;
            }
            let mode_value = mode.active_id().unwrap_or_else(|| "fill".into());
            if let Some(helper) = resolve_program_path("slopos-wallpaper") {
                if let Err(error) = Command::new(helper)
                    .args(["set", selected.as_str(), "--mode", mode_value.as_str()])
                    .spawn()
                {
                    log::warn!("Failed to apply wallpaper: {error}");
                }
            } else {
                log::warn!("slopos-wallpaper helper is unavailable");
            }

            // Save dock preferences
            let pos_val = dock_pos_combo
                .active_id()
                .unwrap_or_else(|| "bottom".into());
            let align_val = dock_align_combo
                .active_id()
                .unwrap_or_else(|| "center".into());
            let dodge_val = if dock_dodge_check.is_active() {
                "1"
            } else {
                "0"
            };

            save_dock_settings(pos_val.as_str(), align_val.as_str(), dodge_val);
            let _ = Command::new("pkill")
                .args(["-USR1", "-x", "slopos-shell"])
                .status();
        }
        ResponseType::Other(1) if command_exists("pcmanfm") => {
            let _ = Command::new("pcmanfm").arg("--desktop-pref").spawn();
        }
        _ => {}
    }
    dialog.close();
}

fn current_saved_dock_position() -> String {
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    if let Some(config_home) = config_home {
        if let Ok(val) = std::fs::read_to_string(config_home.join("slopos-i/dock_position")) {
            let v = val.trim().to_ascii_lowercase();
            if v == "left" || v == "right" {
                return v;
            }
        }
    }
    "bottom".to_string()
}

fn current_saved_dock_alignment() -> String {
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    if let Some(config_home) = config_home {
        if let Ok(val) = std::fs::read_to_string(config_home.join("slopos-i/dock_alignment")) {
            let v = val.trim().to_ascii_lowercase();
            if v == "start" || v == "end" {
                return v;
            }
        }
    }
    "center".to_string()
}

fn current_saved_dock_dodge() -> bool {
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    if let Some(config_home) = config_home {
        if let Ok(val) = std::fs::read_to_string(config_home.join("slopos-i/dock_dodge")) {
            let v = val.trim();
            return v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes");
        }
    }
    false
}

fn save_dock_settings(pos: &str, align: &str, dodge: &str) {
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    if let Some(config_home) = config_home {
        let dir = config_home.join("slopos-i");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("dock_position"), pos);
        let _ = std::fs::write(dir.join("dock_alignment"), align);
        let _ = std::fs::write(dir.join("dock_dodge"), dodge);
    }
}

fn find_wallpaper_path(filename: &str) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(share) = env::var("SLOPOS_SHARE_DIR") {
        candidates.push(PathBuf::from(&share).join("wallpapers").join(filename));
        candidates.push(
            PathBuf::from(share)
                .join("slopos-i/wallpapers")
                .join(filename),
        );
    }
    candidates.extend([
        PathBuf::from("assets/wallpapers").join(filename),
        PathBuf::from("/usr/local/share/slopos-i/wallpapers").join(filename),
        PathBuf::from("/usr/share/slopos-i/wallpapers").join(filename),
    ]);
    candidates.into_iter().find(|path| path.is_file())
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
