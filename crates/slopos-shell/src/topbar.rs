//! SLOPOS-I classic top menu/system bar.

use crate::launcher::Launcher;
use gdk::prelude::*;
use gtk::prelude::*;
use gtk::{
    Align, Box as GtkBox, Button, IconSize, Image, Label, Menu, MenuBar, MenuItem, Orientation,
    SeparatorMenuItem, Window, WindowPosition, WindowType,
};
use std::fs;
use std::process::Command;
use std::rc::Rc;

pub struct TopBar {
    _window: Window,
    _active_title_label: Label,
    _clock_label: Label,
}

impl TopBar {
    pub fn new(launcher: Rc<Launcher>) -> Rc<Self> {
        let window = Window::new(WindowType::Toplevel);
        window.set_title("SLOPOS Top Bar");
        let screen_width = gdk::Screen::default().map(|s| s.width()).unwrap_or(1280);
        window.set_default_size(screen_width, 26);
        window.set_position(WindowPosition::None);
        window.move_(0, 0);
        window.set_decorated(false);
        window.set_keep_above(true);
        window.set_skip_taskbar_hint(true);
        window.set_skip_pager_hint(true);
        window.style_context().add_class("slopos-topbar");

        let main_box = GtkBox::new(Orientation::Horizontal, 0);
        main_box.set_margin_start(5);
        main_box.set_margin_end(8);

        let system_button = Button::with_label("S");
        system_button.style_context().add_class("slopos-logo-btn");
        system_button.set_tooltip_text(Some("SLOPOS menu"));
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
        main_box.pack_start(&system_button, false, false, 0);

        let active_title_label = Label::new(Some("SLOPOS Desktop"));
        active_title_label.style_context().add_class("slopos-active-app");
        active_title_label.set_halign(Align::Start);
        main_box.pack_start(&active_title_label, false, false, 7);

        main_box.pack_start(&build_global_menu_bar(), false, false, 0);

        let status_box = GtkBox::new(Orientation::Horizontal, 7);
        status_box.style_context().add_class("slopos-status-area");

        let search_button = Button::new();
        search_button.style_context().add_class("slopos-menubar-control");
        let search_box = GtkBox::new(Orientation::Horizontal, 3);
        search_box.pack_start(
            &Image::from_icon_name(Some("system-search-symbolic"), IconSize::Menu),
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
        audio_button.style_context().add_class("slopos-menubar-control");
        let audio_box = GtkBox::new(Orientation::Horizontal, 3);
        audio_box.pack_start(
            &Image::from_icon_name(Some("audio-volume-high-symbolic"), IconSize::Menu),
            false,
            false,
            0,
        );
        let audio_label = Label::new(Some("--"));
        audio_box.pack_start(&audio_label, false, false, 0);
        audio_button.add(&audio_box);
        audio_button.connect_clicked(|_| {
            let _ = Command::new("pavucontrol").spawn();
        });
        status_box.pack_start(&audio_button, false, false, 0);

        let network_box = GtkBox::new(Orientation::Horizontal, 3);
        network_box.pack_start(
            &Image::from_icon_name(Some("network-wireless-symbolic"), IconSize::Menu),
            false,
            false,
            0,
        );
        let network_label = Label::new(Some("--"));
        network_box.pack_start(&network_label, false, false, 0);
        status_box.pack_start(&network_box, false, false, 0);

        let battery_label = Label::new(None);
        status_box.pack_start(&battery_label, false, false, 0);

        let clock_label = Label::new(Some("--:--"));
        clock_label.style_context().add_class("slopos-clock");
        status_box.pack_start(&clock_label, false, false, 2);

        main_box.pack_end(&status_box, false, false, 0);
        window.add(&main_box);
        window.show_all();

        install_live_updates(
            &active_title_label,
            &clock_label,
            &audio_label,
            &network_label,
            &battery_label,
        );

        Rc::new(Self {
            _window: window,
            _active_title_label: active_title_label,
            _clock_label: clock_label,
        })
    }
}

fn install_live_updates(
    active_title: &Label,
    clock: &Label,
    audio: &Label,
    network: &Label,
    battery: &Label,
) {
    let active_title = active_title.clone();
    let clock = clock.clone();
    let audio = audio.clone();
    let network = network.clone();
    let battery = battery.clone();

    glib::timeout_add_seconds_local(1, move || {
        if let Some(title) = command_output("xdotool", &["getactivewindow", "getwindowname"]) {
            if !title.is_empty() && title != "SLOPOS Top Bar" && title != "SLOPOS Application Strip" {
                active_title.set_text(&compact_title(&title));
            }
        }

        if let Some(local_time) = command_output("date", &["+%H:%M"]) {
            clock.set_text(&local_time);
        }

        audio.set_text(&current_volume().unwrap_or_else(|| "--".to_string()));
        network.set_text(&current_network_state());
        battery.set_text(&current_battery_state().unwrap_or_default());
        glib::ControlFlow::Continue
    });
}

fn compact_title(title: &str) -> String {
    let title = title.trim();
    if title.len() <= 28 { return title.to_string(); }
    let mut value = title.chars().take(27).collect::<String>();
    value.push('…');
    value
}

fn current_volume() -> Option<String> {
    let text = command_output("wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"]) ?;
    let value = text.split_whitespace().find_map(|part| part.parse::<f64>().ok())?;
    Some(format!("{}%", (value * 100.0).round() as i32))
}

fn current_network_state() -> String {
    match command_output("nmcli", &["-t", "-f", "STATE", "general"]) {
        Some(value) if value.to_ascii_lowercase().starts_with("connected") => "Online".to_string(),
        Some(_) => "Offline".to_string(),
        None => "--".to_string(),
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

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() { return None; }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn build_system_menu() -> Menu {
    let menu = Menu::new();

    let about = MenuItem::with_label("About SLOPOS-I");
    about.connect_activate(|_| {
        let _ = Command::new("zenity")
            .args(["--info", "--title=About SLOPOS-I", "--text=SLOPOS-I\nX11 Platinum Desktop"])
            .spawn();
    });
    menu.append(&about);
    menu.append(&SeparatorMenuItem::new());

    let settings = MenuItem::with_label("System Settings…");
    settings.connect_activate(|_| { let _ = Command::new("slopos-settings").spawn(); });
    menu.append(&settings);

    let catalogue = MenuItem::with_label("Software Catalogue…");
    catalogue.connect_activate(|_| { let _ = Command::new("slopos-catalogue").spawn(); });
    menu.append(&catalogue);
    menu.append(&SeparatorMenuItem::new());

    let lock = MenuItem::with_label("Lock Screen");
    lock.connect_activate(|_| { let _ = Command::new("xset").args(["s", "activate"]).spawn(); });
    menu.append(&lock);

    let logout = MenuItem::with_label("Log Out…");
    logout.connect_activate(|_| { let _ = Command::new("pkill").args(["-TERM", "-x", "slopos-session"]).spawn(); });
    menu.append(&logout);

    let restart = MenuItem::with_label("Restart…");
    restart.connect_activate(|_| { let _ = Command::new("systemctl").arg("reboot").spawn(); });
    menu.append(&restart);

    let shutdown = MenuItem::with_label("Shut Down…");
    shutdown.connect_activate(|_| { let _ = Command::new("systemctl").arg("poweroff").spawn(); });
    menu.append(&shutdown);

    menu.show_all();
    menu
}

fn build_global_menu_bar() -> MenuBar {
    let menu_bar = MenuBar::new();
    menu_bar.style_context().add_class("slopos-menu-bar");

    let file_item = MenuItem::with_label("File");
    let file_menu = Menu::new();
    file_menu.append(&command_item("New File Window", || spawn("pcmanfm", &[])));
    file_menu.append(&command_item("Open…", || spawn("pcmanfm", &[])));
    file_menu.append(&SeparatorMenuItem::new());
    file_menu.append(&command_item("Close Window", || spawn("xdotool", &["getactivewindow", "windowclose"])));
    file_item.set_submenu(Some(&file_menu));
    menu_bar.append(&file_item);

    let edit_item = MenuItem::with_label("Edit");
    let edit_menu = Menu::new();
    edit_menu.append(&shortcut_item("Undo", "ctrl+z"));
    edit_menu.append(&SeparatorMenuItem::new());
    edit_menu.append(&shortcut_item("Cut", "ctrl+x"));
    edit_menu.append(&shortcut_item("Copy", "ctrl+c"));
    edit_menu.append(&shortcut_item("Paste", "ctrl+v"));
    edit_menu.append(&shortcut_item("Select All", "ctrl+a"));
    edit_item.set_submenu(Some(&edit_menu));
    menu_bar.append(&edit_item);

    let view_item = MenuItem::with_label("View");
    let view_menu = Menu::new();
    view_menu.append(&shortcut_item("Refresh", "F5"));
    view_menu.append(&SeparatorMenuItem::new());
    view_menu.append(&shortcut_item("Zoom In", "ctrl+plus"));
    view_menu.append(&shortcut_item("Zoom Out", "ctrl+minus"));
    view_item.set_submenu(Some(&view_menu));
    menu_bar.append(&view_item);

    let window_item = MenuItem::with_label("Window");
    let window_menu = Menu::new();
    window_menu.append(&command_item("Minimize", || spawn("xdotool", &["getactivewindow", "windowminimize"])));
    window_menu.append(&command_item("Zoom / Maximize", || spawn("wmctrl", &["-r", ":ACTIVE:", "-b", "toggle,maximized_vert,maximized_horz"])));
    window_menu.append(&command_item("Next Window", || spawn("xdotool", &["key", "alt+Tab"])));
    window_item.set_submenu(Some(&window_menu));
    menu_bar.append(&window_item);

    let help_item = MenuItem::with_label("Help");
    let help_menu = Menu::new();
    help_menu.append(&command_item("SLOPOS-I Help", || {
        spawn("zenity", &["--info", "--title=SLOPOS-I Help", "--text=Super+Space: Search\nSuper+Left/Right: switch desktop\nSuper+Q: close window"])
    }));
    help_item.set_submenu(Some(&help_menu));
    menu_bar.append(&help_item);

    menu_bar.show_all();
    menu_bar
}

fn command_item<F>(label: &str, action: F) -> MenuItem
where
    F: Fn() + 'static,
{
    let item = MenuItem::with_label(label);
    item.connect_activate(move |_| action());
    item
}

fn shortcut_item(label: &str, shortcut: &'static str) -> MenuItem {
    command_item(label, move || spawn("xdotool", &["key", "--clearmodifiers", shortcut]))
}

fn spawn(program: &str, args: &[&str]) {
    let _ = Command::new(program).args(args).spawn();
}
