//! SLOPOS-I Settings hub.
//!
//! SLOPOS owns the coherent control-panel entry point while mature upstream
//! X11/Linux utilities perform hardware and service mutation.

use gdk::RGBA;
use gdk_pixbuf::Pixbuf;
use gtk::atk::prelude::AtkObjectExt;
use gtk::prelude::*;
use gtk::{
    Align, Box as GtkBox, Button, CheckButton, ColorButton, ComboBoxText, Dialog, DialogFlags,
    FileChooserAction, FileChooserDialog, FileFilter, FontButton, Grid, IconSize, Image, Label,
    Orientation, RadioButton, ResponseType, ScrolledWindow, Separator, Window, WindowPosition,
    WindowType,
};
use std::cell::RefCell;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;

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

fn get_preset_default_colors(
    preset: &str,
) -> (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
) {
    match preset {
        "classic" => ("#000000", "#FFFFFF", "#FFFFFF", "#000000", "#555555"),
        "graphite" => ("#2B5B84", "#FFFFFF", "#2B2D30", "#F0F0F0", "#202226"),
        "oled" => ("#000000", "#FFFFFF", "#000000", "#FFFFFF", "#000000"),
        _ => ("#000080", "#FFFFFF", "#D9D9D9", "#000000", "#758090"),
    }
}

fn load_theme_preview_image(name: &str) -> Image {
    let filename = format!("preview-{name}.png");
    let mut candidates = Vec::new();
    if let Ok(share_dir) = env::var("SLOPOS_SHARE_DIR") {
        candidates.push(
            PathBuf::from(share_dir.clone())
                .join("themes")
                .join(&filename),
        );
        candidates.push(
            PathBuf::from(share_dir)
                .join("slopos-i/themes")
                .join(&filename),
        );
    }
    if let Ok(executable) = env::current_exe() {
        if let Some(prefix) = executable.parent().and_then(std::path::Path::parent) {
            candidates.push(prefix.join("share/slopos-i/themes").join(&filename));
            candidates.push(prefix.join("assets/themes").join(&filename));
        }
    }
    candidates.extend([
        PathBuf::from(format!("assets/themes/{filename}")),
        PathBuf::from(format!("/usr/local/share/slopos-i/themes/{filename}")),
        PathBuf::from(format!("/usr/share/slopos-i/themes/{filename}")),
    ]);

    for path in candidates {
        if path.is_file() {
            if let Ok(pixbuf) = Pixbuf::from_file_at_scale(&path, 92, 58, true) {
                return Image::from_pixbuf(Some(&pixbuf));
            }
        }
    }
    Image::from_icon_name(Some("preferences-desktop-theme"), IconSize::Dialog)
}

fn show_appearance_dialog(parent: &Window) {
    let dialog = Dialog::with_buttons(
        Some("Appearance, Colors & Fonts"),
        Some(parent),
        DialogFlags::MODAL | DialogFlags::DESTROY_WITH_PARENT,
        &[
            ("Cancel", ResponseType::Cancel),
            ("Apply", ResponseType::Accept),
        ],
    );
    dialog.set_default_response(ResponseType::Accept);
    dialog.set_default_size(620, 680);
    set_accessible_name(&dialog, "SLOPOS appearance chooser");

    let content = dialog.content_area();
    content.set_spacing(4);
    content.set_margin_start(10);
    content.set_margin_end(10);
    content.set_margin_top(6);
    content.set_margin_bottom(6);

    let heading = Label::new(Some("Desktop Appearance & Personalization Studio"));
    heading.set_xalign(0.0);
    heading.style_context().add_class("slopos-control-title");
    content.pack_start(&heading, false, false, 0);

    let explanation = Label::new(Some(
        "Select an authentic SLOPOS theme preset below with live visual preview, or customize individual RGB colors and typography (Windows XP style).",
    ));
    explanation.set_xalign(0.0);
    explanation.set_line_wrap(true);
    explanation
        .style_context()
        .add_class("slopos-secondary-text");
    content.pack_start(&explanation, false, false, 0);

    let scrolled = ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
    scrolled.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scrolled.set_min_content_height(530);

    let inner_box = GtkBox::new(Orientation::Vertical, 4);

    // Section 1: Themes with Visual Pictures
    let theme_header_box = GtkBox::new(Orientation::Horizontal, 8);
    let sec1_label = Label::new(Some("Theme Presets:"));
    sec1_label.set_xalign(0.0);
    sec1_label.style_context().add_class("slopos-result-title");
    theme_header_box.pack_start(&sec1_label, true, true, 0);

    let reset_defaults_btn = Button::with_label("Reset Theme Defaults");
    reset_defaults_btn.set_tooltip_text(Some(
        "Reset color palette to selected theme preset defaults",
    ));
    theme_header_box.pack_end(&reset_defaults_btn, false, false, 0);
    inner_box.pack_start(&theme_header_box, false, false, 0);

    let platinum_radio = RadioButton::with_label("Platinum");
    let classic_radio = RadioButton::with_label_from_widget(&platinum_radio, "Classic Macintosh");
    let graphite_radio = RadioButton::with_label_from_widget(&platinum_radio, "Graphite");
    let oled_radio = RadioButton::with_label_from_widget(&platinum_radio, "OLED Dark");

    let preset_cards = [
        (
            "platinum",
            "Platinum — classic light",
            "Classic Light (System 7)",
            platinum_radio.clone(),
        ),
        (
            "classic",
            "Classic Macintosh — System 6/7 monochrome",
            "System 6/7 Monochrome",
            classic_radio.clone(),
        ),
        (
            "graphite",
            "Graphite — dark",
            "Graphite Dark",
            graphite_radio.clone(),
        ),
        (
            "oled",
            "OLED Dark — pure black contrast",
            "OLED Pure Black",
            oled_radio.clone(),
        ),
    ];

    let presets_grid = Grid::new();
    presets_grid.set_row_spacing(4);
    presets_grid.set_column_spacing(6);
    presets_grid.set_hexpand(true);

    for (idx, (id, title, desc, radio)) in preset_cards.iter().enumerate() {
        let card = GtkBox::new(Orientation::Horizontal, 6);
        card.style_context().add_class("slopos-control-panel");
        card.set_margin_start(1);
        card.set_margin_end(1);
        card.set_margin_top(1);
        card.set_margin_bottom(1);

        let img = load_theme_preview_image(id);
        card.pack_start(&img, false, false, 0);

        let info_box = GtkBox::new(Orientation::Vertical, 1);
        info_box.set_valign(gtk::Align::Center);
        radio.set_label(title);
        info_box.pack_start(radio, false, false, 0);

        let desc_label = Label::new(Some(*desc));
        desc_label.set_xalign(0.0);
        desc_label
            .style_context()
            .add_class("slopos-secondary-text");
        info_box.pack_start(&desc_label, false, false, 0);

        card.pack_start(&info_box, true, true, 0);

        let col = (idx % 2) as i32;
        let row = (idx / 2) as i32;
        presets_grid.attach(&card, col, row, 1, 1);
    }
    inner_box.pack_start(&presets_grid, false, false, 0);

    inner_box.pack_start(&Separator::new(Orientation::Horizontal), false, false, 2);

    // Section 2: Color Studio
    let sec2_label = Label::new(Some("Global RGB Color Palette (Windows XP Style):"));
    sec2_label.set_xalign(0.0);
    sec2_label.style_context().add_class("slopos-result-title");
    inner_box.pack_start(&sec2_label, false, false, 0);

    let color_grid = Grid::new();
    color_grid.set_row_spacing(6);
    color_grid.set_column_spacing(12);

    let accent_btn = ColorButton::new();
    accent_btn.set_rgba(&hex_to_rgba("#000080"));
    let accent_label = Label::new(Some("Accent / Selection Color:"));
    accent_label.set_xalign(0.0);
    color_grid.attach(&accent_label, 0, 0, 1, 1);
    color_grid.attach(&accent_btn, 1, 0, 1, 1);

    let sel_text_btn = ColorButton::new();
    sel_text_btn.set_rgba(&hex_to_rgba("#FFFFFF"));
    let sel_text_label = Label::new(Some("Selection Text Color:"));
    sel_text_label.set_xalign(0.0);
    color_grid.attach(&sel_text_label, 0, 1, 1, 1);
    color_grid.attach(&sel_text_btn, 1, 1, 1, 1);

    let face_btn = ColorButton::new();
    face_btn.set_rgba(&hex_to_rgba("#D9D9D9"));
    let face_label = Label::new(Some("Window & Panel Surface:"));
    face_label.set_xalign(0.0);
    color_grid.attach(&face_label, 0, 2, 1, 1);
    color_grid.attach(&face_btn, 1, 2, 1, 1);

    let text_btn = ColorButton::new();
    text_btn.set_rgba(&hex_to_rgba("#000000"));
    let text_label = Label::new(Some("Main Text Color:"));
    text_label.set_xalign(0.0);
    color_grid.attach(&text_label, 0, 3, 1, 1);
    color_grid.attach(&text_btn, 1, 3, 1, 1);

    let root_btn = ColorButton::new();
    root_btn.set_rgba(&hex_to_rgba("#758090"));
    let root_label = Label::new(Some("Desktop Background:"));
    root_label.set_xalign(0.0);
    color_grid.attach(&root_label, 0, 4, 1, 1);
    color_grid.attach(&root_btn, 1, 4, 1, 1);

    inner_box.pack_start(&color_grid, false, false, 0);

    // Wire radio buttons to preset defaults
    let wire_preset = |radio: &RadioButton, preset_id: &'static str| {
        let accent_c = accent_btn.clone();
        let sel_text_c = sel_text_btn.clone();
        let face_c = face_btn.clone();
        let text_c = text_btn.clone();
        let root_c = root_btn.clone();
        radio.connect_toggled(move |btn| {
            if btn.is_active() {
                let colors = get_preset_default_colors(preset_id);
                accent_c.set_rgba(&hex_to_rgba(colors.0));
                sel_text_c.set_rgba(&hex_to_rgba(colors.1));
                face_c.set_rgba(&hex_to_rgba(colors.2));
                text_c.set_rgba(&hex_to_rgba(colors.3));
                root_c.set_rgba(&hex_to_rgba(colors.4));
            }
        });
    };

    wire_preset(&platinum_radio, "platinum");
    wire_preset(&classic_radio, "classic");
    wire_preset(&graphite_radio, "graphite");
    wire_preset(&oled_radio, "oled");

    // Initial state based on current appearance
    let cur_app = current_appearance();
    match cur_app {
        "oled" => oled_radio.set_active(true),
        "graphite" => graphite_radio.set_active(true),
        "classic" => classic_radio.set_active(true),
        _ => platinum_radio.set_active(true),
    }
    let init_colors = get_preset_default_colors(cur_app);
    accent_btn.set_rgba(&hex_to_rgba(init_colors.0));
    sel_text_btn.set_rgba(&hex_to_rgba(init_colors.1));
    face_btn.set_rgba(&hex_to_rgba(init_colors.2));
    text_btn.set_rgba(&hex_to_rgba(init_colors.3));
    root_btn.set_rgba(&hex_to_rgba(init_colors.4));

    // Reset Theme Defaults button handler
    {
        let clas_c = classic_radio.clone();
        let grap_c = graphite_radio.clone();
        let oled_c = oled_radio.clone();
        let accent_c = accent_btn.clone();
        let sel_text_c = sel_text_btn.clone();
        let face_c = face_btn.clone();
        let text_c = text_btn.clone();
        let root_c = root_btn.clone();
        reset_defaults_btn.connect_clicked(move |_| {
            let colors = if oled_c.is_active() {
                get_preset_default_colors("oled")
            } else if grap_c.is_active() {
                get_preset_default_colors("graphite")
            } else if clas_c.is_active() {
                get_preset_default_colors("classic")
            } else {
                get_preset_default_colors("platinum")
            };
            accent_c.set_rgba(&hex_to_rgba(colors.0));
            sel_text_c.set_rgba(&hex_to_rgba(colors.1));
            face_c.set_rgba(&hex_to_rgba(colors.2));
            text_c.set_rgba(&hex_to_rgba(colors.3));
            root_c.set_rgba(&hex_to_rgba(colors.4));
        });
    }

    // Quick Accent Swatches
    let swatch_box = GtkBox::new(Orientation::Horizontal, 4);
    let swatch_label = Label::new(Some("Quick Accents:"));
    swatch_label
        .style_context()
        .add_class("slopos-secondary-text");
    swatch_box.pack_start(&swatch_label, false, false, 0);

    let swatches = [
        ("#000080", "Navy"),
        ("#2563EB", "Azure"),
        ("#008080", "Teal"),
        ("#5C616C", "Slate"),
        ("#7B1FA2", "Purple"),
        ("#B71C1C", "Crimson"),
        ("#1B5E20", "Forest"),
        ("#F57F17", "Amber"),
    ];
    for (hex, name) in swatches {
        let btn = Button::with_label(name);
        let hex_s = hex.to_string();
        let accent_c = accent_btn.clone();
        btn.connect_clicked(move |_| {
            accent_c.set_rgba(&hex_to_rgba(&hex_s));
        });
        swatch_box.pack_start(&btn, false, false, 0);
    }
    inner_box.pack_start(&swatch_box, false, false, 0);

    inner_box.pack_start(&Separator::new(Orientation::Horizontal), false, false, 2);

    // Section 3: Typography / Fonts
    let sec3_label = Label::new(Some("User Interface Typography:"));
    sec3_label.set_xalign(0.0);
    sec3_label.style_context().add_class("slopos-result-title");
    inner_box.pack_start(&sec3_label, false, false, 0);

    let font_box = GtkBox::new(Orientation::Horizontal, 8);
    let font_label = Label::new(Some("Interface Font & Size:"));
    let font_btn = FontButton::new();
    font_btn.set_font(&get_current_font());
    font_box.pack_start(&font_label, false, false, 0);
    font_box.pack_start(&font_btn, true, true, 0);
    inner_box.pack_start(&font_box, false, false, 0);

    inner_box.pack_start(&Separator::new(Orientation::Horizontal), false, false, 2);

    // Section 4: Dock & Desktop Behaviors
    let sec4_label = Label::new(Some("Dock & Window Management:"));
    sec4_label.set_xalign(0.0);
    sec4_label.style_context().add_class("slopos-result-title");
    inner_box.pack_start(&sec4_label, false, false, 0);

    let dodge_check = CheckButton::with_label(
        "Dodge maximized windows (auto-hide dock when active window is maximized)",
    );
    dodge_check.set_active(is_dock_dodge_enabled());
    inner_box.pack_start(&dodge_check, false, false, 0);

    scrolled.add(&inner_box);
    content.pack_start(&scrolled, true, true, 0);

    dialog.show_all();
    let response = dialog.run();
    if response == ResponseType::Accept {
        set_dock_dodge_enabled(dodge_check.is_active());
        let font_chosen = font_btn
            .font()
            .map(|f| f.to_string())
            .unwrap_or_else(|| "Liberation Sans 9".into());

        // Save chosen font
        let config_home = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
        if let Some(ref config_home) = config_home {
            let dir = config_home.join("slopos-i");
            let _ = fs::create_dir_all(&dir);
            let _ = fs::write(dir.join("font"), format!("{}\n", font_chosen.as_str()));
        }

        let mode = if oled_radio.is_active() {
            "oled"
        } else if graphite_radio.is_active() {
            "graphite"
        } else if classic_radio.is_active() {
            "classic"
        } else {
            "platinum"
        };
        let def_colors = get_preset_default_colors(mode);

        let accent_hex = rgba_to_hex(&accent_btn.rgba());
        let sel_text_hex = rgba_to_hex(&sel_text_btn.rgba());
        let face_hex = rgba_to_hex(&face_btn.rgba());
        let text_hex = rgba_to_hex(&text_btn.rgba());
        let root_hex = rgba_to_hex(&root_btn.rgba());

        let colors_match_preset = accent_hex.eq_ignore_ascii_case(def_colors.0)
            && sel_text_hex.eq_ignore_ascii_case(def_colors.1)
            && face_hex.eq_ignore_ascii_case(def_colors.2)
            && text_hex.eq_ignore_ascii_case(def_colors.3)
            && root_hex.eq_ignore_ascii_case(def_colors.4);

        if colors_match_preset {
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
        } else {
            if let Some(ref config_home) = config_home {
                let gtk_dir = config_home.join("gtk-3.0");
                let _ = fs::create_dir_all(&gtk_dir);
                let custom_css = generate_custom_theme_css(
                    &accent_hex,
                    &sel_text_hex,
                    &face_hex,
                    &text_hex,
                    font_chosen.as_str(),
                );
                let _ = fs::write(gtk_dir.join("gtk.css"), &custom_css);
                let _ = fs::write(config_home.join("slopos-i/custom-theme.css"), &custom_css);
                let _ = fs::write(
                    config_home.join("slopos-i/root_color"),
                    format!("{root_hex}\n"),
                );
            }

            if let Some(helper) = appearance_helper() {
                let _ = Command::new(helper).arg("custom").spawn();
            }
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
    dialog.set_default_size(540, 520);
    set_accessible_name(&dialog, "SLOPOS wallpaper chooser");

    let content = dialog.content_area();
    content.set_spacing(8);
    content.set_margin_start(14);
    content.set_margin_end(14);
    content.set_margin_top(10);
    content.set_margin_bottom(10);

    let heading = Label::new(Some("Desktop Wallpaper & Background Patterns"));
    heading.set_xalign(0.0);
    heading.style_context().add_class("slopos-control-title");
    content.pack_start(&heading, false, false, 0);

    let explanation = Label::new(Some(
        "Choose an authentic retro background pattern or select your own custom image from disk:",
    ));
    explanation.set_xalign(0.0);
    explanation
        .style_context()
        .add_class("slopos-secondary-text");
    content.pack_start(&explanation, false, false, 0);

    let scrolled = ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
    scrolled.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scrolled.set_min_content_height(280);

    let wp_grid = GtkBox::new(Orientation::Vertical, 6);

    let wp_items = [
        (
            "01_classic_system_gray.png",
            "01 Classic System Gray",
            "50% 1-Bit Monochrome Dither Pattern",
        ),
        (
            "02_platinum_cool_slate.png",
            "02 Platinum Cool Slate",
            "Fine Matrix Slate Grid (#758090)",
        ),
        (
            "03_vintage_mac_blue.png",
            "03 Vintage Mac Blue",
            "Classic System 8/9 Blue Tweed Pattern (#3A5F8B)",
        ),
        (
            "04_retro_teal_grid.png",
            "04 Retro Teal Grid",
            "1990s Desktop Teal Geometric Matrix (#008080)",
        ),
        (
            "05_oled_pure_dark.png",
            "05 OLED Pure Dark",
            "Pure Black Obsidian Constellation (#000000)",
        ),
    ];

    let wp_radios: Rc<RefCell<Vec<(RadioButton, String)>>> = Rc::new(RefCell::new(Vec::new()));
    let custom_path: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

    let mut first_radio: Option<RadioButton> = None;

    for (idx, (file, title, desc)) in wp_items.iter().enumerate() {
        let row_box = GtkBox::new(Orientation::Horizontal, 10);
        row_box.style_context().add_class("slopos-catalogue-row");
        row_box.set_margin_top(2);
        row_box.set_margin_bottom(2);

        if let Some(path) = find_wallpaper_path(file) {
            if let Ok(pixbuf) = Pixbuf::from_file_at_scale(&path, 72, 45, false) {
                let img = Image::from_pixbuf(Some(&pixbuf));
                row_box.pack_start(&img, false, false, 2);
            }
        }

        let label_box = GtkBox::new(Orientation::Vertical, 2);
        let title_lbl = Label::new(Some(*title));
        title_lbl.set_xalign(0.0);
        title_lbl.style_context().add_class("slopos-result-title");
        let desc_lbl = Label::new(Some(*desc));
        desc_lbl.set_xalign(0.0);
        desc_lbl.style_context().add_class("slopos-secondary-text");
        label_box.pack_start(&title_lbl, false, false, 0);
        label_box.pack_start(&desc_lbl, false, false, 0);

        let radio = if let Some(ref first) = first_radio {
            RadioButton::with_label_from_widget(first, "")
        } else {
            let r = RadioButton::with_label("");
            first_radio = Some(r.clone());
            r
        };

        if idx == 1 {
            radio.set_active(true);
        }

        row_box.pack_start(&radio, false, false, 0);
        row_box.pack_start(&label_box, true, true, 0);

        wp_radios.borrow_mut().push((radio, (*file).to_string()));
        wp_grid.pack_start(&row_box, false, false, 0);
    }

    // Custom File Row
    let custom_row = GtkBox::new(Orientation::Horizontal, 10);
    custom_row.style_context().add_class("slopos-catalogue-row");
    custom_row.set_margin_top(2);
    custom_row.set_margin_bottom(2);

    let custom_radio = RadioButton::with_label_from_widget(first_radio.as_ref().unwrap(), "");
    let custom_label = Label::new(Some("Custom Image File…"));
    custom_label.set_xalign(0.0);
    custom_label
        .style_context()
        .add_class("slopos-result-title");

    let browse_btn = Button::with_label("Browse Image…");
    let custom_label_c = custom_label.clone();
    let custom_radio_c = custom_radio.clone();
    let custom_path_c = custom_path.clone();
    let dialog_parent = dialog.clone();

    browse_btn.connect_clicked(move |_| {
        let file_chooser = FileChooserDialog::with_buttons(
            Some("Choose Wallpaper Image"),
            Some(&dialog_parent),
            FileChooserAction::Open,
            &[
                ("Cancel", ResponseType::Cancel),
                ("Open", ResponseType::Accept),
            ],
        );
        let filter = FileFilter::new();
        filter.set_name(Some("Image files (PNG, JPG, SVG, BMP, WEBP)"));
        filter.add_mime_type("image/png");
        filter.add_mime_type("image/jpeg");
        filter.add_mime_type("image/jpg");
        filter.add_mime_type("image/bmp");
        filter.add_mime_type("image/svg+xml");
        filter.add_mime_type("image/webp");
        filter.add_pattern("*.png");
        filter.add_pattern("*.jpg");
        filter.add_pattern("*.jpeg");
        filter.add_pattern("*.bmp");
        filter.add_pattern("*.svg");
        filter.add_pattern("*.webp");
        file_chooser.add_filter(filter);

        if file_chooser.run() == ResponseType::Accept {
            if let Some(file_path) = file_chooser.filename() {
                let path_str = file_path.to_string_lossy().to_string();
                custom_label_c.set_text(&format!(
                    "Custom: {}",
                    file_path.file_name().unwrap_or_default().to_string_lossy()
                ));
                *custom_path_c.borrow_mut() = Some(path_str);
                custom_radio_c.set_active(true);
            }
        }
        file_chooser.close();
    });

    custom_row.pack_start(&custom_radio, false, false, 0);
    custom_row.pack_start(&custom_label, true, true, 0);
    custom_row.pack_start(&browse_btn, false, false, 0);

    wp_radios
        .borrow_mut()
        .push((custom_radio, "custom".to_string()));
    wp_grid.pack_start(&custom_row, false, false, 0);

    scrolled.add(&wp_grid);
    content.pack_start(&scrolled, true, true, 0);

    let mode_box = GtkBox::new(Orientation::Horizontal, 8);
    let mode_label = Label::new(Some("Display Mode:"));
    let mode_combo = ComboBoxText::new();
    mode_combo.append(Some("fill"), "Fill / Stretch (Default)");
    mode_combo.append(Some("tile"), "Tile Pattern");
    mode_combo.append(Some("center"), "Center");
    mode_combo.append(Some("max"), "Max / Fit to Screen");
    mode_combo.set_active(Some(0));
    mode_box.pack_start(&mode_label, false, false, 0);
    mode_box.pack_start(&mode_combo, true, true, 0);
    content.pack_start(&mode_box, false, false, 0);

    let dodge_check = CheckButton::with_label(
        "Dodge maximized windows (auto-hide dock when active window is maximized)",
    );
    dodge_check.set_active(is_dock_dodge_enabled());
    content.pack_start(&dodge_check, false, false, 0);

    dialog.show_all();
    let response = dialog.run();
    if response == ResponseType::Accept {
        set_dock_dodge_enabled(dodge_check.is_active());

        let mut chosen_file = "02_platinum_cool_slate.png".to_string();
        for (radio, file_name) in wp_radios.borrow().iter() {
            if radio.is_active() {
                if file_name == "custom" {
                    if let Some(ref custom_img) = *custom_path.borrow() {
                        chosen_file = custom_img.clone();
                    }
                } else {
                    chosen_file = file_name.clone();
                }
                break;
            }
        }

        let mode = mode_combo.active_id().unwrap_or_else(|| "fill".into());
        let _ = Command::new("scripts/slopos-wallpaper")
            .args(["set", &chosen_file, "--mode", mode.as_str()])
            .spawn()
            .or_else(|_| {
                Command::new("slopos-wallpaper")
                    .args(["set", &chosen_file, "--mode", mode.as_str()])
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

fn rgba_to_hex(rgba: &RGBA) -> String {
    format!(
        "#{:02x}{:02x}{:02x}",
        (rgba.red() * 255.0).round() as u8,
        (rgba.green() * 255.0).round() as u8,
        (rgba.blue() * 255.0).round() as u8
    )
}

fn hex_to_rgba(hex: &str) -> RGBA {
    let hex = hex.trim().trim_start_matches('#');
    if hex.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&hex[0..2], 16),
            u8::from_str_radix(&hex[2..4], 16),
            u8::from_str_radix(&hex[4..6], 16),
        ) {
            return RGBA::new(r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0, 1.0);
        }
    }
    RGBA::new(0.0, 0.0, 0.5, 1.0)
}

fn is_dock_dodge_enabled() -> bool {
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    if let Some(config_home) = config_home {
        let flag_file = config_home.join("slopos-i/dock_dodge");
        if let Ok(content) = fs::read_to_string(flag_file) {
            let t = content.trim();
            return t == "1" || t.eq_ignore_ascii_case("true") || t.eq_ignore_ascii_case("yes");
        }
    }
    false
}

fn set_dock_dodge_enabled(enabled: bool) {
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    if let Some(config_home) = config_home {
        let dir = config_home.join("slopos-i");
        let _ = fs::create_dir_all(&dir);
        let flag_file = dir.join("dock_dodge");
        let _ = fs::write(flag_file, if enabled { "1\n" } else { "0\n" });

        let ob_rc = config_home.join("openbox/rc.xml");
        if let Ok(content) = fs::read_to_string(&ob_rc) {
            let new_content = if enabled {
                content.replace("<bottom>60</bottom>", "<bottom>0</bottom>")
            } else {
                content.replace("<bottom>0</bottom>", "<bottom>60</bottom>")
            };
            let _ = fs::write(&ob_rc, new_content);
            let _ = std::process::Command::new("openbox")
                .arg("--reconfigure")
                .spawn();
        }
    }
}

fn get_current_font() -> String {
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    if let Some(ref config_home) = config_home {
        if let Ok(content) = fs::read_to_string(config_home.join("slopos-i/font")) {
            let t = content.trim().to_string();
            if !t.is_empty() {
                return t;
            }
        }
        if let Ok(content) = fs::read_to_string(config_home.join("gtk-3.0/settings.ini")) {
            for line in content.lines() {
                if let Some(rest) = line.trim().strip_prefix("gtk-font-name") {
                    if let Some(val) = rest.strip_prefix('=').or_else(|| rest.strip_prefix(" =")) {
                        let f = val.trim().to_string();
                        if !f.is_empty() {
                            return f;
                        }
                    }
                }
            }
        }
    }
    "Liberation Sans 9".to_string()
}

fn find_wallpaper_path(filename: &str) -> Option<PathBuf> {
    let candidates = [
        PathBuf::from(format!("assets/wallpapers/{filename}")),
        PathBuf::from(format!("/usr/local/share/slopos-i/wallpapers/{filename}")),
        PathBuf::from(format!("/usr/share/slopos-i/wallpapers/{filename}")),
    ];
    candidates.into_iter().find(|cand| cand.is_file())
}

fn is_dark_color(hex: &str) -> bool {
    let hex = hex.trim().trim_start_matches('#');
    if hex.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&hex[0..2], 16),
            u8::from_str_radix(&hex[2..4], 16),
            u8::from_str_radix(&hex[4..6], 16),
        ) {
            let luma = 0.299 * (r as f64) + 0.587 * (g as f64) + 0.114 * (b as f64);
            return luma < 128.0;
        }
    }
    false
}

fn generate_custom_theme_css(
    accent_hex: &str,
    accent_text_hex: &str,
    face_hex: &str,
    text_hex: &str,
    font_name: &str,
) -> String {
    let is_dark = is_dark_color(face_hex);
    let border_light = if is_dark { "#3a3c45" } else { "#ffffff" };
    let border_mid = if is_dark { "#2a2c33" } else { "#b0b0b0" };
    let border_dark = if is_dark { "#000000" } else { "#606060" };
    let secondary_text = if is_dark { "#9ca3af" } else { "#707070" };
    let entry_bg = if is_dark { "#0a0a0c" } else { "#ffffff" };
    let entry_fg = if is_dark { "#ffffff" } else { "#000000" };
    let list_bg = if is_dark { "#050505" } else { "#ffffff" };
    let list_fg = if is_dark { "#ffffff" } else { "#000000" };

    format!(
        r#"/* SLOPOS-I Custom User Theme — Personal Color & Typography Studio */
@define-color slopos_face {face_hex};
@define-color slopos_face_dark {face_hex};
@define-color slopos_light {border_light};
@define-color slopos_highlight {accent_hex};
@define-color slopos_shadow {border_dark};
@define-color slopos_mid {border_mid};
@define-color slopos_text {text_hex};
@define-color slopos_disabled #888888;
@define-color slopos_warning #f59e0b;

* {{ font: {font_name}; color: @slopos_text; border-radius: 0; }}
window, .background, dialog {{ background-color: @slopos_face; color: @slopos_text; }}
headerbar, .titlebar {{ min-height: 24px; padding: 2px 4px; background-color: @slopos_face; border-bottom: 1px solid @slopos_mid; }}
headerbar .title, .titlebar .title {{ font-weight: bold; color: @slopos_text; }}
window.csd, window.solid-csd, window.csd decoration {{ border-radius: 0; box-shadow: none; }}

.slopos-topbar {{ min-height: 26px; background-color: @slopos_face; border-bottom: 1px solid @slopos_mid; }}
.slopos-topbar label, .slopos-active-app {{ color: @slopos_text; }}
.slopos-active-app {{ font-weight: bold; padding: 0 4px; color: @slopos_text; }}
.slopos-topbar button, .slopos-menubar-control, .slopos-logo-btn {{ min-height: 20px; padding: 1px 5px; background: transparent; border: 0; }}
.slopos-topbar button:hover, .slopos-topbar button:checked, .slopos-menu-bar menuitem:hover {{ background-color: @slopos_highlight; }}
.slopos-topbar button:hover label, .slopos-topbar button:checked label, .slopos-menu-bar menuitem:hover label {{ color: {accent_text_hex}; }}
.slopos-menu-bar {{ background: transparent; border: 0; }}
.slopos-menu-bar menuitem {{ min-height: 18px; padding: 3px 7px; }}
menubar {{ min-height: 22px; padding: 0 2px; background-color: @slopos_face; border-bottom: 1px solid @slopos_mid; }}
menubar > menuitem:hover, menubar > menuitem:focus, menubar > menuitem:selected {{ background-color: @slopos_highlight; color: {accent_text_hex}; }}
menubar > menuitem:hover label, menubar > menuitem:focus label, menubar > menuitem:selected label {{ color: {accent_text_hex}; }}

menu, popover.background {{ background-color: @slopos_face; border: 2px solid @slopos_shadow; box-shadow: 3px 3px rgba(0,0,0,.35); padding: 2px; }}
menu menuitem {{ min-height: 18px; padding: 3px 18px 3px 8px; }}
menu menuitem:hover, menu menuitem:focus, menu menuitem:selected {{ background-color: @slopos_highlight; color: {accent_text_hex}; }}
menu menuitem:hover label, menu menuitem:focus label, menu menuitem:selected label {{ color: {accent_text_hex}; }}

button {{ min-height: 22px; padding: 3px 9px; background-color: @slopos_face; border-style: solid; border-width: 1px 2px 2px 1px; border-top-color: @slopos_light; border-left-color: @slopos_light; border-right-color: @slopos_shadow; border-bottom-color: @slopos_shadow; }}
button:hover {{ background-color: @slopos_highlight; color: {accent_text_hex}; }}
button:active, button:checked {{ border-width: 2px 1px 1px 2px; border-top-color: @slopos_shadow; border-left-color: @slopos_shadow; border-right-color: @slopos_light; border-bottom-color: @slopos_light; }}
button:focus {{ box-shadow: inset 0 0 0 1px @slopos_highlight; }}

entry {{ min-height: 24px; padding: 2px 5px; background-color: {entry_bg}; color: {entry_fg}; border: 1px solid @slopos_shadow; }}
entry:focus {{ border-color: @slopos_highlight; }}

list, listbox, treeview, textview {{ background-color: {list_bg}; color: {list_fg}; }}
row:selected {{ background-color: @slopos_highlight; color: {accent_text_hex}; }}
row:selected label {{ color: {accent_text_hex}; }}

.slopos-dock-container {{ padding: 4px 6px; background-color: @slopos_face; border-style: solid; border-width: 2px; border-top-color: @slopos_light; border-left-color: @slopos_light; border-right-color: @slopos_shadow; border-bottom-color: @slopos_shadow; box-shadow: 3px 3px rgba(0,0,0,.35); }}
.slopos-dock-label {{ min-width: 30px; padding: 1px 2px; color: @slopos_text; font-size: 9px; font-weight: bold; }}
.slopos-dock-btn {{ min-width: 42px; min-height: 40px; margin: 0 1px; padding: 2px; background-color: @slopos_face; border: 1px solid @slopos_mid; }}
.slopos-dock-btn:hover {{ background-color: @slopos_highlight; }}
.slopos-launcher {{ padding: 9px; background-color: @slopos_face; border: 2px solid @slopos_shadow; box-shadow: 4px 4px rgba(0,0,0,.45); }}
.slopos-notification, .slopos-alert-box {{ padding: 9px; background-color: @slopos_face; border: 2px solid @slopos_shadow; box-shadow: 4px 4px rgba(0,0,0,.45); }}
.slopos-window-body {{ padding: 10px; background-color: @slopos_face; }}
.slopos-panel-title {{ font-weight: bold; font-size: 15px; color: @slopos_text; }}
.slopos-panel-subtitle {{ color: {secondary_text}; font-size: 11px; }}
.slopos-control-panel {{ min-height: 64px; padding: 6px 8px; background-color: @slopos_face; border-top-color: @slopos_light; border-left-color: @slopos_light; border-right-color: @slopos_shadow; border-bottom-color: @slopos_shadow; }}
"#
    )
}

fn current_appearance() -> &'static str {
    if let Ok(env_appearance) = env::var("SLOPOS_APPEARANCE") {
        let v = env_appearance.trim();
        if v.eq_ignore_ascii_case("custom") {
            return "custom";
        }
        if v.eq_ignore_ascii_case("oled") {
            return "oled";
        }
        if v.eq_ignore_ascii_case("graphite") {
            return "graphite";
        }
        if v.eq_ignore_ascii_case("classic") {
            return "classic";
        }
        if v.eq_ignore_ascii_case("platinum") {
            return "platinum";
        }
    }
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    if let Some(config_home) = config_home {
        if let Ok(value) = std::fs::read_to_string(config_home.join("slopos-i/appearance")) {
            let v = value.trim();
            if v.eq_ignore_ascii_case("custom") {
                return "custom";
            }
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
