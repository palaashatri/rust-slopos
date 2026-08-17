//! SLOPOS-I classic Macintosh-style top menu/system bar.

use crate::gmenu::{self, GtkMenuExporter};
use crate::launcher::Launcher;
use gdk_pixbuf::{InterpType, Pixbuf};
use gtk::atk::prelude::AtkObjectExt;
use gtk::prelude::*;
use gtk::{
    Align, Box as GtkBox, Button, Dialog, DialogFlags, IconSize, Image, Label, Menu, MenuBar,
    MenuItem, Orientation, ResponseType, SeparatorMenuItem, Window, WindowPosition, WindowType,
};
use std::cell::{Cell, RefCell};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const LOCK_COMMANDS: &[(&str, &[&str])] = &[
    ("loginctl", &["lock-session"]),
    ("xflock4", &[]),
    ("light-locker-command", &["-l"]),
    ("dm-tool", &["lock"]),
    ("xdg-screensaver", &["lock"]),
    ("slock", &[]),
    ("i3lock", &["-c", "758090"]),
];

const SWITCH_USER_COMMANDS: &[(&str, &[&str])] = &[
    ("dm-tool", &["switch-to-greeter"]),
    ("gdmflexiserver", &[]),
    ("kdmctl", &["reserve"]),
    ("loginctl", &["activate-session"]),
];

const SUSPEND_COMMANDS: &[(&str, &[&str])] = &[
    ("systemctl", &["suspend"]),
    ("loginctl", &["suspend"]),
    ("pm-suspend", &[]),
];

const REBOOT_COMMANDS: &[(&str, &[&str])] = &[
    ("systemctl", &["reboot"]),
    ("loginctl", &["reboot"]),
    ("reboot", &[]),
];

const POWEROFF_COMMANDS: &[(&str, &[&str])] = &[
    ("systemctl", &["poweroff"]),
    ("loginctl", &["poweroff"]),
    ("poweroff", &[]),
];

static OPEN_SYSTEM_MENU: AtomicBool = AtomicBool::new(false);

pub struct TopBar {
    _window: Window,
    _active_title_label: Label,
    _clock_label: Label,
}

impl TopBar {
    pub fn new(launcher: Rc<Launcher>) -> Rc<Self> {
        let window = Window::new(WindowType::Toplevel);
        window.set_title("SLOPOS Top Bar");
        let (screen_width, _) = screen_geometry();
        window.set_default_size(screen_width, 26);
        window.set_position(WindowPosition::None);
        window.move_(0, 0);
        window.set_decorated(false);
        window.set_type_hint(gdk::WindowTypeHint::Dock);
        window.set_keep_above(true);
        window.set_accept_focus(false);
        window.set_skip_taskbar_hint(true);
        window.set_skip_pager_hint(true);
        set_accessible_name(&window, "SLOPOS top menu bar");
        window.style_context().add_class("slopos-topbar");

        let main_box = GtkBox::new(Orientation::Horizontal, 0);
        main_box.style_context().add_class("slopos-topbar");
        main_box.set_hexpand(true);
        main_box.set_vexpand(true);
        main_box.set_margin_start(5);
        main_box.set_margin_end(8);

        let system_button = Button::new();
        system_button.style_context().add_class("slopos-logo-btn");
        system_button.set_tooltip_text(Some("SLOPOS menu (Ctrl+F2)"));
        set_accessible_name(&system_button, "SLOPOS menu");
        if let Some(mark) = load_slopos_mark() {
            system_button.set_image(Some(&mark));
            system_button.set_always_show_image(true);
        } else {
            system_button.set_label("S");
        }
        let system_menu = build_system_menu();
        let menu_ref = system_menu.clone();
        system_button.connect_clicked(move |button| {
            menu_ref.popup_at_widget(
                button,
                gdk::Gravity::SouthWest,
                gdk::Gravity::NorthWest,
                None,
            );
        });
        install_system_menu_signal_bridge(&system_button, &system_menu);
        main_box.pack_start(&system_button, false, false, 0);

        let active_title_label = Label::new(Some("SLOPOS Desktop"));
        active_title_label
            .style_context()
            .add_class("slopos-active-app");
        active_title_label.set_halign(Align::Start);
        active_title_label.set_ellipsize(pango::EllipsizeMode::End);
        active_title_label.set_max_width_chars(24);
        set_accessible_name(&active_title_label, "Active application");
        main_box.pack_start(&active_title_label, false, false, 7);

        let global_menu_host = GtkBox::new(Orientation::Horizontal, 0);
        global_menu_host
            .style_context()
            .add_class("slopos-global-menu-host");
        set_accessible_name(&global_menu_host, "Focused application global menu");
        main_box.pack_start(&global_menu_host, false, false, 0);

        let status_box = GtkBox::new(Orientation::Horizontal, 5);
        status_box.style_context().add_class("slopos-status-area");

        let search_button = Button::new();
        search_button
            .style_context()
            .add_class("slopos-menubar-control");
        search_button.set_tooltip_text(Some("Search applications (Super+Space)"));
        set_accessible_name(&search_button, "Search applications (Super+Space)");
        let search_box = GtkBox::new(Orientation::Horizontal, 3);
        search_box.pack_start(
            &Image::from_icon_name(Some("edit-find"), IconSize::Menu),
            false,
            false,
            0,
        );
        search_box.pack_start(&Label::new(Some("Search")), false, false, 0);
        search_button.add(&search_box);
        let launcher_ref = launcher.clone();
        search_button.connect_clicked(move |_| launcher_ref.toggle());
        status_box.pack_start(&search_button, false, false, 0);

        let audio_button = Button::new();
        audio_button
            .style_context()
            .add_class("slopos-menubar-control");
        set_accessible_name(&audio_button, "Sound controls");
        let audio_box = GtkBox::new(Orientation::Horizontal, 3);
        audio_box.pack_start(
            &Image::from_icon_name(Some("audio-volume-high-symbolic"), IconSize::Menu),
            false,
            false,
            0,
        );
        let audio_label = Label::new(Some("—"));
        audio_box.pack_start(&audio_label, false, false, 0);
        audio_button.add(&audio_box);
        if resolve_program("pavucontrol").is_some() {
            audio_button.set_tooltip_text(Some("Open sound controls"));
            audio_button.connect_clicked(|_| spawn_resolved("pavucontrol", &[]));
        } else {
            audio_button.set_sensitive(false);
            audio_button.set_tooltip_text(Some("Sound controls are not installed"));
        }
        status_box.pack_start(&audio_button, false, false, 0);

        let network_button = Button::new();
        network_button
            .style_context()
            .add_class("slopos-menubar-control");
        set_accessible_name(&network_button, "Network connections");
        let network_box = GtkBox::new(Orientation::Horizontal, 3);
        network_box.pack_start(
            &Image::from_icon_name(Some("network-wireless-symbolic"), IconSize::Menu),
            false,
            false,
            0,
        );
        let network_label = Label::new(Some("—"));
        network_box.pack_start(&network_label, false, false, 0);
        network_button.add(&network_box);
        if resolve_program("nm-connection-editor").is_some() {
            network_button.set_tooltip_text(Some("Open network connections"));
            network_button.connect_clicked(|_| spawn_resolved("nm-connection-editor", &[]));
        } else {
            network_button.set_sensitive(false);
            network_button.set_tooltip_text(Some("Network controls are not installed"));
        }
        status_box.pack_start(&network_button, false, false, 0);

        let battery_box = GtkBox::new(Orientation::Horizontal, 3);
        set_accessible_name(&battery_box, "Battery status");
        battery_box.pack_start(
            &Image::from_icon_name(Some("battery-good-symbolic"), IconSize::Menu),
            false,
            false,
            0,
        );
        let battery_label = Label::new(None);
        battery_box.pack_start(&battery_label, false, false, 0);
        status_box.pack_start(&battery_box, false, false, 0);

        let initial_clock =
            command_output("date", &["+%H:%M"]).unwrap_or_else(|| "--:--".to_string());
        let clock_label = Label::new(Some(&initial_clock));
        clock_label.style_context().add_class("slopos-clock");
        clock_label.set_tooltip_text(Some("Local time"));
        set_accessible_name(&clock_label, "Local time");
        status_box.pack_start(&clock_label, false, false, 2);

        main_box.pack_end(&status_box, false, false, 0);
        window.add(&main_box);
        window.show_all();

        install_live_updates(
            &active_title_label,
            &global_menu_host,
            &clock_label,
            &audio_label,
            &network_label,
            &battery_box,
            &battery_label,
        );

        Rc::new(Self {
            _window: window,
            _active_title_label: active_title_label,
            _clock_label: clock_label,
        })
    }
}

fn install_system_menu_signal_bridge(button: &Button, menu: &Menu) {
    unsafe {
        libc::signal(
            libc::SIGUSR2,
            system_menu_signal_handler as *const () as usize,
        );
    }

    let keyboard_open_pending = Rc::new(Cell::new(false));
    let map_pending = keyboard_open_pending.clone();
    menu.connect_map(move |menu| {
        if !map_pending.replace(false) {
            return;
        }

        let menu = menu.clone();
        glib::idle_add_local_once(move || {
            menu.select_first(true);
            log::info!("SLOPOS_SYSTEM_MENU_KEYBOARD_READY");
        });
    });

    let button = button.clone();
    let menu = menu.clone();
    glib::timeout_add_local(Duration::from_millis(80), move || {
        if OPEN_SYSTEM_MENU.swap(false, Ordering::SeqCst) {
            keyboard_open_pending.set(true);
            menu.popup_at_widget(
                &button,
                gdk::Gravity::SouthWest,
                gdk::Gravity::NorthWest,
                None,
            );
            log::info!("SLOPOS_SYSTEM_MENU_KEYBOARD_OPENED");
        }
        glib::ControlFlow::Continue
    });
}

extern "C" fn system_menu_signal_handler(_sig: libc::c_int) {
    OPEN_SYSTEM_MENU.store(true, Ordering::SeqCst);
}

#[allow(clippy::too_many_arguments)]
fn install_live_updates(
    active_title: &Label,
    global_menu_host: &GtkBox,
    clock: &Label,
    audio: &Label,
    network: &Label,
    battery_box: &GtkBox,
    battery: &Label,
) {
    let active_title = active_title.clone();
    let global_menu_host = global_menu_host.clone();
    let active_menu_state: Rc<RefCell<Option<ActiveMenuState>>> = Rc::new(RefCell::new(None));
    glib::timeout_add_local(Duration::from_millis(300), move || {
        update_active_window(&active_title, &global_menu_host, &active_menu_state);
        glib::ControlFlow::Continue
    });

    let clock = clock.clone();
    glib::timeout_add_seconds_local(1, move || {
        if let Some(local_time) = command_output("date", &["+%H:%M"]) {
            clock.set_text(&local_time);
        }
        glib::ControlFlow::Continue
    });

    let audio = audio.clone();
    let network = network.clone();
    let battery_box = battery_box.clone();
    let battery = battery.clone();
    battery_box.set_visible(current_battery_state().is_some());
    glib::timeout_add_seconds_local(5, move || {
        audio.set_text(&current_volume().unwrap_or_else(|| "—".to_string()));
        network.set_text(&current_network_state());
        if let Some(value) = current_battery_state() {
            battery.set_text(&value);
            battery_box.set_visible(true);
        } else {
            battery.set_text("");
            battery_box.set_visible(false);
        }
        glib::ControlFlow::Continue
    });
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ActiveMenuState {
    Exporter(GtkMenuExporter),
    Tailored {
        window_id: Option<u64>,
        kind: AppKind,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppKind {
    Desktop,
    Terminal,
    FileManager,
    TextEditor,
    Browser,
    Calculator,
    ImageViewer,
    DocumentViewer,
    Generic,
}

fn detect_app_kind(title: &str, wm_class: &str) -> AppKind {
    let lower_title = title.to_ascii_lowercase();
    let lower_class = wm_class.to_ascii_lowercase();

    if lower_class.contains("terminal")
        || lower_title.contains("terminal")
        || lower_class.contains("xterm")
        || lower_class.contains("kitty")
        || lower_class.contains("alacritty")
    {
        AppKind::Terminal
    } else if lower_class.contains("pcmanfm")
        || lower_class.contains("nautilus")
        || lower_class.contains("thunar")
        || lower_class.contains("dolphin")
        || lower_title.contains("workspace")
        || lower_title.contains("folder")
        || lower_class.contains("file")
    {
        AppKind::FileManager
    } else if lower_class.contains("mousepad")
        || lower_class.contains("gedit")
        || lower_class.contains("leafpad")
        || lower_title.ends_with(" - mousepad")
        || lower_class.contains("editor")
    {
        AppKind::TextEditor
    } else if lower_class.contains("firefox")
        || lower_class.contains("chromium")
        || lower_class.contains("chrome")
        || lower_class.contains("browser")
    {
        AppKind::Browser
    } else if lower_class.contains("galculator") || lower_class.contains("calc") {
        AppKind::Calculator
    } else if lower_class.contains("ristretto")
        || lower_class.contains("viewnior")
        || lower_class.contains("gimp")
        || lower_class.contains("inkscape")
        || lower_class.contains("image")
    {
        AppKind::ImageViewer
    } else if lower_class.contains("zathura")
        || lower_class.contains("evince")
        || lower_class.contains("pdf")
    {
        AppKind::DocumentViewer
    } else if title.is_empty() || title == "SLOPOS Desktop" {
        AppKind::Desktop
    } else {
        AppKind::Generic
    }
}

fn send_key_action(window_id: Option<u64>, key_combo: &'static str) -> Box<dyn Fn() + 'static> {
    Box::new(move || {
        if let Some(id) = window_id {
            let _ = Command::new("xdotool")
                .args(["key", "--window", &id.to_string(), key_combo])
                .spawn();
        } else {
            let _ = Command::new("xdotool").args(["key", key_combo]).spawn();
        }
    })
}

fn window_management_action(
    window_id: Option<u64>,
    action: &'static str,
) -> Box<dyn Fn() + 'static> {
    Box::new(move || {
        if let Some(id) = window_id {
            match action {
                "close" => {
                    let _ = Command::new("xdotool")
                        .args(["windowclose", &id.to_string()])
                        .spawn();
                }
                "minimize" => {
                    let _ = Command::new("xdotool")
                        .args(["windowminimize", &id.to_string()])
                        .spawn();
                }
                "maximize" => {
                    let _ = Command::new("wmctrl")
                        .args([
                            "-i",
                            "-r",
                            &id.to_string(),
                            "-b",
                            "toggle,maximized_vert,maximized_horz",
                        ])
                        .spawn();
                }
                _ => {}
            }
        }
    })
}

type MenuItemCallback = Box<dyn Fn() + 'static>;
type MenuItemSpec = (&'static str, Option<MenuItemCallback>);

fn add_menu_section(bar: &MenuBar, label: &str, items: Vec<MenuItemSpec>) {
    let root_item = MenuItem::with_label(label);
    let menu = Menu::new();
    for (item_label, callback) in items {
        if item_label == "---" {
            menu.append(&SeparatorMenuItem::new());
        } else {
            let item = MenuItem::with_label(item_label);
            if let Some(cb) = callback {
                item.connect_activate(move |_| cb());
            } else {
                item.set_sensitive(false);
            }
            menu.append(&item);
        }
    }
    menu.show_all();
    root_item.set_submenu(Some(&menu));
    bar.append(&root_item);
}

fn build_tailored_menubar(kind: AppKind, window_id: Option<u64>) -> MenuBar {
    let bar = MenuBar::new();
    bar.style_context().add_class("slopos-menu-bar");

    match kind {
        AppKind::Desktop => {
            add_menu_section(
                &bar,
                "File",
                vec![
                    (
                        "New Folder",
                        Some(Box::new(|| {
                            let _ = Command::new("xdotool")
                                .args(["key", "ctrl+shift+n"])
                                .spawn();
                        })),
                    ),
                    (
                        "Open…",
                        Some(Box::new(|| {
                            let _ = Command::new("xdotool").args(["key", "ctrl+o"]).spawn();
                        })),
                    ),
                    ("---", None),
                    (
                        "Close Window",
                        Some(Box::new(|| {
                            let _ = Command::new("xdotool").args(["key", "ctrl+w"]).spawn();
                        })),
                    ),
                ],
            );
            add_menu_section(
                &bar,
                "Edit",
                vec![
                    (
                        "Undo",
                        Some(Box::new(|| {
                            let _ = Command::new("xdotool").args(["key", "ctrl+z"]).spawn();
                        })),
                    ),
                    ("---", None),
                    (
                        "Cut",
                        Some(Box::new(|| {
                            let _ = Command::new("xdotool").args(["key", "ctrl+x"]).spawn();
                        })),
                    ),
                    (
                        "Copy",
                        Some(Box::new(|| {
                            let _ = Command::new("xdotool").args(["key", "ctrl+c"]).spawn();
                        })),
                    ),
                    (
                        "Paste",
                        Some(Box::new(|| {
                            let _ = Command::new("xdotool").args(["key", "ctrl+v"]).spawn();
                        })),
                    ),
                    (
                        "Select All",
                        Some(Box::new(|| {
                            let _ = Command::new("xdotool").args(["key", "ctrl+a"]).spawn();
                        })),
                    ),
                ],
            );
            add_menu_section(
                &bar,
                "View",
                vec![
                    ("By Name", Some(Box::new(|| {}))),
                    ("By Date", Some(Box::new(|| {}))),
                    ("By Size", Some(Box::new(|| {}))),
                    ("---", None),
                    (
                        "Refresh Desktop",
                        Some(Box::new(|| {
                            let _ = Command::new("xdotool").args(["key", "F5"]).spawn();
                        })),
                    ),
                ],
            );
            add_menu_section(
                &bar,
                "Special",
                vec![
                    (
                        "Clean Up Desktop",
                        Some(Box::new(|| {
                            let _ = Command::new("xdotool").args(["key", "ctrl+r"]).spawn();
                        })),
                    ),
                    (
                        "Empty Trash",
                        Some(Box::new(|| {
                            let _ = Command::new("trash-empty").spawn();
                        })),
                    ),
                    ("---", None),
                    (
                        "Restart…",
                        Some(Box::new(|| {
                            if let Some((program, args)) = resolve_first_command(REBOOT_COMMANDS) {
                                confirm_action(
                                    "Restart",
                                    "Are you sure you want to restart the system?",
                                    move || spawn_resolved(program, args),
                                );
                            }
                        })),
                    ),
                    (
                        "Shut Down…",
                        Some(Box::new(|| {
                            if let Some((program, args)) = resolve_first_command(POWEROFF_COMMANDS)
                            {
                                confirm_action(
                                    "Shut Down",
                                    "Are you sure you want to shut down the system?",
                                    move || spawn_resolved(program, args),
                                );
                            }
                        })),
                    ),
                ],
            );
            add_menu_section(
                &bar,
                "Help",
                vec![
                    (
                        "SLOPOS-I Help",
                        Some(Box::new(|| {
                            show_message(
                                "SLOPOS-I Help",
                                "SLOPOS-I is a lightweight classic X11 desktop environment.\n\nShortcuts:\nSuper+Space: Application Search\nCtrl+F2: System Menu\nAlt+Tab: Switch Windows\nSuper+Q: Close Window",
                            );
                        })),
                    ),
                    (
                        "About SLOPOS-I",
                        Some(Box::new(|| {
                            show_message(
                                "About SLOPOS-I",
                                "SLOPOS-I\nX11 Macintosh-inspired desktop",
                            );
                        })),
                    ),
                ],
            );
        }
        AppKind::Terminal => {
            add_menu_section(
                &bar,
                "File",
                vec![
                    (
                        "New Window",
                        Some(send_key_action(window_id, "ctrl+shift+n")),
                    ),
                    ("New Tab", Some(send_key_action(window_id, "ctrl+shift+t"))),
                    ("---", None),
                    (
                        "Close Tab",
                        Some(send_key_action(window_id, "ctrl+shift+w")),
                    ),
                    (
                        "Close Window",
                        Some(send_key_action(window_id, "ctrl+shift+q")),
                    ),
                ],
            );
            add_menu_section(
                &bar,
                "Edit",
                vec![
                    ("Copy", Some(send_key_action(window_id, "ctrl+shift+c"))),
                    ("Paste", Some(send_key_action(window_id, "ctrl+shift+v"))),
                    (
                        "Select All",
                        Some(send_key_action(window_id, "ctrl+shift+a")),
                    ),
                    ("---", None),
                    ("Find…", Some(send_key_action(window_id, "ctrl+shift+f"))),
                ],
            );
            add_menu_section(
                &bar,
                "View",
                vec![
                    ("Zoom In", Some(send_key_action(window_id, "ctrl+plus"))),
                    ("Zoom Out", Some(send_key_action(window_id, "ctrl+minus"))),
                    ("Normal Size", Some(send_key_action(window_id, "ctrl+0"))),
                    ("---", None),
                    ("Full Screen", Some(send_key_action(window_id, "F11"))),
                ],
            );
            add_menu_section(
                &bar,
                "Terminal",
                vec![
                    (
                        "Clear Scrollback",
                        Some(send_key_action(window_id, "ctrl+shift+k")),
                    ),
                    (
                        "Reset and Clear",
                        Some(send_key_action(window_id, "ctrl+l")),
                    ),
                ],
            );
            add_menu_section(
                &bar,
                "Tabs",
                vec![
                    (
                        "Previous Tab",
                        Some(send_key_action(window_id, "ctrl+Page_Up")),
                    ),
                    (
                        "Next Tab",
                        Some(send_key_action(window_id, "ctrl+Page_Down")),
                    ),
                ],
            );
            add_menu_section(
                &bar,
                "Help",
                vec![
                    ("Terminal Help", Some(send_key_action(window_id, "F1"))),
                    (
                        "About Terminal",
                        Some(Box::new(|| {
                            show_message(
                            "About Terminal",
                            "Xfce4 Terminal is the high-performance X11 terminal emulator for SLOPOS-I.",
                        );
                        })),
                    ),
                ],
            );
        }
        AppKind::FileManager => {
            add_menu_section(
                &bar,
                "File",
                vec![
                    ("New Window", Some(send_key_action(window_id, "ctrl+n"))),
                    ("New Tab", Some(send_key_action(window_id, "ctrl+t"))),
                    (
                        "New Folder",
                        Some(send_key_action(window_id, "ctrl+shift+n")),
                    ),
                    ("---", None),
                    ("Properties", Some(send_key_action(window_id, "alt+Return"))),
                    ("Close Window", Some(send_key_action(window_id, "ctrl+w"))),
                ],
            );
            add_menu_section(
                &bar,
                "Edit",
                vec![
                    ("Cut", Some(send_key_action(window_id, "ctrl+x"))),
                    ("Copy", Some(send_key_action(window_id, "ctrl+c"))),
                    ("Paste", Some(send_key_action(window_id, "ctrl+v"))),
                    ("Select All", Some(send_key_action(window_id, "ctrl+a"))),
                    ("---", None),
                    (
                        "Preferences",
                        Some(send_key_action(window_id, "ctrl+shift+p")),
                    ),
                ],
            );
            add_menu_section(
                &bar,
                "View",
                vec![
                    ("Icon View", Some(send_key_action(window_id, "ctrl+1"))),
                    ("Compact View", Some(send_key_action(window_id, "ctrl+2"))),
                    (
                        "Detailed List View",
                        Some(send_key_action(window_id, "ctrl+4")),
                    ),
                    ("---", None),
                    (
                        "Show Hidden Files",
                        Some(send_key_action(window_id, "ctrl+h")),
                    ),
                    ("Reload", Some(send_key_action(window_id, "ctrl+r"))),
                ],
            );
            add_menu_section(
                &bar,
                "Bookmarks",
                vec![
                    ("Home Folder", Some(send_key_action(window_id, "alt+Home"))),
                    (
                        "Documents",
                        Some(Box::new(|| {
                            let home = env::var("HOME").unwrap_or_default();
                            let _ = Command::new("pcmanfm")
                                .arg(format!("{home}/Documents"))
                                .spawn();
                        })),
                    ),
                    (
                        "Downloads",
                        Some(Box::new(|| {
                            let home = env::var("HOME").unwrap_or_default();
                            let _ = Command::new("pcmanfm")
                                .arg(format!("{home}/Downloads"))
                                .spawn();
                        })),
                    ),
                ],
            );
            add_menu_section(
                &bar,
                "Go",
                vec![
                    ("Back", Some(send_key_action(window_id, "alt+Left"))),
                    ("Forward", Some(send_key_action(window_id, "alt+Right"))),
                    ("Parent Folder", Some(send_key_action(window_id, "alt+Up"))),
                    ("---", None),
                    ("Location Bar…", Some(send_key_action(window_id, "ctrl+l"))),
                ],
            );
            add_menu_section(
                &bar,
                "Tools",
                vec![
                    ("Open Terminal Here", Some(send_key_action(window_id, "F4"))),
                    (
                        "Find Files…",
                        Some(send_key_action(window_id, "ctrl+shift+f")),
                    ),
                ],
            );
            add_menu_section(
                &bar,
                "Help",
                vec![
                    ("File Manager Help", Some(send_key_action(window_id, "F1"))),
                    (
                        "About File Manager",
                        Some(Box::new(|| {
                            show_message(
                                "About File Manager",
                                "PCManFM is the lightweight, mature X11 file manager for SLOPOS-I.",
                            );
                        })),
                    ),
                ],
            );
        }
        AppKind::TextEditor => {
            add_menu_section(
                &bar,
                "File",
                vec![
                    ("New", Some(send_key_action(window_id, "ctrl+n"))),
                    ("Open…", Some(send_key_action(window_id, "ctrl+o"))),
                    ("Save", Some(send_key_action(window_id, "ctrl+s"))),
                    ("Save As…", Some(send_key_action(window_id, "ctrl+shift+s"))),
                    ("---", None),
                    ("Close", Some(send_key_action(window_id, "ctrl+w"))),
                    ("Quit", Some(send_key_action(window_id, "ctrl+q"))),
                ],
            );
            add_menu_section(
                &bar,
                "Edit",
                vec![
                    ("Undo", Some(send_key_action(window_id, "ctrl+z"))),
                    ("Redo", Some(send_key_action(window_id, "ctrl+shift+z"))),
                    ("---", None),
                    ("Cut", Some(send_key_action(window_id, "ctrl+x"))),
                    ("Copy", Some(send_key_action(window_id, "ctrl+c"))),
                    ("Paste", Some(send_key_action(window_id, "ctrl+v"))),
                    ("Select All", Some(send_key_action(window_id, "ctrl+a"))),
                ],
            );
            add_menu_section(
                &bar,
                "Search",
                vec![
                    ("Find…", Some(send_key_action(window_id, "ctrl+f"))),
                    ("Find Next", Some(send_key_action(window_id, "F3"))),
                    ("Replace…", Some(send_key_action(window_id, "ctrl+r"))),
                ],
            );
            add_menu_section(
                &bar,
                "View",
                vec![
                    ("Line Numbers", Some(send_key_action(window_id, "ctrl+l"))),
                    ("Word Wrap", Some(send_key_action(window_id, "ctrl+w"))),
                    ("---", None),
                    ("Zoom In", Some(send_key_action(window_id, "ctrl+plus"))),
                    ("Zoom Out", Some(send_key_action(window_id, "ctrl+minus"))),
                    ("Normal Size", Some(send_key_action(window_id, "ctrl+0"))),
                ],
            );
            add_menu_section(
                &bar,
                "Document",
                vec![
                    ("Line Endings", Some(Box::new(|| {}))),
                    ("Filetype", Some(Box::new(|| {}))),
                ],
            );
            add_menu_section(
                &bar,
                "Help",
                vec![
                    ("Editor Help", Some(send_key_action(window_id, "F1"))),
                    (
                        "About Mousepad",
                        Some(Box::new(|| {
                            show_message(
                                "About Mousepad",
                                "Mousepad is the classic fast text editor for SLOPOS-I.",
                            );
                        })),
                    ),
                ],
            );
        }
        AppKind::Browser => {
            add_menu_section(
                &bar,
                "File",
                vec![
                    ("New Window", Some(send_key_action(window_id, "ctrl+n"))),
                    ("New Tab", Some(send_key_action(window_id, "ctrl+t"))),
                    (
                        "New Private Window",
                        Some(send_key_action(window_id, "ctrl+shift+p")),
                    ),
                    ("---", None),
                    ("Open File…", Some(send_key_action(window_id, "ctrl+o"))),
                    ("Save Page As…", Some(send_key_action(window_id, "ctrl+s"))),
                    ("Close Tab", Some(send_key_action(window_id, "ctrl+w"))),
                ],
            );
            add_menu_section(
                &bar,
                "Edit",
                vec![
                    ("Undo", Some(send_key_action(window_id, "ctrl+z"))),
                    ("Redo", Some(send_key_action(window_id, "ctrl+y"))),
                    ("---", None),
                    ("Cut", Some(send_key_action(window_id, "ctrl+x"))),
                    ("Copy", Some(send_key_action(window_id, "ctrl+c"))),
                    ("Paste", Some(send_key_action(window_id, "ctrl+v"))),
                    ("Select All", Some(send_key_action(window_id, "ctrl+a"))),
                ],
            );
            add_menu_section(
                &bar,
                "View",
                vec![
                    ("Zoom In", Some(send_key_action(window_id, "ctrl+plus"))),
                    ("Zoom Out", Some(send_key_action(window_id, "ctrl+minus"))),
                    ("Normal Size", Some(send_key_action(window_id, "ctrl+0"))),
                    ("Full Screen", Some(send_key_action(window_id, "F11"))),
                    ("---", None),
                    ("Reload", Some(send_key_action(window_id, "ctrl+r"))),
                ],
            );
            add_menu_section(
                &bar,
                "History",
                vec![
                    ("Back", Some(send_key_action(window_id, "alt+Left"))),
                    ("Forward", Some(send_key_action(window_id, "alt+Right"))),
                    ("---", None),
                    (
                        "Show All History",
                        Some(send_key_action(window_id, "ctrl+h")),
                    ),
                ],
            );
            add_menu_section(
                &bar,
                "Bookmarks",
                vec![
                    (
                        "Bookmark Current Tab",
                        Some(send_key_action(window_id, "ctrl+d")),
                    ),
                    (
                        "Show All Bookmarks",
                        Some(send_key_action(window_id, "ctrl+shift+o")),
                    ),
                ],
            );
            add_menu_section(
                &bar,
                "Tools",
                vec![
                    ("Downloads", Some(send_key_action(window_id, "ctrl+j"))),
                    (
                        "Web Developer Tools",
                        Some(send_key_action(window_id, "F12")),
                    ),
                ],
            );
            add_menu_section(
                &bar,
                "Help",
                vec![
                    ("Browser Help", Some(send_key_action(window_id, "F1"))),
                    (
                        "About Web Browser",
                        Some(Box::new(|| {
                            show_message(
                                "About Web Browser",
                                "Standard secure web browser for SLOPOS-I.",
                            );
                        })),
                    ),
                ],
            );
        }
        AppKind::Calculator => {
            add_menu_section(
                &bar,
                "File",
                vec![
                    (
                        "Copy Calculation",
                        Some(send_key_action(window_id, "ctrl+c")),
                    ),
                    ("Paste", Some(send_key_action(window_id, "ctrl+v"))),
                    ("---", None),
                    ("Close", Some(send_key_action(window_id, "ctrl+w"))),
                ],
            );
            add_menu_section(
                &bar,
                "Edit",
                vec![
                    ("Clear All", Some(send_key_action(window_id, "Escape"))),
                    ("Undo", Some(send_key_action(window_id, "ctrl+z"))),
                ],
            );
            add_menu_section(
                &bar,
                "View",
                vec![
                    ("Basic Mode", Some(send_key_action(window_id, "ctrl+1"))),
                    (
                        "Scientific Mode",
                        Some(send_key_action(window_id, "ctrl+2")),
                    ),
                ],
            );
            add_menu_section(
                &bar,
                "Help",
                vec![
                    ("Calculator Help", Some(send_key_action(window_id, "F1"))),
                    (
                        "About Calculator",
                        Some(Box::new(|| {
                            show_message(
                                "About Calculator",
                                "Galculator scientific and basic calculator for SLOPOS-I.",
                            );
                        })),
                    ),
                ],
            );
        }
        AppKind::ImageViewer | AppKind::DocumentViewer => {
            add_menu_section(
                &bar,
                "File",
                vec![
                    ("Open…", Some(send_key_action(window_id, "ctrl+o"))),
                    ("Save As…", Some(send_key_action(window_id, "ctrl+shift+s"))),
                    ("---", None),
                    ("Close", Some(send_key_action(window_id, "ctrl+w"))),
                    ("Quit", Some(send_key_action(window_id, "ctrl+q"))),
                ],
            );
            add_menu_section(
                &bar,
                "Edit",
                vec![
                    ("Copy", Some(send_key_action(window_id, "ctrl+c"))),
                    ("Select All", Some(send_key_action(window_id, "ctrl+a"))),
                ],
            );
            add_menu_section(
                &bar,
                "View",
                vec![
                    ("Zoom In", Some(send_key_action(window_id, "ctrl+plus"))),
                    ("Zoom Out", Some(send_key_action(window_id, "ctrl+minus"))),
                    ("Original Size", Some(send_key_action(window_id, "ctrl+0"))),
                    ("Full Screen", Some(send_key_action(window_id, "F11"))),
                ],
            );
            add_menu_section(
                &bar,
                "Go",
                vec![
                    ("Previous Page", Some(send_key_action(window_id, "Page_Up"))),
                    ("Next Page", Some(send_key_action(window_id, "Page_Down"))),
                    ("First Page", Some(send_key_action(window_id, "Home"))),
                    ("Last Page", Some(send_key_action(window_id, "End"))),
                ],
            );
            add_menu_section(
                &bar,
                "Help",
                vec![
                    ("Help", Some(send_key_action(window_id, "F1"))),
                    (
                        "About Viewer",
                        Some(Box::new(|| {
                            show_message(
                                "About Viewer",
                                "High-performance document and image viewer for SLOPOS-I.",
                            );
                        })),
                    ),
                ],
            );
        }
        AppKind::Generic => {
            add_menu_section(
                &bar,
                "File",
                vec![
                    ("New", Some(send_key_action(window_id, "ctrl+n"))),
                    ("Open…", Some(send_key_action(window_id, "ctrl+o"))),
                    ("Save", Some(send_key_action(window_id, "ctrl+s"))),
                    ("---", None),
                    (
                        "Close Window",
                        Some(window_management_action(window_id, "close")),
                    ),
                ],
            );
            add_menu_section(
                &bar,
                "Edit",
                vec![
                    ("Undo", Some(send_key_action(window_id, "ctrl+z"))),
                    ("Redo", Some(send_key_action(window_id, "ctrl+y"))),
                    ("---", None),
                    ("Cut", Some(send_key_action(window_id, "ctrl+x"))),
                    ("Copy", Some(send_key_action(window_id, "ctrl+c"))),
                    ("Paste", Some(send_key_action(window_id, "ctrl+v"))),
                    ("Select All", Some(send_key_action(window_id, "ctrl+a"))),
                ],
            );
            add_menu_section(
                &bar,
                "View",
                vec![
                    ("Zoom In", Some(send_key_action(window_id, "ctrl+plus"))),
                    ("Zoom Out", Some(send_key_action(window_id, "ctrl+minus"))),
                    ("Full Screen", Some(send_key_action(window_id, "F11"))),
                    ("---", None),
                    ("Refresh", Some(send_key_action(window_id, "F5"))),
                ],
            );
            add_menu_section(
                &bar,
                "Window",
                vec![
                    (
                        "Minimize",
                        Some(window_management_action(window_id, "minimize")),
                    ),
                    (
                        "Maximize",
                        Some(window_management_action(window_id, "maximize")),
                    ),
                    (
                        "Next Window",
                        Some(Box::new(|| {
                            let _ = Command::new("xdotool").args(["key", "alt+Tab"]).spawn();
                        })),
                    ),
                ],
            );
            add_menu_section(
                &bar,
                "Help",
                vec![
                    ("Help", Some(send_key_action(window_id, "F1"))),
                    (
                        "About SLOPOS-I",
                        Some(Box::new(|| {
                            show_message(
                                "About SLOPOS-I",
                                "SLOPOS-I\nX11 Macintosh-inspired desktop",
                            );
                        })),
                    ),
                ],
            );
        }
    }

    bar.show_all();
    bar
}

fn update_active_window(
    label: &Label,
    global_menu_host: &GtkBox,
    active_menu_state: &RefCell<Option<ActiveMenuState>>,
) {
    let Some(id_text) = command_output("xdotool", &["getactivewindow"]) else {
        show_desktop_state(label, global_menu_host, active_menu_state);
        return;
    };
    let Ok(id) = id_text.trim().parse::<u64>() else {
        show_desktop_state(label, global_menu_host, active_menu_state);
        return;
    };
    let Some(title) = command_output("xdotool", &["getwindowname", &id.to_string()]) else {
        show_desktop_state(label, global_menu_host, active_menu_state);
        return;
    };

    if is_shell_surface(&title) {
        show_desktop_state(label, global_menu_host, active_menu_state);
        return;
    }
    if title.is_empty() {
        label.set_text("SLOPOS Desktop");
    } else {
        let compact = compact_title(&title);
        label.set_text(&compact);
    }

    let next_state = if let Some(exporter) = u32::try_from(id).ok().and_then(gmenu::detect) {
        ActiveMenuState::Exporter(exporter)
    } else {
        let wm_class =
            command_output("xdotool", &["getwindowclassname", &id.to_string()]).unwrap_or_default();
        let kind = detect_app_kind(&title, &wm_class);
        ActiveMenuState::Tailored {
            window_id: Some(id),
            kind,
        }
    };

    refresh_global_menu(global_menu_host, active_menu_state, next_state);
}

fn show_desktop_state(
    label: &Label,
    global_menu_host: &GtkBox,
    active_menu_state: &RefCell<Option<ActiveMenuState>>,
) {
    label.set_text("SLOPOS Desktop");
    refresh_global_menu(
        global_menu_host,
        active_menu_state,
        ActiveMenuState::Tailored {
            window_id: None,
            kind: AppKind::Desktop,
        },
    );
}

fn refresh_global_menu(
    host: &GtkBox,
    current: &RefCell<Option<ActiveMenuState>>,
    next_state: ActiveMenuState,
) {
    if current.borrow().as_ref() == Some(&next_state) {
        return;
    }
    for child in host.children() {
        host.remove(&child);
    }
    *current.borrow_mut() = Some(next_state.clone());

    match next_state {
        ActiveMenuState::Exporter(ref exporter) => {
            if current_active_window_id() != Some(exporter.window_id) {
                host.hide();
                return;
            }
            match gmenu::build_menu_bar(exporter) {
                Ok(menu_bar) => {
                    host.pack_start(&menu_bar, false, false, 0);
                    host.show_all();
                    log::info!(
                        "Imported GTK global menubar bus={} path={}",
                        exporter.bus_name,
                        exporter.menu_path
                    );
                }
                Err(error) => {
                    log::warn!("Could not import focused application's GTK menu: {error}");
                    let wm_class = command_output(
                        "xdotool",
                        &["getwindowclassname", &exporter.window_id.to_string()],
                    )
                    .unwrap_or_default();
                    let kind = detect_app_kind("", &wm_class);
                    let bar = build_tailored_menubar(kind, Some(exporter.window_id as u64));
                    host.pack_start(&bar, false, false, 0);
                    host.show_all();
                }
            }
        }
        ActiveMenuState::Tailored { window_id, kind } => {
            let bar = build_tailored_menubar(kind, window_id);
            host.pack_start(&bar, false, false, 0);
            host.show_all();
        }
    }
}

fn current_active_window_id() -> Option<u32> {
    command_output("xdotool", &["getactivewindow"])
        .and_then(|value| value.trim().parse::<u32>().ok())
}

fn is_shell_surface(title: &str) -> bool {
    matches!(
        title.trim(),
        "SLOPOS Top Bar" | "SLOPOS Application Strip" | "SLOPOS Search" | "SLOPOS Notification"
    )
}

fn compact_title(title: &str) -> String {
    let title = title.trim();
    if title.chars().count() <= 24 {
        return title.to_string();
    }
    let mut value = title.chars().take(23).collect::<String>();
    value.push('…');
    value
}

fn current_volume() -> Option<String> {
    let text = command_output("wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"])?;
    if text.contains("[MUTED]") {
        return Some("Muted".to_string());
    }
    let value = text
        .split_whitespace()
        .find_map(|part| part.parse::<f64>().ok())?;
    Some(format!("{}%", (value * 100.0).round() as i32))
}

fn current_network_state() -> String {
    match command_output("nmcli", &["-t", "-f", "STATE", "general"]) {
        Some(value) if value.to_ascii_lowercase().starts_with("connected") => "Online".to_string(),
        Some(_) => "Offline".to_string(),
        None => "—".to_string(),
    }
}

fn current_battery_state() -> Option<String> {
    for name in ["BAT0", "BAT1"] {
        let path = format!("/sys/class/power_supply/{name}/capacity");
        if let Ok(value) = fs::read_to_string(path) {
            return Some(format!("{}%", value.trim()));
        }
    }
    None
}

fn screen_geometry() -> (i32, i32) {
    let Some(output) = command_output("xrandr", &["--current"]) else {
        return (1280, 800);
    };
    for line in output.lines() {
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
            let scale = ui_scale();
            return ((width / scale).max(1), (height / scale).max(1));
        }
    }
    (1280, 800)
}

fn ui_scale() -> i32 {
    env::var("GDK_SCALE")
        .ok()
        .and_then(|value| value.parse::<i32>().ok())
        .filter(|scale| *scale > 0)
        .unwrap_or(1)
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn build_system_menu() -> Menu {
    let menu = Menu::new();

    let about = MenuItem::with_label("About SLOPOS-I");
    about.connect_activate(|_| {
        show_message("About SLOPOS-I", "SLOPOS-I\nX11 Macintosh-inspired desktop")
    });
    menu.append(&about);
    menu.append(&SeparatorMenuItem::new());

    let settings = MenuItem::with_label("Control Panels…");
    if resolve_program("slopos-settings").is_some() {
        settings.connect_activate(|_| spawn_resolved("slopos-settings", &[]));
    } else {
        settings.set_sensitive(false);
    }
    menu.append(&settings);

    let catalogue = MenuItem::with_label("Software…");
    if resolve_program("slopos-catalogue").is_some() {
        catalogue.connect_activate(|_| spawn_resolved("slopos-catalogue", &[]));
    } else {
        catalogue.set_sensitive(false);
    }
    menu.append(&catalogue);

    let appearance = MenuItem::with_label("Appearance");
    let appearance_menu = Menu::new();
    let classic = MenuItem::with_label("Classic Macintosh (System 6/7)");
    let platinum = MenuItem::with_label("Platinum (Light)");
    let graphite = MenuItem::with_label("Graphite (Dark)");
    if resolve_program("slopos-appearance").is_some() {
        classic.connect_activate(|_| spawn_resolved("slopos-appearance", &["classic"]));
        platinum.connect_activate(|_| spawn_resolved("slopos-appearance", &["platinum"]));
        graphite.connect_activate(|_| spawn_resolved("slopos-appearance", &["graphite"]));
    } else {
        classic.set_sensitive(false);
        platinum.set_sensitive(false);
        graphite.set_sensitive(false);
    }
    appearance_menu.append(&classic);
    appearance_menu.append(&platinum);
    appearance_menu.append(&graphite);
    appearance_menu.show_all();
    appearance.set_submenu(Some(&appearance_menu));
    menu.append(&appearance);
    menu.append(&SeparatorMenuItem::new());

    let lock = MenuItem::with_label("Lock Screen");
    if let Some((program, args)) = resolve_first_command(LOCK_COMMANDS) {
        lock.connect_activate(move |_| spawn_resolved(program, args));
    } else {
        lock.connect_activate(|_| {
            show_message(
                "Lock Screen",
                "No screen locker utility found.\nInstall loginctl, light-locker, xflock4, slock, or i3lock.",
            );
        });
    }
    menu.append(&lock);

    let switch_user = MenuItem::with_label("Switch User…");
    if let Some((program, args)) = resolve_first_command(SWITCH_USER_COMMANDS) {
        switch_user.connect_activate(move |_| {
            confirm_action(
                "Switch User",
                "Switch to the login screen for another user?",
                move || spawn_resolved(program, args),
            );
        });
    } else {
        switch_user.connect_activate(|_| {
            show_message(
                "Switch User",
                "No display manager switch utility found.\nInstall dm-tool (LightDM), gdmflexiserver (GDM), or loginctl.",
            );
        });
    }
    menu.append(&switch_user);

    let sleep = MenuItem::with_label("Sleep");
    if let Some((program, args)) = resolve_first_command(SUSPEND_COMMANDS) {
        sleep.connect_activate(move |_| {
            confirm_action("Sleep", "Put this computer to sleep now?", move || {
                spawn_resolved(program, args)
            });
        });
    } else {
        sleep.set_sensitive(false);
    }
    menu.append(&sleep);

    let logout = MenuItem::with_label("Log Out…");
    logout.connect_activate(|_| {
        confirm_action("Log Out", "End the current SLOPOS session?", || {
            if env::var_os("SLOPOS_SESSION_MANAGED").is_some() {
                unsafe {
                    libc::kill(libc::getppid(), libc::SIGTERM);
                }
            } else if let Some((program, args)) = resolve_first_command(&[
                ("loginctl", &["terminate-session", "self"]),
                ("loginctl", &["terminate-user", ""]),
            ]) {
                spawn_resolved(program, args);
            } else {
                std::process::exit(0);
            }
        });
    });
    menu.append(&logout);

    let restart = MenuItem::with_label("Restart…");
    if let Some((program, args)) = resolve_first_command(REBOOT_COMMANDS) {
        restart.connect_activate(move |_| {
            confirm_action("Restart", "Restart this computer now?", move || {
                spawn_resolved(program, args);
            });
        });
    } else {
        restart.set_sensitive(false);
    }
    menu.append(&restart);

    let shutdown = MenuItem::with_label("Shut Down…");
    if let Some((program, args)) = resolve_first_command(POWEROFF_COMMANDS) {
        shutdown.connect_activate(move |_| {
            confirm_action("Shut Down", "Shut down this computer now?", move || {
                spawn_resolved(program, args);
            });
        });
    } else {
        shutdown.set_sensitive(false);
    }
    menu.append(&shutdown);
    menu.show_all();
    menu
}

fn show_message(title: &str, message: &str) {
    let dialog = platinum_dialog(title, message, &[("Close", ResponseType::Close)]);
    dialog.connect_response(|dialog, _| dialog.close());
    dialog.show_all();
}

fn confirm_action<F>(title: &str, message: &str, action: F)
where
    F: Fn() + 'static,
{
    let dialog = platinum_dialog(
        title,
        message,
        &[("No", ResponseType::No), ("Yes", ResponseType::Yes)],
    );
    dialog.connect_response(move |dialog, response| {
        if response == ResponseType::Yes {
            action();
        }
        dialog.close();
    });
    dialog.show_all();
}

fn platinum_dialog(title: &str, message: &str, buttons: &[(&str, ResponseType)]) -> Dialog {
    let button_specs = buttons.to_vec();
    let dialog = Dialog::with_buttons(
        Some(title),
        None::<&Window>,
        DialogFlags::MODAL,
        &button_specs,
    );
    dialog.set_default_size(360, 150);
    dialog.set_resizable(false);

    let alert = GtkBox::new(Orientation::Horizontal, 9);
    alert.style_context().add_class("slopos-alert-box");
    if let Some(mark) = load_slopos_mark_sized(40) {
        alert.pack_start(&mark, false, false, 0);
    }
    let label = Label::new(Some(message));
    label.set_xalign(0.0);
    label.set_line_wrap(true);
    label.set_line_wrap_mode(pango::WrapMode::WordChar);
    label.set_max_width_chars(48);
    alert.pack_start(&label, true, true, 0);
    dialog.content_area().pack_start(&alert, false, false, 0);
    dialog
}

fn resolve_first_command(
    candidates: &[(&'static str, &'static [&'static str])],
) -> Option<(&'static str, &'static [&'static str])> {
    for &(program, args) in candidates {
        if resolve_program(program).is_some() {
            return Some((program, args));
        }
    }
    None
}

fn spawn_resolved(program: &str, args: &[&str]) {
    let Some(path) = resolve_program(program) else {
        log::warn!("Cannot launch {program}: command not found");
        return;
    };
    if let Err(error) = Command::new(&path).args(args).spawn() {
        log::warn!("Failed to launch {}: {error}", path.display());
    }
}

fn resolve_program(program: &str) -> Option<PathBuf> {
    if program.starts_with("slopos-") {
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
    }

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

fn load_slopos_mark() -> Option<Image> {
    load_slopos_mark_sized(20)
}

fn load_slopos_mark_sized(size: i32) -> Option<Image> {
    let mut candidates = Vec::new();
    if let Ok(share_dir) = env::var("SLOPOS_SHARE_DIR") {
        candidates.push(format!("{share_dir}/slopos-i/slopos-logo.png"));
    }
    candidates.extend([
        "assets/slopos-logo.png".to_string(),
        "/usr/local/share/slopos-i/slopos-logo.png".to_string(),
        "/usr/share/slopos-i/slopos-logo.png".to_string(),
    ]);
    candidates.into_iter().find_map(|path| {
        if !Path::new(&path).is_file() {
            return None;
        }
        match Pixbuf::from_file(&path) {
            Ok(pixbuf) => {
                let mark = if pixbuf.width() >= 512 && pixbuf.height() >= 512 {
                    let crop = (pixbuf.width().min(pixbuf.height()) / 4).max(1);
                    let x = (pixbuf.width() - crop) / 2;
                    let y = ((pixbuf.height() * 3) / 10).min(pixbuf.height() - crop);
                    pixbuf.new_subpixbuf(x, y, crop, crop)
                } else {
                    pixbuf
                };
                let scaled = mark.scale_simple(size, size, InterpType::Bilinear)?;
                Some(Image::from_pixbuf(Some(&scaled)))
            }
            Err(error) => {
                log::warn!("Failed to load SLOPOS mark from {path}: {error}");
                None
            }
        }
    })
}
