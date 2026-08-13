//! SLOPOS-I Settings hub.
//!
//! The hub provides a consistent SLOPOS control-panel surface while delegating
//! system mutation to mature upstream X11/Linux configuration utilities.

use gdk_pixbuf::Pixbuf;
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
    icon_file: &'a str,
    fallback_icon: &'a str,
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
    // room for four rows of delegated utilities. Retained high-resolution
    // captures get a bounded larger surface; canonical 1x layouts retain the
    // historical dimensions.
    let (screen_width, screen_height) = screen_geometry();
    let (window_width, window_height) = adaptive_window_size(screen_width, screen_height);
    window.set_default_size(640, 460);
    if (window_width, window_height) != (640, 460) {
        window.set_default_size(window_width, window_height);
    }
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
            icon_file: "display.svg",
            fallback_icon: "video-display-symbolic",
            title: "Displays",
            description: "Resolution, rotation and monitor layout",
            candidates: &[("arandr", &[]), ("lxrandr", &[])],
        },
        ControlPanel {
            icon_file: "sound.svg",
            fallback_icon: "audio-card-symbolic",
            title: "Sound",
            description: "Output, input devices and volume",
            candidates: &[("pavucontrol", &[])],
        },
        ControlPanel {
            icon_file: "network.svg",
            fallback_icon: "network-wireless-symbolic",
            title: "Network",
            description: "Wi-Fi, Ethernet and saved connections",
            candidates: &[("nm-connection-editor", &[])],
        },
        ControlPanel {
            icon_file: "bluetooth.svg",
            fallback_icon: "bluetooth-symbolic",
            title: "Bluetooth",
            description: "Discover, pair and manage devices",
            candidates: &[("blueman-manager", &[])],
        },
        ControlPanel {
            icon_file: "power.svg",
            fallback_icon: "battery-good-symbolic",
            title: "Power",
            description: "Sleep, lid and battery behaviour",
            candidates: &[("xfce4-power-manager-settings", &[])],
        },
        ControlPanel {
            icon_file: "appearance.svg",
            fallback_icon: "preferences-desktop-theme-symbolic",
            title: "Appearance",
            description: "GTK theme, icons and font preferences",
            candidates: &[("lxappearance", &[])],
        },
        ControlPanel {
            icon_file: "desktop.svg",
            fallback_icon: "preferences-desktop-wallpaper-symbolic",
            title: "Desktop",
            description: "Wallpaper and desktop presentation",
            candidates: &[("pcmanfm", &["--desktop-pref"])],
        },
        ControlPanel {
            icon_file: "keyboard.svg",
            fallback_icon: "input-keyboard-symbolic",
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

    let icon = load_control_icon(panel.icon_file, panel.fallback_icon);
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

fn adaptive_window_size(screen_width: i32, screen_height: i32) -> (i32, i32) {
    let width = if screen_width <= 1600 {
        640
    } else {
        (screen_width * 2 / 5).clamp(720, 1080)
    };
    let height = if screen_height <= 1000 {
        460
    } else {
        (screen_height * 7 / 12).clamp(560, 720)
    };
    (width, height)
}

fn screen_geometry() -> (i32, i32) {
    let Ok(output) = Command::new("xrandr").arg("--current").output() else {
        return (1280, 800);
    };
    if !output.status.success() {
        return (1280, 800);
    }

    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        let Some(after_current) = line.split("current ").nth(1) else {
            continue;
        };
        let Some(dimensions) = after_current.split(',').next() else {
            continue;
        };
        let mut parts = dimensions.split('x').map(str::trim);
        let (Some(width), Some(height)) = (parts.next(), parts.next()) else {
            continue;
        };
        if let (Ok(width), Ok(height)) = (width.parse::<i32>(), height.parse::<i32>()) {
            let scale = env::var("GDK_SCALE")
                .ok()
                .and_then(|value| value.parse::<i32>().ok())
                .filter(|scale| *scale > 0)
                .unwrap_or(1);
            return ((width / scale).max(1), (height / scale).max(1));
        }
    }
    (1280, 800)
}

fn load_control_icon(file_name: &str, fallback: &str) -> Image {
    let mut candidates = Vec::new();
    if let Ok(share_dir) = env::var("SLOPOS_SHARE_DIR") {
        candidates.push(
            PathBuf::from(share_dir)
                .join("slopos-i/themes/platinum/icons")
                .join(file_name),
        );
    }
    candidates.extend([
        PathBuf::from("themes/platinum/icons").join(file_name),
        PathBuf::from("/usr/local/share/slopos-i/themes/platinum/icons").join(file_name),
        PathBuf::from("/usr/share/slopos-i/themes/platinum/icons").join(file_name),
    ]);
    for path in candidates {
        if path.is_file() {
            if let Ok(pixbuf) = Pixbuf::from_file_at_scale(&path, 32, 32, true) {
                return Image::from_pixbuf(Some(&pixbuf));
            }
        }
    }
    Image::from_icon_name(Some(fallback), IconSize::LargeToolbar)
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

#[cfg(test)]
mod tests {
    use super::adaptive_window_size;

    #[test]
    fn settings_keeps_compact_canonical_size_and_scales_large_surfaces() {
        assert_eq!(adaptive_window_size(1366, 768), (640, 460));
        assert_eq!(adaptive_window_size(1280, 800), (640, 460));
        assert_eq!(adaptive_window_size(3440, 1440), (1080, 720));
        assert_eq!(adaptive_window_size(7680, 4320), (1080, 720));
    }
}
