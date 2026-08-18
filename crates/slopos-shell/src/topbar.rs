//! SLOPOS-I top menu and system bar.
//!
//! Event-driven status, global menu hosting, and desktop integration
//! powered by x11rb and system service adapters without subprocess polling.

use crate::launcher::Launcher;
use crate::menu::gmenu::{self, GtkMenuExporter};
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
use std::cell::{Cell, RefCell};
use std::env;
use std::path::Path;
use std::process::Command;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static OPEN_SYSTEM_MENU: AtomicBool = AtomicBool::new(false);

pub struct TopBar {
    window: Window,
    active_title_label: Label,
    _clock_label: Label,
    audio_label: Label,
    network_label: Label,
    battery_box: GtkBox,
    battery_label: Label,
    global_menu_host: GtkBox,
    active_menu_state: RefCell<Option<GtkMenuExporter>>,
    current_active_window: Cell<Option<u32>>,
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
        let audio_label = Label::new(Some("Vol: --"));
        audio_box.pack_start(&audio_label, false, false, 0);
        audio_button.add(&audio_box);
        if session::resolve_program("pavucontrol").is_some() {
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
        let network_label = Label::new(Some("Net: --"));
        network_box.pack_start(&network_label, false, false, 0);
        network_button.add(&network_box);
        if session::resolve_program("nm-connection-editor").is_some() {
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
        clock_button.connect_clicked(|_| spawn_resolved("slopos-settings", &["--datetime"]));
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
            battery_box,
            battery_label,
            global_menu_host,
            active_menu_state: RefCell::new(None),
            current_active_window: Cell::new(None),
        });

        // In-process local clock ticker (every 1 second, zero I/O or subprocesses)
        glib::timeout_add_seconds_local(1, move || {
            clock_label.set_text(&clock::current_time_str());
            glib::ControlFlow::Continue
        });

        topbar
    }

    pub fn update_system_status(&self, status: &SystemStatus) {
        self.audio_label.set_text(&status.audio_text);
        self.network_label.set_text(&status.network_text);
        if let Some(battery_text) = &status.battery_text {
            self.battery_label.set_text(battery_text);
            self.battery_box.set_visible(true);
        } else {
            self.battery_label.set_text("");
            self.battery_box.set_visible(false);
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
                    self.refresh_global_menu(exporter);
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
        self.refresh_global_menu(None);
    }

    fn set_fullscreen_visibility(&self, is_fullscreen: bool) {
        if is_fullscreen && self.window.is_visible() {
            self.window.set_visible(false);
        } else if !is_fullscreen && !self.window.is_visible() {
            self.window.set_visible(true);
        }
    }

    fn reposition_for_monitors(&self, model: &MonitorModel) {
        if let Some(primary) = model.primary() {
            self.window.resize(primary.gdk_width(), 26);
            self.window.move_(primary.gdk_x(), primary.gdk_y());
        }
    }

    fn refresh_global_menu(&self, next: Option<GtkMenuExporter>) {
        if *self.active_menu_state.borrow() == next {
            return;
        }

        for child in self.global_menu_host.children() {
            self.global_menu_host.remove(&child);
        }
        self.global_menu_host.hide();
        *self.active_menu_state.borrow_mut() = None;

        let Some(exporter) = next else {
            return;
        };
        if self.current_active_window.get() != Some(exporter.window_id) {
            return;
        }

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
            }
            Err(error) => {
                log::warn!("Could not import focused application's GTK menu: {error}");
            }
        }
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

fn is_shell_surface(title: &str) -> bool {
    matches!(
        title.trim(),
        "SLOPOS Top Bar" | "SLOPOS Application Strip" | "SLOPOS Search" | "SLOPOS Notification"
    )
}

fn compact_title(title: &str) -> String {
    let t = title.trim();
    if t.is_empty() {
        return "SLOPOS Desktop".to_string();
    }
    if let Some(pos) = t.rfind(" — ") {
        let app = t[pos + 3..].trim();
        if !app.is_empty() {
            return app.to_string();
        }
    }
    if let Some(pos) = t.rfind(" - ") {
        let app = t[pos + 3..].trim();
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

fn build_system_menu() -> Menu {
    let menu = Menu::new();
    menu.style_context().add_class("slopos-system-menu");

    let about = MenuItem::with_label("About SLOPOS-I…");
    about.connect_activate(|_| {
        show_message(
            "About SLOPOS-I",
            "SLOPOS-I Desktop Environment\n\nRelease: 0.1.0-alpha\nPlatform: X11 / Linux\nLicense: MIT\n\nAn original, consumer-ready Linux desktop.",
        );
    });
    menu.append(&about);
    menu.append(&SeparatorMenuItem::new());

    let settings = MenuItem::with_label("Control Panels…");
    if session::resolve_program("slopos-settings").is_some() {
        settings.connect_activate(|_| spawn_resolved("slopos-settings", &[]));
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

    let lock = MenuItem::with_label("Lock Screen");
    if session::can_lock_screen() {
        lock.connect_activate(|_| {
            session::lock_screen();
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
        switch_user.set_tooltip_text(Some(
            "No supported display-manager switch utility is installed",
        ));
    }
    menu.append(&switch_user);

    let sleep = MenuItem::with_label("Sleep");
    if session::can_suspend() {
        sleep.connect_activate(|_| {
            session::suspend_system();
        });
    } else {
        sleep.set_sensitive(false);
        sleep.set_tooltip_text(Some("Suspend is not supported on this host"));
    }
    menu.append(&sleep);

    let logout = MenuItem::with_label("Log Out…");
    logout.connect_activate(|_| {
        confirm_action("Log Out", "Log out of SLOPOS-I now?", || {
            if let Some(session_ctl) = session::resolve_program("slopos-session") {
                if let Err(error) = Command::new(&session_ctl).arg("--logout").spawn() {
                    log::warn!("Failed to invoke session logout: {error}");
                    std::process::exit(0);
                }
            } else {
                std::process::exit(0);
            }
        });
    });
    menu.append(&logout);

    let restart = MenuItem::with_label("Restart…");
    if session::can_reboot() {
        restart.connect_activate(|_| {
            confirm_action("Restart", "Restart this computer now?", || {
                session::reboot_system();
            });
        });
    } else {
        restart.set_sensitive(false);
    }
    menu.append(&restart);

    let shutdown = MenuItem::with_label("Shut Down…");
    if session::can_poweroff() {
        shutdown.connect_activate(|_| {
            confirm_action("Shut Down", "Shut down this computer now?", || {
                session::poweroff_system();
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
    let dialog = slopos_dialog(title, message, &[("Close", ResponseType::Close)]);
    dialog.connect_response(|dialog, _| dialog.close());
    dialog.show_all();
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

fn spawn_resolved(program: &str, args: &[&str]) {
    let Some(path) = session::resolve_program(program) else {
        log::warn!("Cannot launch {program}: command not found");
        return;
    };
    if let Err(error) = Command::new(&path).args(args).spawn() {
        log::warn!("Failed to launch {}: {error}", path.display());
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
