//! Macintosh-inspired Top System Bar
//! Reserved top 30px dock window with SLOPOS menu, active window title, search button, status indicators.

use crate::launcher::Launcher;
use gtk::prelude::*;
use gtk::{
    Align, Box as GtkBox, Button, Label, Menu, MenuItem, Orientation, Window, WindowPosition,
    WindowType,
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

        let main_box = GtkBox::new(Orientation::Horizontal, 12);
        main_box.set_margin_start(8);
        main_box.set_margin_end(12);

        // --- Left Section: SLOPOS Menu & Active Window Title ---
        let left_box = GtkBox::new(Orientation::Horizontal, 8);

        // SLOPOS System Menu Button
        let slopos_btn = Button::with_label("SLOPOS");
        let menu = build_slopos_menu();
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

        // Active Application Title Label
        let active_title_label = Label::new(Some("SLOPOS Desktop"));
        active_title_label.set_halign(Align::Start);
        left_box.pack_start(&active_title_label, false, false, 8);

        main_box.pack_start(&left_box, true, true, 0);

        // --- Right Section: Search & Status Indicators ---
        let right_box = GtkBox::new(Orientation::Horizontal, 8);

        // Spotlight Search Launcher Button
        let search_btn = Button::with_label("🔍 Search");
        let l_ref = launcher.clone();
        search_btn.connect_clicked(move |_| {
            l_ref.toggle();
        });
        right_box.pack_start(&search_btn, false, false, 0);

        // Audio Indicator Button
        let audio_btn = Button::with_label("🔊 100%");
        audio_btn.connect_clicked(|_| {
            let _ = Command::new("pavucontrol").spawn();
        });
        right_box.pack_start(&audio_btn, false, false, 0);

        // Network Indicator Label
        let net_label = Label::new(Some("📡 Online"));
        right_box.pack_start(&net_label, false, false, 4);

        // Clock Label
        let clock_label = Label::new(Some("10:00 AM"));
        right_box.pack_start(&clock_label, false, false, 4);

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

fn build_slopos_menu() -> Menu {
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
