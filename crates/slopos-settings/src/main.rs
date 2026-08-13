//! SLOPOS-I Settings hub.
//!
//! The hub provides a consistent SLOPOS control-panel surface while delegating
//! system mutation to mature upstream X11/Linux configuration utilities.

use gtk::atk::prelude::AtkObjectExt;
use gtk::prelude::*;
use gtk::{
    Align, Box as GtkBox, Button, Grid, IconSize, Image, Label, Orientation, Window,
    WindowPosition, WindowType,
};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

struct ControlPanel<'a> {
    icon: &'a str,
    title: &'a str,
    description: &'a str,
    candidates: &'a [(&'a str, &'a [&'a str])],
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    gtk::init().expect("Failed to initialize GTK3");
    load_css_theme();

    let window = Window::new(WindowType::Toplevel);
    window.set_title("System Settings");
    set_accessible_name(&window, "SLOPOS system settings");
    // Keep the hub compact like a classic control panel while leaving enough
    // room for four rows of delegated utilities at 1x scaling.
    window.set_default_size(640, 460);
    window.set_position(WindowPosition::Center);
    window.connect_delete_event(|_, _| {
        gtk::main_quit();
        glib::Propagation::Proceed
    });

    let body = GtkBox::new(Orientation::Vertical, 6);
    body.style_context().add_class("slopos-window-body");

    let title = Label::new(Some("System Settings"));
    title.set_xalign(0.0);
    title.style_context().add_class("slopos-panel-title");
    set_accessible_name(&title, "System Settings");
    body.pack_start(&title, false, false, 0);

    let subtitle = Label::new(Some(
        "Open a control panel to configure your Linux desktop and hardware.",
    ));
    subtitle.set_xalign(0.0);
    subtitle.style_context().add_class("slopos-panel-subtitle");
    body.pack_start(&subtitle, false, false, 0);

    let separator = gtk::Separator::new(Orientation::Horizontal);
    body.pack_start(&separator, false, false, 0);

    let grid = Grid::new();
    grid.set_row_spacing(6);
    grid.set_column_spacing(6);
    grid.set_column_homogeneous(true);
    grid.set_row_homogeneous(true);
    grid.set_hexpand(true);
    grid.set_vexpand(true);

    let panels = [
        ControlPanel {
            icon: "video-display-symbolic",
            title: "Displays",
            description: "Resolution, rotation and monitor layout",
            candidates: &[("arandr", &[]), ("lxrandr", &[])],
        },
        ControlPanel {
            icon: "audio-card-symbolic",
            title: "Sound",
            description: "Output, input devices and volume",
            candidates: &[("pavucontrol", &[])],
        },
        ControlPanel {
            icon: "network-wireless-symbolic",
            title: "Network",
            description: "Wi-Fi, Ethernet and saved connections",
            candidates: &[("nm-connection-editor", &[])],
        },
        ControlPanel {
            icon: "bluetooth-symbolic",
            title: "Bluetooth",
            description: "Discover, pair and manage devices",
            candidates: &[("blueman-manager", &[])],
        },
        ControlPanel {
            icon: "battery-good-symbolic",
            title: "Power",
            description: "Sleep, lid and battery behaviour",
            candidates: &[("xfce4-power-manager-settings", &[])],
        },
        ControlPanel {
            icon: "preferences-desktop-theme-symbolic",
            title: "Appearance",
            description: "GTK theme, icons and font preferences",
            candidates: &[("lxappearance", &[])],
        },
        ControlPanel {
            icon: "preferences-desktop-wallpaper-symbolic",
            title: "Desktop",
            description: "Wallpaper and desktop presentation",
            candidates: &[("pcmanfm", &["--desktop-pref"])],
        },
        ControlPanel {
            icon: "input-keyboard-symbolic",
            title: "Keyboard & Mouse",
            description: "Pointer and keyboard preferences",
            candidates: &[("lxinput", &[])],
        },
    ];

    for (index, panel) in panels.iter().enumerate() {
        let button = control_panel_button(panel);
        grid.attach(&button, (index % 2) as i32, (index / 2) as i32, 1, 1);
    }
    body.pack_start(&grid, true, true, 0);

    let status = Label::new(Some(
        "Unavailable control panels are disabled rather than simulated.",
    ));
    status.set_xalign(0.0);
    status.style_context().add_class("slopos-statusbar");
    set_accessible_name(&status, "Settings availability status");
    body.pack_end(&status, false, false, 0);

    window.add(&body);
    window.show_all();
    gtk::main();
}

fn control_panel_button(panel: &ControlPanel<'_>) -> Button {
    let selected = panel
        .candidates
        .iter()
        .find(|(program, _)| command_exists(program))
        .map(|(program, args)| {
            (
                (*program).to_string(),
                args.iter()
                    .map(|argument| (*argument).to_string())
                    .collect::<Vec<_>>(),
            )
        });

    let button = Button::new();
    button.style_context().add_class("slopos-control-panel");
    button.set_hexpand(true);
    button.set_vexpand(false);
    button.set_tooltip_text(Some(panel.description));
    let accessible_name = format!("{} settings", panel.title);
    set_accessible_name(&button, &accessible_name);

    let content = GtkBox::new(Orientation::Horizontal, 10);
    content.set_halign(Align::Fill);
    content.set_valign(Align::Center);

    let icon = Image::from_icon_name(Some(panel.icon), IconSize::LargeToolbar);
    icon.set_pixel_size(32);
    icon.style_context().add_class("slopos-control-icon");
    content.pack_start(&icon, false, false, 0);

    let labels = GtkBox::new(Orientation::Vertical, 2);
    labels.set_valign(Align::Center);
    let title = Label::new(Some(panel.title));
    title.set_xalign(0.0);
    title.style_context().add_class("slopos-control-title");
    labels.pack_start(&title, false, false, 0);

    let description = if selected.is_some() {
        panel.description.to_string()
    } else {
        format!("{} — utility not installed", panel.description)
    };
    let subtitle = Label::new(Some(&description));
    subtitle.set_xalign(0.0);
    subtitle.set_line_wrap(true);
    subtitle.style_context().add_class("slopos-secondary-text");
    labels.pack_start(&subtitle, false, false, 0);
    content.pack_start(&labels, true, true, 0);
    button.add(&content);

    if let Some((program, args)) = selected {
        button.connect_clicked(move |_| {
            if let Err(error) = Command::new(&program).args(&args).spawn() {
                log::warn!("Failed to launch {program}: {error}");
            }
        });
    } else {
        button.set_sensitive(false);
    }

    button
}

fn command_exists(program: &str) -> bool {
    if program.contains('/') {
        return Path::new(program).is_file();
    }
    let Some(path) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&path).any(|dir| dir.join(program).is_file())
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

fn load_css_theme() {
    let mut css_paths = Vec::new();
    if let Ok(share_dir) = env::var("SLOPOS_SHARE_DIR") {
        css_paths.push(PathBuf::from(share_dir).join("themes/slopos-gtk/gtk-3.0/gtk.css"));
    }
    css_paths.extend([
        PathBuf::from("assets/config/gtk-3.0/gtk.css"),
        PathBuf::from("/usr/local/share/themes/slopos-gtk/gtk-3.0/gtk.css"),
        PathBuf::from("/usr/share/themes/slopos-gtk/gtk-3.0/gtk.css"),
    ]);
    for path in css_paths {
        if !path.exists() {
            continue;
        }
        let provider = gtk::CssProvider::new();
        let Some(path_text) = path.to_str() else {
            continue;
        };
        if provider.load_from_path(path_text).is_ok() {
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
