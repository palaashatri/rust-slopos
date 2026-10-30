//! SLOPOS-I top menu and system bar.
//!
//! Event-driven status, global menu hosting, and desktop integration
//! powered by x11rb and system service adapters without subprocess polling.

use crate::launcher::Launcher;
use crate::menu::gmenu::{self, GtkMenuExporter};
use crate::services::audio;
use crate::services::clock;
use crate::services::session;
use crate::services::SystemStatus;
use crate::x11::{MonitorModel, X11Event};
use gdk_pixbuf::{InterpType, Pixbuf};
use gtk::atk::prelude::AtkObjectExt;
use gtk::prelude::*;
use gtk::{
    Align, Box as GtkBox, Button, Dialog, DialogFlags, IconSize, Image, Label, Menu, MenuItem,
    Orientation, ResponseType, SeparatorMenuItem, Window, WindowPosition, WindowType,
};
use slopos_core::process::{self, FILE_MANAGERS, TERMINALS, TEXT_EDITORS};
use slopos_services::power::SuspendCapability;
use std::cell::{Cell, RefCell};
use std::env;
use std::path::Path;
use std::rc::Rc;

pub struct TopBar {
    window: Window,
    active_title_label: Label,
    _clock_label: Label,
    audio_label: Label,
    network_label: Label,
    network_icon: Image,
    network_menu: Menu,
    battery_box: GtkBox,
    battery_label: Label,
    global_menu_host: GtkBox,
    launcher: Rc<Launcher>,
    active_menu_state: RefCell<Option<GtkMenuExporter>>,
    current_active_window: Cell<Option<u32>>,
    suspend_capability: Cell<Option<SuspendCapability>>,
    system_sleep_item: MenuItem,
    desktop_sleep_item: RefCell<Option<MenuItem>>,
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
        window.connect_delete_event(|_, _| glib::Propagation::Stop);
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
        let (system_menu, system_sleep_item) = build_system_menu(None);
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
        active_title_label.set_xalign(0.0);
        active_title_label.set_ellipsize(pango::EllipsizeMode::End);
        active_title_label.set_max_width_chars(28);
        set_accessible_name(&active_title_label, "Active application");
        main_box.pack_start(&active_title_label, false, false, 5);

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
        let audio_label = Label::new(Some("Vol: --"));
        audio_box.pack_start(&audio_label, false, false, 0);
        audio_button.add(&audio_box);
        audio_button.set_tooltip_text(Some("Sound & volume preferences"));
        let sound_menu = build_sound_menu();
        let sound_menu_ref = sound_menu.clone();
        audio_button.connect_clicked(move |button| {
            sound_menu_ref.popup_at_widget(
                button,
                gdk::Gravity::SouthWest,
                gdk::Gravity::NorthWest,
                None,
            );
        });
        status_box.pack_start(&audio_button, false, false, 0);

        let network_button = Button::new();
        network_button
            .style_context()
            .add_class("slopos-menubar-control");
        set_accessible_name(&network_button, "Network connections");
        let network_box = GtkBox::new(Orientation::Horizontal, 3);
        let network_icon = Image::from_icon_name(Some("network-offline-symbolic"), IconSize::Menu);
        network_box.pack_start(&network_icon, false, false, 0);
        let network_label = Label::new(Some("…"));
        network_box.pack_start(&network_label, false, false, 0);
        network_button.add(&network_box);
        network_button.set_tooltip_text(Some("Network & Wi-Fi connections"));
        let network_menu = Menu::new();
        fill_network_menu(
            &network_menu,
            &crate::services::network::NetworkSummary::checking(),
        );
        let network_menu_ref = network_menu.clone();
        network_button.connect_clicked(move |button| {
            network_menu_ref.popup_at_widget(
                button,
                gdk::Gravity::SouthWest,
                gdk::Gravity::NorthWest,
                None,
            );
        });
        status_box.pack_start(&network_button, false, false, 0);

        let battery_box = GtkBox::new(Orientation::Horizontal, 3);
        set_accessible_name(&battery_box, "Battery status");
        battery_box.pack_start(
            &Image::from_icon_name(Some("battery-good-symbolic"), IconSize::Menu),
            false,
            false,
            0,
        );
        let battery_label = Label::new(Some(""));
        battery_box.pack_start(&battery_label, false, false, 0);
        battery_box.set_visible(false);
        status_box.pack_start(&battery_box, false, false, 0);

        let clock_button = Button::new();
        clock_button
            .style_context()
            .add_class("slopos-menubar-control");
        let initial_clock = clock::current_time_str();
        let clock_label = Label::new(Some(&initial_clock));
        clock_label.style_context().add_class("slopos-clock");
        clock_button.add(&clock_label);
        clock_button.set_tooltip_text(Some("Local time (Click for Date & Time Settings)"));
        set_accessible_name(&clock_button, "Date and time settings");
        clock_button.connect_clicked(|_| open_control_panel("--datetime"));
        status_box.pack_start(&clock_button, false, false, 2);

        main_box.pack_end(&status_box, false, false, 0);
        window.add(&main_box);
        window.show_all();

        let topbar = Rc::new(Self {
            window,
            active_title_label,
            _clock_label: clock_label.clone(),
            audio_label,
            network_label,
            network_icon,
            network_menu,
            battery_box,
            battery_label,
            global_menu_host,
            launcher,
            active_menu_state: RefCell::new(None),
            current_active_window: Cell::new(None),
            suspend_capability: Cell::new(None),
            system_sleep_item,
            desktop_sleep_item: RefCell::new(None),
        });

        topbar.show_desktop_state();
        announce_global_menu();

        // In-process local clock ticker (every 1 second, zero I/O or subprocesses)
        glib::timeout_add_seconds_local(1, move || {
            clock_label.set_text(&clock::current_time_str());
            glib::ControlFlow::Continue
        });

        topbar
    }

    pub fn update_system_status(&self, status: &SystemStatus) {
        self.audio_label.set_text(&status.audio_text);
        self.network_label.set_text(&status.network.label);
        self.network_icon
            .set_from_icon_name(Some(status.network.icon), IconSize::Menu);
        fill_network_menu(&self.network_menu, &status.network);
        if let Some(battery_text) = &status.battery_text {
            self.battery_label.set_text(battery_text);
            self.battery_box.set_visible(true);
        } else {
            self.battery_label.set_text("");
            self.battery_box.set_visible(false);
        }
        self.suspend_capability.set(status.suspend_capability);
        set_suspend_menu_state(&self.system_sleep_item, status.suspend_capability);
        if let Some(item) = self.desktop_sleep_item.borrow().as_ref() {
            set_suspend_menu_state(item, status.suspend_capability);
        }
    }

    pub fn handle_x11_event(&self, event: &X11Event) {
        match event {
            X11Event::ActiveWindowChanged {
                window_id,
                title,
                is_fullscreen,
                ..
            } => {
                self.current_active_window.set(*window_id);
                if is_shell_surface(title) || window_id.is_none() {
                    self.show_desktop_state();
                } else {
                    let display_title = if title.is_empty() {
                        "SLOPOS Desktop".to_string()
                    } else {
                        compact_title(title)
                    };
                    self.active_title_label.set_text(&display_title);
                    let exporter = window_id.and_then(gmenu::detect);
                    self.refresh_global_menu(exporter, *window_id, &display_title);
                }

                self.set_fullscreen_visibility(*is_fullscreen);
            }
            X11Event::WindowStateChanged {
                window_id,
                is_fullscreen,
                ..
            } => {
                if self.current_active_window.get() == Some(*window_id) {
                    self.set_fullscreen_visibility(*is_fullscreen);
                }
            }
            X11Event::WindowTitleChanged { window_id, title } => {
                if self.current_active_window.get() == Some(*window_id) && !is_shell_surface(title)
                {
                    self.active_title_label.set_text(&compact_title(title));
                }
            }
            X11Event::MonitorsChanged { model } => {
                self.reposition_for_monitors(model);
            }
            _ => {}
        }
    }

    fn show_desktop_state(&self) {
        self.active_title_label.set_text("SLOPOS Desktop");
        self.current_active_window.set(None);
        *self.active_menu_state.borrow_mut() = None;
        for child in self.global_menu_host.children() {
            self.global_menu_host.remove(&child);
        }
        let (desktop_menu, sleep_item) =
            build_desktop_menu_bar(self.launcher.clone(), self.suspend_capability.get());
        self.desktop_sleep_item.replace(Some(sleep_item));
        self.global_menu_host
            .pack_start(&desktop_menu, false, false, 0);
        self.global_menu_host.show_all();
    }

    fn set_fullscreen_visibility(&self, is_fullscreen: bool) {
        if is_fullscreen {
            if self.window.is_visible() {
                self.window.hide();
            }
        } else if !self.window.is_visible() {
            self.window.show_all();
            self.window.present();
        }
    }

    fn reposition_for_monitors(&self, model: &MonitorModel) {
        if let Some(primary) = model.primary() {
            self.window.resize(primary.gdk_width(), 26);
            self.window.move_(primary.gdk_x(), primary.gdk_y());
        }
    }

    fn refresh_global_menu(
        &self,
        next: Option<GtkMenuExporter>,
        window_id: Option<u32>,
        app_title: &str,
    ) {
        if let Some(ref exporter) = next {
            if *self.active_menu_state.borrow() == Some(exporter.clone()) {
                return;
            }
        }

        for child in self.global_menu_host.children() {
            self.global_menu_host.remove(&child);
        }
        *self.active_menu_state.borrow_mut() = None;
        self.desktop_sleep_item.replace(None);

        if let Some(exporter) = next {
            match gmenu::build_menu_bar(&exporter) {
                Ok(menu_bar) => {
                    self.global_menu_host.pack_start(&menu_bar, false, false, 0);
                    self.global_menu_host.show_all();
                    log::info!(
                        "Imported GTK global menubar bus={} path={}",
                        exporter.bus_name,
                        exporter.menu_path
                    );
                    *self.active_menu_state.borrow_mut() = Some(exporter);
                    return;
                }
                Err(error) => {
                    log::warn!("Could not import focused application's GTK menu: {error}");
                }
            }
        }

        if let Some(win_id) = window_id {
            let win_menu = build_window_action_menu_bar(win_id, app_title);
            self.global_menu_host.pack_start(&win_menu, false, false, 0);
            self.global_menu_host.show_all();
        } else {
            let (desktop_menu, sleep_item) =
                build_desktop_menu_bar(self.launcher.clone(), self.suspend_capability.get());
            self.desktop_sleep_item.replace(Some(sleep_item));
            self.global_menu_host
                .pack_start(&desktop_menu, false, false, 0);
            self.global_menu_host.show_all();
        }
    }
}

/// Own the bus name that tells SLOPOS toolkit apps to hand their menus to
/// this bar (`slopos_ui::platform::global_menu::SHELL_NAME`). Ownership ends
/// with this process, so apps fall back to in-window menus if the shell
/// dies. The name is held for the life of the shell.
fn announce_global_menu() {
    let _owner = gio::bus_own_name(
        gio::BusType::Session,
        GLOBAL_MENU_NAME,
        gio::BusNameOwnerFlags::NONE,
        |_, _| {},
        |_, name| log::info!("Global menu bar announced as {name}"),
        |_, name| log::warn!("Could not own {name}; apps keep in-window menus"),
    );
}

/// Must match `slopos_ui::platform::global_menu::SHELL_NAME`.
pub const GLOBAL_MENU_NAME: &str = "org.slopos.GlobalMenu";

/// `SIGUSR2` (sent by the Ctrl+F2 listener and by automation) opens the
/// system menu with keyboard focus on its first item.
fn install_system_menu_signal_bridge(button: &Button, menu: &Menu) {
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
    glib::unix_signal_add_local(libc::SIGUSR2, move || {
        keyboard_open_pending.set(true);
        menu.popup_at_widget(
            &button,
            gdk::Gravity::SouthWest,
            gdk::Gravity::NorthWest,
            None,
        );
        log::info!("SLOPOS_SYSTEM_MENU_KEYBOARD_OPENED");
        glib::ControlFlow::Continue
    });
}

fn is_shell_surface(title: &str) -> bool {
    matches!(
        title.trim(),
        "SLOPOS Top Bar" | "SLOPOS Search" | "SLOPOS Notification"
    )
}

fn compact_title(title: &str) -> String {
    let t = title.trim();
    if t.is_empty() {
        return "SLOPOS Desktop".to_string();
    }
    if let Some((_, app)) = t.rsplit_once(" — ") {
        let app = app.trim();
        if !app.is_empty() {
            return app.to_string();
        }
    }
    if let Some((_, app)) = t.rsplit_once(" - ") {
        let app = app.trim();
        if !app.is_empty() {
            return app.to_string();
        }
    }
    t.chars().take(28).collect()
}

fn screen_geometry() -> (i32, i32) {
    gdk::Display::default()
        .and_then(|disp| {
            disp.primary_monitor()
                .or_else(|| disp.monitor(0))
                .map(|mon| {
                    let rect = mon.geometry();
                    (rect.width().max(1), rect.height().max(1))
                })
        })
        .unwrap_or((1280, 800))
}

fn build_desktop_menu_bar(
    launcher: Rc<Launcher>,
    suspend_capability: Option<SuspendCapability>,
) -> (gtk::MenuBar, MenuItem) {
    let bar = gtk::MenuBar::new();
    bar.style_context().add_class("slopos-menu-bar");

    // File
    let file_menu = Menu::new();
    let new_folder = MenuItem::with_label("New Folder");
    new_folder.connect_activate(|_| match crate::desktop_files::create_untitled_folder() {
        Ok(path) => log::info!("Created {}", path.display()),
        Err(error) => show_message("Could not create a folder", &error),
    });
    file_menu.append(&new_folder);
    file_menu.append(&program_item(
        "Open File Manager…",
        FILE_MANAGERS,
        "No file manager is installed",
    ));
    file_menu.append(&program_item(
        "New Text Document…",
        TEXT_EDITORS,
        "No text editor is installed",
    ));
    file_menu.append(&program_item(
        "Open Terminal…",
        TERMINALS,
        "No terminal emulator is installed",
    ));
    file_menu.append(&SeparatorMenuItem::new());
    let find_apps = MenuItem::with_label("Find Applications… (Super+Space)");
    let launcher_ref = launcher.clone();
    find_apps.connect_activate(move |_| launcher_ref.toggle());
    file_menu.append(&find_apps);
    append_submenu(&bar, "File", &file_menu);

    // View
    let view_menu = Menu::new();
    view_menu.append(&slopos_item(
        "Control Panels…",
        control_panels_program(),
        &[],
    ));
    view_menu.append(&slopos_item("Software…", "slopos-catalogue", &[]));
    append_submenu(&bar, "View", &view_menu);

    // Special
    let special_menu = Menu::new();
    let empty_trash = MenuItem::with_label("Empty Trash…");
    empty_trash.connect_activate(|_| {
        confirm_action(
            "Empty Trash",
            "Permanently delete all items in the Trash? This cannot be undone.",
            || {
                run_off_thread(
                    "Could not empty the Trash",
                    crate::desktop_files::empty_trash,
                );
            },
        );
    });
    special_menu.append(&empty_trash);
    special_menu.append(&SeparatorMenuItem::new());
    let sleep = append_session_items(&special_menu, suspend_capability, false);
    append_submenu(&bar, "Special", &special_menu);

    // Help
    let help_menu = Menu::new();
    let about = MenuItem::with_label("About SLOPOS-I…");
    about.connect_activate(|_| show_message("About SLOPOS-I", &about_text()));
    help_menu.append(&about);
    append_submenu(&bar, "Help", &help_menu);

    bar.show_all();
    (bar, sleep)
}

/// Menu shown for an application that does not export its own menu: real
/// window-management commands only.
fn build_window_action_menu_bar(window_id: u32, _app_title: &str) -> gtk::MenuBar {
    let bar = gtk::MenuBar::new();
    bar.style_context().add_class("slopos-menu-bar");

    let window_menu = Menu::new();
    let minimize = MenuItem::with_label("Minimize (Super+M)");
    minimize.connect_activate(move |_| crate::x11::windows::send_minimize_window(window_id));
    window_menu.append(&minimize);
    let maximize = MenuItem::with_label("Zoom (Super+F)");
    maximize.connect_activate(move |_| crate::x11::windows::send_toggle_maximize_window(window_id));
    window_menu.append(&maximize);
    window_menu.append(&SeparatorMenuItem::new());
    let close = MenuItem::with_label("Close Window (Super+Q)");
    close.connect_activate(move |_| crate::x11::windows::send_close_window(window_id));
    window_menu.append(&close);
    append_submenu(&bar, "Window", &window_menu);

    bar.show_all();
    bar
}

fn append_submenu(bar: &gtk::MenuBar, title: &str, menu: &Menu) {
    let item = MenuItem::with_label(title);
    menu.show_all();
    item.set_submenu(Some(menu));
    bar.append(&item);
}

/// An item that launches the first installed program from `candidates`, or a
/// disabled item explaining what is missing.
fn program_item(
    label: &str,
    candidates: &'static [(&'static str, &'static [&'static str])],
    missing: &str,
) -> MenuItem {
    let item = MenuItem::with_label(label);
    match process::first_available(candidates) {
        Some((path, args)) => {
            item.connect_activate(move |_| {
                if let Err(error) = process::spawn_detached(&path, args) {
                    show_message("Could not open the application", &error.to_string());
                }
            });
        }
        None => {
            item.set_sensitive(false);
            item.set_tooltip_text(Some(missing));
        }
    }
    item
}

/// An item for one of SLOPOS's own programs, disabled when it is not installed.
fn slopos_item(label: &str, program: &'static str, args: &'static [&'static str]) -> MenuItem {
    let item = MenuItem::with_label(label);
    if process::resolve_program(program).is_some() {
        item.connect_activate(move |_| spawn_resolved(program, args));
    } else {
        item.set_sensitive(false);
        item.set_tooltip_text(Some(&format!("{program} is not installed")));
    }
    item
}

/// Lock, Switch User, Sleep, Log Out (optional), Restart and Shut Down, with
/// every item enabled only when the system can actually do it. Returns the
/// Sleep item so suspend capability updates can reach it.
fn append_session_items(
    menu: &Menu,
    suspend_capability: Option<SuspendCapability>,
    include_logout: bool,
) -> MenuItem {
    let lock = MenuItem::with_label("Lock Screen");
    if session::can_lock_screen() {
        lock.connect_activate(|_| {
            if !session::lock_screen() {
                show_message(
                    "Could not lock the screen",
                    "The screen locker did not start.",
                );
            }
        });
    } else {
        lock.set_sensitive(false);
        lock.set_tooltip_text(Some("No supported screen locker is installed"));
    }
    menu.append(&lock);

    let switch_user = MenuItem::with_label("Switch User…");
    if session::can_switch_user() {
        switch_user.connect_activate(|_| {
            session::switch_user();
        });
    } else {
        switch_user.set_sensitive(false);
        switch_user.set_tooltip_text(Some("The display manager does not support switching users"));
    }
    menu.append(&switch_user);

    let sleep = MenuItem::with_label("Sleep");
    set_suspend_menu_state(&sleep, suspend_capability);
    sleep.connect_activate(|_| request_suspend());
    menu.append(&sleep);
    menu.append(&SeparatorMenuItem::new());

    if include_logout {
        let logout = MenuItem::with_label("Log Out…");
        logout.connect_activate(|_| {
            confirm_action(
                "Log Out",
                "Log out of SLOPOS-I now? Unsaved work in open applications may be lost.",
                || {
                    let Some(path) = process::resolve_program("slopos-session") else {
                        show_message("Could not log out", "slopos-session is not installed");
                        return;
                    };
                    match std::process::Command::new(path).arg("--logout").status() {
                        Ok(status) if status.success() => {}
                        Ok(_) => show_message(
                            "Could not log out",
                            "No SLOPOS session is running on this display.",
                        ),
                        Err(error) => show_message("Could not log out", &error.to_string()),
                    }
                },
            );
        });
        menu.append(&logout);
    }

    let restart = MenuItem::with_label("Restart…");
    if session::can_reboot() {
        restart.connect_activate(|_| {
            confirm_action("Restart", "Restart this computer now?", || {
                run_off_thread("Could not restart", session::reboot_system);
            });
        });
    } else {
        restart.set_sensitive(false);
        restart.set_tooltip_text(Some(
            "This system does not allow restarting from the desktop",
        ));
    }
    menu.append(&restart);

    let shutdown = MenuItem::with_label("Shut Down…");
    if session::can_poweroff() {
        shutdown.connect_activate(|_| {
            confirm_action("Shut Down", "Shut down this computer now?", || {
                run_off_thread("Could not shut down", session::poweroff_system);
            });
        });
    } else {
        shutdown.set_sensitive(false);
        shutdown.set_tooltip_text(Some(
            "This system does not allow shutting down from the desktop",
        ));
    }
    menu.append(&shutdown);
    sleep
}

fn about_text() -> String {
    format!(
        "SLOPOS-I {}\n\nAn experimental X11 desktop environment for Linux.\n\
         This is an alpha release: expect missing features and rough edges.\n\n\
         MIT License",
        env!("CARGO_PKG_VERSION")
    )
}

fn build_system_menu(suspend_capability: Option<SuspendCapability>) -> (Menu, MenuItem) {
    let menu = Menu::new();
    menu.style_context().add_class("slopos-system-menu");

    let about = MenuItem::with_label("About SLOPOS-I…");
    about.connect_activate(|_| show_message("About SLOPOS-I", &about_text()));
    menu.append(&about);
    menu.append(&SeparatorMenuItem::new());

    let settings = MenuItem::with_label("Control Panels…");
    if session::resolve_program(control_panels_program()).is_some() {
        settings.connect_activate(|_| spawn_resolved(control_panels_program(), &[]));
    } else {
        settings.set_sensitive(false);
    }
    menu.append(&settings);

    let catalogue = MenuItem::with_label("Software…");
    if session::resolve_program("slopos-catalogue").is_some() {
        catalogue.connect_activate(|_| spawn_resolved("slopos-catalogue", &[]));
    } else {
        catalogue.set_sensitive(false);
    }
    menu.append(&catalogue);

    let appearance = MenuItem::with_label("Appearance");
    let appearance_menu = Menu::new();
    let classic = MenuItem::with_label("Classic Contrast");
    let platinum = MenuItem::with_label("Platinum Light");
    let graphite = MenuItem::with_label("Graphite Dark");
    let oled = MenuItem::with_label("OLED Dark");
    if session::resolve_program("slopos-appearance").is_some() {
        classic.connect_activate(|_| spawn_resolved("slopos-appearance", &["classic"]));
        platinum.connect_activate(|_| spawn_resolved("slopos-appearance", &["platinum"]));
        graphite.connect_activate(|_| spawn_resolved("slopos-appearance", &["graphite"]));
        oled.connect_activate(|_| spawn_resolved("slopos-appearance", &["oled"]));
    } else {
        classic.set_sensitive(false);
        platinum.set_sensitive(false);
        graphite.set_sensitive(false);
        oled.set_sensitive(false);
    }
    appearance_menu.append(&classic);
    appearance_menu.append(&platinum);
    appearance_menu.append(&graphite);
    appearance_menu.append(&oled);
    appearance.set_submenu(Some(&appearance_menu));
    menu.append(&appearance);

    let wallpaper = MenuItem::with_label("Desktop Wallpaper…");
    if session::resolve_program("slopos-settings").is_some() {
        wallpaper.connect_activate(|_| spawn_resolved("slopos-settings", &["--wallpaper"]));
    } else {
        wallpaper.set_sensitive(false);
    }
    menu.append(&wallpaper);

    menu.append(&SeparatorMenuItem::new());

    let sleep = append_session_items(&menu, suspend_capability, true);
    menu.show_all();
    (menu, sleep)
}

fn show_message(title: &str, message: &str) {
    let dialog = slopos_dialog(title, message, &[("Close", ResponseType::Close)]);
    dialog.connect_response(|dialog, _| dialog.close());
    dialog.show_all();
}

/// Run a blocking or interactive action away from the GTK thread and report a
/// failure on the main loop. The action may wait for a polkit dialog, empty a
/// large Trash or run several fallback commands.
fn run_off_thread<F>(failure: &'static str, action: F)
where
    F: FnOnce() -> Result<(), String> + Send + 'static,
{
    let spawned = std::thread::Builder::new()
        .name("slopos-menu-action".to_owned())
        .spawn(move || {
            let result = action();
            glib::MainContext::default().invoke(move || {
                if let Err(error) = result {
                    show_message(failure, &error);
                }
            });
        });
    if let Err(error) = spawned {
        show_message(
            failure,
            &format!("could not start the background request: {error}"),
        );
    }
}

fn request_suspend() {
    if let Err(error) = crate::services::power::request_suspend(|result| {
        if let Err(error) = result {
            show_message("Unable to suspend", &error);
        }
    }) {
        show_message("Unable to suspend", &error.to_string());
    }
}

fn set_suspend_menu_state(item: &MenuItem, capability: Option<SuspendCapability>) {
    let (enabled, tooltip) = suspend_menu_presentation(capability);
    item.set_sensitive(enabled);
    item.set_tooltip_text(Some(tooltip));
}

fn suspend_menu_presentation(capability: Option<SuspendCapability>) -> (bool, &'static str) {
    match capability {
        Some(SuspendCapability::Available) => (true, "Suspend the computer"),
        Some(SuspendCapability::RequiresAuthorization) => {
            (true, "Authentication may be required to suspend")
        }
        Some(SuspendCapability::Unavailable) => (false, "System suspend is unavailable"),
        Some(SuspendCapability::Unknown) | None => (false, "Checking suspend capability"),
    }
}

fn confirm_action<F>(title: &str, message: &str, action: F)
where
    F: Fn() + 'static,
{
    let dialog = slopos_dialog(
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

fn slopos_dialog(title: &str, message: &str, buttons: &[(&str, ResponseType)]) -> Dialog {
    let button_specs = buttons.to_vec();
    let dialog = Dialog::with_buttons(
        Some(title),
        None::<&Window>,
        DialogFlags::MODAL,
        &button_specs,
    );
    dialog.set_default_size(360, 150);
    dialog.set_resizable(false);
    dialog.content_area().set_margin_top(12);
    dialog.content_area().set_margin_bottom(12);
    dialog.content_area().set_margin_start(12);
    dialog.content_area().set_margin_end(12);
    dialog.connect_key_press_event(|dialog, event| {
        if event.keyval() == gdk::keys::constants::Escape {
            dialog.close();
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });

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

/// The native Control Panels when installed, else the legacy GTK settings.
fn control_panels_program() -> &'static str {
    if process::resolve_program("slopos-control-panels").is_some() {
        "slopos-control-panels"
    } else {
        "slopos-settings"
    }
}

/// Open one panel (`--sound`, `--network`, `--datetime`); both programs
/// accept these flags.
fn open_control_panel(flag: &str) {
    spawn_resolved(control_panels_program(), &[flag]);
}

fn spawn_resolved(program: &str, args: &[&str]) {
    if let Err(error) = process::launch(program, args) {
        log::warn!("{error}");
        show_message("Could not open the application", &error);
    }
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
            Err(_) => None,
        }
    })
}

fn build_sound_menu() -> Menu {
    let menu = Menu::new();
    let header = MenuItem::with_label("Sound & Volume");
    header.set_sensitive(false);
    menu.append(&header);

    let vol_up = MenuItem::with_label("Volume Up (+5%)");
    vol_up.connect_activate(|_| audio::adjust_default_output_volume(5));
    menu.append(&vol_up);

    let vol_down = MenuItem::with_label("Volume Down (-5%)");
    vol_down.connect_activate(|_| audio::adjust_default_output_volume(-5));
    menu.append(&vol_down);

    let mute_item = MenuItem::with_label("Toggle Output Mute");
    mute_item.connect_activate(|_| audio::toggle_default_output_mute());
    menu.append(&mute_item);

    menu.append(&SeparatorMenuItem::new());

    let settings_item = MenuItem::with_label("Sound Preferences…");
    settings_item.connect_activate(|_| {
        if process::resolve_program(control_panels_program()).is_some() {
            open_control_panel("--sound");
        } else {
            spawn_resolved("pavucontrol", &[]);
        }
    });
    menu.append(&settings_item);

    menu.show_all();
    menu
}

/// Rebuild the network menu from the latest provider summary.
fn fill_network_menu(menu: &Menu, summary: &crate::services::network::NetworkSummary) {
    for child in menu.children() {
        menu.remove(&child);
    }
    let header = MenuItem::with_label("Network");
    header.set_sensitive(false);
    menu.append(&header);
    for line in &summary.details {
        let item = MenuItem::with_label(line);
        item.set_sensitive(false);
        menu.append(&item);
    }
    menu.append(&SeparatorMenuItem::new());

    let settings_item = MenuItem::with_label("Network Preferences…");
    if process::resolve_program(control_panels_program()).is_some() {
        settings_item.connect_activate(|_| open_control_panel("--network"));
    } else if process::resolve_program("nm-connection-editor").is_some() {
        settings_item.connect_activate(|_| spawn_resolved("nm-connection-editor", &[]));
    } else {
        settings_item.set_sensitive(false);
    }
    menu.append(&settings_item);
    menu.show_all();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compact_title_em_dash() {
        assert_eq!(
            compact_title("Example Domain — Mozilla Firefox"),
            "Mozilla Firefox"
        );
        assert_eq!(
            compact_title("Inbox (3) — Mozilla Thunderbird"),
            "Mozilla Thunderbird"
        );
    }

    #[test]
    fn test_compact_title_regular_dash() {
        assert_eq!(compact_title("Untitled Document 1 - Mousepad"), "Mousepad");
    }

    #[test]
    fn test_compact_title_fallback() {
        assert_eq!(compact_title(""), "SLOPOS Desktop");
        assert_eq!(compact_title("Calculator"), "Calculator");
    }

    #[test]
    fn suspend_menu_is_enabled_only_for_actionable_logind_capabilities() {
        assert!(suspend_menu_presentation(Some(SuspendCapability::Available)).0);
        assert!(suspend_menu_presentation(Some(SuspendCapability::RequiresAuthorization)).0);
        assert!(!suspend_menu_presentation(Some(SuspendCapability::Unavailable)).0);
        assert!(!suspend_menu_presentation(Some(SuspendCapability::Unknown)).0);
        assert!(!suspend_menu_presentation(None).0);
    }
}
