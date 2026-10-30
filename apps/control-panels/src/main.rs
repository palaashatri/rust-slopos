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
use slopos_services::display::{self, DisplaySnapshot, OutputConfig, Placement};
use slopos_services::network::{NetworkDeviceType, NetworkSnapshot};
use slopos_services::timedate::TimedateSnapshot;
use slopos_ui::accessibility::window_tree;
use slopos_ui::controls::button::{Button, ButtonOutcome};
use slopos_ui::controls::dialog::{Dialog, DialogOutcome};
use slopos_ui::controls::file_chooser::{ChooserOutcome, FileChooser};
use slopos_ui::controls::focus::FocusChain;
use slopos_ui::controls::list::{ListOutcome, ListView};
use slopos_ui::controls::menu::{MenuBar, MenuBarOutcome, MenuItem};
use slopos_ui::controls::popup::{PopUpButton, PopUpOutcome};
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
const OUTPUT_DEVICE_ID: u64 = 34;
const INPUT_DEVICE_ID: u64 = 35;
const APP_ID: u64 = 36;
const APP_VOLUME_ID: u64 = 37;
const APP_MUTE_ID: u64 = 38;
const APP_OUTPUT_ID: u64 = 39;
const BRIGHTNESS_ID: u64 = 40;
const OUTPUT_ID: u64 = 41;
const RESOLUTION_ID: u64 = 42;
const REFRESH_ID: u64 = 43;
const ROTATION_ID: u64 = 44;
const PLACEMENT_ID: u64 = 45;
const DISPLAY_ON_ID: u64 = 46;
const MAIN_DISPLAY_ID: u64 = 47;
const KEEP_DIALOG_ID: u64 = 610;
const REVERT_TIMER: u64 = 1;
const DISPLAY_POPUPS: [u64; 5] = [
    OUTPUT_ID,
    RESOLUTION_ID,
    REFRESH_ID,
    ROTATION_ID,
    PLACEMENT_ID,
];
const SOUND_POPUPS: [u64; 4] = [OUTPUT_DEVICE_ID, INPUT_DEVICE_ID, APP_ID, APP_OUTPUT_ID];
/// Seconds to confirm a display change before it is undone.
const REVERT_SECONDS: u32 = 15;

/// A display change on probation: undone unless kept in time.
struct PendingDisplays {
    previous: Vec<OutputConfig>,
    configs: Vec<OutputConfig>,
    left: u32,
    /// The server took it (the countdown runs from then).
    applied: bool,
    dialog: Dialog,
}
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
    output_device: PopUpButton,
    input_device: PopUpButton,
    /// The application whose sound the controls below it change.
    app: PopUpButton,
    app_volume: Slider,
    app_mute: Checkbox,
    app_output: PopUpButton,
    brightness_slider: Slider,
    displays: State<DisplaySnapshot>,
    /// The output the Displays controls edit.
    output: PopUpButton,
    resolution: PopUpButton,
    refresh: PopUpButton,
    rotation: PopUpButton,
    placement: PopUpButton,
    display_on: Checkbox,
    main_display: Checkbox,
    /// A display change waiting to be kept or reverted.
    keep: Option<PendingDisplays>,
    /// The change went through: start counting down.
    start_countdown: bool,
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
            output_device: PopUpButton::new(OUTPUT_DEVICE_ID, "Output device"),
            input_device: PopUpButton::new(INPUT_DEVICE_ID, "Input device"),
            app: PopUpButton::new(APP_ID, "Application"),
            app_volume: Slider::new(APP_VOLUME_ID, "Application volume", 0.0, 100.0, 5.0),
            app_mute: Checkbox::new(APP_MUTE_ID, "Mute application", false),
            app_output: PopUpButton::new(APP_OUTPUT_ID, "Play on"),
            brightness_slider: Slider::new(BRIGHTNESS_ID, "Brightness", 0.0, 100.0, 5.0),
            displays: model::loading(),
            output: PopUpButton::new(OUTPUT_ID, "Display"),
            resolution: PopUpButton::new(RESOLUTION_ID, "Resolution"),
            refresh: PopUpButton::new(REFRESH_ID, "Refresh rate"),
            rotation: PopUpButton::new(ROTATION_ID, "Rotation"),
            placement: PopUpButton::new(PLACEMENT_ID, "Position"),
            display_on: Checkbox::new(DISPLAY_ON_ID, "Use this display", false),
            main_display: Checkbox::new(MAIN_DISPLAY_ID, "Show the menu bar here", false),
            keep: None,
            start_countdown: false,
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
        let popup_at = |y: i32| Rect {
            x: x + 110,
            y,
            width: 220,
            height: 20,
        };
        let slider_at = |y: i32| Rect {
            x: x + 110,
            y,
            width: 220,
            height: 18,
        };
        self.output_device.bounds = popup_at(top);
        self.output_volume.bounds = slider_at(top + 30);
        self.output_mute = self.output_mute.clone().at(x + 350, top + 30);
        self.input_device.bounds = popup_at(top + 64);
        self.input_volume.bounds = slider_at(top + 94);
        self.input_mute = self.input_mute.clone().at(x + 350, top + 94);
        self.app.bounds = popup_at(top + 132);
        self.app_volume.bounds = slider_at(top + 162);
        self.app_mute = self.app_mute.clone().at(x + 350, top + 162);
        self.app_output.bounds = popup_at(top + 192);
        self.brightness_slider.bounds = Rect {
            x: x + 110,
            y: top,
            width: 220,
            height: 18,
        };
        for (row, popup) in [
            &mut self.output,
            &mut self.resolution,
            &mut self.refresh,
            &mut self.rotation,
            &mut self.placement,
        ]
        .into_iter()
        .enumerate()
        {
            popup.bounds = Rect {
                x: x + 110,
                y: top + 34 + row as i32 * 30,
                width: 220,
                height: 20,
            };
        }
        self.display_on = self.display_on.clone().at(x + 110, top + 186);
        self.main_display = self.main_display.clone().at(x + 110, top + 210);
        if let Some(pending) = &mut self.keep {
            pending.dialog.layout(self.window);
        }
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
                OUTPUT_DEVICE_ID,
                OUTPUT_VOLUME_ID,
                OUTPUT_MUTE_ID,
                INPUT_DEVICE_ID,
                INPUT_VOLUME_ID,
                INPUT_MUTE_ID,
                APP_ID,
                APP_VOLUME_ID,
                APP_MUTE_ID,
                APP_OUTPUT_ID,
            ],
            Panel::Display => vec![
                BRIGHTNESS_ID,
                OUTPUT_ID,
                RESOLUTION_ID,
                REFRESH_ID,
                ROTATION_ID,
                PLACEMENT_ID,
                DISPLAY_ON_ID,
                MAIN_DISPLAY_ID,
            ],
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
        // A pop-up with nothing to choose is skipped, as disabled controls
        // are on the Classic desktop.
        ids.retain(|&id| {
            !(DISPLAY_POPUPS.contains(&id) || SOUND_POPUPS.contains(&id)) || self.popup(id).enabled
        });
        ids
    }

    fn popup(&self, id: u64) -> &PopUpButton {
        match id {
            OUTPUT_DEVICE_ID => &self.output_device,
            INPUT_DEVICE_ID => &self.input_device,
            APP_ID => &self.app,
            APP_OUTPUT_ID => &self.app_output,
            OUTPUT_ID => &self.output,
            RESOLUTION_ID => &self.resolution,
            REFRESH_ID => &self.refresh,
            ROTATION_ID => &self.rotation,
            _ => &self.placement,
        }
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
        self.output_device.focused = f == Some(OUTPUT_DEVICE_ID);
        self.input_device.focused = f == Some(INPUT_DEVICE_ID);
        self.app.focused = f == Some(APP_ID);
        self.app_volume.focused = f == Some(APP_VOLUME_ID);
        self.app_mute.focused = f == Some(APP_MUTE_ID);
        self.app_output.focused = f == Some(APP_OUTPUT_ID);
        self.brightness_slider.focused = f == Some(BRIGHTNESS_ID);
        self.output.focused = f == Some(OUTPUT_ID);
        self.resolution.focused = f == Some(RESOLUTION_ID);
        self.refresh.focused = f == Some(REFRESH_ID);
        self.rotation.focused = f == Some(ROTATION_ID);
        self.placement.focused = f == Some(PLACEMENT_ID);
        self.display_on.focused = f == Some(DISPLAY_ON_ID);
        self.main_display.focused = f == Some(MAIN_DISPLAY_ID);
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
        self.sync_sound_choices();
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
        self.sync_displays();
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
            Update::Displays(state) => self.displays = state,
            Update::DisplaysApplied(error) => match (error, self.keep.as_mut()) {
                (None, Some(pending)) if !pending.applied => {
                    pending.applied = true;
                    self.start_countdown = true;
                }
                (None, _) => {}
                (Some(error), _) => {
                    // Nothing changed; there is nothing to keep.
                    self.keep = None;
                    self.fail(format!("The display change didn't work: {error}"));
                }
            },
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

    /// The output being edited and the current layout.
    fn display_state(&self) -> Option<(usize, Vec<OutputConfig>)> {
        let snapshot = self.displays.as_ref().ok()?;
        let index = self
            .output
            .selected()
            .filter(|&i| i < snapshot.outputs.len())
            .or_else(|| snapshot.outputs.iter().position(|o| o.primary))
            .unwrap_or(0);
        (!snapshot.outputs.is_empty()).then(|| (index, display::current_configs(snapshot)))
    }

    /// Fill the Displays controls from the provider's snapshot.
    fn sync_displays(&mut self) {
        let Some((index, configs)) = self.display_state() else {
            for popup in [
                &mut self.output,
                &mut self.resolution,
                &mut self.refresh,
                &mut self.rotation,
                &mut self.placement,
            ] {
                popup.set_items(Vec::new(), None);
                popup.enabled = false;
            }
            self.display_on.enabled = false;
            self.main_display.enabled = false;
            return;
        };
        let Ok(snapshot) = self.displays.as_ref() else {
            return;
        };
        let output = &snapshot.outputs[index];
        let config = &configs[index];
        let busy = self.keep.is_some();
        let names: Vec<String> = snapshot.outputs.iter().map(model::output_label).collect();
        self.output.set_items(names, Some(index));
        self.output.enabled = snapshot.outputs.len() > 1;
        let sizes = display::sizes(output);
        self.resolution.set_items(
            sizes
                .iter()
                .map(|&(w, h)| model::size_label(w, h))
                .collect(),
            sizes
                .iter()
                .position(|&size| size == (config.width, config.height)),
        );
        self.resolution.enabled = config.enabled && !busy && sizes.len() > 1;
        let rates = display::refresh_rates(output, config.width, config.height);
        self.refresh.set_items(
            rates.iter().map(|&r| model::refresh_label(r)).collect(),
            rates
                .iter()
                .position(|r| r.abs_diff(config.refresh_millihz) < 50),
        );
        self.refresh.enabled = config.enabled && !busy && rates.len() > 1;
        self.rotation.set_items(
            output
                .rotations
                .iter()
                .map(|&r| model::rotation_label(r).to_owned())
                .collect(),
            output.rotations.iter().position(|&r| r == config.rotation),
        );
        self.rotation.enabled = config.enabled && !busy && output.rotations.len() > 1;
        // Placement is relative to the main display (or the first other one).
        let anchor = configs
            .iter()
            .enumerate()
            .position(|(i, c)| i != index && c.primary && c.enabled)
            .or_else(|| (0..configs.len()).find(|&i| i != index && configs[i].enabled));
        match anchor {
            Some(anchor) => {
                let name = &snapshot.outputs[anchor].name;
                self.placement.set_items(
                    Placement::ALL
                        .iter()
                        .map(|&p| model::placement_label(p, name))
                        .collect(),
                    display::placement_of(&configs, index, anchor)
                        .and_then(|p| Placement::ALL.iter().position(|&q| q == p)),
                );
                self.placement.enabled = config.enabled && !busy;
            }
            None => {
                self.placement.set_items(Vec::new(), None);
                self.placement.enabled = false;
            }
        }
        self.display_on.checked = config.enabled;
        self.display_on.enabled = !busy && snapshot.outputs.len() > 1;
        self.main_display.checked = config.primary;
        self.main_display.enabled =
            !busy && config.enabled && !config.primary && snapshot.outputs.len() > 1;
    }

    fn popup_mut(&mut self, id: u64) -> &mut PopUpButton {
        match id {
            OUTPUT_DEVICE_ID => &mut self.output_device,
            INPUT_DEVICE_ID => &mut self.input_device,
            APP_ID => &mut self.app,
            APP_OUTPUT_ID => &mut self.app_output,
            OUTPUT_ID => &mut self.output,
            RESOLUTION_ID => &mut self.resolution,
            REFRESH_ID => &mut self.refresh,
            ROTATION_ID => &mut self.rotation,
            _ => &mut self.placement,
        }
    }

    /// The user picked item `choice` of pop-up `id`, or toggled a checkbox
    /// (`choice` 1 for on).
    fn display_chosen(&mut self, id: u64, choice: usize) {
        let Some((index, before)) = self.display_state() else {
            return;
        };
        let Ok(snapshot) = self.displays.as_ref() else {
            return;
        };
        let output = &snapshot.outputs[index];
        let mut after = before.clone();
        let anchor = (0..after.len())
            .find(|&i| i != index && after[i].primary && after[i].enabled)
            .or_else(|| (0..after.len()).find(|&i| i != index && after[i].enabled));
        match id {
            RESOLUTION_ID => {
                let Some(&(width, height)) = display::sizes(output).get(choice) else {
                    return;
                };
                let refresh =
                    display::find_mode(output, width, height, after[index].refresh_millihz)
                        .map_or(0, |mode| mode.refresh_millihz);
                after[index].width = width;
                after[index].height = height;
                after[index].refresh_millihz = refresh;
                display::reflow(&before, &mut after, index);
            }
            REFRESH_ID => {
                let rates = display::refresh_rates(output, after[index].width, after[index].height);
                let Some(&rate) = rates.get(choice) else {
                    return;
                };
                after[index].refresh_millihz = rate;
            }
            ROTATION_ID => {
                let Some(&rotation) = output.rotations.get(choice) else {
                    return;
                };
                after[index].rotation = rotation;
                display::reflow(&before, &mut after, index);
            }
            PLACEMENT_ID => {
                let (Some(&placement), Some(anchor)) = (Placement::ALL.get(choice), anchor) else {
                    return;
                };
                display::place(&mut after, index, placement, anchor);
            }
            DISPLAY_ON_ID => {
                after[index].enabled = choice == 1;
                if choice == 1 {
                    if let Some(mode) = output
                        .preferred
                        .and_then(|id| output.modes.iter().find(|m| m.id == id))
                        .or(output.modes.first())
                    {
                        after[index].width = mode.width;
                        after[index].height = mode.height;
                        after[index].refresh_millihz = mode.refresh_millihz;
                    }
                    if let Some(anchor) = anchor {
                        display::place(&mut after, index, Placement::RightOf, anchor);
                    }
                } else if after[index].primary {
                    // The menu bar moves to a display that stays on.
                    after[index].primary = false;
                    if let Some(anchor) = anchor {
                        after[anchor].primary = true;
                    }
                }
                display::normalize(&mut after);
            }
            MAIN_DISPLAY_ID => {
                for (i, config) in after.iter_mut().enumerate() {
                    config.primary = i == index;
                }
            }
            _ => return,
        }
        self.change_displays(before, after);
    }

    /// The application stream the Sound controls edit.
    fn selected_stream(&self) -> Option<slopos_services::audio::AudioStream> {
        let snapshot = self.audio.as_ref().ok()?;
        snapshot.streams.get(self.app.selected()?).cloned()
    }

    /// Fill the device and application pop-ups from the sound server.
    fn sync_sound_choices(&mut self) {
        let Ok(snapshot) = self.audio.as_ref() else {
            for id in SOUND_POPUPS {
                let popup = self.popup_mut(id);
                popup.set_items(Vec::new(), None);
                popup.enabled = false;
            }
            self.app_volume.enabled = false;
            self.app_mute.enabled = false;
            return;
        };
        let names = |devices: &[slopos_services::audio::AudioDevice]| {
            devices.iter().map(model::device_name).collect::<Vec<_>>()
        };
        let output = model::default_output(snapshot).map(|d| d.name.clone());
        let input = model::default_input(snapshot).map(|d| d.name.clone());
        let outputs = names(&snapshot.outputs);
        let inputs = names(&snapshot.inputs);
        let output_index = snapshot
            .outputs
            .iter()
            .position(|d| Some(&d.name) == output.as_ref());
        let input_index = snapshot
            .inputs
            .iter()
            .position(|d| Some(&d.name) == input.as_ref());
        // Keep the chosen application while it plays.
        let chosen = self
            .app
            .selected()
            .and_then(|i| snapshot.streams.get(i))
            .map(|s| s.index);
        let streams = snapshot.streams.clone();
        let output_ids: Vec<u32> = snapshot.outputs.iter().map(|d| d.index).collect();
        let (output_count, input_count) = (snapshot.outputs.len(), snapshot.inputs.len());
        self.output_device.set_items(outputs.clone(), output_index);
        self.output_device.enabled = output_count > 1;
        self.input_device.set_items(inputs, input_index);
        self.input_device.enabled = input_count > 1;
        let selected = chosen
            .and_then(|index| streams.iter().position(|s| s.index == index))
            .or((!streams.is_empty()).then_some(0));
        self.app
            .set_items(streams.iter().map(model::stream_label).collect(), selected);
        self.app.enabled = !streams.is_empty();
        let stream = selected.map(|i| &streams[i]);
        self.app_volume.enabled = stream.is_some();
        self.app_mute.enabled = stream.is_some();
        if let Some(stream) = stream {
            if !self.app_volume.dragging() {
                self.app_volume
                    .set_value(f64::from(model::stream_percent(stream)));
            }
            self.app_mute.checked = stream.muted;
        }
        self.app_output.set_items(
            outputs,
            stream.and_then(|s| output_ids.iter().position(|&id| id == s.output)),
        );
        self.app_output.enabled = stream.is_some() && output_count > 1;
    }

    /// The user chose item `choice` of Sound pop-up `id`.
    fn sound_chosen(&mut self, id: u64, choice: usize) {
        let Ok(snapshot) = self.audio.as_ref() else {
            return;
        };
        let output = snapshot.outputs.get(choice).map(|d| d.name.clone());
        let input = snapshot.inputs.get(choice).map(|d| d.name.clone());
        match (id, output, input) {
            (OUTPUT_DEVICE_ID, Some(device), _) => self.send(Command::SetDefaultOutput(device)),
            (INPUT_DEVICE_ID, _, Some(device)) => self.send(Command::SetDefaultInput(device)),
            (APP_OUTPUT_ID, Some(device), _) => {
                if let Some(stream) = self.selected_stream() {
                    self.send(Command::MoveStream(stream.index, device));
                }
            }
            // APP_ID only changes which application the controls edit.
            _ => {}
        }
    }

    /// Apply a new layout and ask whether to keep it.
    fn change_displays(&mut self, previous: Vec<OutputConfig>, configs: Vec<OutputConfig>) {
        if previous == configs || self.keep.is_some() {
            return;
        }
        if let Ok(snapshot) = self.displays.as_ref() {
            if let Err(error) = display::plan(snapshot, &configs) {
                return self.fail(format!("Can't use that: {error}."));
            }
        }
        let mut dialog = Dialog::new(
            KEEP_DIALOG_ID,
            "Keep these display settings?",
            &model::revert_message(REVERT_SECONDS),
            &["Revert", "Keep"],
        );
        dialog.layout(self.window);
        self.keep = Some(PendingDisplays {
            previous,
            configs: configs.clone(),
            left: REVERT_SECONDS,
            applied: false,
            dialog,
        });
        self.send(Command::ApplyDisplays(configs));
        self.notice("Changing the display settings…");
    }

    /// Undo the change waiting for confirmation.
    fn revert_displays(&mut self) {
        if let Some(pending) = self.keep.take() {
            self.send(Command::ApplyDisplays(pending.previous));
            self.notice("The previous display settings are back.");
        }
    }

    fn keep_displays(&mut self) {
        let Some(PendingDisplays { configs, .. }) = self.keep.take() else {
            return;
        };
        match self
            .config
            .as_ref()
            .map(|dir| dir.join(model::DISPLAYS_FILE))
        {
            Some(path) => match display::save(&path, &configs) {
                Ok(()) => self.notice("The display settings are kept."),
                Err(error) => self.fail(format!("Kept, but couldn't save them: {error}")),
            },
            None => self.notice("Kept for this session (no settings folder)."),
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
        if let Some(pending) = &mut self.keep {
            if let Event::Timer(REVERT_TIMER) = event {
                pending.left = pending.left.saturating_sub(1);
                if pending.left == 0 {
                    return self.revert_displays();
                }
                pending.dialog.message = model::revert_message(pending.left);
                control.start_timer(std::time::Duration::from_secs(1), REVERT_TIMER);
                return;
            }
            if !pending.applied {
                // Still being applied: nothing to answer yet.
                return;
            }
            match pending.dialog.handle(event) {
                DialogOutcome::Chosen { index: 1, .. } => self.keep_displays(),
                DialogOutcome::Chosen { .. } => self.revert_displays(),
                _ => {}
            }
            return;
        }
        if let Event::Timer(REVERT_TIMER) = event {
            return;
        }
        // An open pop-up menu takes every event.
        if let Some(id) = DISPLAY_POPUPS
            .into_iter()
            .chain(SOUND_POPUPS)
            .find(|&id| self.popup_mut(id).is_open())
        {
            let window = self.window;
            if let PopUpOutcome::Selected(index) = self.popup_mut(id).handle(event, window) {
                if SOUND_POPUPS.contains(&id) {
                    self.sound_chosen(id, index);
                } else {
                    self.display_chosen(id, index);
                }
            }
            return;
        }
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
                let window = self.window;
                for id in SOUND_POPUPS {
                    self.popup_mut(id).handle(event, window);
                }
                let stream = self.selected_stream().map(|s| s.index);
                if let (ValueOutcome::Value { value, last: true }, Some(stream)) =
                    (self.app_volume.handle(event), stream)
                {
                    self.send(Command::SetStreamVolume(stream, value as u32));
                }
                if let (ToggleOutcome::Toggled(muted), Some(stream)) =
                    (self.app_mute.handle(event), stream)
                {
                    self.send(Command::SetStreamMuted(stream, muted));
                }
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
                let window = self.window;
                for id in DISPLAY_POPUPS {
                    self.popup_mut(id).handle(event, window);
                }
                if let ToggleOutcome::Toggled(on) = self.display_on.handle(event) {
                    self.display_chosen(DISPLAY_ON_ID, usize::from(on));
                }
                if let ToggleOutcome::Toggled(true) = self.main_display.handle(event) {
                    self.display_chosen(MAIN_DISPLAY_ID, 1);
                }
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
                let mut lines = match &self.displays {
                    Ok(snapshot) if !snapshot.outputs.is_empty() => snapshot
                        .outputs
                        .iter()
                        .take(2)
                        .map(|output| match output.current_mode() {
                            Some(mode) => format!(
                                "{}: {}, {}",
                                model::output_label(output),
                                model::size_label(mode.width, mode.height),
                                model::refresh_label(mode.refresh_millihz)
                            ),
                            None => format!("{}: off", model::output_label(output)),
                        })
                        .collect(),
                    _ => vec![match self.screen {
                        Some((w, h)) => format!("Screen: {w} × {h} pixels"),
                        None => "Screen size: unavailable".into(),
                    }],
                };
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
                let label = |text: &str, y: i32, enabled: bool| {
                    let ink = if enabled {
                        colors.ink
                    } else {
                        colors.ink_secondary
                    };
                    surface.draw_text(text, c.x, y + 3, font, ink)
                };
                // A device pop-up, or a line saying there is none.
                let device = |popup: &PopUpButton, none: &str| {
                    if popup.items().is_empty() {
                        surface.draw_text(
                            none,
                            popup.bounds.x,
                            popup.bounds.y + 3,
                            font,
                            colors.ink_secondary,
                        )
                    } else {
                        popup.paint(surface, font)
                    }
                };
                label(
                    "Output",
                    self.output_device.bounds.y,
                    self.output_volume.enabled,
                )?;
                device(&self.output_device, "No output device")?;
                label(
                    "Volume",
                    self.output_volume.bounds.y,
                    self.output_volume.enabled,
                )?;
                self.output_volume.paint(surface)?;
                self.output_mute.paint(surface)?;
                label(
                    "Input",
                    self.input_device.bounds.y,
                    self.input_volume.enabled,
                )?;
                device(&self.input_device, "No input device")?;
                label(
                    "Volume",
                    self.input_volume.bounds.y,
                    self.input_volume.enabled,
                )?;
                self.input_volume.paint(surface)?;
                self.input_mute.paint(surface)?;
                label("Application", self.app.bounds.y, true)?;
                if self.app.enabled {
                    self.app.paint(surface, font)?;
                    label("Volume", self.app_volume.bounds.y, true)?;
                    self.app_volume.paint(surface)?;
                    self.app_mute.paint(surface)?;
                    label("Play on", self.app_output.bounds.y, true)?;
                    self.app_output.paint(surface, font)?;
                } else {
                    surface.draw_text(
                        "No application is playing sound.",
                        self.app.bounds.x,
                        self.app.bounds.y + 3,
                        font,
                        colors.ink_secondary,
                    )?;
                }
            }
            Panel::Display => {
                surface.draw_text("Brightness", c.x, top + 2, font, colors.ink)?;
                self.brightness_slider.paint(surface)?;
                for id in DISPLAY_POPUPS {
                    let popup = match id {
                        OUTPUT_ID => &self.output,
                        RESOLUTION_ID => &self.resolution,
                        REFRESH_ID => &self.refresh,
                        ROTATION_ID => &self.rotation,
                        _ => &self.placement,
                    };
                    if id == PLACEMENT_ID && popup.items().is_empty() {
                        continue;
                    }
                    let ink = if popup.enabled {
                        colors.ink
                    } else {
                        colors.ink_secondary
                    };
                    surface.draw_text(&popup.label, c.x, popup.bounds.y + 3, font, ink)?;
                    popup.paint(surface, font)?;
                }
                if self.display_on.enabled {
                    self.display_on.paint(surface)?;
                    self.main_display.paint(surface)?;
                }
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
        for popup in [
            &self.output,
            &self.resolution,
            &self.refresh,
            &self.rotation,
            &self.placement,
            &self.output_device,
            &self.input_device,
            &self.app,
            &self.app_output,
        ] {
            popup.paint_menu(surface)?;
        }
        if let Some((dialog, _)) = &mut self.password {
            dialog.paint(surface)?;
        }
        if let Some(pending) = self.keep.as_mut().filter(|p| p.applied) {
            pending.dialog.paint(surface)?;
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
        // Controls enable and disable with the providers' state (not under
        // a dialog, which disables them only while it is up).
        let modal = self.keep.is_some() || self.password.is_some() || self.chooser.is_some();
        if !modal && self.focus.ids() != self.focus_ids().as_slice() {
            self.sync_focus();
        }
        if std::mem::take(&mut self.start_countdown) && self.keep.is_some() {
            control.start_timer(std::time::Duration::from_secs(1), REVERT_TIMER);
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
                nodes.extend(self.output_device.accessibility_nodes());
                nodes.extend(self.input_device.accessibility_nodes());
                nodes.extend(self.app.accessibility_nodes());
                nodes.push((APP_VOLUME_ID, self.app_volume.accessibility_node()));
                nodes.push((APP_MUTE_ID, self.app_mute.accessibility_node()));
                nodes.extend(self.app_output.accessibility_nodes());
            }
            Panel::Display => {
                nodes.push((BRIGHTNESS_ID, self.brightness_slider.accessibility_node()));
                for popup in [
                    &self.output,
                    &self.resolution,
                    &self.refresh,
                    &self.rotation,
                    &self.placement,
                ] {
                    nodes.extend(popup.accessibility_nodes());
                }
                nodes.push((DISPLAY_ON_ID, self.display_on.accessibility_node()));
                nodes.push((MAIN_DISPLAY_ID, self.main_display.accessibility_node()));
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
        if let Some(pending) = self.keep.as_ref().filter(|p| p.applied) {
            nodes.extend(pending.dialog.accessibility_nodes());
            let focus = Some(pending.dialog.focus_node());
            return Some(window_tree("Control Panels", nodes, focus));
        }
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
                Some(OUTPUT_DEVICE_ID) => Some(self.output_device.focus_node()),
                Some(INPUT_DEVICE_ID) => Some(self.input_device.focus_node()),
                Some(APP_ID) => Some(self.app.focus_node()),
                Some(APP_OUTPUT_ID) => Some(self.app_output.focus_node()),
                Some(id) if DISPLAY_POPUPS.contains(&id) => Some(match id {
                    OUTPUT_ID => self.output.focus_node(),
                    RESOLUTION_ID => self.resolution.focus_node(),
                    REFRESH_ID => self.refresh.focus_node(),
                    ROTATION_ID => self.rotation.focus_node(),
                    _ => self.placement.focus_node(),
                }),
                other => other,
            },
        };
        Some(window_tree("Control Panels", nodes, focus))
    }
}

/// `--apply-displays`: put back the kept layout when the same monitors are
/// connected. Nothing saved, or different monitors, is not an error.
fn apply_saved_displays() -> Result<(), String> {
    let Some(path) = wallpaper::config_dir().map(|dir| dir.join(model::DISPLAYS_FILE)) else {
        return Ok(());
    };
    let saved = display::load(&path);
    if saved.is_empty() {
        return Ok(());
    }
    let client = display::DisplayClient::connect().map_err(|e| e.to_string())?;
    let snapshot = client.snapshot().map_err(|e| e.to_string())?;
    match display::saved_for(&snapshot, &saved) {
        Some(configs) if configs != display::current_configs(&snapshot) => client
            .apply(&configs)
            .map(|_| ())
            .map_err(|e| e.to_string()),
        _ => Ok(()),
    }
}

fn main() {
    if std::env::var_os("RUST_LOG").is_some() {
        let _ = env_logger::try_init();
    }
    if std::env::args().nth(1).as_deref() == Some("--apply-displays") {
        std::process::exit(match apply_saved_displays() {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("slopos-control-panels: {error}");
                1
            }
        });
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
            streams: Vec::new(),
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
    fn display_changes_are_confirmed_or_reverted() {
        use slopos_services::display::{DisplayMode, DisplayOutput, Rotation};
        let (commands, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let (_sender, updates) = std::sync::mpsc::channel();
        let mut app = ControlPanels::new(Some(Services::for_tests(commands, updates)));
        let dir = tempfile::tempdir().unwrap();
        app.config = Some(dir.path().to_owned());
        let mode = |id, width, height, refresh_millihz| DisplayMode {
            id,
            width,
            height,
            refresh_millihz,
        };
        let output = |id: u32, name: &str, crtc: u32, x: i16| DisplayOutput {
            id,
            name: name.into(),
            enabled: true,
            primary: id == 1,
            modes: vec![
                mode(10, 1920, 1080, 60000),
                mode(11, 1920, 1080, 50000),
                mode(12, 1280, 720, 60000),
            ],
            preferred: Some(10),
            mode: Some(10),
            rotation: Rotation::Normal,
            rotations: vec![Rotation::Normal, Rotation::Left],
            x,
            y: 0,
            crtc,
            crtcs: vec![100, 200],
            width_mm: 0,
            height_mm: 0,
        };
        let snapshot = DisplaySnapshot {
            availability: Availability::Available,
            outputs: vec![output(1, "eDP-1", 100, 0), output(2, "HDMI-1", 200, 1920)],
            min_size: (320, 200),
            max_size: (8192, 8192),
        };
        app.apply_update(Update::Displays(Ok(snapshot.clone())));
        let mut control = Control::default();
        app.event(&Event::MenuChosen { menu: 1, item: 2 }, &mut control);
        assert_eq!(app.panel(), Panel::Display);
        assert_eq!(app.lines().1[0], "eDP-1 (built-in): 1920 × 1080, 60 Hz");
        assert_eq!(app.output.value(), "eDP-1 (built-in)");
        assert_eq!(app.resolution.items(), &["1920 × 1080", "1280 × 720"]);
        assert_eq!(app.refresh.items(), &["60 Hz", "50 Hz"]);
        assert_eq!(app.placement.value(), "Left of HDMI-1");
        // Tab from the sidebar: brightness (off without a backlight is
        // still in the chain), Display, Resolution.
        for _ in 0..3 {
            app.event(&key(Key::Tab), &mut control);
        }
        assert!(app.resolution.focused);
        app.event(&key(Key::Space), &mut control);
        assert!(app.resolution.is_open());
        let tree = app.accessibility().unwrap();
        assert!(
            tree.nodes
                .iter()
                .any(|(_, n)| n.role() == accesskit::Role::ComboBox
                    && n.label() == Some("Resolution"))
        );
        app.event(&key(Key::Down), &mut control);
        app.event(&key(Key::Enter), &mut control);
        // HDMI-1 stays beside the smaller panel.
        let Command::ApplyDisplays(changed) = receiver.try_recv().unwrap() else {
            panic!("expected a display change");
        };
        assert_eq!((changed[0].width, changed[0].height), (1280, 720));
        assert_eq!((changed[1].x, changed[1].y), (1280, 0));
        // Until the server answers, the dialog is not shown or answerable.
        assert!(app.keep.as_ref().is_some_and(|p| !p.applied));
        app.event(&key(Key::Enter), &mut control);
        assert!(app.keep.is_some());
        app.apply_update(Update::DisplaysApplied(None));
        assert!(app.start_countdown);
        app.event(&Event::Wake, &mut control);
        // Fourteen seconds pass, then the old layout comes back.
        for _ in 0..14 {
            app.event(&Event::Timer(REVERT_TIMER), &mut control);
        }
        assert!(app
            .keep
            .as_ref()
            .unwrap()
            .dialog
            .message
            .contains("in 1 second."));
        let tree = app.accessibility().unwrap();
        assert!(tree
            .nodes
            .iter()
            .any(|(_, n)| n.label() == Some("Keep these display settings?")));
        app.event(&Event::Timer(REVERT_TIMER), &mut control);
        assert!(app.keep.is_none());
        let Command::ApplyDisplays(reverted) = receiver.try_recv().unwrap() else {
            panic!("expected the revert");
        };
        assert_eq!(reverted, display::current_configs(&snapshot));
        // Turn the panel sideways and keep it: the layout is saved.
        app.event(&key(Key::Tab), &mut control);
        app.event(&key(Key::Tab), &mut control);
        assert!(app.rotation.focused);
        app.event(&key(Key::Space), &mut control);
        app.event(&key(Key::Down), &mut control);
        app.event(&key(Key::Enter), &mut control);
        let Command::ApplyDisplays(turned) = receiver.try_recv().unwrap() else {
            panic!("expected a rotation");
        };
        assert_eq!(turned[0].rotation, Rotation::Left);
        assert_eq!(turned[1].x, 1080);
        app.apply_update(Update::DisplaysApplied(None));
        app.event(&Event::Wake, &mut control);
        for factor in [1, 2] {
            let surface = RasterSurface::new(WIDTH, HEIGHT, Scale::new(factor).unwrap()).unwrap();
            app.paint(&surface);
        }
        app.event(&key(Key::Enter), &mut control);
        assert!(app.keep.is_none(), "{}", app.status);
        assert_eq!(app.status, "The display settings are kept.");
        assert_eq!(
            display::load(&dir.path().join(model::DISPLAYS_FILE)),
            turned
        );
        // A change the server refuses leaves nothing to keep.
        app.event(&key(Key::Space), &mut control);
        app.event(&key(Key::Up), &mut control);
        app.event(&key(Key::Enter), &mut control);
        assert!(receiver.try_recv().is_ok());
        app.apply_update(Update::DisplaysApplied(Some("BadMatch".into())));
        assert!(app.keep.is_none() && app.status_error);
        // Without RandR the controls are off and the panel says why.
        app.apply_update(Update::Displays(Err(model::Unavailable::new(
            Availability::UnsupportedHardware,
            "no RandR",
        ))));
        app.sync_from_state();
        assert!(!app.resolution.enabled && !app.rotation.enabled);
    }

    #[test]
    fn sound_devices_and_applications_are_chosen_from_pop_ups() {
        use slopos_services::audio::{AudioDevice, AudioDirection, AudioStream};
        let (commands, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let (_sender, updates) = std::sync::mpsc::channel();
        let mut app = ControlPanels::new(Some(Services::for_tests(commands, updates)));
        let device = |index, name: &str, description: &str, default| AudioDevice {
            index,
            name: name.into(),
            description: Some(description.into()),
            direction: AudioDirection::Output,
            muted: false,
            volume_millipercent: vec![50_000],
            is_default: default,
        };
        let mut snapshot = AudioSnapshot {
            revision: 1,
            availability: Availability::Available,
            default_output: Some("speakers".into()),
            default_input: None,
            outputs: vec![
                device(1, "speakers", "Speakers", true),
                device(2, "hdmi", "HDMI Audio", false),
            ],
            inputs: Vec::new(),
            streams: Vec::new(),
        };
        app.apply_update(Update::Audio(Ok(snapshot.clone())));
        let mut control = Control::default();
        app.event(&key(Key::Down), &mut control);
        assert_eq!(app.panel(), Panel::Sound);
        assert_eq!(app.output_device.value(), "Speakers");
        assert!(app.output_device.enabled && !app.input_device.enabled);
        assert!(!app.app.enabled && !app.app_volume.enabled);
        // Output device: Tab, Space, Down, Return.
        app.event(&key(Key::Tab), &mut control);
        assert!(app.output_device.focused);
        app.event(&key(Key::Space), &mut control);
        app.event(&key(Key::Down), &mut control);
        app.event(&key(Key::Enter), &mut control);
        assert_eq!(
            receiver.try_recv().unwrap(),
            Command::SetDefaultOutput("hdmi".into())
        );
        // An application starts playing on the speakers.
        snapshot.streams.push(AudioStream {
            index: 42,
            application: "Music".into(),
            media: Some("Song".into()),
            output: 1,
            muted: false,
            volume_millipercent: vec![80_000, 80_000],
        });
        app.apply_update(Update::Audio(Ok(snapshot.clone())));
        app.sync_from_state();
        assert_eq!(app.app.value(), "Music — Song");
        assert_eq!(app.app_volume.value(), 80.0);
        assert_eq!(app.app_output.value(), "Speakers");
        // Output volume and mute, input volume and mute (the Input pop-up
        // has nothing to choose, so Tab skips it), then the application.
        for _ in 0..6 {
            app.event(&key(Key::Tab), &mut control);
        }
        assert!(app.app_volume.focused);
        app.event(&key(Key::Left), &mut control);
        assert_eq!(
            receiver.try_recv().unwrap(),
            Command::SetStreamVolume(42, 75)
        );
        app.event(&key(Key::Tab), &mut control);
        app.event(&key(Key::Space), &mut control);
        assert_eq!(
            receiver.try_recv().unwrap(),
            Command::SetStreamMuted(42, true)
        );
        app.event(&key(Key::Tab), &mut control);
        assert!(app.app_output.focused);
        app.event(&key(Key::Space), &mut control);
        app.event(&key(Key::Down), &mut control);
        app.event(&key(Key::Enter), &mut control);
        assert_eq!(
            receiver.try_recv().unwrap(),
            Command::MoveStream(42, "hdmi".into())
        );
        let tree = app.accessibility().unwrap();
        assert!(tree.nodes.iter().any(
            |(_, n)| n.role() == accesskit::Role::ComboBox && n.value() == Some("Music — Song")
        ));
        for factor in [1, 2] {
            let surface = RasterSurface::new(WIDTH, HEIGHT, Scale::new(factor).unwrap()).unwrap();
            app.paint(&surface);
        }
        // The application stops: its controls go away.
        snapshot.streams.clear();
        app.apply_update(Update::Audio(Ok(snapshot)));
        app.sync_from_state();
        assert!(!app.app.enabled && !app.app_volume.enabled && !app.app_output.enabled);
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
