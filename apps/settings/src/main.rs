#![allow(
    dead_code,
    unused_imports,
    clippy::manual_div_ceil,
    clippy::field_reassign_with_default
)]

use slopos_bus::{
    read_spaces_snapshot, send_session_control, SessionControlRequest, SpaceClassification,
    SpaceTargetWire, SpacesControlCommand, SpacesDisplayPolicy, SpacesSnapshot,
};
use slopos_kit::button::Button;
use slopos_kit::event::{KeyCode, Modifiers};
use slopos_kit::label::Label;
use slopos_kit::slider::Slider;
use slopos_kit::text_field::TextField;
use slopos_kit::window::Window;
use slopos_kit::{
    AccessibilityNode, AccessibilityRole, Event, EventResult, FocusManager, LayoutConstraint,
    PointerDispatcher, Rect, Size, ThemeContext, Widget, WidgetState,
};
use slopos_sdk::{build_menu, Application};
use slopos_shell::{get_network_status, DisplayConfig};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

fn main() {
    let _ = tracing_subscriber::fmt::try_init();

    let mut app = Application::new("Settings", "com.slopos.settings");

    let mut file_menu = build_menu("File");
    file_menu.add_action("Show All Settings");

    let mut window_menu = build_menu("Window");
    window_menu.add_action("Minimize");

    app.set_menus(vec![file_menu, window_menu]);

    app.on_menu_action(|action, window| {
        let Some(content) = window.content.as_mut() else {
            return;
        };
        let Some(view) = content.as_any_mut().downcast_mut::<SettingsView>() else {
            return;
        };
        let action = action
            .strip_prefix("com.slopos.settings.")
            .unwrap_or(action);
        if action == "file.show_all_settings" {
            view.select_category(Category::General);
        }
    });

    let store = SettingsStore::default();
    let view = SettingsView::load(store);
    let mut window = Window::new("Settings");
    window.set_content(Box::new(view));
    app.set_main_window(window);
    app.run();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Choice {
    AppearanceLight,
    AppearanceDark,
    AppearanceSystem,
    ThemeClassic,
    ThemeDark,
    ThemeGrape,
    ThemeBlueberry,
    ThemeStrawberry,
    ThemeSolarized,
    ThemeDracula,
    ThemeHighContrast,
    DesktopIconsOn,
    DesktopIconsOff,
    DockBottom,
    DockRight,
    HdrOff,
    HdrOn,
    VrrOff,
    VrrAdaptive,
    Refresh60,
    Refresh120,
    RefreshAdaptive,
    ColorSrgb,
    ColorRec2020,
    ArrangeExtendRight,
    ArrangeExtendDown,
    ArrangeMirror,
    ArrangePrimaryOnly,
    Scale100,
    Scale150,
    Scale200,
    SoundOff,
    SoundOn,
    NetworkOffline,
    NetworkDhcp,
    KeyboardSlow,
    KeyboardFast,
    MouseNaturalOff,
    MouseNaturalOn,
    AccessibilityOff,
    AccessibilityOn,
    PrivacyStandard,
    PrivacyStrict,
    NotificationsOff,
    NotificationsOn,
    SpacesSharedSpan,
    SpacesIndependentPerDisplay,
    SpacesNormal,
    SpacesFullscreen,
}

impl Choice {
    fn is_display_policy(self) -> bool {
        matches!(
            self,
            Self::HdrOff
                | Self::HdrOn
                | Self::VrrOff
                | Self::VrrAdaptive
                | Self::Refresh60
                | Self::Refresh120
                | Self::RefreshAdaptive
                | Self::ColorSrgb
                | Self::ColorRec2020
        )
    }

    fn is_display_topology(self) -> bool {
        matches!(
            self,
            Self::ArrangeExtendRight
                | Self::ArrangeExtendDown
                | Self::ArrangeMirror
                | Self::ArrangePrimaryOnly
                | Self::Scale100
                | Self::Scale150
                | Self::Scale200
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Category {
    General,
    Appearance,
    DesktopDock,
    Display,
    Sound,
    Network,
    Keyboard,
    Mouse,
    Accessibility,
    Privacy,
    Notifications,
    Spaces,
}

impl Category {
    const ALL: [Category; 12] = [
        Category::General,
        Category::Appearance,
        Category::DesktopDock,
        Category::Display,
        Category::Sound,
        Category::Network,
        Category::Keyboard,
        Category::Mouse,
        Category::Accessibility,
        Category::Privacy,
        Category::Notifications,
        Category::Spaces,
    ];

    fn label(self) -> &'static str {
        match self {
            Category::General => "General",
            Category::Appearance => "Appearance",
            Category::DesktopDock => "Desktop & Dock",
            Category::Display => "Display",
            Category::Sound => "Sound",
            Category::Network => "Network",
            Category::Keyboard => "Keyboard",
            Category::Mouse => "Mouse",
            Category::Accessibility => "Accessibility",
            Category::Privacy => "Privacy & Security",
            Category::Notifications => "Notifications",
            Category::Spaces => "SLOPOS Spaces",
        }
    }

    fn title(self) -> String {
        match self {
            Category::DesktopDock => "DESKTOP & DOCK".to_string(),
            Category::Privacy => "PRIVACY & SECURITY".to_string(),
            _ => self.label().to_ascii_uppercase(),
        }
    }

    fn description(self) -> &'static str {
        match self {
            Category::General => "Choose system defaults used by first-party SLOPOS-I apps.",
            Category::Appearance => "Choose how SLOPOS-I draws native windows and apps.",
            Category::DesktopDock => "Control desktop icons and the shell launcher position.",
            Category::Display => "Configure advertised display capabilities for the shell session.",
            Category::Sound => "Control desktop sound effects for native SLOPOS-I apps.",
            Category::Network => "Set the network profile exposed to shell status surfaces.",
            Category::Keyboard => "Tune keyboard repeat behavior for native controls.",
            Category::Mouse => "Tune pointer and scrolling behavior.",
            Category::Accessibility => "Enable high-visibility affordances across native apps.",
            Category::Privacy => "Control privacy defaults used by app services.",
            Category::Notifications => "Control notification delivery for native apps.",
            Category::Spaces => {
                "Manage compositor-owned Spaces, membership policy and per-Space metadata."
            }
        }
    }

    fn choices(self) -> &'static [(Choice, &'static str)] {
        match self {
            Category::General => &[
                (Choice::AppearanceSystem, "System Appearance"),
                (Choice::NotificationsOn, "Notifications On"),
                (Choice::SoundOn, "Sound Effects On"),
            ],
            Category::Appearance => &[
                (Choice::AppearanceLight, "Light"),
                (Choice::AppearanceDark, "Dark"),
                (Choice::AppearanceSystem, "System"),
                (Choice::ThemeClassic, "Classic"),
                (Choice::ThemeDark, "Dark Theme"),
                (Choice::ThemeGrape, "Grape"),
                (Choice::ThemeBlueberry, "Blueberry"),
                (Choice::ThemeStrawberry, "Strawberry"),
                (Choice::ThemeSolarized, "Solarized"),
                (Choice::ThemeDracula, "Dracula"),
                (Choice::ThemeHighContrast, "High Contrast"),
            ],
            Category::DesktopDock => &[
                (Choice::DesktopIconsOn, "Desktop Icons On"),
                (Choice::DesktopIconsOff, "Desktop Icons Off"),
                (Choice::DockBottom, "Dock Bottom"),
                (Choice::DockRight, "Dock Right"),
            ],
            Category::Display => &[
                (Choice::HdrOff, "HDR Off"),
                (Choice::HdrOn, "HDR Requested"),
                (Choice::VrrOff, "VRR Off"),
                (Choice::VrrAdaptive, "VRR Adaptive"),
                (Choice::Refresh60, "Refresh 60Hz"),
                (Choice::Refresh120, "Refresh 120Hz"),
                (Choice::RefreshAdaptive, "Refresh Adaptive"),
                (Choice::ColorSrgb, "Color sRGB"),
                (Choice::ColorRec2020, "Color Rec2020"),
                (Choice::ArrangeExtendRight, "Arrange Extend Right"),
                (Choice::ArrangeExtendDown, "Arrange Extend Down"),
                (Choice::ArrangeMirror, "Arrange Mirror"),
                (Choice::ArrangePrimaryOnly, "Arrange Primary Only"),
                (Choice::Scale100, "Scale 100%"),
                (Choice::Scale150, "Scale 150%"),
                (Choice::Scale200, "Scale 200%"),
            ],
            Category::Sound => &[
                (Choice::SoundOff, "Sound Off"),
                (Choice::SoundOn, "Sound On"),
            ],
            Category::Network => &[
                (Choice::NetworkOffline, "Offline"),
                (Choice::NetworkDhcp, "DHCP"),
            ],
            Category::Keyboard => &[
                (Choice::KeyboardSlow, "Slow Repeat"),
                (Choice::KeyboardFast, "Fast Repeat"),
            ],
            Category::Mouse => &[
                (Choice::MouseNaturalOff, "Natural Scroll Off"),
                (Choice::MouseNaturalOn, "Natural Scroll On"),
            ],
            Category::Accessibility => &[
                (Choice::AccessibilityOff, "Assistive UI Off"),
                (Choice::AccessibilityOn, "Assistive UI On"),
            ],
            Category::Privacy => &[
                (Choice::PrivacyStandard, "Standard"),
                (Choice::PrivacyStrict, "Strict"),
            ],
            Category::Notifications => &[
                (Choice::NotificationsOff, "Notifications Off"),
                (Choice::NotificationsOn, "Notifications On"),
            ],
            Category::Spaces => &[],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppearanceMode {
    System,
    Light,
    Dark,
}

impl AppearanceMode {
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "system" => Some(Self::System),
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::System => "SYSTEM",
            Self::Light => "LIGHT",
            Self::Dark => "DARK",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SettingsState {
    appearance: AppearanceMode,
    theme: String,
    desktop_icons: bool,
    dock_position: String,
    hdr_requested: bool,
    vrr_adaptive: bool,
    refresh_rate: String,
    color_space: String,
    arrange_mode: String,
    scale_percent: u32,
    sound_effects: bool,
    volume_percent: u8,
    network_profile: String,
    keyboard_repeat: String,
    natural_scroll: bool,
    pointer_speed: u8,
    assistive_ui: bool,
    privacy_mode: String,
    notifications: bool,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            appearance: AppearanceMode::System,
            theme: "classic".to_string(),
            desktop_icons: true,
            dock_position: "bottom".to_string(),
            hdr_requested: false,
            vrr_adaptive: false,
            refresh_rate: "60hz".to_string(),
            color_space: "srgb".to_string(),
            arrange_mode: "extend_right".to_string(),
            scale_percent: 100,
            sound_effects: true,
            volume_percent: 75,
            network_profile: "dhcp".to_string(),
            keyboard_repeat: "fast".to_string(),
            natural_scroll: false,
            pointer_speed: 50,
            assistive_ui: false,
            privacy_mode: "standard".to_string(),
            notifications: true,
        }
    }
}

impl SettingsState {
    /// Build shell [`DisplayConfig`] from Display pane fields for plan + env apply.
    fn display_config(&self) -> DisplayConfig {
        DisplayConfig::from_settings_fields(
            self.hdr_requested,
            self.vrr_adaptive,
            self.refresh_rate.as_str(),
            self.color_space.as_str(),
            self.arrange_mode.as_str(),
            self.scale_percent,
        )
    }

    fn choice_enabled(&self, choice: Choice) -> bool {
        match choice {
            Choice::AppearanceLight => self.appearance == AppearanceMode::Light,
            Choice::AppearanceDark => self.appearance == AppearanceMode::Dark,
            Choice::AppearanceSystem => self.appearance == AppearanceMode::System,
            Choice::ThemeClassic => self.theme == "classic",
            Choice::ThemeDark => self.theme == "dark",
            Choice::ThemeGrape => self.theme == "grape",
            Choice::ThemeBlueberry => self.theme == "blueberry",
            Choice::ThemeStrawberry => self.theme == "strawberry",
            Choice::ThemeSolarized => self.theme == "solarized",
            Choice::ThemeDracula => self.theme == "dracula",
            Choice::ThemeHighContrast => self.theme == "highcontrast",
            Choice::DesktopIconsOn => self.desktop_icons,
            Choice::DesktopIconsOff => !self.desktop_icons,
            Choice::DockBottom => self.dock_position == "bottom",
            Choice::DockRight => self.dock_position == "right",
            Choice::HdrOff => !self.hdr_requested,
            Choice::HdrOn => self.hdr_requested,
            Choice::VrrOff => !self.vrr_adaptive,
            Choice::VrrAdaptive => self.vrr_adaptive,
            Choice::Refresh60 => self.refresh_rate == "60hz",
            Choice::Refresh120 => self.refresh_rate == "120hz",
            Choice::RefreshAdaptive => self.refresh_rate == "adaptive",
            Choice::ColorSrgb => self.color_space == "srgb",
            Choice::ColorRec2020 => self.color_space == "rec2020",
            Choice::ArrangeExtendRight => self.arrange_mode == "extend_right",
            Choice::ArrangeExtendDown => self.arrange_mode == "extend_down",
            Choice::ArrangeMirror => self.arrange_mode == "mirror",
            Choice::ArrangePrimaryOnly => self.arrange_mode == "primary_only",
            Choice::Scale100 => self.scale_percent == 100,
            Choice::Scale150 => self.scale_percent == 150,
            Choice::Scale200 => self.scale_percent == 200,
            Choice::SoundOff => !self.sound_effects,
            Choice::SoundOn => self.sound_effects,
            Choice::NetworkOffline => self.network_profile == "offline",
            Choice::NetworkDhcp => self.network_profile == "dhcp",
            Choice::KeyboardSlow => self.keyboard_repeat == "slow",
            Choice::KeyboardFast => self.keyboard_repeat == "fast",
            Choice::MouseNaturalOff => !self.natural_scroll,
            Choice::MouseNaturalOn => self.natural_scroll,
            Choice::AccessibilityOff => !self.assistive_ui,
            Choice::AccessibilityOn => self.assistive_ui,
            Choice::PrivacyStandard => self.privacy_mode == "standard",
            Choice::PrivacyStrict => self.privacy_mode == "strict",
            Choice::NotificationsOff => !self.notifications,
            Choice::NotificationsOn => self.notifications,
            Choice::SpacesSharedSpan
            | Choice::SpacesIndependentPerDisplay
            | Choice::SpacesNormal
            | Choice::SpacesFullscreen => false,
        }
    }

    fn apply_choice(&mut self, choice: Choice) {
        match choice {
            Choice::AppearanceLight => self.appearance = AppearanceMode::Light,
            Choice::AppearanceDark => self.appearance = AppearanceMode::Dark,
            Choice::AppearanceSystem => self.appearance = AppearanceMode::System,
            Choice::ThemeClassic => self.theme = "classic".to_string(),
            Choice::ThemeDark => self.theme = "dark".to_string(),
            Choice::ThemeGrape => self.theme = "grape".to_string(),
            Choice::ThemeBlueberry => self.theme = "blueberry".to_string(),
            Choice::ThemeStrawberry => self.theme = "strawberry".to_string(),
            Choice::ThemeSolarized => self.theme = "solarized".to_string(),
            Choice::ThemeDracula => self.theme = "dracula".to_string(),
            Choice::ThemeHighContrast => self.theme = "highcontrast".to_string(),
            Choice::DesktopIconsOn => self.desktop_icons = true,
            Choice::DesktopIconsOff => self.desktop_icons = false,
            Choice::DockBottom => self.dock_position = "bottom".to_string(),
            Choice::DockRight => self.dock_position = "right".to_string(),
            Choice::HdrOff => self.hdr_requested = false,
            Choice::HdrOn => self.hdr_requested = true,
            Choice::VrrOff => self.vrr_adaptive = false,
            Choice::VrrAdaptive => self.vrr_adaptive = true,
            Choice::Refresh60 => self.refresh_rate = "60hz".to_string(),
            Choice::Refresh120 => self.refresh_rate = "120hz".to_string(),
            Choice::RefreshAdaptive => self.refresh_rate = "adaptive".to_string(),
            Choice::ColorSrgb => self.color_space = "srgb".to_string(),
            Choice::ColorRec2020 => self.color_space = "rec2020".to_string(),
            Choice::ArrangeExtendRight => self.arrange_mode = "extend_right".to_string(),
            Choice::ArrangeExtendDown => self.arrange_mode = "extend_down".to_string(),
            Choice::ArrangeMirror => self.arrange_mode = "mirror".to_string(),
            Choice::ArrangePrimaryOnly => self.arrange_mode = "primary_only".to_string(),
            Choice::Scale100 => self.scale_percent = 100,
            Choice::Scale150 => self.scale_percent = 150,
            Choice::Scale200 => self.scale_percent = 200,
            Choice::SoundOff => self.sound_effects = false,
            Choice::SoundOn => self.sound_effects = true,
            Choice::NetworkOffline => self.network_profile = "offline".to_string(),
            Choice::NetworkDhcp => self.network_profile = "dhcp".to_string(),
            Choice::KeyboardSlow => self.keyboard_repeat = "slow".to_string(),
            Choice::KeyboardFast => self.keyboard_repeat = "fast".to_string(),
            Choice::MouseNaturalOff => self.natural_scroll = false,
            Choice::MouseNaturalOn => self.natural_scroll = true,
            Choice::AccessibilityOff => self.assistive_ui = false,
            Choice::AccessibilityOn => self.assistive_ui = true,
            Choice::PrivacyStandard => self.privacy_mode = "standard".to_string(),
            Choice::PrivacyStrict => self.privacy_mode = "strict".to_string(),
            Choice::NotificationsOff => self.notifications = false,
            Choice::NotificationsOn => self.notifications = true,
            Choice::SpacesSharedSpan
            | Choice::SpacesIndependentPerDisplay
            | Choice::SpacesNormal
            | Choice::SpacesFullscreen => {}
        }
    }

    fn status_line(&self, category: Category) -> String {
        match category {
            Category::General => format!(
                "GENERAL - {} / {} / {}",
                self.appearance.label(),
                if self.notifications {
                    "NOTIFY ON"
                } else {
                    "NOTIFY OFF"
                },
                if self.sound_effects {
                    "SOUND ON"
                } else {
                    "SOUND OFF"
                }
            ),
            Category::Appearance => format!(
                "MODE - {} / THEME - {}",
                self.appearance.label(),
                self.theme.to_ascii_uppercase()
            ),
            Category::DesktopDock => format!(
                "DESKTOP - ICONS {} / DOCK {}",
                if self.desktop_icons { "ON" } else { "OFF" },
                self.dock_position.to_ascii_uppercase()
            ),
            Category::Display => format!(
                "DISPLAY - HDR {} / VRR {} / {} / {} / ARRANGE {} / SCALE {}%",
                if self.hdr_requested {
                    "REQUESTED"
                } else {
                    "OFF"
                },
                if self.vrr_adaptive { "ADAPTIVE" } else { "OFF" },
                self.refresh_rate.to_ascii_uppercase(),
                self.color_space.to_ascii_uppercase(),
                self.arrange_mode.to_ascii_uppercase(),
                self.scale_percent
            ),
            Category::Sound => format!(
                "SOUND - EFFECTS {} / VOLUME {}%",
                if self.sound_effects { "ON" } else { "OFF" },
                self.volume_percent
            ),
            Category::Network => {
                let live = format!(" / LIVE {}", live_network_summary());
                format!(
                    "NETWORK - {}{}",
                    self.network_profile.to_ascii_uppercase(),
                    live
                )
            }
            Category::Keyboard => {
                format!(
                    "KEYBOARD - {} REPEAT",
                    self.keyboard_repeat.to_ascii_uppercase()
                )
            }
            Category::Mouse => format!(
                "MOUSE - NATURAL SCROLL {} / SPEED {}%",
                if self.natural_scroll { "ON" } else { "OFF" },
                self.pointer_speed
            ),
            Category::Accessibility => format!(
                "ACCESSIBILITY - ASSISTIVE UI {}",
                if self.assistive_ui { "ON" } else { "OFF" }
            ),
            Category::Privacy => format!("PRIVACY - {}", self.privacy_mode.to_ascii_uppercase()),
            Category::Notifications => format!(
                "NOTIFICATIONS - {}",
                if self.notifications { "ON" } else { "OFF" }
            ),
            Category::Spaces => "SPACES - compositor state is authoritative".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
struct SettingsStore {
    path: PathBuf,
}

impl Default for SettingsStore {
    fn default() -> Self {
        let config_dir = std::env::var_os("SLOPOS_CONFIG_DIR")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|home| home.join(".config/slopos-i"))
            })
            .unwrap_or_else(|| PathBuf::from("/tmp/slopos-i"));
        Self {
            path: config_dir.join("settings.conf"),
        }
    }
}

impl SettingsStore {
    #[cfg(test)]
    fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn load(&self) -> SettingsState {
        let Ok(content) = fs::read_to_string(&self.path) else {
            return SettingsState::default();
        };

        let mut state = SettingsState::default();
        for line in content.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "appearance" => {
                    if let Some(mode) = AppearanceMode::parse(value) {
                        state.appearance = mode;
                    }
                }
                "theme"
                    if matches!(
                        value,
                        "classic"
                            | "dark"
                            | "grape"
                            | "blueberry"
                            | "strawberry"
                            | "solarized"
                            | "dracula"
                            | "highcontrast"
                    ) =>
                {
                    state.theme = value.to_string();
                }
                "desktop_icons" => state.desktop_icons = parse_bool(value, state.desktop_icons),
                "dock_position" if matches!(value, "bottom" | "right") => {
                    state.dock_position = value.to_string();
                }
                "hdr_requested" => state.hdr_requested = parse_bool(value, state.hdr_requested),
                "vrr_adaptive" => state.vrr_adaptive = parse_bool(value, state.vrr_adaptive),
                "refresh_rate"
                    if matches!(value, "60hz" | "120hz" | "144hz" | "165hz" | "adaptive") =>
                {
                    state.refresh_rate = value.to_string();
                }
                "color_space" if matches!(value, "srgb" | "rec2020" | "scrgb") => {
                    state.color_space = value.to_string();
                }
                "arrange_mode"
                    if matches!(
                        value,
                        "extend_right"
                            | "extend_down"
                            | "mirror"
                            | "primary_only"
                            | "extend"
                            | "stack"
                            | "clone"
                            | "primary"
                    ) =>
                {
                    // Normalize aliases to canonical snake_case used by DisplayConfig.
                    state.arrange_mode = match value {
                        "extend" | "extend_right" => "extend_right".to_string(),
                        "stack" | "extend_down" => "extend_down".to_string(),
                        "clone" | "mirror" => "mirror".to_string(),
                        "primary" | "primary_only" => "primary_only".to_string(),
                        other => other.to_string(),
                    };
                }
                "scale_percent" => {
                    if let Ok(n) = value.parse::<u32>() {
                        if (50..=400).contains(&n) {
                            state.scale_percent = n;
                        }
                    }
                }
                "sound_effects" => state.sound_effects = parse_bool(value, state.sound_effects),
                "volume_percent" => {
                    state.volume_percent = parse_percent(value, state.volume_percent)
                }
                "network_profile" if matches!(value, "offline" | "dhcp") => {
                    state.network_profile = value.to_string();
                }
                "keyboard_repeat" if matches!(value, "slow" | "fast") => {
                    state.keyboard_repeat = value.to_string();
                }
                "natural_scroll" => state.natural_scroll = parse_bool(value, state.natural_scroll),
                "pointer_speed" => state.pointer_speed = parse_percent(value, state.pointer_speed),
                "assistive_ui" => state.assistive_ui = parse_bool(value, state.assistive_ui),
                "privacy_mode" if matches!(value, "standard" | "strict") => {
                    state.privacy_mode = value.to_string();
                }
                "notifications" => state.notifications = parse_bool(value, state.notifications),
                _ => {}
            }
        }
        state
    }

    fn save(&self, state: &SettingsState) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Merge-preserving save: keep unknown keys (e.g. lock_password) and
        // update/insert known settings keys.
        let mut map = std::collections::BTreeMap::<String, String>::new();
        if let Ok(existing) = fs::read_to_string(&self.path) {
            for line in existing.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((key, value)) = line.split_once('=') {
                    map.insert(key.trim().to_string(), value.trim().to_string());
                }
            }
        }

        map.insert("appearance".into(), state.appearance.as_str().to_string());
        map.insert("theme".into(), state.theme.clone());
        map.insert("desktop_icons".into(), state.desktop_icons.to_string());
        map.insert("dock_position".into(), state.dock_position.clone());
        map.insert("hdr_requested".into(), state.hdr_requested.to_string());
        map.insert("vrr_adaptive".into(), state.vrr_adaptive.to_string());
        map.insert("refresh_rate".into(), state.refresh_rate.clone());
        map.insert("color_space".into(), state.color_space.clone());
        map.insert("arrange_mode".into(), state.arrange_mode.clone());
        map.insert("scale_percent".into(), state.scale_percent.to_string());
        map.insert("sound_effects".into(), state.sound_effects.to_string());
        map.insert("volume_percent".into(), state.volume_percent.to_string());
        map.insert("network_profile".into(), state.network_profile.clone());
        map.insert("keyboard_repeat".into(), state.keyboard_repeat.clone());
        map.insert("natural_scroll".into(), state.natural_scroll.to_string());
        map.insert("pointer_speed".into(), state.pointer_speed.to_string());
        map.insert("assistive_ui".into(), state.assistive_ui.to_string());
        map.insert("privacy_mode".into(), state.privacy_mode.clone());
        map.insert("notifications".into(), state.notifications.to_string());

        let mut content = String::new();
        for (key, value) in map {
            content.push_str(&format!("{key}={value}\n"));
        }
        self.write_atomic(&content)
    }

    /// Write `content` to `self.path` without ever exposing a partially
    /// written or truncated file to a concurrent reader/writer.
    ///
    /// The naive `fs::write(&self.path, content)` this replaces truncates the
    /// destination in place: a second process reading `settings.conf` while
    /// the write is in flight (or a process that crashes mid-write) can
    /// observe a corrupt or empty file, and two writers racing the
    /// read-modify-write in `save()` can clobber each other's keys. Instead we
    /// write to a sibling temp file in the same directory (so the later
    /// rename stays on one filesystem), `fsync` it so its bytes are durable,
    /// then `fs::rename` it over the destination. Rename within a filesystem
    /// is atomic: any reader sees either the complete old file or the
    /// complete new file, never a mixture.
    fn write_atomic(&self, content: &str) -> std::io::Result<()> {
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        let file_name = self.path.file_name().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "settings path has no file name",
            )
        })?;

        // Unique per-process, per-call name so two SettingsStore instances
        // (or two saves racing on separate threads of the same process)
        // never collide on the same temp file; only the final rename touches
        // the shared destination path. Wall-clock time alone can repeat
        // within a process (coarse timer resolution on some platforms), so a
        // monotonically increasing counter is included as a tiebreaker.
        static SAVE_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let sequence = SAVE_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut tmp_name = file_name.to_os_string();
        tmp_name.push(format!(".tmp.{}.{unique}.{sequence}", std::process::id()));
        let tmp_path = parent.join(tmp_name);

        let write_result = (|| -> std::io::Result<()> {
            let mut file = fs::File::create(&tmp_path)?;
            file.write_all(content.as_bytes())?;
            file.sync_all()?;
            Ok(())
        })();

        if let Err(err) = write_result {
            let _ = fs::remove_file(&tmp_path);
            return Err(err);
        }

        fs::rename(&tmp_path, &self.path).inspect_err(|_| {
            let _ = fs::remove_file(&tmp_path);
        })
    }
}

fn parse_bool(value: &str, fallback: bool) -> bool {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => true,
        "false" | "0" | "no" | "off" => false,
        _ => fallback,
    }
}

/// Build CLI argv for volume backends (pure; unit-tested).
fn volume_pactl_args(percent: u8) -> [String; 3] {
    let percent = percent.min(100);
    [
        "set-sink-volume".into(),
        "@DEFAULT_SINK@".into(),
        format!("{percent}%"),
    ]
}

fn volume_wpctl_args(percent: u8) -> [String; 3] {
    let percent = percent.min(100);
    let linear = f32::from(percent) / 100.0;
    [
        "set-volume".into(),
        "@DEFAULT_AUDIO_SINK@".into(),
        format!("{linear:.2}"),
    ]
}

/// Apply volume to PulseAudio (`pactl`) or PipeWire (`wpctl`). Best-effort.
fn apply_system_volume(percent: u8) -> Result<(), String> {
    let percent = percent.min(100);
    let pactl = volume_pactl_args(percent);
    if std::process::Command::new("pactl")
        .args(pactl.iter().map(String::as_str))
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return Ok(());
    }
    let wpctl = volume_wpctl_args(percent);
    if std::process::Command::new("wpctl")
        .args(wpctl.iter().map(String::as_str))
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return Ok(());
    }
    Err("no pactl/wpctl (volume preference saved only)".into())
}

/// Live network summary from the same authoritative NetworkManager D-Bus
/// snapshot used by shell status surfaces. Errors stay visible as
/// `UNAVAILABLE`; a missing `nmcli` must not silently hide the live state.
fn live_network_summary() -> String {
    get_network_status().settings_summary()
}

fn parse_percent(value: &str, fallback: u8) -> u8 {
    value
        .trim()
        .parse::<u8>()
        .map(|value| value.min(100))
        .unwrap_or(fallback)
}

fn optional_metadata(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

/// Normalize and validate the application ID entered in Settings before a
/// request reaches the compositor. The compositor remains the final
/// authority, but rejecting malformed input here avoids sending requests that
/// cannot possibly succeed and keeps the UI state non-optimistic.
fn parse_application_id_input(value: &str) -> Result<String, &'static str> {
    if value.chars().any(char::is_control) {
        return Err("INVALID APPLICATION ID");
    }
    let value = value.trim();
    if value.is_empty() {
        return Err("ENTER AN APPLICATION ID FIRST");
    }
    Ok(value.to_string())
}

/// Parse a Settings target against the latest compositor snapshot. Numeric
/// targets are stable Space IDs, while `all` and `current` map directly to the
/// wire protocol. Unknown or zero IDs are rejected before any IPC request is
/// sent.
fn parse_application_target_input(
    value: &str,
    snapshot: &SpacesSnapshot,
) -> Result<SpaceTargetWire, &'static str> {
    if value.chars().any(char::is_control) {
        return Err("INVALID SPACE TARGET");
    }
    let value = value.trim();
    if value.is_empty() {
        return Err("ENTER A SPACE ID, ALL, OR CURRENT");
    }
    if value.eq_ignore_ascii_case("all") {
        return Ok(SpaceTargetWire::All);
    }
    if value.eq_ignore_ascii_case("current") {
        return Ok(SpaceTargetWire::Current);
    }

    let id = value.parse::<u64>().map_err(|_| "INVALID SPACE ID")?;
    if id == 0 {
        return Err("INVALID SPACE ID");
    }
    if !snapshot.spaces.iter().any(|space| space.id == id) {
        return Err("UNKNOWN SPACE ID");
    }
    Ok(SpaceTargetWire::Id { id })
}

fn valid_spaces_snapshot(snapshot: &SpacesSnapshot) -> bool {
    if snapshot.spaces.is_empty() || snapshot.active_space == 0 {
        return false;
    }
    let mut rows = snapshot.spaces.iter().collect::<Vec<_>>();
    rows.sort_by_key(|space| space.order);
    if !rows
        .iter()
        .enumerate()
        .all(|(index, space)| space.order == index && space.id != 0)
    {
        return false;
    }
    if rows.iter().filter(|space| space.active).count() != 1 {
        return false;
    }
    if rows
        .iter()
        .filter(|space| space.id == snapshot.active_space)
        .count()
        != 1
        || !rows
            .iter()
            .any(|space| space.id == snapshot.active_space && space.active)
    {
        return false;
    }
    let mut ids = std::collections::HashSet::new();
    let mut names = std::collections::HashSet::new();
    for space in rows {
        if space.name.is_empty()
            || space.name.trim() != space.name
            || space.name.chars().any(char::is_control)
            || !ids.insert(space.id)
            || !names.insert(space.name.to_lowercase())
        {
            return false;
        }
        for value in [space.wallpaper.as_deref(), space.appearance.as_deref()]
            .into_iter()
            .flatten()
        {
            if value.is_empty() || value.trim() != value || value.chars().any(char::is_control) {
                return false;
            }
        }
        if let Some(output_id) = space.output_id.as_deref() {
            if output_id.is_empty()
                || output_id.trim() != output_id
                || output_id.chars().any(char::is_control)
            {
                return false;
            }
        }
    }
    let outputs_valid = snapshot.multi_monitor_policy != SpacesDisplayPolicy::SharedSpan
        || snapshot
            .spaces
            .iter()
            .all(|space| space.output_id.is_none());
    let mut application_ids = std::collections::HashSet::new();
    let policies_valid = snapshot.application_policies.iter().all(|policy| {
        !policy.app_id.is_empty()
            && policy.app_id.trim() == policy.app_id
            && !policy.app_id.chars().any(char::is_control)
            && application_ids.insert(policy.app_id.as_str())
            && match policy.target {
                SpaceTargetWire::Id { id } => ids.contains(&id),
                SpaceTargetWire::All => true,
                SpaceTargetWire::Current => false,
            }
    });
    outputs_valid && policies_valid
}

struct SettingsView {
    state: WidgetState,
    category_buttons: Vec<Button>,
    heading: Label,
    description: Label,
    status: Label,
    option_buttons: Vec<Button>,
    volume_label: Label,
    volume_slider: Slider,
    pointer_speed_label: Label,
    pointer_speed_slider: Slider,
    spaces_snapshot: Option<SpacesSnapshot>,
    spaces_rows: Vec<(u64, Button)>,
    spaces_action_buttons: Vec<Button>,
    spaces_policy_buttons: Vec<Button>,
    spaces_classification_button: Button,
    spaces_name_field: TextField,
    spaces_wallpaper_field: TextField,
    spaces_appearance_field: TextField,
    spaces_output_field: TextField,
    /// Application-ID policy editor. The compositor remains authoritative;
    /// these controls only send typed requests and render snapshot readback.
    spaces_application_id_field: TextField,
    spaces_application_target_field: TextField,
    spaces_application_apply_button: Button,
    spaces_application_policy_rows: Vec<Button>,
    spaces_feedback: Option<String>,
    selected_category: Category,
    settings: SettingsState,
    store: SettingsStore,
    last_error: Option<String>,
    focus: FocusManager,
    pointer: PointerDispatcher,
}

impl SettingsView {
    fn load(store: SettingsStore) -> Self {
        let settings = store.load();
        let mut view = Self {
            state: WidgetState::new(),
            category_buttons: Category::ALL
                .iter()
                .map(|category| Button::new(category.label()))
                .collect(),
            heading: Label::new("APPEARANCE"),
            description: Label::new("Choose how SLOPOS-I draws native windows and apps."),
            status: Label::new(""),
            option_buttons: Vec::new(),
            volume_label: Label::new("VOLUME"),
            volume_slider: Slider::new(),
            pointer_speed_label: Label::new("POINTER SPEED"),
            pointer_speed_slider: Slider::new(),
            spaces_snapshot: None,
            spaces_rows: Vec::new(),
            spaces_action_buttons: vec![
                Button::new("Create"),
                Button::new("Rename Active"),
                Button::new("Move Up"),
                Button::new("Move Down"),
                Button::new("Remove Active"),
                Button::new("Apply Metadata"),
                Button::new("Assign Output"),
                Button::new("Clear Output"),
                Button::new("Move Active Window to Output"),
            ],
            spaces_policy_buttons: vec![
                Button::new("Shared Across Displays"),
                Button::new("Independent Per Display"),
            ],
            spaces_classification_button: Button::new("Toggle Fullscreen Policy"),
            spaces_name_field: TextField::new().with_placeholder("Space name"),
            spaces_wallpaper_field: TextField::new().with_placeholder("Wallpaper path (optional)"),
            spaces_appearance_field: TextField::new().with_placeholder("Appearance (optional)"),
            spaces_output_field: TextField::new().with_placeholder("Output ID (e.g. DP-1)"),
            spaces_application_id_field: TextField::new()
                .with_placeholder("Application ID (e.g. org.example.Editor)"),
            spaces_application_target_field: TextField::new()
                .with_placeholder("Space ID, all, or current"),
            spaces_application_apply_button: Button::new("Apply"),
            spaces_application_policy_rows: Vec::new(),
            spaces_feedback: None,
            selected_category: Category::Appearance,
            settings,
            store,
            last_error: None,
            focus: FocusManager::new(),
            pointer: PointerDispatcher::new(),
        };
        view.refresh_spaces_snapshot();
        view.refresh_labels();
        view
    }

    fn refresh_labels(&mut self) {
        self.heading.text = self.selected_category.title();
        self.description.text = self.selected_category.description().to_string();

        self.option_buttons = self
            .selected_category
            .choices()
            .iter()
            .map(|(choice, label)| {
                let mut button = Button::new(if self.settings.choice_enabled(*choice) {
                    format!("{label} *")
                } else {
                    (*label).to_string()
                });
                button.checked = self.settings.choice_enabled(*choice);
                button
            })
            .collect();

        self.volume_label.text = format!("VOLUME {}%", self.settings.volume_percent);
        self.volume_slider.min = 0.0;
        self.volume_slider.max = 100.0;
        self.volume_slider.step = 5.0;
        self.volume_slider
            .set_value(self.settings.volume_percent as f32);

        self.pointer_speed_label.text = format!("POINTER SPEED {}%", self.settings.pointer_speed);
        self.pointer_speed_slider.min = 0.0;
        self.pointer_speed_slider.max = 100.0;
        self.pointer_speed_slider.step = 5.0;
        self.pointer_speed_slider
            .set_value(self.settings.pointer_speed as f32);

        for (button, category) in self
            .category_buttons
            .iter_mut()
            .zip(Category::ALL.iter().copied())
        {
            button.checked = category == self.selected_category;
            button.set_label(if button.checked {
                format!("{} *", category.label())
            } else {
                category.label().to_string()
            });
        }

        let error = self
            .last_error
            .as_deref()
            .map(|error| format!(" - {error}"))
            .unwrap_or_default();
        self.status.text = format!(
            "{}{}",
            self.settings.status_line(self.selected_category),
            if self.selected_category == Category::Spaces {
                self.spaces_feedback
                    .as_deref()
                    .map(|feedback| format!(" - {feedback}"))
                    .unwrap_or_default()
            } else {
                error
            }
        );
        if self.selected_category == Category::Spaces {
            self.refresh_spaces_controls();
            self.relayout_if_visible();
        }
    }

    fn refresh_spaces_snapshot(&mut self) {
        let Ok(snapshot) = read_spaces_snapshot() else {
            if self.selected_category != Category::Spaces {
                return;
            }
            let changed = self.spaces_snapshot.take().is_some()
                || self.spaces_feedback.as_deref() != Some("NO LIVE COMPOSITOR SESSION");
            self.spaces_feedback = Some("NO LIVE COMPOSITOR SESSION".to_string());
            if changed {
                self.refresh_spaces_controls();
                self.refresh_labels();
            }
            return;
        };
        if !valid_spaces_snapshot(&snapshot) {
            let changed = self.spaces_snapshot.take().is_some()
                || self.spaces_feedback.as_deref() != Some("INVALID COMPOSITOR SNAPSHOT");
            self.spaces_feedback = Some("INVALID COMPOSITOR SNAPSHOT".to_string());
            if changed {
                self.refresh_spaces_controls();
                self.refresh_labels();
            }
            return;
        }
        let changed = self.spaces_snapshot.as_ref() != Some(&snapshot);
        self.spaces_snapshot = Some(snapshot);
        if changed {
            self.spaces_feedback = None;
            self.refresh_spaces_controls();
            self.refresh_labels();
        }
    }

    fn refresh_spaces_controls(&mut self) {
        let Some(snapshot) = self.spaces_snapshot.as_ref() else {
            self.spaces_rows.clear();
            self.spaces_application_policy_rows.clear();
            self.spaces_classification_button.set_enabled(false);
            for button in &mut self.spaces_action_buttons {
                button.set_enabled(false);
            }
            for button in &mut self.spaces_policy_buttons {
                button.set_enabled(false);
            }
            self.spaces_application_apply_button.set_enabled(false);
            return;
        };

        self.spaces_rows = snapshot
            .spaces
            .iter()
            .map(|space| {
                let active = if space.active { " *" } else { "" };
                let output = space
                    .output_id
                    .as_deref()
                    .map(|output| format!(" @ {output}"))
                    .unwrap_or_default();
                (
                    space.id,
                    Button::new(format!(
                        "{}  {}{}{}  ({} windows)",
                        space.order + 1,
                        space.name,
                        active,
                        output,
                        space.window_count
                    )),
                )
            })
            .collect();

        let active = snapshot.spaces.iter().find(|space| space.active);
        let has_active = active.is_some();
        for button in &mut self.spaces_action_buttons {
            button.set_enabled(has_active);
        }
        self.spaces_action_buttons[4].set_enabled(snapshot.spaces.len() > 1 && has_active);
        self.spaces_action_buttons[6].set_enabled(
            has_active
                && snapshot.multi_monitor_policy == SpacesDisplayPolicy::IndependentPerDisplay,
        );
        self.spaces_action_buttons[7].set_enabled(has_active);
        self.spaces_action_buttons[8].set_enabled(has_active);
        self.spaces_classification_button.set_enabled(has_active);
        self.spaces_classification_button
            .set_label(match active.map(|s| s.classification) {
                Some(SpaceClassification::Fullscreen) => "Use Normal Space Policy",
                _ => "Use Fullscreen Space Policy",
            });
        for (button, policy) in self.spaces_policy_buttons.iter_mut().zip([
            SpacesDisplayPolicy::SharedSpan,
            SpacesDisplayPolicy::IndependentPerDisplay,
        ]) {
            button.set_enabled(true);
            button.checked = snapshot.multi_monitor_policy == policy;
            button.set_label(match policy {
                SpacesDisplayPolicy::SharedSpan => {
                    if button.checked {
                        "Shared Across Displays *"
                    } else {
                        "Shared Across Displays"
                    }
                }
                SpacesDisplayPolicy::IndependentPerDisplay => {
                    if button.checked {
                        "Independent Per Display *"
                    } else {
                        "Independent Per Display"
                    }
                }
            });
        }

        // Application policies are authoritative readback. Keep the editor
        // enabled only when a valid active Space exists; all input is parsed
        // against this snapshot before a typed request is sent.
        self.spaces_application_apply_button.set_enabled(has_active);
        self.spaces_application_policy_rows = snapshot
            .application_policies
            .iter()
            .map(|policy| {
                let target = match policy.target {
                    SpaceTargetWire::Id { id } => format!("Space {id}"),
                    SpaceTargetWire::All => "All Spaces".to_string(),
                    // valid_spaces_snapshot rejects Current in readback, but
                    // retain a safe label if an older producer sends it.
                    SpaceTargetWire::Current => "Active Space (clear)".to_string(),
                };
                let mut button = Button::new(format!("{}  →  {}", policy.app_id, target));
                // Readback rows are informational, not mutation controls.
                button.set_enabled(false);
                button
            })
            .collect();
    }

    fn send_spaces_command(&mut self, command: SpacesControlCommand) -> bool {
        let request = SessionControlRequest::Spaces { command };
        match send_session_control(&request) {
            Ok(()) => {
                self.spaces_feedback = Some("REQUEST SENT — waiting for compositor".to_string());
                self.refresh_labels();
                true
            }
            Err(error) => {
                self.spaces_feedback = Some(format!("REQUEST FAILED: {error}"));
                self.refresh_labels();
                false
            }
        }
    }

    fn process_spaces_action(&mut self, index: usize) {
        let Some(snapshot) = self.spaces_snapshot.clone() else {
            self.spaces_feedback = Some("NO LIVE COMPOSITOR SESSION".to_string());
            self.refresh_labels();
            return;
        };
        let Some(active) = snapshot.spaces.iter().find(|space| space.active) else {
            self.spaces_feedback = Some("SNAPSHOT HAS NO ACTIVE SPACE".to_string());
            self.refresh_labels();
            return;
        };
        if index == 5 {
            let wallpaper = optional_metadata(self.spaces_wallpaper_field.text());
            let appearance = optional_metadata(self.spaces_appearance_field.text());
            if wallpaper.is_none() && appearance.is_none() {
                self.spaces_feedback = Some("ENTER WALLPAPER OR APPEARANCE FIRST".to_string());
                self.refresh_labels();
                return;
            }
            let mut sent = true;
            if wallpaper.is_some() {
                sent &= self.send_spaces_command(SpacesControlCommand::SetWallpaper {
                    id: active.id,
                    wallpaper,
                });
            }
            if let Some(appearance) = appearance {
                sent &= self.send_spaces_command(SpacesControlCommand::SetAppearance {
                    id: active.id,
                    appearance: Some(appearance),
                });
            }
            if sent {
                self.spaces_feedback = Some("METADATA REQUESTED".to_string());
                self.refresh_labels();
            }
            return;
        }
        if index == 6 {
            let output_id = self.spaces_output_field.text().trim();
            if output_id.is_empty() {
                self.spaces_feedback = Some("ENTER AN OUTPUT ID FIRST".to_string());
                self.refresh_labels();
                return;
            }
            let _ = self.send_spaces_command(SpacesControlCommand::AssignOutput {
                id: active.id,
                output_id: Some(output_id.to_string()),
            });
            return;
        }
        if index == 7 {
            let _ = self.send_spaces_command(SpacesControlCommand::AssignOutput {
                id: active.id,
                output_id: None,
            });
            return;
        }
        if index == 8 {
            let output_id = self.spaces_output_field.text().trim();
            if output_id.is_empty() {
                self.spaces_feedback = Some("ENTER AN OUTPUT ID FIRST".to_string());
                self.refresh_labels();
                return;
            }
            if output_id.chars().any(char::is_control) {
                self.spaces_feedback = Some("INVALID OUTPUT ID".to_string());
                self.refresh_labels();
                return;
            }
            let _ = self.send_spaces_command(SpacesControlCommand::MoveActiveWindowToOutput {
                output_id: output_id.to_string(),
            });
            return;
        }
        let command = match index {
            0 => {
                let name = self.spaces_name_field.text().trim();
                if name.is_empty() {
                    self.spaces_feedback = Some("ENTER A SPACE NAME FIRST".to_string());
                    self.refresh_labels();
                    return;
                }
                SpacesControlCommand::Create {
                    name: name.to_string(),
                }
            }
            1 => {
                let name = self.spaces_name_field.text().trim();
                if name.is_empty() {
                    self.spaces_feedback = Some("ENTER A SPACE NAME FIRST".to_string());
                    self.refresh_labels();
                    return;
                }
                SpacesControlCommand::Rename {
                    id: active.id,
                    name: name.to_string(),
                }
            }
            2 if active.order > 0 => SpacesControlCommand::Reorder {
                id: active.id,
                order: active.order - 1,
            },
            3 if active.order + 1 < snapshot.spaces.len() => SpacesControlCommand::Reorder {
                id: active.id,
                order: active.order + 1,
            },
            4 if snapshot.spaces.len() > 1 => SpacesControlCommand::Remove { id: active.id },
            _ => return,
        };
        let _ = self.send_spaces_command(command);
    }

    fn process_spaces_classification(&mut self) {
        let Some(snapshot) = self.spaces_snapshot.as_ref() else {
            return;
        };
        let Some(active) = snapshot.spaces.iter().find(|space| space.active) else {
            return;
        };
        let classification = match active.classification {
            SpaceClassification::Normal => SpaceClassification::Fullscreen,
            SpaceClassification::Fullscreen => SpaceClassification::Normal,
        };
        let _ = self.send_spaces_command(SpacesControlCommand::SetClassification {
            id: active.id,
            classification,
        });
    }

    fn process_spaces_application_policy(&mut self) {
        let Some(snapshot) = self.spaces_snapshot.clone() else {
            self.spaces_feedback = Some("NO LIVE COMPOSITOR SESSION".to_string());
            self.refresh_labels();
            return;
        };
        let app_id = match parse_application_id_input(self.spaces_application_id_field.text()) {
            Ok(app_id) => app_id,
            Err(feedback) => {
                self.spaces_feedback = Some(feedback.to_string());
                self.refresh_labels();
                return;
            }
        };
        let target = match parse_application_target_input(
            self.spaces_application_target_field.text(),
            &snapshot,
        ) {
            Ok(target) => target,
            Err(feedback) => {
                self.spaces_feedback = Some(feedback.to_string());
                self.refresh_labels();
                return;
            }
        };
        let _ =
            self.send_spaces_command(SpacesControlCommand::SetApplicationPolicy { app_id, target });
    }

    fn process_spaces_policy(&mut self, index: usize) {
        let policy = match index {
            0 => SpacesDisplayPolicy::SharedSpan,
            1 => SpacesDisplayPolicy::IndependentPerDisplay,
            _ => return,
        };
        let _ = self.send_spaces_command(SpacesControlCommand::SetMultiMonitorPolicy { policy });
    }

    fn select_space_from_settings(&mut self, index: usize) {
        let Some((id, _)) = self.spaces_rows.get(index) else {
            return;
        };
        let _ = self.send_spaces_command(SpacesControlCommand::Select { id: *id });
    }

    fn select_category(&mut self, category: Category) {
        self.selected_category = category;
        self.last_error = None;
        if category == Category::Spaces {
            self.refresh_spaces_snapshot();
        }
        self.refresh_labels();
        self.relayout_if_visible();
    }

    fn apply_choice(&mut self, choice: Choice) -> bool {
        let previous = self.settings.clone();
        let mut candidate = previous.clone();
        candidate.apply_choice(choice);

        // Topology changes are an end-to-end compositor operation.  Do not
        // mutate the UI mirror or persist the preference until the typed
        // request has at least reached the exact session socket.
        if choice.is_display_topology() {
            if let Err(error) = candidate.display_config().apply_arrangement_session(&[]) {
                self.last_error = Some(format!("DISPLAY APPLY {error}"));
                self.refresh_labels();
                self.relayout_if_visible();
                return false;
            }
        }

        // Runtime display policy is compositor-authoritative too. Send the
        // typed request before persistence, and refuse to save when the live
        // session is unavailable or its published capabilities reject it.
        if choice.is_display_policy() {
            if let Err(error) = candidate.display_config().apply_policy_session() {
                self.last_error = Some(format!("DISPLAY POLICY APPLY {error}"));
                self.refresh_labels();
                self.relayout_if_visible();
                return false;
            }
        }

        match self.store.save(&candidate) {
            Ok(()) => {
                self.settings = candidate;
                self.last_error = None;
                self.refresh_labels();
                self.relayout_if_visible();
                true
            }
            Err(error) => {
                // The compositor may already have accepted the topology.  Try
                // to restore the previous plan before exposing the save error
                // to the user; either way, keep the in-memory/config state at
                // the last known persisted value.
                if choice.is_display_topology() {
                    let rollback = previous.display_config().apply_arrangement_session(&[]);
                    if let Err(rollback_error) = rollback {
                        self.last_error = Some(format!(
                            "SAVE FAILED {error}; DISPLAY ROLLBACK FAILED {rollback_error}"
                        ));
                    } else {
                        self.last_error = Some(format!("SAVE FAILED {error}"));
                    }
                } else if choice.is_display_policy() {
                    if let Err(rollback_error) = previous.display_config().apply_policy_session() {
                        self.last_error = Some(format!(
                            "SAVE FAILED {error}; DISPLAY POLICY ROLLBACK FAILED {rollback_error}"
                        ));
                    } else {
                        self.last_error = Some(format!("SAVE FAILED {error}"));
                    }
                } else {
                    self.last_error = Some(format!("SAVE FAILED {error}"));
                }
                self.refresh_labels();
                self.relayout_if_visible();
                false
            }
        }
    }

    fn save_slider_value(&mut self) -> bool {
        match self.selected_category {
            Category::Sound => {
                self.settings.volume_percent = self.volume_slider.value.round() as u8;
                // Apply to the running audio stack (Pulse/PipeWire), not only conf.
                if let Err(err) = apply_system_volume(self.settings.volume_percent) {
                    self.last_error = Some(format!("VOLUME APPLY {err}"));
                    // Still persist preference so UI and conf stay in sync.
                }
            }
            Category::Mouse => {
                self.settings.pointer_speed = self.pointer_speed_slider.value.round() as u8
            }
            _ => return false,
        }

        match self.store.save(&self.settings) {
            Ok(()) => {
                if self
                    .last_error
                    .as_deref()
                    .is_none_or(|e| !e.starts_with("VOLUME APPLY"))
                {
                    self.last_error = None;
                }
                self.refresh_labels();
                self.relayout_if_visible();
                true
            }
            Err(err) => {
                self.last_error = Some(format!("SAVE FAILED {err}"));
                self.refresh_labels();
                self.relayout_if_visible();
                false
            }
        }
    }

    fn relayout_if_visible(&mut self) {
        let rect = self.rect();
        if rect.width > 0.0 && rect.height > 0.0 {
            let _ = self.layout(LayoutConstraint::tight(Size::new(rect.width, rect.height)));
        }
    }

    /// Drain widget activations after an input event went through generic
    /// dispatch: buttons record a click (`take_clicked`), sliders move their
    /// `value`; this is where those turn into app behaviour.
    fn process_activations(&mut self) {
        if let Some(index) = self
            .category_buttons
            .iter_mut()
            .position(|button| button.take_clicked())
        {
            self.select_category(Category::ALL[index]);
            return;
        }
        if self.selected_category == Category::Spaces {
            if let Some(index) = self
                .spaces_rows
                .iter_mut()
                .position(|(_, button)| button.take_clicked())
            {
                self.select_space_from_settings(index);
                return;
            }
            if let Some(index) = self
                .spaces_action_buttons
                .iter_mut()
                .position(|button| button.take_clicked())
            {
                self.process_spaces_action(index);
                return;
            }
            if let Some(index) = self
                .spaces_policy_buttons
                .iter_mut()
                .position(|button| button.take_clicked())
            {
                self.process_spaces_policy(index);
                return;
            }
            if self.spaces_classification_button.take_clicked() {
                self.process_spaces_classification();
                return;
            }
            if self.spaces_application_apply_button.take_clicked() {
                self.process_spaces_application_policy();
                return;
            }
            return;
        }
        if let Some(index) = self
            .option_buttons
            .iter_mut()
            .position(|button| button.take_clicked())
        {
            let choice = self.selected_category.choices()[index].0;
            self.apply_choice(choice);
            return;
        }
        self.sync_slider_value();
    }

    fn sync_slider_value(&mut self) {
        let changed = match self.selected_category {
            Category::Sound => {
                self.volume_slider.value.round() as u8 != self.settings.volume_percent
            }
            Category::Mouse => {
                self.pointer_speed_slider.value.round() as u8 != self.settings.pointer_speed
            }
            _ => false,
        };
        if changed {
            self.save_slider_value();
        }
    }

    fn layout_spaces_controls(&mut self, content_x: f32, content_w: f32, mut y: f32, bottom: f32) {
        let field_w = (content_w - 12.0).clamp(220.0, 520.0);
        for field in [
            &mut self.spaces_name_field,
            &mut self.spaces_wallpaper_field,
            &mut self.spaces_appearance_field,
            &mut self.spaces_output_field,
            &mut self.spaces_application_id_field,
            &mut self.spaces_application_target_field,
        ] {
            field.set_expands_horizontally(true);
            field.set_rect(Rect::new(content_x, y, field_w, 28.0));
            let _ = field.layout(LayoutConstraint::tight(Size::new(field_w, 28.0)));
            y += 34.0;
        }

        let button_w = ((content_w - 12.0) / 2.0).clamp(132.0, 240.0);
        for (index, button) in self.spaces_action_buttons.iter_mut().enumerate() {
            let col = index % 2;
            let row = index / 2;
            let x = content_x + col as f32 * (button_w + 12.0);
            let button_y = y + row as f32 * 34.0;
            button.set_rect(Rect::new(x, button_y, button_w, 28.0));
            let _ = button.layout(LayoutConstraint::tight(Size::new(button_w, 28.0)));
        }
        y += self.spaces_action_buttons.len().div_ceil(2) as f32 * 34.0 + 4.0;

        for (index, button) in self.spaces_policy_buttons.iter_mut().enumerate() {
            let x = content_x + index as f32 * (button_w + 12.0);
            button.set_rect(Rect::new(x, y, button_w, 28.0));
            let _ = button.layout(LayoutConstraint::tight(Size::new(button_w, 28.0)));
        }
        y += 34.0;

        self.spaces_classification_button
            .set_rect(Rect::new(content_x, y, button_w, 28.0));
        let _ = self
            .spaces_classification_button
            .layout(LayoutConstraint::tight(Size::new(button_w, 28.0)));
        y += 34.0;

        self.spaces_application_apply_button
            .set_rect(Rect::new(content_x, y, button_w, 28.0));
        let _ = self
            .spaces_application_apply_button
            .layout(LayoutConstraint::tight(Size::new(button_w, 28.0)));
        y += 34.0;

        for (_, button) in &mut self.spaces_rows {
            button.set_rect(Rect::new(content_x, y, content_w.min(520.0), 28.0));
            let _ = button.layout(LayoutConstraint::tight(Size::new(
                content_w.min(520.0),
                28.0,
            )));
            y += 32.0;
            if y > bottom - 48.0 {
                button.set_enabled(false);
            }
        }

        // These rows are a read-only projection of the compositor's stored
        // application policies. Give them real geometry so the authoritative
        // readback is visible instead of merely being present in the widget
        // tree with the default zero-sized rectangle.
        for button in &mut self.spaces_application_policy_rows {
            button.set_enabled(false);
            button.set_rect(Rect::new(content_x, y, content_w.min(520.0), 28.0));
            let _ = button.layout(LayoutConstraint::tight(Size::new(
                content_w.min(520.0),
                28.0,
            )));
            y += 32.0;
        }
    }
}

impl Widget for SettingsView {
    fn widget_state(&self) -> &WidgetState {
        &self.state
    }

    fn widget_state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        let size = constraint.clamp(Size::new(constraint.max_width, constraint.max_height));
        let rect = Rect::new(self.rect().x, self.rect().y, size.width, size.height);
        self.set_rect(rect);

        let sidebar_w = (rect.width * 0.28).clamp(170.0, 240.0);
        let mut y = rect.y + 12.0;
        for button in &mut self.category_buttons {
            button.set_rect(Rect::new(rect.x + 10.0, y, sidebar_w - 20.0, 24.0));
            let _ = button.layout(LayoutConstraint::tight(Size::new(sidebar_w - 20.0, 24.0)));
            y += 28.0;
        }

        let content_x = rect.x + sidebar_w + 18.0;
        let content_w = (rect.width - sidebar_w - 36.0).max(0.0);
        let mut content_y = rect.y + 20.0;

        self.heading
            .set_rect(Rect::new(content_x, content_y, content_w, 24.0));
        let _ = self
            .heading
            .layout(LayoutConstraint::tight(Size::new(content_w, 24.0)));
        content_y += 32.0;

        self.description
            .set_rect(Rect::new(content_x, content_y, content_w, 44.0));
        let _ = self
            .description
            .layout(LayoutConstraint::tight(Size::new(content_w, 44.0)));
        content_y += 56.0;

        if self.selected_category == Category::Spaces {
            self.layout_spaces_controls(content_x, content_w, content_y, rect.y + rect.height);
            self.status.set_rect(Rect::new(
                content_x,
                rect.y + rect.height - 36.0,
                content_w,
                24.0,
            ));
            let _ = self
                .status
                .layout(LayoutConstraint::tight(Size::new(content_w, 24.0)));
            return size;
        }

        let button_w = (content_w / 2.0 - 8.0).clamp(132.0, 220.0);
        for (index, button) in self.option_buttons.iter_mut().enumerate() {
            let col = index % 2;
            let row = index / 2;
            let x = content_x + col as f32 * (button_w + 12.0);
            let y = content_y + row as f32 * 38.0;
            button.set_rect(Rect::new(x, y, button_w, 28.0));
            let _ = button.layout(LayoutConstraint::tight(Size::new(button_w, 28.0)));
        }

        let slider_y = content_y + ((self.option_buttons.len() + 1) / 2) as f32 * 38.0 + 12.0;
        match self.selected_category {
            Category::Sound => {
                self.volume_label
                    .set_rect(Rect::new(content_x, slider_y, 180.0, 24.0));
                let _ = self
                    .volume_label
                    .layout(LayoutConstraint::tight(Size::new(180.0, 24.0)));
                self.volume_slider
                    .set_rect(Rect::new(content_x + 190.0, slider_y, 190.0, 24.0));
                let _ = self
                    .volume_slider
                    .layout(LayoutConstraint::tight(Size::new(190.0, 24.0)));
            }
            Category::Mouse => {
                self.pointer_speed_label
                    .set_rect(Rect::new(content_x, slider_y, 180.0, 24.0));
                let _ = self
                    .pointer_speed_label
                    .layout(LayoutConstraint::tight(Size::new(180.0, 24.0)));
                self.pointer_speed_slider.set_rect(Rect::new(
                    content_x + 190.0,
                    slider_y,
                    190.0,
                    24.0,
                ));
                let _ = self
                    .pointer_speed_slider
                    .layout(LayoutConstraint::tight(Size::new(190.0, 24.0)));
            }
            _ => {}
        }

        self.status.set_rect(Rect::new(
            content_x,
            rect.y + rect.height - 36.0,
            content_w,
            24.0,
        ));
        let _ = self
            .status
            .layout(LayoutConstraint::tight(Size::new(content_w, 24.0)));

        size
    }

    fn draw(&self, theme: &ThemeContext) {
        for button in &self.category_buttons {
            button.draw(theme);
        }
        self.heading.draw(theme);
        self.description.draw(theme);
        if self.selected_category == Category::Spaces {
            self.spaces_name_field.draw(theme);
            self.spaces_wallpaper_field.draw(theme);
            self.spaces_appearance_field.draw(theme);
            self.spaces_output_field.draw(theme);
            self.spaces_application_id_field.draw(theme);
            self.spaces_application_target_field.draw(theme);
            for button in &self.spaces_action_buttons {
                button.draw(theme);
            }
            for button in &self.spaces_policy_buttons {
                button.draw(theme);
            }
            self.spaces_classification_button.draw(theme);
            self.spaces_application_apply_button.draw(theme);
            for (_, button) in &self.spaces_rows {
                button.draw(theme);
            }
            for button in &self.spaces_application_policy_rows {
                button.draw(theme);
            }
            self.status.draw(theme);
            return;
        }
        for button in &self.option_buttons {
            button.draw(theme);
        }
        match self.selected_category {
            Category::Sound => {
                self.volume_label.draw(theme);
                self.volume_slider.draw(theme);
            }
            Category::Mouse => {
                self.pointer_speed_label.draw(theme);
                self.pointer_speed_slider.draw(theme);
            }
            _ => {}
        }
        self.status.draw(theme);
    }

    fn handle_event(&mut self, event: &Event) -> EventResult {
        match event {
            // Tab / Shift+Tab walk the focusable widgets in tree order.
            Event::KeyDown {
                key: KeyCode::Tab,
                modifiers,
            } => {
                let mut focus = std::mem::take(&mut self.focus);
                if modifiers.shift {
                    focus.focus_prev(self);
                } else {
                    focus.focus_next(self);
                }
                self.focus = focus;
                EventResult::Handled
            }
            // Every other key goes to the focused widget (Enter/Space
            // activate the focused button).
            Event::KeyDown { .. } | Event::KeyUp { .. } | Event::Char { .. } => {
                let mut focus = std::mem::take(&mut self.focus);
                let result = focus.dispatch_key(self, event);
                self.focus = focus;
                if matches!(result, EventResult::Handled) {
                    self.process_activations();
                }
                result
            }
            // Pointer events go through generic rect-checked dispatch with
            // implicit capture; no hand-rolled hit-testing.
            Event::MouseDown { .. }
            | Event::MouseUp { .. }
            | Event::MouseMove { .. }
            | Event::DoubleClick { .. }
            | Event::MouseLeave => {
                let mut pointer = std::mem::take(&mut self.pointer);
                let result = pointer.dispatch(self, event);
                self.pointer = pointer;
                if matches!(result, EventResult::Handled) {
                    self.process_activations();
                }
                result
            }
            _ => EventResult::Ignored,
        }
    }

    fn update(&mut self) {
        if self.selected_category == Category::Spaces {
            self.refresh_spaces_snapshot();
        }
        for button in &mut self.category_buttons {
            button.update();
        }
        self.heading.update();
        self.description.update();
        for button in &mut self.option_buttons {
            button.update();
        }
        self.volume_label.update();
        self.volume_slider.update();
        self.pointer_speed_label.update();
        self.pointer_speed_slider.update();
        self.spaces_name_field.update();
        self.spaces_wallpaper_field.update();
        self.spaces_appearance_field.update();
        self.spaces_output_field.update();
        self.spaces_application_id_field.update();
        self.spaces_application_target_field.update();
        for (_, button) in &mut self.spaces_rows {
            button.update();
        }
        for button in &mut self.spaces_action_buttons {
            button.update();
        }
        for button in &mut self.spaces_policy_buttons {
            button.update();
        }
        self.spaces_classification_button.update();
        self.spaces_application_apply_button.update();
        self.status.update();
    }

    fn accessibility(&self) -> Option<AccessibilityNode> {
        // The SDK recursively projects this root through the live widget
        // children below. Keeping the root semantic (rather than returning
        // None) means Settings is discoverable before a category-specific
        // control tree is selected and lets the sync path publish live
        // focus, bounds and text changes for every visible control.
        Some(AccessibilityNode::new(
            AccessibilityRole::Window,
            "Settings",
        ))
    }

    fn children(&self) -> Vec<&dyn Widget> {
        let mut children: Vec<&dyn Widget> = Vec::new();
        for button in &self.category_buttons {
            children.push(button);
        }
        children.push(&self.heading);
        children.push(&self.description);
        for button in &self.option_buttons {
            children.push(button);
        }
        match self.selected_category {
            Category::Sound => {
                children.push(&self.volume_label);
                children.push(&self.volume_slider);
            }
            Category::Mouse => {
                children.push(&self.pointer_speed_label);
                children.push(&self.pointer_speed_slider);
            }
            Category::Spaces => {
                children.push(&self.spaces_name_field);
                children.push(&self.spaces_wallpaper_field);
                children.push(&self.spaces_appearance_field);
                children.push(&self.spaces_output_field);
                children.push(&self.spaces_application_id_field);
                children.push(&self.spaces_application_target_field);
                for button in &self.spaces_action_buttons {
                    children.push(button);
                }
                for button in &self.spaces_policy_buttons {
                    children.push(button);
                }
                children.push(&self.spaces_classification_button);
                children.push(&self.spaces_application_apply_button);
                for (_, button) in &self.spaces_rows {
                    children.push(button);
                }
                for button in &self.spaces_application_policy_rows {
                    children.push(button);
                }
            }
            _ => {}
        }
        children.push(&self.status);
        children
    }

    fn children_mut(&mut self) -> Vec<&mut dyn Widget> {
        let mut children: Vec<&mut dyn Widget> = Vec::new();
        for button in &mut self.category_buttons {
            children.push(button);
        }
        children.push(&mut self.heading);
        children.push(&mut self.description);
        for button in &mut self.option_buttons {
            children.push(button);
        }
        match self.selected_category {
            Category::Sound => {
                children.push(&mut self.volume_label);
                children.push(&mut self.volume_slider);
            }
            Category::Mouse => {
                children.push(&mut self.pointer_speed_label);
                children.push(&mut self.pointer_speed_slider);
            }
            Category::Spaces => {
                children.push(&mut self.spaces_name_field);
                children.push(&mut self.spaces_wallpaper_field);
                children.push(&mut self.spaces_appearance_field);
                children.push(&mut self.spaces_output_field);
                children.push(&mut self.spaces_application_id_field);
                children.push(&mut self.spaces_application_target_field);
                for button in &mut self.spaces_action_buttons {
                    children.push(button);
                }
                for button in &mut self.spaces_policy_buttons {
                    children.push(button);
                }
                children.push(&mut self.spaces_classification_button);
                children.push(&mut self.spaces_application_apply_button);
                for (_, button) in &mut self.spaces_rows {
                    children.push(button);
                }
                for button in &mut self.spaces_application_policy_rows {
                    children.push(button);
                }
            }
            _ => {}
        }
        children.push(&mut self.status);
        children
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slopos_bus::SessionControlListener;
    use slopos_kit::event::MouseButton;
    use slopos_kit::Point;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);
    static SESSION_RUNTIME_ENV_LOCK: Mutex<()> = Mutex::new(());

    fn temp_settings_path() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir()
            .join(format!("slopos-i_settings_{unique}_{sequence}"))
            .join("settings.conf")
    }

    fn assert_handled(result: EventResult) {
        assert!(matches!(result, EventResult::Handled), "result={result:?}");
    }

    fn mouse_down(point: Point) -> Event {
        Event::MouseDown {
            button: MouseButton::Left,
            point,
            modifiers: Modifiers::NONE,
        }
    }

    fn mouse_up(point: Point) -> Event {
        Event::MouseUp {
            button: MouseButton::Left,
            point,
            modifiers: Modifiers::NONE,
        }
    }

    /// A real click is press + release inside the same rect; activation
    /// happens on the release.
    fn click(view: &mut SettingsView, rect: Rect) {
        let point = Point::new(rect.x + 2.0, rect.y + 2.0);
        let down = view.handle_event(&mouse_down(point));
        assert!(
            matches!(down, EventResult::Handled),
            "down={down:?} rect={rect:?}"
        );
        let up = view.handle_event(&mouse_up(point));
        assert!(
            matches!(up, EventResult::Handled),
            "up={up:?} rect={rect:?}"
        );
    }

    fn click_and_report(view: &mut SettingsView, rect: Rect) -> bool {
        let before = view.settings.clone();
        click(view, rect);
        view.settings != before
    }

    fn key_down(key: KeyCode, modifiers: Modifiers) -> Event {
        Event::KeyDown { key, modifiers }
    }

    #[test]
    fn settings_store_persists_all_supported_values() {
        let path = temp_settings_path();
        let store = SettingsStore::new(path.clone());
        let state = SettingsState {
            appearance: AppearanceMode::Dark,
            theme: "dracula".to_string(),
            desktop_icons: false,
            dock_position: "right".to_string(),
            hdr_requested: true,
            vrr_adaptive: true,
            refresh_rate: "120hz".to_string(),
            color_space: "rec2020".to_string(),
            arrange_mode: "mirror".to_string(),
            scale_percent: 200,
            sound_effects: false,
            volume_percent: 35,
            network_profile: "offline".to_string(),
            keyboard_repeat: "slow".to_string(),
            natural_scroll: true,
            pointer_speed: 85,
            assistive_ui: true,
            privacy_mode: "strict".to_string(),
            notifications: false,
        };

        store.save(&state).unwrap();
        assert_eq!(store.load(), state);
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("arrange_mode=mirror"));
        assert!(content.contains("scale_percent=200"));
    }

    #[cfg(unix)]
    #[test]
    fn settings_display_arrange_sends_typed_request_before_persisting() {
        with_control_runtime(|_runtime, listener| {
            let path = temp_settings_path();
            let store = SettingsStore::new(path.clone());
            let mut view = SettingsView::load(store);
            view.select_category(Category::Display);
            view.set_rect(Rect::new(0.0, 0.0, 640.0, 520.0));
            view.layout(LayoutConstraint::tight(Size::new(640.0, 520.0)));

            let mirror_rect = view
                .option_buttons
                .iter()
                .find(|b| b.label.contains("Arrange Mirror"))
                .expect("Arrange Mirror button")
                .rect();
            click(&mut view, mirror_rect);

            let loaded = SettingsStore::new(path).load();
            assert_eq!(loaded.arrange_mode, "mirror");
            assert_eq!(view.settings.arrange_mode, "mirror");
            assert!(view.status.text.contains("ARRANGE MIRROR"));
            assert_eq!(
                listener.drain(),
                vec![SessionControlRequest::ReconfigureOutputs {
                    layout: "eDP-1:1920x1080@0,0:s100".to_string()
                }]
            );

            let layout = std::env::var("SLOPOS_OUTPUTS_LAYOUT")
                .expect("successful typed apply mirrors the accepted layout");
            assert!(layout.contains("eDP-1"), "layout={layout}");
            std::env::remove_var("SLOPOS_OUTPUTS_LAYOUT");
        });
    }

    #[cfg(unix)]
    #[test]
    fn settings_display_arrange_rolls_back_when_session_is_unavailable() {
        let _environment_guard = SESSION_RUNTIME_ENV_LOCK.lock().unwrap();
        let previous_runtime = std::env::var_os("SLOPOS_SESSION_RUNTIME_DIR");
        std::env::remove_var("SLOPOS_SESSION_RUNTIME_DIR");
        std::env::remove_var("SLOPOS_OUTPUTS_LAYOUT");

        let path = temp_settings_path();
        let mut view = SettingsView::load(SettingsStore::new(path.clone()));
        view.select_category(Category::Display);
        view.set_rect(Rect::new(0.0, 0.0, 640.0, 520.0));
        view.layout(LayoutConstraint::tight(Size::new(640.0, 520.0)));
        let mirror_rect = view
            .option_buttons
            .iter()
            .find(|b| b.label.contains("Arrange Mirror"))
            .expect("Arrange Mirror button")
            .rect();
        assert!(!view.settings.arrange_mode.eq("mirror"));
        assert!(!click_and_report(&mut view, mirror_rect));

        assert_eq!(view.settings.arrange_mode, "extend_right");
        assert_eq!(SettingsStore::new(path).load().arrange_mode, "extend_right");
        assert!(view.status.text.contains("DISPLAY APPLY"));
        assert!(view.status.text.contains("session compositor unavailable"));
        assert!(std::env::var_os("SLOPOS_OUTPUTS_LAYOUT").is_none());

        if let Some(previous_runtime) = previous_runtime {
            std::env::set_var("SLOPOS_SESSION_RUNTIME_DIR", previous_runtime);
        }
    }

    #[cfg(unix)]
    #[test]
    fn settings_display_arrange_rolls_back_compositor_when_persistence_fails() {
        with_control_runtime(|_runtime, listener| {
            let parent = temp_settings_path().with_extension("parent");
            fs::create_dir_all(parent.parent().unwrap()).unwrap();
            fs::write(&parent, "not a directory").unwrap();
            let path = parent.join("settings.conf");
            let mut view = SettingsView::load(SettingsStore::new(path));
            view.select_category(Category::Display);
            view.set_rect(Rect::new(0.0, 0.0, 640.0, 520.0));
            view.layout(LayoutConstraint::tight(Size::new(640.0, 520.0)));
            let mirror_rect = view
                .option_buttons
                .iter()
                .find(|b| b.label.contains("Arrange Mirror"))
                .expect("Arrange Mirror button")
                .rect();

            assert!(!click_and_report(&mut view, mirror_rect));
            assert_eq!(view.settings.arrange_mode, "extend_right");
            assert!(view.status.text.contains("SAVE FAILED"));
            assert_eq!(
                listener.drain(),
                vec![
                    SessionControlRequest::ReconfigureOutputs {
                        layout: "eDP-1:1920x1080@0,0:s100".to_string()
                    },
                    SessionControlRequest::ReconfigureOutputs {
                        layout: "eDP-1:1920x1080@0,0:s100".to_string()
                    }
                ]
            );
            let _ = fs::remove_file(parent);
        });
    }

    #[test]
    fn settings_save_preserves_lock_password_and_unknown_keys() {
        let path = temp_settings_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(
            &path,
            "theme=classic\nlock_password=secret123\ncustom_key=keepme\n",
        )
        .unwrap();

        let store = SettingsStore::new(path.clone());
        let mut state = store.load();
        state.theme = "solarized".to_string();
        store.save(&state).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("lock_password=secret123"));
        assert!(content.contains("custom_key=keepme"));
        assert!(content.contains("theme=solarized"));
    }

    #[test]
    fn settings_save_is_atomic_leaves_only_destination_file() {
        let path = temp_settings_path();
        let store = SettingsStore::new(path.clone());
        let state = SettingsState::default();
        store.save(&state).unwrap();

        // A crash or a second reader mid-write must never observe a stray
        // `.tmp.` file or a missing/truncated destination: the temp file used
        // by write_atomic() is renamed away (not copied), so exactly one file
        // should exist in the directory afterward.
        let dir_entries: Vec<String> = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(dir_entries, vec!["settings.conf".to_string()]);
        assert_eq!(store.load(), state);
    }

    #[test]
    fn settings_save_survives_concurrent_writers_without_corruption() {
        let path = temp_settings_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        // Seed an unknown key up front, the way another component
        // (e.g. the lock screen writing lock_password) would.
        fs::write(&path, "lock_password=seed\n").unwrap();

        let store = SettingsStore::new(path.clone());
        let handles: Vec<_> = (0..8u8)
            .map(|i| {
                let store = store.clone();
                std::thread::spawn(move || {
                    let mut state = SettingsState::default();
                    state.volume_percent = i * 10;
                    store.save(&state).unwrap();
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }

        // Regardless of which writer's rename landed last, the destination
        // must be a single well-formed file: every line is a full `key=value`
        // pair, no key appears twice (which would indicate an interleaved,
        // non-atomic write), and the unknown key survives every writer's
        // read-modify-write.
        let content = fs::read_to_string(&path).unwrap();
        let mut seen_keys = std::collections::HashSet::new();
        for line in content.lines() {
            let (key, _value) = line
                .split_once('=')
                .expect("atomic save must never produce a partial line");
            assert!(!key.is_empty());
            assert!(
                seen_keys.insert(key.to_string()),
                "duplicate key {key} indicates a corrupted, non-atomic write"
            );
        }
        assert!(seen_keys.contains("volume_percent"));
        assert!(content.contains("lock_password=seed"));

        let leftover_tmp = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .any(|entry| entry.file_name().to_string_lossy().contains(".tmp."));
        assert!(
            !leftover_tmp,
            "atomic save must not leave temp files behind"
        );
    }

    #[test]
    fn volume_cli_args_match_shipped_backends() {
        let pactl = volume_pactl_args(42);
        assert_eq!(pactl[0], "set-sink-volume");
        assert_eq!(pactl[1], "@DEFAULT_SINK@");
        assert_eq!(pactl[2], "42%");
        assert_eq!(volume_pactl_args(200)[2], "100%");

        let wpctl = volume_wpctl_args(50);
        assert_eq!(wpctl[0], "set-volume");
        assert_eq!(wpctl[1], "@DEFAULT_AUDIO_SINK@");
        assert_eq!(wpctl[2], "0.50");
    }

    #[test]
    fn settings_loads_all_eight_theme_names() {
        for theme in [
            "classic",
            "dark",
            "grape",
            "blueberry",
            "strawberry",
            "solarized",
            "dracula",
            "highcontrast",
        ] {
            let path = temp_settings_path();
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, format!("theme={theme}\n")).unwrap();
            let loaded = SettingsStore::new(path).load();
            assert_eq!(loaded.theme, theme);
        }
    }

    #[test]
    fn settings_category_click_rebuilds_visible_options() {
        let store = SettingsStore::new(temp_settings_path());
        let mut view = SettingsView::load(store);
        view.set_rect(Rect::new(0.0, 0.0, 640.0, 420.0));
        view.layout(LayoutConstraint::tight(Size::new(640.0, 420.0)));

        let display_rect = view.category_buttons[3].rect();
        click(&mut view, display_rect);

        assert_eq!(view.selected_category, Category::Display);
        assert!(view.heading.text.contains("DISPLAY"));
        assert!(view
            .option_buttons
            .iter()
            .any(|button| button.label.contains("HDR")));
        assert!(view
            .option_buttons
            .iter()
            .any(|button| button.label.contains("VRR")));
        assert!(view
            .option_buttons
            .iter()
            .all(|button| button.rect().width > 0.0));
    }

    #[test]
    fn settings_option_click_updates_and_saves_state() {
        with_control_runtime(|_runtime, _listener| {
            let path = temp_settings_path();
            let store = SettingsStore::new(path.clone());
            let mut view = SettingsView::load(store);
            view.select_category(Category::Display);
            view.set_rect(Rect::new(0.0, 0.0, 640.0, 420.0));
            view.layout(LayoutConstraint::tight(Size::new(640.0, 420.0)));

            let hdr_rect = view.option_buttons[1].rect();
            click(&mut view, hdr_rect);

            let loaded = SettingsStore::new(path).load();
            assert!(loaded.hdr_requested);
            assert!(view.status.text.contains("HDR REQUESTED"));
        });
    }

    #[test]
    fn settings_click_outside_every_widget_is_ignored() {
        let store = SettingsStore::new(temp_settings_path());
        let mut view = SettingsView::load(store);
        view.set_rect(Rect::new(0.0, 0.0, 640.0, 420.0));
        view.layout(LayoutConstraint::tight(Size::new(640.0, 420.0)));
        let before = view.selected_category;

        // Dead space between the sidebar and the option grid.
        let point = Point::new(620.0, 410.0);
        assert!(matches!(
            view.handle_event(&mouse_down(point)),
            EventResult::Ignored
        ));
        assert!(matches!(
            view.handle_event(&mouse_up(point)),
            EventResult::Ignored
        ));
        assert_eq!(view.selected_category, before);
    }

    #[test]
    fn settings_press_and_release_on_different_buttons_activates_neither() {
        let store = SettingsStore::new(temp_settings_path());
        let mut view = SettingsView::load(store);
        view.set_rect(Rect::new(0.0, 0.0, 640.0, 420.0));
        view.layout(LayoutConstraint::tight(Size::new(640.0, 420.0)));
        assert_eq!(view.selected_category, Category::Appearance);

        let general = view.category_buttons[0].rect();
        let display = view.category_buttons[3].rect();

        // Press on General, drag to Display, release: pointer capture sends
        // the release to General (outside its rect -> press cancelled), so
        // neither category activates.
        assert_handled(
            view.handle_event(&mouse_down(Point::new(general.x + 2.0, general.y + 2.0))),
        );
        let _ = view.handle_event(&mouse_up(Point::new(display.x + 2.0, display.y + 2.0)));
        assert_eq!(view.selected_category, Category::Appearance);
    }

    #[test]
    fn settings_tab_then_space_activates_category_via_keyboard() {
        let store = SettingsStore::new(temp_settings_path());
        let mut view = SettingsView::load(store);
        view.set_rect(Rect::new(0.0, 0.0, 640.0, 420.0));
        view.layout(LayoutConstraint::tight(Size::new(640.0, 420.0)));
        assert_eq!(view.selected_category, Category::Appearance);

        // Tab lands on the first focusable widget: the General category button.
        assert_handled(view.handle_event(&key_down(KeyCode::Tab, Modifiers::NONE)));
        assert!(view.category_buttons[0].widget_state().focused);
        assert!(!view.category_buttons[1].widget_state().focused);

        // Space activates it.
        assert_handled(view.handle_event(&key_down(KeyCode::Space, Modifiers::NONE)));
        assert_eq!(view.selected_category, Category::General);

        // Shift+Tab from the first widget wraps to the last focusable one.
        let shift = Modifiers {
            shift: true,
            control: false,
            alt: false,
            meta: false,
        };
        assert_handled(view.handle_event(&key_down(KeyCode::Tab, shift)));
        assert!(!view.category_buttons[0].widget_state().focused);
    }

    #[test]
    fn settings_slider_drag_keeps_tracking_outside_its_rect() {
        let path = temp_settings_path();
        let store = SettingsStore::new(path.clone());
        let mut view = SettingsView::load(store);
        view.select_category(Category::Sound);
        view.set_rect(Rect::new(0.0, 0.0, 640.0, 420.0));
        view.layout(LayoutConstraint::tight(Size::new(640.0, 420.0)));

        let slider = view.volume_slider.rect();
        // Press mid-track, then drag far off the slider (and off its row):
        // implicit pointer capture must keep routing the motion to it.
        assert_handled(view.handle_event(&mouse_down(Point::new(
            slider.x + slider.width / 2.0,
            slider.y + 12.0,
        ))));
        assert_handled(view.handle_event(&Event::MouseMove {
            point: Point::new(slider.x - 200.0, slider.y + 150.0),
            modifiers: Modifiers::NONE,
        }));
        assert_handled(
            view.handle_event(&mouse_up(Point::new(slider.x - 200.0, slider.y + 150.0))),
        );

        // Dragged all the way left of the track -> clamped to 0 and saved.
        let loaded = SettingsStore::new(path).load();
        assert_eq!(loaded.volume_percent, 0);
        assert!(!view.volume_slider.dragging);
    }

    #[test]
    fn settings_sound_slider_updates_and_saves_volume() {
        let path = temp_settings_path();
        let store = SettingsStore::new(path.clone());
        let mut view = SettingsView::load(store);
        view.select_category(Category::Sound);
        view.set_rect(Rect::new(0.0, 0.0, 640.0, 420.0));
        view.layout(LayoutConstraint::tight(Size::new(640.0, 420.0)));

        let slider = view.volume_slider.rect();
        assert_handled(view.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point: Point::new(slider.x + slider.width - 9.0, slider.y + 12.0),
            modifiers: Modifiers::NONE,
        }));

        let loaded = SettingsStore::new(path).load();
        assert_eq!(loaded.volume_percent, 100);
        assert!(view.status.text.contains("VOLUME 100%"));
    }

    #[cfg(unix)]
    fn spaces_snapshot_for_settings() -> SpacesSnapshot {
        SpacesSnapshot {
            session_epoch: 7,
            revision: 3,
            active_space: 11,
            multi_monitor_policy: SpacesDisplayPolicy::SharedSpan,
            application_policies: Vec::new(),
            spaces: vec![
                slopos_bus::SpaceSnapshot {
                    id: 11,
                    order: 0,
                    name: "Personal".to_string(),
                    active: true,
                    window_count: 1,
                    wallpaper: None,
                    appearance: None,
                    classification: SpaceClassification::Normal,
                    output_id: None,
                },
                slopos_bus::SpaceSnapshot {
                    id: 22,
                    order: 1,
                    name: "Projects".to_string(),
                    active: false,
                    window_count: 2,
                    wallpaper: None,
                    appearance: None,
                    classification: SpaceClassification::Normal,
                    output_id: None,
                },
            ],
        }
    }

    #[cfg(unix)]
    fn with_control_runtime(test: impl FnOnce(&Path, &SessionControlListener)) {
        let _environment_guard = SESSION_RUNTIME_ENV_LOCK.lock().unwrap();
        let runtime = PathBuf::from("/tmp").join(format!(
            "slo-s-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&runtime).unwrap();
        let listener = SessionControlListener::bind(&runtime).unwrap();
        let previous_runtime = std::env::var_os("SLOPOS_SESSION_RUNTIME_DIR");
        std::env::set_var("SLOPOS_SESSION_RUNTIME_DIR", &runtime);
        test(&runtime, &listener);
        if let Some(previous_runtime) = previous_runtime {
            std::env::set_var("SLOPOS_SESSION_RUNTIME_DIR", previous_runtime);
        } else {
            std::env::remove_var("SLOPOS_SESSION_RUNTIME_DIR");
        }
        drop(listener);
        fs::remove_dir_all(runtime).unwrap();
    }

    #[cfg(unix)]
    fn with_spaces_runtime(test: impl FnOnce(&Path, &SessionControlListener)) {
        with_control_runtime(|runtime, listener| {
            slopos_bus::write_spaces_snapshot(&spaces_snapshot_for_settings()).unwrap();
            test(runtime, listener);
        });
    }

    #[cfg(unix)]
    #[test]
    fn settings_spaces_sends_authoritative_mutations() {
        with_spaces_runtime(|_runtime, listener| {
            let mut view = SettingsView::load(SettingsStore::new(temp_settings_path()));
            view.select_category(Category::Spaces);
            view.set_rect(Rect::new(0.0, 0.0, 720.0, 720.0));
            view.layout(LayoutConstraint::tight(Size::new(720.0, 720.0)));

            view.spaces_name_field.set_text("Writing");
            let rect = view.spaces_action_buttons[0].rect();
            click(&mut view, rect);
            assert_eq!(
                listener.drain(),
                vec![SessionControlRequest::Spaces {
                    command: SpacesControlCommand::Create {
                        name: "Writing".to_string()
                    }
                }]
            );

            let rect = view.spaces_action_buttons[1].rect();
            click(&mut view, rect);
            assert_eq!(
                listener.drain(),
                vec![SessionControlRequest::Spaces {
                    command: SpacesControlCommand::Rename {
                        id: 11,
                        name: "Writing".to_string()
                    }
                }]
            );

            let rect = view.spaces_action_buttons[3].rect();
            click(&mut view, rect);
            assert_eq!(
                listener.drain(),
                vec![SessionControlRequest::Spaces {
                    command: SpacesControlCommand::Reorder { id: 11, order: 1 }
                }]
            );

            let rect = view.spaces_action_buttons[4].rect();
            click(&mut view, rect);
            assert_eq!(
                listener.drain(),
                vec![SessionControlRequest::Spaces {
                    command: SpacesControlCommand::Remove { id: 11 }
                }]
            );

            view.spaces_wallpaper_field.set_text("wallpaper.png");
            view.spaces_appearance_field.set_text("graphite");
            let rect = view.spaces_action_buttons[5].rect();
            click(&mut view, rect);
            assert_eq!(
                listener.drain(),
                vec![
                    SessionControlRequest::Spaces {
                        command: SpacesControlCommand::SetWallpaper {
                            id: 11,
                            wallpaper: Some("wallpaper.png".to_string())
                        }
                    },
                    SessionControlRequest::Spaces {
                        command: SpacesControlCommand::SetAppearance {
                            id: 11,
                            appearance: Some("graphite".to_string())
                        }
                    }
                ]
            );

            let rect = view.spaces_policy_buttons[1].rect();
            click(&mut view, rect);
            assert_eq!(
                listener.drain(),
                vec![SessionControlRequest::Spaces {
                    command: SpacesControlCommand::SetMultiMonitorPolicy {
                        policy: SpacesDisplayPolicy::IndependentPerDisplay
                    }
                }]
            );

            let mut independent = spaces_snapshot_for_settings();
            independent.multi_monitor_policy = SpacesDisplayPolicy::IndependentPerDisplay;
            independent.revision = 4;
            slopos_bus::write_spaces_snapshot(&independent).unwrap();
            view.update();
            view.spaces_output_field.set_text("DP-1");
            let rect = view.spaces_action_buttons[6].rect();
            click(&mut view, rect);
            assert_eq!(
                listener.drain(),
                vec![SessionControlRequest::Spaces {
                    command: SpacesControlCommand::AssignOutput {
                        id: 11,
                        output_id: Some("DP-1".to_string())
                    }
                }]
            );

            let rect = view.spaces_action_buttons[7].rect();
            click(&mut view, rect);
            assert_eq!(
                listener.drain(),
                vec![SessionControlRequest::Spaces {
                    command: SpacesControlCommand::AssignOutput {
                        id: 11,
                        output_id: None
                    }
                }]
            );

            let rect = view.spaces_classification_button.rect();
            click(&mut view, rect);
            assert_eq!(
                listener.drain(),
                vec![SessionControlRequest::Spaces {
                    command: SpacesControlCommand::SetClassification {
                        id: 11,
                        classification: SpaceClassification::Fullscreen
                    }
                }]
            );

            let rect = view.spaces_rows[1].1.rect();
            click(&mut view, rect);
            assert_eq!(
                listener.drain(),
                vec![SessionControlRequest::Spaces {
                    command: SpacesControlCommand::Select { id: 22 }
                }]
            );
        });
    }

    #[cfg(unix)]
    #[test]
    fn settings_spaces_moves_active_window_to_selected_output() {
        with_spaces_runtime(|_runtime, listener| {
            let mut snapshot = spaces_snapshot_for_settings();
            snapshot.multi_monitor_policy = SpacesDisplayPolicy::IndependentPerDisplay;
            snapshot.revision = 4;
            slopos_bus::write_spaces_snapshot(&snapshot).unwrap();

            let mut view = SettingsView::load(SettingsStore::new(temp_settings_path()));
            view.select_category(Category::Spaces);
            view.set_rect(Rect::new(0.0, 0.0, 720.0, 720.0));
            view.layout(LayoutConstraint::tight(Size::new(720.0, 720.0)));

            view.spaces_output_field.set_text("HDMI-A-1");
            let move_rect = view.spaces_action_buttons[8].rect();
            click(&mut view, move_rect);
            assert_eq!(
                listener.drain(),
                vec![SessionControlRequest::Spaces {
                    command: SpacesControlCommand::MoveActiveWindowToOutput {
                        output_id: "HDMI-A-1".to_string(),
                    },
                }]
            );
        });
    }

    #[cfg(unix)]
    #[test]
    fn settings_spaces_rejects_empty_active_window_output_target() {
        with_spaces_runtime(|_runtime, listener| {
            let mut view = SettingsView::load(SettingsStore::new(temp_settings_path()));
            view.select_category(Category::Spaces);
            view.set_rect(Rect::new(0.0, 0.0, 720.0, 720.0));
            view.layout(LayoutConstraint::tight(Size::new(720.0, 720.0)));

            let move_rect = view.spaces_action_buttons[8].rect();
            click(&mut view, move_rect);
            assert!(listener.drain().is_empty());
            assert!(view.status.text.contains("ENTER AN OUTPUT ID FIRST"));
        });
    }

    #[cfg(unix)]
    #[test]
    fn settings_spaces_reconciles_new_compositor_snapshot() {
        with_spaces_runtime(|runtime, _listener| {
            let mut view = SettingsView::load(SettingsStore::new(temp_settings_path()));
            view.select_category(Category::Spaces);
            view.set_rect(Rect::new(0.0, 0.0, 720.0, 720.0));
            view.layout(LayoutConstraint::tight(Size::new(720.0, 720.0)));
            assert!(view.spaces_rows[0].1.label.contains("Personal"));

            let mut next = spaces_snapshot_for_settings();
            next.revision = 4;
            next.active_space = 22;
            next.spaces[0].active = false;
            next.spaces[1].active = true;
            next.spaces[1].name = "Renamed Projects".to_string();
            next.application_policies = vec![slopos_bus::ApplicationSpacePolicySnapshot {
                app_id: "org.example.Editor".to_string(),
                target: SpaceTargetWire::Id { id: 22 },
            }];
            slopos_bus::write_spaces_snapshot(&next).unwrap();
            view.update();

            assert!(view.spaces_rows[1].1.label.contains("Renamed Projects"));
            assert!(view.spaces_rows[1].1.label.contains(" *"));
            assert_eq!(view.spaces_application_policy_rows.len(), 1);
            assert!(view.spaces_application_policy_rows[0]
                .label
                .contains("org.example.Editor  →  Space 22"));
            assert!(view.spaces_application_policy_rows[0].rect().width > 0.0);
            assert!(view.status.text.contains("SPACES"));
            assert!(runtime.join(slopos_bus::SPACES_STATE_FILE).exists());
        });
    }

    #[cfg(unix)]
    #[test]
    fn settings_spaces_rejects_invalid_application_policy_projection() {
        let mut snapshot = spaces_snapshot_for_settings();
        snapshot.application_policies = vec![slopos_bus::ApplicationSpacePolicySnapshot {
            app_id: "org.example.Editor".to_string(),
            target: SpaceTargetWire::Id { id: 22 },
        }];
        assert!(valid_spaces_snapshot(&snapshot));

        snapshot.application_policies[0].target = SpaceTargetWire::Current;
        assert!(!valid_spaces_snapshot(&snapshot));
        snapshot.application_policies[0].target = SpaceTargetWire::Id { id: 99 };
        assert!(!valid_spaces_snapshot(&snapshot));
        snapshot.application_policies[0].app_id = "org.example.\nEditor".to_string();
        assert!(!valid_spaces_snapshot(&snapshot));
    }

    #[cfg(unix)]
    #[test]
    fn settings_spaces_renders_application_policy_readback_rows() {
        with_spaces_runtime(|_runtime, _listener| {
            let mut snapshot = spaces_snapshot_for_settings();
            snapshot.application_policies = vec![
                slopos_bus::ApplicationSpacePolicySnapshot {
                    app_id: "org.example.Editor".to_string(),
                    target: SpaceTargetWire::Id { id: 22 },
                },
                slopos_bus::ApplicationSpacePolicySnapshot {
                    app_id: "org.example.Terminal".to_string(),
                    target: SpaceTargetWire::All,
                },
            ];
            slopos_bus::write_spaces_snapshot(&snapshot).unwrap();

            let mut view = SettingsView::load(SettingsStore::new(temp_settings_path()));
            view.select_category(Category::Spaces);
            view.set_rect(Rect::new(0.0, 0.0, 720.0, 720.0));
            view.layout(LayoutConstraint::tight(Size::new(720.0, 720.0)));

            assert_eq!(view.spaces_application_policy_rows.len(), 2);
            assert!(view.spaces_application_policy_rows[0]
                .label
                .contains("org.example.Editor"));
            assert!(view.spaces_application_policy_rows[0].rect().width > 0.0);
            assert!(view.spaces_application_policy_rows[1]
                .label
                .contains("All Spaces"));
            assert!(
                view.spaces_application_policy_rows[1].rect().y
                    > view.spaces_application_policy_rows[0].rect().y
            );
            assert!(
                !view.spaces_application_policy_rows[0]
                    .widget_state()
                    .enabled
            );
        });
    }

    #[cfg(unix)]
    #[test]
    fn settings_spaces_application_policy_targets_id_all_and_current() {
        with_spaces_runtime(|_runtime, listener| {
            let mut view = SettingsView::load(SettingsStore::new(temp_settings_path()));
            view.select_category(Category::Spaces);
            view.set_rect(Rect::new(0.0, 0.0, 720.0, 720.0));
            view.layout(LayoutConstraint::tight(Size::new(720.0, 720.0)));
            view.spaces_application_id_field
                .set_text("org.example.Editor");

            view.spaces_application_target_field.set_text("22");
            let apply_rect = view.spaces_application_apply_button.rect();
            click(&mut view, apply_rect);
            assert_eq!(
                listener.drain(),
                vec![SessionControlRequest::Spaces {
                    command: SpacesControlCommand::SetApplicationPolicy {
                        app_id: "org.example.Editor".to_string(),
                        target: SpaceTargetWire::Id { id: 22 },
                    }
                }]
            );

            view.spaces_application_target_field.set_text("all");
            let apply_rect = view.spaces_application_apply_button.rect();
            click(&mut view, apply_rect);
            assert_eq!(
                listener.drain(),
                vec![SessionControlRequest::Spaces {
                    command: SpacesControlCommand::SetApplicationPolicy {
                        app_id: "org.example.Editor".to_string(),
                        target: SpaceTargetWire::All,
                    }
                }]
            );

            view.spaces_application_target_field.set_text(" current ");
            let apply_rect = view.spaces_application_apply_button.rect();
            click(&mut view, apply_rect);
            assert_eq!(
                listener.drain(),
                vec![SessionControlRequest::Spaces {
                    command: SpacesControlCommand::SetApplicationPolicy {
                        app_id: "org.example.Editor".to_string(),
                        target: SpaceTargetWire::Current,
                    }
                }]
            );
        });
    }

    #[cfg(unix)]
    #[test]
    fn settings_spaces_application_policy_rejects_invalid_input_without_request() {
        with_spaces_runtime(|_runtime, listener| {
            let mut view = SettingsView::load(SettingsStore::new(temp_settings_path()));
            view.select_category(Category::Spaces);
            view.set_rect(Rect::new(0.0, 0.0, 720.0, 720.0));
            view.layout(LayoutConstraint::tight(Size::new(720.0, 720.0)));

            view.spaces_application_id_field
                .set_text("org.example.\nEditor");
            view.spaces_application_target_field.set_text("all");
            let apply_rect = view.spaces_application_apply_button.rect();
            click(&mut view, apply_rect);
            assert!(listener.drain().is_empty());
            assert!(view.status.text.contains("INVALID APPLICATION ID"));

            view.spaces_application_id_field.set_text("");
            let apply_rect = view.spaces_application_apply_button.rect();
            click(&mut view, apply_rect);
            assert!(listener.drain().is_empty());
            assert!(view.status.text.contains("ENTER AN APPLICATION ID"));

            view.spaces_application_id_field
                .set_text("org.example.Editor");
            view.spaces_application_target_field.set_text("999");
            let apply_rect = view.spaces_application_apply_button.rect();
            click(&mut view, apply_rect);
            assert!(listener.drain().is_empty());
            assert!(view.status.text.contains("UNKNOWN SPACE ID"));

            view.spaces_application_target_field.set_text(" ");
            let apply_rect = view.spaces_application_apply_button.rect();
            click(&mut view, apply_rect);
            assert!(listener.drain().is_empty());
            assert!(view.status.text.contains("ENTER A SPACE ID"));
        });
    }
}
