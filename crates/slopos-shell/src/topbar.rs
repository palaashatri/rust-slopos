//! SLOPOS-I classic top menu/system bar.

use crate::appmenu::{self, AppMenuStatus};
use crate::launcher::Launcher;
use gdk_pixbuf::{InterpType, Pixbuf};
use gtk::atk::prelude::AtkObjectExt;
use gtk::prelude::*;
use gtk::{
    Align, Box as GtkBox, Button, Dialog, DialogFlags, IconSize, Image, Label, Menu, MenuItem,
    Orientation, ResponseType, SeparatorMenuItem, Window, WindowPosition, WindowType,
};
use std::cell::{Cell, RefCell};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::sync::mpsc::{self, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

const LOCK_COMMANDS: &[(&str, &[&str])] = &[
    ("xdg-screensaver", &["lock"]),
    ("light-locker-command", &["-l"]),
    ("dm-tool", &["lock"]),
];

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
        window.set_keep_above(true);
        window.set_skip_taskbar_hint(true);
        window.set_skip_pager_hint(true);
        set_accessible_name(&window, "SLOPOS top bar");
        window.style_context().add_class("slopos-topbar");

        let app_menu_status = Rc::new(Cell::new(AppMenuStatus::ShellOwned));
        let main_box = GtkBox::new(Orientation::Horizontal, 0);
        // Paint the bar on the child that owns the full allocation as well as
        // on the borderless window. GTK themes do not consistently paint a
        // GtkWindow background under Xvfb/Openbox, which otherwise leaves a
        // black/transparent strip behind the menu widgets.
        main_box.style_context().add_class("slopos-topbar");
        main_box.set_hexpand(true);
        main_box.set_vexpand(true);
        main_box.set_margin_start(5);
        main_box.set_margin_end(8);

        let system_button = Button::new();
        system_button.style_context().add_class("slopos-logo-btn");
        system_button.set_tooltip_text(Some("SLOPOS menu"));
        set_accessible_name(&system_button, "SLOPOS menu");
        if let Some(mark) = load_slopos_mark() {
            system_button.set_image(Some(&mark));
            system_button.set_always_show_image(true);
        } else {
            // Keep a recognizable, text-only fallback if the optional packaged
            // mark is unavailable during development or recovery.
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
        main_box.pack_start(&system_button, false, false, 0);

        let active_title_label = Label::new(Some("SLOPOS Desktop"));
        active_title_label
            .style_context()
            .add_class("slopos-active-app");
        active_title_label.set_halign(Align::Start);
        active_title_label.set_ellipsize(pango::EllipsizeMode::End);
        active_title_label.set_max_width_chars(28);
        set_accessible_name(&active_title_label, "Active application");
        main_box.pack_start(&active_title_label, false, false, 7);
        let app_menu_button = build_app_menu_button();
        let app_menu_exporter = Rc::new(RefCell::new(None));
        let app_menu_exporter_ref = app_menu_exporter.clone();
        app_menu_button.connect_clicked(move |button| {
            open_imported_app_menu(button, app_menu_exporter_ref.borrow().clone());
        });
        main_box.pack_start(&app_menu_button, false, false, 0);

        let status_box = GtkBox::new(Orientation::Horizontal, 6);
        status_box.style_context().add_class("slopos-status-area");

        let search_button = Button::new();
        search_button
            .style_context()
            .add_class("slopos-menubar-control");
        search_button.set_tooltip_text(Some("Search applications (Super+Space)"));
        set_accessible_name(&search_button, "Search applications (Super+Space)");
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
        let audio_label = Label::new(Some("--"));
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
        let network_label = Label::new(Some("--"));
        network_box.pack_start(&network_label, false, false, 0);
        network_button.add(&network_box);
        if resolve_program("nm-connection-editor").is_some() {
            network_button.set_tooltip_text(Some("Open network connections"));
            network_button.connect_clicked(|_| spawn_resolved("nm-connection-editor", &[]));
        } else {
            network_button.set_sensitive(false);
            network_button.set_tooltip_text(Some("Network status"));
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

        let clock_label = Label::new(Some("--:--"));
        clock_label.style_context().add_class("slopos-clock");
        clock_label.set_tooltip_text(Some("Local time"));
        set_accessible_name(&clock_label, "Local time");
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
            &app_menu_button,
            app_menu_status,
            app_menu_exporter,
        );

        Rc::new(Self {
            _window: window,
            _active_title_label: active_title_label,
            _clock_label: clock_label,
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn install_live_updates(
    active_title: &Label,
    clock: &Label,
    audio: &Label,
    network: &Label,
    battery: &Label,
    app_menu_button: &Button,
    app_menu_status: Rc<Cell<AppMenuStatus>>,
    app_menu_exporter: Rc<RefCell<Option<appmenu::AppMenuExporter>>>,
) {
    let active_title = active_title.clone();
    let app_menu_button = app_menu_button.clone();
    glib::timeout_add_local(Duration::from_millis(500), move || {
        update_active_window(
            &active_title,
            &app_menu_button,
            &app_menu_status,
            &app_menu_exporter,
        );
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
    let battery = battery.clone();
    glib::timeout_add_seconds_local(5, move || {
        audio.set_text(&current_volume().unwrap_or_else(|| "--".to_string()));
        network.set_text(&current_network_state());
        battery.set_text(&current_battery_state().unwrap_or_default());
        glib::ControlFlow::Continue
    });
}

fn update_active_window(
    label: &Label,
    app_menu_button: &Button,
    app_menu_status: &Cell<AppMenuStatus>,
    app_menu_exporter: &RefCell<Option<appmenu::AppMenuExporter>>,
) {
    let Some(id_text) = command_output("xdotool", &["getactivewindow"]) else {
        label.set_text("SLOPOS Desktop");
        update_app_menu_status(
            app_menu_button,
            app_menu_status,
            app_menu_exporter,
            AppMenuStatus::ShellOwned,
            None,
        );
        return;
    };
    let Ok(id) = id_text.trim().parse::<u64>() else {
        label.set_text("SLOPOS Desktop");
        update_app_menu_status(
            app_menu_button,
            app_menu_status,
            app_menu_exporter,
            AppMenuStatus::ShellOwned,
            None,
        );
        return;
    };
    let Some(title) = command_output("xdotool", &["getwindowname", &id.to_string()]) else {
        label.set_text("SLOPOS Desktop");
        update_app_menu_status(
            app_menu_button,
            app_menu_status,
            app_menu_exporter,
            AppMenuStatus::ShellOwned,
            None,
        );
        return;
    };

    if is_shell_surface(&title) {
        label.set_text("SLOPOS Desktop");
        update_app_menu_status(
            app_menu_button,
            app_menu_status,
            app_menu_exporter,
            AppMenuStatus::ShellOwned,
            None,
        );
        return;
    }
    if title.is_empty() {
        label.set_text("SLOPOS Desktop");
    } else {
        label.set_text(&compact_title(&title));
    }
    let Some(window_id) = u32::try_from(id).ok() else {
        update_app_menu_status(
            app_menu_button,
            app_menu_status,
            app_menu_exporter,
            AppMenuStatus::NoExporter,
            None,
        );
        return;
    };
    let (status, exporter) = appmenu::status_for_window(window_id);
    update_app_menu_status(
        app_menu_button,
        app_menu_status,
        app_menu_exporter,
        status,
        exporter.as_ref(),
    );
}

fn update_app_menu_status(
    button: &Button,
    previous: &Cell<AppMenuStatus>,
    exporter_state: &RefCell<Option<appmenu::AppMenuExporter>>,
    status: AppMenuStatus,
    exporter: Option<&appmenu::AppMenuExporter>,
) {
    *exporter_state.borrow_mut() = exporter.cloned();
    if previous.get() != status {
        match (status, exporter) {
            (AppMenuStatus::ExporterDetected, Some(exporter)) => log::info!(
                "Focused X11 application exports AppMenu bus={} path={}; bounded DBusMenu importer enabled",
                exporter.bus_name,
                exporter.object_path
            ),
            (AppMenuStatus::NoExporter, _) => {
                log::info!("Focused X11 application exports no AppMenu; using its local menu")
            }
            (AppMenuStatus::ShellOwned, _) => {
                log::info!("SLOPOS shell owns the global menu area")
            }
            (AppMenuStatus::ExporterDetected, None) => log::warn!(
                "Focused X11 application advertises AppMenu but exporter details were unavailable"
            ),
        }
        previous.set(status);
    }

    button.set_sensitive(false);
    match status {
        AppMenuStatus::ShellOwned => {
            button.set_label("App");
            button.set_tooltip_text(Some("SLOPOS owns this menu area"));
            set_accessible_name(button, "Application menu unavailable for SLOPOS shell");
        }
        AppMenuStatus::NoExporter => {
            button.set_label("App");
            button.set_tooltip_text(Some(
                "This application exports no X11 AppMenu; use its local menu",
            ));
            set_accessible_name(
                button,
                "Application has no exported menu; use its local menu",
            );
        }
        AppMenuStatus::ExporterDetected => {
            button.set_label("App");
            button.set_tooltip_text(Some("Open the focused application's exported AppMenu"));
            button.set_sensitive(true);
            set_accessible_name(button, "Open exported application menu");
        }
    }
}

fn is_shell_surface(title: &str) -> bool {
    matches!(
        title.trim(),
        "SLOPOS Top Bar" | "SLOPOS Application Strip" | "SLOPOS Search" | "SLOPOS Notification"
    )
}

fn compact_title(title: &str) -> String {
    let title = title.trim();
    if title.chars().count() <= 28 {
        return title.to_string();
    }
    let mut value = title.chars().take(27).collect::<String>();
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

fn build_app_menu_button() -> Button {
    let button = Button::with_label("App");
    button.style_context().add_class("slopos-menubar-control");
    button.set_sensitive(false);
    button.set_tooltip_text(Some(
        "Application menus stay local unless a safe X11 AppMenu importer is available",
    ));
    set_accessible_name(
        &button,
        "Application menu unavailable; use the local application menu",
    );
    button
}

fn open_imported_app_menu(button: &Button, exporter: Option<appmenu::AppMenuExporter>) {
    let Some(exporter) = exporter else {
        return;
    };

    button.set_sensitive(false);
    let (sender, receiver) = mpsc::channel();
    let worker_exporter = exporter.clone();
    let deadline = Instant::now() + Duration::from_millis(900);
    thread::spawn(move || {
        let result =
            appmenu::fetch_layout_with_timeout(&worker_exporter, Duration::from_millis(750));
        let _ = sender.send(result);
    });

    let button = button.clone();
    glib::timeout_add_local(Duration::from_millis(25), move || {
        if Instant::now() >= deadline {
            log::warn!("Focused application's AppMenu import exceeded its UI deadline");
            button.set_sensitive(true);
            button.set_tooltip_text(Some(
                "The focused application's AppMenu timed out; use its local menu",
            ));
            return glib::ControlFlow::Break;
        }
        match receiver.try_recv() {
            Ok(Ok(layout)) => {
                button.set_sensitive(true);
                show_imported_app_menu(&button, layout, exporter.clone());
                glib::ControlFlow::Break
            }
            Ok(Err(error)) => {
                log::warn!("Focused application's AppMenu was not imported: {error}");
                button.set_sensitive(true);
                button.set_tooltip_text(Some(
                    "The focused application's AppMenu is unavailable; use its local menu",
                ));
                glib::ControlFlow::Break
            }
            Err(TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(TryRecvError::Disconnected) => {
                button.set_sensitive(true);
                glib::ControlFlow::Break
            }
        }
    });
}

fn show_imported_app_menu(
    button: &Button,
    layout: appmenu::AppMenuLayout,
    exporter: appmenu::AppMenuExporter,
) {
    let menu = Menu::new();
    append_imported_menu_items(&menu, &layout.items, &exporter);
    if menu.children().is_empty() {
        log::warn!("Focused application's AppMenu exported no visible items");
        return;
    }
    menu.show_all();
    menu.popup_at_widget(
        button,
        gdk::Gravity::SouthWest,
        gdk::Gravity::NorthWest,
        None,
    );
}

fn append_imported_menu_items(
    menu: &Menu,
    items: &[appmenu::AppMenuItem],
    exporter: &appmenu::AppMenuExporter,
) {
    for item in items {
        if !item.visible {
            continue;
        }
        if item.kind == appmenu::AppMenuItemKind::Separator {
            menu.append(&SeparatorMenuItem::new());
            continue;
        }

        let menu_item = MenuItem::with_label(&item.label);
        menu_item.set_sensitive(item.enabled);
        if !item.children.is_empty() {
            let submenu = Menu::new();
            append_imported_menu_items(&submenu, &item.children, exporter);
            menu_item.set_submenu(Some(&submenu));
        } else if item.kind == appmenu::AppMenuItemKind::Standard {
            let exporter = exporter.clone();
            let item_id = item.id;
            menu_item.connect_activate(move |_| {
                let exporter = exporter.clone();
                thread::spawn(move || {
                    if let Err(error) =
                        appmenu::activate(&exporter, item_id, Duration::from_millis(750))
                    {
                        log::warn!("Focused application's AppMenu action failed: {error}");
                    }
                });
            });
        }
        menu.append(&menu_item);
    }
}

fn build_system_menu() -> Menu {
    let menu = Menu::new();

    let about = MenuItem::with_label("About SLOPOS-I");
    about.connect_activate(|_| show_message("About SLOPOS-I", "SLOPOS-I\nX11 Platinum Desktop"));
    menu.append(&about);
    menu.append(&SeparatorMenuItem::new());

    let settings = MenuItem::with_label("System Settings…");
    if resolve_program("slopos-settings").is_some() {
        settings.connect_activate(|_| spawn_resolved("slopos-settings", &[]));
    } else {
        settings.set_sensitive(false);
    }
    menu.append(&settings);

    let catalogue = MenuItem::with_label("Software Catalogue…");
    if resolve_program("slopos-catalogue").is_some() {
        catalogue.connect_activate(|_| spawn_resolved("slopos-catalogue", &[]));
    } else {
        catalogue.set_sensitive(false);
    }
    menu.append(&catalogue);
    menu.append(&SeparatorMenuItem::new());

    let lock = MenuItem::with_label("Lock Screen");
    if let Some((program, args)) = lock_command() {
        lock.connect_activate(move |_| spawn_resolved(program, args));
    } else {
        lock.set_sensitive(false);
    }
    menu.append(&lock);

    let logout = MenuItem::with_label("Log Out…");
    if env::var_os("SLOPOS_SESSION_MANAGED").is_some() {
        logout.connect_activate(|_| {
            confirm_action("Log Out", "End the current SLOPOS session?", || unsafe {
                libc::kill(libc::getppid(), libc::SIGTERM);
            });
        });
    } else {
        logout.set_sensitive(false);
    }
    menu.append(&logout);

    let restart = MenuItem::with_label("Restart…");
    if resolve_program("systemctl").is_some() {
        restart.connect_activate(|_| {
            confirm_action("Restart", "Restart this computer now?", || {
                spawn_resolved("systemctl", &["reboot"])
            });
        });
    } else {
        restart.set_sensitive(false);
    }
    menu.append(&restart);

    let shutdown = MenuItem::with_label("Shut Down…");
    if resolve_program("systemctl").is_some() {
        shutdown.connect_activate(|_| {
            confirm_action("Shut Down", "Shut down this computer now?", || {
                spawn_resolved("systemctl", &["poweroff"])
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

fn lock_command() -> Option<(&'static str, &'static [&'static str])> {
    for &(program, args) in LOCK_COMMANDS {
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

/// Keep image-led top-bar controls discoverable to ATK clients even when the
/// host GTK theme does not expose their tooltip text as the accessible name.
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
                // The shipped identity image is a square poster. Use its
                // central S mark for the 20px menu button instead of shrinking
                // the entire poster into an unreadable thumbnail. Smaller
                // replacement assets are scaled as-is.
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
