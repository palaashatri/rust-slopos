//! SLOPOS Control Panels: native Network, Sound, Display, Bluetooth,
//! Date & Time and Appearance panels on the SLOPOS toolkit.
//!
//! All state comes from `slopos-services` through the provider thread in
//! `services.rs`; a panel whose provider is missing says so and disables its
//! controls. Changes go back through the same providers, and the window
//! shows the provider's answer (the new snapshot or the error).

mod model;
mod services;

use std::sync::{Arc, Mutex};

use model::{State, APPEARANCES};
use services::{Command, Services, Update};
use slopos_services::audio::AudioSnapshot;
use slopos_services::bluetooth::BluetoothSnapshot;
use slopos_services::brightness::BrightnessSnapshot;
use slopos_services::network::{NetworkDeviceType, NetworkSnapshot};
use slopos_services::timedate::TimedateSnapshot;
use slopos_ui::accessibility::window_tree;
use slopos_ui::controls::button::{Button, ButtonOutcome};
use slopos_ui::controls::focus::FocusChain;
use slopos_ui::controls::list::{ListOutcome, ListView};
use slopos_ui::controls::range::{Checkbox, Slider, ToggleOutcome, ValueOutcome};
use slopos_ui::event::{AccessibilityRequest, Event, Key};
use slopos_ui::foundation::geometry::Rect;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Panel {
    Network,
    Sound,
    Display,
    Bluetooth,
    DateTime,
    Appearance,
}

const PANELS: [(Panel, &str); 6] = [
    (Panel::Network, "Network"),
    (Panel::Sound, "Sound"),
    (Panel::Display, "Display"),
    (Panel::Bluetooth, "Bluetooth"),
    (Panel::DateTime, "Date & Time"),
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
    focus: FocusChain,
    status: String,
    font: Font,
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
            focus: FocusChain::default(),
            status: String::new(),
            font: Font::system(12.0),
        };
        if app.services.is_none() {
            app.status = "Can't start the service thread; panels show no live state.".into();
        }
        app.layout();
        app.sync_from_state();
        app
    }

    fn panel(&self) -> Panel {
        PANELS[self.sidebar.cursor().unwrap_or(0).min(PANELS.len() - 1)].0
    }

    fn content(&self) -> Rect {
        Rect {
            x: SIDEBAR_WIDTH as i32 + 16,
            y: 16,
            width: self.window.0.saturating_sub(SIDEBAR_WIDTH + 32),
            height: self.window.1.saturating_sub(48),
        }
    }

    fn layout(&mut self) {
        let (_, height) = self.window;
        self.sidebar.set_bounds(Rect {
            x: 1,
            y: 1,
            width: SIDEBAR_WIDTH,
            height: height.saturating_sub(26),
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
    }

    fn apply_update(&mut self, update: Update) {
        match update {
            Update::Network(state) => self.network = state,
            Update::Audio(state) => self.audio = state,
            Update::Brightness(state) => self.brightness = state,
            Update::Bluetooth(state) => self.bluetooth = state,
            Update::Time(state) => self.time = state,
            Update::Failed(error) => self.status = format!("That change failed: {error}"),
        }
    }

    fn send(&mut self, command: Command) {
        match &self.services {
            Some(services) => {
                services.send(command);
                self.status.clear();
            }
            None => self.status = "No service thread is running.".into(),
        }
    }

    fn handle(&mut self, event: &Event, control: &mut Control) {
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
                        self.status = "Searching for Wi-Fi networks…".into();
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
                self.status = format!("Switching to the {name} appearance…");
            }
            Err(error) => self.status = format!("Can't change the appearance: {error}"),
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
        surface.clear(colors.surface)?;
        let font = &self.font;
        let line = measure("Ag", font).height as i32;
        self.sidebar.paint(surface, |surface, index, rect, _| {
            surface.draw_text(
                PANELS[index].1,
                rect.x + 10,
                rect.y + (rect.height as i32 - line) / 2,
                font,
                colors.ink,
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
        surface.draw_text(&self.status, c.x, status_y + 2, font, colors.danger)
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

    fn accessibility(&self) -> Option<accesskit::TreeUpdate> {
        let names = |index: usize| {
            PANELS
                .get(index)
                .map(|(_, name)| (*name).to_owned())
                .unwrap_or_default()
        };
        let mut nodes = self.sidebar.accessibility_nodes(&names);
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
        }
        let mut status = accesskit::Node::new(accesskit::Role::Label);
        status.set_label(self.status.as_str());
        nodes.push((STATUS_ID, status));
        let focus = match self.focus.focused() {
            Some(SIDEBAR_ID) => Some(self.sidebar.focus_node()),
            other => other,
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
}
