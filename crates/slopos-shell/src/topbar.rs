//! Macintosh-inspired Global Top Bar
//! Features SLOPOS system logo menu, global application menu bar (File, Edit, View, Window, Help),
//! active window name, Spotlight search trigger, and clean GTK status bar indicators.

use crate::launcher::Launcher;
use gtk::prelude::*;
use gtk::{
    Align, Box as GtkBox, Button, IconSize, Image, Label, Menu, MenuBar, MenuItem, Orientation,
    Window, WindowPosition, WindowType,
};
use std::process::Command;
use std::rc::Rc;

pub struct TopBar {
    _window: Window,
    active_title_label: Label,
    clock_label: Label,
}

impl TopBar {
    pub fn new(launcher: Rc<Launcher>) -> Rc<Self> {
        let window = Window::new(WindowType::Toplevel);
        window.set_title("SLOPOS Top Bar");
        window.set_default_size(1280, 30);
        window.set_position(WindowPosition::None);
        window.move_(0, 0);
        window.set_decorated(false);
        window.set_keep_above(true);
        window.set_skip_taskbar_hint(true);
        window.set_skip_pager_hint(true);

        let style_ctx = window.style_context();
        style_ctx.add_class("slopos-topbar");

        let main_box = GtkBox::new(Orientation::Horizontal, 4);
        main_box.set_margin_start(6);
        main_box.set_margin_end(12);

        // --- Left Section: SLOPOS Logo Menu & Global Application Menu Bar ---
        let left_box = GtkBox::new(Orientation::Horizontal, 4);

        // SLOPOS Logo Button
        let slopos_btn = Button::with_label("");
        slopos_btn.style_context().add_class("slopos-logo-btn");
        let menu = build_slopos_system_menu();
        let menu_ref = menu.clone();
        slopos_btn.connect_clicked(move |btn| {
            menu_ref.popup_at_widget(
                btn,
                gdk::Gravity::SouthWest,
                gdk::Gravity::NorthWest,
                None,
            );
        });
        left_box.pack_start(&slopos_btn, false, false, 0);

        // Active Application Name Label (Bold)
        let active_title_label = Label::new(Some("SLOPOS Desktop"));
        active_title_label.style_context().add_class("slopos-active-app");
        active_title_label.set_halign(Align::Start);
        left_box.pack_start(&active_title_label, false, false, 6);

        // Global Application Menu Bar (File, Edit, View, Window, Help)
        let global_menu_bar = build_global_menu_bar();
        left_box.pack_start(&global_menu_bar, false, false, 4);

        main_box.pack_start(&left_box, true, true, 0);

        // --- Right Section: Search & Status Indicators ---
        let right_box = GtkBox::new(Orientation::Horizontal, 6);

        // Spotlight Search Launcher Button
        let search_btn = Button::new();
        let search_box = GtkBox::new(Orientation::Horizontal, 4);
        let search_icon = Image::from_icon_name(Some("system-search-symbolic"), IconSize::Menu);
        search_box.pack_start(&search_icon, false, false, 0);
        search_box.pack_start(&Label::new(Some("Search")), false, false, 0);
        search_btn.add(&search_box);

        let l_ref = launcher.clone();
        search_btn.connect_clicked(move |_| {
            l_ref.toggle();
        });
        right_box.pack_start(&search_btn, false, false, 0);

        // Audio Status Indicator
        let audio_btn = Button::new();
        let audio_box = GtkBox::new(Orientation::Horizontal, 4);
        let audio_icon = Image::from_icon_name(Some("audio-volume-high-symbolic"), IconSize::Menu);
        audio_box.pack_start(&audio_icon, false, false, 0);
        audio_box.pack_start(&Label::new(Some("100%")), false, false, 0);
        audio_btn.add(&audio_box);
        audio_btn.connect_clicked(|_| {
            let _ = Command::new("pavucontrol").spawn();
        });
        right_box.pack_start(&audio_btn, false, false, 0);

        // Network Status Indicator
        let net_box = GtkBox::new(Orientation::Horizontal, 4);
        let net_icon = Image::from_icon_name(Some("network-wireless-symbolic"), IconSize::Menu);
        net_box.pack_start(&net_icon, false, false, 0);
        net_box.pack_start(&Label::new(Some("Online")), false, false, 0);
        right_box.pack_start(&net_box, false, false, 4);

        // Battery Status Indicator
        let battery_icon = Image::from_icon_name(Some("battery-good-symbolic"), IconSize::Menu);
        right_box.pack_start(&battery_icon, false, false, 2);

        // Clock Label
        let clock_label = Label::new(Some("10:00 AM"));
        right_box.pack_start(&clock_label, false, false, 6);

        main_box.pack_end(&right_box, false, false, 0);

        window.add(&main_box);
        window.show_all();

        let topbar = Rc::new(Self {
            _window: window,
            active_title_label,
            clock_label,
        });

        topbar.start_clock_timer();
        topbar
    }

    fn start_clock_timer(&self) {
        let label = self.clock_label.clone();
        glib::timeout_add_seconds_local(1, move || {
            let now = chrono_now();
            label.set_text(&now);
            glib::ControlFlow::Continue
        });
    }

    pub fn set_active_window_title(&self, title: &str) {
        self.active_title_label.set_text(title);
    }
}

fn build_slopos_system_menu() -> Menu {
    let menu = Menu::new();

    let about_item = MenuItem::with_label("About SLOPOS-I");
    about_item.connect_activate(|_| {
        let _ = Command::new("zenity")
            .args(&["--info", "--title=About SLOPOS-I", "--text=SLOPOS-I Macintosh-inspired Desktop Operating Environment\nVersion 1.0 (X11)"])
            .spawn();
    });
    menu.append(&about_item);

    menu.append(&MenuItem::new()); // Separator

    let settings_item = MenuItem::with_label("System Settings...");
    settings_item.connect_activate(|_| {
        let _ = Command::new("slopos-settings").spawn();
    });
    menu.append(&settings_item);

    let catalogue_item = MenuItem::with_label("AppImage Catalogue...");
    catalogue_item.connect_activate(|_| {
        let _ = Command::new("slopos-catalogue").spawn();
    });
    menu.append(&catalogue_item);

    menu.append(&MenuItem::new()); // Separator

    let lock_item = MenuItem::with_label("Lock Screen");
    lock_item.connect_activate(|_| {
        let _ = Command::new("xset").args(&["s", "activate"]).spawn();
    });
    menu.append(&lock_item);

    let logout_item = MenuItem::with_label("Log Out...");
    logout_item.connect_activate(|_| {
        let _ = Command::new("pkill").arg("openbox").spawn();
    });
    menu.append(&logout_item);

    let reboot_item = MenuItem::with_label("Restart...");
    reboot_item.connect_activate(|_| {
        let _ = Command::new("systemctl").arg("reboot").spawn();
    });
    menu.append(&reboot_item);

    let shutdown_item = MenuItem::with_label("Shut Down...");
    shutdown_item.connect_activate(|_| {
        let _ = Command::new("systemctl").arg("poweroff").spawn();
    });
    menu.append(&shutdown_item);

    menu.show_all();
    menu
}

fn build_global_menu_bar() -> MenuBar {
    let menu_bar = MenuBar::new();
    menu_bar.style_context().add_class("slopos-menu-bar");

    // File Menu
    let file_item = MenuItem::with_label("File");
    let file_menu = Menu::new();
    let new_window = MenuItem::with_label("New Window");
    new_window.connect_activate(|_| {
        let _ = Command::new("pcmanfm").spawn();
    });
    file_menu.append(&new_window);
    let open_file = MenuItem::with_label("Open File...");
    open_file.connect_activate(|_| {
        let _ = Command::new("pcmanfm").spawn();
    });
    file_menu.append(&open_file);
    file_menu.append(&MenuItem::new()); // Separator
    let close_win = MenuItem::with_label("Close Window");
    close_win.connect_activate(|_| {
        let _ = Command::new("xdotool").args(&["getactivewindow", "windowclose"]).spawn();
    });
    file_menu.append(&close_win);
    file_item.set_submenu(Some(&file_menu));
    menu_bar.append(&file_item);

    // Edit Menu
    let edit_item = MenuItem::with_label("Edit");
    let edit_menu = Menu::new();
    let undo = MenuItem::with_label("Undo");
    let cut = MenuItem::with_label("Cut");
    let copy = MenuItem::with_label("Copy");
    let paste = MenuItem::with_label("Paste");
    let select_all = MenuItem::with_label("Select All");
    edit_menu.append(&undo);
    edit_menu.append(&MenuItem::new());
    edit_menu.append(&cut);
    edit_menu.append(&copy);
    edit_menu.append(&paste);
    edit_menu.append(&select_all);
    edit_item.set_submenu(Some(&edit_menu));
    menu_bar.append(&edit_item);

    // View Menu
    let view_item = MenuItem::with_label("View");
    let view_menu = Menu::new();
    let show_dock = MenuItem::with_label("Show Dock");
    let zoom_in = MenuItem::with_label("Zoom In");
    let zoom_out = MenuItem::with_label("Zoom Out");
    view_menu.append(&show_dock);
    view_menu.append(&zoom_in);
    view_menu.append(&zoom_out);
    view_item.set_submenu(Some(&view_menu));
    menu_bar.append(&view_item);

    // Window Menu
    let window_item = MenuItem::with_label("Window");
    let window_menu = Menu::new();
    let minimize = MenuItem::with_label("Minimize");
    minimize.connect_activate(|_| {
        let _ = Command::new("xdotool").args(&["getactivewindow", "windowminimize"]).spawn();
    });
    let zoom = MenuItem::with_label("Zoom / Maximize");
    zoom.connect_activate(|_| {
        let _ = Command::new("xdotool").args(&["getactivewindow", "windowsize", "100%", "100%"]).spawn();
    });
    window_menu.append(&minimize);
    window_menu.append(&zoom);
    window_item.set_submenu(Some(&window_menu));
    menu_bar.append(&window_item);

    // Help Menu
    let help_item = MenuItem::with_label("Help");
    let help_menu = Menu::new();
    let slopos_help = MenuItem::with_label("SLOPOS-I Help");
    slopos_help.connect_activate(|_| {
        let _ = Command::new("zenity").args(&["--info", "--title=SLOPOS Help", "--text=Press Super+Space to launch Spotlight Launcher."]).spawn();
    });
    help_menu.append(&slopos_help);
    help_item.set_submenu(Some(&help_menu));
    menu_bar.append(&help_item);

    menu_bar.show_all();
    menu_bar
}

fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let hours = (secs / 3600 + 5) % 24;
    let mins = (secs / 60) % 60;
    format!("{:02}:{:02}", hours, mins)
}
