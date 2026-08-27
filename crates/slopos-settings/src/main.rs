//! SLOPOS-I Settings hub.
//!
//! The parity branch presents settings as a compact Control Panels folder:
//! icon-first, dense, and visually consistent with the classic desktop rather
//! than a modern card dashboard. Mature Linux utilities remain authoritative
//! for hardware and system services.

pub mod panels;
pub mod providers;
pub mod theme;

use gdk_pixbuf::Pixbuf;
use gtk::atk::prelude::AtkObjectExt;
use gtk::prelude::*;
use gtk::{
    Align, Box as GtkBox, Button, Grid, IconSize, Image, Label, Orientation, ReliefStyle, Window,
    WindowPosition, WindowType,
};
use panels::{
    show_appearance_dialog, show_datetime_dialog, show_network_dialog, show_sound_dialog,
    show_wallpaper_dialog,
};
use providers::availability::command_exists;
use std::env;
use std::path::Path;
use std::process::Command;
use theme::load_css_theme;

pub struct ControlPanel<'a> {
    pub icon_file: &'a str,
    pub fallback_icon: &'a str,
    pub title: &'a str,
    pub description: &'a str,
    pub candidates: &'a [(&'a str, &'a [&'a str])],
    pub built_in: BuiltInPanel,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BuiltInPanel {
    None,
    Appearance,
    Desktop,
    DateTime,
    Sound,
    Network,
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    gtk::init().expect("Failed to initialize GTK3");
    load_css_theme();

    let window = Window::new(WindowType::Toplevel);
    window.set_title("Control Panels");
    set_accessible_name(&window, "SLOPOS Control Panels");
    let (screen_width, screen_height) = screen_geometry();
    let (window_width, window_height) = adaptive_window_size(screen_width, screen_height);
    window.set_default_size(window_width, window_height);
    window.set_position(WindowPosition::Center);
    window.connect_delete_event(|_, _| {
        gtk::main_quit();
        glib::Propagation::Proceed
    });

    let args: Vec<String> = env::args().collect();
    if args.iter().any(|arg| {
        matches!(
            arg.as_str(),
            "--datetime" | "--panel=datetime" | "date-time" | "datetime"
        )
    }) {
        show_datetime_dialog(&window);
        return;
    }
    if args.iter().any(|arg| {
        matches!(
            arg.as_str(),
            "--wallpaper" | "--panel=wallpaper" | "wallpaper"
        )
    }) {
        show_wallpaper_dialog(&window);
        return;
    }
    if args.iter().any(|arg| {
        matches!(
            arg.as_str(),
            "--appearance" | "--panel=appearance" | "appearance"
        )
    }) {
        show_appearance_dialog(&window);
        return;
    }
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--sound" | "--panel=sound" | "sound"))
    {
        show_sound_dialog(&window);
        return;
    }
    if args.iter().any(|arg| {
        matches!(
            arg.as_str(),
            "--network" | "--panel=network" | "network" | "--wifi" | "wifi"
        )
    }) {
        show_network_dialog(&window);
        return;
    }

    let body = GtkBox::new(Orientation::Vertical, 3);
    body.style_context().add_class("slopos-window-body");

    let hint = Label::new(Some("Control Panels"));
    hint.set_xalign(0.0);
    hint.style_context().add_class("slopos-folder-caption");
    set_accessible_name(&hint, "Control Panels folder");
    body.pack_start(&hint, false, false, 0);

    let grid = Grid::new();
    grid.style_context().add_class("slopos-icon-grid");
    grid.set_row_spacing(5);
    grid.set_column_spacing(4);
    grid.set_column_homogeneous(true);
    grid.set_row_homogeneous(false);
    grid.set_hexpand(true);
    grid.set_vexpand(false);

    let panels = [
        ControlPanel {
            icon_file: "display.svg",
            fallback_icon: "video-display-symbolic",
            title: "Displays",
            description: "Resolution and monitor layout",
            candidates: &[
                ("arandr", &[]),
                ("xfce4-display-settings", &[]),
                ("lxrandr", &[]),
            ],
            built_in: BuiltInPanel::None,
        },
        ControlPanel {
            icon_file: "sound.svg",
            fallback_icon: "audio-card-symbolic",
            title: "Sound",
            description: "Input, output and volume",
            candidates: &[("pavucontrol", &[])],
            built_in: BuiltInPanel::Sound,
        },
        ControlPanel {
            icon_file: "network.svg",
            fallback_icon: "network-wireless-symbolic",
            title: "Network",
            description: "Wi-Fi and Ethernet",
            candidates: &[("nm-connection-editor", &[])],
            built_in: BuiltInPanel::Network,
        },
        ControlPanel {
            icon_file: "bluetooth.svg",
            fallback_icon: "bluetooth-symbolic",
            title: "Bluetooth",
            description: "Pair and manage devices",
            candidates: &[("blueman-manager", &[])],
            built_in: BuiltInPanel::None,
        },
        ControlPanel {
            icon_file: "power.svg",
            fallback_icon: "battery-good-symbolic",
            title: "Power",
            description: "Sleep, lid and battery",
            candidates: &[("xfce4-power-manager-settings", &[])],
            built_in: BuiltInPanel::None,
        },
        ControlPanel {
            icon_file: "appearance.svg",
            fallback_icon: "preferences-desktop-theme-symbolic",
            title: "Appearance",
            description: "Theme and typography",
            candidates: &[],
            built_in: BuiltInPanel::Appearance,
        },
        ControlPanel {
            icon_file: "desktop.svg",
            fallback_icon: "preferences-desktop-wallpaper-symbolic",
            title: "Desktop",
            description: "Wallpaper and desktop background",
            candidates: &[],
            built_in: BuiltInPanel::Desktop,
        },
        ControlPanel {
            icon_file: "keyboard.svg",
            fallback_icon: "input-keyboard-symbolic",
            title: "Keyboard & Mouse",
            description: "Pointer and keyboard preferences",
            candidates: &[
                ("lxinput", &[]),
                ("xfce4-mouse-settings", &[]),
                ("xfce4-keyboard-settings", &[]),
            ],
            built_in: BuiltInPanel::None,
        },
        ControlPanel {
            icon_file: "date-time.svg",
            fallback_icon: "preferences-system-time-symbolic",
            title: "Date & Time",
            description: "Timezone and automatic clock sync",
            candidates: &[],
            built_in: BuiltInPanel::DateTime,
        },
    ];

    for (index, panel) in panels.iter().enumerate() {
        let button = control_panel_button(panel, &window);
        grid.attach(&button, (index % 5) as i32, (index / 5) as i32, 1, 1);
    }
    body.pack_start(&grid, false, false, 2);

    let status = Label::new(Some(
        "Unavailable control panels are dimmed when their system utility is not installed.",
    ));
    status.set_xalign(0.0);
    status.style_context().add_class("slopos-statusbar");
    set_accessible_name(&status, "Settings availability status");
    body.pack_end(&status, false, false, 0);

    window.add(&body);
    window.show_all();
    gtk::main();
}

fn control_panel_button(panel: &ControlPanel<'_>, parent: &Window) -> Button {
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
    button.set_relief(ReliefStyle::None);
    button.style_context().add_class("slopos-control-panel-icon");
    button.set_hexpand(true);
    button.set_vexpand(false);
    set_accessible_name(&button, &format!("{} settings", panel.title));

    let available = panel.built_in != BuiltInPanel::None || selected.is_some();
    if available {
        button.set_tooltip_text(Some(panel.description));
    } else {
        button.set_tooltip_text(Some("Required system utility is not installed"));
    }

    let content = GtkBox::new(Orientation::Vertical, 2);
    content.set_halign(Align::Center);
    content.set_valign(Align::Start);

    let icon = load_control_icon(panel.icon_file, panel.fallback_icon);
    icon.set_pixel_size(32);
    icon.style_context().add_class("slopos-control-icon");
    content.pack_start(&icon, false, false, 0);

    let title = Label::new(Some(panel.title));
    title.set_xalign(0.5);
    title.set_justify(gtk::Justification::Center);
    title.set_line_wrap(true);
    title.set_max_width_chars(15);
    title.style_context().add_class("slopos-control-title");
    content.pack_start(&title, false, false, 0);
    button.add(&content);

    match panel.built_in {
        BuiltInPanel::Appearance => {
            let parent = parent.clone();
            button.connect_clicked(move |_| show_appearance_dialog(&parent));
        }
        BuiltInPanel::Desktop => {
            let parent = parent.clone();
            button.connect_clicked(move |_| show_wallpaper_dialog(&parent));
        }
        BuiltInPanel::DateTime => {
            let parent = parent.clone();
            button.connect_clicked(move |_| show_datetime_dialog(&parent));
        }
        BuiltInPanel::Sound => {
            let parent = parent.clone();
            button.connect_clicked(move |_| show_sound_dialog(&parent));
        }
        BuiltInPanel::Network => {
            let parent = parent.clone();
            button.connect_clicked(move |_| show_network_dialog(&parent));
        }
        BuiltInPanel::None => {
            if let Some((program, args)) = selected {
                button.connect_clicked(move |_| {
                    if let Err(error) = Command::new(&program).args(&args).spawn() {
                        log::warn!("Failed to launch {program}: {error}");
                    }
                });
            } else {
                button.set_sensitive(false);
            }
        }
    }

    button
}

fn load_control_icon(file_name: &str, fallback: &str) -> Image {
    let mut candidates = Vec::new();
    if let Ok(share) = env::var("SLOPOS_SHARE_DIR") {
        candidates.push(format!("{share}/themes/platinum/icons/{file_name}"));
        candidates.push(format!(
            "{share}/slopos-i/themes/platinum/icons/{file_name}"
        ));
    }
    candidates.extend([
        format!("themes/platinum/icons/{file_name}"),
        format!("/usr/local/share/slopos-i/themes/platinum/icons/{file_name}"),
        format!("/usr/share/slopos-i/themes/platinum/icons/{file_name}"),
    ]);
    for path in candidates {
        if Path::new(&path).exists() {
            if let Ok(pixbuf) = Pixbuf::from_file_at_scale(&path, 32, 32, true) {
                return Image::from_pixbuf(Some(&pixbuf));
            }
        }
    }
    Image::from_icon_name(Some(fallback), IconSize::LargeToolbar)
}

fn screen_geometry() -> (i32, i32) {
    gdk::Display::default()
        .and_then(|display| {
            display
                .primary_monitor()
                .or_else(|| display.monitor(0))
                .map(|monitor| {
                    let geom = monitor.geometry();
                    (geom.width().max(1), geom.height().max(1))
                })
        })
        .unwrap_or((1280, 800))
}

fn adaptive_window_size(screen_width: i32, screen_height: i32) -> (i32, i32) {
    let width = if screen_width <= 1600 {
        560
    } else {
        (screen_width / 3).clamp(600, 720)
    };
    let height = if screen_height <= 900 {
        350
    } else {
        (screen_height * 2 / 5).clamp(390, 480)
    };
    (width, height)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_window_is_compact_but_scales_for_large_displays() {
        assert_eq!(adaptive_window_size(1280, 800), (560, 350));
        assert_eq!(adaptive_window_size(1920, 1080), (640, 432));
        assert_eq!(adaptive_window_size(2560, 1440), (720, 480));
    }
}
