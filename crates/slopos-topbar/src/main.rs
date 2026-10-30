//! SLOPOS top bar on the SLOPOS toolkit: the System menu, the focused
//! application's menus (imported over `org.gtk.Menus`) or the desktop's own,
//! live sound, network and battery status, the clock and Find Applications.
//!
//! Menus, status menus, the launcher and confirmations are popup windows
//! below the 26-pixel bar. Every source of change (focus, menu exports,
//! providers, signals) is a thread that blocks and wakes the bar; the only
//! timer is the clock's, once a minute.

mod desktop_files;
mod launcher;
mod menus;
mod notifications;
mod session;
mod status;
mod windows;

use std::collections::HashMap;
use std::io::Read;
use std::os::fd::FromRawFd;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use menus::{Capabilities, Command, MenuSpec};
use slopos_core::process;
use slopos_ui::accessibility::window_tree;
use slopos_ui::controls::dialog::{Dialog, DialogOutcome};
use slopos_ui::controls::menu::{Menu, MenuItem, MenuOutcome};
use slopos_ui::event::{AccessibilityRequest, Event, Modifiers, PointerButton};
use slopos_ui::foundation::geometry::Rect;
use slopos_ui::icons::IconLookup;
use slopos_ui::image::Image;
use slopos_ui::platform::menu_import::{Imported, Importer, MenuSource, Update as MenuUpdate};
use slopos_ui::platform::{self, App, Control, Edge, Proxy, WindowKind, WindowOptions};
use slopos_ui::render::{RasterSurface, RenderError};
use slopos_ui::text::{measure, Font};
use slopos_ui::theme::palette;
use status::{Status, Update as StatusUpdate};
use windows::{Active, WindowAction};

const HEIGHT: u32 = 26;
const POPUP_MENU: u64 = 1;
const POPUP_LAUNCHER: u64 = 2;
const POPUP_DIALOG: u64 = 3;
const CLOCK_TIMER: u64 = 1;
/// Toast popups and expiry timers use these plus the notification id.
const TOAST_POPUP: u64 = 1_000;
const TOAST_TIMER: u64 = 10_000;
const TOAST_SIZE: (u32, u32) = (340, 74);
/// The bus name toolkit apps watch to move their menus into this bar.
const GLOBAL_MENU_NAME: &str = "org.slopos.GlobalMenu";
const BAR_ID: u64 = 10;
const MENU_ID: u64 = 100;
const DIALOG_ID: u64 = 400;
const CONTROL_PANELS: &str = "slopos-control-panels";
const SOFTWARE: &str = "slopos-software";
const FILES: &str = "slopos-files";

// X keysyms for the global shortcuts.
const XK_SPACE: u32 = 0x20;
const XK_F2: u32 = 0xffbf;

/// Which popup menu is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Open {
    /// Bar menu `i` (0 is the System menu).
    Bar(usize),
    Status(StatusMenu),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StatusMenu {
    Sound,
    Network,
    Battery,
    Clock,
}

/// What a status-menu item does.
#[derive(Debug, Clone, PartialEq, Eq)]
enum StatusCommand {
    VolumeUp,
    VolumeDown,
    ToggleMute,
    Output(String),
    OpenPanel(&'static str),
}

/// A dialog's purpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pending {
    Confirm(Command),
    Message,
}

/// Results of work done off the UI thread.
enum Background {
    Failed(&'static str, String),
}

/// Signals that ask for the launcher (SIGUSR1) or System menu (SIGUSR2).
static WANT_LAUNCHER: AtomicBool = AtomicBool::new(false);
static WANT_SYSTEM_MENU: AtomicBool = AtomicBool::new(false);
static SIGNAL_PIPE: AtomicI32 = AtomicI32::new(-1);

extern "C" fn on_signal(signal: libc::c_int) {
    match signal {
        libc::SIGUSR1 => WANT_LAUNCHER.store(true, Ordering::SeqCst),
        libc::SIGUSR2 => WANT_SYSTEM_MENU.store(true, Ordering::SeqCst),
        _ => {}
    }
    let fd = SIGNAL_PIPE.load(Ordering::SeqCst);
    if fd >= 0 {
        // SAFETY: write(2) is async-signal-safe; the buffer is static.
        unsafe {
            libc::write(fd, [1u8].as_ptr().cast(), 1);
        }
    }
}

type WakeSlot = Arc<Mutex<Option<Proxy>>>;

fn wake(slot: &WakeSlot) {
    if let Ok(guard) = slot.lock() {
        if let Some(proxy) = guard.as_ref() {
            proxy.wake();
        }
    }
}

/// Signals write to a pipe; a thread blocked on it wakes the bar.
fn install_signal_bridge(slot: WakeSlot) {
    let mut fds = [0; 2];
    // SAFETY: pipe2 fills two descriptors on success.
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return;
    }
    SIGNAL_PIPE.store(fds[1], Ordering::SeqCst);
    // SAFETY: we own fds[0] from here on.
    let mut reader = unsafe { std::fs::File::from_raw_fd(fds[0]) };
    let _ = std::thread::Builder::new()
        .name("topbar-signals".into())
        .spawn(move || {
            let mut byte = [0u8; 1];
            while reader.read(&mut byte).is_ok_and(|n| n > 0) {
                wake(&slot);
            }
        });
    for signal in [libc::SIGUSR1, libc::SIGUSR2] {
        // SAFETY: installing a handler that only touches atomics and write(2).
        unsafe {
            libc::signal(
                signal,
                on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t,
            );
        }
    }
}

/// Another top bar already holds the global-menu name on this session bus.
fn another_bar_is_running() -> bool {
    let Ok(connection) = zbus::blocking::Connection::session() else {
        return false;
    };
    let Ok(dbus) = zbus::blocking::fdo::DBusProxy::new(&connection) else {
        return false;
    };
    GLOBAL_MENU_NAME
        .try_into()
        .ok()
        .and_then(|name| dbus.name_has_owner(name).ok())
        .unwrap_or(false)
}

fn capabilities() -> Capabilities {
    let has = |program: &str| process::resolve_program(program).is_some();
    Capabilities {
        control_panels: has(CONTROL_PANELS),
        software: has(SOFTWARE),
        files: has(FILES),
        appearance: has("slopos-appearance"),
        text_editor: process::first_available(process::TEXT_EDITORS).is_some(),
        terminal: process::first_available(process::TERMINALS).is_some(),
        lock: session::can_lock_screen(),
        switch_user: session::can_switch_user(),
        sleep: false,
        restart: session::can_reboot(),
        shut_down: session::can_poweroff(),
    }
}

/// The SLOPOS mark from the bundled logo, cropped to the symbol.
fn load_mark() -> Option<Image> {
    let mut candidates = Vec::new();
    if let Some(share) = std::env::var_os("SLOPOS_SHARE_DIR") {
        candidates.push(std::path::PathBuf::from(share).join("slopos-i/slopos-logo.png"));
    }
    for dir in std::env::split_paths(
        &std::env::var_os("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into()),
    ) {
        candidates.push(dir.join("slopos-i/slopos-logo.png"));
    }
    candidates.push("assets/slopos-logo.png".into());
    let image = candidates
        .iter()
        .find_map(|path| Image::load(path, 64).ok())?;
    let crop = (image.width().min(image.height()) / 4).max(1);
    let x = (image.width() - crop) / 2;
    let y = (image.height() * 3 / 10).min(image.height() - crop);
    image.cropped(x, y, crop, crop).ok()?.scaled(36, 36).ok()
}

/// Local time as "Fri 14:05" and the date for the clock menu.
fn local_time() -> (String, String, u64) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let seconds = now.as_secs() as libc::time_t;
    // SAFETY: localtime_r with valid pointers.
    let tm = unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&seconds, &mut tm);
        tm
    };
    const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let day = DAYS[tm.tm_wday.clamp(0, 6) as usize];
    let bar = format!("{day} {:02}:{:02}", tm.tm_hour, tm.tm_min);
    let date = format!(
        "{day} {} {} {}",
        tm.tm_mday,
        MONTHS[tm.tm_mon.clamp(0, 11) as usize],
        tm.tm_year + 1900
    );
    // Milliseconds until the next minute starts.
    let into_minute = (now.as_secs() % 60) * 1000 + u64::from(now.subsec_millis());
    (bar, date, 60_000 - into_minute.min(59_999))
}

pub struct TopBar {
    width: u32,
    screen: (u32, u32),
    slot: WakeSlot,
    can: Capabilities,
    mark: Option<Image>,
    font: Font,
    bold: Font,
    active: Option<Active>,
    window_updates: Option<Receiver<Option<Active>>>,
    importer: Option<Importer>,
    imported: Option<(MenuSource, Imported)>,
    specs: Vec<MenuSpec>,
    open: Option<Open>,
    menu: Menu,
    status_commands: Vec<Option<StatusCommand>>,
    status: Option<Status>,
    audio: status::State<slopos_services::audio::AudioSnapshot>,
    network: status::State<slopos_services::network::NetworkSnapshot>,
    power: status::State<slopos_services::power::PowerSnapshot>,
    clock: String,
    date: String,
    launcher: Option<launcher::Launcher>,
    icons: IconLookup,
    icon_cache: HashMap<String, Option<Image>>,
    dialog: Option<(Dialog, Pending)>,
    background: (Sender<Background>, Receiver<Background>),
    modifiers: Modifiers,
    /// Holds GLOBAL_MENU_NAME for as long as the bar runs.
    menu_name: Option<zbus::blocking::Connection>,
    notifications: Option<notifications::Notifications>,
    /// Toasts, oldest first; the first `notifications::VISIBLE` are shown.
    toasts: Vec<notifications::Toast>,
}

impl TopBar {
    fn new(screen: (u32, u32), slot: WakeSlot, can: Capabilities) -> Self {
        let (clock, date, _) = local_time();
        let mut bar = Self {
            width: screen.0,
            screen,
            slot,
            can,
            mark: load_mark(),
            font: Font::system(12.0),
            bold: Font::system(12.0).bold(),
            active: None,
            window_updates: None,
            importer: None,
            imported: None,
            specs: Vec::new(),
            open: None,
            menu: Menu::new(MENU_ID, Vec::new()),
            status_commands: Vec::new(),
            status: None,
            audio: Err("loading".into()),
            network: Err("loading".into()),
            power: Err("loading".into()),
            clock,
            date,
            launcher: None,
            icons: IconLookup::from_environment(),
            icon_cache: HashMap::new(),
            dialog: None,
            background: mpsc::channel(),
            modifiers: Modifiers::default(),
            menu_name: None,
            notifications: None,
            toasts: Vec::new(),
        };
        bar.rebuild_specs();
        bar
    }

    /// Start the threads that feed the bar (needs the wake proxy).
    fn start_sources(&mut self) {
        let slot = self.slot.clone();
        self.window_updates = windows::watch(move || wake(&slot));
        let slot = self.slot.clone();
        self.importer = Importer::start(move || wake(&slot));
        let slot = self.slot.clone();
        self.status = Status::start(move || wake(&slot)).ok();
        let slot = self.slot.clone();
        self.notifications = notifications::Notifications::start(move || wake(&slot));
        // Toolkit apps hand their menus to the bar while this name is owned.
        self.menu_name = zbus::blocking::connection::Builder::session()
            .and_then(|builder| builder.name(GLOBAL_MENU_NAME))
            .and_then(|builder| builder.build())
            .map_err(|error| log::warn!("could not own {GLOBAL_MENU_NAME}: {error}"))
            .ok();
    }

    // ----- notifications ---------------------------------------------------

    /// Place the visible toasts down the right edge under the bar.
    fn layout_toasts(&mut self, control: &mut Control) {
        let (width, height) = TOAST_SIZE;
        for (index, toast) in self.toasts.iter().enumerate() {
            let id = TOAST_POPUP + u64::from(toast.id);
            if index >= notifications::VISIBLE {
                control.close_popup(id);
                continue;
            }
            control.open_popup(
                id,
                Rect {
                    x: self.screen.0 as i32 - width as i32 - 10,
                    y: HEIGHT as i32 + 10 + index as i32 * (height as i32 + 8),
                    width,
                    height,
                },
                false,
            );
        }
    }

    fn show_toast(&mut self, toast: notifications::Toast, control: &mut Control) {
        if let Some(existing) = self.toasts.iter_mut().find(|t| t.id == toast.id) {
            *existing = toast.clone();
        } else {
            self.toasts.push(toast.clone());
        }
        if let Some(timeout) = toast.timeout_ms {
            control.start_timer(
                Duration::from_millis(u64::from(timeout)),
                TOAST_TIMER + u64::from(toast.id),
            );
        }
        self.layout_toasts(control);
    }

    fn remove_toast(&mut self, id: u32, reason: notifications::Reason, control: &mut Control) {
        let before = self.toasts.len();
        self.toasts.retain(|toast| toast.id != id);
        if self.toasts.len() != before {
            control.close_popup(TOAST_POPUP + u64::from(id));
            if let Some(server) = &self.notifications {
                server.closed(id, reason);
            }
            self.layout_toasts(control);
        }
    }

    fn paint_toast(&self, id: u32, surface: &RasterSurface) -> Result<(), RenderError> {
        let Some(toast) = self.toasts.iter().find(|t| t.id == id) else {
            return Ok(());
        };
        let colors = palette();
        let bounds = Rect {
            x: 0,
            y: 0,
            width: surface.logical_width(),
            height: surface.logical_height(),
        };
        surface.fill_rect(bounds, colors.surface)?;
        surface.stroke_rect(bounds, colors.border)?;
        let line = measure("Ag", &self.font).height as i32;
        let inner = bounds.width.saturating_sub(20);
        surface.draw_text(&toast.app, 10, 6, &self.font, colors.ink_secondary)?;
        surface.draw_text(&toast.summary, 10, 8 + line, &self.bold, colors.ink)?;
        let body = slopos_ui::controls::dialog::wrap(&toast.body, &self.font, inner);
        for (index, text) in body.iter().take(2).enumerate() {
            surface.draw_text(
                text,
                10,
                10 + line * (2 + index as i32),
                &self.font,
                colors.ink,
            )?;
        }
        Ok(())
    }

    /// The menus for the current focus: System menu first.
    fn rebuild_specs(&mut self) {
        let mut specs = vec![menus::system_menu(&self.can)];
        specs.extend(match (&self.active, &self.imported) {
            (None, _) => menus::desktop_menus(&self.can),
            (Some(_), Some((_, imported))) if !imported.is_empty() => {
                menus::imported_menus(&imported.titles, &imported.menus)
            }
            (Some(_), _) => menus::window_menus(),
        });
        self.specs = specs;
    }

    // ----- geometry --------------------------------------------------------

    /// The bar slot for each menu title; slot 0 is the SLOPOS mark.
    fn title_rects(&self) -> Vec<Rect> {
        let mut x = 6;
        self.specs
            .iter()
            .enumerate()
            .map(|(index, spec)| {
                let width = if index == 0 {
                    30
                } else {
                    let font = if index == 1 && self.active.is_some() {
                        &self.bold
                    } else {
                        &self.font
                    };
                    measure(&spec.title, font).width as u32 + 18
                };
                let rect = Rect {
                    x,
                    y: 0,
                    width,
                    height: HEIGHT - 1,
                };
                x += width as i32;
                rect
            })
            .collect()
    }

    /// Status items from the right edge: clock, network, sound, battery.
    fn status_rects(&self) -> Vec<(StatusMenu, Rect, String)> {
        let mut items = vec![
            (StatusMenu::Clock, self.clock.clone()),
            (StatusMenu::Network, status::network_text(&self.network)),
            (StatusMenu::Sound, status::sound_text(&self.audio)),
        ];
        if let Some(battery) = status::battery_text(&self.power) {
            items.push((StatusMenu::Battery, battery));
        }
        let mut right = self.width as i32 - 6;
        items
            .into_iter()
            .map(|(kind, text)| {
                let icon = if kind == StatusMenu::Clock { 0 } else { 20 };
                let width = measure(&text, &self.font).width as u32 + 16 + icon;
                right -= width as i32;
                (
                    kind,
                    Rect {
                        x: right,
                        y: 0,
                        width,
                        height: HEIGHT - 1,
                    },
                    text,
                )
            })
            .collect()
    }

    // ----- menus -----------------------------------------------------------

    fn open_bar_menu(&mut self, index: usize, keyboard: bool, control: &mut Control) {
        let Some(spec) = self.specs.get(index) else {
            return;
        };
        let Some(rect) = self.title_rects().get(index).copied() else {
            return;
        };
        self.menu = Menu::new(MENU_ID, spec.menu_items());
        self.status_commands.clear();
        self.show_menu(rect.x, keyboard, Open::Bar(index), control);
    }

    fn open_status_menu(&mut self, kind: StatusMenu, keyboard: bool, control: &mut Control) {
        let Some(rect) = self
            .status_rects()
            .into_iter()
            .find(|(k, _, _)| *k == kind)
            .map(|(_, rect, _)| rect)
        else {
            return;
        };
        let (items, commands) = self.status_menu(kind);
        self.menu = Menu::new(MENU_ID, items);
        self.status_commands = commands;
        self.show_menu(rect.x, keyboard, Open::Status(kind), control);
    }

    fn show_menu(&mut self, x: i32, keyboard: bool, open: Open, control: &mut Control) {
        let (width, height) = self.menu.size();
        let size = (width + 2, height + 2);
        if keyboard {
            self.menu.open_with_keyboard(0, 0, size);
        } else {
            self.menu.open_at(0, 0, size);
        }
        control.open_popup(
            POPUP_MENU,
            Rect {
                x,
                y: HEIGHT as i32,
                width: size.0,
                height: size.1,
            },
            true,
        );
        self.open = Some(open);
    }

    fn close_menu(&mut self, control: &mut Control) {
        if self.open.take().is_some() {
            self.menu.close();
            control.close_popup(POPUP_MENU);
        }
    }

    /// Items for a status menu: truthful state lines, then actions.
    fn status_menu(&self, kind: StatusMenu) -> (Vec<MenuItem>, Vec<Option<StatusCommand>>) {
        let mut items = Vec::new();
        let mut commands = Vec::new();
        let info = |text: String, items: &mut Vec<MenuItem>, commands: &mut Vec<_>| {
            items.push(MenuItem::new(&text).enabled(false));
            commands.push(None);
        };
        let action = |item: MenuItem,
                      command: StatusCommand,
                      items: &mut Vec<MenuItem>,
                      commands: &mut Vec<_>| {
            items.push(item);
            commands.push(Some(command));
        };
        let separator = |items: &mut Vec<MenuItem>, commands: &mut Vec<Option<StatusCommand>>| {
            items.push(MenuItem::separator());
            commands.push(None);
        };
        let panels = self.can.control_panels;
        match kind {
            StatusMenu::Sound => {
                match &self.audio {
                    Ok(snapshot) => {
                        let output = status::default_output(snapshot);
                        match output {
                            Some(device) => {
                                info(
                                    format!("Volume: {}%", status::volume_percent(device)),
                                    &mut items,
                                    &mut commands,
                                );
                                action(
                                    MenuItem::new("Louder").shortcut("Up"),
                                    StatusCommand::VolumeUp,
                                    &mut items,
                                    &mut commands,
                                );
                                action(
                                    MenuItem::new("Quieter").shortcut("Down"),
                                    StatusCommand::VolumeDown,
                                    &mut items,
                                    &mut commands,
                                );
                                action(
                                    MenuItem::new("Mute").checked(device.muted),
                                    StatusCommand::ToggleMute,
                                    &mut items,
                                    &mut commands,
                                );
                            }
                            None => info("No sound output".into(), &mut items, &mut commands),
                        }
                        if snapshot.outputs.len() > 1 {
                            separator(&mut items, &mut commands);
                            for device in &snapshot.outputs {
                                let name = device
                                    .description
                                    .clone()
                                    .unwrap_or_else(|| device.name.clone());
                                let current = output.is_some_and(|o| o.name == device.name);
                                action(
                                    MenuItem::new(&name).checked(current),
                                    StatusCommand::Output(device.name.clone()),
                                    &mut items,
                                    &mut commands,
                                );
                            }
                        }
                    }
                    Err(why) => info(
                        format!("Sound unavailable: {why}"),
                        &mut items,
                        &mut commands,
                    ),
                }
                separator(&mut items, &mut commands);
                action(
                    MenuItem::new("Sound Preferences…").enabled(panels),
                    StatusCommand::OpenPanel("--sound"),
                    &mut items,
                    &mut commands,
                );
            }
            StatusMenu::Network => {
                match &self.network {
                    Ok(snapshot) => {
                        if snapshot.active_connections.is_empty() {
                            info("Not connected".into(), &mut items, &mut commands);
                        }
                        for connection in &snapshot.active_connections {
                            let address = connection
                                .ipv4
                                .as_ref()
                                .and_then(|ip| ip.addresses.first().map(|a| a.address.clone()))
                                .unwrap_or_default();
                            let line = if address.is_empty() {
                                connection.id.clone()
                            } else {
                                format!("{} — {address}", connection.id)
                            };
                            info(line, &mut items, &mut commands);
                        }
                    }
                    Err(why) => info(
                        format!("Network unavailable: {why}"),
                        &mut items,
                        &mut commands,
                    ),
                }
                separator(&mut items, &mut commands);
                action(
                    MenuItem::new("Network Preferences…").enabled(panels),
                    StatusCommand::OpenPanel("--network"),
                    &mut items,
                    &mut commands,
                );
            }
            StatusMenu::Battery => {
                if let Some(text) = status::battery_text(&self.power) {
                    info(format!("Battery: {text}"), &mut items, &mut commands);
                }
                separator(&mut items, &mut commands);
                action(
                    MenuItem::new("Display Preferences…").enabled(panels),
                    StatusCommand::OpenPanel("--display"),
                    &mut items,
                    &mut commands,
                );
            }
            StatusMenu::Clock => {
                info(self.date.clone(), &mut items, &mut commands);
                separator(&mut items, &mut commands);
                action(
                    MenuItem::new("Date & Time Preferences…").enabled(panels),
                    StatusCommand::OpenPanel("--datetime"),
                    &mut items,
                    &mut commands,
                );
            }
        }
        (items, commands)
    }

    fn run_status(&mut self, command: StatusCommand) {
        let device = self
            .audio
            .as_ref()
            .ok()
            .and_then(status::default_output)
            .cloned();
        let Some(status) = &self.status else {
            return;
        };
        let percent = device.as_ref().map_or(0, status::volume_percent);
        match (command, device) {
            (StatusCommand::VolumeUp, Some(device)) => status.send(
                status::Command::SetOutputVolume(device.name, (percent + 5).min(100)),
            ),
            (StatusCommand::VolumeDown, Some(device)) => status.send(
                status::Command::SetOutputVolume(device.name, percent.saturating_sub(5)),
            ),
            (StatusCommand::ToggleMute, Some(device)) => {
                status.send(status::Command::SetOutputMuted(device.name, !device.muted))
            }
            (StatusCommand::Output(name), _) => {
                status.send(status::Command::SetDefaultOutput(name))
            }
            (StatusCommand::OpenPanel(flag), _) => launch(CONTROL_PANELS, &[flag]),
            _ => {}
        }
    }

    // ----- commands --------------------------------------------------------

    fn run(&mut self, command: Command, control: &mut Control) {
        if let Some((title, message, verb)) = menus::confirmation(command) {
            // Cancel is the default: Return never ends the session.
            let mut dialog = Dialog::new(DIALOG_ID, title, message, &["Cancel", verb]);
            dialog.set_default(0);
            self.show_dialog(dialog, Pending::Confirm(command), control);
            return;
        }
        self.execute(command, control);
    }

    fn execute(&mut self, command: Command, control: &mut Control) {
        match command {
            Command::About => {
                let message = format!(
                    "SLOPOS-I {}\n\nAn X11 desktop environment for Linux, written in Rust.\nThis is an alpha release.\n\nMIT License",
                    env!("CARGO_PKG_VERSION")
                );
                self.show_dialog(
                    Dialog::new(DIALOG_ID, "About SLOPOS-I", &message, &["OK"]),
                    Pending::Message,
                    control,
                );
            }
            Command::ControlPanels => launch(CONTROL_PANELS, &[]),
            Command::Software => launch(SOFTWARE, &[]),
            Command::DesktopPicture => launch(CONTROL_PANELS, &["--desktop"]),
            Command::Appearance(id) => launch("slopos-appearance", &[id]),
            Command::NewFilesWindow => launch(FILES, &[]),
            Command::NewTextDocument => launch_first(process::TEXT_EDITORS),
            Command::OpenTerminal => launch_first(process::TERMINALS),
            Command::FindApplications => self.open_launcher(control),
            Command::NewFolder => {
                if let Err(error) = desktop_files::create_untitled_folder() {
                    self.message("Could not create a folder", &error, control);
                }
            }
            Command::Lock => {
                if !session::lock_screen() {
                    self.message(
                        "Could not lock the screen",
                        "The screen locker did not start.",
                        control,
                    );
                }
            }
            Command::SwitchUser => {
                session::switch_user();
            }
            Command::Sleep => {
                if let Some(status) = &self.status {
                    status.send(status::Command::Suspend);
                }
            }
            Command::LogOut => self.off_thread("Could not log out", || {
                let path = process::resolve_program("slopos-session")
                    .ok_or_else(|| "slopos-session is not installed".to_owned())?;
                match std::process::Command::new(path).arg("--logout").status() {
                    Ok(status) if status.success() => Ok(()),
                    Ok(_) => Err("No SLOPOS session is running on this display.".into()),
                    Err(error) => Err(error.to_string()),
                }
            }),
            Command::Restart => self.off_thread("Could not restart", session::reboot_system),
            Command::ShutDown => self.off_thread("Could not shut down", session::poweroff_system),
            Command::EmptyTrash => {
                self.off_thread("Could not empty the Trash", desktop_files::empty_trash)
            }
            Command::Minimize | Command::Zoom | Command::CloseWindow => {
                if let Some(active) = &self.active {
                    let action = match command {
                        Command::Minimize => WindowAction::Minimize,
                        Command::Zoom => WindowAction::Zoom,
                        _ => WindowAction::Close,
                    };
                    if let Err(error) = windows::request(active.window, action) {
                        log::warn!("window action failed: {error}");
                    }
                }
            }
            Command::Imported(menu, item) => {
                if let (Some(importer), Some((source, imported))) = (&self.importer, &self.imported)
                {
                    importer.activate(source, imported, menu, item);
                }
            }
        }
    }

    /// Run blocking work (logind, file removal) off the UI thread; failures
    /// come back as a message.
    fn off_thread(
        &self,
        what: &'static str,
        work: impl FnOnce() -> Result<(), String> + Send + 'static,
    ) {
        let sender = self.background.0.clone();
        let slot = self.slot.clone();
        let _ = std::thread::Builder::new()
            .name("topbar-action".into())
            .spawn(move || {
                if let Err(error) = work() {
                    let _ = sender.send(Background::Failed(what, error));
                    wake(&slot);
                }
            });
    }

    fn message(&mut self, title: &str, message: &str, control: &mut Control) {
        self.show_dialog(
            Dialog::new(DIALOG_ID, title, message, &["OK"]),
            Pending::Message,
            control,
        );
    }

    fn show_dialog(&mut self, mut dialog: Dialog, pending: Pending, control: &mut Control) {
        let (width, height) = dialog.layout_standalone(420);
        control.open_popup(
            POPUP_DIALOG,
            Rect {
                x: (self.screen.0 as i32 - width as i32) / 2,
                y: (self.screen.1 as i32 - height as i32) / 4,
                width,
                height,
            },
            true,
        );
        self.dialog = Some((dialog, pending));
    }

    fn open_launcher(&mut self, control: &mut Control) {
        if self.launcher.is_some() {
            return;
        }
        self.close_menu(control);
        let mut apps = slopos_core::desktop_entry::scan_desktop_apps();
        apps.retain(|app| !app.argv.is_empty());
        self.launcher = Some(launcher::Launcher::new(apps));
        control.open_popup(
            POPUP_LAUNCHER,
            Rect {
                x: (self.screen.0 as i32 - launcher::WIDTH as i32) / 2,
                y: (self.screen.1 as i32 / 5).max(HEIGHT as i32 + 8),
                width: launcher::WIDTH,
                height: launcher::HEIGHT,
            },
            true,
        );
    }

    fn close_launcher(&mut self, control: &mut Control) {
        if self.launcher.take().is_some() {
            control.close_popup(POPUP_LAUNCHER);
        }
    }

    fn icon(&mut self, name: &str, size: u32) -> Option<Image> {
        if let Some(image) = self.icon_cache.get(name) {
            return image.clone();
        }
        let image = self
            .icons
            .load(name, size, 1)
            .or_else(|| self.icons.load("application-x-executable", size, 1));
        if self.icon_cache.len() > 256 {
            self.icon_cache.clear();
        }
        self.icon_cache.insert(name.to_owned(), image.clone());
        image
    }

    // ----- updates from the sources ----------------------------------------

    fn receive(&mut self, control: &mut Control) -> bool {
        let mut changed = false;
        if let Some(updates) = &self.window_updates {
            let latest: Vec<Option<Active>> = updates.try_iter().collect();
            if let Some(active) = latest.into_iter().last() {
                changed = true;
                let source = active.as_ref().and_then(|a| a.menus.clone());
                let same_source = source.as_ref() == self.imported.as_ref().map(|(s, _)| s);
                self.active = active;
                if !same_source {
                    self.imported = None;
                    if let (Some(importer), Some(source)) = (&self.importer, source) {
                        importer.fetch(source);
                    }
                }
                self.close_menu(control);
                self.rebuild_specs();
            }
        }
        if let Some(importer) = &self.importer {
            let wanted = self.active.as_ref().and_then(|a| a.menus.clone());
            for update in importer.updates.try_iter() {
                match update {
                    MenuUpdate::Menus(source, imported) if Some(&source) == wanted.as_ref() => {
                        self.imported = Some((source, imported));
                        changed = true;
                    }
                    MenuUpdate::Changed(name) => {
                        if let Some(source) = wanted.as_ref().filter(|s| s.bus_name == name) {
                            importer.fetch(source.clone());
                        }
                    }
                    MenuUpdate::Failed(source, error) => {
                        log::info!("menus of {} unavailable: {error}", source.bus_name);
                    }
                    _ => {}
                }
            }
            if changed && self.open.is_none() {
                self.rebuild_specs();
            }
        }
        if let Some(status) = &self.status {
            for update in status.updates.try_iter() {
                changed = true;
                match update {
                    StatusUpdate::Audio(state) => self.audio = state,
                    StatusUpdate::Network(state) => self.network = state,
                    StatusUpdate::Power(state) => {
                        self.can.sleep = state.as_ref().is_ok_and(|p| {
                            p.suspend == slopos_services::power::SuspendCapability::Available
                        });
                        self.power = state;
                    }
                    StatusUpdate::Failed(error) => log::warn!("status change failed: {error}"),
                }
            }
        }
        let messages: Vec<notifications::Message> = self
            .notifications
            .as_ref()
            .map(|server| server.messages.try_iter().collect())
            .unwrap_or_default();
        for message in messages {
            changed = true;
            match message {
                notifications::Message::Show(toast) => self.show_toast(toast, control),
                notifications::Message::Close(id) => {
                    self.remove_toast(id, notifications::Reason::Closed, control)
                }
            }
        }
        let failures: Vec<Background> = self.background.1.try_iter().collect();
        for Background::Failed(what, error) in failures {
            self.message(what, &error, control);
            changed = true;
        }
        if WANT_LAUNCHER.swap(false, Ordering::SeqCst) {
            if self.launcher.is_some() {
                self.close_launcher(control);
            } else {
                self.open_launcher(control);
            }
            changed = true;
        }
        if WANT_SYSTEM_MENU.swap(false, Ordering::SeqCst) {
            self.close_launcher(control);
            self.open_bar_menu(0, true, control);
            changed = true;
        }
        changed
    }

    // ----- input -----------------------------------------------------------

    fn handle_bar(&mut self, event: &Event, control: &mut Control) {
        match event {
            Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
            } => {
                if let Some(index) = self.title_rects().iter().position(|r| r.contains(*x, *y)) {
                    if self.open == Some(Open::Bar(index)) {
                        self.close_menu(control);
                    } else {
                        self.open_bar_menu(index, false, control);
                    }
                    return;
                }
                if let Some(kind) = self
                    .status_rects()
                    .into_iter()
                    .find(|(_, rect, _)| rect.contains(*x, *y))
                    .map(|(kind, _, _)| kind)
                {
                    if self.open == Some(Open::Status(kind)) {
                        self.close_menu(control);
                    } else {
                        self.open_status_menu(kind, false, control);
                    }
                    return;
                }
                self.close_menu(control);
            }
            // While a menu is open, sliding across titles switches menus.
            Event::PointerMoved { x, y } if self.open.is_some() => {
                if let Some(index) = self.title_rects().iter().position(|r| r.contains(*x, *y)) {
                    if self.open != Some(Open::Bar(index))
                        && matches!(self.open, Some(Open::Bar(_)))
                    {
                        self.open_bar_menu(index, false, control);
                    }
                }
            }
            Event::Accessibility(AccessibilityRequest::Click(id)) => {
                let index = (*id).checked_sub(BAR_ID + 1).map(|i| i as usize);
                if let Some(index) = index.filter(|i| *i < self.specs.len()) {
                    self.open_bar_menu(index, true, control);
                }
            }
            _ => {}
        }
    }

    fn handle_menu(&mut self, event: &Event, control: &mut Control) {
        let Some(open) = self.open else {
            return;
        };
        match self.menu.handle(event) {
            MenuOutcome::Chosen(item) => {
                self.close_menu(control);
                match open {
                    Open::Bar(index) => {
                        if let Some(command) =
                            self.specs.get(index).and_then(|s| s.commands()[item])
                        {
                            self.run(command, control);
                        }
                    }
                    Open::Status(_) => {
                        if let Some(Some(command)) = self.status_commands.get(item).cloned() {
                            self.run_status(command);
                        }
                    }
                }
            }
            MenuOutcome::Dismissed => self.close_menu(control),
            MenuOutcome::Neighbour(direction) => {
                if let Open::Bar(index) = open {
                    let count = self.specs.len() as i32;
                    let next = (index as i32 + direction).rem_euclid(count) as usize;
                    self.open_bar_menu(next, true, control);
                }
            }
            _ => {}
        }
    }

    fn handle_dialog(&mut self, event: &Event, control: &mut Control) {
        let Some((dialog, _)) = &mut self.dialog else {
            return;
        };
        if let DialogOutcome::Chosen { index, .. } = dialog.handle(event) {
            let (_, pending) = self.dialog.take().expect("dialog is open");
            control.close_popup(POPUP_DIALOG);
            if let (Pending::Confirm(command), 1) = (pending, index) {
                self.execute(command, control);
            }
        }
    }

    fn handle_launcher(&mut self, event: &Event, control: &mut Control) {
        let Some(launcher) = &mut self.launcher else {
            return;
        };
        match launcher.handle(event, self.modifiers) {
            launcher::Outcome::Launch(index) => {
                let app = launcher.apps[index].clone();
                self.close_launcher(control);
                match app.command_for(&[]) {
                    Some(argv) => {
                        if let Err(error) = process::launch(
                            &argv[0],
                            &argv[1..].iter().map(String::as_str).collect::<Vec<_>>(),
                        ) {
                            self.message("Could not open the application", &error, control);
                        }
                    }
                    None => self.message(
                        "Could not open the application",
                        "Its desktop entry has no command.",
                        control,
                    ),
                }
            }
            launcher::Outcome::Close => self.close_launcher(control),
            _ => {}
        }
    }

    // ----- painting --------------------------------------------------------

    fn paint_bar(&mut self, surface: &RasterSurface) -> Result<(), RenderError> {
        let colors = palette();
        let bar = Rect {
            x: 0,
            y: 0,
            width: self.width,
            height: HEIGHT,
        };
        surface.fill_rect(bar, colors.content)?;
        surface.fill_rect(
            Rect {
                y: HEIGHT as i32 - 1,
                height: 1,
                ..bar
            },
            colors.border,
        )?;
        let line = measure("Ag", &self.font).height as i32;
        let text_y = (HEIGHT as i32 - 1 - line) / 2;
        let highlighted = match self.open {
            Some(Open::Bar(index)) => Some(index),
            _ => None,
        };
        for (index, rect) in self.title_rects().into_iter().enumerate() {
            let active = highlighted == Some(index);
            if active {
                surface.fill_rect(rect, colors.menu_highlight)?;
            }
            let ink = if active {
                colors.menu_highlight_ink
            } else {
                colors.ink
            };
            if index == 0 {
                match &self.mark {
                    Some(mark) => surface.draw_image_scaled(mark, rect.x + 6, 4, 18, 18)?,
                    None => surface.draw_text("◆", rect.x + 9, text_y, &self.bold, ink)?,
                }
                continue;
            }
            // The application's first menu title is bold, as in Classic.
            let font = if index == 1 && self.active.is_some() {
                &self.bold
            } else {
                &self.font
            };
            surface.draw_text(&self.specs[index].title, rect.x + 9, text_y, font, ink)?;
        }
        let open_status = match self.open {
            Some(Open::Status(kind)) => Some(kind),
            _ => None,
        };
        for (kind, rect, text) in self.status_rects() {
            let active = open_status == Some(kind);
            if active {
                surface.fill_rect(rect, colors.menu_highlight)?;
            }
            let ink = if active {
                colors.menu_highlight_ink
            } else {
                colors.ink
            };
            let mut x = rect.x + 8;
            if kind != StatusMenu::Clock {
                let name = match kind {
                    StatusMenu::Sound => {
                        if status::sound_text(&self.audio) == "Muted" {
                            "audio-volume-muted-symbolic"
                        } else {
                            "audio-volume-high-symbolic"
                        }
                    }
                    StatusMenu::Network if status::on_wifi(&self.network) => {
                        "network-wireless-symbolic"
                    }
                    StatusMenu::Network if self.network.is_ok() => "network-wired-symbolic",
                    StatusMenu::Network => "network-offline-symbolic",
                    _ => "battery-good-symbolic",
                };
                if let Some(icon) = self.icon(name, 16) {
                    surface.draw_image_scaled(&icon, x, 5, 16, 16)?;
                }
                x += 20;
            }
            let font = if kind == StatusMenu::Clock {
                &self.bold
            } else {
                &self.font
            };
            surface.draw_text(&text, x, text_y, font, ink)?;
        }
        Ok(())
    }
}

fn launch(program: &str, args: &[&str]) {
    if let Err(error) = process::launch(program, args) {
        log::warn!("{error}");
    }
}

fn launch_first(candidates: &'static [(&'static str, &'static [&'static str])]) {
    if let Some((path, args)) = process::first_available(candidates) {
        if let Err(error) = process::spawn_detached(&path, args) {
            log::warn!("{error}");
        }
    }
}

impl App for TopBar {
    fn event(&mut self, event: &Event, control: &mut Control) {
        if let Some(modifiers) = match event {
            Event::KeyPressed { modifiers, .. } => Some(*modifiers),
            _ => None,
        } {
            self.modifiers = modifiers;
        }
        match event {
            Event::Resized { width, .. } => self.width = *width,
            Event::CloseRequested => {}
            Event::Wake => {
                if !self.receive(control) {
                    return;
                }
            }
            Event::Timer(timer) if *timer >= TOAST_TIMER => {
                let id = (*timer - TOAST_TIMER) as u32;
                self.remove_toast(id, notifications::Reason::Expired, control);
            }
            Event::Timer(CLOCK_TIMER) => {
                let (clock, date, next) = local_time();
                self.clock = clock;
                self.date = date;
                control.start_timer(Duration::from_millis(next), CLOCK_TIMER);
            }
            Event::GlobalKey { keysym, modifiers } => {
                if *keysym == XK_SPACE && modifiers.logo {
                    if self.launcher.is_some() {
                        self.close_launcher(control);
                    } else {
                        self.open_launcher(control);
                    }
                } else if *keysym == XK_F2 && modifiers.control {
                    self.close_launcher(control);
                    self.open_bar_menu(0, true, control);
                }
            }
            Event::Popup { id, event } => {
                let event = event.as_ref();
                match *id {
                    POPUP_MENU => self.handle_menu(event, control),
                    POPUP_DIALOG => self.handle_dialog(event, control),
                    POPUP_LAUNCHER => self.handle_launcher(event, control),
                    toast if toast >= TOAST_POPUP => {
                        // A click opens the notification's default action
                        // (when it has one) and dismisses it.
                        if let Event::PointerPressed { .. } = event {
                            let id = (toast - TOAST_POPUP) as u32;
                            let action = self
                                .toasts
                                .iter()
                                .find(|t| t.id == id)
                                .and_then(|t| t.default_action().map(str::to_owned));
                            if let (Some(action), Some(server)) = (action, &self.notifications) {
                                server.invoked(id, &action);
                            }
                            self.remove_toast(id, notifications::Reason::Dismissed, control);
                        }
                    }
                    _ => {}
                }
            }
            // Keys while a popup grabs the keyboard arrive as popup events;
            // anything else is for the bar itself.
            other => {
                if self.dialog.is_none() && self.launcher.is_none() {
                    self.handle_bar(other, control);
                }
            }
        }
        control.request_redraw();
    }

    fn paint(&mut self, surface: &RasterSurface) {
        if let Err(error) = self.paint_bar(surface) {
            log::warn!("paint failed: {error}");
        }
    }

    fn paint_popup(&mut self, id: u64, surface: &RasterSurface) {
        let result = match id {
            POPUP_MENU => surface
                .clear(palette().content)
                .and_then(|()| self.menu.paint(surface)),
            POPUP_DIALOG => match &mut self.dialog {
                Some((dialog, _)) => surface
                    .clear(palette().content)
                    .and_then(|()| dialog.paint(surface)),
                None => Ok(()),
            },
            toast if toast >= TOAST_POPUP => {
                self.paint_toast((toast - TOAST_POPUP) as u32, surface)
            }
            POPUP_LAUNCHER => {
                let mut launcher = self.launcher.take();
                let result = match &mut launcher {
                    Some(launcher) => launcher.paint(surface, &mut |name| self.icon(name, 32)),
                    None => Ok(()),
                };
                self.launcher = launcher;
                result
            }
            _ => Ok(()),
        };
        if let Err(error) = result {
            log::warn!("popup paint failed: {error}");
        }
    }

    fn accessibility(&self) -> Option<accesskit::TreeUpdate> {
        let mut bar = accesskit::Node::new(accesskit::Role::MenuBar);
        let mut nodes = Vec::new();
        let mut children = Vec::new();
        for (index, (spec, rect)) in self.specs.iter().zip(self.title_rects()).enumerate() {
            let id = BAR_ID + 1 + index as u64;
            let mut node = accesskit::Node::new(accesskit::Role::MenuItem);
            node.set_label(if index == 0 {
                "System"
            } else {
                spec.title.as_str()
            });
            node.set_bounds(accesskit::Rect {
                x0: f64::from(rect.x),
                y0: 0.0,
                x1: f64::from(rect.x + rect.width as i32),
                y1: f64::from(HEIGHT),
            });
            node.add_action(accesskit::Action::Click);
            children.push(accesskit::NodeId(id));
            nodes.push((id, node));
        }
        for (offset, (kind, _, text)) in self.status_rects().into_iter().enumerate() {
            let id = BAR_ID + 60 + offset as u64;
            let mut node = accesskit::Node::new(accesskit::Role::Button);
            let what = match kind {
                StatusMenu::Sound => "Sound",
                StatusMenu::Network => "Network",
                StatusMenu::Battery => "Battery",
                StatusMenu::Clock => "Clock",
            };
            node.set_label(format!("{what}: {text}"));
            children.push(accesskit::NodeId(id));
            nodes.push((id, node));
        }
        bar.set_children(children);
        nodes.insert(0, (BAR_ID, bar));
        let mut focus = None;
        if self.open.is_some() {
            nodes.extend(self.menu.accessibility_nodes());
            focus = Some(self.menu.focus_node());
        }
        if let Some((dialog, _)) = &self.dialog {
            nodes.extend(dialog.accessibility_nodes());
            focus = Some(dialog.focus_node());
        }
        if let Some(launcher) = &self.launcher {
            nodes.extend(launcher.accessibility_nodes());
            focus = Some(launcher.focus_node());
        }
        Some(window_tree("SLOPOS Top Bar", nodes, focus))
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let screen = platform::x11::screen_size().unwrap_or((1280, 800));
    let slot: WakeSlot = Arc::new(Mutex::new(None));
    if another_bar_is_running() {
        eprintln!("slopos-topbar: Refusing to start a second SLOPOS top bar on this session bus");
        std::process::exit(0);
    }
    install_signal_bridge(slot.clone());
    let mut bar = TopBar::new(screen, slot.clone(), capabilities());
    let mut options = WindowOptions::new("SLOPOS Top Bar", "slopos-topbar", screen.0, HEIGHT);
    options.kind = WindowKind::Dock {
        edge: Edge::Top,
        strut: HEIGHT,
    };
    options.resizable = false;
    options.position = Some((0, 0));
    // Super+Space belongs to Openbox, which sends SIGUSR1 (rc.xml).
    options.global_keys = vec![(
        XK_F2,
        Modifiers {
            control: true,
            ..Modifiers::default()
        },
    )];
    let start_slot = slot.clone();
    let mut started = false;
    // The sources need the wake proxy, so they start once it exists.
    let result = platform::run(
        options,
        TopBarStarter {
            bar: &mut bar,
            started: &mut started,
        },
        move |proxy| {
            if let Ok(mut guard) = start_slot.lock() {
                *guard = Some(proxy.clone());
            }
            proxy.wake();
        },
    );
    if let Err(error) = result {
        eprintln!("slopos-topbar: {error}");
        std::process::exit(1);
    }
}

/// Starts the bar's sources on the first event, once the wake proxy is set.
struct TopBarStarter<'a> {
    bar: &'a mut TopBar,
    started: &'a mut bool,
}

impl App for TopBarStarter<'_> {
    fn event(&mut self, event: &Event, control: &mut Control) {
        if !*self.started {
            *self.started = true;
            self.bar.start_sources();
            let (_, _, next) = local_time();
            control.start_timer(Duration::from_millis(next), CLOCK_TIMER);
        }
        self.bar.event(event, control);
    }

    fn paint(&mut self, surface: &RasterSurface) {
        self.bar.paint(surface);
    }

    fn paint_popup(&mut self, id: u64, surface: &RasterSurface) {
        self.bar.paint_popup(id, surface);
    }

    fn accessibility(&self) -> Option<accesskit::TreeUpdate> {
        self.bar.accessibility()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slopos_ui::event::Key;
    use slopos_ui::foundation::geometry::Scale;

    fn bar() -> TopBar {
        TopBar::new(
            (1280, 800),
            Arc::new(Mutex::new(None)),
            Capabilities::default(),
        )
    }

    fn press(x: i32, y: i32) -> Event {
        Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
        }
    }

    fn key(key: Key) -> Event {
        Event::KeyPressed {
            key,
            modifiers: Modifiers::default(),
        }
    }

    #[test]
    fn desktop_menus_open_as_popups_switch_and_dismiss() {
        let mut bar = bar();
        let mut control = Control::default();
        let titles: Vec<&str> = bar.specs.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["", "File", "View", "Special", "Help"]);
        let file = bar.title_rects()[1];
        bar.event(&press(file.x + 4, 10), &mut control);
        assert_eq!(bar.open, Some(Open::Bar(1)));
        assert!(matches!(
            control.popup_requests().last(),
            Some(platform::PopupRequest::Open { id: POPUP_MENU, grab: true, rect }) if rect.y == HEIGHT as i32 && rect.x == file.x
        ));
        // Sliding to View switches menus; Right moves on to Special.
        let view = bar.title_rects()[2];
        bar.event(
            &Event::PointerMoved {
                x: view.x + 4,
                y: 10,
            },
            &mut control,
        );
        assert_eq!(bar.open, Some(Open::Bar(2)));
        bar.event(
            &Event::Popup {
                id: POPUP_MENU,
                event: Box::new(key(Key::Right)),
            },
            &mut control,
        );
        assert_eq!(bar.open, Some(Open::Bar(3)));
        // A press outside every window dismisses the menu.
        bar.event(
            &Event::Popup {
                id: POPUP_MENU,
                event: Box::new(press(-50, 400)),
            },
            &mut control,
        );
        assert_eq!(bar.open, None);
        assert!(control
            .popup_requests()
            .contains(&platform::PopupRequest::Close { id: POPUP_MENU }));
        for factor in [1, 2] {
            let surface = RasterSurface::new(1280, HEIGHT, Scale::new(factor).unwrap()).unwrap();
            bar.paint(&surface);
        }
    }

    #[test]
    fn shut_down_asks_first_and_cancel_is_the_default() {
        let mut bar = bar();
        let mut control = Control::default();
        bar.run(Command::ShutDown, &mut control);
        let (dialog, pending) = bar.dialog.as_ref().unwrap();
        assert_eq!(*pending, Pending::Confirm(Command::ShutDown));
        assert_eq!(dialog.default, 0);
        bar.event(
            &Event::Popup {
                id: POPUP_DIALOG,
                event: Box::new(key(Key::Enter)),
            },
            &mut control,
        );
        assert!(
            bar.dialog.is_none(),
            "Enter chose Cancel and closed the dialog"
        );
        assert!(control
            .popup_requests()
            .contains(&platform::PopupRequest::Close { id: POPUP_DIALOG }));
    }

    #[test]
    fn focus_changes_choose_imported_window_or_desktop_menus() {
        let mut bar = bar();
        bar.active = Some(Active {
            window: 7,
            title: "Notes".into(),
            class: "xterm".into(),
            fullscreen: false,
            menus: None,
        });
        bar.rebuild_specs();
        assert_eq!(bar.specs[1].title, "Window");
        bar.active = None;
        bar.rebuild_specs();
        assert_eq!(bar.specs[1].title, "File");
    }

    #[test]
    fn status_items_show_unavailable_state_truthfully() {
        let bar = bar();
        let items: Vec<String> = bar.status_rects().into_iter().map(|(_, _, t)| t).collect();
        assert_eq!(items[1], "Network unavailable");
        assert_eq!(items[2], "—");
        assert_eq!(items.len(), 3, "no battery item without a battery");
        let (lines, _) = bar.status_menu(StatusMenu::Sound);
        assert!(lines[0].label.starts_with("Sound unavailable"));
        assert!(!lines[0].enabled);
    }

    #[test]
    fn the_clock_wakes_once_a_minute_at_most() {
        let (bar, date, next) = local_time();
        assert!(bar.len() >= 9 && date.contains(' '));
        assert!((1..=60_000).contains(&next));
    }
}
