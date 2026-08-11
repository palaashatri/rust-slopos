//! SLOPOS-I Settings hub.
//!
//! This intentionally delegates to mature upstream configuration utilities.
//! It does not expose decorative controls that pretend to modify system state.

use gtk::prelude::*;
use gtk::{Box as GtkBox, Button, IconSize, Image, Label, Orientation, Window, WindowPosition, WindowType};
use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    gtk::init().expect("Failed to initialize GTK3");
    load_css_theme();

    let window = Window::new(WindowType::Toplevel);
    window.set_title("System Settings");
    window.set_default_size(620, 520);
    window.set_position(WindowPosition::Center);

    let body = GtkBox::new(Orientation::Vertical, 7);
    body.style_context().add_class("slopos-window-body");

    let title = Label::new(Some("System Settings"));
    title.set_xalign(0.0);
    title.style_context().add_class("slopos-panel-title");
    body.pack_start(&title, false, false, 0);

    let subtitle = Label::new(Some("SLOPOS delegates system configuration to mature X11/Linux utilities."));
    subtitle.set_xalign(0.0);
    subtitle.style_context().add_class("slopos-panel-subtitle");
    body.pack_start(&subtitle, false, false, 2);

    add_launcher_row(
        &body,
        "video-display-symbolic",
        "Displays",
        "Resolution, orientation and monitor layout",
        &[("arandr", &[]), ("lxrandr", &[])],
    );
    add_launcher_row(
        &body,
        "audio-card-symbolic",
        "Sound",
        "Output devices, input devices and volume",
        &[("pavucontrol", &[])],
    );
    add_launcher_row(
        &body,
        "network-wireless-symbolic",
        "Network",
        "Wi-Fi, Ethernet and saved connections",
        &[("nm-connection-editor", &[])],
    );
    add_launcher_row(
        &body,
        "bluetooth-symbolic",
        "Bluetooth",
        "Discover, pair and manage Bluetooth devices",
        &[("blueman-manager", &[])],
    );
    add_launcher_row(
        &body,
        "battery-good-symbolic",
        "Power",
        "Sleep, lid and battery behavior",
        &[("xfce4-power-manager-settings", &[])],
    );
    add_launcher_row(
        &body,
        "preferences-desktop-theme-symbolic",
        "Appearance",
        "GTK theme, icons and font preferences",
        &[("lxappearance", &[])],
    );
    add_launcher_row(
        &body,
        "preferences-desktop-wallpaper-symbolic",
        "Desktop",
        "Wallpaper and desktop presentation",
        &[("pcmanfm", &["--desktop-pref"])],
    );
    add_launcher_row(
        &body,
        "input-keyboard-symbolic",
        "Keyboard & Mouse",
        "Pointer and keyboard preferences",
        &[("lxinput", &[])],
    );

    let status = Label::new(Some("Unavailable tools are disabled instead of simulated."));
    status.set_xalign(0.0);
    status.style_context().add_class("slopos-statusbar");
    body.pack_end(&status, false, false, 0);

    window.add(&body);
    window.show_all();
    gtk::main();
}

fn add_launcher_row(
    parent: &GtkBox,
    icon_name: &str,
    title_text: &str,
    description_text: &str,
    candidates: &[(&str, &[&str])],
) {
    let row = GtkBox::new(Orientation::Horizontal, 9);
    row.style_context().add_class("slopos-preference-row");

    row.pack_start(&Image::from_icon_name(Some(icon_name), IconSize::Dnd), false, false, 0);

    let text = GtkBox::new(Orientation::Vertical, 1);
    let title = Label::new(Some(title_text));
    title.set_xalign(0.0);
    title.style_context().add_class("slopos-result-title");
    text.pack_start(&title, false, false, 0);

    let description = Label::new(Some(description_text));
    description.set_xalign(0.0);
    description.style_context().add_class("slopos-secondary-text");
    text.pack_start(&description, false, false, 0);
    row.pack_start(&text, true, true, 0);

    let selected = candidates.iter().find(|(program, _)| command_exists(program)).map(|(program, args)| {
        (
            (*program).to_string(),
            args.iter().map(|arg| (*arg).to_string()).collect::<Vec<_>>(),
        )
    });

    let button = Button::with_label(if selected.is_some() { "Open…" } else { "Not installed" });
    if let Some((program, args)) = selected {
        button.connect_clicked(move |_| {
            if let Err(err) = Command::new(&program).args(&args).spawn() {
                log::warn!("Failed to launch {program}: {err}");
            }
        });
    } else {
        button.set_sensitive(false);
    }
    row.pack_end(&button, false, false, 0);
    parent.pack_start(&row, false, false, 0);
}

fn command_exists(program: &str) -> bool {
    if program.contains('/') { return Path::new(program).is_file(); }
    let Some(path) = env::var_os("PATH") else { return false; };
    env::split_paths(&path).any(|dir| dir.join(program).is_file())
}

fn load_css_theme() {
    for path in [
        "assets/config/gtk-3.0/gtk.css",
        "/usr/local/share/themes/slopos-gtk/gtk-3.0/gtk.css",
        "/usr/share/themes/slopos-gtk/gtk-3.0/gtk.css",
    ] {
        if !Path::new(path).exists() { continue; }
        let provider = gtk::CssProvider::new();
        if provider.load_from_path(path).is_ok() {
            if let Some(screen) = gdk::Screen::default() {
                gtk::StyleContext::add_provider_for_screen(
                    &screen,
                    &provider,
                    gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
                );
            }
            break;
        }
    }
}
