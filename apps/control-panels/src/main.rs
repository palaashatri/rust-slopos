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
use slopos_ui::controls::dialog::{Dialog, DialogOutcome};
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
const JOIN_ID: u64 = 24;
const DISCONNECT_ID: u64 = 25;
const FORGET_ID: u64 = 26;
const DIALOG_ID: u64 = 600;
const WIFI_LIST_ID: u64 = 23;
const OUTPUT_VOLUME_ID: u64 = 30;
const OUTPUT_MUTE_ID: u64 = 31;
const INPUT_VOLUME_ID: u64 = 32;
const INPUT_MUTE_ID: u64 = 33;
const BRIGHTNESS_ID: u64 = 40;
const BLUETOOTH_ID: u64 = 50;
const BT_SEARCH_ID: u64 = 51;
const BT_LIST_ID: u64 = 52;
const BT_PAIR_ID: u64 = 53;
const BT_CONNECT_ID: u64 = 54;
const BT_FORGET_ID: u64 = 55;
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
    join: Button,
    disconnect: Button,
    forget: Button,
    /// Saved Wi-Fi profile names, from NetworkManager.
    saved_wifi: Vec<String>,
    /// The Wi-Fi password prompt, for the named network.
    password: Option<(Dialog, String)>,
    output_volume: Slider,
    output_mute: Checkbox,
    input_volume: Slider,
    input_mute: Checkbox,
    brightness_slider: Slider,
    bluetooth_power: Checkbox,
    bt_search: Button,
    bt_list: ListView,
    bt_pair: Button,
    bt_connect: Button,
    bt_forget: Button,
    /// The device being paired, until BlueZ answers.
    pairing: Option<String>,
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
            join: Button::new(JOIN_ID, "Join", ButtonPlateVariant::Default),
            disconnect: Button::new(DISCONNECT_ID, "Disconnect", ButtonPlateVariant::Regular),
            forget: Button::new(FORGET_ID, "Forget", ButtonPlateVariant::Regular),
            saved_wifi: Vec::new(),
            password: None,
            output_volume: Slider::new(OUTPUT_VOLUME_ID, "Output volume", 0.0, 100.0, 5.0),
            output_mute: Checkbox::new(OUTPUT_MUTE_ID, "Mute output", false),
            input_volume: Slider::new(INPUT_VOLUME_ID, "Input volume", 0.0, 100.0, 5.0),
            input_mute: Checkbox::new(INPUT_MUTE_ID, "Mute input", false),
            brightness_slider: Slider::new(BRIGHTNESS_ID, "Brightness", 0.0, 100.0, 5.0),
            bluetooth_power: Checkbox::new(BLUETOOTH_ID, "Bluetooth", false),
            bt_search: Button::new(
                BT_SEARCH_ID,
                "Search for Devices",
                ButtonPlateVariant::Regular,
            ),
            bt_list: ListView::new(BT_LIST_ID, "Bluetooth devices", 20),
            bt_pair: Button::new(BT_PAIR_ID, "Pair", ButtonPlateVariant::Default),
            bt_connect: Button::new(BT_CONNECT_ID, "Connect", ButtonPlateVariant::Regular),
            bt_forget: Button::new(BT_FORGET_ID, "Forget", ButtonPlateVariant::Regular),
            pairing: None,
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
        let list_height = c.height.saturating_sub(172);
        self.wifi_list.set_bounds(Rect {
            x: x + 1,
            y: top + 32,
            width: c.width.saturating_sub(2),
            height: list_height,
        });
        let buttons_y = top + 40 + list_height as i32;
        let mut button_x = x + c.width as i32;
        for button in [&mut self.forget, &mut self.disconnect, &mut self.join] {
            button_x -= 96;
            button.bounds = Rect {
                x: button_x,
                y: buttons_y,
                width: 90,
                height: 20,
            };
        }
        if let Some((dialog, _)) = &mut self.password {
            dialog.layout(self.window);
        }
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
        self.bt_search.bounds = Rect {
            x: x + 140,
            y: top - 1,
            width: 150,
            height: 20,
        };
        self.bt_list.set_bounds(Rect {
            x: x + 1,
            y: top + 32,
            width: c.width.saturating_sub(2),
            height: list_height,
        });
        let mut button_x = x + c.width as i32;
        for button in [&mut self.bt_forget, &mut self.bt_connect, &mut self.bt_pair] {
            button_x -= 132;
            button.bounds = Rect {
                x: button_x,
                y: buttons_y,
                width: 126,
                height: 20,
            };
        }
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
            Panel::Network => vec![
                NETWORKING_ID,
                WIFI_ID,
                SCAN_ID,
                WIFI_LIST_ID,
                JOIN_ID,
                DISCONNECT_ID,
                FORGET_ID,
            ],
            Panel::Sound => vec![
                OUTPUT_VOLUME_ID,
                OUTPUT_MUTE_ID,
                INPUT_VOLUME_ID,
                INPUT_MUTE_ID,
            ],
            Panel::Display => vec![BRIGHTNESS_ID],
            Panel::Bluetooth => vec![
                BLUETOOTH_ID,
                BT_SEARCH_ID,
                BT_LIST_ID,
                BT_PAIR_ID,
                BT_CONNECT_ID,
                BT_FORGET_ID,
            ],
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
        self.join.focused = f == Some(JOIN_ID);
        self.disconnect.focused = f == Some(DISCONNECT_ID);
        self.forget.focused = f == Some(FORGET_ID);
        self.output_volume.focused = f == Some(OUTPUT_VOLUME_ID);
        self.output_mute.focused = f == Some(OUTPUT_MUTE_ID);
        self.input_volume.focused = f == Some(INPUT_VOLUME_ID);
        self.input_mute.focused = f == Some(INPUT_MUTE_ID);
        self.brightness_slider.focused = f == Some(BRIGHTNESS_ID);
        self.bluetooth_power.focused = f == Some(BLUETOOTH_ID);
        self.bt_search.focused = f == Some(BT_SEARCH_ID);
        self.bt_list.focused = f == Some(BT_LIST_ID);
        self.bt_pair.focused = f == Some(BT_PAIR_ID);
        self.bt_connect.focused = f == Some(BT_CONNECT_ID);
        self.bt_forget.focused = f == Some(BT_FORGET_ID);
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
                self.sync_wifi_buttons();
            }
            Err(_) => {
                self.networking.enabled = false;
                self.wifi.enabled = false;
                self.scan.enabled = false;
                self.wifi_list.reset(0);
                self.join.enabled = false;
                self.disconnect.enabled = false;
                self.forget.enabled = false;
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
        self.bluetooth_power.checked = adapter.as_ref().is_some_and(|a| a.powered);
        let powered = adapter.as_ref().is_some_and(|a| a.powered);
        let searching = adapter.as_ref().is_some_and(|a| a.discovering);
        self.bt_search.enabled = powered;
        self.bt_search.label = if searching {
            "Stop Searching".into()
        } else {
            "Search for Devices".into()
        };
        let devices = self
            .bluetooth
            .as_ref()
            .map(model::bluetooth_devices)
            .unwrap_or_default();
        if self.bt_list.len() != devices.len() {
            self.bt_list.reset(devices.len());
        }
        let selected = self.bt_list.cursor().and_then(|i| devices.get(i));
        let busy = self.pairing.is_some();
        self.bt_pair.label = if busy {
            "Cancel Pairing".into()
        } else {
            "Pair".into()
        };
        self.bt_pair.enabled = busy || (powered && selected.is_some_and(|d| !d.paired));
        self.bt_connect.label = if selected.is_some_and(|d| d.connected) {
            "Disconnect".into()
        } else {
            "Connect".into()
        };
        self.bt_connect.enabled = !busy && powered && selected.is_some_and(|d| d.paired);
        self.bt_forget.enabled = !busy && selected.is_some_and(|d| d.paired);
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
            Update::SavedWifi(names) => self.saved_wifi = names,
            Update::PairingDone(device, error) => {
                self.pairing = None;
                let name = self.bluetooth_name(&device);
                match error {
                    None => self.notice(format!("Paired with “{name}”.")),
                    Some(error) => self.fail(format!("Couldn't pair with “{name}”: {error}")),
                }
            }
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

    /// The selected Bluetooth device.
    fn selected_device(&self) -> Option<slopos_services::bluetooth::BluetoothDevice> {
        let devices = model::bluetooth_devices(self.bluetooth.as_ref().ok()?);
        devices.get(self.bt_list.cursor()?).cloned()
    }

    fn bluetooth_name(&self, path: &str) -> String {
        self.bluetooth
            .as_ref()
            .ok()
            .and_then(|s| s.devices.iter().find(|d| d.path == path))
            .map_or_else(|| "the device".to_owned(), |d| d.name.clone())
    }

    /// Join, Disconnect and Forget follow the selected network.
    fn sync_wifi_buttons(&mut self) {
        let target = self.selected_wifi();
        self.join.enabled = target
            .as_ref()
            .is_some_and(|t| t.active_connection.is_none());
        self.disconnect.enabled = target
            .as_ref()
            .is_some_and(|t| t.active_connection.is_some());
        self.forget.enabled = self
            .selected_wifi_name()
            .is_some_and(|name| self.saved_wifi.contains(&name));
    }

    /// The network selected in the Wi-Fi list.
    fn selected_wifi_name(&self) -> Option<String> {
        let networks = self.network.as_ref().ok().map(model::wifi_networks)?;
        networks
            .get(self.wifi_list.cursor()?)
            .map(|network| network.0.clone())
    }

    fn selected_wifi(&self) -> Option<model::WifiTarget> {
        let name = self.selected_wifi_name()?;
        model::wifi_target(self.network.as_ref().ok()?, &name)
    }

    /// Join the selected network: secured networks without a saved profile
    /// ask for the password first.
    fn join_selected(&mut self) {
        let (Some(name), Some(target)) = (self.selected_wifi_name(), self.selected_wifi()) else {
            return;
        };
        if target.secured && !self.saved_wifi.contains(&name) {
            let mut dialog = Dialog::new(
                DIALOG_ID,
                &format!("Join “{name}”"),
                &format!("Enter the password for the Wi-Fi network “{name}”."),
                &["Cancel", "Join"],
            )
            .with_field("Password", "");
            if let Some(field) = &mut dialog.field {
                field.secret = true;
            }
            dialog.layout(self.window);
            self.password = Some((dialog, name));
            return;
        }
        self.send(Command::JoinWifi(
            target.device,
            target.access_point,
            target.ssid,
            None,
        ));
        self.notice(format!("Joining “{name}”…"));
    }

    fn handle(&mut self, event: &Event, control: &mut Control) {
        if let Some((dialog, _)) = &mut self.password {
            match dialog.handle(event) {
                DialogOutcome::Chosen { index, text } => {
                    let (_, name) = self.password.take().expect("prompt is open");
                    let target = self
                        .network
                        .as_ref()
                        .ok()
                        .and_then(|snapshot| model::wifi_target(snapshot, &name));
                    match (index, text, target) {
                        (1, Some(password), Some(target)) => {
                            if let Err(error) = slopos_services::network::wifi_settings(
                                &name,
                                &target.ssid,
                                Some(&password),
                            ) {
                                return self.fail(error.to_string());
                            }
                            self.send(Command::JoinWifi(
                                target.device,
                                target.access_point,
                                target.ssid,
                                Some(password),
                            ));
                            self.notice(format!("Joining “{name}”…"));
                        }
                        (1, _, None) => self.fail(format!("“{name}” is no longer in range.")),
                        _ => {}
                    }
                }
                DialogOutcome::PasteRequested => control.request_paste(),
                _ => {}
            }
            return;
        }
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
                match self.wifi_list.handle(event, control.modifiers(), &label) {
                    ListOutcome::FocusRequested => {
                        self.focus.focus(WIFI_LIST_ID);
                        self.sync_focus();
                    }
                    ListOutcome::Activated(_) => self.join_selected(),
                    _ => {}
                }
                self.sync_wifi_buttons();
                if self.join.handle(event) == ButtonOutcome::Activated
                    || (clicked(JOIN_ID) && self.join.enabled)
                {
                    self.join_selected();
                }
                if self.disconnect.handle(event) == ButtonOutcome::Activated
                    || (clicked(DISCONNECT_ID) && self.disconnect.enabled)
                {
                    if let Some(active) = self.selected_wifi().and_then(|t| t.active_connection) {
                        self.send(Command::Disconnect(active));
                    }
                }
                if self.forget.handle(event) == ButtonOutcome::Activated
                    || (clicked(FORGET_ID) && self.forget.enabled)
                {
                    if let Some(name) = self.selected_wifi_name() {
                        self.send(Command::ForgetWifi(name.clone()));
                        self.notice(format!("Forgetting “{name}”…"));
                    }
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
                let adapter = self
                    .bluetooth
                    .as_ref()
                    .ok()
                    .and_then(|s| s.adapters.first().cloned());
                if (self.bt_search.handle(event) == ButtonOutcome::Activated
                    || (clicked(BT_SEARCH_ID) && self.bt_search.enabled))
                    && self.bt_search.enabled
                {
                    if let Some(adapter) = &adapter {
                        self.send(Command::SetBluetoothDiscovery(
                            adapter.path.clone(),
                            !adapter.discovering,
                        ));
                    }
                }
                let devices = self
                    .bluetooth
                    .as_ref()
                    .map(model::bluetooth_devices)
                    .unwrap_or_default();
                let label = |i: usize| {
                    devices
                        .get(i)
                        .map(|d| format!("{}, {}", d.name, model::bluetooth_state(d)))
                        .unwrap_or_default()
                };
                let mut activated = false;
                match self.bt_list.handle(event, control.modifiers(), &label) {
                    ListOutcome::FocusRequested => {
                        self.focus.focus(BT_LIST_ID);
                        self.sync_focus();
                    }
                    ListOutcome::Activated(_) => activated = true,
                    _ => {}
                }
                self.sync_from_state();
                let selected = self.selected_device();
                if self.bt_pair.handle(event) == ButtonOutcome::Activated
                    || (clicked(BT_PAIR_ID) && self.bt_pair.enabled)
                    || (activated && self.bt_pair.enabled && self.pairing.is_none())
                {
                    if let Some(device) = self.pairing.clone() {
                        self.send(Command::CancelBluetoothPairing(device));
                        self.notice("Cancelling pairing…".to_owned());
                    } else if let Some(device) = selected.clone() {
                        self.send(Command::PairBluetooth(device.path.clone()));
                        self.notice(format!(
                            "Pairing with “{}”… Confirm on the device if it asks.",
                            device.name
                        ));
                        self.pairing = Some(device.path);
                    }
                } else if (self.bt_connect.handle(event) == ButtonOutcome::Activated
                    || (clicked(BT_CONNECT_ID) && self.bt_connect.enabled)
                    || (activated && self.bt_connect.enabled))
                    && self.bt_connect.enabled
                {
                    if let Some(device) = selected.clone() {
                        if device.connected {
                            self.send(Command::DisconnectBluetooth(device.path));
                        } else {
                            self.send(Command::ConnectBluetooth(device.path));
                            self.notice(format!("Connecting to “{}”…", device.name));
                        }
                    }
                }
                if self.bt_forget.handle(event) == ButtonOutcome::Activated
                    || (clicked(BT_FORGET_ID) && self.bt_forget.enabled)
                {
                    if let Some(device) = selected {
                        self.send(Command::ForgetBluetooth(
                            device.adapter_path.clone(),
                            device.path.clone(),
                        ));
                        self.notice(format!("Forgetting “{}”…", device.name));
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
                let connected: Vec<bool> = networks
                    .iter()
                    .map(|(name, _, _)| {
                        self.network
                            .as_ref()
                            .ok()
                            .and_then(|s| model::wifi_target(s, name))
                            .is_some_and(|t| t.active_connection.is_some())
                    })
                    .collect();
                let saved = &self.saved_wifi;
                self.wifi_list
                    .paint(surface, |surface, i, rect, selected| {
                        let (name, strength, secured) = &networks[i];
                        let ink = if selected {
                            colors.selection_ink
                        } else {
                            colors.ink
                        };
                        if connected[i] {
                            surface.draw_text("✓", rect.x + 6, rect.y + 2, font, ink)?;
                        }
                        let mut text = format!("{name}  —  {strength}%");
                        if *secured {
                            text.push_str("  (secured)");
                        }
                        if saved.contains(name) {
                            text.push_str("  · saved");
                        }
                        surface.draw_text(&text, rect.x + 22, rect.y + 2, font, ink)
                    })?;
                self.join.paint(surface, font)?;
                self.disconnect.paint(surface, font)?;
                self.forget.paint(surface, font)?;
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
            Panel::Bluetooth => {
                self.bluetooth_power.paint(surface)?;
                self.bt_search.paint(surface, font)?;
                let devices = self
                    .bluetooth
                    .as_ref()
                    .map(model::bluetooth_devices)
                    .unwrap_or_default();
                let pairing = self.pairing.as_deref();
                let list = self.bt_list.bounds();
                self.bt_list.paint(surface, |surface, i, rect, selected| {
                    let device = &devices[i];
                    let ink = if selected {
                        colors.selection_ink
                    } else {
                        colors.ink
                    };
                    let soft = if selected {
                        colors.selection_ink
                    } else {
                        colors.ink_secondary
                    };
                    surface.draw_text(&device.name, rect.x + 8, rect.y + 2, font, ink)?;
                    let state = if pairing == Some(device.path.as_str()) {
                        "Pairing…"
                    } else {
                        model::bluetooth_state(device)
                    };
                    surface.draw_text(
                        state,
                        rect.x + list.width as i32 - 110,
                        rect.y + 2,
                        font,
                        soft,
                    )
                })?;
                if devices.is_empty() {
                    surface.draw_text(
                        "No Bluetooth devices to show",
                        list.x + 8,
                        list.y + 6,
                        font,
                        colors.ink_secondary,
                    )?;
                }
                self.bt_pair.paint(surface, font)?;
                self.bt_connect.paint(surface, font)?;
                self.bt_forget.paint(surface, font)?;
            }
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
        if let Some((dialog, _)) = &mut self.password {
            dialog.paint(surface)?;
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
                nodes.push((JOIN_ID, self.join.accessibility_node()));
                nodes.push((DISCONNECT_ID, self.disconnect.accessibility_node()));
                nodes.push((FORGET_ID, self.forget.accessibility_node()));
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
                nodes.push((BLUETOOTH_ID, self.bluetooth_power.accessibility_node()));
                nodes.push((BT_SEARCH_ID, self.bt_search.accessibility_node()));
                nodes.push((BT_PAIR_ID, self.bt_pair.accessibility_node()));
                nodes.push((BT_CONNECT_ID, self.bt_connect.accessibility_node()));
                nodes.push((BT_FORGET_ID, self.bt_forget.accessibility_node()));
                let devices = self
                    .bluetooth
                    .as_ref()
                    .map(model::bluetooth_devices)
                    .unwrap_or_default();
                let label = |i: usize| {
                    devices
                        .get(i)
                        .map(|d| format!("{}, {}", d.name, model::bluetooth_state(d)))
                        .unwrap_or_default()
                };
                nodes.extend(self.bt_list.accessibility_nodes(&label))
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
        if let Some((dialog, _)) = &self.password {
            nodes.extend(dialog.accessibility_nodes());
            let focus = Some(dialog.focus_node());
            return Some(window_tree("Control Panels", nodes, focus));
        }
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
                Some(WIFI_LIST_ID) => Some(self.wifi_list.focus_node()),
                Some(BT_LIST_ID) => Some(self.bt_list.focus_node()),
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
    fn wifi_join_asks_for_a_password_and_sends_it_to_the_provider() {
        use slopos_services::network::{NetworkAccessPoint, NetworkDevice, NetworkDeviceState};
        let (commands, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let (_sender, updates) = std::sync::mpsc::channel();
        let mut app = ControlPanels::new(Some(Services::for_tests(commands, updates)));
        let ap = |path: &str, ssid: &[u8], strength, rsn_flags| NetworkAccessPoint {
            object_path: path.into(),
            ssid: ssid.to_vec(),
            strength,
            frequency_mhz: 2412,
            wpa_flags: 0,
            rsn_flags,
        };
        let mut snapshot = NetworkSnapshot {
            revision: 1,
            networking_enabled: true,
            wireless_enabled: true,
            wireless_hardware_enabled: true,
            devices: vec![NetworkDevice {
                object_path: "/d/1".into(),
                interface: "wlan0".into(),
                ip_interface: "wlan0".into(),
                kind: NetworkDeviceType::Wifi,
                state: NetworkDeviceState::Disconnected,
                active_access_point: None,
                access_points: vec![ap("/a/1", b"Home", 70, 8), ap("/a/2", b"Cafe", 50, 0)],
                active_connection: None,
            }],
            active_connections: Vec::new(),
        };
        app.apply_update(Update::Network(Ok(snapshot.clone())));
        app.sync_from_state();
        let mut control = Control::default();
        assert_eq!(app.panel(), Panel::Network);
        // Nothing selected: nothing to join.
        assert!(!app.join.enabled && !app.disconnect.enabled && !app.forget.enabled);
        // Sidebar, Networking, Wi-Fi, Scan, then the network list.
        for _ in 0..5 {
            app.event(&key(Key::Tab), &mut control);
        }
        assert!(app.wifi_list.focused);
        app.event(&key(Key::Down), &mut control);
        assert_eq!(app.selected_wifi_name().as_deref(), Some("Home"));
        assert!(app.join.enabled && !app.disconnect.enabled && !app.forget.enabled);
        // A secured network without a saved profile asks for the password,
        // which is masked and never in the accessibility tree.
        app.event(&key(Key::Enter), &mut control);
        assert!(app.password.is_some());
        for c in "hunter22".chars() {
            app.event(&key(Key::Character(c)), &mut control);
        }
        let tree = app.accessibility().unwrap();
        assert!(tree
            .nodes
            .iter()
            .all(|(_, n)| n.value() != Some("hunter22")));
        assert!(tree
            .nodes
            .iter()
            .any(|(_, n)| n.role() == accesskit::Role::PasswordInput));
        for factor in [1, 2] {
            let surface = RasterSurface::new(WIDTH, HEIGHT, Scale::new(factor).unwrap()).unwrap();
            app.paint(&surface);
        }
        app.event(&key(Key::Enter), &mut control);
        assert!(app.password.is_none());
        assert_eq!(
            receiver.try_recv().unwrap(),
            Command::JoinWifi(
                "/d/1".into(),
                "/a/1".into(),
                b"Home".to_vec(),
                Some("hunter22".into())
            )
        );
        // A too-short password is refused before it reaches NetworkManager.
        app.event(&key(Key::Enter), &mut control);
        app.event(&key(Key::Character('x')), &mut control);
        app.event(&key(Key::Enter), &mut control);
        assert!(app.status_error);
        assert!(receiver.try_recv().is_err());
        // Once joined and saved: Disconnect and Forget, no Join.
        snapshot.devices[0].active_access_point = Some("/a/1".into());
        snapshot.devices[0].active_connection = Some("/ac/1".into());
        app.apply_update(Update::Network(Ok(snapshot)));
        app.apply_update(Update::SavedWifi(vec!["Home".into()]));
        app.sync_from_state();
        assert!(!app.join.enabled && app.disconnect.enabled && app.forget.enabled);
        app.event(&key(Key::Tab), &mut control);
        app.event(&key(Key::Tab), &mut control);
        assert!(app.disconnect.focused);
        app.event(&key(Key::Space), &mut control);
        assert_eq!(
            receiver.try_recv().unwrap(),
            Command::Disconnect("/ac/1".into())
        );
        app.event(&key(Key::Tab), &mut control);
        app.event(&key(Key::Space), &mut control);
        assert_eq!(
            receiver.try_recv().unwrap(),
            Command::ForgetWifi("Home".into())
        );
    }

    #[test]
    fn bluetooth_devices_are_searched_paired_connected_and_forgotten() {
        use slopos_services::bluetooth::{BluetoothAdapter, BluetoothDevice};
        let (commands, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let (_sender, updates) = std::sync::mpsc::channel();
        let mut app = ControlPanels::new(Some(Services::for_tests(commands, updates)));
        let device = |path: &str, name: &str, paired, connected| BluetoothDevice {
            path: path.into(),
            name: name.into(),
            address: name.into(),
            adapter_path: "/hci0".into(),
            paired,
            trusted: paired,
            connected,
        };
        let mut snapshot = BluetoothSnapshot {
            revision: 1,
            availability: Availability::Available,
            adapters: vec![BluetoothAdapter {
                path: "/hci0".into(),
                name: "slopos".into(),
                address: "AA".into(),
                powered: true,
                discovering: false,
            }],
            devices: vec![
                device("/hci0/dev_1", "Speaker", false, false),
                device("/hci0/dev_2", "Mouse", true, true),
            ],
        };
        app.apply_update(Update::Bluetooth(Ok(snapshot.clone())));
        let mut control = Control::default();
        app.event(&Event::MenuChosen { menu: 1, item: 3 }, &mut control);
        assert_eq!(app.panel(), Panel::Bluetooth);
        // From the sidebar: power, then Search; Space starts searching.
        for _ in 0..2 {
            app.event(&key(Key::Tab), &mut control);
        }
        assert!(app.bt_search.focused && app.bt_search.enabled);
        app.event(&key(Key::Space), &mut control);
        assert_eq!(
            receiver.try_recv().unwrap(),
            Command::SetBluetoothDiscovery("/hci0".into(), true)
        );
        snapshot.adapters[0].discovering = true;
        app.apply_update(Update::Bluetooth(Ok(snapshot.clone())));
        app.sync_from_state();
        assert_eq!(app.bt_search.label, "Stop Searching");
        // The connected Mouse is listed first; nothing to pair there.
        app.event(&key(Key::Tab), &mut control);
        assert!(app.bt_list.focused);
        app.event(&key(Key::Down), &mut control);
        assert_eq!(app.selected_device().unwrap().name, "Mouse");
        assert!(!app.bt_pair.enabled && app.bt_connect.enabled && app.bt_forget.enabled);
        assert_eq!(app.bt_connect.label, "Disconnect");
        // The Speaker pairs with Return; Pair becomes Cancel Pairing.
        app.event(&key(Key::Down), &mut control);
        assert_eq!(app.selected_device().unwrap().name, "Speaker");
        assert!(app.bt_pair.enabled && !app.bt_connect.enabled && !app.bt_forget.enabled);
        app.event(&key(Key::Enter), &mut control);
        assert_eq!(
            receiver.try_recv().unwrap(),
            Command::PairBluetooth("/hci0/dev_1".into())
        );
        assert_eq!(app.bt_pair.label, "Cancel Pairing");
        app.event(&key(Key::Tab), &mut control);
        assert!(app.bt_pair.focused);
        app.event(&key(Key::Space), &mut control);
        assert_eq!(
            receiver.try_recv().unwrap(),
            Command::CancelBluetoothPairing("/hci0/dev_1".into())
        );
        let tree = app.accessibility().unwrap();
        assert!(tree
            .nodes
            .iter()
            .any(|(_, n)| n.label() == Some("Speaker, Not paired")));
        for factor in [1, 2] {
            let surface = RasterSurface::new(WIDTH, HEIGHT, Scale::new(factor).unwrap()).unwrap();
            app.paint(&surface);
        }
        app.apply_update(Update::PairingDone(
            "/hci0/dev_1".into(),
            Some("Authentication Canceled".into()),
        ));
        assert!(app.status_error && app.status.contains("Speaker"));
        // Paired and connected: Disconnect, then Forget.
        snapshot.devices[0].paired = true;
        snapshot.devices[0].connected = true;
        app.apply_update(Update::Bluetooth(Ok(snapshot)));
        app.apply_update(Update::PairingDone("/hci0/dev_1".into(), None));
        app.sync_from_state();
        assert_eq!(app.status, "Paired with “Speaker”.");
        assert_eq!(app.selected_device().unwrap().name, "Speaker");
        app.event(&key(Key::Tab), &mut control);
        assert!(app.bt_connect.focused);
        app.event(&key(Key::Space), &mut control);
        assert_eq!(
            receiver.try_recv().unwrap(),
            Command::DisconnectBluetooth("/hci0/dev_1".into())
        );
        app.event(&key(Key::Tab), &mut control);
        app.event(&key(Key::Space), &mut control);
        assert_eq!(
            receiver.try_recv().unwrap(),
            Command::ForgetBluetooth("/hci0".into(), "/hci0/dev_1".into())
        );
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
