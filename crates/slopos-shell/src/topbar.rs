//! SLOPOS-I classic top menu/system bar.

use crate::launcher::Launcher;
use gdk_pixbuf::{InterpType, Pixbuf};
use gtk::atk::prelude::AtkObjectExt;
use gtk::prelude::*;
use gtk::{
    Align, Box as GtkBox, Button, ButtonsType, DialogFlags, IconSize, Image, Label, Menu, MenuBar,
    MenuItem, MessageDialog, MessageType, Orientation, ResponseType, SeparatorMenuItem, Window,
    WindowPosition, WindowType,
};
use std::cell::{Cell, RefCell};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::time::Duration;

type TargetMenuControls = Rc<RefCell<Vec<(MenuItem, &'static str)>>>;

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

        let target_window = Rc::new(Cell::new(0_u64));
        let target_menu_controls: TargetMenuControls = Rc::new(RefCell::new(Vec::new()));
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
        main_box.pack_start(
            &build_global_menu_bar(target_window.clone(), target_menu_controls.clone()),
            false,
            false,
            0,
        );

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
            target_window,
            target_menu_controls,
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
    target_window: Rc<Cell<u64>>,
    target_menu_controls: TargetMenuControls,
    active_title: &Label,
    clock: &Label,
    audio: &Label,
    network: &Label,
    battery: &Label,
) {
    let title_target = target_window.clone();
    let target_menu_controls = target_menu_controls.clone();
    let active_title = active_title.clone();
    glib::timeout_add_local(Duration::from_millis(500), move || {
        update_active_window(&title_target, &active_title, &target_menu_controls);
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
    target_window: &Cell<u64>,
    label: &Label,
    target_menu_controls: &TargetMenuControls,
) {
    let Some(id_text) = command_output("xdotool", &["getactivewindow"]) else {
        target_window.set(0);
        update_target_menu_controls(target_window, target_menu_controls);
        label.set_text("SLOPOS Desktop");
        return;
    };
    let Ok(id) = id_text.trim().parse::<u64>() else {
        target_window.set(0);
        update_target_menu_controls(target_window, target_menu_controls);
        label.set_text("SLOPOS Desktop");
        return;
    };
    let Some(title) = command_output("xdotool", &["getwindowname", &id.to_string()]) else {
        target_window.set(0);
        update_target_menu_controls(target_window, target_menu_controls);
        label.set_text("SLOPOS Desktop");
        return;
    };

    if is_shell_surface(&title) {
        target_window.set(0);
        update_target_menu_controls(target_window, target_menu_controls);
        label.set_text("SLOPOS Desktop");
        return;
    }
    if title.is_empty() {
        target_window.set(0);
        label.set_text("SLOPOS Desktop");
    } else {
        target_window.set(id);
        label.set_text(&compact_title(&title));
    }
    update_target_menu_controls(target_window, target_menu_controls);
}

fn update_target_menu_controls(
    target_window: &Cell<u64>,
    target_menu_controls: &TargetMenuControls,
) {
    let has_target = target_window.get() != 0;
    for (item, required_command) in target_menu_controls.borrow().iter() {
        item.set_sensitive(has_target && resolve_program(required_command).is_some());
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
            return (width, height);
        }
    }
    (1280, 800)
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

fn build_global_menu_bar(
    target_window: Rc<Cell<u64>>,
    target_menu_controls: TargetMenuControls,
) -> MenuBar {
    let menu_bar = MenuBar::new();
    menu_bar.style_context().add_class("slopos-menu-bar");

    let file_item = MenuItem::with_label("File");
    let file_menu = Menu::new();
    file_menu.append(&command_item("New File Window", || {
        spawn_first_or_message(&["pcmanfm", "thunar"], &[])
    }));
    file_menu.append(&command_item("Home Folder", || {
        spawn_first_or_message(&["pcmanfm", "thunar"], &[])
    }));
    file_menu.append(&SeparatorMenuItem::new());
    let close_target = target_window.clone();
    let close_item = command_item("Close Window", move || {
        if !target_xdotool(&close_target, "windowclose") {
            show_target_unavailable();
        }
    });
    register_target_menu_control(&target_menu_controls, &close_item, "xdotool");
    file_menu.append(&close_item);
    file_item.set_submenu(Some(&file_menu));
    menu_bar.append(&file_item);

    let edit_item = MenuItem::with_label("Edit");
    let edit_menu = Menu::new();
    edit_menu.append(&target_shortcut_item(
        "Undo",
        "ctrl+z",
        target_window.clone(),
        target_menu_controls.clone(),
    ));
    edit_menu.append(&SeparatorMenuItem::new());
    edit_menu.append(&target_shortcut_item(
        "Cut",
        "ctrl+x",
        target_window.clone(),
        target_menu_controls.clone(),
    ));
    edit_menu.append(&target_shortcut_item(
        "Copy",
        "ctrl+c",
        target_window.clone(),
        target_menu_controls.clone(),
    ));
    edit_menu.append(&target_shortcut_item(
        "Paste",
        "ctrl+v",
        target_window.clone(),
        target_menu_controls.clone(),
    ));
    edit_menu.append(&target_shortcut_item(
        "Select All",
        "ctrl+a",
        target_window.clone(),
        target_menu_controls.clone(),
    ));
    edit_item.set_submenu(Some(&edit_menu));
    menu_bar.append(&edit_item);

    let view_item = MenuItem::with_label("View");
    let view_menu = Menu::new();
    view_menu.append(&target_shortcut_item(
        "Refresh",
        "F5",
        target_window.clone(),
        target_menu_controls.clone(),
    ));
    view_menu.append(&SeparatorMenuItem::new());
    view_menu.append(&target_shortcut_item(
        "Zoom In",
        "ctrl+plus",
        target_window.clone(),
        target_menu_controls.clone(),
    ));
    view_menu.append(&target_shortcut_item(
        "Zoom Out",
        "ctrl+minus",
        target_window.clone(),
        target_menu_controls.clone(),
    ));
    view_item.set_submenu(Some(&view_menu));
    menu_bar.append(&view_item);

    let window_item = MenuItem::with_label("Window");
    let window_menu = Menu::new();
    let minimize_target = target_window.clone();
    let minimize_item = command_item("Minimize", move || {
        if !target_xdotool(&minimize_target, "windowminimize") {
            show_target_unavailable();
        }
    });
    register_target_menu_control(&target_menu_controls, &minimize_item, "xdotool");
    window_menu.append(&minimize_item);
    let maximize_target = target_window.clone();
    let maximize_item = command_item("Zoom / Maximize", move || {
        if !target_maximize(&maximize_target) {
            show_target_unavailable();
        }
    });
    register_target_menu_control(&target_menu_controls, &maximize_item, "wmctrl");
    window_menu.append(&maximize_item);
    window_menu.append(&command_item("Next Window", || {
        spawn_resolved("xdotool", &["key", "alt+Tab"])
    }));
    window_item.set_submenu(Some(&window_menu));
    menu_bar.append(&window_item);

    let help_item = MenuItem::with_label("Help");
    let help_menu = Menu::new();
    help_menu.append(&command_item("Keyboard Shortcuts", || {
        show_message(
            "SLOPOS-I Keyboard Shortcuts",
            "Super+Space  Search\nSuper+Left/Right  Switch desktop\nSuper+Q  Close window\nSuper+M  Minimize\nSuper+F  Zoom / maximize",
        )
    }));
    help_item.set_submenu(Some(&help_menu));
    menu_bar.append(&help_item);

    menu_bar.show_all();
    menu_bar
}

fn target_shortcut_item(
    label: &str,
    shortcut: &'static str,
    target: Rc<Cell<u64>>,
    target_menu_controls: TargetMenuControls,
) -> MenuItem {
    let item = command_item(label, move || {
        if !target_shortcut(&target, shortcut) {
            show_target_unavailable();
        }
    });
    register_target_menu_control(&target_menu_controls, &item, "xdotool");
    item
}

fn register_target_menu_control(
    target_menu_controls: &TargetMenuControls,
    item: &MenuItem,
    required_command: &'static str,
) {
    item.set_sensitive(false);
    target_menu_controls
        .borrow_mut()
        .push((item.clone(), required_command));
}

fn target_shortcut(target: &Cell<u64>, shortcut: &str) -> bool {
    let id = target.get();
    if id == 0 {
        return false;
    }
    let id = id.to_string();
    Command::new("xdotool")
        .args([
            "windowactivate",
            "--sync",
            &id,
            "key",
            "--clearmodifiers",
            shortcut,
        ])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn target_xdotool(target: &Cell<u64>, action: &str) -> bool {
    let id = target.get();
    if id == 0 {
        return false;
    }
    Command::new("xdotool")
        .arg(action)
        .arg(id.to_string())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn target_maximize(target: &Cell<u64>) -> bool {
    let id = target.get();
    if id == 0 {
        return false;
    }
    let window = format!("0x{id:x}");
    Command::new("wmctrl")
        .args([
            "-i",
            "-r",
            &window,
            "-b",
            "toggle,maximized_vert,maximized_horz",
        ])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn show_target_unavailable() {
    show_message(
        "SLOPOS-I",
        "This command needs a focused application window.",
    );
}

fn command_item<F>(label: &str, action: F) -> MenuItem
where
    F: Fn() + 'static,
{
    let item = MenuItem::with_label(label);
    item.connect_activate(move |_| action());
    item
}

fn show_message(title: &str, message: &str) {
    let dialog = MessageDialog::new(
        None::<&Window>,
        DialogFlags::MODAL,
        MessageType::Info,
        ButtonsType::Close,
        message,
    );
    dialog.set_title(title);
    dialog.connect_response(|dialog, _| dialog.close());
    dialog.show_all();
}

fn confirm_action<F>(title: &str, message: &str, action: F)
where
    F: Fn() + 'static,
{
    let dialog = MessageDialog::new(
        None::<&Window>,
        DialogFlags::MODAL,
        MessageType::Question,
        ButtonsType::YesNo,
        message,
    );
    dialog.set_title(title);
    dialog.connect_response(move |dialog, response| {
        if response == ResponseType::Yes {
            action();
        }
        dialog.close();
    });
    dialog.show_all();
}

fn lock_command() -> Option<(&'static str, &'static [&'static str])> {
    for &(program, args) in LOCK_COMMANDS {
        if resolve_program(program).is_some() {
            return Some((program, args));
        }
    }
    None
}

fn spawn_first(programs: &[&str], args: &[&str]) {
    if let Some(program) = programs
        .iter()
        .find(|program| resolve_program(program).is_some())
    {
        spawn_resolved(program, args);
    }
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

fn spawn_first_or_message(programs: &[&str], args: &[&str]) {
    if programs
        .iter()
        .any(|program| resolve_program(program).is_some())
    {
        spawn_first(programs, args);
    } else {
        show_message(
            "SLOPOS-I",
            "No compatible file manager is installed for this command.",
        );
    }
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
                let scaled = mark.scale_simple(20, 20, InterpType::Bilinear)?;
                Some(Image::from_pixbuf(Some(&scaled)))
            }
            Err(error) => {
                log::warn!("Failed to load SLOPOS mark from {path}: {error}");
                None
            }
        }
    })
}
