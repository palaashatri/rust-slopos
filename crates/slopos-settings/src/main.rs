//! SLOPOS-I Settings hub.
//!
//! SLOPOS owns the coherent control-panel entry point while mature upstream
//! X11/Linux utilities perform hardware and service mutation.

use gdk_pixbuf::Pixbuf;
use gtk::atk::prelude::AtkObjectExt;
use gtk::prelude::*;
use gtk::{
    Align, Box as GtkBox, Button, Dialog, DialogFlags, Grid, IconSize, Image, Label, Orientation,
    RadioButton, ResponseType, Window, WindowPosition, WindowType,
};
use std::env;
use std::path::PathBuf;
use std::process::Command;

struct ControlPanel<'a> {
    icon_file: &'a str,
    fallback_icon: &'a str,
    title: &'a str,
    description: &'a str,
    candidates: &'a [(&'a str, &'a [&'a str])],
    built_in: bool,
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    gtk::init().expect("Failed to initialize GTK3");
    load_css_theme();

    let window = Window::new(WindowType::Toplevel);
    window.set_title("System Settings");
    set_accessible_name(&window, "SLOPOS system settings");
    let (screen_width, screen_height) = screen_geometry();
    let (window_width, window_height) = adaptive_window_size(screen_width, screen_height);
    window.set_default_size(window_width, window_height);
    window.set_position(WindowPosition::Center);
    window.connect_delete_event(|_, _| {
        gtk::main_quit();
        glib::Propagation::Proceed
    });

    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| {
        a == "--datetime" || a == "--panel=datetime" || a == "date-time" || a == "datetime"
    }) {
        show_datetime_dialog(&window);
        return;
    }
    if args
        .iter()
        .any(|a| a == "--wallpaper" || a == "--panel=wallpaper" || a == "wallpaper")
    {
        show_wallpaper_dialog(&window);
        return;
    }
    if args
        .iter()
        .any(|a| a == "--appearance" || a == "--panel=appearance" || a == "appearance")
    {
        show_appearance_dialog(&window);
        return;
    }

    let body = GtkBox::new(Orientation::Vertical, 6);
    body.style_context().add_class("slopos-window-body");

    let title = Label::new(Some("Control Panels"));
    title.set_xalign(0.0);
    title.style_context().add_class("slopos-panel-title");
    set_accessible_name(&title, "Control Panels");
    body.pack_start(&title, false, false, 0);

    let subtitle = Label::new(Some(
        "Configure the desktop and open the system utility responsible for each device or service.",
    ));
    subtitle.set_xalign(0.0);
    subtitle.set_line_wrap(true);
    subtitle.style_context().add_class("slopos-panel-subtitle");
    body.pack_start(&subtitle, false, false, 0);

    body.pack_start(
        &gtk::Separator::new(Orientation::Horizontal),
        false,
        false,
        0,
    );

    let grid = Grid::new();
    grid.set_row_spacing(7);
    grid.set_column_spacing(7);
    grid.set_column_homogeneous(true);
    grid.set_row_homogeneous(true);
    grid.set_hexpand(true);
    grid.set_vexpand(true);

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
            built_in: false,
        },
        ControlPanel {
            icon_file: "sound.svg",
            fallback_icon: "audio-card-symbolic",
            title: "Sound",
            description: "Input, output and volume",
            candidates: &[("pavucontrol", &[])],
            built_in: false,
        },
        ControlPanel {
            icon_file: "network.svg",
            fallback_icon: "network-wireless-symbolic",
            title: "Network",
            description: "Wi-Fi and Ethernet",
            candidates: &[("nm-connection-editor", &[])],
            built_in: false,
        },
        ControlPanel {
            icon_file: "bluetooth.svg",
            fallback_icon: "bluetooth-symbolic",
            title: "Bluetooth",
            description: "Pair and manage devices",
            candidates: &[("blueman-manager", &[])],
            built_in: false,
        },
        ControlPanel {
            icon_file: "power.svg",
            fallback_icon: "battery-good-symbolic",
            title: "Power",
            description: "Sleep, lid and battery",
            candidates: &[("xfce4-power-manager-settings", &[])],
            built_in: false,
        },
        ControlPanel {
            icon_file: "appearance.svg",
            fallback_icon: "preferences-desktop-theme-symbolic",
            title: "Appearance",
            description: "Platinum or Graphite",
            candidates: &[],
            built_in: true,
        },
        ControlPanel {
            icon_file: "desktop.svg",
            fallback_icon: "preferences-desktop-wallpaper-symbolic",
            title: "Desktop",
            description: "Wallpaper and desktop icons",
            candidates: &[("pcmanfm", &["--desktop-pref"])],
            built_in: false,
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
            built_in: false,
        },
    ];

    for (index, panel) in panels.iter().enumerate() {
        let button = control_panel_button(panel, &window);
        grid.attach(&button, (index % 4) as i32, (index / 4) as i32, 1, 1);
    }
    body.pack_start(&grid, true, true, 0);

    let status = Label::new(Some(
        "SLOPOS provides the control-panel surface; mature Linux tools perform system changes.",
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
    button.style_context().add_class("slopos-control-panel");
    button.set_hexpand(true);
    button.set_vexpand(true);
    button.set_tooltip_text(Some(panel.description));
    set_accessible_name(&button, &format!("{} settings", panel.title));

    let content = GtkBox::new(Orientation::Vertical, 4);
    content.set_halign(Align::Center);
    content.set_valign(Align::Center);

    let icon = load_control_icon(panel.icon_file, panel.fallback_icon);
    icon.set_pixel_size(32);
    icon.style_context().add_class("slopos-control-icon");
    content.pack_start(&icon, false, false, 0);

    let title = Label::new(Some(panel.title));
    title.set_xalign(0.5);
    title.style_context().add_class("slopos-control-title");
    content.pack_start(&title, false, false, 0);

    let description = if panel.built_in || selected.is_some() {
        panel.description.to_string()
    } else {
        "Utility not installed".to_string()
    };
    let subtitle = Label::new(Some(&description));
    subtitle.set_xalign(0.5);
    subtitle.set_justify(gtk::Justification::Center);
    subtitle.set_line_wrap(true);
    subtitle.set_max_width_chars(24);
    subtitle.style_context().add_class("slopos-secondary-text");
    content.pack_start(&subtitle, false, false, 0);
    button.add(&content);

    if panel.built_in && panel.title == "Appearance" {
        let parent = parent.clone();
        button.connect_clicked(move |_| show_appearance_dialog(&parent));
    } else if panel.built_in && panel.title.contains("Desktop") {
        let parent = parent.clone();
        button.connect_clicked(move |_| show_wallpaper_dialog(&parent));
    } else if panel.built_in && panel.title.contains("Date") {
        let parent = parent.clone();
        button.connect_clicked(move |_| show_datetime_dialog(&parent));
    } else if let Some((program, args)) = selected {
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

fn show_appearance_dialog(parent: &Window) {
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
    set_accessible_name(&dialog, "SLOPOS appearance chooser");

    let content = dialog.content_area();
    content.set_spacing(8);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_top(10);
    content.set_margin_bottom(10);

    let heading = Label::new(Some("Desktop Appearance"));
    heading.set_xalign(0.0);
    heading.style_context().add_class("slopos-control-title");
    content.pack_start(&heading, false, false, 0);

    let explanation = Label::new(Some(
        "Choose between Classic Macintosh (System 6/7 monochrome), Platinum (classic light), Graphite (dark), and OLED Dark (pure black contrast).",
    ));
    explanation.set_xalign(0.0);
    explanation.set_line_wrap(true);
    explanation
        .style_context()
        .add_class("slopos-secondary-text");
    content.pack_start(&explanation, false, false, 0);

    let classic = RadioButton::with_label("Classic Macintosh — System 6/7 monochrome");
    let platinum = RadioButton::with_label_from_widget(&classic, "Platinum — classic light");
    let graphite = RadioButton::with_label_from_widget(&classic, "Graphite — dark");
    let oled = RadioButton::with_label_from_widget(&classic, "OLED Dark — pure black contrast");
    match current_appearance() {
        "oled" => oled.set_active(true),
        "graphite" => graphite.set_active(true),
        "classic" => classic.set_active(true),
        _ => platinum.set_active(true),
    }
    content.pack_start(&classic, false, false, 0);
    content.pack_start(&platinum, false, false, 0);
    content.pack_start(&graphite, false, false, 0);
    content.pack_start(&oled, false, false, 0);

    dialog.show_all();
    let response = dialog.run();
    if response == ResponseType::Accept {
        let mode = if oled.is_active() {
            "oled"
        } else if graphite.is_active() {
            "graphite"
        } else if classic.is_active() {
            "classic"
        } else {
            "platinum"
        };
        if let Some(helper) = appearance_helper() {
            match Command::new(helper).arg(mode).spawn() {
                Ok(_) => {
                    dialog.close();
                    gtk::main_quit();
                    return;
                }
                Err(error) => log::warn!("Failed to switch appearance: {error}"),
            }
        } else {
            log::warn!("slopos-appearance helper is unavailable");
        }
    }
    dialog.close();
}

fn show_wallpaper_dialog(parent: &Window) {
    let dialog = Dialog::with_buttons(
        Some("Desktop & Wallpaper"),
        Some(parent),
        DialogFlags::MODAL | DialogFlags::DESTROY_WITH_PARENT,
        &[
            ("Preferences…", ResponseType::Other(1)),
            ("Cancel", ResponseType::Cancel),
            ("Apply", ResponseType::Accept),
        ],
    );
    dialog.set_default_response(ResponseType::Accept);
    set_accessible_name(&dialog, "SLOPOS wallpaper chooser");

    let content = dialog.content_area();
    content.set_spacing(8);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_top(10);
    content.set_margin_bottom(10);

    let heading = Label::new(Some("Desktop Wallpaper & Patterns"));
    heading.set_xalign(0.0);
    heading.style_context().add_class("slopos-control-title");
    content.pack_start(&heading, false, false, 0);

    let explanation = Label::new(Some(
        "Select an authentic retro background pattern or choose a custom image:",
    ));
    explanation.set_xalign(0.0);
    explanation
        .style_context()
        .add_class("slopos-secondary-text");
    content.pack_start(&explanation, false, false, 0);

    let wp1 = RadioButton::with_label("01 Classic System Gray — 50% 1-Bit Dither");
    let wp2 =
        RadioButton::with_label_from_widget(&wp1, "02 Platinum Cool Slate — Fine Matrix (#758090)");
    let wp3 =
        RadioButton::with_label_from_widget(&wp1, "03 Vintage Mac Blue — Classic Tweed (#3A5F8B)");
    let wp4 = RadioButton::with_label_from_widget(
        &wp1,
        "04 Retro Teal Grid — 90s Desktop Matrix (#008080)",
    );
    let wp5 = RadioButton::with_label_from_widget(
        &wp1,
        "05 OLED Pure Dark — Obsidian Constellation (#000000)",
    );
    wp2.set_active(true);

    content.pack_start(&wp1, false, false, 0);
    content.pack_start(&wp2, false, false, 0);
    content.pack_start(&wp3, false, false, 0);
    content.pack_start(&wp4, false, false, 0);
    content.pack_start(&wp5, false, false, 0);

    let mode_box = GtkBox::new(Orientation::Horizontal, 8);
    let mode_label = Label::new(Some("Display Mode:"));
    let mode_combo = gtk::ComboBoxText::new();
    mode_combo.append(Some("fill"), "Fill / Stretch");
    mode_combo.append(Some("tile"), "Tile Pattern");
    mode_combo.append(Some("center"), "Center");
    mode_combo.set_active(Some(0));
    mode_box.pack_start(&mode_label, false, false, 0);
    mode_box.pack_start(&mode_combo, true, true, 0);
    content.pack_start(&mode_box, false, false, 0);

    dialog.show_all();
    let response = dialog.run();
    if response == ResponseType::Accept {
        let chosen_file = if wp1.is_active() {
            "01_classic_system_gray.png"
        } else if wp2.is_active() {
            "02_platinum_cool_slate.png"
        } else if wp3.is_active() {
            "03_vintage_mac_blue.png"
        } else if wp4.is_active() {
            "04_retro_teal_grid.png"
        } else {
            "05_oled_pure_dark.png"
        };
        let mode = mode_combo.active_id().unwrap_or_else(|| "fill".into());
        let _ = Command::new("scripts/slopos-wallpaper")
            .args(["set", chosen_file, "--mode", mode.as_str()])
            .spawn()
            .or_else(|_| {
                Command::new("slopos-wallpaper")
                    .args(["set", chosen_file, "--mode", mode.as_str()])
                    .spawn()
            });
    } else if response == ResponseType::Other(1) {
        let _ = Command::new("pcmanfm").arg("--desktop-pref").spawn();
    }
    dialog.close();
}

fn show_datetime_dialog(parent: &Window) {
    let dialog = Dialog::with_buttons(
        Some("Date & Time Settings"),
        Some(parent),
        DialogFlags::MODAL | DialogFlags::DESTROY_WITH_PARENT,
        &[
            ("Cancel", ResponseType::Cancel),
            ("Apply", ResponseType::Accept),
        ],
    );
    dialog.set_default_response(ResponseType::Accept);
    set_accessible_name(&dialog, "SLOPOS date and time settings");

    let content = dialog.content_area();
    content.set_spacing(8);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_top(10);
    content.set_margin_bottom(10);

    let heading = Label::new(Some("System Date & Time"));
    heading.set_xalign(0.0);
    heading.style_context().add_class("slopos-control-title");
    content.pack_start(&heading, false, false, 0);

    let current_dt = Command::new("date")
        .arg("+%A, %B %d, %Y — %H:%M:%S (%Z)")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_else(|| "Current System Time".into());
    let current_label = Label::new(Some(current_dt.trim()));
    current_label.set_xalign(0.0);
    current_label
        .style_context()
        .add_class("slopos-secondary-text");
    content.pack_start(&current_label, false, false, 0);

    content.pack_start(
        &gtk::Separator::new(Orientation::Horizontal),
        false,
        false,
        0,
    );

    let tz_box = GtkBox::new(Orientation::Horizontal, 8);
    let tz_label = Label::new(Some("Timezone:"));
    let tz_combo = gtk::ComboBoxText::new();
    tz_combo.append(Some("UTC"), "UTC (Coordinated Universal Time)");
    tz_combo.append(Some("America/New_York"), "America/New York (EST/EDT)");
    tz_combo.append(Some("America/Chicago"), "America/Chicago (CST/CDT)");
    tz_combo.append(Some("America/Denver"), "America/Denver (MST/MDT)");
    tz_combo.append(Some("America/Los_Angeles"), "America/Los Angeles (PST/PDT)");
    tz_combo.append(Some("Europe/London"), "Europe/London (GMT/BST)");
    tz_combo.append(Some("Europe/Paris"), "Europe/Paris (CET/CEST)");
    tz_combo.append(Some("Europe/Berlin"), "Europe/Berlin (CET/CEST)");
    tz_combo.append(Some("Asia/Tokyo"), "Asia/Tokyo (JST)");
    tz_combo.append(Some("Asia/Kolkata"), "Asia/Kolkata (IST)");
    tz_combo.append(Some("Australia/Sydney"), "Australia/Sydney (AEST)");
    tz_combo.set_active(Some(0));
    tz_box.pack_start(&tz_label, false, false, 0);
    tz_box.pack_start(&tz_combo, true, true, 0);
    content.pack_start(&tz_box, false, false, 0);

    let ntp_check = gtk::CheckButton::with_label(
        "Synchronize clock automatically via Network Time Protocol (NTP)",
    );
    ntp_check.set_active(true);
    content.pack_start(&ntp_check, false, false, 0);

    dialog.show_all();
    let response = dialog.run();
    if response == ResponseType::Accept {
        if let Some(tz) = tz_combo.active_id() {
            let _ = Command::new("timedatectl")
                .args(["set-timezone", tz.as_str()])
                .status();
        }
        if ntp_check.is_active() {
            let _ = Command::new("timedatectl")
                .args(["set-ntp", "true"])
                .status();
        }
    }
    dialog.close();
}

fn appearance_helper() -> Option<PathBuf> {
    if let Ok(executable) = env::current_exe() {
        if let Some(dir) = executable.parent() {
            let sibling = dir.join("slopos-appearance");
            if sibling.is_file() {
                return Some(sibling);
            }
        }
    }
    let local = PathBuf::from("scripts/slopos-appearance");
    if local.is_file() {
        return Some(local);
    }
    resolve_program_path("slopos-appearance")
}

fn command_exists(program: &str) -> bool {
    resolve_program_path(program).is_some()
}

fn resolve_program_path(program: &str) -> Option<PathBuf> {
    if program.contains('/') {
        let path = PathBuf::from(program);
        return path.is_file().then_some(path);
    }
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
}

fn adaptive_window_size(screen_width: i32, screen_height: i32) -> (i32, i32) {
    let width = if screen_width <= 1600 {
        640
    } else {
        (screen_width * 2 / 5).clamp(720, 960)
    };
    let height = if screen_height <= 1000 {
        390
    } else {
        (screen_height / 2).clamp(460, 620)
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

fn current_appearance() -> &'static str {
    if let Ok(env_appearance) = env::var("SLOPOS_APPEARANCE") {
        let v = env_appearance.trim();
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
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    if let Some(config_home) = config_home {
        if let Ok(value) = std::fs::read_to_string(config_home.join("slopos-i/appearance")) {
            let v = value.trim();
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

fn load_css_theme() {
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
    if let Ok(share_dir) = env::var("SLOPOS_SHARE_DIR") {
        css_paths.push(
            PathBuf::from(share_dir)
                .join("themes")
                .join(installed_theme)
                .join("gtk-3.0/gtk.css"),
        );
    }
    css_paths.extend([
        PathBuf::from(source_css),
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
        assert_eq!(adaptive_window_size(1366, 768), (640, 390));
        assert_eq!(adaptive_window_size(1280, 800), (640, 390));
        assert_eq!(adaptive_window_size(3440, 1440), (960, 620));
        assert_eq!(adaptive_window_size(7680, 4320), (960, 620));
    }
}
