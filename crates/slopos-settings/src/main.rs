//! SLOPOS-I Unified System Settings Utility

use gtk::prelude::*;
use gtk::{
    Box as GtkBox, Button, ComboBoxText, HeaderBar, IconSize, Image, Label, Notebook, Orientation,
    Scale, Switch, Window, WindowPosition, WindowType,
};
use std::process::Command;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("Starting SLOPOS-I System Settings");

    gtk::init().expect("Failed to initialize GTK3");

    let window = Window::new(WindowType::Toplevel);
    window.set_title("System Settings");
    window.set_default_size(720, 500);
    window.set_position(WindowPosition::Center);

    let header = HeaderBar::new();
    header.set_show_close_button(true);
    header.set_title(Some("System Settings"));
    header.set_subtitle(Some("SLOPOS-I Desktop Environment Preferences"));
    window.set_titlebar(Some(&header));

    let notebook = Notebook::new();

    // 1. Displays Tab
    notebook.append_page(
        &build_displays_panel(),
        Some(&create_tab_header("video-display-symbolic", "Displays")),
    );

    // 2. Audio Tab
    notebook.append_page(
        &build_audio_panel(),
        Some(&create_tab_header("audio-card-symbolic", "Audio")),
    );

    // 3. Network Tab
    notebook.append_page(
        &build_network_panel(),
        Some(&create_tab_header("network-workgroup-symbolic", "Network")),
    );

    // 4. Bluetooth Tab
    notebook.append_page(
        &build_bluetooth_panel(),
        Some(&create_tab_header("bluetooth-symbolic", "Bluetooth")),
    );

    // 5. Power Tab
    notebook.append_page(
        &build_power_panel(),
        Some(&create_tab_header("battery-good-symbolic", "Power")),
    );

    // 6. Appearance Tab
    notebook.append_page(
        &build_appearance_panel(),
        Some(&create_tab_header("preferences-desktop-theme-symbolic", "Appearance")),
    );

    // 7. Input Tab
    notebook.append_page(
        &build_input_panel(),
        Some(&create_tab_header("input-keyboard-symbolic", "Input")),
    );

    window.add(&notebook);
    window.show_all();
    gtk::main();
}

fn create_tab_header(icon_name: &str, title: &str) -> GtkBox {
    let box_widget = GtkBox::new(Orientation::Horizontal, 6);
    let img = Image::from_icon_name(Some(icon_name), IconSize::Menu);
    box_widget.pack_start(&img, false, false, 0);
    let label = Label::new(Some(title));
    box_widget.pack_start(&label, false, false, 0);
    box_widget.show_all();
    box_widget
}

fn build_displays_panel() -> GtkBox {
    let panel = GtkBox::new(Orientation::Vertical, 16);
    panel.set_margin_start(24);
    panel.set_margin_end(24);
    panel.set_margin_top(20);

    let title = Label::new(Some("Display Settings (XRandR)"));
    title.set_xalign(0.0);
    panel.pack_start(&title, false, false, 0);

    let res_box = GtkBox::new(Orientation::Horizontal, 12);
    res_box.pack_start(&Label::new(Some("Resolution:")), false, false, 0);
    let combo = ComboBoxText::new();
    combo.append_text("1920x1080 (16:9)");
    combo.append_text("2560x1440 (16:9)");
    combo.append_text("1366x768 (16:9)");
    combo.append_text("1280x800 (16:10)");
    combo.set_active(Some(0));
    res_box.pack_start(&combo, false, false, 0);
    panel.pack_start(&res_box, false, false, 0);

    let apply_btn = Button::with_label("Apply Resolution");
    apply_btn.connect_clicked(|_| {
        let _ = Command::new("xrandr").args(&["-s", "1920x1080"]).spawn();
    });
    panel.pack_start(&apply_btn, false, false, 0);

    panel
}

fn build_audio_panel() -> GtkBox {
    let panel = GtkBox::new(Orientation::Vertical, 16);
    panel.set_margin_start(24);
    panel.set_margin_end(24);
    panel.set_margin_top(20);

    let title = Label::new(Some("Audio Output & Volume (PipeWire / WirePlumber)"));
    title.set_xalign(0.0);
    panel.pack_start(&title, false, false, 0);

    let vol_box = GtkBox::new(Orientation::Horizontal, 12);
    vol_box.pack_start(&Label::new(Some("Volume:")), false, false, 0);
    let scale = Scale::with_range(Orientation::Horizontal, 0.0, 100.0, 5.0);
    scale.set_value(80.0);
    vol_box.pack_start(&scale, true, true, 0);
    panel.pack_start(&vol_box, false, false, 0);

    let mixer_btn = Button::with_label("Open Volume Control (Pavucontrol)");
    mixer_btn.connect_clicked(|_| {
        let _ = Command::new("pavucontrol").spawn();
    });
    panel.pack_start(&mixer_btn, false, false, 0);

    panel
}

fn build_network_panel() -> GtkBox {
    let panel = GtkBox::new(Orientation::Vertical, 16);
    panel.set_margin_start(24);
    panel.set_margin_end(24);
    panel.set_margin_top(20);

    let title = Label::new(Some("Network Connections (NetworkManager)"));
    title.set_xalign(0.0);
    panel.pack_start(&title, false, false, 0);

    let nm_btn = Button::with_label("Manage Wi-Fi & Connections (nm-connection-editor)");
    nm_btn.connect_clicked(|_| {
        let _ = Command::new("nm-connection-editor").spawn();
    });
    panel.pack_start(&nm_btn, false, false, 0);

    panel
}

fn build_bluetooth_panel() -> GtkBox {
    let panel = GtkBox::new(Orientation::Vertical, 16);
    panel.set_margin_start(24);
    panel.set_margin_end(24);
    panel.set_margin_top(20);

    let title = Label::new(Some("Bluetooth Manager (BlueZ)"));
    title.set_xalign(0.0);
    panel.pack_start(&title, false, false, 0);

    let bt_btn = Button::with_label("Scan & Pair Bluetooth Devices (blueman-manager)");
    bt_btn.connect_clicked(|_| {
        let _ = Command::new("blueman-manager").spawn();
    });
    panel.pack_start(&bt_btn, false, false, 0);

    panel
}

fn build_power_panel() -> GtkBox {
    let panel = GtkBox::new(Orientation::Vertical, 16);
    panel.set_margin_start(24);
    panel.set_margin_end(24);
    panel.set_margin_top(20);

    let title = Label::new(Some("Power & Battery Settings (UPower)"));
    title.set_xalign(0.0);
    panel.pack_start(&title, false, false, 0);

    let sleep_box = GtkBox::new(Orientation::Horizontal, 12);
    sleep_box.pack_start(&Label::new(Some("Automatic Sleep Timeout:")), false, false, 0);
    let combo = ComboBoxText::new();
    combo.append_text("15 Minutes");
    combo.append_text("30 Minutes");
    combo.append_text("1 Hour");
    combo.append_text("Never");
    combo.set_active(Some(0));
    sleep_box.pack_start(&combo, false, false, 0);
    panel.pack_start(&sleep_box, false, false, 0);

    panel
}

fn build_appearance_panel() -> GtkBox {
    let panel = GtkBox::new(Orientation::Vertical, 16);
    panel.set_margin_start(24);
    panel.set_margin_end(24);
    panel.set_margin_top(20);

    let title = Label::new(Some("Theme, Font & Wallpaper Selection"));
    title.set_xalign(0.0);
    panel.pack_start(&title, false, false, 0);

    let dark_box = GtkBox::new(Orientation::Horizontal, 12);
    dark_box.pack_start(&Label::new(Some("Dark Mode Appearance:")), false, false, 0);
    let sw = Switch::new();
    dark_box.pack_start(&sw, false, false, 0);
    panel.pack_start(&dark_box, false, false, 0);

    let bg_btn = Button::with_label("Change Wallpaper...");
    bg_btn.connect_clicked(|_| {
        let _ = Command::new("pcmanfm").arg("--desktop-pref").spawn();
    });
    panel.pack_start(&bg_btn, false, false, 0);

    panel
}

fn build_input_panel() -> GtkBox {
    let panel = GtkBox::new(Orientation::Vertical, 16);
    panel.set_margin_start(24);
    panel.set_margin_end(24);
    panel.set_margin_top(20);

    let title = Label::new(Some("Keyboard & Mouse Configuration"));
    title.set_xalign(0.0);
    panel.pack_start(&title, false, false, 0);

    let tap_box = GtkBox::new(Orientation::Horizontal, 12);
    tap_box.pack_start(&Label::new(Some("Touchpad Tap-to-Click:")), false, false, 0);
    let sw = Switch::new();
    sw.set_active(true);
    tap_box.pack_start(&sw, false, false, 0);
    panel.pack_start(&tap_box, false, false, 0);

    panel
}
