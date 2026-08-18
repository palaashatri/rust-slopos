//! SLOPOS-I Settings hub.
//!
//! SLOPOS owns a coherent control-panel surface while mature Linux utilities
//! remain authoritative for hardware and system services.

use gdk_pixbuf::Pixbuf;
use gtk::atk::prelude::AtkObjectExt;
use gtk::prelude::*;
use gtk::{
    Align, Box as GtkBox, Button, CheckButton, ComboBoxText, Dialog, DialogFlags,
    FileChooserAction, FileChooserDialog, FileFilter, FontButton, Grid, IconSize, Image, Label,
    Orientation, RadioButton, ResponseType, ScrolledWindow, Window, WindowPosition, WindowType,
};
use std::cell::RefCell;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;

struct ControlPanel<'a> {
    icon_file: &'a str,
    fallback_icon: &'a str,
    title: &'a str,
    description: &'a str,
    candidates: &'a [(&'a str, &'a [&'a str])],
    built_in: BuiltInPanel,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BuiltInPanel {
    None,
    Appearance,
    Desktop,
    DateTime,
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

    let body = GtkBox::new(Orientation::Vertical, 8);
    body.style_context().add_class("slopos-window-body");

    let title = Label::new(Some("System Settings"));
    title.set_xalign(0.0);
    title.style_context().add_class("slopos-panel-title");
    set_accessible_name(&title, "System Settings");
    body.pack_start(&title, false, false, 0);

    let subtitle = Label::new(Some(
        "Personalize SLOPOS-I and open the installed Linux utility responsible for each device or service.",
    ));
    subtitle.set_xalign(0.0);
    subtitle.set_line_wrap(true);
    subtitle.style_context().add_class("slopos-panel-subtitle");
    body.pack_start(&subtitle, false, false, 0);

    let grid = Grid::new();
    grid.set_row_spacing(8);
    grid.set_column_spacing(8);
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
            built_in: BuiltInPanel::None,
        },
        ControlPanel {
            icon_file: "sound.svg",
            fallback_icon: "audio-card-symbolic",
            title: "Sound",
            description: "Input, output and volume",
            candidates: &[("pavucontrol", &[])],
            built_in: BuiltInPanel::None,
        },
        ControlPanel {
            icon_file: "network.svg",
            fallback_icon: "network-wireless-symbolic",
            title: "Network",
            description: "Wi-Fi and Ethernet",
            candidates: &[("nm-connection-editor", &[])],
            built_in: BuiltInPanel::None,
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
            description: "Theme, typography and desktop behavior",
            candidates: &[],
            built_in: BuiltInPanel::Appearance,
        },
        ControlPanel {
            icon_file: "desktop.svg",
            fallback_icon: "preferences-desktop-wallpaper-symbolic",
            title: "Desktop",
            description: "Wallpaper and background layout",
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
        grid.attach(&button, (index % 3) as i32, (index / 3) as i32, 1, 1);
    }
    body.pack_start(&grid, true, true, 0);

    let status = Label::new(Some(
        "Unavailable controls are disabled when their system utility is not installed.",
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

    let available = panel.built_in != BuiltInPanel::None || selected.is_some();
    let description = if available {
        panel.description
    } else {
        "Required utility is not installed"
    };
    let subtitle = Label::new(Some(description));
    subtitle.set_xalign(0.5);
    subtitle.set_justify(gtk::Justification::Center);
    subtitle.set_line_wrap(true);
    subtitle.set_max_width_chars(24);
    subtitle.style_context().add_class("slopos-secondary-text");
    content.pack_start(&subtitle, false, false, 0);
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
    explanation.style_context().add_class("slopos-secondary-text");
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
        description_label.style_context().add_class("slopos-secondary-text");
        labels.pack_start(&description_label, false, false, 0);
        row.pack_start(&labels, true, true, 0);
        presets_box.pack_start(&row, false, false, 0);
    }
    content.pack_start(&presets_box, true, true, 0);

    match current_appearance().as_str() {
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

        if let Some(helper) = resolve_slopos_program("slopos-appearance") {
            if let Err(error) = Command::new(helper).arg(mode).spawn() {
                log::warn!("Failed to apply appearance: {error}");
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
    explanation.style_context().add_class("slopos-secondary-text");
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
        (
            "03_slate_blue.png",
            "Slate Blue",
            "Deep blue woven pattern",
        ),
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
        description_label.style_context().add_class("slopos-secondary-text");
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
    custom_label.style_context().add_class("slopos-secondary-text");
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
            if let Some(helper) = resolve_slopos_program("slopos-wallpaper") {
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
        ResponseType::Other(1) => {
            if command_exists("pcmanfm") {
                let _ = Command::new("pcmanfm").arg("--desktop-pref").spawn();
            }
        }
        _ => {}
    }
    dialog.close();
}

fn show_datetime_dialog(parent: &Window) {
    let dialog = Dialog::with_buttons(
        Some("Date & Time"),
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
    content.set_spacing(10);
    content.set_margin_start(14);
    content.set_margin_end(14);
    content.set_margin_top(12);
    content.set_margin_bottom(12);

    let current = Command::new("date")
        .arg("+%A, %B %d, %Y — %H:%M:%S (%Z)")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .unwrap_or_else(|| "Current system time unavailable".to_string());
    let current_label = Label::new(Some(current.trim()));
    current_label.set_xalign(0.0);
    current_label.style_context().add_class("slopos-control-title");
    content.pack_start(&current_label, false, false, 0);

    let timezone_row = GtkBox::new(Orientation::Horizontal, 8);
    timezone_row.pack_start(&Label::new(Some("Timezone:")), false, false, 0);
    let timezone = ComboBoxText::new();
    for (id, label) in [
        ("UTC", "UTC"),
        ("America/New_York", "America/New York"),
        ("America/Chicago", "America/Chicago"),
        ("America/Denver", "America/Denver"),
        ("America/Los_Angeles", "America/Los Angeles"),
        ("Europe/London", "Europe/London"),
        ("Europe/Paris", "Europe/Paris"),
        ("Europe/Berlin", "Europe/Berlin"),
        ("Asia/Kolkata", "Asia/Kolkata"),
        ("Asia/Tokyo", "Asia/Tokyo"),
        ("Australia/Sydney", "Australia/Sydney"),
    ] {
        timezone.append(Some(id), label);
    }
    timezone.set_active_id(Some("UTC"));
    timezone_row.pack_start(&timezone, true, true, 0);
    content.pack_start(&timezone_row, false, false, 0);

    let ntp = CheckButton::with_label("Set time automatically using network time");
    ntp.set_active(true);
    content.pack_start(&ntp, false, false, 0);

    let note = Label::new(Some(
        "Changing system time may require administrator authorization from your Linux distribution.",
    ));
    note.set_xalign(0.0);
    note.set_line_wrap(true);
    note.style_context().add_class("slopos-secondary-text");
    content.pack_start(&note, false, false, 0);

    dialog.show_all();
    if dialog.run() == ResponseType::Accept {
        if !command_exists("timedatectl") {
            log::warn!("timedatectl is unavailable; date/time changes were not applied");
        } else {
            if let Some(id) = timezone.active_id() {
                let _ = Command::new("timedatectl")
                    .args(["set-timezone", id.as_str()])
                    .status();
            }
            let _ = Command::new("timedatectl")
                .args(["set-ntp", if ntp.is_active() { "true" } else { "false" }])
                .status();
        }
    }
    dialog.close();
}

fn load_theme_preview(name: &str) -> Option<Image> {
    let filename = format!("preview-{name}.png");
    theme_asset_candidates(&filename)
        .into_iter()
        .find_map(|path| {
            if !path.is_file() {
                return None;
            }
            Pixbuf::from_file_at_scale(path, 96, 60, true)
                .ok()
                .map(|pixbuf| Image::from_pixbuf(Some(&pixbuf)))
        })
}

fn theme_asset_candidates(filename: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(share) = env::var("SLOPOS_SHARE_DIR") {
        candidates.push(PathBuf::from(&share).join("themes").join(filename));
        candidates.push(PathBuf::from(share).join("slopos-i/themes").join(filename));
    }
    candidates.extend([
        PathBuf::from("assets/themes").join(filename),
        PathBuf::from("/usr/local/share/slopos-i/themes").join(filename),
        PathBuf::from("/usr/share/slopos-i/themes").join(filename),
    ]);
    candidates
}

fn current_appearance() -> String {
    if let Ok(value) = env::var("SLOPOS_APPEARANCE") {
        let value = value.trim().to_ascii_lowercase();
        if matches!(value.as_str(), "platinum" | "graphite" | "oled" | "classic") {
            return value;
        }
    }
    config_home()
        .and_then(|path| fs::read_to_string(path.join("slopos-i/appearance")).ok())
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| matches!(value.as_str(), "platinum" | "graphite" | "oled" | "classic"))
        .unwrap_or_else(|| "platinum".to_string())
}

fn current_font() -> String {
    config_home()
        .and_then(|path| fs::read_to_string(path.join("slopos-i/font")).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Liberation Sans 9".to_string())
}

fn save_font(font: &str) {
    let Some(config) = config_home() else {
        return;
    };
    let directory = config.join("slopos-i");
    if fs::create_dir_all(&directory).is_ok() {
        let _ = fs::write(directory.join("font"), format!("{font}\n"));
    }
}

fn is_dock_dodge_enabled() -> bool {
    config_home()
        .and_then(|path| fs::read_to_string(path.join("slopos-i/dock_dodge")).ok())
        .map(|value| matches!(value.trim(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

fn set_dock_dodge_enabled(enabled: bool) {
    let Some(config) = config_home() else {
        return;
    };
    let directory = config.join("slopos-i");
    if fs::create_dir_all(&directory).is_err() {
        return;
    }
    let _ = fs::write(
        directory.join("dock_dodge"),
        if enabled { "1\n" } else { "0\n" },
    );
}

fn config_home() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
}

fn find_wallpaper_path(filename: &str) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(share) = env::var("SLOPOS_SHARE_DIR") {
        candidates.push(PathBuf::from(&share).join("wallpapers").join(filename));
        candidates.push(PathBuf::from(share).join("slopos-i/wallpapers").join(filename));
    }
    candidates.extend([
        PathBuf::from("assets/wallpapers").join(filename),
        PathBuf::from("/usr/local/share/slopos-i/wallpapers").join(filename),
        PathBuf::from("/usr/share/slopos-i/wallpapers").join(filename),
    ]);
    candidates.into_iter().find(|path| path.is_file())
}

fn resolve_slopos_program(program: &str) -> Option<PathBuf> {
    if let Ok(executable) = env::current_exe() {
        if let Some(parent) = executable.parent() {
            let sibling = parent.join(program);
            if sibling.is_file() {
                return Some(sibling);
            }
        }
    }
    let local = PathBuf::from("scripts").join(program);
    if local.is_file() {
        return Some(local);
    }
    resolve_program_path(program)
}

fn command_exists(program: &str) -> bool {
    resolve_program_path(program).is_some()
}

fn resolve_program_path(program: &str) -> Option<PathBuf> {
    let path = Path::new(program);
    if path.components().count() > 1 {
        return path.is_file().then(|| path.to_path_buf());
    }
    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths)
            .map(|directory| directory.join(program))
            .find(|candidate| candidate.is_file())
    })
}

fn screen_geometry() -> (i32, i32) {
    gdk::Screen::default()
        .map(|screen| {
            let scale = env::var("GDK_SCALE")
                .ok()
                .and_then(|value| value.parse::<i32>().ok())
                .filter(|value| *value > 0)
                .unwrap_or(1);
            ((screen.width() / scale).max(1), (screen.height() / scale).max(1))
        })
        .unwrap_or((1280, 800))
}

fn adaptive_window_size(screen_width: i32, screen_height: i32) -> (i32, i32) {
    let width = if screen_width <= 1600 {
        680
    } else {
        (screen_width * 2 / 5).clamp(720, 980)
    };
    let height = if screen_height <= 1000 {
        520
    } else {
        (screen_height / 2).clamp(560, 700)
    };
    (width, height)
}

fn load_control_icon(file_name: &str, fallback: &str) -> Image {
    let mut candidates = Vec::new();
    if let Ok(share) = env::var("SLOPOS_SHARE_DIR") {
        candidates.push(
            PathBuf::from(share)
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
            if let Ok(pixbuf) = Pixbuf::from_file_at_scale(path, 32, 32, true) {
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
    let appearance = current_appearance();
    let (installed_theme, source_css) = match appearance.as_str() {
        "oled" => ("slopos-gtk-oled", "assets/config/gtk-3.0/gtk-oled.css"),
        "graphite" => (
            "slopos-gtk-graphite",
            "assets/config/gtk-3.0/gtk-graphite.css",
        ),
        "classic" => (
            "slopos-gtk-classic",
            "assets/config/gtk-3.0/gtk-classic.css",
        ),
        _ => ("slopos-gtk", "assets/config/gtk-3.0/gtk.css"),
    };

    let mut candidates = Vec::new();
    if let Some(config) = config_home() {
        candidates.push(config.join("gtk-3.0/gtk.css"));
    }
    if let Ok(share) = env::var("SLOPOS_SHARE_DIR") {
        candidates.push(
            PathBuf::from(share)
                .join("themes")
                .join(installed_theme)
                .join("gtk-3.0/gtk.css"),
        );
    }
    candidates.extend([
        PathBuf::from(source_css),
        PathBuf::from(format!(
            "/usr/local/share/themes/{installed_theme}/gtk-3.0/gtk.css"
        )),
        PathBuf::from(format!(
            "/usr/share/themes/{installed_theme}/gtk-3.0/gtk.css"
        )),
    ]);

    for path in candidates {
        if !path.is_file() {
            continue;
        }
        let Some(path_text) = path.to_str() else {
            continue;
        };
        let provider = gtk::CssProvider::new();
        if let Err(error) = provider.load_from_path(path_text) {
            log::warn!("Could not parse SLOPOS theme {}: {error}", path.display());
            continue;
        }
        if let Some(screen) = gdk::Screen::default() {
            gtk::StyleContext::add_provider_for_screen(
                &screen,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_USER,
            );
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::adaptive_window_size;

    #[test]
    fn settings_window_is_compact_but_scales_for_large_displays() {
        assert_eq!(adaptive_window_size(1366, 768), (680, 520));
        assert_eq!(adaptive_window_size(1280, 800), (680, 520));
        assert_eq!(adaptive_window_size(3440, 1440), (980, 700));
        assert_eq!(adaptive_window_size(7680, 4320), (980, 700));
    }
}
