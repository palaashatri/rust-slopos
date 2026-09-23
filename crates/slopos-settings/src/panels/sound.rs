//! Native Sound & Audio Settings Panel.

use crate::providers::availability::command_exists;
use gtk::atk::prelude::AtkObjectExt;
use gtk::prelude::*;
use gtk::{
    Box as GtkBox, Button, CheckButton, ComboBoxText, Dialog, DialogFlags, Frame, IconSize, Image,
    Label, Orientation, ResponseType, Scale, Window,
};
use std::process::Command;

pub fn show_sound_dialog(parent: &Window) {
    let dialog = Dialog::with_buttons(
        Some("Sound & Volume"),
        Some(parent),
        DialogFlags::MODAL | DialogFlags::DESTROY_WITH_PARENT,
        &[
            ("Cancel", ResponseType::Cancel),
            ("Apply", ResponseType::Accept),
        ],
    );
    dialog.set_default_response(ResponseType::Accept);
    dialog.set_default_size(460, 420);
    set_accessible_name(&dialog, "SLOPOS sound and volume settings");

    let content = dialog.content_area();
    content.set_spacing(10);
    content.set_margin_start(14);
    content.set_margin_end(14);
    content.set_margin_top(12);
    content.set_margin_bottom(12);

    let title = Label::new(Some("Sound Preferences"));
    title.set_xalign(0.0);
    title.style_context().add_class("slopos-control-title");
    content.pack_start(&title, false, false, 0);

    // Output Section
    let output_frame = Frame::new(Some("Output (Speakers / Headphones)"));
    output_frame.style_context().add_class("slopos-section");
    let output_box = GtkBox::new(Orientation::Vertical, 8);
    output_box.set_margin_start(10);
    output_box.set_margin_end(10);
    output_box.set_margin_top(8);
    output_box.set_margin_bottom(8);

    let device_row = GtkBox::new(Orientation::Horizontal, 8);
    device_row.pack_start(&Label::new(Some("Output Device:")), false, false, 0);
    let device_combo = ComboBoxText::new();
    device_combo.append(Some("default"), "Default Audio Output (PipeWire / ALSA)");
    device_combo.append(Some("built-in"), "Built-in Analog Stereo Speakers");
    device_combo.append(Some("headphones"), "Headphones / Line Out");
    device_combo.append(Some("hdmi"), "Digital Output (HDMI / DisplayPort)");
    device_combo.set_active_id(Some("default"));
    device_row.pack_start(&device_combo, true, true, 0);
    output_box.pack_start(&device_row, false, false, 0);

    let vol_row = GtkBox::new(Orientation::Horizontal, 8);
    vol_row.pack_start(
        &Image::from_icon_name(Some("audio-volume-high-symbolic"), IconSize::Button),
        false,
        false,
        0,
    );
    vol_row.pack_start(&Label::new(Some("Volume:")), false, false, 0);
    let vol_scale = Scale::with_range(Orientation::Horizontal, 0.0, 100.0, 1.0);
    vol_scale.set_value(75.0);
    vol_scale.set_hexpand(true);
    vol_scale.set_draw_value(true);
    vol_scale.set_value_pos(gtk::PositionType::Right);
    vol_row.pack_start(&vol_scale, true, true, 0);
    output_box.pack_start(&vol_row, false, false, 0);

    let mute_check = CheckButton::with_label("Mute output");
    output_box.pack_start(&mute_check, false, false, 0);

    let test_row = GtkBox::new(Orientation::Horizontal, 8);
    let test_btn = Button::with_label("Play Test Sound");
    test_btn.style_context().add_class("slopos-push-btn");
    test_btn.connect_clicked(|_| {
        if command_exists("paplay") {
            let _ = Command::new("paplay")
                .arg("/usr/share/sounds/freedesktop/stereo/bell.oga")
                .spawn();
        } else if command_exists("canberra-gtk-play") {
            let _ = Command::new("canberra-gtk-play")
                .args(["-i", "bell"])
                .spawn();
        } else if command_exists("speaker-test") {
            let _ = Command::new("speaker-test")
                .args(["-t", "sine", "-f", "440", "-l", "1"])
                .spawn();
        }
    });
    test_row.pack_start(&test_btn, false, false, 0);
    output_box.pack_start(&test_row, false, false, 0);

    output_frame.add(&output_box);
    content.pack_start(&output_frame, false, false, 0);

    // Input Section
    let input_frame = Frame::new(Some("Input (Microphone)"));
    input_frame.style_context().add_class("slopos-section");
    let input_box = GtkBox::new(Orientation::Vertical, 8);
    input_box.set_margin_start(10);
    input_box.set_margin_end(10);
    input_box.set_margin_top(8);
    input_box.set_margin_bottom(8);

    let input_vol_row = GtkBox::new(Orientation::Horizontal, 8);
    input_vol_row.pack_start(
        &Image::from_icon_name(Some("audio-input-microphone-symbolic"), IconSize::Button),
        false,
        false,
        0,
    );
    input_vol_row.pack_start(&Label::new(Some("Mic Level:")), false, false, 0);
    let input_scale = Scale::with_range(Orientation::Horizontal, 0.0, 100.0, 1.0);
    input_scale.set_value(60.0);
    input_scale.set_hexpand(true);
    input_scale.set_draw_value(true);
    input_scale.set_value_pos(gtk::PositionType::Right);
    input_vol_row.pack_start(&input_scale, true, true, 0);
    input_box.pack_start(&input_vol_row, false, false, 0);

    let input_mute_check = CheckButton::with_label("Mute microphone");
    input_box.pack_start(&input_mute_check, false, false, 0);

    input_frame.add(&input_box);
    content.pack_start(&input_frame, false, false, 0);

    if command_exists("pavucontrol") {
        let adv_btn = Button::with_label("Open Advanced Audio Mixer (Pavucontrol)");
        adv_btn.style_context().add_class("slopos-push-btn");
        adv_btn.connect_clicked(|_| {
            let _ = Command::new("pavucontrol").spawn();
        });
        content.pack_start(&adv_btn, false, false, 0);
    }

    dialog.show_all();
    if dialog.run() == ResponseType::Accept {
        let vol = vol_scale.value() as u32;
        let is_muted = mute_check.is_active();
        if command_exists("pactl") {
            let _ = Command::new("pactl")
                .args(["set-sink-volume", "@DEFAULT_SINK@", &format!("{vol}%")])
                .status();
            let _ = Command::new("pactl")
                .args([
                    "set-sink-mute",
                    "@DEFAULT_SINK@",
                    if is_muted { "1" } else { "0" },
                ])
                .status();
        } else if command_exists("amixer") {
            let _ = Command::new("amixer")
                .args(["set", "Master", &format!("{vol}%")])
                .status();
        }
    }
    dialog.close();
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
