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
        }
        ResponseType::Other(1) if command_exists("pcmanfm") => {
            let _ = Command::new("pcmanfm").arg("--desktop-pref").spawn();
        }
        _ => {}
    }
    dialog.close();
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
