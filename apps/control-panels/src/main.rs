//! SLOPOS Control Panels: native Network, Sound, Display, Bluetooth,
//! Date & Time, Desktop and Appearance panels on the SLOPOS toolkit.
//!
//! All state comes from `slopos-services` through the provider thread in
//! `services.rs`; a panel whose provider is missing says so and disables its
//! controls. Changes go back through the same providers, and the window
//! shows the provider's answer (the new snapshot or the error).

mod model;
mod services;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use model::{State, APPEARANCES};
use services::{Command, Services, Update};
use slopos_core::wallpaper::{self, Wallpaper};
use slopos_services::audio::AudioSnapshot;
use slopos_services::bluetooth::BluetoothSnapshot;
use slopos_services::brightness::BrightnessSnapshot;
use slopos_services::network::{NetworkDeviceType, NetworkSnapshot};
use slopos_services::timedate::TimedateSnapshot;
use slopos_ui::accessibility::window_tree;
use slopos_ui::controls::button::{Button, ButtonOutcome};
use slopos_ui::controls::file_chooser::{ChooserOutcome, FileChooser};
use slopos_ui::controls::focus::FocusChain;
use slopos_ui::controls::list::{ListOutcome, ListView};
use slopos_ui::controls::menu::{MenuBar, MenuBarOutcome, MenuItem};
use slopos_ui::controls::range::{Checkbox, Slider, ToggleOutcome, ValueOutcome};
use slopos_ui::event::{AccessibilityRequest, Event, Key};
use slopos_ui::foundation::color::Color;
use slopos_ui::foundation::geometry::Rect;
use slopos_ui::image::{BackgroundMode, Image};
use slopos_ui::platform::{self, App, Control, Proxy, WindowOptions};
use slopos_ui::primitives::button_plate::ButtonPlateVariant;
use slopos_ui::render::{RasterSurface, RenderError};
use slopos_ui::text::{measure, Font};
use slopos_ui::theme::palette;

const WIDTH: u32 = 680;
const HEIGHT: u32 = 440;
const SIDEBAR_WIDTH: u32 = 150;
const SIDEBAR_ID: u64 = 10;
const STATUS_ID: u64 = 2;
const LINES_ID: u64 = 3;
// Panel controls.
const NETWORKING_ID: u64 = 20;
const WIFI_ID: u64 = 21;
const SCAN_ID: u64 = 22;
const WIFI_LIST_ID: u64 = 23;
const OUTPUT_VOLUME_ID: u64 = 30;
const OUTPUT_MUTE_ID: u64 = 31;
const INPUT_VOLUME_ID: u64 = 32;
const INPUT_MUTE_ID: u64 = 33;
const BRIGHTNESS_ID: u64 = 40;
const BLUETOOTH_ID: u64 = 50;
const NTP_ID: u64 = 60;
const APPEARANCE_LIST_ID: u64 = 70;
const APPLY_ID: u64 = 71;
const WALLPAPER_LIST_ID: u64 = 80;
const SET_PICTURE_ID: u64 = 81;
const CHOOSE_PICTURE_ID: u64 = 82;
const MENU_ID: u64 = 100;
const CHOOSER_ID: u64 = 300;
/// Thumbnail size in the Desktop panel's picture list.
const THUMBNAIL: (u32, u32) = (64, 36);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Panel {
    Network,
    Sound,
    Display,
    Bluetooth,
    DateTime,
    Desktop,
    Appearance,
}

/// Theme icon candidates per panel (Adwaita ships mostly symbolic ones).
const PANEL_ICONS: [&[&str]; 7] = [
    &[
        "network-workgroup",
        "network-wireless-symbolic",
        "network-wired-symbolic",
    ],
    &["audio-volume-high", "audio-volume-high-symbolic"],
    &["video-display", "video-display-symbolic", "computer"],
    &[
        "bluetooth",
        "bluetooth-active-symbolic",
        "bluetooth-symbolic",
    ],
    &[
        "preferences-system-time",
        "preferences-system-time-symbolic",
        "x-office-calendar-symbolic",
    ],
    &["user-desktop", "preferences-desktop-wallpaper-symbolic"],
    &[
        "preferences-desktop-theme",
        "preferences-desktop-appearance-symbolic",
        "preferences-system",
    ],
];

const PANELS: [(Panel, &str); 7] = [
    (Panel::Network, "Network"),
    (Panel::Sound, "Sound"),
    (Panel::Display, "Display"),
    (Panel::Bluetooth, "Bluetooth"),
    (Panel::DateTime, "Date & Time"),
    (Panel::Desktop, "Desktop"),
    (Panel::Appearance, "Appearance"),
];

type WakeSlot = Arc<Mutex<Option<Proxy>>>;

/// The panel a command-line argument asks for: the legacy settings flags
/// (`--sound`, `--network`, `--datetime`, …) or a panel name.
fn panel_for_argument(argument: &str) -> Option<usize> {
    let wanted = match argument.trim_start_matches("--") {
        "datetime" | "date" | "time" => Panel::DateTime,
        "sound" | "audio" => Panel::Sound,
        "network" | "wifi" => Panel::Network,
        "display" | "displays" | "brightness" => Panel::Display,
        "bluetooth" => Panel::Bluetooth,
        "appearance" | "theme" => Panel::Appearance,
        "desktop" | "wallpaper" | "background" => Panel::Desktop,
        other => {
            return PANELS
                .iter()
                .position(|(_, name)| name.eq_ignore_ascii_case(other))
        }
    };
    PANELS.iter().position(|(panel, _)| *panel == wanted)
}

pub struct ControlPanels {
    window: (u32, u32),
    services: Option<Services>,
    network: State<NetworkSnapshot>,
    audio: State<AudioSnapshot>,
    brightness: State<BrightnessSnapshot>,
    bluetooth: State<BluetoothSnapshot>,
    time: State<TimedateSnapshot>,
    screen: Option<(u32, u32)>,
    appearance: Option<String>,
    sidebar: ListView,
    networking: Checkbox,
    wifi: Checkbox,
    scan: Button,
    wifi_list: ListView,
    output_volume: Slider,
    output_mute: Checkbox,
    input_volume: Slider,
    input_mute: Checkbox,
    brightness_slider: Slider,
    bluetooth_power: Checkbox,
    ntp: Checkbox,
    appearance_list: ListView,
    apply: Button,
    wallpaper: Option<Wallpaper>,
    wallpaper_list: ListView,
    /// Bundled-picture thumbnails, decoded the first time Desktop shows.
    thumbnails: Option<Vec<Option<Image>>>,
    set_picture: Button,
    choose_picture: Button,
    chooser: Option<FileChooser>,
    /// Where the wallpaper choice is saved (`~/.config/slopos-i`).
    config: Option<PathBuf>,
    /// Paints a background on the desktop (the X root window).
    paint_root: RootPainter,
    menu: MenuBar,
    /// Sidebar icons, looked up on the first paint (per scale).
    panel_icons: Option<(u32, Vec<Option<Image>>)>,
    focus: FocusChain,
    status: String,
    /// Whether `status` reports a failure (shown in the danger color).
    status_error: bool,
    font: Font,
}

type RootPainter = fn(&Image, BackgroundMode) -> Result<(), String>;

fn paint_x11_root(image: &Image, mode: BackgroundMode) -> Result<(), String> {
    platform::x11::set_root_background(|width, height| {
        image
            .compose_background(width, height, mode, Color::BLACK)
            .ok()
    })
    .map_err(|error| error.to_string())
}

/// The menu bar: File, and View with one item per panel.
fn menu_bar() -> MenuBar {
    let panels = PANELS
        .iter()
        .enumerate()
        .map(|(index, (_, name))| {
            MenuItem::new(name)
                .shortcut(&format!("Ctrl+{}", index + 1))
                .checkable()
        })
        .collect();
    MenuBar::new(
        MENU_ID,
        vec![
            (
                "File",
                vec![MenuItem::new("Close Window").shortcut("Ctrl+W")],
            ),
            ("View", panels),
        ],
    )
}

/// Bundled pictures that are installed, as (path, display name, mode).
fn bundled_pictures() -> Vec<(PathBuf, &'static str, &'static str)> {
    let dirs = wallpaper::bundled_dirs();
    wallpaper::BUNDLED
        .iter()
        .filter_map(|(file, name, mode)| {
            wallpaper::find_bundled(&dirs, file).map(|path| (path, *name, *mode))
        })
        .collect()
}

/// A name for the desktop picture at `path`.
fn picture_name(path: &Path) -> String {
    let file = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    wallpaper::BUNDLED
        .iter()
        .find(|(bundled, _, _)| *bundled == file)
        .map_or_else(|| file.to_owned(), |entry| entry.1.to_owned())
}

fn config_home() -> std::path::PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".config"))
        })
        .unwrap_or_else(|| "/tmp".into())
}

impl ControlPanels {
    pub fn new(services: Option<Services>) -> Self {
        let mut sidebar = ListView::new(SIDEBAR_ID, "Control panels", 24);
        sidebar.set_len(PANELS.len());
        sidebar.select_only(0);
        sidebar.focused = true;
        let mut appearance_list = ListView::new(APPEARANCE_LIST_ID, "Appearance", 22);
        appearance_list.set_len(APPEARANCES.len());
        let appearance = model::current_appearance(&config_home());
        if let Some(index) = APPEARANCES
            .iter()
            .position(|(id, _)| Some(*id) == appearance.as_deref())
        {
            appearance_list.select_only(index);
        }
        let config = wallpaper::config_dir();
        let wallpaper = wallpaper::current(config.as_deref(), &wallpaper::bundled_dirs());
        let mut wallpaper_list = ListView::new(WALLPAPER_LIST_ID, "Desktop pictures", 44);
        let pictures = bundled_pictures();
        wallpaper_list.set_len(pictures.len());
        if let Some(index) = pictures
            .iter()
            .position(|(path, _, _)| Some(path) == wallpaper.as_ref().map(|w| &w.path))
        {
            wallpaper_list.select_only(index);
        }
        let mut app = Self {
            window: (WIDTH, HEIGHT),
            services,
            network: model::loading(),
            audio: model::loading(),
            brightness: model::loading(),
            bluetooth: model::loading(),
            time: model::loading(),
            screen: platform::x11::screen_size().ok(),
            appearance,
            sidebar,
            networking: Checkbox::new(NETWORKING_ID, "Networking", false),
            wifi: Checkbox::new(WIFI_ID, "Wi-Fi", false),
            scan: Button::new(SCAN_ID, "Search for Networks", ButtonPlateVariant::Regular),
            wifi_list: ListView::new(WIFI_LIST_ID, "Wi-Fi networks", 20),
            output_volume: Slider::new(OUTPUT_VOLUME_ID, "Output volume", 0.0, 100.0, 5.0),
            output_mute: Checkbox::new(OUTPUT_MUTE_ID, "Mute output", false),
            input_volume: Slider::new(INPUT_VOLUME_ID, "Input volume", 0.0, 100.0, 5.0),
            input_mute: Checkbox::new(INPUT_MUTE_ID, "Mute input", false),
            brightness_slider: Slider::new(BRIGHTNESS_ID, "Brightness", 0.0, 100.0, 5.0),
            bluetooth_power: Checkbox::new(BLUETOOTH_ID, "Bluetooth", false),
            ntp: Checkbox::new(NTP_ID, "Set time automatically", false),
            appearance_list,
            apply: Button::new(APPLY_ID, "Apply", ButtonPlateVariant::Default),
            wallpaper,
            wallpaper_list,
            thumbnails: None,
            set_picture: Button::new(
                SET_PICTURE_ID,
                "Set Desktop Picture",
                ButtonPlateVariant::Default,
            ),
            choose_picture: Button::new(
                CHOOSE_PICTURE_ID,
                "Choose Picture…",
                ButtonPlateVariant::Regular,
            ),
            chooser: None,
            config,
            paint_root: paint_x11_root,
            menu: menu_bar(),
            panel_icons: None,
            focus: FocusChain::default(),
            status: String::new(),
            status_error: false,
            font: Font::system(12.0),
        };
        if app.services.is_none() {
            app.fail("Can't start the service thread; panels show no live state.");
        }
        app.layout();
        app.sync_from_state();
        app
    }

    fn panel(&self) -> Panel {
        PANELS[self.sidebar.cursor().unwrap_or(0).min(PANELS.len() - 1)].0
    }

    fn notice(&mut self, text: impl Into<String>) {
        self.status = text.into();
        self.status_error = false;
    }

    fn fail(&mut self, text: impl Into<String>) {
        self.status = text.into();
        self.status_error = true;
    }

    /// Below the in-window menu bar, or the window top when menus are global.
    fn top(&self) -> i32 {
        self.menu.height() as i32
    }

    fn content(&self) -> Rect {
        let top = self.top();
        Rect {
            x: SIDEBAR_WIDTH as i32 + 16,
            y: top + 16,
            width: self.window.0.saturating_sub(SIDEBAR_WIDTH + 32),
            height: self.window.1.saturating_sub(top as u32 + 48),
        }
    }

    fn layout(&mut self) {
        let (width, height) = self.window;
        let top = self.top();
        self.menu.bounds = Rect {
            x: 0,
            y: 0,
            width,
            height: self.menu.height(),
        };
        self.sidebar.set_bounds(Rect {
            x: 1,
            y: top + 1,
            width: SIDEBAR_WIDTH,
            height: height.saturating_sub(top as u32 + 26),
        });
        let c = self.content();
        let x = c.x;
        // Controls sit below the panel's text lines (three lines + title).
        let top = c.y + 100;
        self.networking = self.networking.clone().at(x, top);
        self.wifi = self.wifi.clone().at(x + 140, top);
        self.scan.bounds = Rect {
            x: x + 240,
            y: top - 1,
            width: 150,
            height: 20,
        };
        self.wifi_list.set_bounds(Rect {
            x: x + 1,
            y: top + 32,
            width: c.width.saturating_sub(2),
            height: c.height.saturating_sub(140),
        });
        self.output_volume.bounds = Rect {
            x: x + 110,
            y: top,
            width: 220,
            height: 18,
        };
        self.output_mute = self.output_mute.clone().at(x + 350, top);
        self.input_volume.bounds = Rect {
            x: x + 110,
            y: top + 60,
            width: 220,
            height: 18,
        };
        self.input_mute = self.input_mute.clone().at(x + 350, top + 60);
        self.brightness_slider.bounds = Rect {
            x: x + 110,
            y: top,
            width: 220,
            height: 18,
        };
        self.bluetooth_power = self.bluetooth_power.clone().at(x, top);
        self.ntp = self.ntp.clone().at(x, top);
        self.appearance_list.set_bounds(Rect {
            x: x + 1,
            y: c.y + 60,
            width: 220,
            height: 22 * APPEARANCES.len() as u32,
        });
        self.apply.bounds = Rect {
            x: x + 240,
            y: c.y + 60,
            width: 80,
            height: 20,
        };
        let list_width = c.width.saturating_sub(170).max(200);
        self.wallpaper_list.set_bounds(Rect {
            x: x + 1,
            y: c.y + 60,
            width: list_width,
            height: c.height.saturating_sub(64),
        });
        let buttons_x = x + list_width as i32 + 14;
        self.set_picture.bounds = Rect {
            x: buttons_x,
            y: c.y + 60,
            width: 140,
            height: 20,
        };
        self.choose_picture.bounds = Rect {
            x: buttons_x,
            y: c.y + 88,
            width: 140,
            height: 20,
        };
        if let Some(chooser) = &mut self.chooser {
            chooser.layout(self.window);
        }
    }

    /// The focusable controls of the current panel, in Tab order.
    fn focus_ids(&self) -> Vec<u64> {
        let mut ids = vec![SIDEBAR_ID];
        ids.extend(match self.panel() {
            Panel::Network => vec![NETWORKING_ID, WIFI_ID, SCAN_ID, WIFI_LIST_ID],
            Panel::Sound => vec![
                OUTPUT_VOLUME_ID,
                OUTPUT_MUTE_ID,
                INPUT_VOLUME_ID,
                INPUT_MUTE_ID,
            ],
            Panel::Display => vec![BRIGHTNESS_ID],
            Panel::Bluetooth => vec![BLUETOOTH_ID],
            Panel::DateTime => vec![NTP_ID],
            Panel::Desktop => vec![WALLPAPER_LIST_ID, SET_PICTURE_ID, CHOOSE_PICTURE_ID],
            Panel::Appearance => vec![APPEARANCE_LIST_ID, APPLY_ID],
        });
        ids
    }

    fn sync_focus(&mut self) {
        let ids = self.focus_ids();
        self.focus.set_ids(ids);
        if self.focus.focused().is_none() {
            self.focus.focus(SIDEBAR_ID);
        }
        let f = self.focus.focused();
        self.sidebar.focused = f == Some(SIDEBAR_ID);
        self.networking.focused = f == Some(NETWORKING_ID);
        self.wifi.focused = f == Some(WIFI_ID);
        self.scan.focused = f == Some(SCAN_ID);
        self.wifi_list.focused = f == Some(WIFI_LIST_ID);
        self.output_volume.focused = f == Some(OUTPUT_VOLUME_ID);
        self.output_mute.focused = f == Some(OUTPUT_MUTE_ID);
        self.input_volume.focused = f == Some(INPUT_VOLUME_ID);
        self.input_mute.focused = f == Some(INPUT_MUTE_ID);
        self.brightness_slider.focused = f == Some(BRIGHTNESS_ID);
        self.bluetooth_power.focused = f == Some(BLUETOOTH_ID);
        self.ntp.focused = f == Some(NTP_ID);
        self.appearance_list.focused = f == Some(APPEARANCE_LIST_ID);
        self.apply.focused = f == Some(APPLY_ID);
        self.wallpaper_list.focused = f == Some(WALLPAPER_LIST_ID);
        self.set_picture.focused = f == Some(SET_PICTURE_ID);
        self.choose_picture.focused = f == Some(CHOOSE_PICTURE_ID);
    }

    /// Copy provider state into the controls; disable what can't be used.
    fn sync_from_state(&mut self) {
        match &self.network {
            Ok(snapshot) => {
                self.networking.enabled = true;
                self.networking.checked = snapshot.networking_enabled;
                let has_wifi = snapshot
                    .devices
                    .iter()
                    .any(|d| d.kind == NetworkDeviceType::Wifi);
                self.wifi.enabled = has_wifi && snapshot.wireless_hardware_enabled;
                self.wifi.checked = snapshot.wireless_enabled && has_wifi;
                self.scan.enabled = has_wifi && snapshot.wireless_enabled;
                let networks = model::wifi_networks(snapshot).len();
                if self.wifi_list.len() != networks {
                    self.wifi_list.reset(networks);
                }
            }
            Err(_) => {
                self.networking.enabled = false;
                self.wifi.enabled = false;
                self.scan.enabled = false;
                self.wifi_list.reset(0);
            }
        }
        let (output, input) = match &self.audio {
            Ok(snapshot) => (
                model::default_output(snapshot).cloned(),
                model::default_input(snapshot).cloned(),
            ),
            Err(_) => (None, None),
        };
        self.output_volume.enabled = output.is_some();
        self.output_mute.enabled = output.is_some();
        if let Some(device) = &output {
            if !self.output_volume.dragging() {
                self.output_volume
                    .set_value(f64::from(model::volume_percent(device)));
            }
            self.output_mute.checked = device.muted;
        }
        self.input_volume.enabled = input.is_some();
        self.input_mute.enabled = input.is_some();
        if let Some(device) = &input {
            if !self.input_volume.dragging() {
                self.input_volume
                    .set_value(f64::from(model::volume_percent(device)));
            }
            self.input_mute.checked = device.muted;
        }
        let brightness = self
            .brightness
            .as_ref()
            .ok()
            .and_then(model::brightness_percent);
        self.brightness_slider.enabled = brightness.is_some();
        if let Some((_, percent)) = brightness {
            if !self.brightness_slider.dragging() {
                self.brightness_slider.set_value(f64::from(percent));
            }
        }
        let adapter = self
            .bluetooth
            .as_ref()
            .ok()
            .and_then(|s| s.adapters.first().cloned());
        self.bluetooth_power.enabled = adapter.is_some();
        self.bluetooth_power.checked = adapter.is_some_and(|a| a.powered);
        match &self.time {
            Ok(snapshot) => {
                self.ntp.enabled = snapshot.ntp_available;
                self.ntp.checked = snapshot.ntp_enabled;
            }
            Err(_) => self.ntp.enabled = false,
        }
        self.apply.enabled = self.appearance_list.cursor().is_some();
        self.set_picture.enabled = self.wallpaper_list.cursor().is_some();
        let current = self.panel();
        for (item, (panel, _)) in self.menu.menus[1].items.iter_mut().zip(PANELS) {
            item.checked = panel == current;
        }
    }

    fn apply_update(&mut self, update: Update) {
        match update {
            Update::Network(state) => self.network = state,
            Update::Audio(state) => self.audio = state,
            Update::Brightness(state) => self.brightness = state,
            Update::Bluetooth(state) => self.bluetooth = state,
            Update::Time(state) => self.time = state,
            Update::Failed(error) => self.fail(format!("That change failed: {error}")),
        }
    }

    fn send(&mut self, command: Command) {
        match &self.services {
            Some(services) => {
                services.send(command);
                self.status.clear();
            }
            None => self.fail("No service thread is running."),
        }
    }

    fn handle(&mut self, event: &Event, control: &mut Control) {
        if let Some(chooser) = &mut self.chooser {
            match chooser.handle(event, control.modifiers()) {
                ChooserOutcome::Chosen(path) => {
                    self.chooser = None;
                    self.set_wallpaper(Wallpaper {
                        path,
                        mode: BackgroundMode::Fill.as_str().to_owned(),
                    });
                }
                ChooserOutcome::Cancelled => self.chooser = None,
                ChooserOutcome::Copy(text) => control.set_clipboard(&text),
                ChooserOutcome::PasteRequested => control.request_paste(),
                _ => {}
            }
            return;
        }
        match self.menu.handle(event, self.window) {
            MenuBarOutcome::Chosen { menu: 0, .. } => return control.quit(),
            MenuBarOutcome::Chosen { item, .. } => {
                self.show_panel(item);
                return;
            }
            MenuBarOutcome::Changed => return,
            MenuBarOutcome::Ignored => {}
        }
        if let Event::KeyPressed {
            key: Key::Character(digit @ '1'..='9'),
            modifiers,
        } = event
        {
            if modifiers.control {
                if let Some(index) = digit.to_digit(10).map(|d| d as usize - 1) {
                    if index < PANELS.len() {
                        self.show_panel(index);
                    }
                }
                return;
            }
        }
        if self.focus.handle(event).is_some() {
            self.sync_focus();
            return;
        }
        if let Event::KeyPressed {
            key: Key::Character('q' | 'Q' | 'w' | 'W'),
            modifiers,
        } = event
        {
            if modifiers.control {
                return control.quit();
            }
        }
        if let Event::Accessibility(AccessibilityRequest::Focus(id)) = event {
            if let Some(index) = self.sidebar.row_for_node(*id) {
                self.sidebar.select_only(index);
            }
            self.focus.focus(*id);
            self.sync_focus();
            return;
        }
        let names = |index: usize| {
            PANELS
                .get(index)
                .map(|(_, name)| (*name).to_owned())
                .unwrap_or_default()
        };
        let before = self.panel();
        match self.sidebar.handle(event, control.modifiers(), &names) {
            ListOutcome::FocusRequested => {
                self.focus.focus(SIDEBAR_ID);
                self.sync_focus();
            }
            ListOutcome::Ignored => {}
            _ => {}
        }
        if let Event::Accessibility(AccessibilityRequest::Click(id)) = event {
            if let Some(index) = self.sidebar.row_for_node(*id) {
                self.sidebar.select_only(index);
            }
        }
        if self.panel() != before {
            self.status.clear();
            self.sync_focus();
            return;
        }
        let clicked = |id: u64| matches!(event, Event::Accessibility(AccessibilityRequest::Click(target)) if *target == id);
        match self.panel() {
            Panel::Network => {
                if let ToggleOutcome::Toggled(on) = self.networking.handle(event) {
                    self.send(Command::SetNetworking(on));
                }
                if let ToggleOutcome::Toggled(on) = self.wifi.handle(event) {
                    self.send(Command::SetWifi(on));
                }
                if self.scan.handle(event) == ButtonOutcome::Activated
                    || (clicked(SCAN_ID) && self.scan.enabled)
                {
                    let device = self.network.as_ref().ok().and_then(|s| {
                        s.devices
                            .iter()
                            .find(|d| d.kind == NetworkDeviceType::Wifi)
                            .map(|d| d.object_path.clone())
                    });
                    if let Some(device) = device {
                        self.send(Command::ScanWifi(device));
                        self.notice("Searching for Wi-Fi networks…");
                    }
                }
                let networks = self
                    .network
                    .as_ref()
                    .map(model::wifi_networks)
                    .unwrap_or_default();
                let label = |i: usize| networks.get(i).map(|n| n.0.clone()).unwrap_or_default();
                if self.wifi_list.handle(event, control.modifiers(), &label)
                    == ListOutcome::FocusRequested
                {
                    self.focus.focus(WIFI_LIST_ID);
                    self.sync_focus();
                }
            }
            Panel::Sound => {
                let output = self
                    .audio
                    .as_ref()
                    .ok()
                    .and_then(model::default_output)
                    .map(|d| d.name.clone());
                let input = self
                    .audio
                    .as_ref()
                    .ok()
                    .and_then(model::default_input)
                    .map(|d| d.name.clone());
                if let (ValueOutcome::Value { value, last: true }, Some(device)) =
                    (self.output_volume.handle(event), &output)
                {
                    self.send(Command::SetOutputVolume(device.clone(), value as u32));
                }
                if let (ToggleOutcome::Toggled(muted), Some(device)) =
                    (self.output_mute.handle(event), &output)
                {
                    self.send(Command::SetOutputMuted(device.clone(), muted));
                }
                if let (ValueOutcome::Value { value, last: true }, Some(device)) =
                    (self.input_volume.handle(event), &input)
                {
                    self.send(Command::SetInputVolume(device.clone(), value as u32));
                }
                if let (ToggleOutcome::Toggled(muted), Some(device)) =
                    (self.input_mute.handle(event), &input)
                {
                    self.send(Command::SetInputMuted(device.clone(), muted));
                }
            }
            Panel::Display => {
                if let ValueOutcome::Value { value, last: true } =
                    self.brightness_slider.handle(event)
                {
                    let target = self
                        .brightness
                        .as_ref()
                        .ok()
                        .and_then(|s| model::brightness_value(s, value as u32));
                    if let Some((device, raw)) = target {
                        self.send(Command::SetBrightness(device, raw));
                    }
                }
            }
            Panel::Bluetooth => {
                if let ToggleOutcome::Toggled(on) = self.bluetooth_power.handle(event) {
                    let adapter = self
                        .bluetooth
                        .as_ref()
                        .ok()
                        .and_then(|s| s.adapters.first().map(|a| a.path.clone()));
                    if let Some(adapter) = adapter {
                        self.send(Command::SetBluetoothPowered(adapter, on));
                    }
                }
            }
            Panel::DateTime => {
                if let ToggleOutcome::Toggled(on) = self.ntp.handle(event) {
                    self.send(Command::SetNtp(on));
                }
            }
            Panel::Desktop => {
                let pictures = bundled_pictures();
                let names = |i: usize| pictures.get(i).map(|p| p.1.to_owned()).unwrap_or_default();
                let outcome = self
                    .wallpaper_list
                    .handle(event, control.modifiers(), &names);
                if outcome == ListOutcome::FocusRequested {
                    self.focus.focus(WALLPAPER_LIST_ID);
                    self.sync_focus();
                }
                let activated = matches!(outcome, ListOutcome::Activated(_));
                if activated
                    || self.set_picture.handle(event) == ButtonOutcome::Activated
                    || (clicked(SET_PICTURE_ID) && self.set_picture.enabled)
                {
                    if let Some((path, _, mode)) = self
                        .wallpaper_list
                        .cursor()
                        .and_then(|i| pictures.get(i).cloned())
                    {
                        self.set_wallpaper(Wallpaper {
                            path,
                            mode: mode.to_owned(),
                        });
                    }
                }
                if self.choose_picture.handle(event) == ButtonOutcome::Activated
                    || clicked(CHOOSE_PICTURE_ID)
                {
                    self.open_chooser();
                }
            }
            Panel::Appearance => {
                let names = |i: usize| {
                    APPEARANCES
                        .get(i)
                        .map(|a| a.1.to_owned())
                        .unwrap_or_default()
                };
                let outcome = self
                    .appearance_list
                    .handle(event, control.modifiers(), &names);
                if outcome == ListOutcome::FocusRequested {
                    self.focus.focus(APPEARANCE_LIST_ID);
                    self.sync_focus();
                }
                let activated = matches!(outcome, ListOutcome::Activated(_));
                if activated
                    || self.apply.handle(event) == ButtonOutcome::Activated
                    || (clicked(APPLY_ID) && self.apply.enabled)
                {
                    self.apply_appearance();
                }
            }
        }
    }

    /// Switch to panel `index` (sidebar, View menu or Ctrl+digit).
    fn show_panel(&mut self, index: usize) {
        if index < PANELS.len() && self.sidebar.cursor() != Some(index) {
            self.sidebar.select_only(index);
            self.status.clear();
            self.sync_focus();
        }
    }

    fn open_chooser(&mut self) {
        let dir = std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| {
                let pictures = home.join("Pictures");
                if pictures.is_dir() {
                    pictures
                } else {
                    home
                }
            })
            .unwrap_or_else(|| PathBuf::from("/"));
        let mut chooser = FileChooser::new(
            CHOOSER_ID,
            "Choose a Desktop Picture",
            &dir,
            Some(vec!["png".into()]),
        );
        chooser.layout(self.window);
        self.chooser = Some(chooser);
    }

    /// Save `choice` and repaint the desktop with it. The picture is decoded
    /// first, so an unreadable file changes nothing.
    fn set_wallpaper(&mut self, choice: Wallpaper) {
        let image = match Image::load(&choice.path, 512) {
            Ok(image) => image,
            Err(error) => return self.fail(format!("Can't use that picture: {error}")),
        };
        let Some(config) = self.config.clone() else {
            return self.fail("Can't find your settings folder (HOME is not set).");
        };
        if let Err(error) = wallpaper::save(&config, &choice) {
            return self.fail(format!("Can't save the desktop picture: {error}"));
        }
        let name = picture_name(&choice.path);
        let painted = (self.paint_root)(&image, BackgroundMode::parse(&choice.mode));
        self.wallpaper = Some(choice);
        match painted {
            Ok(()) => self.notice(format!("The desktop picture is now {name}.")),
            Err(error) => self.fail(format!(
                "Saved {name}, but the desktop could not be repainted: {error}"
            )),
        }
    }

    /// Decode the bundled thumbnails once, when Desktop is first shown.
    fn ensure_thumbnails(&mut self) {
        if self.thumbnails.is_some() {
            return;
        }
        let (width, height) = THUMBNAIL;
        self.thumbnails = Some(
            bundled_pictures()
                .iter()
                .map(|(path, _, mode)| {
                    let image = Image::load(path, width).ok()?;
                    image
                        .compose_background(
                            width,
                            height,
                            BackgroundMode::parse(mode),
                            Color::BLACK,
                        )
                        .ok()
                })
                .collect(),
        );
    }

    fn apply_appearance(&mut self) {
        let Some((id, name)) = self
            .appearance_list
            .cursor()
            .and_then(|i| APPEARANCES.get(i))
        else {
            return;
        };
        match slopos_core::process::launch("slopos-appearance", &[id]) {
            Ok(_) => {
                self.appearance = Some((*id).to_owned());
                self.notice(format!("Switching to the {name} appearance…"));
            }
            Err(error) => self.fail(format!("Can't change the appearance: {error}")),
        }
    }

    fn lines(&self) -> (String, Vec<String>) {
        let unavailable = |error: &model::Unavailable, service: &str| vec![error.message(service)];
        match self.panel() {
            Panel::Network => (
                "Network".into(),
                match &self.network {
                    Ok(snapshot) => model::network_lines(snapshot),
                    Err(error) => unavailable(error, "NetworkManager"),
                },
            ),
            Panel::Sound => (
                "Sound".into(),
                match &self.audio {
                    Ok(snapshot) => vec![
                        format!(
                            "Output: {}",
                            model::default_output(snapshot)
                                .map_or_else(|| "none".into(), model::device_name)
                        ),
                        format!(
                            "Input: {}",
                            model::default_input(snapshot)
                                .map_or_else(|| "none".into(), model::device_name)
                        ),
                    ],
                    Err(error) => unavailable(error, "the sound server"),
                },
            ),
            Panel::Display => {
                let mut lines = vec![match self.screen {
                    Some((w, h)) => format!("Screen: {w} × {h} pixels"),
                    None => "Screen size: unavailable".into(),
                }];
                lines.push(match &self.brightness {
                    Ok(snapshot) => match model::brightness_percent(snapshot) {
                        Some((name, percent)) => format!("Backlight {name}: {percent}%"),
                        None => "This display has no adjustable backlight.".into(),
                    },
                    Err(error) => error.message("the backlight service"),
                });
                ("Display".into(), lines)
            }
            Panel::Bluetooth => (
                "Bluetooth".into(),
                match &self.bluetooth {
                    Ok(snapshot) => model::bluetooth_lines(snapshot),
                    Err(error) => unavailable(error, "BlueZ"),
                },
            ),
            Panel::DateTime => (
                "Date & Time".into(),
                match &self.time {
                    Ok(snapshot) => model::timedate_lines(snapshot),
                    Err(error) => unavailable(error, "systemd-timedated"),
                },
            ),
            Panel::Desktop => (
                "Desktop".into(),
                vec![
                    match &self.wallpaper {
                        Some(choice) => format!("Desktop picture: {}", picture_name(&choice.path)),
                        None => "Desktop picture: none (the appearance color shows)".into(),
                    },
                    "Pick a pattern, or choose one of your own PNG pictures.".into(),
                ],
            ),
            Panel::Appearance => (
                "Appearance".into(),
                vec![format!(
                    "Current: {}",
                    self.appearance
                        .as_deref()
                        .and_then(|id| APPEARANCES.iter().find(|a| a.0 == id))
                        .map_or("Platinum (default)", |a| a.1)
                )],
            ),
        }
    }

    fn paint_all(&mut self, surface: &RasterSurface) -> Result<(), RenderError> {
        let colors = palette();
        let (width, height) = self.window;
        if self.panel() == Panel::Desktop {
            self.ensure_thumbnails();
        }
        surface.clear(colors.surface)?;
        self.menu.paint(surface)?;
        let scale = surface.scale().factor();
        if self.panel_icons.as_ref().map(|(s, _)| *s) != Some(scale) {
            let mut lookup = slopos_ui::icons::IconLookup::from_environment();
            let icons = PANEL_ICONS
                .iter()
                .map(|names| names.iter().find_map(|name| lookup.load(name, 16, scale)))
                .collect();
            self.panel_icons = Some((scale, icons));
        }
        let icons = self.panel_icons.as_ref().map(|(_, icons)| icons.as_slice());
        let font = &self.font;
        let line = measure("Ag", font).height as i32;
        self.sidebar
            .paint(surface, |surface, index, rect, selected| {
                if let Some(Some(icon)) = icons.and_then(|icons| icons.get(index)) {
                    surface.draw_image_scaled(icon, rect.x + 8, rect.y + 4, 16, 16)?;
                }
                surface.draw_text(
                    PANELS[index].1,
                    rect.x + 30,
                    rect.y + (rect.height as i32 - line) / 2,
                    font,
                    if selected {
                        colors.selection_ink
                    } else {
                        colors.ink
                    },
                )
            })?;
        let c = self.content();
        let (title, lines) = self.lines();
        surface.draw_text(&title, c.x, c.y, &font.clone().bold(), colors.ink)?;
        // Wrap long lines (provider errors can be long); four rows fit
        // above the controls.
        let wrapped: Vec<String> = lines
            .iter()
            .flat_map(|text| slopos_ui::controls::dialog::wrap(text, font, c.width))
            .take(4)
            .collect();
        for (i, text) in wrapped.iter().enumerate() {
            surface.draw_text(
                text,
                c.x,
                c.y + 22 + i as i32 * (line + 2),
                font,
                colors.ink,
            )?;
        }
        let top = c.y + 100;
        match self.panel() {
            Panel::Network => {
                self.networking.paint(surface)?;
                self.wifi.paint(surface)?;
                self.scan.paint(surface, font)?;
                let networks = self
                    .network
                    .as_ref()
                    .map(model::wifi_networks)
                    .unwrap_or_default();
                self.wifi_list.paint(surface, |surface, i, rect, _| {
                    let (name, strength, secured) = &networks[i];
                    let text = format!(
                        "{name}  —  {strength}%{}",
                        if *secured { "  (secured)" } else { "" }
                    );
                    surface.draw_text(&text, rect.x + 8, rect.y + 2, font, colors.ink)
                })?;
                if networks.is_empty() {
                    let list = self.wifi_list.bounds();
                    surface.draw_text(
                        "No Wi-Fi networks to show",
                        list.x + 8,
                        list.y + 6,
                        font,
                        colors.ink_secondary,
                    )?;
                }
            }
            Panel::Sound => {
                surface.draw_text("Output volume", c.x, top + 2, font, colors.ink)?;
                self.output_volume.paint(surface)?;
                self.output_mute.paint(surface)?;
                surface.draw_text("Input volume", c.x, top + 62, font, colors.ink)?;
                self.input_volume.paint(surface)?;
                self.input_mute.paint(surface)?;
            }
            Panel::Display => {
                surface.draw_text("Brightness", c.x, top + 2, font, colors.ink)?;
                self.brightness_slider.paint(surface)?;
            }
            Panel::Bluetooth => self.bluetooth_power.paint(surface)?,
            Panel::DateTime => self.ntp.paint(surface)?,
            Panel::Desktop => {
                let pictures = bundled_pictures();
                let thumbnails = self.thumbnails.as_deref().unwrap_or(&[]);
                let current = self.wallpaper.as_ref().map(|w| w.path.clone());
                let bold = font.clone().bold();
                self.wallpaper_list.paint(surface, |surface, i, rect, _| {
                    let thumb = Rect {
                        x: rect.x + 6,
                        y: rect.y + (rect.height as i32 - THUMBNAIL.1 as i32) / 2,
                        width: THUMBNAIL.0,
                        height: THUMBNAIL.1,
                    };
                    surface.fill_rect(
                        Rect {
                            x: thumb.x - 1,
                            y: thumb.y - 1,
                            width: thumb.width + 2,
                            height: thumb.height + 2,
                        },
                        colors.border,
                    )?;
                    if let Some(Some(image)) = thumbnails.get(i) {
                        surface.draw_image(image, thumb.x, thumb.y)?;
                    }
                    let (path, name, _) = &pictures[i];
                    let is_current = current.as_ref() == Some(path);
                    surface.draw_text(
                        name,
                        thumb.x + thumb.width as i32 + 12,
                        rect.y + (rect.height as i32 - line) / 2,
                        if is_current { &bold } else { font },
                        colors.ink,
                    )
                })?;
                self.set_picture.paint(surface, font)?;
                self.choose_picture.paint(surface, font)?;
            }
            Panel::Appearance => {
                self.appearance_list.paint(surface, |surface, i, rect, _| {
                    surface.draw_text(APPEARANCES[i].1, rect.x + 8, rect.y + 3, font, colors.ink)
                })?;
                self.apply.paint(surface, font)?;
            }
        }
        let status_y = height as i32 - 22;
        surface.fill_rect(
            Rect {
                x: SIDEBAR_WIDTH as i32 + 2,
                y: status_y - 2,
                width,
                height: 1,
            },
            colors.bevel_shadow,
        )?;
        let ink = if self.status_error {
            colors.danger
        } else {
            colors.ink_secondary
        };
        surface.draw_text(&self.status, c.x, status_y + 2, font, ink)?;
        self.menu.paint_menus(surface)?;
        if let Some(chooser) = &mut self.chooser {
            chooser.paint(surface)?;
        }
        Ok(())
    }
}

impl App for ControlPanels {
    fn event(&mut self, event: &Event, control: &mut Control) {
        match event {
            Event::CloseRequested => return control.quit(),
            Event::Resized { width, height } => {
                self.window = (*width, *height);
                self.layout();
            }
            // Applied even under the chooser: the bar's height changes the layout.
            Event::GlobalMenu(_) => {
                self.menu.handle(event, self.window);
                self.layout();
            }
            Event::Wake => {
                let updates: Vec<Update> = match &self.services {
                    Some(services) => services.updates.try_iter().collect(),
                    None => Vec::new(),
                };
                if updates.is_empty() {
                    return;
                }
                for update in updates {
                    self.apply_update(update);
                }
            }
            _ => self.handle(event, control),
        }
        self.sync_from_state();
        if self.focus.ids().is_empty() {
            self.sync_focus();
        }
        control.request_redraw();
    }

    fn paint(&mut self, surface: &RasterSurface) {
        if let Err(error) = self.paint_all(surface) {
            eprintln!("slopos-control-panels: paint failed: {error}");
        }
    }

    fn menu_bar(&self) -> Option<&MenuBar> {
        Some(&self.menu)
    }

    fn accessibility(&self) -> Option<accesskit::TreeUpdate> {
        let names = |index: usize| {
            PANELS
                .get(index)
                .map(|(_, name)| (*name).to_owned())
                .unwrap_or_default()
        };
        let mut nodes = self.menu.accessibility_nodes();
        nodes.extend(self.sidebar.accessibility_nodes(&names));
        let (title, lines) = self.lines();
        let mut text = accesskit::Node::new(accesskit::Role::Label);
        text.set_label(format!("{title}. {}", lines.join(". ")));
        nodes.push((LINES_ID, text));
        match self.panel() {
            Panel::Network => {
                nodes.push((NETWORKING_ID, self.networking.accessibility_node()));
                nodes.push((WIFI_ID, self.wifi.accessibility_node()));
                nodes.push((SCAN_ID, self.scan.accessibility_node()));
                let networks = self
                    .network
                    .as_ref()
                    .map(model::wifi_networks)
                    .unwrap_or_default();
                let label = |i: usize| {
                    networks
                        .get(i)
                        .map(|n| format!("{}, {}% signal", n.0, n.1))
                        .unwrap_or_default()
                };
                nodes.extend(self.wifi_list.accessibility_nodes(&label));
            }
            Panel::Sound => {
                nodes.push((OUTPUT_VOLUME_ID, self.output_volume.accessibility_node()));
                nodes.push((OUTPUT_MUTE_ID, self.output_mute.accessibility_node()));
                nodes.push((INPUT_VOLUME_ID, self.input_volume.accessibility_node()));
                nodes.push((INPUT_MUTE_ID, self.input_mute.accessibility_node()));
            }
            Panel::Display => {
                nodes.push((BRIGHTNESS_ID, self.brightness_slider.accessibility_node()))
            }
            Panel::Bluetooth => {
                nodes.push((BLUETOOTH_ID, self.bluetooth_power.accessibility_node()))
            }
            Panel::DateTime => nodes.push((NTP_ID, self.ntp.accessibility_node())),
            Panel::Appearance => {
                let names = |i: usize| {
                    APPEARANCES
                        .get(i)
                        .map(|a| a.1.to_owned())
                        .unwrap_or_default()
                };
                nodes.extend(self.appearance_list.accessibility_nodes(&names));
                nodes.push((APPLY_ID, self.apply.accessibility_node()));
            }
            Panel::Desktop => {
                let pictures = bundled_pictures();
                let names = |i: usize| pictures.get(i).map(|p| p.1.to_owned()).unwrap_or_default();
                nodes.extend(self.wallpaper_list.accessibility_nodes(&names));
                nodes.push((SET_PICTURE_ID, self.set_picture.accessibility_node()));
                nodes.push((CHOOSE_PICTURE_ID, self.choose_picture.accessibility_node()));
            }
        }
        let mut status = accesskit::Node::new(accesskit::Role::Label);
        status.set_label(self.status.as_str());
        nodes.push((STATUS_ID, status));
        if let Some(chooser) = &self.chooser {
            nodes.extend(chooser.accessibility_nodes());
            return Some(window_tree(
                "Control Panels",
                nodes,
                Some(chooser.focus_node()),
            ));
        }
        let focus = match self.menu.focus_node() {
            Some(node) => Some(node),
            None => match self.focus.focused() {
                Some(SIDEBAR_ID) => Some(self.sidebar.focus_node()),
                Some(WALLPAPER_LIST_ID) => Some(self.wallpaper_list.focus_node()),
                other => other,
            },
        };
        Some(window_tree("Control Panels", nodes, focus))
    }
}

fn main() {
    if std::env::var_os("RUST_LOG").is_some() {
        let _ = env_logger::try_init();
    }
    let slot: WakeSlot = Arc::new(Mutex::new(None));
    let wake_slot = slot.clone();
    let services = Services::start(move || {
        if let Ok(guard) = wake_slot.lock() {
            if let Some(proxy) = guard.as_ref() {
                proxy.wake();
            }
        }
    })
    .ok();
    let mut app = ControlPanels::new(services);
    if let Some(index) = std::env::args()
        .nth(1)
        .and_then(|arg| panel_for_argument(&arg))
    {
        app.sidebar.select_only(index);
    }
    let options = WindowOptions::new("Control Panels", "slopos-control-panels", WIDTH, HEIGHT);
    if let Err(error) = platform::run(options, app, move |proxy| {
        if let Ok(mut guard) = slot.lock() {
            *guard = Some(proxy.clone());
        }
        proxy.wake();
    }) {
        eprintln!("slopos-control-panels: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slopos_core::capability::Availability;
    use slopos_ui::event::Modifiers;
    use slopos_ui::foundation::geometry::Scale;

    fn key(key: Key) -> Event {
        Event::KeyPressed {
            key,
            modifiers: Modifiers::default(),
        }
    }

    #[test]
    fn legacy_flags_open_the_matching_panel() {
        assert_eq!(
            panel_for_argument("--datetime").map(|i| PANELS[i].0),
            Some(Panel::DateTime)
        );
        assert_eq!(
            panel_for_argument("--sound").map(|i| PANELS[i].0),
            Some(Panel::Sound)
        );
        assert_eq!(
            panel_for_argument("Bluetooth").map(|i| PANELS[i].0),
            Some(Panel::Bluetooth)
        );
        assert_eq!(panel_for_argument("--nope"), None);
    }

    #[test]
    fn missing_services_disable_controls_and_say_unavailable() {
        let mut app = ControlPanels::new(None);
        app.apply_update(Update::Network(Err(model::Unavailable::new(
            Availability::MissingService,
            "",
        ))));
        app.apply_update(Update::Audio(Err(model::Unavailable::new(
            Availability::Disconnected,
            "socket closed",
        ))));
        app.sync_from_state();
        assert!(!app.networking.enabled && !app.wifi.enabled && !app.output_volume.enabled);
        assert_eq!(
            app.lines().1,
            vec!["Unavailable: NetworkManager is not running on this computer.".to_owned()]
        );
        let mut control = Control::default();
        app.event(&key(Key::Down), &mut control);
        assert_eq!(app.panel(), Panel::Sound);
        assert!(app.lines().1[0].contains("connection to the sound server was lost"));
        // Brightness without devices: no slider, an honest line.
        app.apply_update(Update::Brightness(Ok(BrightnessSnapshot {
            revision: 1,
            availability: Availability::UnsupportedHardware,
            devices: Vec::new(),
        })));
        app.sync_from_state();
        app.event(&key(Key::Down), &mut control);
        assert_eq!(app.panel(), Panel::Display);
        assert!(!app.brightness_slider.enabled);
        assert!(app.lines().1[1].contains("no adjustable backlight"));
        for factor in [1, 2] {
            let surface = RasterSurface::new(WIDTH, HEIGHT, Scale::new(factor).unwrap()).unwrap();
            app.paint(&surface);
        }
    }

    #[test]
    fn sound_changes_go_to_the_provider_for_the_default_device() {
        let (commands, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let (_sender, updates) = std::sync::mpsc::channel();
        let mut app = ControlPanels::new(Some(Services::for_tests(commands, updates)));
        app.apply_update(Update::Audio(Ok(AudioSnapshot {
            revision: 1,
            availability: Availability::Available,
            default_output: Some("sink".into()),
            default_input: None,
            outputs: vec![slopos_services::audio::AudioDevice {
                index: 0,
                name: "sink".into(),
                description: Some("Speakers".into()),
                direction: slopos_services::audio::AudioDirection::Output,
                muted: false,
                volume_millipercent: vec![50_000],
                is_default: true,
            }],
            inputs: Vec::new(),
        })));
        app.sync_from_state();
        let mut control = Control::default();
        app.event(&key(Key::Down), &mut control);
        assert_eq!(app.output_volume.value(), 50.0);
        assert!(!app.input_volume.enabled);
        // Focus starts on the sidebar; Tab reaches the first control.
        app.event(&key(Key::Tab), &mut control);
        assert!(app.output_volume.focused);
        app.event(&key(Key::Right), &mut control);
        assert_eq!(
            receiver.try_recv().unwrap(),
            Command::SetOutputVolume("sink".into(), 55)
        );
        app.event(&key(Key::Tab), &mut control);
        app.event(&key(Key::Space), &mut control);
        assert_eq!(
            receiver.try_recv().unwrap(),
            Command::SetOutputMuted("sink".into(), true)
        );
        let tree = app.accessibility().unwrap();
        assert!(tree
            .nodes
            .iter()
            .any(|(_, n)| n.label() == Some("Output volume")));
    }

    #[test]
    fn desktop_picture_is_saved_painted_and_reported() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static PAINTED: AtomicUsize = AtomicUsize::new(0);
        fn painter(image: &Image, mode: BackgroundMode) -> Result<(), String> {
            assert_eq!((image.width(), image.height()), (1, 1));
            assert_eq!(mode, BackgroundMode::Fill);
            PAINTED.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        let dir = tempfile::tempdir().unwrap();
        let mut app = ControlPanels::new(None);
        app.config = Some(dir.path().join("config"));
        app.paint_root = painter;
        let mut control = Control::default();
        // Ctrl+6 and the View menu both reach the Desktop panel.
        app.event(
            &Event::KeyPressed {
                key: Key::Character('6'),
                modifiers: Modifiers {
                    control: true,
                    ..Modifiers::default()
                },
            },
            &mut control,
        );
        assert_eq!(app.panel(), Panel::Desktop);
        app.event(&Event::MenuChosen { menu: 1, item: 0 }, &mut control);
        assert_eq!(app.panel(), Panel::Network);
        assert!(app.menu.menus[1].items[0].checked && !app.menu.menus[1].items[5].checked);
        app.event(&Event::MenuChosen { menu: 1, item: 5 }, &mut control);
        assert_eq!(app.panel(), Panel::Desktop);
        // A picture that doesn't decode changes nothing.
        let broken = dir.path().join("broken.png");
        std::fs::write(&broken, b"not a png").unwrap();
        app.set_wallpaper(Wallpaper {
            path: broken,
            mode: "fill".into(),
        });
        assert!(app.status_error && app.status.starts_with("Can't use that picture"));
        assert_eq!(PAINTED.load(Ordering::SeqCst), 0);
        assert!(wallpaper::load(&dir.path().join("config")).is_none());
        // A real picture is saved, painted and named.
        let picture = dir.path().join("Beach.png");
        let surface = RasterSurface::new(1, 1, Scale::new(1).unwrap()).unwrap();
        std::fs::write(&picture, surface.encode_png().unwrap()).unwrap();
        app.set_wallpaper(Wallpaper {
            path: picture.clone(),
            mode: "fill".into(),
        });
        assert_eq!(PAINTED.load(Ordering::SeqCst), 1);
        assert!(!app.status_error, "{}", app.status);
        assert_eq!(app.status, "The desktop picture is now Beach.png.");
        assert_eq!(
            wallpaper::load(&dir.path().join("config")).map(|w| w.path),
            Some(picture)
        );
        assert_eq!(app.lines().1[0], "Desktop picture: Beach.png");
        for factor in [1, 2] {
            let surface = RasterSurface::new(WIDTH, HEIGHT, Scale::new(factor).unwrap()).unwrap();
            app.paint(&surface);
        }
        // The global bar takes the in-window menu bar away.
        app.event(&Event::GlobalMenu(true), &mut control);
        assert_eq!(app.content().y, 16);
        app.event(&Event::GlobalMenu(false), &mut control);
        assert_eq!(app.content().y, 36);
        // File > Close Window quits.
        app.event(&Event::MenuChosen { menu: 0, item: 0 }, &mut control);
        assert!(control.quit_requested());
    }
}
