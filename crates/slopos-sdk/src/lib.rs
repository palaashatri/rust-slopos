#![allow(dead_code)]

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use slopos_bus::{
    send_session_control, ApplicationControlListener, ApplicationMenuRequest,
    SessionControlRequest, SloposBus, WindowPresentationAction,
};
use slopos_kit::button::Button;
use slopos_kit::design_tokens::{
    CLASSIC_DARK_GRAY_RGBA, CLASSIC_FACE_ALT_RGBA, CLASSIC_FACE_RGBA, CLASSIC_INK_RGBA,
    CLASSIC_LAVENDER_DARK_RGBA, CLASSIC_LAVENDER_RGBA, CLASSIC_MID_LIGHT_RGBA, CLASSIC_MID_RGBA,
    CLASSIC_PALETTE, CLASSIC_PAPER_RGBA, MENU_BAR_HEIGHT, MENU_ITEM_HEIGHT, MENU_LABEL_INSET,
    MENU_SHADOW_OFFSET, MENU_SHORTCUT_INSET, WINDOW_CONTROL_SIZE, WINDOW_TITLE_BAR_HEIGHT,
};
use slopos_kit::dialog::Dialog;
use slopos_kit::dock_view::DockView;
use slopos_kit::event::{KeyCode, Modifiers, MouseButton};
use slopos_kit::icon_view::{IconItem, IconView, DESKTOP_ITEM_WIDTH};
use slopos_kit::label::Label;
use slopos_kit::layout::{Layout, LayoutView};
use slopos_kit::list_view::ListView;
use slopos_kit::menu::{Menu, MenuItem, MenuItemKind};
use slopos_kit::menu_bar::MenuBar;
use slopos_kit::panel::Panel;
use slopos_kit::popup_button::PopupButton;
use slopos_kit::progress_bar::ProgressBar;
use slopos_kit::scroll_view::ScrollView;
use slopos_kit::slider::Slider;
use slopos_kit::split_view::SplitView;
use slopos_kit::status_bar::StatusBar;
use slopos_kit::tab_view::TabView;
use slopos_kit::text_field::TextField;
use slopos_kit::theme::ThemeToken;
use slopos_kit::toolbar::Toolbar;
use slopos_kit::tree_view::{TreeNode, TreeView};
use slopos_kit::window::{hit_test_window_chrome, Window, WindowChromeHit};
use slopos_kit::workspace_grid_view::WorkspaceGridView;
use slopos_kit::{
    accessibility_tree_from_widget, at_spi_connection_available, default_accessibility_tree,
    register_at_spi_app_with_tree, sync_at_spi_registered_tree, AccessibilityTree, Color,
    ImageView, LayoutConstraint, MonospaceView, Point, Rect, Size, Widget, WidgetId,
};
use slopos_render::font::{
    ellipsize_text as render_ellipsize_text, shape_text, ShapedGlyph, TextLayout, TextLayoutOptions,
};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use wgpu::util::DeviceExt;
use winit::event_loop::EventLoopProxy;

#[cfg(target_os = "linux")]
use winit::platform::wayland::WindowAttributesExtWayland;

static RENDER_DARK_MODE: AtomicBool = AtomicBool::new(false);
static RENDER_ACCENT_COLOR: Mutex<[f32; 4]> = Mutex::new([0.36, 0.54, 0.85, 1.0]); // default Mac OS 7 blue

/// Snaps a float value to the nearest integer pixel.
pub fn snap_to_pixel(val: f32) -> f32 {
    val.round()
}

/// Snaps a 2D point (x, y) to integer pixel boundaries.
pub fn snap_point_to_pixel(x: f32, y: f32) -> (f32, f32) {
    (x.round(), y.round())
}

/// Snaps a rectangle to integer pixel boundaries.
pub fn snap_rect_to_pixel(rect: Rect) -> Rect {
    let x = rect.x.round();
    let y = rect.y.round();
    let width = rect.width.round().max(1.0);
    let height = rect.height.round().max(1.0);
    Rect::new(x, y, width, height)
}

/// Snaps 1-pixel strokes to half-pixel raster alignment.
pub fn snap_stroke_1px(val: f32) -> f32 {
    val.floor() + 0.5
}

// System 7 Classic palette is owned by slopos-kit; the SDK only aliases the
// renderer-friendly values for its immediate-mode presenter.
const S7_BG: [f32; 4] = CLASSIC_PAPER_RGBA;
const S7_FG: [f32; 4] = CLASSIC_INK_RGBA;
const S7_GRAY100: [f32; 4] = CLASSIC_FACE_RGBA;
const S7_GRAY200: [f32; 4] = CLASSIC_FACE_ALT_RGBA;
const S7_GRAY300: [f32; 4] = CLASSIC_MID_LIGHT_RGBA;
const S7_GRAY400: [f32; 4] = CLASSIC_MID_RGBA;
const S7_GRAY500: [f32; 4] = CLASSIC_DARK_GRAY_RGBA;
const S7_LAVENDER100: [f32; 4] = CLASSIC_LAVENDER_RGBA;
const S7_LAVENDER300: [f32; 4] = CLASSIC_LAVENDER_DARK_RGBA;

const COLOR_PLATINUM_BG: [f32; 4] = S7_GRAY100;
const COLOR_BUTTON_BG: [f32; 4] = S7_GRAY100;
const COLOR_BUTTON_HOVER: [f32; 4] = S7_GRAY200;
const COLOR_WINDOW_BORDER: [f32; 4] = S7_FG;
const COLOR_TEXT_PRIMARY: [f32; 4] = S7_FG;
const COLOR_TEXT_SECONDARY: [f32; 4] = S7_GRAY500;
const COLOR_SELECTION_BG: [f32; 4] = [0.39, 0.59, 0.86, 1.0]; // classic Mac blue
const COLOR_SELECTION_TEXT: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const COLOR_FOCUS_RING: [f32; 4] = [0.39, 0.59, 0.86, 1.0];
const COLOR_EDGE_LIGHT: [f32; 4] = S7_BG;
const COLOR_EDGE_DARK: [f32; 4] = S7_GRAY500;

// Graphite / dark mode
const COLOR_DARK_BG: [f32; 4] = [0.14, 0.14, 0.15, 1.0];
const COLOR_DARK_BUTTON_BG: [f32; 4] = [0.22, 0.22, 0.24, 1.0];
const COLOR_DARK_BUTTON_HOVER: [f32; 4] = [0.30, 0.30, 0.34, 1.0];
const COLOR_DARK_BORDER: [f32; 4] = [0.02, 0.02, 0.02, 1.0];
const COLOR_DARK_TEXT: [f32; 4] = [0.92, 0.92, 0.93, 1.0];
const COLOR_DARK_TITLE_INACTIVE: [f32; 4] = [0.72, 0.74, 0.80, 1.0];
const COLOR_DARK_EDGE_LIGHT: [f32; 4] = [0.42, 0.42, 0.45, 1.0];
const COLOR_DARK_EDGE_DARK: [f32; 4] = [0.02, 0.02, 0.02, 1.0];
const COLOR_DARK_MENU: [f32; 4] = [0.18, 0.18, 0.19, 1.0];

fn theme_face() -> [f32; 4] {
    if render_dark_mode() {
        COLOR_DARK_BUTTON_BG
    } else {
        S7_GRAY100
    }
}

fn theme_menu() -> [f32; 4] {
    if render_dark_mode() {
        COLOR_DARK_MENU
    } else {
        S7_BG
    }
}

fn theme_paper() -> [f32; 4] {
    if render_dark_mode() {
        COLOR_DARK_BG
    } else {
        S7_BG
    }
}

fn theme_ink() -> [f32; 4] {
    if render_dark_mode() {
        COLOR_DARK_TEXT
    } else {
        S7_FG
    }
}

fn theme_muted() -> [f32; 4] {
    if render_dark_mode() {
        COLOR_DARK_EDGE_LIGHT
    } else {
        S7_GRAY400
    }
}

fn classic_palette_rgba(token: ThemeToken) -> [f32; 4] {
    let color = CLASSIC_PALETTE.color(token);
    [color.r, color.g, color.b, color.a]
}

fn inactive_title_color(is_dark: bool) -> [f32; 4] {
    if is_dark {
        COLOR_DARK_TITLE_INACTIVE
    } else {
        classic_palette_rgba(ThemeToken::WindowTitleInactive)
    }
}

fn set_render_dark_mode(is_dark: bool) {
    RENDER_DARK_MODE.store(is_dark, Ordering::Relaxed);
}

fn render_dark_mode() -> bool {
    RENDER_DARK_MODE.load(Ordering::Relaxed)
}

fn set_render_accent(color: [f32; 4]) {
    *RENDER_ACCENT_COLOR.lock() = color;
}

fn render_accent() -> [f32; 4] {
    *RENDER_ACCENT_COLOR.lock()
}

fn request_compositor_window_action(action: WindowPresentationAction) {
    let request = SessionControlRequest::FocusedWindow { action };
    if let Err(error) = send_session_control(&request) {
        tracing::debug!(?action, %error, "compositor window action unavailable");
    }
}

/// Get a color value based on current theme (light/dark mode).
/// Maps semantic color names to System 7 palette values.
fn theme_color(color_name: &str) -> [f32; 4] {
    if render_dark_mode() {
        match color_name {
            "window_bg" => COLOR_DARK_BG,
            "button_bg" => COLOR_DARK_BUTTON_BG,
            "button_hover" => COLOR_DARK_BUTTON_HOVER,
            "border" => COLOR_DARK_BORDER,
            "text" => COLOR_DARK_TEXT,
            "window_title_inactive" => COLOR_DARK_TITLE_INACTIVE,
            "edge_light" => COLOR_DARK_EDGE_LIGHT,
            "edge_dark" => COLOR_DARK_EDGE_DARK,
            _ => [0.5, 0.5, 0.5, 1.0], // fallback gray
        }
    } else {
        match color_name {
            "window_bg" => COLOR_PLATINUM_BG,
            "button_bg" => COLOR_BUTTON_BG,
            "button_hover" => COLOR_BUTTON_HOVER,
            "border" => COLOR_WINDOW_BORDER,
            "text" => COLOR_TEXT_PRIMARY,
            "window_title_inactive" => inactive_title_color(false),
            "edge_light" => COLOR_EDGE_LIGHT,
            "edge_dark" => COLOR_EDGE_DARK,
            _ => [0.5, 0.5, 0.5, 1.0], // fallback gray
        }
    }
}

/// Apply both dark mode flag and accent color together (used when theme changes).
pub fn apply_theme(is_dark: bool, accent: [f32; 4]) {
    set_render_dark_mode(is_dark);
    set_render_accent(accent);
}

/// Accent color definitions for each named theme.
pub mod theme_accents {
    /// Classic (Mac OS 7 Platinum) — blue
    pub const CLASSIC: [f32; 4] = [0.36, 0.54, 0.85, 1.0];
    /// Dark — same blue in dark mode
    pub const DARK: [f32; 4] = [0.36, 0.54, 0.85, 1.0];
    /// Grape — purple
    pub const GRAPE: [f32; 4] = [0.55, 0.28, 0.72, 1.0];
    /// Blueberry — deep blue
    pub const BLUEBERRY: [f32; 4] = [0.15, 0.25, 0.62, 1.0];
    /// Strawberry — red-pink
    pub const STRAWBERRY: [f32; 4] = [0.82, 0.23, 0.28, 1.0];
    /// Solarized — #268bd2 (matches ThemeName::Solarized in slopos-shell)
    pub const SOLARIZED: [f32; 4] = [0.15, 0.55, 0.82, 1.0];
    /// Dracula — #bd93f9
    pub const DRACULA: [f32; 4] = [0.74, 0.58, 0.98, 1.0];
    /// HighContrast — yellow
    pub const HIGH_CONTRAST: [f32; 4] = [1.0, 1.0, 0.0, 1.0];
}

/// Read settings.conf and return (is_dark, accent_color) for the current theme.
fn load_theme_preference() -> (bool, [f32; 4]) {
    let config_dir = std::env::var_os("SLOPOS_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".config/slopos-i"))
        })
        .unwrap_or_else(|| PathBuf::from("/tmp/slopos-i"));
    let path = config_dir.join("settings.conf");
    let Ok(content) = std::fs::read_to_string(path) else {
        return (false, theme_accents::CLASSIC);
    };
    parse_theme_preference(&content)
}

fn parse_theme_preference(content: &str) -> (bool, [f32; 4]) {
    let mut theme_name: Option<String> = None;
    let mut appearance: Option<String> = None;
    for line in content.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "theme" => theme_name = Some(value.trim().to_ascii_lowercase()),
            "appearance" => appearance = Some(value.trim().to_ascii_lowercase()),
            _ => {}
        }
    }
    // Named theme takes precedence over appearance
    if let Some(name) = theme_name {
        // Must stay in sync with slopos_shell::theme_manager::ThemeName
        // (accent + is_dark); a name missing here silently renders as Classic.
        return match name.as_str() {
            "grape" => (true, theme_accents::GRAPE),
            "blueberry" => (true, theme_accents::BLUEBERRY),
            "strawberry" => (false, theme_accents::STRAWBERRY),
            "dark" => (true, theme_accents::DARK),
            "solarized" => (true, theme_accents::SOLARIZED),
            "dracula" => (true, theme_accents::DRACULA),
            "highcontrast" => (false, theme_accents::HIGH_CONTRAST),
            _ => (false, theme_accents::CLASSIC), // classic and unknown
        };
    }
    // Fall back to appearance key
    let is_dark = appearance.as_deref().map(|a| a == "dark").unwrap_or(false);
    let accent = if is_dark {
        theme_accents::DARK
    } else {
        theme_accents::CLASSIC
    };
    (is_dark, accent)
}

pub fn menu_manifest_dir() -> Option<PathBuf> {
    std::env::var_os("SLOPOS_MENU_MANIFEST_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("XDG_RUNTIME_DIR")
                .map(|runtime| PathBuf::from(runtime).join("slopos-i").join("menus"))
        })
}

pub fn global_menu_mode_enabled() -> bool {
    std::env::var_os("SLOPOS_GLOBAL_MENU")
        .and_then(|value| value.into_string().ok())
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn sanitize_manifest_name(name: &str) -> String {
    name.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn publish_bytes_atomically(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let Some(parent) = path.parent() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "manifest path has no parent directory",
        ));
    };
    let stem = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("manifest");
    let temporary_path = parent.join(format!(
        ".{stem}.{}.{}.tmp",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary_path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary_path, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

fn ui(light: [f32; 4], dark: [f32; 4]) -> [f32; 4] {
    if render_dark_mode() {
        dark
    } else {
        light
    }
}

pub type MenuActionHandler = Box<dyn FnMut(&str, &mut Window)>;

/// A thread-safe wake handle for work that arrives outside the Winit event
/// loop. Applications use this for bounded background work such as Vision
/// job completion; the SDK installs the concrete event-loop proxy when
/// [`Application::run`] starts.
#[derive(Clone, Default)]
pub struct EventLoopWaker(Arc<OnceLock<EventLoopProxy<()>>>);

impl EventLoopWaker {
    /// Wake the application event loop if it is running. Calling this before
    /// `run` is harmless, which lets a view be constructed before startup.
    pub fn wake(&self) {
        if let Some(proxy) = self.0.get() {
            let _ = proxy.send_event(());
        }
    }

    fn install(&self, proxy: EventLoopProxy<()>) {
        let _ = self.0.set(proxy);
    }
}

pub struct Application {
    pub name: String,
    pub bundle_id: String,
    pub main_window: Option<Window>,
    pub initial_size: Size,
    pub menus: Vec<Menu>,
    pub bus: Option<SloposBus>,
    pub running: bool,
    menu_action_handler: Option<MenuActionHandler>,
    event_waker: EventLoopWaker,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MenuManifest {
    pub app_name: String,
    pub bundle_id: String,
    pub menus: Vec<Menu>,
    pub updated_at_millis: u64,
}

impl Application {
    pub fn new(name: &str, bundle_id: &str) -> Self {
        Self {
            name: name.to_string(),
            bundle_id: bundle_id.to_string(),
            main_window: None,
            initial_size: Size::new(960.0, 640.0),
            menus: vec![],
            bus: None,
            running: false,
            menu_action_handler: None,
            event_waker: EventLoopWaker::default(),
        }
    }

    /// Return a cloneable handle that wakes this application's event loop.
    /// The handle is useful to views that complete work on a background
    /// thread and must request a single redraw without polling while idle.
    pub fn event_waker(&self) -> EventLoopWaker {
        self.event_waker.clone()
    }

    pub fn with_bus(mut self, bus: SloposBus) -> Self {
        self.bus = Some(bus);
        self
    }

    /// Register the application-owned receiver for actions selected in the
    /// compositor-wide global menu. The shell only forwards the action id;
    /// application state and document semantics stay inside the client.
    pub fn on_menu_action<F>(&mut self, handler: F)
    where
        F: FnMut(&str, &mut Window) + 'static,
    {
        self.menu_action_handler = Some(Box::new(handler));
    }

    pub fn set_main_window(&mut self, window: Window) {
        self.main_window = Some(window);
    }

    pub fn set_initial_size(&mut self, size: Size) {
        self.initial_size = Size::new(size.width.max(1.0), size.height.max(1.0));
    }

    pub fn set_menus(&mut self, menus: Vec<Menu>) {
        self.menus = menus;
    }

    fn complete_menus(&self) -> Vec<Menu> {
        let mut menus = self.menus.clone();
        let mut app_menu = Menu::new(&self.name);
        app_menu.add_action(format!("About {}", self.name));
        app_menu.add_separator();
        app_menu.add_action(format!("Hide {}", self.name));
        app_menu.add_separator();
        app_menu.add_action(format!("Quit {}", self.name));
        menus.insert(0, app_menu);
        assign_default_menu_actions(&mut menus, &self.bundle_id);
        menus
    }

    pub fn menu_manifest(&self) -> MenuManifest {
        MenuManifest {
            app_name: self.name.clone(),
            bundle_id: self.bundle_id.clone(),
            menus: self.complete_menus(),
            updated_at_millis: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        }
    }

    pub fn publish_menu_manifest(&self) -> std::io::Result<Option<PathBuf>> {
        if self.menus.is_empty() {
            return Ok(None);
        }

        let Some(dir) = menu_manifest_dir() else {
            return Ok(None);
        };
        fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.json", sanitize_manifest_name(&self.bundle_id)));
        let json =
            serde_json::to_vec_pretty(&self.menu_manifest()).map_err(std::io::Error::other)?;

        // The shell polls this directory while clients are starting and while
        // a client can republish its menu after a live configuration change.
        // Never expose a partially-written JSON document to the shell: publish
        // into the same directory and rename, which is atomic on the session's
        // filesystem.
        publish_bytes_atomically(&path, &json)?;
        Ok(Some(path))
    }

    pub fn run(&mut self) {
        if let Err(err) = self.publish_menu_manifest() {
            tracing::warn!("failed to publish menu manifest: {err}");
        }
        self.running = true;
        tracing::info!("Application '{}' started", self.name);

        let event_loop = match slopos_render::event_loop::RetroEventLoop::new() {
            Ok(event_loop) => event_loop,
            Err(err) => {
                tracing::error!(
                    app = %self.name,
                    wayland_display = ?std::env::var("WAYLAND_DISPLAY").ok(),
                    display = ?std::env::var("DISPLAY").ok(),
                    "cannot start: no display server connection: {err}"
                );
                eprintln!(
                    "[{}] cannot start: no Wayland/X11 display server reachable ({err})",
                    self.name
                );
                std::process::exit(1);
            }
        };
        self.event_waker.install(event_loop.proxy());
        let main_window = self.main_window.take();

        struct AppHandler {
            name: String,
            bundle_id: String,
            window: Option<Window>,
            initial_size: Size,
            platform_window: Option<Arc<winit::window::Window>>,
            presenter: Option<WgpuPresenter>,
            modifiers: winit::keyboard::ModifiersState,
            cursor_position: Point,
            last_click: Option<(MouseButton, Point, std::time::Instant)>,
            dirty: bool,
            dark_mode: bool,
            accent_color: [f32; 4],
            scale: f32,
            control_requests: Option<mpsc::Receiver<ApplicationMenuRequest>>,
            menu_action_handler: Option<MenuActionHandler>,
            accessibility_registration_started: bool,
            last_accessibility_registration_attempt: Option<std::time::Instant>,
        }

        impl AppHandler {
            fn modifiers(&self) -> Modifiers {
                modifiers_from_winit(self.modifiers)
            }

            /// Snapshot the live window for the toolkit accessibility bridge.
            /// A window-less application still exports a truthful minimal
            /// application child, while a missing D-Bus session remains a
            /// non-fatal best-effort condition inside the kit registration
            /// helpers.
            fn accessibility_tree(&self) -> AccessibilityTree {
                self.window
                    .as_ref()
                    .map(|window| accessibility_tree_from_widget(window))
                    .unwrap_or_else(|| default_accessibility_tree(&self.name))
            }

            fn register_initial_accessibility(&mut self) {
                self.accessibility_registration_started = true;
                self.last_accessibility_registration_attempt = Some(std::time::Instant::now());
                let tree = self.accessibility_tree();
                if let Err(error) = register_at_spi_app_with_tree(&self.name, &tree) {
                    tracing::warn!(
                        app = %self.name,
                        %error,
                        "initial AT-SPI accessibility export failed; continuing without external bus"
                    );
                }
            }

            fn maybe_retry_accessibility_registration(&mut self) {
                // A headless startup may have no session/a11y bus yet.  Keep
                // that path non-fatal, but retry at a bounded cadence so a
                // bus that appears shortly after launch can still receive the
                // live tree.  Avoid a registration attempt on every frame.
                const RETRY_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);
                if !self.accessibility_registration_started
                    || at_spi_connection_available()
                    || self
                        .last_accessibility_registration_attempt
                        .is_some_and(|attempt| attempt.elapsed() < RETRY_INTERVAL)
                {
                    return;
                }
                self.register_initial_accessibility();
            }

            fn sync_accessibility(&mut self) {
                self.maybe_retry_accessibility_registration();
                let tree = self.accessibility_tree();
                match sync_at_spi_registered_tree(&tree) {
                    Ok(_) => {}
                    Err(error) => {
                        tracing::warn!(
                            app = %self.name,
                            %error,
                            "AT-SPI accessibility snapshot sync failed; continuing"
                        );
                    }
                }
            }

            fn dispatch(&mut self, event: slopos_kit::Event) -> slopos_kit::EventResult {
                let result = if let Some(ref mut win) = self.window {
                    win.handle_event(&event)
                } else {
                    slopos_kit::EventResult::Ignored
                };
                // Dispatch can mutate labels, focus, enabled state, or the
                // dynamic child list. Publish the resulting snapshot before
                // asking the platform window to redraw.
                self.sync_accessibility();
                self.dirty = true;
                if let Some(window) = &self.platform_window {
                    window.request_redraw();
                }
                result
            }

            fn layout_window(&mut self, width: u32, height: u32) {
                if let Some(ref mut win) = self.window {
                    let logical_width = (width as f32 / self.scale).max(1.0);
                    let logical_height = (height as f32 / self.scale).max(1.0);
                    let size = Size::new(logical_width, logical_height);
                    win.set_rect(Rect::new(0.0, 0.0, size.width, size.height));
                    win.layout(LayoutConstraint::tight(size));
                    self.dirty = true;
                }
                self.sync_accessibility();
            }

            fn drain_menu_actions(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
                let requests = self
                    .control_requests
                    .as_ref()
                    .map(|receiver| receiver.try_iter().collect::<Vec<_>>())
                    .unwrap_or_default();
                for request in requests {
                    if request.bundle_id != self.bundle_id {
                        continue;
                    }
                    if is_application_menu_action(&request.action_id, &self.bundle_id, &self.name)
                        == Some(ApplicationMenuAction::Quit)
                    {
                        tracing::info!(
                            bundle_id = %self.bundle_id,
                            "application quit selected from global menu"
                        );
                        event_loop.exit();
                        continue;
                    }
                    if is_application_menu_action(&request.action_id, &self.bundle_id, &self.name)
                        == Some(ApplicationMenuAction::Hide)
                    {
                        request_compositor_window_action(WindowPresentationAction::Minimize);
                        continue;
                    }
                    let mut handler = self.menu_action_handler.take();
                    if let (Some(window), Some(menu_action_handler)) =
                        (self.window.as_mut(), handler.as_mut())
                    {
                        menu_action_handler(&request.action_id, window);
                        self.dirty = true;
                        if let Some(platform_window) = &self.platform_window {
                            platform_window.request_redraw();
                        }
                        self.sync_accessibility();
                        tracing::info!(
                            bundle_id = %self.bundle_id,
                            action_id = %request.action_id,
                            "application handled global menu action"
                        );
                    } else {
                        tracing::warn!(
                            bundle_id = %self.bundle_id,
                            action_id = %request.action_id,
                            "application received global menu action without a handler"
                        );
                    }
                    self.menu_action_handler = handler;
                }
            }

            fn paint(&mut self) {
                // Re-layout before drawing. update() can swap in entirely new
                // content (lock screen fields, a new terminal tab, dialogs);
                // without this those widgets keep Rect::ZERO until the next
                // resize and draw_widget skips them, painting an empty window.
                if let Some(ref mut win) = self.window {
                    let size = Size::new(win.rect().width, win.rect().height);
                    if size.width > 0.0 && size.height > 0.0 {
                        win.layout(LayoutConstraint::tight(size));
                    }
                }
                self.sync_accessibility();
                let Some(window) = &self.window else {
                    return;
                };
                let Some(presenter) = &mut self.presenter else {
                    return;
                };
                apply_theme(self.dark_mode, self.accent_color);
                let scale = self.scale;
                if let Err(err) = presenter.render(|canvas| {
                    canvas.set_scale(scale);
                    draw_desktop_backdrop(canvas);
                    draw_window(canvas, window);
                }) {
                    tracing::error!("failed to render frame: {err}");
                } else {
                    self.dirty = false;
                }
            }
        }

        impl slopos_render::event_loop::RetroAppHandler for AppHandler {
            fn init(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
                let initial_size = self.initial_size;
                // No winit/Adwaita CSD — classic Mac chrome is drawn by the kit
                // (title bar) and the global menu lives in slopos-shell layer-shell.
                let attrs = winit::window::Window::default_attributes()
                    .with_title(&self.name)
                    .with_inner_size(winit::dpi::LogicalSize::new(
                        initial_size.width,
                        initial_size.height,
                    ))
                    .with_decorations(false);

                // Publish the SDK bundle identity on the Wayland xdg_toplevel.
                // Without this, the compositor sees every first-party client as
                // `slopos-i.app`, so authoritative focus cannot select the
                // corresponding global-menu manifest.
                #[cfg(target_os = "linux")]
                let attrs = attrs.with_name(self.bundle_id.clone(), self.name.clone());

                match event_loop.create_window(attrs) {
                    Ok(window) => {
                        let window = Arc::new(window);
                        self.scale = window.scale_factor() as f32;
                        let size = window.inner_size();
                        match futures::executor::block_on(WgpuPresenter::new(window.clone())) {
                            Ok(presenter) => {
                                self.layout_window(size.width, size.height);
                                self.register_initial_accessibility();
                                window.request_redraw();
                                self.presenter = Some(presenter);
                                self.platform_window = Some(window);
                            }
                            Err(err) => {
                                tracing::error!("failed to create presenter: {err}");
                                event_loop.exit();
                            }
                        }
                    }
                    Err(err) => {
                        tracing::error!("failed to create application window: {err}");
                        event_loop.exit();
                    }
                }
            }

            fn handle_window_event(
                &mut self,
                event_loop: &winit::event_loop::ActiveEventLoop,
                event: winit::event::WindowEvent,
            ) {
                match event {
                    winit::event::WindowEvent::CloseRequested => event_loop.exit(),
                    winit::event::WindowEvent::RedrawRequested => self.paint(),
                    winit::event::WindowEvent::Resized(size) => {
                        if let Some(presenter) = &mut self.presenter {
                            presenter.resize(size.width, size.height);
                        }
                        self.layout_window(size.width, size.height);
                        if let Some(window) = &self.platform_window {
                            window.request_redraw();
                        }
                    }
                    winit::event::WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                        self.scale = scale_factor as f32;
                        let size_and_win = self
                            .platform_window
                            .as_ref()
                            .map(|w| (w.inner_size(), w.clone()));
                        if let Some((size, window)) = size_and_win {
                            if let Some(presenter) = &mut self.presenter {
                                presenter.resize(size.width, size.height);
                            }
                            self.layout_window(size.width, size.height);
                            window.request_redraw();
                        }
                    }
                    winit::event::WindowEvent::ModifiersChanged(new_mods) => {
                        self.modifiers = new_mods.state();
                    }
                    winit::event::WindowEvent::CursorMoved { position, .. } => {
                        let scale = self.scale;
                        self.cursor_position =
                            Point::new(position.x as f32 / scale, position.y as f32 / scale);
                        if let Some(window) = &self.platform_window {
                            let logical = window.inner_size().to_logical::<f32>(self.scale as f64);
                            let hit = hit_test_window_chrome(
                                self.cursor_position,
                                Size::new(logical.width, logical.height),
                            );
                            window.set_cursor(match hit {
                                WindowChromeHit::ResizeSouthEast => {
                                    winit::window::CursorIcon::NwseResize
                                }
                                _ => winit::window::CursorIcon::Default,
                            });
                        }
                        let _ = self.dispatch(slopos_kit::Event::MouseMove {
                            point: self.cursor_position,
                            modifiers: self.modifiers(),
                        });
                    }
                    winit::event::WindowEvent::CursorEntered { .. } => {
                        let _ = self.dispatch(slopos_kit::Event::MouseEnter);
                    }
                    winit::event::WindowEvent::CursorLeft { .. } => {
                        let _ = self.dispatch(slopos_kit::Event::MouseLeave);
                    }
                    winit::event::WindowEvent::MouseInput { state, button, .. } => {
                        if let Some(button) = winit_to_retro_mouse_button(button) {
                            let now = std::time::Instant::now();
                            let is_double_click = state == winit::event::ElementState::Pressed
                                && self
                                    .last_click
                                    .as_ref()
                                    .map(|(last_button, last_point, last_time)| {
                                        *last_button == button
                                            && now.duration_since(*last_time)
                                                <= std::time::Duration::from_millis(500)
                                            && distance_squared(*last_point, self.cursor_position)
                                                <= 16.0
                                    })
                                    .unwrap_or(false);

                            if state == winit::event::ElementState::Pressed {
                                self.last_click = Some((button, self.cursor_position, now));
                            }

                            if button == MouseButton::Left
                                && state == winit::event::ElementState::Pressed
                            {
                                if let Some(window) = &self.platform_window {
                                    let logical =
                                        window.inner_size().to_logical::<f32>(self.scale as f64);
                                    match hit_test_window_chrome(
                                        self.cursor_position,
                                        Size::new(logical.width, logical.height),
                                    ) {
                                        WindowChromeHit::Close => {
                                            event_loop.exit();
                                            return;
                                        }
                                        WindowChromeHit::Zoom => {
                                            request_compositor_window_action(
                                                WindowPresentationAction::ToggleZoom,
                                            );
                                            return;
                                        }
                                        WindowChromeHit::Titlebar => {
                                            if is_double_click {
                                                request_compositor_window_action(
                                                    WindowPresentationAction::ToggleZoom,
                                                );
                                            } else if let Err(err) = window.drag_window() {
                                                tracing::warn!("failed to request compositor window move: {err}");
                                            }
                                            return;
                                        }
                                        WindowChromeHit::ResizeSouthEast => {
                                            if let Err(err) = window.drag_resize_window(
                                                winit::window::ResizeDirection::SouthEast,
                                            ) {
                                                tracing::warn!(
                                                    "failed to request compositor resize: {err}"
                                                );
                                            }
                                            return;
                                        }
                                        WindowChromeHit::Content => {}
                                    }
                                }
                            }

                            let event = match state {
                                winit::event::ElementState::Pressed if is_double_click => {
                                    slopos_kit::Event::DoubleClick {
                                        button,
                                        point: self.cursor_position,
                                        modifiers: self.modifiers(),
                                    }
                                }
                                winit::event::ElementState::Pressed => {
                                    slopos_kit::Event::MouseDown {
                                        button,
                                        point: self.cursor_position,
                                        modifiers: self.modifiers(),
                                    }
                                }
                                winit::event::ElementState::Released => {
                                    slopos_kit::Event::MouseUp {
                                        button,
                                        point: self.cursor_position,
                                        modifiers: self.modifiers(),
                                    }
                                }
                            };
                            let _ = self.dispatch(event);
                        }
                    }
                    winit::event::WindowEvent::MouseWheel { delta, .. } => {
                        let delta = winit_to_retro_scroll_delta(delta);
                        let _ = self.dispatch(slopos_kit::Event::Scroll {
                            delta,
                            modifiers: self.modifiers(),
                        });
                    }
                    winit::event::WindowEvent::Focused(true) => {
                        let _ = self.dispatch(slopos_kit::Event::FocusIn);
                    }
                    winit::event::WindowEvent::Focused(false) => {
                        let _ = self.dispatch(slopos_kit::Event::FocusOut);
                    }
                    winit::event::WindowEvent::KeyboardInput {
                        event: key_event, ..
                    } => {
                        let mut handled = false;
                        if let winit::keyboard::PhysicalKey::Code(phys_key) = key_event.physical_key
                        {
                            if let Some(rkey) = winit_to_retro_key(phys_key) {
                                let retro_event = match key_event.state {
                                    winit::event::ElementState::Pressed => {
                                        slopos_kit::Event::KeyDown {
                                            key: rkey,
                                            modifiers: self.modifiers(),
                                        }
                                    }
                                    winit::event::ElementState::Released => {
                                        slopos_kit::Event::KeyUp {
                                            key: rkey,
                                            modifiers: self.modifiers(),
                                        }
                                    }
                                };
                                handled = matches!(
                                    self.dispatch(retro_event),
                                    slopos_kit::EventResult::Handled
                                        | slopos_kit::EventResult::StopPropagation
                                );
                            }
                        }
                        if key_event.state == winit::event::ElementState::Pressed && !handled {
                            if let Some(ref text) = key_event.text {
                                for character in text.chars() {
                                    if !character.is_control() {
                                        let _ =
                                            self.dispatch(slopos_kit::Event::Char { character });
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }

            fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
                self.drain_menu_actions(event_loop);
                let (next_dark_mode, next_accent) = load_theme_preference();
                if next_dark_mode != self.dark_mode || next_accent != self.accent_color {
                    self.dark_mode = next_dark_mode;
                    self.accent_color = next_accent;
                    self.dirty = true;
                }
                if let Some(ref mut win) = self.window {
                    win.update();
                }
                self.sync_accessibility();
                if self.dirty {
                    if let Some(window) = &self.platform_window {
                        window.request_redraw();
                    }
                }
            }

            fn user_event(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
                // User events are the wake path for menu requests and
                // application-owned background work. The next update may
                // consume state from either queue, so ensure it is painted.
                self.dirty = true;
                self.drain_menu_actions(event_loop);
            }
        }

        let (init_dark_mode, init_accent) = load_theme_preference();
        let control_requests = std::env::var_os("SLOPOS_SESSION_RUNTIME_DIR").and_then(|_| {
            match ApplicationControlListener::bind(&self.bundle_id) {
                Ok(listener) => {
                    let (sender, receiver) = mpsc::channel();
                    let event_proxy = event_loop.proxy();
                    std::thread::spawn(move || loop {
                        match listener.recv_blocking() {
                            Ok(request) => {
                                if sender.send(request).is_err() {
                                    break;
                                }
                                if event_proxy.send_event(()).is_err() {
                                    break;
                                }
                            }
                            Err(error) => {
                                tracing::debug!(%error, "application global-menu listener stopped");
                                break;
                            }
                        }
                    });
                    Some(receiver)
                }
                Err(error) => {
                    tracing::debug!(
                        bundle_id = %self.bundle_id,
                        %error,
                        "application global-menu endpoint unavailable"
                    );
                    None
                }
            }
        });
        let mut handler = AppHandler {
            name: self.name.clone(),
            bundle_id: self.bundle_id.clone(),
            window: main_window,
            initial_size: self.initial_size,
            platform_window: None,
            presenter: None,
            modifiers: winit::keyboard::ModifiersState::default(),
            cursor_position: Point::ZERO,
            last_click: None,
            dirty: true,
            dark_mode: init_dark_mode,
            accent_color: init_accent,
            scale: 1.0,
            control_requests,
            menu_action_handler: self.menu_action_handler.take(),
            accessibility_registration_started: false,
            last_accessibility_registration_attempt: None,
        };
        if let Err(err) = event_loop.run(&mut handler) {
            tracing::error!("application event loop failed: {err}");
        }
    }

    pub fn quit(&mut self) {
        self.running = false;
        tracing::info!("Application '{}' quit", self.name);
    }
}

pub trait AppDelegate {
    fn app_did_finish_launching(&mut self);
    fn app_will_terminate(&mut self);
    fn app_did_resign_active(&mut self);
    fn app_did_become_active(&mut self);
}

pub fn build_menu(title: &str) -> Menu {
    Menu::new(title)
}

fn assign_default_menu_actions(menus: &mut [Menu], bundle_id: &str) {
    for menu in menus {
        let menu_slug = action_slug(&menu.title);
        for item in &mut menu.items {
            if matches!(item.kind, MenuItemKind::Action) && item.action_id.is_empty() {
                item.action_id = format!("{bundle_id}.{}.{}", menu_slug, action_slug(&item.label));
            }
            if let Some(submenu) = &mut item.submenu {
                assign_default_menu_actions(std::slice::from_mut(submenu), bundle_id);
            }
        }
    }
}

fn action_slug(label: &str) -> String {
    let mut slug = String::new();
    let mut last_was_separator = false;
    for ch in label.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            last_was_separator = false;
        } else if !last_was_separator && !slug.is_empty() {
            slug.push('_');
            last_was_separator = true;
        }
    }
    while slug.ends_with('_') {
        slug.pop();
    }
    if slug.is_empty() {
        "action".to_string()
    } else {
        slug
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApplicationMenuAction {
    Hide,
    Quit,
}

fn is_application_menu_action(
    action_id: &str,
    bundle_id: &str,
    app_name: &str,
) -> Option<ApplicationMenuAction> {
    let prefix = format!("{}.", bundle_id);
    let action = action_id.strip_prefix(&prefix)?;
    let app_slug = action_slug(app_name);
    if action == format!("finder.quit_{}", app_slug)
        || action == format!("{}.quit_{}", app_slug, app_slug)
    {
        return Some(ApplicationMenuAction::Quit);
    }
    if action == format!("finder.hide_{}", app_slug)
        || action == format!("{}.hide_{}", app_slug, app_slug)
    {
        return Some(ApplicationMenuAction::Hide);
    }
    None
}

pub fn menu_item(label: &str, action: &str) -> MenuItem {
    let mut item = MenuItem::action(label);
    item.with_action(action);
    item
}

pub fn separator() -> MenuItem {
    MenuItem::separator()
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 4],
}

/// Maximum source tile edge uploaded to the GPU. Tiling keeps a large source
/// from requiring one texture allocation larger than common mobile/VM limits.
const IMAGE_TILE_SIZE: u32 = 2048;
/// Retained image textures are bounded across frames and source images. The
/// Preview decoder itself also enforces a 40 MP source limit.
const IMAGE_CACHE_MAX_BYTES: usize = 256 * 1024 * 1024;

fn align_image_row_bytes(row_bytes: usize) -> usize {
    row_bytes.saturating_add(255) & !255
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct ImageVertex {
    position: [f32; 2],
    uv: [f32; 2],
}

impl ImageVertex {
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ImageVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

impl Vertex {
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

/// Fixed-size, single-channel coverage atlas pages used by the textured glyph
/// pipeline. Pages are backed by one bounded GPU texture array, so adding a
/// page never invalidates vertices emitted earlier in the same frame.
const GLYPH_ATLAS_WIDTH: u32 = 1024;
const GLYPH_ATLAS_HEIGHT: u32 = 1024;
const GLYPH_ATLAS_PADDING: u32 = 1;
const GLYPH_ATLAS_PAGE_COUNT: usize = 4;
/// Per-page entry cap keeps insertion bounded even when tiny synthetic glyphs
/// would otherwise leave most of a page's area unused.
const GLYPH_ATLAS_MAX_ENTRIES: usize = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphAtlasKey {
    raster_hash: u64,
    width: u32,
    height: u32,
    bearing_x_bits: u32,
    bearing_y_bits: u32,
    scale_bits: u32,
}

impl GlyphAtlasKey {
    fn from_raster(glyph: &slopos_render::font::RasterGlyph, scale: f32) -> Option<Self> {
        if glyph.width == 0 || glyph.height == 0 || glyph.data.is_empty() {
            return None;
        }
        let mut hasher = DefaultHasher::new();
        glyph.data.hash(&mut hasher);
        Some(Self {
            raster_hash: hasher.finish(),
            width: glyph.width,
            height: glyph.height,
            bearing_x_bits: glyph.bearing_x.to_bits(),
            bearing_y_bits: glyph.bearing_y.to_bits(),
            scale_bits: scale.to_bits(),
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct GlyphAtlasRegion {
    page: u32,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    u0: f32,
    v0: f32,
    u1: f32,
    v1: f32,
}

#[derive(Debug)]
struct GlyphAtlasPage {
    pixels: Vec<u8>,
    entry_count: usize,
    next_x: u32,
    next_y: u32,
    row_height: u32,
    dirty: bool,
}

impl GlyphAtlasPage {
    fn new() -> Self {
        Self {
            pixels: vec![0; (GLYPH_ATLAS_WIDTH * GLYPH_ATLAS_HEIGHT) as usize],
            entry_count: 0,
            next_x: 0,
            next_y: 0,
            row_height: 0,
            dirty: true,
        }
    }

    fn insert(&mut self, glyph: &slopos_render::font::RasterGlyph) -> Option<(u32, u32)> {
        if self.entry_count >= GLYPH_ATLAS_MAX_ENTRIES
            || glyph.width == 0
            || glyph.height == 0
            || glyph.data.len() < (glyph.width as usize).saturating_mul(glyph.height as usize)
        {
            return None;
        }

        let padded_width = glyph.width.saturating_add(GLYPH_ATLAS_PADDING * 2);
        let padded_height = glyph.height.saturating_add(GLYPH_ATLAS_PADDING * 2);
        if padded_width > GLYPH_ATLAS_WIDTH || padded_height > GLYPH_ATLAS_HEIGHT {
            return None;
        }
        if self.next_x.saturating_add(padded_width) > GLYPH_ATLAS_WIDTH {
            self.next_x = 0;
            self.next_y = self.next_y.saturating_add(self.row_height);
            self.row_height = 0;
        }
        if self.next_y.saturating_add(padded_height) > GLYPH_ATLAS_HEIGHT {
            return None;
        }

        let x = self.next_x + GLYPH_ATLAS_PADDING;
        let y = self.next_y + GLYPH_ATLAS_PADDING;
        for row in 0..glyph.height {
            let source_start = (row * glyph.width) as usize;
            let source_end = source_start + glyph.width as usize;
            let destination_start = ((y + row) * GLYPH_ATLAS_WIDTH + x) as usize;
            let destination_end = destination_start + glyph.width as usize;
            self.pixels[destination_start..destination_end]
                .copy_from_slice(&glyph.data[source_start..source_end]);
        }

        self.next_x = self.next_x.saturating_add(padded_width);
        self.row_height = self.row_height.max(padded_height);
        self.entry_count = self.entry_count.saturating_add(1);
        self.dirty = true;
        Some((x, y))
    }
}

#[derive(Debug)]
struct GlyphAtlas {
    pages: Vec<GlyphAtlasPage>,
    entries: HashMap<GlyphAtlasKey, GlyphAtlasRegion>,
}

impl GlyphAtlas {
    fn new() -> Self {
        Self {
            pages: (0..GLYPH_ATLAS_PAGE_COUNT)
                .map(|_| GlyphAtlasPage::new())
                .collect(),
            entries: HashMap::with_capacity(GLYPH_ATLAS_MAX_ENTRIES),
        }
    }

    fn insert(
        &mut self,
        glyph: &slopos_render::font::RasterGlyph,
        scale: f32,
    ) -> Option<GlyphAtlasRegion> {
        let key = GlyphAtlasKey::from_raster(glyph, scale)?;
        if let Some(region) = self.entries.get(&key).copied() {
            return Some(region);
        }
        for (page_index, page) in self.pages.iter_mut().enumerate() {
            let Some((x, y)) = page.insert(glyph) else {
                continue;
            };
            let region = GlyphAtlasRegion {
                page: page_index as u32,
                x,
                y,
                width: glyph.width,
                height: glyph.height,
                u0: x as f32 / GLYPH_ATLAS_WIDTH as f32,
                v0: y as f32 / GLYPH_ATLAS_HEIGHT as f32,
                u1: (x + glyph.width) as f32 / GLYPH_ATLAS_WIDTH as f32,
                v1: (y + glyph.height) as f32 / GLYPH_ATLAS_HEIGHT as f32,
            };
            self.entries.insert(key, region);
            return Some(region);
        }
        None
    }

    fn take_dirty_pages(&mut self) -> Vec<usize> {
        let mut dirty_pages = Vec::new();
        for (index, page) in self.pages.iter_mut().enumerate() {
            if page.dirty {
                page.dirty = false;
                dirty_pages.push(index);
            }
        }
        dirty_pages
    }

    fn pixels(&self, page: usize) -> &[u8] {
        &self.pages[page].pixels
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ImageTileKey {
    widget_id: WidgetId,
    tile_x: u32,
    tile_y: u32,
    tile_width: u32,
    tile_height: u32,
}

#[derive(Clone)]
struct ImageUpload {
    key: ImageTileKey,
    source_width: u32,
    source_height: u32,
    pixels: Arc<[u8]>,
}

struct CachedImageTexture {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    bytes: usize,
    last_used: u64,
}

#[derive(Clone)]
enum DrawCommand {
    Color {
        start: u32,
        count: u32,
    },
    Glyph {
        start: u32,
        count: u32,
    },
    Image {
        start: u32,
        count: u32,
        upload: ImageUpload,
    },
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct GlyphVertex {
    position: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
    page: u32,
}

impl GlyphVertex {
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GlyphVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: (std::mem::size_of::<[f32; 2]>() * 2) as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: (std::mem::size_of::<[f32; 2]>() * 2 + std::mem::size_of::<[f32; 4]>())
                        as wgpu::BufferAddress,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Uint32,
                },
            ],
        }
    }
}

/// Build a [`slopos_render::DisplayRenderPolicy`] from env and optional settings.conf.
fn display_render_policy_from_env() -> slopos_render::DisplayRenderPolicy {
    let mut hdr_enabled = env_flag_true("SLOPOS_HDR");
    let mut vrr_adaptive = env_flag_true("SLOPOS_VRR");

    if let Some(path) = dirs_settings_conf() {
        if let Ok(content) = fs::read_to_string(path) {
            for line in content.lines() {
                let Some((key, value)) = line.split_once('=') else {
                    continue;
                };
                match key.trim() {
                    "hdr_requested" | "hdr_request" => {
                        hdr_enabled = parse_conf_bool(value, hdr_enabled);
                    }
                    "vrr_adaptive" => {
                        vrr_adaptive = parse_conf_bool(value, vrr_adaptive);
                    }
                    _ => {}
                }
            }
        }
    }

    slopos_render::DisplayRenderPolicy {
        hdr_enabled,
        vrr_adaptive,
    }
}

fn env_flag_true(name: &str) -> bool {
    std::env::var(name)
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

fn parse_conf_bool(value: &str, fallback: bool) -> bool {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => true,
        "false" | "0" | "no" | "off" => false,
        _ => fallback,
    }
}

fn dirs_settings_conf() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".config/slopos-i/settings.conf"))
        .or_else(|| {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .map(|base| base.join("slopos-i/settings.conf"))
        })
}

pub struct WgpuPresenter {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    glyph_pipeline: wgpu::RenderPipeline,
    glyph_atlas: GlyphAtlas,
    glyph_atlas_texture: wgpu::Texture,
    glyph_atlas_view: wgpu::TextureView,
    glyph_atlas_sampler: wgpu::Sampler,
    glyph_bind_group: wgpu::BindGroup,
    image_pipeline: wgpu::RenderPipeline,
    image_bind_group_layout: wgpu::BindGroupLayout,
    image_sampler: wgpu::Sampler,
    image_cache: HashMap<ImageTileKey, CachedImageTexture>,
    image_cache_bytes: usize,
    image_frame: u64,
}

/// Renders immediate-mode UI onto a Wayland surface created outside winit
/// (e.g. a wlr-layer-shell surface owned by slopos-shell).
pub struct RawSurfaceRenderer {
    presenter: WgpuPresenter,
}

impl RawSurfaceRenderer {
    /// Create a renderer from raw Wayland handles for a layer-shell surface.
    ///
    /// # Safety
    ///
    /// `display` must be a valid `*mut wl_display` and `surface` must be a valid
    /// `*mut wl_surface`. Both must outlive the returned renderer. They will not
    /// be freed by this renderer — the caller retains ownership and responsibility
    /// for cleanup.
    pub async unsafe fn new(
        display: *mut std::ffi::c_void,
        surface: *mut std::ffi::c_void,
        width: u32,
        height: u32,
    ) -> Result<Self, String> {
        Ok(Self {
            presenter: WgpuPresenter::new_raw(display, surface, width, height).await?,
        })
    }

    /// Resize the rendering surface.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.presenter.resize(width, height);
    }

    /// Render a frame by calling the draw closure with a mutable Canvas.
    pub fn render(&mut self, draw: impl FnOnce(&mut Canvas<'_>)) -> Result<(), String> {
        self.presenter.render(draw)
    }
}

/// Backend-agnostic UI runtime: owns a Window's widget tree, lays it out,
/// paints it via a RawSurfaceRenderer, and accepts neutral input events.
/// Mirrors the logic of the winit `AppHandler` without any winit dependency,
/// so a wlr-layer-shell driver (slopos-shell) can drive the same UI.
pub struct UiRuntime {
    window: Option<Window>,
    scale: f32,
    dark_mode: bool,
    accent_color: [f32; 4],
    modifiers: Modifiers,
    cursor_position: Point,
    last_click: Option<(MouseButton, Point, u128)>,
    dirty: bool,
    /// Last wall-clock minute painted; used so idle drivers wake the menu clock.
    last_clock_minute: Option<u64>,
}

impl UiRuntime {
    /// Create a new UI runtime with the given widget tree, sized in physical pixels.
    /// The widget is wrapped in a Window and laid out at the logical size (px / scale).
    pub fn new(
        content: Box<dyn slopos_kit::Widget>,
        width_px: u32,
        height_px: u32,
        scale: f32,
    ) -> Self {
        // NOTE: the title MUST be "SLOPOS-I Desktop" — draw_window special-cases
        // that exact title to render chromeless (no titlebar, no content clip), so
        // the menu bar sits at y=0 and the dock reaches the bottom edge.
        let mut window = Window::new("SLOPOS-I Desktop");
        window.set_content(content);

        let mut rt = Self {
            window: Some(window),
            scale: scale.max(1.0),
            dark_mode: false,
            accent_color: theme_accents::CLASSIC,
            modifiers: Modifiers::NONE,
            cursor_position: Point::ZERO,
            last_click: None,
            dirty: true,
            last_clock_minute: None,
        };

        rt.layout_window(width_px, height_px);
        rt
    }

    /// Resize and re-layout the widget tree at the new physical pixel dimensions.
    pub fn resize(&mut self, width_px: u32, height_px: u32, scale: f32) {
        self.scale = scale.max(1.0);
        self.layout_window(width_px, height_px);
    }

    /// Update the dark mode and accent color theme.
    pub fn set_theme(&mut self, dark_mode: bool, accent_color: [f32; 4]) {
        self.dark_mode = dark_mode;
        self.accent_color = accent_color;
        self.dirty = true;
    }

    /// Per-frame tick: reload theme preference and drive the content's
    /// `update()` so dynamic content (dock items, notifications, etc.) is
    /// rebuilt — mirrors the winit `AppHandler::about_to_wait` logic. A driver
    /// must call this each event-loop iteration or the dock never populates.
    /// Also dirties when the wall-clock minute changes so the menu clock advances
    /// even when the driver only wakes on a timer (no pointer/keyboard events).
    pub fn tick(&mut self) {
        let (dark, accent) = load_theme_preference();
        if dark != self.dark_mode || accent != self.accent_color {
            self.dark_mode = dark;
            self.accent_color = accent;
            self.dirty = true;
        }
        if let Some(ref mut win) = self.window {
            win.update();
        }
        let minute = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            / 60;
        if self.last_clock_minute != Some(minute) {
            self.last_clock_minute = Some(minute);
            self.dirty = true;
        }
    }

    /// Update the current modifier key state.
    pub fn set_modifiers(&mut self, modifiers: Modifiers) {
        self.modifiers = modifiers;
    }

    /// Handle pointer movement at logical coordinates.
    pub fn pointer_moved(&mut self, x: f32, y: f32) {
        self.cursor_position = Point::new(x, y);
        let _ = self.dispatch(slopos_kit::Event::MouseMove {
            point: self.cursor_position,
            modifiers: self.modifiers,
        });
    }

    /// Handle pointer button press/release at logical coordinates.
    /// Implements double-click detection: if the same button is pressed within
    /// 400ms and within 4 logical units (distance squared <= 16.0), emits DoubleClick;
    /// otherwise MouseDown on press, MouseUp on release.
    pub fn pointer_button(
        &mut self,
        button: MouseButton,
        pressed: bool,
        time_ms: u128,
    ) -> slopos_kit::EventResult {
        if pressed {
            let is_double_click = self
                .last_click
                .as_ref()
                .map(|(last_button, last_point, last_time)| {
                    *last_button == button
                        && time_ms.saturating_sub(*last_time) <= 400
                        && distance_squared(*last_point, self.cursor_position) <= 16.0
                })
                .unwrap_or(false);
            self.last_click = Some((button, self.cursor_position, time_ms));
            if is_double_click {
                self.dispatch(slopos_kit::Event::DoubleClick {
                    button,
                    point: self.cursor_position,
                    modifiers: self.modifiers,
                })
            } else {
                self.dispatch(slopos_kit::Event::MouseDown {
                    button,
                    point: self.cursor_position,
                    modifiers: self.modifiers,
                })
            }
        } else {
            self.dispatch(slopos_kit::Event::MouseUp {
                button,
                point: self.cursor_position,
                modifiers: self.modifiers,
            })
        }
    }

    /// Handle mouse wheel scroll.
    pub fn wheel(&mut self, delta_x: f32, delta_y: f32) {
        let _ = self.dispatch(slopos_kit::Event::Scroll {
            delta: Point::new(delta_x, delta_y),
            modifiers: self.modifiers,
        });
    }

    /// Handle a keyboard event (caller builds the neutral Event).
    pub fn key(&mut self, event: slopos_kit::Event) {
        let _ = self.dispatch(event);
    }

    /// Set window focus state.
    pub fn set_focus(&mut self, focused: bool) {
        if focused {
            let _ = self.dispatch(slopos_kit::Event::FocusIn);
        } else {
            let _ = self.dispatch(slopos_kit::Event::FocusOut);
        }
    }

    /// Check if a redraw is needed.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Layout (if needed), paint the widget tree and desktop backdrop through the renderer.
    /// Clears the dirty flag on success.
    pub fn paint(&mut self, renderer: &mut RawSurfaceRenderer) -> Result<(), String> {
        self.paint_ex(renderer, true, true)
    }

    /// Paint with optional backdrop and optional re-layout.
    ///
    /// Layer-shell chrome strips call this with `relayout = false` after manually
    /// positioning menu/dock widgets onto strip-sized surfaces.
    pub fn paint_ex(
        &mut self,
        renderer: &mut RawSurfaceRenderer,
        backdrop: bool,
        relayout: bool,
    ) -> Result<(), String> {
        if relayout {
            if let Some(ref mut win) = self.window {
                let size = Size::new(win.rect().width, win.rect().height);
                if size.width > 0.0 && size.height > 0.0 {
                    win.layout(LayoutConstraint::tight(size));
                }
            }
        }
        let Some(window) = &self.window else {
            return Ok(());
        };
        apply_theme(self.dark_mode, self.accent_color);
        let scale = self.scale;
        renderer.render(|canvas| {
            canvas.set_scale(scale);
            if backdrop {
                draw_desktop_backdrop(canvas);
            }
            draw_window(canvas, window);
        })?;
        self.dirty = false;
        Ok(())
    }

    /// Access the root content widget (under the chromeless Window).
    pub fn with_root_content_mut<R>(
        &mut self,
        f: impl FnOnce(&mut dyn slopos_kit::Widget) -> R,
    ) -> Option<R> {
        let win = self.window.as_mut()?;
        let content = win.content.as_mut()?;
        Some(f(content.as_mut()))
    }

    /// Mark the UI dirty so the next driver iteration repaints.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Mark the current pixels/layout as synchronized. Layer-shell drivers use
    /// this after restoring hit-test layout following a multi-surface paint.
    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    /// Dispatch an event to the window and mark as dirty on any result.
    fn dispatch(&mut self, event: slopos_kit::Event) -> slopos_kit::EventResult {
        let result = if let Some(ref mut win) = self.window {
            win.handle_event(&event)
        } else {
            slopos_kit::EventResult::Ignored
        };
        self.dirty = true;
        result
    }

    /// Re-layout the window at the new physical pixel dimensions.
    fn layout_window(&mut self, width_px: u32, height_px: u32) {
        if let Some(ref mut win) = self.window {
            let logical_width = (width_px as f32 / self.scale).max(1.0);
            let logical_height = (height_px as f32 / self.scale).max(1.0);
            let size = Size::new(logical_width, logical_height);
            win.set_rect(Rect::new(0.0, 0.0, size.width, size.height));
            win.layout(LayoutConstraint::tight(size));
            self.dirty = true;
        }
    }
}

impl WgpuPresenter {
    async fn new(window: Arc<winit::window::Window>) -> Result<Self, String> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(Default::default());
        let surface = instance
            .create_surface(window)
            .map_err(|err| format!("surface creation failed: {err}"))?;
        Self::from_surface(instance, surface, size.width, size.height).await
    }

    /// Build a presenter from raw Wayland handles for a layer-shell surface
    /// created outside winit. `display` = `*mut wl_display`, `surface` =
    /// `*mut wl_surface`.
    ///
    /// # Safety
    /// both pointers must reference a valid `wl_display` / `wl_surface`
    /// that outlive the returned presenter.
    pub async unsafe fn new_raw(
        display: *mut std::ffi::c_void,
        surface: *mut std::ffi::c_void,
        width: u32,
        height: u32,
    ) -> Result<Self, String> {
        use raw_window_handle::{
            RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
        };
        let instance = wgpu::Instance::new(Default::default());
        let display_nn =
            std::ptr::NonNull::new(display).ok_or_else(|| "null wl_display".to_string())?;
        let surface_nn =
            std::ptr::NonNull::new(surface).ok_or_else(|| "null wl_surface".to_string())?;
        let raw_display_handle = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(display_nn));
        let raw_window_handle = RawWindowHandle::Wayland(WaylandWindowHandle::new(surface_nn));
        let wgpu_surface = instance
            .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle,
                raw_window_handle,
            })
            .map_err(|err| format!("raw surface creation failed: {err}"))?;
        Self::from_surface(instance, wgpu_surface, width, height).await
    }

    async fn from_surface(
        instance: wgpu::Instance,
        surface: wgpu::Surface<'static>,
        width: u32,
        height: u32,
    ) -> Result<Self, String> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| "no compatible graphics adapter found".to_string())?;
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("RetroSDK Device"),
                    required_features: wgpu::Features::default(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            )
            .await
            .map_err(|err| format!("device creation failed: {err}"))?;

        let caps = surface.get_capabilities(&adapter);
        let policy = display_render_policy_from_env();
        let format = slopos_render::select_surface_format(&caps.formats, policy);
        let present_mode = slopos_render::select_present_mode(&caps.present_modes, policy);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("RetroSDK Immediate UI Shader"),
            source: wgpu::ShaderSource::Wgsl(
                r#"
struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(@location(0) position: vec2<f32>, @location(1) color: vec4<f32>) -> VsOut {
    var out: VsOut;
    out.position = vec4<f32>(position, 0.0, 1.0);
    out.color = color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return in.color;
}
"#
                .into(),
            ),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("RetroSDK Pipeline Layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("RetroSDK Immediate UI Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &[Vertex::layout()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let glyph_atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("RetroSDK R8 Glyph Atlas"),
            size: wgpu::Extent3d {
                width: GLYPH_ATLAS_WIDTH,
                height: GLYPH_ATLAS_HEIGHT,
                depth_or_array_layers: GLYPH_ATLAS_PAGE_COUNT as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let glyph_atlas_view = glyph_atlas_texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            array_layer_count: Some(GLYPH_ATLAS_PAGE_COUNT as u32),
            ..Default::default()
        });
        let glyph_atlas_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("RetroSDK Glyph Atlas Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let glyph_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("RetroSDK Glyph Atlas Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        let glyph_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("RetroSDK Glyph Atlas Bind Group"),
            layout: &glyph_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&glyph_atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&glyph_atlas_sampler),
                },
            ],
        });
        let glyph_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("RetroSDK Textured Glyph Shader"),
            source: wgpu::ShaderSource::Wgsl(
                r#"
struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) page: u32,
};

@vertex
fn vs_main(
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) page: u32,
) -> VsOut {
    var out: VsOut;
    out.position = vec4<f32>(position, 0.0, 1.0);
    out.uv = uv;
    out.color = color;
    out.page = page;
    return out;
}

@group(0) @binding(0) var glyph_atlas: texture_2d_array<f32>;
@group(0) @binding(1) var glyph_sampler: sampler;

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let coverage = textureSample(glyph_atlas, glyph_sampler, in.uv, i32(in.page)).r;
    return vec4<f32>(in.color.rgb, in.color.a * coverage);
}
"#
                .into(),
            ),
        });
        let glyph_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("RetroSDK Glyph Pipeline Layout"),
                bind_group_layouts: &[&glyph_bind_group_layout],
                push_constant_ranges: &[],
            });
        let glyph_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("RetroSDK Textured Glyph Pipeline"),
            layout: Some(&glyph_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &glyph_shader,
                entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &[GlyphVertex::layout()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &glyph_shader,
                entry_point: "fs_main",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let image_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("RetroSDK Image Texture Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        let image_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("RetroSDK Image Texture Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let image_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("RetroSDK Image Texture Shader"),
            source: wgpu::ShaderSource::Wgsl(
                r#"
struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@location(0) position: vec2<f32>, @location(1) uv: vec2<f32>) -> VsOut {
    var out: VsOut;
    out.position = vec4<f32>(position, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@group(0) @binding(0) var image_texture: texture_2d<f32>;
@group(0) @binding(1) var image_sampler: sampler;

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(image_texture, image_sampler, in.uv);
}
"#
                .into(),
            ),
        });
        let image_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("RetroSDK Image Texture Pipeline Layout"),
                bind_group_layouts: &[&image_bind_group_layout],
                push_constant_ranges: &[],
            });
        let image_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("RetroSDK Image Texture Pipeline"),
            layout: Some(&image_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &image_shader,
                entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &[ImageVertex::layout()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &image_shader,
                entry_point: "fs_main",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            glyph_pipeline,
            glyph_atlas: GlyphAtlas::new(),
            glyph_atlas_texture,
            glyph_atlas_view,
            glyph_atlas_sampler,
            glyph_bind_group,
            image_pipeline,
            image_bind_group_layout,
            image_sampler,
            image_cache: HashMap::new(),
            image_cache_bytes: 0,
            image_frame: 0,
        })
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.device, &self.config);
    }

    fn upload_glyph_atlas(&mut self) {
        for page in self.glyph_atlas.take_dirty_pages() {
            self.queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: &self.glyph_atlas_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: page as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                self.glyph_atlas.pixels(page),
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(GLYPH_ATLAS_WIDTH),
                    rows_per_image: Some(GLYPH_ATLAS_HEIGHT),
                },
                wgpu::Extent3d {
                    width: GLYPH_ATLAS_WIDTH,
                    height: GLYPH_ATLAS_HEIGHT,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    fn ensure_image_texture(
        &mut self,
        upload: &ImageUpload,
        protected_keys: &HashSet<ImageTileKey>,
    ) -> Result<(), String> {
        if let Some(cached) = self.image_cache.get_mut(&upload.key) {
            cached.last_used = self.image_frame;
            return Ok(());
        }

        let tile_width = upload.key.tile_width as usize;
        let tile_height = upload.key.tile_height as usize;
        let source_width = upload.source_width as usize;
        let source_height = upload.source_height as usize;
        let tile_x = upload.key.tile_x as usize;
        let tile_y = upload.key.tile_y as usize;
        if tile_width == 0
            || tile_height == 0
            || tile_x.saturating_add(tile_width) > source_width
            || tile_y.saturating_add(tile_height) > source_height
        {
            return Err("invalid image tile bounds".to_string());
        }
        let source_row_bytes = source_width
            .checked_mul(4)
            .ok_or_else(|| "image row size overflow".to_string())?;
        let source_required = source_row_bytes
            .checked_mul(source_height)
            .ok_or_else(|| "image storage size overflow".to_string())?;
        if upload.pixels.len() != source_required {
            return Err("image source storage does not match its dimensions".to_string());
        }
        let row_bytes = tile_width
            .checked_mul(4)
            .ok_or_else(|| "image tile row size overflow".to_string())?;
        let padded_row_bytes = align_image_row_bytes(row_bytes);
        let staging_len = padded_row_bytes
            .checked_mul(tile_height)
            .ok_or_else(|| "image tile storage size overflow".to_string())?;
        let mut staging = vec![0u8; staging_len];
        for row in 0..tile_height {
            let source_start = (tile_y + row)
                .checked_mul(source_row_bytes)
                .and_then(|offset| offset.checked_add(tile_x * 4))
                .ok_or_else(|| "image tile source offset overflow".to_string())?;
            let source_end = source_start + row_bytes;
            let destination_start = row * padded_row_bytes;
            staging[destination_start..destination_start + row_bytes]
                .copy_from_slice(&upload.pixels[source_start..source_end]);
        }

        let bytes = staging_len;
        while self.image_cache_bytes.saturating_add(bytes) > IMAGE_CACHE_MAX_BYTES {
            let Some(oldest_key) = self
                .image_cache
                .iter()
                .filter(|(key, _)| !protected_keys.contains(key))
                .min_by_key(|(_, cached)| cached.last_used)
                .map(|(key, _)| *key)
            else {
                return Err("visible image tiles exceed the retained GPU cache budget".to_string());
            };
            let Some(evicted) = self.image_cache.remove(&oldest_key) else {
                return Err("image cache eviction lost its selected tile".to_string());
            };
            self.image_cache_bytes = self.image_cache_bytes.saturating_sub(evicted.bytes);
        }
        if bytes > IMAGE_CACHE_MAX_BYTES {
            return Err("image tile exceeds the retained GPU cache budget".to_string());
        }

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("RetroSDK Image Tile"),
            size: wgpu::Extent3d {
                width: upload.key.tile_width,
                height: upload.key.tile_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &staging,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(padded_row_bytes as u32),
                rows_per_image: Some(upload.key.tile_height),
            },
            wgpu::Extent3d {
                width: upload.key.tile_width,
                height: upload.key.tile_height,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("RetroSDK Image Tile Bind Group"),
            layout: &self.image_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.image_sampler),
                },
            ],
        });
        self.image_cache.insert(
            upload.key,
            CachedImageTexture {
                texture,
                bind_group,
                bytes,
                last_used: self.image_frame,
            },
        );
        self.image_cache_bytes = self.image_cache_bytes.saturating_add(bytes);
        Ok(())
    }

    fn render(&mut self, draw: impl FnOnce(&mut Canvas<'_>)) -> Result<(), String> {
        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                self.surface
                    .get_current_texture()
                    .map_err(|err| format!("surface acquire failed after reconfigure: {err}"))?
            }
            Err(err) => return Err(format!("surface acquire failed: {err}")),
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut canvas = Canvas::with_glyph_atlas(
            self.config.width as f32,
            self.config.height as f32,
            &mut self.glyph_atlas,
        );
        draw(&mut canvas);
        let draw_data = canvas.finish();
        self.image_frame = self.image_frame.wrapping_add(1);
        self.upload_glyph_atlas();
        let protected_image_keys: HashSet<ImageTileKey> = draw_data
            .commands
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Image { upload, .. } => Some(upload.key),
                _ => None,
            })
            .collect();
        for command in &draw_data.commands {
            if let DrawCommand::Image { upload, .. } = command {
                self.ensure_image_texture(upload, &protected_image_keys)?;
            }
        }

        let vertex_buffer = (!draw_data.vertices.is_empty()).then(|| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("RetroSDK Immediate UI Vertex Buffer"),
                    contents: bytemuck::cast_slice(&draw_data.vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                })
        });
        let glyph_vertex_buffer = (!draw_data.glyph_vertices.is_empty()).then(|| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("RetroSDK Textured Glyph Vertex Buffer"),
                    contents: bytemuck::cast_slice(&draw_data.glyph_vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                })
        });
        let image_vertex_buffer = (!draw_data.image_vertices.is_empty()).then(|| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("RetroSDK Image Tile Vertex Buffer"),
                    contents: bytemuck::cast_slice(&draw_data.image_vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                })
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("RetroSDK Frame Encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("RetroSDK Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            for command in &draw_data.commands {
                match command {
                    DrawCommand::Color { start, count } => {
                        let Some(vertex_buffer) = vertex_buffer.as_ref() else {
                            continue;
                        };
                        pass.set_pipeline(&self.pipeline);
                        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                        pass.draw(*start..start.saturating_add(*count), 0..1);
                    }
                    DrawCommand::Glyph { start, count } => {
                        let Some(glyph_vertex_buffer) = glyph_vertex_buffer.as_ref() else {
                            continue;
                        };
                        pass.set_pipeline(&self.glyph_pipeline);
                        pass.set_bind_group(0, &self.glyph_bind_group, &[]);
                        pass.set_vertex_buffer(0, glyph_vertex_buffer.slice(..));
                        pass.draw(*start..start.saturating_add(*count), 0..1);
                    }
                    DrawCommand::Image {
                        start,
                        count,
                        upload,
                    } => {
                        let Some(image_vertex_buffer) = image_vertex_buffer.as_ref() else {
                            continue;
                        };
                        let Some(cached) = self.image_cache.get(&upload.key) else {
                            continue;
                        };
                        pass.set_pipeline(&self.image_pipeline);
                        pass.set_bind_group(0, &cached.bind_group, &[]);
                        pass.set_vertex_buffer(0, image_vertex_buffer.slice(..));
                        pass.draw(*start..start.saturating_add(*count), 0..1);
                    }
                }
            }
        }
        self.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }
}

enum CanvasAtlas<'a> {
    Borrowed(&'a mut GlyphAtlas),
    Owned(GlyphAtlas),
}

struct CanvasDrawData {
    vertices: Vec<Vertex>,
    glyph_vertices: Vec<GlyphVertex>,
    image_vertices: Vec<ImageVertex>,
    commands: Vec<DrawCommand>,
}

pub struct Canvas<'a> {
    width: f32,
    height: f32,
    /// Number of physical framebuffer pixels per logical UI unit.
    pixel_scale: f32,
    vertices: Vec<Vertex>,
    glyph_vertices: Vec<GlyphVertex>,
    image_vertices: Vec<ImageVertex>,
    commands: Vec<DrawCommand>,
    clip: Option<Rect>,
    atlas: CanvasAtlas<'a>,
}

impl<'a> Canvas<'a> {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            pixel_scale: 1.0,
            vertices: Vec::with_capacity(8192),
            glyph_vertices: Vec::with_capacity(1024),
            image_vertices: Vec::with_capacity(1024),
            commands: Vec::with_capacity(1024),
            clip: None,
            atlas: CanvasAtlas::Owned(GlyphAtlas::new()),
        }
    }

    fn with_glyph_atlas(width: f32, height: f32, atlas: &'a mut GlyphAtlas) -> Self {
        Self {
            width,
            height,
            pixel_scale: 1.0,
            vertices: Vec::with_capacity(8192),
            glyph_vertices: Vec::with_capacity(1024),
            image_vertices: Vec::with_capacity(1024),
            commands: Vec::with_capacity(1024),
            clip: None,
            atlas: CanvasAtlas::Borrowed(atlas),
        }
    }

    fn finish(self) -> CanvasDrawData {
        CanvasDrawData {
            vertices: self.vertices,
            glyph_vertices: self.glyph_vertices,
            image_vertices: self.image_vertices,
            commands: self.commands,
        }
    }

    fn atlas_mut(&mut self) -> &mut GlyphAtlas {
        match &mut self.atlas {
            CanvasAtlas::Borrowed(atlas) => atlas,
            CanvasAtlas::Owned(atlas) => atlas,
        }
    }

    fn push_command(&mut self, command: DrawCommand) {
        let merged = match (self.commands.last_mut(), &command) {
            (
                Some(DrawCommand::Color {
                    start: previous_start,
                    count: previous_count,
                }),
                DrawCommand::Color { start, count },
            ) if previous_start.saturating_add(*previous_count) == *start => {
                *previous_count = previous_count.saturating_add(*count);
                true
            }
            (
                Some(DrawCommand::Glyph {
                    start: previous_start,
                    count: previous_count,
                }),
                DrawCommand::Glyph { start, count },
            ) if previous_start.saturating_add(*previous_count) == *start => {
                *previous_count = previous_count.saturating_add(*count);
                true
            }
            _ => false,
        };
        if !merged {
            self.commands.push(command);
        }
    }

    #[cfg(test)]
    fn glyph_atlas_entry_count(&self) -> usize {
        match &self.atlas {
            CanvasAtlas::Borrowed(atlas) => atlas.len(),
            CanvasAtlas::Owned(atlas) => atlas.len(),
        }
    }

    #[cfg(test)]
    fn glyph_batch_count(&self) -> usize {
        self.commands
            .iter()
            .filter(|command| matches!(command, DrawCommand::Glyph { .. }))
            .count()
    }

    /// Configure logical layout coordinates while preserving the physical
    /// framebuffer scale for text rasterization and pixel snapping.
    pub fn set_scale(&mut self, scale: f32) {
        let scale = scale.max(1.0);
        if (self.pixel_scale - scale).abs() <= f32::EPSILON {
            return;
        }
        self.width /= scale;
        self.height /= scale;
        self.pixel_scale = scale;
    }

    pub fn pixel_scale(&self) -> f32 {
        self.pixel_scale
    }

    pub fn rect(&mut self, rect: Rect, color: [f32; 4]) {
        let mut x0 = rect.x.max(0.0);
        let mut y0 = rect.y.max(0.0);
        let mut x1 = (rect.x + rect.width).min(self.width);
        let mut y1 = (rect.y + rect.height).min(self.height);
        if let Some(clip) = self.clip {
            x0 = x0.max(clip.x);
            y0 = y0.max(clip.y);
            x1 = x1.min(clip.x + clip.width);
            y1 = y1.min(clip.y + clip.height);
        }
        if x0 >= x1 || y0 >= y1 {
            return;
        }

        let p0 = self.ndc(x0, y0);
        let p1 = self.ndc(x1, y0);
        let p2 = self.ndc(x1, y1);
        let p3 = self.ndc(x0, y1);
        self.vertices.extend_from_slice(&[
            Vertex {
                position: p0,
                color,
            },
            Vertex {
                position: p1,
                color,
            },
            Vertex {
                position: p2,
                color,
            },
            Vertex {
                position: p0,
                color,
            },
            Vertex {
                position: p2,
                color,
            },
            Vertex {
                position: p3,
                color,
            },
        ]);
        self.push_command(DrawCommand::Color {
            start: self.vertices.len().saturating_sub(6) as u32,
            count: 6,
        });
    }

    /// Draw a decoded RGBA8 image through retained, bounded GPU tile textures.
    /// Only tiles intersecting the current clip are emitted, so a zoomed or
    /// scrolled image does not upload invisible pixels for the current frame.
    /// Quarter-turn rotation changes only the tile geometry and UVs; decoded
    /// source bytes remain shared with the retained texture cache.
    pub fn image(&mut self, image: &ImageView, rect: Rect) {
        if image.width() == 0 || image.height() == 0 || rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        let canvas_rect = Rect::new(0.0, 0.0, self.width, self.height);
        let clip = self.clip.unwrap_or(canvas_rect);
        let visible = intersect_rect(canvas_rect, clip).and_then(|clip| intersect_rect(clip, rect));
        let Some(visible) = visible else {
            return;
        };

        let source_width = image.width();
        let source_height = image.height();
        let rotation = image.rotation_quadrants() % 4;
        let display_width = if rotation.is_multiple_of(2) {
            source_width
        } else {
            source_height
        };
        let display_height = if rotation.is_multiple_of(2) {
            source_height
        } else {
            source_width
        };
        let scale_x = rect.width / display_width as f32;
        let scale_y = rect.height / display_height as f32;
        if !scale_x.is_finite() || !scale_y.is_finite() || scale_x <= 0.0 || scale_y <= 0.0 {
            return;
        }
        // Map the visible display rectangle back into source coordinates
        // before selecting tiles. This keeps rotated images just as bounded
        // as unrotated ones instead of walking every source tile for a small
        // clipped viewport.
        let display_x_start =
            (((visible.x - rect.x) / scale_x).floor().max(0.0)).min(display_width as f32);
        let display_y_start =
            (((visible.y - rect.y) / scale_y).floor().max(0.0)).min(display_height as f32);
        let display_x_end = (((visible.x + visible.width - rect.x) / scale_x)
            .ceil()
            .max(0.0))
        .min(display_width as f32);
        let display_y_end = (((visible.y + visible.height - rect.y) / scale_y)
            .ceil()
            .max(0.0))
        .min(display_height as f32);
        let (source_x_start, source_x_end, source_y_start, source_y_end) = match rotation {
            0 => (
                display_x_start,
                display_x_end,
                display_y_start,
                display_y_end,
            ),
            1 => (
                display_y_start,
                display_y_end,
                source_height as f32 - display_x_end,
                source_height as f32 - display_x_start,
            ),
            2 => (
                source_width as f32 - display_x_end,
                source_width as f32 - display_x_start,
                source_height as f32 - display_y_end,
                source_height as f32 - display_y_start,
            ),
            _ => (
                source_width as f32 - display_y_end,
                source_width as f32 - display_y_start,
                display_x_start,
                display_x_end,
            ),
        };
        let source_x_start = source_x_start.floor().clamp(0.0, source_width as f32) as u32;
        let source_x_end = source_x_end.ceil().clamp(0.0, source_width as f32) as u32;
        let source_y_start = source_y_start.floor().clamp(0.0, source_height as f32) as u32;
        let source_y_end = source_y_end.ceil().clamp(0.0, source_height as f32) as u32;
        if source_x_start >= source_x_end || source_y_start >= source_y_end {
            return;
        }
        let first_tile_x = source_x_start / IMAGE_TILE_SIZE;
        let first_tile_y = source_y_start / IMAGE_TILE_SIZE;
        let last_tile_x = source_x_end.saturating_sub(1) / IMAGE_TILE_SIZE;
        let last_tile_y = source_y_end.saturating_sub(1) / IMAGE_TILE_SIZE;
        let pixels = image.pixels_arc();
        for tile_y in first_tile_y..=last_tile_y {
            for tile_x in first_tile_x..=last_tile_x {
                let tile_source_x = tile_x * IMAGE_TILE_SIZE;
                let tile_source_y = tile_y * IMAGE_TILE_SIZE;
                let tile_width = IMAGE_TILE_SIZE.min(source_width - tile_source_x);
                let tile_height = IMAGE_TILE_SIZE.min(source_height - tile_source_y);
                let tile_rect = match rotation {
                    0 => Rect::new(
                        rect.x + tile_source_x as f32 * scale_x,
                        rect.y + tile_source_y as f32 * scale_y,
                        tile_width as f32 * scale_x,
                        tile_height as f32 * scale_y,
                    ),
                    1 => Rect::new(
                        rect.x + (source_height - tile_source_y - tile_height) as f32 * scale_x,
                        rect.y + tile_source_x as f32 * scale_y,
                        tile_height as f32 * scale_x,
                        tile_width as f32 * scale_y,
                    ),
                    2 => Rect::new(
                        rect.x + (source_width - tile_source_x - tile_width) as f32 * scale_x,
                        rect.y + (source_height - tile_source_y - tile_height) as f32 * scale_y,
                        tile_width as f32 * scale_x,
                        tile_height as f32 * scale_y,
                    ),
                    _ => Rect::new(
                        rect.x + tile_source_y as f32 * scale_x,
                        rect.y + (source_width - tile_source_x - tile_width) as f32 * scale_y,
                        tile_height as f32 * scale_x,
                        tile_width as f32 * scale_y,
                    ),
                };
                let Some(draw_rect) = intersect_rect(tile_rect, visible) else {
                    continue;
                };
                let tx0 = ((draw_rect.x - tile_rect.x) / tile_rect.width).clamp(0.0, 1.0);
                let ty0 = ((draw_rect.y - tile_rect.y) / tile_rect.height).clamp(0.0, 1.0);
                let tx1 = ((draw_rect.x + draw_rect.width - tile_rect.x) / tile_rect.width)
                    .clamp(0.0, 1.0);
                let ty1 = ((draw_rect.y + draw_rect.height - tile_rect.y) / tile_rect.height)
                    .clamp(0.0, 1.0);
                let uv_corners = match rotation {
                    0 => [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
                    // Clockwise quarter-turn: display top-left is source
                    // bottom-left, then source top-left at display top-right.
                    1 => [[0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
                    2 => [[1.0, 1.0], [0.0, 1.0], [0.0, 0.0], [1.0, 0.0]],
                    // Three clockwise quarter-turns (counter-clockwise
                    // visually): source top-right starts at display
                    // top-left.
                    _ => [[1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.0, 0.0]],
                };
                let uv_at = |tx: f32, ty: f32| {
                    let top = [
                        uv_corners[0][0] + (uv_corners[1][0] - uv_corners[0][0]) * tx,
                        uv_corners[0][1] + (uv_corners[1][1] - uv_corners[0][1]) * tx,
                    ];
                    let bottom = [
                        uv_corners[3][0] + (uv_corners[2][0] - uv_corners[3][0]) * tx,
                        uv_corners[3][1] + (uv_corners[2][1] - uv_corners[3][1]) * tx,
                    ];
                    [
                        top[0] + (bottom[0] - top[0]) * ty,
                        top[1] + (bottom[1] - top[1]) * ty,
                    ]
                };
                let uv0 = uv_at(tx0, ty0);
                let uv1 = uv_at(tx1, ty0);
                let uv2 = uv_at(tx1, ty1);
                let uv3 = uv_at(tx0, ty1);
                let p0 = self.ndc(draw_rect.x, draw_rect.y);
                let p1 = self.ndc(draw_rect.x + draw_rect.width, draw_rect.y);
                let p2 = self.ndc(
                    draw_rect.x + draw_rect.width,
                    draw_rect.y + draw_rect.height,
                );
                let p3 = self.ndc(draw_rect.x, draw_rect.y + draw_rect.height);
                let start = self.image_vertices.len() as u32;
                self.image_vertices.extend_from_slice(&[
                    ImageVertex {
                        position: p0,
                        uv: uv0,
                    },
                    ImageVertex {
                        position: p1,
                        uv: uv1,
                    },
                    ImageVertex {
                        position: p2,
                        uv: uv2,
                    },
                    ImageVertex {
                        position: p0,
                        uv: uv0,
                    },
                    ImageVertex {
                        position: p2,
                        uv: uv2,
                    },
                    ImageVertex {
                        position: p3,
                        uv: uv3,
                    },
                ]);
                self.push_command(DrawCommand::Image {
                    start,
                    count: 6,
                    upload: ImageUpload {
                        key: ImageTileKey {
                            widget_id: image.id(),
                            tile_x: tile_source_x,
                            tile_y: tile_source_y,
                            tile_width,
                            tile_height,
                        },
                        source_width,
                        source_height,
                        pixels: Arc::clone(&pixels),
                    },
                });
            }
        }
    }

    pub fn stroke(&mut self, rect: Rect, color: [f32; 4]) {
        self.rect(Rect::new(rect.x, rect.y, rect.width, 1.0), color);
        self.rect(
            Rect::new(rect.x, rect.y + rect.height - 1.0, rect.width, 1.0),
            color,
        );
        self.rect(Rect::new(rect.x, rect.y, 1.0, rect.height), color);
        self.rect(
            Rect::new(rect.x + rect.width - 1.0, rect.y, 1.0, rect.height),
            color,
        );
    }

    pub fn measure_text(&self, text: &str) -> f32 {
        shape_text(text, TextLayoutOptions::new(13.0, self.pixel_scale)).first_line_width()
    }

    /// Return text that fits the requested logical width, adding a measured
    /// ellipsis when truncation is required.
    pub fn ellipsize_text(&self, text: &str, max_width: f32) -> String {
        render_ellipsize_text(
            text,
            max_width,
            TextLayoutOptions::new(13.0, self.pixel_scale),
        )
    }

    pub fn text(&mut self, text: &str, x: f32, y: f32, color: [f32; 4]) {
        let layout = shape_text(text, TextLayoutOptions::new(13.0, self.pixel_scale));
        self.draw_text_layout(&layout, x, y, color);
    }

    fn glyph(&mut self, ch: char, x: f32, y: f32, color: [f32; 4]) -> f32 {
        let scale = self.pixel_scale.max(1.0);
        let text = ch.to_string();
        let layout = shape_text(&text, TextLayoutOptions::new(13.0, scale));
        let advance = layout.first_line_width();
        self.draw_text_layout(&layout, x, y, color);
        advance
    }

    fn draw_text_layout(&mut self, layout: &TextLayout, x: f32, y: f32, color: [f32; 4]) {
        let scale = layout.scale();
        for glyph in layout.glyphs() {
            self.draw_shaped_glyph(glyph, x, y, scale, color);
        }
    }

    fn draw_shaped_glyph(
        &mut self,
        glyph: &ShapedGlyph,
        x: f32,
        y: f32,
        scale: f32,
        color: [f32; 4],
    ) {
        let origin_x_px = (x + glyph.x) * scale;
        let baseline_y_px = (y + glyph.baseline_y) * scale;
        if let Some(raster) = glyph.raster() {
            if !self.draw_raster_glyph(raster, origin_x_px, baseline_y_px, scale, color) {
                self.draw_raster_glyph_pixels(raster, origin_x_px, baseline_y_px, scale, color);
            }
        } else if let Some(ch) = glyph.fallback_char() {
            self.draw_bitmap_glyph(ch, origin_x_px, baseline_y_px - 9.0 * scale, scale, color);
        }
    }

    #[cfg(test)]
    fn draw_test_raster_glyph(&mut self, glyph: &slopos_render::font::RasterGlyph) {
        if !self.draw_raster_glyph(glyph, 1.0, 1.0, 1.0, [0.0, 0.0, 0.0, 1.0]) {
            self.draw_raster_glyph_pixels(glyph, 1.0, 1.0, 1.0, [0.0, 0.0, 0.0, 1.0]);
        }
    }

    fn draw_raster_glyph(
        &mut self,
        glyph: &slopos_render::font::RasterGlyph,
        origin_x_px: f32,
        baseline_y_px: f32,
        scale: f32,
        color: [f32; 4],
    ) -> bool {
        let region = {
            let atlas = self.atlas_mut();
            atlas.insert(glyph, scale)
        };
        let Some(region) = region else {
            return false;
        };
        let scale = scale.max(0.0001);
        let logical_x = (origin_x_px + glyph.bearing_x).round() / scale;
        let logical_y = (baseline_y_px + glyph.bearing_y).round() / scale;
        let logical_width = glyph.width as f32 / scale;
        let logical_height = glyph.height as f32 / scale;
        let mut x0 = logical_x.max(0.0);
        let mut y0 = logical_y.max(0.0);
        let mut x1 = (logical_x + logical_width).min(self.width);
        let mut y1 = (logical_y + logical_height).min(self.height);
        if let Some(clip) = self.clip {
            x0 = x0.max(clip.x);
            y0 = y0.max(clip.y);
            x1 = x1.min(clip.x + clip.width);
            y1 = y1.min(clip.y + clip.height);
        }
        if x0 >= x1 || y0 >= y1 {
            return true;
        }

        let tx0 = ((x0 - logical_x) / logical_width).clamp(0.0, 1.0);
        let ty0 = ((y0 - logical_y) / logical_height).clamp(0.0, 1.0);
        let tx1 = ((x1 - logical_x) / logical_width).clamp(0.0, 1.0);
        let ty1 = ((y1 - logical_y) / logical_height).clamp(0.0, 1.0);
        let u0 = region.u0 + (region.u1 - region.u0) * tx0;
        let v0 = region.v0 + (region.v1 - region.v0) * ty0;
        let u1 = region.u0 + (region.u1 - region.u0) * tx1;
        let v1 = region.v0 + (region.v1 - region.v0) * ty1;
        let p0 = self.ndc(x0, y0);
        let p1 = self.ndc(x1, y0);
        let p2 = self.ndc(x1, y1);
        let p3 = self.ndc(x0, y1);
        let start = self.glyph_vertices.len() as u32;
        self.glyph_vertices.extend_from_slice(&[
            GlyphVertex {
                position: p0,
                uv: [u0, v0],
                color,
                page: region.page,
            },
            GlyphVertex {
                position: p1,
                uv: [u1, v0],
                color,
                page: region.page,
            },
            GlyphVertex {
                position: p2,
                uv: [u1, v1],
                color,
                page: region.page,
            },
            GlyphVertex {
                position: p0,
                uv: [u0, v0],
                color,
                page: region.page,
            },
            GlyphVertex {
                position: p2,
                uv: [u1, v1],
                color,
                page: region.page,
            },
            GlyphVertex {
                position: p3,
                uv: [u0, v1],
                color,
                page: region.page,
            },
        ]);
        self.push_command(DrawCommand::Glyph { start, count: 6 });
        true
    }

    fn draw_raster_glyph_pixels(
        &mut self,
        glyph: &slopos_render::font::RasterGlyph,
        origin_x_px: f32,
        baseline_y_px: f32,
        scale: f32,
        color: [f32; 4],
    ) {
        let start_x_px = origin_x_px + glyph.bearing_x;
        let start_y_px = baseline_y_px + glyph.bearing_y;
        let logical_pixel = 1.0 / scale;

        for row in 0..glyph.height {
            for col in 0..glyph.width {
                let idx = (row * glyph.width + col) as usize;
                let Some(&coverage) = glyph.data.get(idx) else {
                    continue;
                };
                let alpha = coverage as f32 / 255.0;
                if alpha > 0.05 {
                    let mut c = color;
                    c[3] *= alpha;
                    self.rect(
                        Rect::new(
                            (start_x_px + col as f32).round() / scale,
                            (start_y_px + row as f32).round() / scale,
                            logical_pixel,
                            logical_pixel,
                        ),
                        c,
                    );
                }
            }
        }
    }

    fn draw_bitmap_glyph(&mut self, ch: char, x_px: f32, y_px: f32, scale: f32, color: [f32; 4]) {
        let logical_pixel = 1.0 / scale;
        for (row, bits) in glyph_pattern(ch).iter().enumerate() {
            for col in 0..5 {
                if bits & (1 << (4 - col)) != 0 {
                    self.rect(
                        Rect::new(
                            (x_px + col as f32).round() / scale,
                            (y_px + row as f32).round() / scale,
                            logical_pixel,
                            logical_pixel,
                        ),
                        color,
                    );
                }
            }
        }
    }

    pub fn with_clip(&mut self, clip: Rect, draw: impl FnOnce(&mut Self)) {
        let old = self.clip;
        self.clip = Some(if let Some(old) = old {
            intersect_rect(old, clip).unwrap_or(Rect::ZERO)
        } else {
            clip
        });
        draw(self);
        self.clip = old;
    }

    fn ndc(&self, x: f32, y: f32) -> [f32; 2] {
        [(x / self.width) * 2.0 - 1.0, 1.0 - (y / self.height) * 2.0]
    }
}

fn draw_desktop_backdrop(canvas: &mut Canvas<'_>) {
    let backdrop_base = if render_dark_mode() {
        [0.10, 0.11, 0.11, 1.0]
    } else {
        [0.60, 0.60, 0.58, 1.0]
    };
    canvas.rect(
        Rect::new(0.0, 0.0, canvas.width, canvas.height),
        backdrop_base,
    );
    let width = canvas.width as usize;
    let height = canvas.height as usize;
    for y in (0..height).step_by(4) {
        for x in (0..width).step_by(4) {
            let pattern_x = x / 4;
            let pattern_y = y / 4;
            let shade = match (pattern_x + pattern_y) % 4 {
                0 => {
                    if render_dark_mode() {
                        34
                    } else {
                        168
                    }
                }
                1 => {
                    if render_dark_mode() {
                        24
                    } else {
                        148
                    }
                }
                2 => {
                    if render_dark_mode() {
                        30
                    } else {
                        160
                    }
                }
                _ => {
                    if render_dark_mode() {
                        28
                    } else {
                        152
                    }
                }
            };
            let size = 2.0;
            canvas.rect(
                Rect::new(x as f32, y as f32, size, size),
                rgb(shade, shade, shade),
            );
            if x + 2 < width {
                canvas.rect(
                    Rect::new(x as f32 + 2.0, y as f32, size, size),
                    rgb(shade, shade, shade),
                );
            }
            if y + 2 < height {
                canvas.rect(
                    Rect::new(x as f32, y as f32 + 2.0, size, size),
                    rgb(shade, shade, shade),
                );
            }
            if x + 2 < width && y + 2 < height {
                canvas.rect(
                    Rect::new(x as f32 + 2.0, y as f32 + 2.0, size, size),
                    rgb(shade, shade, shade),
                );
            }
        }
    }

    // Menu bar area under layer chrome is painted by the menu surface; for winit
    // fallback keep a subtle strip matching theme.
    if !render_dark_mode() {
        canvas.rect(
            Rect::new(0.0, 0.0, canvas.width, MENU_BAR_HEIGHT),
            rgb(239, 239, 239),
        );
        canvas.rect(Rect::new(0.0, MENU_BAR_HEIGHT, canvas.width, 1.0), S7_FG);
    } else {
        canvas.rect(
            Rect::new(0.0, 0.0, canvas.width, MENU_BAR_HEIGHT),
            COLOR_DARK_MENU,
        );
        canvas.rect(
            Rect::new(0.0, MENU_BAR_HEIGHT, canvas.width, 1.0),
            COLOR_DARK_EDGE_LIGHT,
        );
    }
}

fn draw_window(canvas: &mut Canvas<'_>, window: &Window) {
    let rect = window.rect();
    if window.title() == "SLOPOS-I Desktop" {
        canvas.rect(rect, rgb(152, 152, 148));
        draw_desktop_backdrop(canvas);
        for child in window.children() {
            draw_widget(canvas, child);
        }
        for child in window.children() {
            draw_menu_overlays(canvas, child);
        }
        return;
    }

    // System7 frame: content fill, then 3D border (black + offset shadow)
    let window_bg = if render_dark_mode() {
        theme_color("window_bg")
    } else {
        S7_BG
    };
    canvas.rect(rect, window_bg);
    draw_system7_3d_border(canvas, rect);

    let titlebar = Rect::new(
        rect.x + 1.0,
        rect.y + 1.0,
        rect.width - 2.0,
        WINDOW_TITLE_BAR_HEIGHT,
    );
    draw_classic_titlebar(canvas, titlebar, window.title(), window.is_active);

    // Content below title bar (inside black border)
    canvas.with_clip(
        Rect::new(
            rect.x + 2.0,
            rect.y + WINDOW_TITLE_BAR_HEIGHT + 2.0,
            (rect.width - 4.0).max(0.0),
            (rect.height - WINDOW_TITLE_BAR_HEIGHT - 4.0).max(0.0),
        ),
        |canvas| {
            for child in window.children() {
                draw_widget(canvas, child);
            }
            for child in window.children() {
                draw_menu_overlays(canvas, child);
            }
        },
    );

    draw_resize_grow_box(canvas, rect);
}

fn draw_window_grip(canvas: &mut Canvas<'_>, x: f32, y: f32, width: f32, height: f32) {
    // System7WindowGrip: 6 horizontal Gray400 lines
    let grip = if render_dark_mode() {
        COLOR_DARK_EDGE_LIGHT
    } else {
        S7_GRAY400
    };
    let line_h = 1.0;
    let gap = 1.0;
    let total = 6.0 * line_h + 5.0 * gap;
    let start_y = y + ((height - total) * 0.5).max(0.0);
    for i in 0..6 {
        let ly = start_y + i as f32 * (line_h + gap);
        canvas.rect(Rect::new(x, ly, width, line_h), grip);
    }
}

fn draw_classic_titlebar(canvas: &mut Canvas<'_>, rect: Rect, title: &str, is_active: bool) {
    let title_w = canvas.measure_text(title);
    if !is_active {
        // Keep inactive chrome on the same semantic surface as the rest of
        // the window while using a dedicated title token for readable state
        // contrast.  The old mid-gray text on a white face was too faint.
        canvas.rect(rect, theme_color("window_bg"));
        canvas.stroke(rect, theme_color("border"));
        let text_color = theme_color("window_title_inactive");
        let title_x = (rect.x + (rect.width - title_w) * 0.5).round();
        canvas.text(title, title_x, rect.y + 6.0, text_color);
        return;
    }

    // Focused: lavender rail behind, gray100 face, grips + boxes
    let rail = if render_dark_mode() {
        [0.22, 0.22, 0.28, 1.0]
    } else {
        S7_LAVENDER100
    };
    let face = if render_dark_mode() {
        COLOR_DARK_BUTTON_BG
    } else {
        S7_GRAY100
    };
    canvas.rect(rect, rail);
    let inner = Rect::new(
        rect.x + 1.0,
        rect.y + 1.0,
        rect.width - 2.0,
        rect.height - 2.0,
    );
    canvas.rect(inner, face);
    canvas.stroke(
        rect,
        if render_dark_mode() {
            COLOR_DARK_BORDER
        } else {
            S7_FG
        },
    );

    // Boxes layout
    let box_size = WINDOW_CONTROL_SIZE;
    let box_y = inner.y + (inner.height - box_size) * 0.5;

    // Left close box
    let mut x = inner.x + 3.0;
    draw_window_grip(canvas, x, inner.y + 2.0, 5.0, inner.height - 4.0);
    x += 7.0;
    let close_box = Rect::new(x, box_y, box_size, box_size);
    draw_beveled_rect(canvas, close_box, face, true);
    canvas.stroke(
        close_box,
        if render_dark_mode() {
            COLOR_DARK_TEXT
        } else {
            S7_FG
        },
    );
    let left_end = close_box.x + close_box.width + 2.0;

    // Right zoom box
    let zoom_x = inner.x + inner.width - 3.0 - 5.0 - box_size - 2.0;
    let zoom_box = Rect::new(zoom_x, box_y, box_size, box_size);
    let right_start = zoom_box.x - 2.0;

    // Centered Title Pill
    let pill_w = (title_w + 16.0).min(inner.width - 70.0).max(20.0);
    let pill_x = (rect.x + (rect.width - pill_w) * 0.5).round();
    let pill_rect = Rect::new(pill_x, inner.y + 2.0, pill_w, inner.height - 4.0);

    // Left & Right Grips (Symmetrical around title pill)
    let left_grip_w = (pill_x - 2.0 - left_end).max(0.0);
    if left_grip_w > 4.0 {
        draw_window_grip(
            canvas,
            left_end,
            inner.y + 2.0,
            left_grip_w,
            inner.height - 4.0,
        );
    }

    let right_grip_x = pill_x + pill_w + 2.0;
    let right_grip_w = (right_start - right_grip_x).max(0.0);
    if right_grip_w > 4.0 {
        draw_window_grip(
            canvas,
            right_grip_x,
            inner.y + 2.0,
            right_grip_w,
            inner.height - 4.0,
        );
    }

    // Title face pill + text
    canvas.rect(pill_rect, face);
    let text_x = (pill_rect.x + (pill_w - title_w) * 0.5).round();
    let text_color = theme_color("text");
    canvas.text(title, text_x, rect.y + 6.0, text_color);

    // Zoom box (right)
    draw_beveled_rect(canvas, zoom_box, face, true);
    canvas.stroke(
        zoom_box,
        if render_dark_mode() {
            COLOR_DARK_TEXT
        } else {
            S7_FG
        },
    );
    draw_window_grip(
        canvas,
        zoom_box.x + zoom_box.width + 2.0,
        inner.y + 2.0,
        5.0,
        inner.height - 4.0,
    );
    // Inner zoom mark
    canvas.rect(
        Rect::new(
            zoom_box.x + 3.0,
            zoom_box.y + 3.0,
            zoom_box.width - 6.0,
            zoom_box.height - 6.0,
        ),
        face,
    );
    canvas.stroke(
        Rect::new(
            zoom_box.x + 3.0,
            zoom_box.y + 3.0,
            zoom_box.width - 6.0,
            zoom_box.height - 6.0,
        ),
        if render_dark_mode() {
            COLOR_DARK_TEXT
        } else {
            S7_FG
        },
    );

    draw_window_grip(
        canvas,
        zoom_box.x + box_size + 2.0,
        inner.y + 2.0,
        5.0,
        inner.height - 4.0,
    );
}

fn draw_resize_grow_box(canvas: &mut Canvas<'_>, window_rect: Rect) {
    let box_rect = Rect::new(
        window_rect.x + window_rect.width - 16.0,
        window_rect.y + window_rect.height - 16.0,
        15.0,
        15.0,
    );
    let box_bg = theme_color("button_bg");
    let box_stroke = if render_dark_mode() {
        [0.58, 0.58, 0.59, 1.0]
    } else {
        [0.52, 0.52, 0.49, 1.0]
    };
    canvas.rect(box_rect, box_bg);
    canvas.stroke(box_rect, box_stroke);

    let glyph_color = if render_dark_mode() {
        [0.71, 0.71, 0.72, 1.0]
    } else {
        [0.32, 0.32, 0.31, 1.0]
    };
    for offset in [4.0, 8.0, 12.0] {
        canvas.rect(
            Rect::new(box_rect.x + offset, box_rect.y + 13.0, 1.0, 1.0),
            glyph_color,
        );
        canvas.rect(
            Rect::new(box_rect.x + 13.0, box_rect.y + offset, 1.0, 1.0),
            glyph_color,
        );
        canvas.rect(
            Rect::new(box_rect.x + offset, box_rect.y + offset, 1.0, 1.0),
            if render_dark_mode() {
                [0.33, 0.33, 0.34, 1.0]
            } else {
                [0.64, 0.64, 0.62, 1.0]
            },
        );
    }
}

fn status_item_advance(canvas: &Canvas<'_>, text: &str, requested_width: f32) -> f32 {
    requested_width.max(canvas.measure_text(text) + 12.0)
}

fn menu_button_advance(canvas: &Canvas<'_>, label: &str) -> f32 {
    canvas.measure_text(label) + 18.0
}

fn draw_widget(canvas: &mut Canvas<'_>, widget: &dyn Widget) {
    let rect = widget.rect();
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }

    if let Some(window) = widget.as_any().downcast_ref::<Window>() {
        draw_window(canvas, window);
        return;
    }

    if let Some(label) = widget.as_any().downcast_ref::<Label>() {
        canvas.text(&label.text, rect.x + 2.0, rect.y + 5.0, theme_color("text"));
    } else if let Some(button) = widget.as_any().downcast_ref::<Button>() {
        if rect.height <= 24.0 {
            canvas.text(
                button.label(),
                rect.x + 8.0,
                rect.y + 7.0,
                theme_color("text"),
            );
            if button.widget_state().focused {
                canvas.stroke(rect, COLOR_FOCUS_RING);
            }
            return;
        }
        // Use theme-aware colors for beveled button
        let bg = if button.widget_state().hovered {
            theme_color("button_hover")
        } else {
            theme_color("button_bg")
        };
        canvas.rect(rect, bg);
        draw_beveled_rect(canvas, rect, bg, true);
        canvas.text(
            button.label(),
            rect.x + 12.0,
            rect.y + 9.0,
            if render_dark_mode() {
                COLOR_DARK_TEXT
            } else {
                COLOR_TEXT_PRIMARY
            },
        );
        if button.widget_state().focused {
            canvas.stroke(rect, COLOR_FOCUS_RING);
        }
    } else if let Some(text_field) = widget.as_any().downcast_ref::<TextField>() {
        // Text field uses theme-aware colors
        let tf_bg = if render_dark_mode() {
            [0.12, 0.12, 0.12, 1.0]
        } else {
            [0.99, 0.99, 0.98, 1.0]
        };
        canvas.rect(rect, tf_bg);
        canvas.stroke(rect, theme_color("border"));
        let text = if text_field.text().is_empty() {
            &text_field.placeholder
        } else {
            text_field.text()
        };
        canvas.text(
            text,
            rect.x + 6.0,
            rect.y + 8.0,
            if render_dark_mode() {
                COLOR_DARK_TEXT
            } else {
                COLOR_TEXT_PRIMARY
            },
        );
    } else if let Some(slider) = widget.as_any().downcast_ref::<Slider>() {
        let track = Rect::new(
            rect.x + 9.0,
            rect.y + rect.height * 0.5 - 3.0,
            rect.width - 18.0,
            6.0,
        );
        let track_bg = if render_dark_mode() {
            [0.12, 0.12, 0.13, 1.0]
        } else {
            [0.77, 0.77, 0.75, 1.0]
        };
        canvas.rect(track, track_bg);
        canvas.stroke(track, theme_color("border"));
        let filled = Rect::new(
            track.x + 1.0,
            track.y + 1.0,
            (track.width - 2.0) * slider.normalized_value(),
            track.height - 2.0,
        );
        canvas.rect(filled, render_accent());
        let thumb_x = track.x + track.width * slider.normalized_value() - 5.0;
        let thumb = Rect::new(thumb_x, rect.y + 3.0, 10.0, rect.height - 6.0);
        let thumb_bg = if slider.dragging {
            theme_color("button_hover")
        } else {
            theme_color("button_bg")
        };
        canvas.rect(thumb, thumb_bg);
        draw_beveled_rect(canvas, thumb, thumb_bg, true);
    } else if let Some(tree) = widget.as_any().downcast_ref::<TreeView>() {
        draw_tree(canvas, rect, tree);
    } else if let Some(icon_view) = widget.as_any().downcast_ref::<IconView>() {
        draw_icon_view(canvas, icon_view);
    } else if let Some(list) = widget.as_any().downcast_ref::<ListView>() {
        draw_list(canvas, rect, list);
    } else if let Some(image) = widget.as_any().downcast_ref::<ImageView>() {
        canvas.image(image, rect);
        return;
    } else if let Some(menu_bar) = widget.as_any().downcast_ref::<MenuBar>() {
        draw_menu_bar_widget(canvas, rect, menu_bar);
        return;
    } else if let Some(toolbar) = widget.as_any().downcast_ref::<Toolbar>() {
        if rect.y <= 1.0 && rect.width > 500.0 {
            draw_menu_bar(canvas, rect, toolbar);
        } else {
            let toolbar_bg = if render_dark_mode() {
                [0.16, 0.16, 0.18, 1.0]
            } else {
                [0.85, 0.85, 0.84, 1.0]
            };
            canvas.rect(rect, toolbar_bg);
            canvas.rect(
                Rect::new(rect.x, rect.y + rect.height - 1.0, rect.width, 1.0),
                theme_color("border"),
            );
            for child in toolbar.children() {
                draw_widget(canvas, child);
            }
        }
        return;
    } else if let Some(scroll) = widget.as_any().downcast_ref::<ScrollView>() {
        let scroll_bg = if render_dark_mode() {
            [0.11, 0.11, 0.12, 1.0]
        } else {
            [0.97, 0.97, 0.96, 1.0]
        };
        canvas.rect(rect, scroll_bg);
        canvas.stroke(rect, theme_color("border"));
        canvas.with_clip(rect, |canvas| {
            for child in scroll.children() {
                draw_widget(canvas, child);
            }
        });
        return;
    } else if widget.as_any().is::<SplitView>() {
        let split_bg = if render_dark_mode() {
            [0.13, 0.14, 0.14, 1.0]
        } else {
            [0.90, 0.90, 0.88, 1.0]
        };
        canvas.rect(rect, split_bg);
        if let Some(split) = widget.as_any().downcast_ref::<SplitView>() {
            let divider = match split.direction {
                slopos_kit::split_view::SplitDirection::Horizontal => Rect::new(
                    rect.x + rect.width * split.divider_position,
                    rect.y,
                    split.divider_size,
                    rect.height,
                ),
                slopos_kit::split_view::SplitDirection::Vertical => Rect::new(
                    rect.x,
                    rect.y + rect.height * split.divider_position,
                    rect.width,
                    split.divider_size,
                ),
            };
            let divider_bg = if render_dark_mode() {
                [0.27, 0.28, 0.29, 1.0]
            } else {
                [0.71, 0.71, 0.69, 1.0]
            };
            canvas.rect(divider, divider_bg);
            canvas.stroke(divider, theme_color("border"));
        }
    } else if let Some(grid) = widget.as_any().downcast_ref::<MonospaceView>() {
        draw_monospace_view(canvas, rect, grid);
        return;
    } else if let Some(status) = widget.as_any().downcast_ref::<StatusBar>() {
        let status_bg = if render_dark_mode() {
            [0.14, 0.15, 0.15, 1.0]
        } else {
            [0.86, 0.86, 0.85, 1.0]
        };
        canvas.rect(rect, status_bg);
        canvas.rect(
            Rect::new(rect.x, rect.y, rect.width, 1.0),
            theme_color("border"),
        );
        let mut x = rect.x + 8.0;
        for item in &status.items {
            canvas.text(&item.text, x, rect.y + 8.0, theme_color("text"));
            x += status_item_advance(canvas, &item.text, item.width);
        }
    } else if let Some(dialog) = widget.as_any().downcast_ref::<Dialog>() {
        draw_dialog(canvas, rect, dialog);
        return;
    } else if let Some(pb) = widget.as_any().downcast_ref::<PopupButton>() {
        draw_popup_button(canvas, rect, pb);
        return;
    } else if let Some(pb) = widget.as_any().downcast_ref::<ProgressBar>() {
        draw_progress_bar(canvas, rect, pb);
        return;
    } else if let Some(tv) = widget.as_any().downcast_ref::<TabView>() {
        draw_tab_view(canvas, rect, tv);
        return;
    } else if let Some(dock) = widget.as_any().downcast_ref::<DockView>() {
        draw_dock_view(canvas, rect, dock);
        return;
    } else if let Some(grid) = widget.as_any().downcast_ref::<WorkspaceGridView>() {
        draw_workspace_grid_view(canvas, rect, grid);
        return;
    } else if let Some(layout_view) = widget.as_any().downcast_ref::<LayoutView>() {
        draw_layout(canvas, &layout_view.layout);
        return;
    } else if let Some(panel) = widget.as_any().downcast_ref::<Panel>() {
        let fill = if panel.themed {
            theme_color("window_bg")
        } else {
            panel.fill
        };
        canvas.rect(rect, fill);
        if panel.beveled {
            draw_beveled_rect(canvas, rect, fill, panel.raised);
        } else if panel.bordered {
            canvas.stroke(rect, theme_color("border"));
        }
        return;
    }

    for child in widget.children() {
        draw_widget(canvas, child);
    }
    for child in widget.children() {
        if let Some(menu_bar) = child.as_any().downcast_ref::<MenuBar>() {
            if menu_bar.open_menu.is_some() {
                draw_menu_bar_widget(canvas, menu_bar.rect(), menu_bar);
            }
        }
    }
}

fn draw_dialog(canvas: &mut Canvas<'_>, rect: Rect, dialog: &Dialog) {
    // Background and outer border - use theme colors
    let bg = theme_color("window_bg");
    canvas.rect(rect, bg);
    canvas.stroke(rect, theme_color("border"));

    // Title bar area
    let titlebar_rect = Rect::new(rect.x, rect.y, rect.width, 32.0);
    let titlebar_bg = if render_dark_mode() {
        [0.21, 0.22, 0.23, 1.0]
    } else {
        [0.85, 0.85, 0.84, 1.0]
    };
    canvas.rect(titlebar_rect, titlebar_bg);

    // Title bar highlight (raised bevel top edge)
    canvas.rect(
        Rect::new(rect.x + 1.0, rect.y + 1.0, rect.width - 2.0, 1.0),
        theme_color("edge_light"),
    );

    // Title text centered in title bar
    let title = &dialog.title;
    let title_w = canvas.measure_text(title);
    let title_x = (rect.x + (rect.width - title_w) * 0.5).round();
    canvas.text(title, title_x, rect.y + 10.0, theme_color("text"));

    // Horizontal separator below title
    canvas.rect(
        Rect::new(rect.x, rect.y + 32.0, rect.width, 1.0),
        if render_dark_mode() {
            [0.31, 0.32, 0.33, 1.0]
        } else {
            [0.57, 0.57, 0.55, 1.0]
        },
    );

    // Message text
    canvas.text(
        &dialog.message,
        rect.x + 12.0,
        rect.y + 42.0,
        theme_color("text"),
    );

    // Draw buttons right-aligned at the bottom
    let btn_h = 24.0;
    let btn_y = rect.y + rect.height - btn_h - 10.0;
    let mut btn_x = rect.x + rect.width - 10.0;
    for btn in dialog.buttons.iter().rev() {
        let label = btn.label();
        let label_w = canvas.measure_text(label);
        let btn_w = (label_w + 20.0).max(72.0);
        btn_x -= btn_w;
        let btn_rect = Rect::new(btn_x, btn_y, btn_w, btn_h);
        let btn_bg = theme_color("button_bg");
        canvas.rect(btn_rect, btn_bg);
        draw_beveled_rect(canvas, btn_rect, btn_bg, true);
        let text_x = (btn_rect.x + (btn_w - label_w) * 0.5).round();
        canvas.text(label, text_x, btn_rect.y + 6.0, theme_color("text"));
        btn_x -= 8.0;
    }
}

fn draw_popup_button(canvas: &mut Canvas<'_>, rect: Rect, pb: &PopupButton) {
    // Background with beveled raised look
    let bg = theme_color("button_bg");
    canvas.rect(rect, bg);
    draw_beveled_rect(canvas, rect, bg, true);

    // Selected title text, left-aligned with some padding
    let label = pb.selected_title().unwrap_or("");
    canvas.text(
        label,
        rect.x + 8.0,
        rect.y + (rect.height - 12.0) * 0.5,
        theme_color("text"),
    );

    // Down-arrow indicator on the right side
    // Draw a small triangle using three thin horizontal rects
    let arrow_x = rect.x + rect.width - 14.0;
    let arrow_y = rect.y + rect.height * 0.5 - 2.0;
    let arrow_color = if render_dark_mode() {
        [0.71, 0.71, 0.69, 1.0]
    } else {
        [0.24, 0.24, 0.23, 1.0]
    };
    canvas.rect(Rect::new(arrow_x, arrow_y, 7.0, 1.0), arrow_color);
    canvas.rect(
        Rect::new(arrow_x + 1.0, arrow_y + 1.0, 5.0, 1.0),
        arrow_color,
    );
    canvas.rect(
        Rect::new(arrow_x + 2.0, arrow_y + 2.0, 3.0, 1.0),
        arrow_color,
    );
    canvas.rect(
        Rect::new(arrow_x + 3.0, arrow_y + 3.0, 1.0, 1.0),
        arrow_color,
    );

    // Separator line between label area and arrow area
    canvas.rect(
        Rect::new(
            rect.x + rect.width - 18.0,
            rect.y + 2.0,
            1.0,
            rect.height - 4.0,
        ),
        theme_color("border"),
    );

    // Shadow line at bottom-right for depth
    let shadow_color = theme_color("edge_dark");
    canvas.rect(
        Rect::new(
            rect.x + 1.0,
            rect.y + rect.height - 1.0,
            rect.width - 1.0,
            1.0,
        ),
        shadow_color,
    );
    canvas.rect(
        Rect::new(
            rect.x + rect.width - 1.0,
            rect.y + 1.0,
            1.0,
            rect.height - 1.0,
        ),
        shadow_color,
    );
}

fn draw_progress_bar(canvas: &mut Canvas<'_>, rect: Rect, pb: &ProgressBar) {
    let pb_bg = if render_dark_mode() {
        [0.09, 0.10, 0.11, 1.0]
    } else {
        [0.93, 0.93, 0.91, 1.0]
    };
    canvas.rect(rect, pb_bg);
    canvas.stroke(rect, theme_color("border"));
    let ratio = if pb.max > 0.0 { pb.value / pb.max } else { 0.0 };
    let fill_width = (rect.width - 4.0) * ratio.clamp(0.0, 1.0);
    if fill_width > 0.0 {
        let fill = Rect::new(rect.x + 2.0, rect.y + 2.0, fill_width, rect.height - 4.0);
        let accent = render_accent();
        canvas.rect(fill, accent);
    }
}

fn draw_workspace_grid_view(canvas: &mut Canvas<'_>, _rect: Rect, grid: &WorkspaceGridView) {
    // Cell geometry comes from the widget — the same rects its
    // `handle_event` hit-tests, so paint and input cannot drift.
    for i in 0..grid.items.len() {
        let cell_r = grid.cell_rect(i);
        let cell_w = cell_r.width;
        let cell_h = cell_r.height;
        let cell_x = cell_r.x;
        let cell_y = cell_r.y;

        let bg_color = if i == grid.active_index {
            if render_dark_mode() {
                [0.19, 0.27, 0.38, 1.0]
            } else {
                [0.80, 0.87, 0.94, 1.0]
            }
        } else if render_dark_mode() {
            [0.15, 0.15, 0.16, 1.0]
        } else {
            [0.94, 0.94, 0.92, 1.0]
        };
        canvas.rect(cell_r, bg_color);

        let border_color = if i == grid.active_index {
            if render_dark_mode() {
                [0.55, 0.71, 0.94, 1.0]
            } else {
                [0.04, 0.31, 0.63, 1.0]
            }
        } else {
            theme_color("border")
        };

        canvas.stroke(cell_r, border_color);
        if i == grid.active_index {
            canvas.stroke(
                Rect::new(
                    cell_r.x + 1.0,
                    cell_r.y + 1.0,
                    cell_r.width - 2.0,
                    cell_r.height - 2.0,
                ),
                border_color,
            );
        }
        if grid.widget_state().focused && i == grid.focused_index {
            let inset = 3.0;
            let focus_rect = Rect::new(
                cell_r.x + inset,
                cell_r.y + inset,
                (cell_r.width - inset * 2.0).max(0.0),
                (cell_r.height - inset * 2.0).max(0.0),
            );
            canvas.stroke(focus_rect, render_accent());
        }
        if grid.drag_target() == Some(i) {
            let inset = 2.0;
            let drop_rect = Rect::new(
                cell_r.x + inset,
                cell_r.y + inset,
                (cell_r.width - inset * 2.0).max(0.0),
                (cell_r.height - inset * 2.0).max(0.0),
            );
            canvas.stroke(drop_rect, render_accent());
        }

        if let Some(thumbnail) = grid.thumbnail(i) {
            let thumbnail_rect = grid.thumbnail_rect(i);
            if thumbnail_rect.width > 0.0 && thumbnail_rect.height > 0.0 {
                canvas.image(thumbnail, thumbnail_rect);
            }
        }

        let base_label = grid.items.get(i).map(String::as_str).unwrap_or("");
        let label = match grid.window_counts.get(i).copied() {
            Some(count) => {
                let noun = if count == 1 { "window" } else { "windows" };
                format!("{base_label} ({count} {noun})")
            }
            None => base_label.to_string(),
        };
        let label = canvas.ellipsize_text(&label, (cell_w - 12.0).max(0.0));
        let text_color = if i == grid.active_index {
            if render_dark_mode() {
                [0.90, 0.94, 1.0, 1.0]
            } else {
                [0.04, 0.16, 0.31, 1.0]
            }
        } else {
            theme_color("text")
        };
        let text_width = canvas.measure_text(&label);
        let text_y = if grid.thumbnail(i).is_some() {
            cell_y + cell_h - 14.0
        } else {
            cell_y + (cell_h - 12.0) * 0.5 + 2.0
        };
        canvas.text(
            &label,
            cell_x + (cell_w - text_width) * 0.5,
            text_y,
            text_color,
        );
    }
}

fn draw_tab_view(canvas: &mut Canvas<'_>, rect: Rect, tv: &TabView) {
    let header_height = 30.0;
    let divider_y = rect.y + header_height - 1.0;
    canvas.rect(
        Rect::new(rect.x, divider_y, rect.width, 1.0),
        theme_color("border"),
    );
    let mut current_x = rect.x + 8.0;
    for (i, tab) in tv.tabs.iter().enumerate() {
        let title_w = canvas.measure_text(&tab.title);
        let tab_width = title_w + 24.0;
        let tab_rect = Rect::new(current_x, rect.y + 4.0, tab_width, 25.0);
        let is_selected = tv.selected_tab_index == i;
        if is_selected {
            let tab_bg = theme_color("button_bg");
            canvas.rect(tab_rect, tab_bg);
            draw_beveled_rect(canvas, tab_rect, tab_bg, true);
            canvas.rect(
                Rect::new(tab_rect.x + 1.0, divider_y, tab_rect.width - 2.0, 1.0),
                tab_bg,
            );
            // Accent underline on selected tab
            canvas.rect(
                Rect::new(tab_rect.x + 2.0, tab_rect.y, tab_rect.width - 4.0, 2.0),
                render_accent(),
            );
        } else {
            let inactive_bg = if render_dark_mode() {
                [0.12, 0.12, 0.13, 1.0]
            } else {
                [0.82, 0.82, 0.80, 1.0]
            };
            canvas.rect(tab_rect, inactive_bg);
            draw_beveled_rect(canvas, tab_rect, inactive_bg, false);
        }
        let text_color = if is_selected {
            theme_color("text")
        } else if render_dark_mode() {
            [0.55, 0.55, 0.53, 1.0]
        } else {
            [0.39, 0.39, 0.37, 1.0]
        };
        let text_x = (tab_rect.x + (tab_width - title_w) * 0.5).round();
        canvas.text(&tab.title, text_x, tab_rect.y + 7.0, text_color);
        current_x += tab_width + 4.0;
    }
    if let Some(content) = tv.selected_content() {
        draw_widget(canvas, content);
    }
}

fn draw_dock_view(canvas: &mut Canvas<'_>, _rect: Rect, dock: &DockView) {
    if dock.items.is_empty() {
        return;
    }

    // Geometry comes from the widget itself — the same rects its
    // `handle_event` hit-tests, so paint and input cannot drift.
    let dock_rect = dock.strip_rect();

    let bg_color = theme_face();
    canvas.rect(dock_rect, bg_color);
    draw_beveled_rect(canvas, dock_rect, bg_color, true);
    draw_system7_3d_border(canvas, dock_rect);

    for (i, item) in dock.items.iter().enumerate() {
        let item_rect = dock.item_rect(i);

        if item.is_focused {
            let highlight_rect = Rect::new(
                item_rect.x - 2.0,
                item_rect.y - 2.0,
                item_rect.width + 4.0,
                item_rect.height + 4.0,
            );
            let focus_color = if render_dark_mode() {
                S7_LAVENDER300
            } else {
                S7_LAVENDER100
            };
            canvas.rect(highlight_rect, focus_color);
            draw_beveled_rect(canvas, highlight_rect, focus_color, false);
        }

        let icon_bg = theme_paper();
        canvas.rect(item_rect, icon_bg);
        draw_beveled_rect(canvas, item_rect, icon_bg, true);

        let symbol_x = item_rect.x + (item_rect.width - 32.0) * 0.5;
        let symbol_y = item_rect.y + (item_rect.height - 32.0) * 0.5;
        draw_labeled_icon(canvas, item.label.as_str(), symbol_x, symbol_y);

        if item.is_running {
            canvas.rect(
                Rect::new(
                    item_rect.x + item_rect.width * 0.5 - 2.0,
                    item_rect.y + item_rect.height - 5.0,
                    4.0,
                    4.0,
                ),
                theme_ink(),
            );
        }
    }
}

fn draw_layout(canvas: &mut Canvas<'_>, layout: &Layout) {
    match layout {
        Layout::Horizontal { children, .. }
        | Layout::Vertical { children, .. }
        | Layout::Grid { children, .. }
        | Layout::Stack { children }
        | Layout::Overlay { children } => {
            for child in children {
                draw_widget(canvas, child.as_ref());
            }
            for child in children {
                if child
                    .as_any()
                    .downcast_ref::<MenuBar>()
                    .is_some_and(|menu_bar| menu_bar.open_menu.is_some())
                {
                    draw_widget(canvas, child.as_ref());
                }
                draw_menu_overlays(canvas, child.as_ref());
            }
        }
    }
}

fn draw_menu_overlays(canvas: &mut Canvas<'_>, widget: &dyn Widget) {
    if let Some(menu_bar) = widget.as_any().downcast_ref::<MenuBar>() {
        if menu_bar.open_menu.is_some() {
            draw_menu_bar_widget(canvas, menu_bar.rect(), menu_bar);
        }
    }
    for child in widget.children() {
        draw_menu_overlays(canvas, child);
    }
}

fn draw_menu_bar(canvas: &mut Canvas<'_>, rect: Rect, toolbar: &Toolbar) {
    let menu_bar_bg = if render_dark_mode() {
        [0.11, 0.11, 0.12, 1.0]
    } else {
        [0.93, 0.93, 0.93, 1.0]
    };
    canvas.rect(rect, menu_bar_bg);
    canvas.rect(
        Rect::new(rect.x, rect.y, rect.width, 1.0),
        theme_color("edge_light"),
    );
    canvas.rect(
        Rect::new(rect.x, rect.y + rect.height - 2.0, rect.width, 1.0),
        if render_dark_mode() {
            [0.4, 0.4, 0.4, 1.0]
        } else {
            [0.37, 0.37, 0.37, 1.0]
        },
    );
    canvas.rect(
        Rect::new(rect.x, rect.y + rect.height - 1.0, rect.width, 1.0),
        theme_color("edge_dark"),
    );

    let mut x = rect.x + 10.0;
    draw_slopos_menu_logo(canvas, x + 1.0, rect.y + 6.0, false);
    x += 18.0;

    for child in toolbar.children() {
        if let Some(button) = child.as_any().downcast_ref::<Button>() {
            let label = button.label();
            canvas.text(label, x, rect.y + 8.0, theme_color("text"));
            x += menu_button_advance(canvas, label);
        }
    }

    let right_label = menu_status_label();
    let right_w = canvas.measure_text(&right_label);
    canvas.text(
        &right_label,
        rect.x + rect.width - right_w - 8.0,
        rect.y + 8.0,
        theme_color("text"),
    );
}

fn draw_menu_bar_widget(canvas: &mut Canvas<'_>, rect: Rect, menu_bar: &MenuBar) {
    if menu_bar.layer_popup_origin {
        if let Some(menu_index) = menu_bar.open_menu {
            draw_open_menu_at_origin(canvas, menu_bar, menu_index);
        }
        return;
    }

    // System 7 menu bar: graphite/platinum face + ink bottom rule
    let menu_bar_bg = theme_menu();
    canvas.rect(rect, menu_bar_bg);
    canvas.rect(
        Rect::new(rect.x, rect.y + rect.height - 1.0, rect.width, 1.0),
        theme_ink(),
    );

    for (index, menu) in menu_bar.menus.iter().enumerate() {
        let Some(menu_rect) = menu_bar.menu_rects().get(index).copied() else {
            continue;
        };
        let active = menu_bar.open_menu == Some(index) || menu_bar.hovered_menu == Some(index);
        if active {
            // Classic inverted selection — System 7 style
            let highlight_color = if render_dark_mode() {
                S7_LAVENDER300
            } else {
                S7_FG
            };
            canvas.rect(
                Rect::new(
                    menu_rect.x + 1.0,
                    menu_rect.y + 1.0,
                    menu_rect.width - 2.0,
                    (menu_rect.height - 2.0).max(1.0),
                ),
                highlight_color,
            );
        }
        if index == 0 {
            draw_slopos_menu_logo(canvas, menu_rect.x + 4.0, menu_rect.y + 3.0, active);
            canvas.text(
                &menu.title,
                menu_rect.x + 18.0,
                menu_rect.y + 5.0,
                if active {
                    [1.0, 1.0, 1.0, 1.0]
                } else {
                    theme_ink()
                },
            );
        } else {
            canvas.text(
                &menu.title,
                menu_rect.x + 8.0,
                menu_rect.y + 5.0,
                if active {
                    [1.0, 1.0, 1.0, 1.0]
                } else {
                    theme_ink()
                },
            );
        }
    }

    let right_label = menu_status_label();
    let right_w = canvas.measure_text(&right_label);
    canvas.text(
        &right_label,
        rect.x + rect.width - right_w - 8.0,
        rect.y + 5.0,
        theme_ink(),
    );

    if let Some(menu_index) = menu_bar.open_menu {
        if !menu_bar.suppress_dropdown_paint {
            draw_open_menu(canvas, menu_bar, menu_index);
        }
    }
}

fn draw_slopos_menu_logo(canvas: &mut Canvas<'_>, x: f32, y: f32, active: bool) {
    let main_color = if active {
        [1.0, 1.0, 1.0, 1.0]
    } else {
        theme_ink()
    };
    let accent_color = if active {
        S7_LAVENDER300
    } else if render_dark_mode() {
        [0.4, 0.4, 0.6, 1.0]
    } else {
        S7_LAVENDER100
    };

    // Retro monitor bezel (10x8)
    canvas.rect(Rect::new(x, y, 10.0, 8.0), main_color);
    // Monitor screen interior (6x5)
    canvas.rect(Rect::new(x + 2.0, y + 1.5, 6.0, 5.0), accent_color);
    // 'S' logo symbol inside screen
    let s_color = if active {
        [0.0, 0.0, 0.0, 1.0]
    } else {
        theme_paper()
    };
    canvas.rect(Rect::new(x + 3.0, y + 2.0, 4.0, 1.0), s_color);
    canvas.rect(Rect::new(x + 3.0, y + 3.0, 2.0, 1.0), s_color);
    canvas.rect(Rect::new(x + 3.0, y + 4.0, 4.0, 1.0), s_color);
    canvas.rect(Rect::new(x + 5.0, y + 5.0, 2.0, 1.0), s_color);

    // Keyboard base (12x2)
    canvas.rect(Rect::new(x - 1.0, y + 9.0, 12.0, 2.0), main_color);
}

fn draw_open_menu(canvas: &mut Canvas<'_>, menu_bar: &MenuBar, menu_index: usize) {
    let Some(dropdown) = menu_bar.dropdown_rect(menu_index) else {
        return;
    };
    draw_open_menu_box(canvas, menu_bar, menu_index, dropdown, 0.0, 0.0);
}

/// Draw the open dropdown with its top-left at (0,0) for an Overlay layer surface.
fn draw_open_menu_at_origin(canvas: &mut Canvas<'_>, menu_bar: &MenuBar, menu_index: usize) {
    let Some(dropdown) = menu_bar.dropdown_rect(menu_index) else {
        return;
    };
    draw_open_menu_box(
        canvas,
        menu_bar,
        menu_index,
        Rect::new(0.0, 0.0, dropdown.width, dropdown.height),
        -dropdown.x,
        -dropdown.y,
    );
}

fn draw_open_menu_box(
    canvas: &mut Canvas<'_>,
    menu_bar: &MenuBar,
    menu_index: usize,
    dropdown: Rect,
    item_dx: f32,
    item_dy: f32,
) {
    let Some(menu) = menu_bar.menus.get(menu_index) else {
        return;
    };

    canvas.rect(
        Rect::new(
            dropdown.x + MENU_SHADOW_OFFSET,
            dropdown.y + MENU_SHADOW_OFFSET,
            dropdown.width,
            dropdown.height,
        ),
        S7_FG,
    );
    let menu_bg = if render_dark_mode() {
        [0.16, 0.17, 0.18, 1.0]
    } else {
        [0.96, 0.96, 0.93, 1.0]
    };
    draw_beveled_rect(canvas, dropdown, menu_bg, true);
    canvas.rect(
        Rect::new(
            dropdown.x + 4.0,
            dropdown.y + 4.0,
            dropdown.width - 8.0,
            1.0,
        ),
        theme_color("edge_light"),
    );
    canvas.rect(
        Rect::new(
            dropdown.x + 4.0,
            dropdown.y + 4.0,
            1.0,
            dropdown.height - 8.0,
        ),
        theme_color("edge_light"),
    );

    for (item_index, item) in menu.items.iter().enumerate() {
        let Some(mut item_rect) = menu_bar.item_rect(menu_index, item_index) else {
            continue;
        };
        item_rect.x += item_dx;
        item_rect.y += item_dy;
        if matches!(item.kind, MenuItemKind::Separator) {
            let sep_dark = if render_dark_mode() {
                [0.45, 0.46, 0.47, 1.0]
            } else {
                [0.47, 0.47, 0.45, 1.0]
            };
            let sep_light = if render_dark_mode() {
                [0.11, 0.12, 0.12, 1.0]
            } else {
                [1.0, 1.0, 1.0, 1.0]
            };
            canvas.rect(
                Rect::new(
                    item_rect.x + 12.0,
                    item_rect.y + MENU_ITEM_HEIGHT * 0.5,
                    item_rect.width - 24.0,
                    1.0,
                ),
                sep_dark,
            );
            canvas.rect(
                Rect::new(
                    item_rect.x + 12.0,
                    item_rect.y + MENU_ITEM_HEIGHT * 0.5 + 1.0,
                    item_rect.width - 24.0,
                    1.0,
                ),
                sep_light,
            );
            continue;
        }

        let hovered = menu_bar.hovered_item == Some(item_index);
        if hovered && item.enabled {
            let highlight_color = if render_dark_mode() {
                [0.32, 0.35, 0.41, 1.0]
            } else {
                [0.09, 0.09, 0.09, 1.0]
            };
            canvas.rect(item_rect, highlight_color);
        }
        let text_color = if !item.enabled {
            if render_dark_mode() {
                [0.45, 0.46, 0.47, 1.0]
            } else {
                [0.52, 0.52, 0.50, 1.0]
            }
        } else if hovered {
            [1.0, 1.0, 1.0, 1.0]
        } else {
            theme_color("text")
        };
        match item.kind {
            MenuItemKind::Checkbox if item.checked => {
                canvas.text("✓", item_rect.x + 8.0, item_rect.y + 7.0, text_color);
            }
            MenuItemKind::Radio if item.checked => {
                canvas.rect(
                    Rect::new(item_rect.x + 10.0, item_rect.y + 8.0, 5.0, 5.0),
                    text_color,
                );
            }
            _ => {}
        }
        canvas.text(
            &item.label,
            item_rect.x + MENU_LABEL_INSET,
            item_rect.y + 5.0,
            text_color,
        );
        if let Some((key, modifiers)) = item.shortcut {
            let shortcut = shortcut_label(key, modifiers);
            let shortcut_w = canvas.measure_text(&shortcut);
            canvas.text(
                &shortcut,
                item_rect.x + item_rect.width - shortcut_w - MENU_SHORTCUT_INSET,
                item_rect.y + 5.0,
                text_color,
            );
        }
    }
}

fn shortcut_label(key: KeyCode, modifiers: Modifiers) -> String {
    let mut parts = Vec::new();
    if modifiers.control {
        parts.push("Ctrl".to_string());
    }
    if modifiers.alt {
        parts.push("Alt".to_string());
    }
    if modifiers.shift {
        parts.push("Shift".to_string());
    }
    if modifiers.meta {
        parts.push("Cmd".to_string());
    }
    parts.push(key_label(key).to_string());
    parts.join("+")
}

fn key_label(key: KeyCode) -> &'static str {
    match key {
        KeyCode::A => "A",
        KeyCode::B => "B",
        KeyCode::C => "C",
        KeyCode::D => "D",
        KeyCode::E => "E",
        KeyCode::F => "F",
        KeyCode::G => "G",
        KeyCode::H => "H",
        KeyCode::I => "I",
        KeyCode::J => "J",
        KeyCode::K => "K",
        KeyCode::L => "L",
        KeyCode::M => "M",
        KeyCode::N => "N",
        KeyCode::O => "O",
        KeyCode::P => "P",
        KeyCode::Q => "Q",
        KeyCode::R => "R",
        KeyCode::S => "S",
        KeyCode::T => "T",
        KeyCode::U => "U",
        KeyCode::V => "V",
        KeyCode::W => "W",
        KeyCode::X => "X",
        KeyCode::Y => "Y",
        KeyCode::Z => "Z",
        KeyCode::Backspace => "Del",
        KeyCode::Escape => "Esc",
        KeyCode::Enter => "Ret",
        KeyCode::Space => "Space",
        KeyCode::ArrowUp => "Up",
        KeyCode::ArrowDown => "Down",
        KeyCode::ArrowLeft => "Left",
        KeyCode::ArrowRight => "Right",
        _ => "?",
    }
}

fn current_time_string() -> String {
    let now = std::time::SystemTime::now();
    let duration = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format_clock_from_seconds(duration.as_secs())
}

/// Battery + clock for the menu bar right edge, with a single separating space.
fn menu_status_label() -> String {
    let battery = battery_status_string();
    let clock = current_time_string();
    if battery.is_empty() {
        clock
    } else {
        format!("{} {}", battery.trim_end(), clock)
    }
}

/// Returns a compact battery indicator like "[87%]" or "[87% CHG]" when a
/// battery is present, or an empty string on desktops/VMs without one.
fn battery_status_string() -> String {
    let capacity = std::fs::read_to_string("/sys/class/power_supply/BAT0/capacity")
        .ok()
        .and_then(|s| s.trim().parse::<u8>().ok());
    let Some(pct) = capacity else {
        return String::new();
    };
    if pct >= 100 {
        return String::new();
    }
    let charging = std::fs::read_to_string("/sys/class/power_supply/BAT0/status")
        .ok()
        .map(|s| !s.trim().eq_ignore_ascii_case("Discharging"))
        .unwrap_or(false);
    if charging {
        format!("[{}% CHG]", pct)
    } else {
        format!("[{}%]", pct)
    }
}

fn format_clock_from_seconds(seconds_since_epoch: u64) -> String {
    let local_secs = seconds_since_epoch as i64;
    let minutes = (local_secs / 60).rem_euclid(60);
    let hours_24 = (local_secs / 3600).rem_euclid(24);
    let hour_12 = match hours_24 % 12 {
        0 => 12,
        h => h,
    };
    let am_pm = if hours_24 < 12 { "AM" } else { "PM" };
    format!("{}:{:02} {}", hour_12, minutes, am_pm)
}

/// Edge helpers for System 7 multi-layer borders (System7Components recipes).
fn stroke_edges(canvas: &mut Canvas<'_>, rect: Rect, top_left: [f32; 4], bottom_right: [f32; 4]) {
    // Top + leading
    canvas.rect(Rect::new(rect.x, rect.y, rect.width, 1.0), top_left);
    canvas.rect(Rect::new(rect.x, rect.y, 1.0, rect.height), top_left);
    // Bottom + trailing
    canvas.rect(
        Rect::new(rect.x, rect.y + rect.height - 1.0, rect.width, 1.0),
        bottom_right,
    );
    canvas.rect(
        Rect::new(rect.x + rect.width - 1.0, rect.y, 1.0, rect.height),
        bottom_right,
    );
}

/// Port of System7Components `system73DBorder`:
/// black outer border + 1px bottom/right offset shadow.
fn draw_system7_3d_border(canvas: &mut Canvas<'_>, rect: Rect) {
    let fg = if render_dark_mode() {
        COLOR_DARK_BORDER
    } else {
        S7_FG
    };
    // Offset shadow (bottom/trailing)
    canvas.rect(
        Rect::new(rect.x + 1.0, rect.y + rect.height, rect.width, 1.0),
        fg,
    );
    canvas.rect(
        Rect::new(rect.x + rect.width, rect.y + 1.0, 1.0, rect.height),
        fg,
    );
    // Outer black border
    canvas.stroke(rect, fg);
}

/// Port of System73DButtonStyle edge stack (raised or pressed/inset).
fn draw_beveled_rect(canvas: &mut Canvas<'_>, rect: Rect, fill: [f32; 4], raised: bool) {
    if rect.width < 4.0 || rect.height < 4.0 {
        canvas.rect(rect, fill);
        return;
    }
    canvas.rect(rect, fill);

    if render_dark_mode() {
        let light = if raised {
            COLOR_DARK_EDGE_LIGHT
        } else {
            COLOR_DARK_EDGE_DARK
        };
        let dark = if raised {
            COLOR_DARK_EDGE_DARK
        } else {
            COLOR_DARK_EDGE_LIGHT
        };
        stroke_edges(canvas, rect, light, dark);
        let inner = Rect::new(
            rect.x + 1.0,
            rect.y + 1.0,
            rect.width - 2.0,
            rect.height - 2.0,
        );
        if inner.width > 2.0 && inner.height > 2.0 {
            stroke_edges(canvas, inner, light, dark);
        }
        return;
    }

    // Light mode: three nested edge pairs from System73DButtonStyle
    if raised {
        // Outer: top/left Gray500, bottom/right Foreground
        stroke_edges(canvas, rect, S7_GRAY500, S7_FG);
        let mid = Rect::new(
            rect.x + 1.0,
            rect.y + 1.0,
            rect.width - 2.0,
            rect.height - 2.0,
        );
        if mid.width > 2.0 && mid.height > 2.0 {
            // Mid: top/left Gray100, bottom/right Gray300
            stroke_edges(canvas, mid, S7_GRAY100, S7_GRAY300);
            let inner = Rect::new(mid.x + 1.0, mid.y + 1.0, mid.width - 2.0, mid.height - 2.0);
            if inner.width > 1.0 && inner.height > 1.0 {
                // Inner: top/left White, bottom/right Gray300
                stroke_edges(canvas, inner, S7_BG, S7_GRAY300);
            }
        }
    } else {
        // Pressed: edges reverse (inset)
        stroke_edges(canvas, rect, S7_FG, S7_GRAY100);
        let mid = Rect::new(
            rect.x + 1.0,
            rect.y + 1.0,
            rect.width - 2.0,
            rect.height - 2.0,
        );
        if mid.width > 2.0 && mid.height > 2.0 {
            stroke_edges(canvas, mid, S7_GRAY500, S7_GRAY100);
            let inner = Rect::new(mid.x + 1.0, mid.y + 1.0, mid.width - 2.0, mid.height - 2.0);
            if inner.width > 1.0 && inner.height > 1.0 {
                stroke_edges(canvas, inner, S7_GRAY500, S7_GRAY300);
            }
        }
    }
}

fn draw_tree(canvas: &mut Canvas<'_>, rect: Rect, tree: &TreeView) {
    let tree_bg = if render_dark_mode() {
        [0.12, 0.13, 0.14, 1.0]
    } else {
        [0.87, 0.89, 0.90, 1.0]
    };
    canvas.rect(rect, tree_bg);
    canvas.stroke(rect, theme_color("border"));
    let mut y = rect.y + 8.0;
    for (index, node) in tree.roots.iter().enumerate() {
        draw_tree_node(
            canvas,
            node,
            &tree.selected_path,
            &[index],
            rect.x + 10.0,
            &mut y,
            0,
        );
    }
}

fn draw_tree_node(
    canvas: &mut Canvas<'_>,
    node: &TreeNode,
    selected_path: &Option<Vec<usize>>,
    path: &[usize],
    x: f32,
    y: &mut f32,
    depth: usize,
) {
    let selected = selected_path
        .as_ref()
        .is_some_and(|selected| selected == path);
    if selected {
        let selection_color = if render_dark_mode() {
            [0.32, 0.38, 0.49, 1.0]
        } else {
            [0.25, 0.43, 0.67, 1.0]
        };
        canvas.rect(Rect::new(x - 4.0, *y - 3.0, 170.0, 16.0), selection_color);
    }
    canvas.text(
        &node.label,
        x + depth as f32 * 12.0,
        *y,
        if selected {
            [1.0, 1.0, 1.0, 1.0]
        } else {
            theme_color("text")
        },
    );
    *y += 18.0;
    if node.expanded {
        for (index, child) in node.children.iter().enumerate() {
            let mut child_path = path.to_vec();
            child_path.push(index);
            draw_tree_node(canvas, child, selected_path, &child_path, x, y, depth + 1);
        }
    }
}

/// Truncates a string label to a maximum length, preserving file extensions if possible.
///
/// # Assumptions:
/// - **FIXME**: Characters are assumed to have a fixed layout width (7px width spacing inside `Canvas`).
///   This function only checks character length (`label.len()`) rather than visual bounding boxes.
fn truncate_label(label: &str, max_len: usize) -> String {
    // Counted and sliced in chars, never bytes: labels are user filenames and
    // may be multi-byte UTF-8; byte slicing panics on non-boundaries.
    let char_count = label.chars().count();
    if char_count <= max_len {
        return label.to_string();
    }
    let prefix = |n: usize| -> String { label.chars().take(n).collect() };
    if max_len <= 4 {
        return format!("{}...", prefix(max_len.max(3) - 3));
    }
    if let Some(pos) = label.rfind('.') {
        let ext = &label[pos..];
        let ext_chars = ext.chars().count();
        if ext_chars < max_len - 3 {
            let base_len = max_len - 3 - ext_chars;
            return format!("{}...{}", prefix(base_len), ext);
        }
    }
    format!("{}...", prefix(max_len - 3))
}

/// Renders the `IconView` grid.
///
/// # Limitations:
/// - **FIXME**: The current renderer uses the built-in system pixel font, which only supports
///   uppercase characters (lower-case is automatically mapped to upper-case by the rasterizer).
fn draw_icon_view(canvas: &mut Canvas<'_>, icon_view: &IconView) {
    let rect = icon_view.rect();
    let is_desktop = rect.width >= 600.0
        && rect.height >= 360.0
        && icon_view.items.iter().any(|item| item.label == "Hard Disk")
        && icon_view.items.iter().any(|item| item.label == "Trash");
    if is_desktop {
        canvas.with_clip(rect, draw_desktop_backdrop);
    } else {
        canvas.rect(rect, theme_paper());
    }
    for item in &icon_view.items {
        // Desktop labels use the same bounded cell as their hit target, so
        // useful names such as "Applications" are not truncated merely
        // because the icon graphic itself is compact.
        let label_max_width = if is_desktop {
            DESKTOP_ITEM_WIDTH
        } else {
            (item.rect.width + 8.0).max(36.0)
        };
        let display_label = canvas.ellipsize_text(&item.label, label_max_width);
        if item.selected {
            let sel_rect = Rect::new(
                item.rect.x - 4.0,
                item.rect.y - 2.0,
                item.rect.width + 8.0,
                52.0,
            );
            draw_selection_highlight(canvas, sel_rect);
        }
        draw_desktop_icon(canvas, item);
        let label_y = item.rect.y + 36.0;
        let text_w = canvas.measure_text(&display_label);
        let label_center_x = item.rect.x + item.rect.width * 0.5;
        let label_x = (label_center_x - text_w * 0.5).round();
        if item.selected {
            let plate = Rect::new(label_x - 3.0, label_y - 2.0, text_w + 6.0, 14.0);
            canvas.rect(plate, render_accent());
            canvas.text(&display_label, label_x, label_y, [1.0, 1.0, 1.0, 1.0]);
        } else if is_desktop {
            // Desktop dither needs a nameplate; window interiors do not.
            let plate = Rect::new(label_x - 3.0, label_y - 2.0, text_w + 6.0, 14.0);
            canvas.rect(plate, theme_menu());
            canvas.stroke(plate, theme_muted());
            canvas.text(&display_label, label_x, label_y, theme_ink());
        } else {
            canvas.text(&display_label, label_x, label_y, theme_ink());
        }
    }
}

fn draw_selection_highlight(canvas: &mut Canvas<'_>, rect: Rect) {
    let [r, g, b, a] = render_accent();
    let base = [r, g, b, a];
    // Lighter highlight for top/left edges
    let light = [
        (r + 0.25).min(1.0),
        (g + 0.25).min(1.0),
        (b + 0.25).min(1.0),
        a,
    ];
    // Darker shadow for bottom/right edges
    let dark = [r * 0.6, g * 0.6, b * 0.6, a];
    canvas.rect(rect, base);
    canvas.rect(
        Rect::new(rect.x + 1.0, rect.y + 1.0, rect.width - 2.0, 1.0),
        light,
    );
    canvas.rect(
        Rect::new(rect.x + 1.0, rect.y + 1.0, 1.0, rect.height - 2.0),
        light,
    );
    canvas.rect(
        Rect::new(rect.x, rect.y + rect.height - 1.0, rect.width, 1.0),
        dark,
    );
    canvas.rect(
        Rect::new(rect.x + rect.width - 1.0, rect.y, 1.0, rect.height),
        dark,
    );
}

fn draw_monospace_view(canvas: &mut Canvas<'_>, rect: Rect, grid: &MonospaceView) {
    canvas.rect(rect, rgb(12, 12, 12));
    canvas.stroke(rect, rgb(90, 90, 86));
    let cols = grid.cols;
    let rows = grid.rows;
    for row in 0..rows {
        for col in 0..cols {
            let idx = row * cols + col;
            let Some(cell) = grid.cells.get(idx) else {
                continue;
            };
            let x = rect.x + col as f32 * grid.cell_width;
            let y = rect.y + row as f32 * grid.cell_height;
            if cell.bg[3] > 0.0 {
                canvas.rect(Rect::new(x, y, grid.cell_width, grid.cell_height), cell.bg);
            }
            if cell.ch != ' ' {
                canvas.glyph(cell.ch, x + 1.0, y + 4.0, cell.fg);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconStyle {
    Retro,
    Material,
}

pub fn current_icon_style() -> IconStyle {
    if let Ok(val) = std::env::var("SLOPOS_ICON_STYLE") {
        if val.eq_ignore_ascii_case("material") {
            return IconStyle::Material;
        }
    }
    IconStyle::Retro
}

fn draw_desktop_icon(canvas: &mut Canvas<'_>, item: &IconItem) {
    // Fixed 32×32 icon footprint centered in the cell — NEVER use full
    // item.rect width for the shadow (that painted the gray column bands).
    const ICON: f32 = 32.0;
    let x = item.rect.x + (item.rect.width - ICON) * 0.5;
    let y = item.rect.y + 2.0;
    canvas.rect(
        Rect::new(x + 2.0, y + 2.0, ICON, ICON),
        [0.0, 0.0, 0.0, 0.25],
    );

    // Prefer known desktop/app labels over generic icon kind tags.
    match item.label.as_str() {
        "Hard Disk" | "Home" | "Trash" | "Applications" | "App Store" | "Finder" | "Settings"
        | "Terminal" | "TextEdit" => {
            draw_labeled_icon(canvas, item.label.as_str(), x, y);
            return;
        }
        _ => {}
    }

    if let Some(kind) = item.icon.as_deref() {
        match kind {
            "folder" => {
                draw_folder_icon(canvas, x - 6.0, y - 4.0, rgb(226, 216, 142));
                return;
            }
            "document" => {
                draw_document_icon(canvas, x - 6.0, y - 4.0);
                return;
            }
            "image" => {
                draw_image_icon(canvas, x, y);
                return;
            }
            "audio" => {
                draw_audio_icon(canvas, x, y);
                return;
            }
            "video" => {
                draw_video_icon(canvas, x, y);
                return;
            }
            "code" => {
                draw_code_icon(canvas, x, y);
                return;
            }
            "archive" => {
                draw_archive_icon(canvas, x, y);
                return;
            }
            "network" => {
                draw_network_icon(canvas, x, y);
                return;
            }
            "user" => {
                draw_user_icon(canvas, x, y);
                return;
            }
            _ => {}
        }
    }
    draw_labeled_icon(canvas, item.label.as_str(), x, y);
}

/// Dispatch per-app icons by label with Retro or Material style support.
fn draw_labeled_icon(canvas: &mut Canvas<'_>, label: &str, x: f32, y: f32) {
    if current_icon_style() == IconStyle::Material {
        draw_material_icon(canvas, label, x, y);
        return;
    }
    match label {
        "Hard Disk" => draw_drive_icon(canvas, x - 6.0, y - 4.0),
        "Home" => draw_folder_icon(canvas, x - 6.0, y - 4.0, rgb(226, 216, 142)),
        "Trash" => draw_trash_icon(canvas, x - 4.0, y - 4.0),
        "Applications" => draw_applications_icon(canvas, x, y),
        "App Store" => draw_store_icon(canvas, x, y),
        "Finder" => draw_finder_icon(canvas, x, y),
        "Settings" => draw_settings_icon(canvas, x, y),
        "Terminal" => draw_terminal_icon(canvas, x, y),
        "TextEdit" => draw_textedit_icon(canvas, x, y),
        _ => draw_generic_app_icon(canvas, x, y),
    }
}

fn draw_material_icon(canvas: &mut Canvas<'_>, label: &str, x: f32, y: f32) {
    let card_color = match label {
        "Hard Disk" => rgb(66, 133, 244),    // Material Blue
        "Home" => rgb(251, 188, 4),          // Material Amber
        "Trash" => rgb(234, 67, 53),         // Material Red
        "Applications" => rgb(103, 58, 183), // Material Purple
        "App Store" => rgb(52, 168, 83),     // Material Green
        "Finder" => rgb(0, 172, 193),        // Material Cyan
        "Settings" => rgb(96, 125, 139),     // Material Blue Grey
        "Terminal" => rgb(38, 50, 56),       // Material Dark Slate
        "TextEdit" => rgb(255, 112, 67),     // Material Deep Orange
        _ => rgb(120, 144, 156),
    };

    // Rounded Material card base
    canvas.rect(Rect::new(x, y, 32.0, 32.0), card_color);
    canvas.rect(
        Rect::new(x + 2.0, y + 2.0, 28.0, 28.0),
        [1.0, 1.0, 1.0, 0.15],
    );

    // Material inner glyph symbol
    match label {
        "Hard Disk" => {
            canvas.rect(
                Rect::new(x + 8.0, y + 10.0, 16.0, 12.0),
                [1.0, 1.0, 1.0, 0.9],
            );
            canvas.rect(Rect::new(x + 12.0, y + 14.0, 8.0, 4.0), card_color);
        }
        "Home" | "folder" => {
            canvas.rect(
                Rect::new(x + 6.0, y + 10.0, 20.0, 14.0),
                [1.0, 1.0, 1.0, 0.9],
            );
            canvas.rect(Rect::new(x + 6.0, y + 8.0, 8.0, 4.0), [1.0, 1.0, 1.0, 0.9]);
        }
        "Trash" => {
            canvas.rect(
                Rect::new(x + 10.0, y + 10.0, 12.0, 14.0),
                [1.0, 1.0, 1.0, 0.9],
            );
            canvas.rect(Rect::new(x + 8.0, y + 8.0, 16.0, 2.0), [1.0, 1.0, 1.0, 0.9]);
        }
        "Settings" => {
            canvas.rect(
                Rect::new(x + 10.0, y + 10.0, 12.0, 12.0),
                [1.0, 1.0, 1.0, 0.9],
            );
            canvas.rect(Rect::new(x + 14.0, y + 14.0, 4.0, 4.0), card_color);
        }
        "Terminal" => {
            canvas.rect(Rect::new(x + 8.0, y + 12.0, 6.0, 2.0), rgb(80, 220, 120));
            canvas.rect(Rect::new(x + 12.0, y + 14.0, 2.0, 4.0), rgb(80, 220, 120));
            canvas.rect(
                Rect::new(x + 14.0, y + 18.0, 10.0, 2.0),
                [1.0, 1.0, 1.0, 0.9],
            );
        }
        _ => {
            canvas.rect(
                Rect::new(x + 10.0, y + 10.0, 12.0, 12.0),
                [1.0, 1.0, 1.0, 0.8],
            );
        }
    }
}

fn draw_image_icon(canvas: &mut Canvas<'_>, x: f32, y: f32) {
    draw_beveled_rect(
        canvas,
        Rect::new(x + 2.0, y + 2.0, 28.0, 28.0),
        rgb(180, 130, 210),
        true,
    );
    canvas.rect(Rect::new(x + 6.0, y + 6.0, 20.0, 20.0), theme_paper());
    canvas.rect(Rect::new(x + 10.0, y + 14.0, 12.0, 8.0), rgb(100, 160, 220));
    canvas.rect(Rect::new(x + 18.0, y + 9.0, 4.0, 4.0), rgb(240, 200, 80));
}

fn draw_audio_icon(canvas: &mut Canvas<'_>, x: f32, y: f32) {
    draw_beveled_rect(
        canvas,
        Rect::new(x + 2.0, y + 2.0, 28.0, 28.0),
        rgb(240, 160, 80),
        true,
    );
    canvas.rect(Rect::new(x + 8.0, y + 16.0, 6.0, 8.0), theme_ink());
    canvas.rect(Rect::new(x + 18.0, y + 10.0, 6.0, 8.0), theme_ink());
    canvas.rect(Rect::new(x + 12.0, y + 10.0, 12.0, 3.0), theme_ink());
}

fn draw_video_icon(canvas: &mut Canvas<'_>, x: f32, y: f32) {
    draw_beveled_rect(
        canvas,
        Rect::new(x + 2.0, y + 2.0, 28.0, 28.0),
        rgb(220, 80, 80),
        true,
    );
    canvas.rect(Rect::new(x + 6.0, y + 8.0, 20.0, 16.0), theme_paper());
    canvas.rect(Rect::new(x + 13.0, y + 12.0, 6.0, 8.0), rgb(220, 80, 80));
}

fn draw_code_icon(canvas: &mut Canvas<'_>, x: f32, y: f32) {
    draw_beveled_rect(
        canvas,
        Rect::new(x + 2.0, y + 2.0, 28.0, 28.0),
        rgb(60, 80, 120),
        true,
    );
    canvas.rect(Rect::new(x + 7.0, y + 12.0, 4.0, 8.0), rgb(80, 220, 120));
    canvas.rect(Rect::new(x + 21.0, y + 12.0, 4.0, 8.0), rgb(80, 220, 120));
    canvas.rect(Rect::new(x + 13.0, y + 10.0, 6.0, 12.0), theme_paper());
}

fn draw_archive_icon(canvas: &mut Canvas<'_>, x: f32, y: f32) {
    draw_beveled_rect(
        canvas,
        Rect::new(x + 2.0, y + 2.0, 28.0, 28.0),
        rgb(200, 140, 60),
        true,
    );
    canvas.rect(Rect::new(x + 6.0, y + 8.0, 20.0, 6.0), theme_paper());
    canvas.rect(Rect::new(x + 8.0, y + 14.0, 16.0, 12.0), theme_paper());
    canvas.rect(Rect::new(x + 14.0, y + 16.0, 4.0, 4.0), theme_ink());
}

fn draw_network_icon(canvas: &mut Canvas<'_>, x: f32, y: f32) {
    draw_beveled_rect(
        canvas,
        Rect::new(x + 2.0, y + 2.0, 28.0, 28.0),
        rgb(60, 160, 200),
        true,
    );
    canvas.rect(Rect::new(x + 8.0, y + 8.0, 16.0, 16.0), theme_paper());
    canvas.rect(Rect::new(x + 10.0, y + 10.0, 12.0, 12.0), rgb(60, 160, 200));
}

fn draw_user_icon(canvas: &mut Canvas<'_>, x: f32, y: f32) {
    draw_beveled_rect(
        canvas,
        Rect::new(x + 2.0, y + 2.0, 28.0, 28.0),
        rgb(0, 150, 136),
        true,
    );
    canvas.rect(Rect::new(x + 12.0, y + 8.0, 8.0, 8.0), theme_paper());
    canvas.rect(Rect::new(x + 8.0, y + 18.0, 16.0, 8.0), theme_paper());
}

fn draw_drive_icon(canvas: &mut Canvas<'_>, x: f32, y: f32) {
    // Disk casing — theme-aware Graphite/Platinum
    draw_beveled_rect(
        canvas,
        Rect::new(x, y + 8.0, 44.0, 28.0),
        theme_face(),
        true,
    );
    // Disc slot
    canvas.rect(Rect::new(x + 6.0, y + 14.0, 32.0, 3.0), theme_ink());
    // LED Dot
    canvas.rect(Rect::new(x + 34.0, y + 26.0, 4.0, 4.0), rgb(80, 220, 80));
}

fn draw_document_icon(canvas: &mut Canvas<'_>, x: f32, y: f32) {
    draw_beveled_rect(
        canvas,
        Rect::new(x + 8.0, y + 4.0, 28.0, 36.0),
        theme_paper(),
        true,
    );
    canvas.rect(Rect::new(x + 13.0, y + 12.0, 1.0, 20.0), S7_LAVENDER300);
    canvas.rect(Rect::new(x + 16.0, y + 15.0, 14.0, 1.0), theme_muted());
    canvas.rect(Rect::new(x + 16.0, y + 21.0, 16.0, 1.0), theme_muted());
    canvas.rect(Rect::new(x + 16.0, y + 27.0, 12.0, 1.0), theme_muted());
    canvas.rect(Rect::new(x + 29.0, y + 4.0, 7.0, 7.0), theme_face());
    canvas.rect(Rect::new(x + 29.0, y + 11.0, 8.0, 1.0), theme_muted());
    canvas.rect(Rect::new(x + 28.0, y + 4.0, 1.0, 8.0), theme_muted());
}

fn draw_folder_icon(canvas: &mut Canvas<'_>, x: f32, y: f32, color: [f32; 4]) {
    // Back tab
    canvas.rect(Rect::new(x + 3.0, y + 10.0, 16.0, 6.0), rgb(180, 160, 90));
    canvas.rect(Rect::new(x + 4.0, y + 9.0, 14.0, 1.0), rgb(230, 220, 160));
    // Front body
    draw_beveled_rect(canvas, Rect::new(x, y + 15.0, 44.0, 26.0), color, true);
    // Folder accent highlights
    canvas.rect(Rect::new(x + 1.0, y + 16.0, 42.0, 1.0), rgb(250, 245, 210));
    canvas.rect(Rect::new(x, y + 40.0, 44.0, 1.0), rgb(120, 110, 60));
}

fn draw_app_icon(canvas: &mut Canvas<'_>, x: f32, y: f32) {
    draw_generic_app_icon(canvas, x + 6.0, y + 6.0);
}

/// Generic app (fallback) — stamped rectangle, theme-aware.
fn draw_generic_app_icon(canvas: &mut Canvas<'_>, x: f32, y: f32) {
    draw_beveled_rect(canvas, Rect::new(x, y, 32.0, 32.0), theme_face(), true);
    canvas.rect(Rect::new(x + 6.0, y + 8.0, 20.0, 3.0), theme_muted());
    canvas.rect(Rect::new(x + 6.0, y + 14.0, 16.0, 3.0), theme_muted());
    canvas.rect(Rect::new(x + 6.0, y + 20.0, 12.0, 3.0), theme_muted());
}

/// Face/window finder metaphor — not Apple logo.
fn draw_finder_icon(canvas: &mut Canvas<'_>, x: f32, y: f32) {
    draw_beveled_rect(canvas, Rect::new(x, y, 32.0, 32.0), theme_face(), true);
    canvas.rect(Rect::new(x + 4.0, y + 4.0, 24.0, 6.0), theme_muted());
    canvas.rect(Rect::new(x + 6.0, y + 5.0, 4.0, 4.0), theme_paper());
    let pane = if render_dark_mode() {
        [0.35, 0.35, 0.45, 1.0]
    } else {
        S7_LAVENDER100
    };
    canvas.rect(Rect::new(x + 4.0, y + 12.0, 10.0, 14.0), pane);
    canvas.rect(Rect::new(x + 16.0, y + 12.0, 12.0, 14.0), theme_paper());
    canvas.stroke(Rect::new(x + 4.0, y + 12.0, 24.0, 14.0), theme_ink());
}

fn draw_settings_icon(canvas: &mut Canvas<'_>, x: f32, y: f32) {
    draw_beveled_rect(canvas, Rect::new(x, y, 32.0, 32.0), theme_face(), true);
    canvas.rect(Rect::new(x + 10.0, y + 6.0, 12.0, 4.0), theme_muted());
    canvas.rect(Rect::new(x + 10.0, y + 22.0, 12.0, 4.0), theme_muted());
    canvas.rect(Rect::new(x + 6.0, y + 10.0, 4.0, 12.0), theme_muted());
    canvas.rect(Rect::new(x + 22.0, y + 10.0, 4.0, 12.0), theme_muted());
    canvas.rect(Rect::new(x + 11.0, y + 11.0, 10.0, 10.0), theme_muted());
    canvas.rect(Rect::new(x + 14.0, y + 14.0, 4.0, 4.0), theme_paper());
}

fn draw_terminal_icon(canvas: &mut Canvas<'_>, x: f32, y: f32) {
    let screen = if render_dark_mode() {
        [0.08, 0.10, 0.10, 1.0]
    } else {
        rgb(40, 44, 48)
    };
    draw_beveled_rect(canvas, Rect::new(x, y, 32.0, 32.0), screen, true);
    canvas.rect(Rect::new(x + 6.0, y + 10.0, 8.0, 2.0), rgb(80, 220, 120));
    canvas.rect(Rect::new(x + 6.0, y + 14.0, 2.0, 8.0), rgb(80, 220, 120));
    canvas.rect(Rect::new(x + 10.0, y + 20.0, 14.0, 2.0), theme_muted());
}

fn draw_textedit_icon(canvas: &mut Canvas<'_>, x: f32, y: f32) {
    draw_beveled_rect(
        canvas,
        Rect::new(x + 4.0, y + 2.0, 24.0, 28.0),
        theme_paper(),
        true,
    );
    canvas.rect(Rect::new(x + 20.0, y + 2.0, 8.0, 8.0), theme_face());
    canvas.rect(Rect::new(x + 8.0, y + 12.0, 16.0, 2.0), theme_ink());
    canvas.rect(Rect::new(x + 8.0, y + 17.0, 14.0, 2.0), theme_muted());
    canvas.rect(Rect::new(x + 8.0, y + 22.0, 12.0, 2.0), theme_muted());
}

fn draw_store_icon(canvas: &mut Canvas<'_>, x: f32, y: f32) {
    let bag_bg = if render_dark_mode() {
        [0.30, 0.30, 0.40, 1.0]
    } else {
        S7_LAVENDER100
    };
    draw_beveled_rect(canvas, Rect::new(x, y, 32.0, 32.0), bag_bg, true);
    canvas.rect(Rect::new(x + 8.0, y + 12.0, 16.0, 14.0), theme_paper());
    canvas.stroke(Rect::new(x + 8.0, y + 12.0, 16.0, 14.0), theme_ink());
    canvas.rect(Rect::new(x + 12.0, y + 8.0, 8.0, 6.0), theme_muted());
    canvas.rect(Rect::new(x + 14.0, y + 16.0, 4.0, 6.0), S7_LAVENDER300);
}

fn draw_applications_icon(canvas: &mut Canvas<'_>, x: f32, y: f32) {
    draw_beveled_rect(canvas, Rect::new(x, y, 32.0, 32.0), theme_face(), true);
    let tile = theme_muted();
    for (dx, dy) in [(5.0, 5.0), (17.0, 5.0), (5.0, 17.0), (17.0, 17.0)] {
        canvas.rect(Rect::new(x + dx, y + dy, 10.0, 10.0), tile);
        canvas.rect(
            Rect::new(x + dx + 2.0, y + dy + 2.0, 6.0, 2.0),
            theme_paper(),
        );
    }
}

fn draw_trash_icon(canvas: &mut Canvas<'_>, x: f32, y: f32) {
    let lid_color = theme_face();
    let body_color = theme_muted();
    let shadow_color = theme_ink();

    // Handle
    canvas.rect(Rect::new(x + 18.0, y + 2.0, 8.0, 3.0), lid_color);
    canvas.rect(Rect::new(x + 19.0, y + 1.0, 6.0, 1.0), theme_paper());

    // Lid rim
    draw_beveled_rect(
        canvas,
        Rect::new(x + 6.0, y + 5.0, 32.0, 5.0),
        lid_color,
        true,
    );

    // Can body
    draw_beveled_rect(
        canvas,
        Rect::new(x + 9.0, y + 10.0, 26.0, 34.0),
        body_color,
        true,
    );

    // Rib highlights
    for offset in [14.0, 20.0, 26.0, 32.0] {
        canvas.rect(Rect::new(x + offset, y + 14.0, 1.0, 26.0), shadow_color);
        canvas.rect(
            Rect::new(x + offset + 1.0, y + 14.0, 1.0, 26.0),
            theme_face(),
        );
    }
}

fn draw_list(canvas: &mut Canvas<'_>, rect: Rect, list: &ListView) {
    let list_bg = if render_dark_mode() {
        [0.09, 0.10, 0.11, 1.0]
    } else {
        [1.0, 1.0, 0.99, 1.0]
    };
    canvas.rect(rect, list_bg);
    canvas.stroke(rect, theme_color("border"));
    for (index, item) in list.items.iter().enumerate() {
        let y = rect.y + 6.0 + index as f32 * 18.0;
        if list.selected_index == Some(index) {
            let selection_color = if render_dark_mode() {
                [0.32, 0.38, 0.49, 1.0]
            } else {
                [0.25, 0.43, 0.67, 1.0]
            };
            canvas.rect(
                Rect::new(rect.x + 3.0, y - 3.0, rect.width - 6.0, 16.0),
                selection_color,
            );
        }
        canvas.text(
            item,
            rect.x + 8.0,
            y,
            if list.selected_index == Some(index) {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                theme_color("text")
            },
        );
    }
}

fn intersect_rect(a: Rect, b: Rect) -> Option<Rect> {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = (a.x + a.width).min(b.x + b.width);
    let y1 = (a.y + a.height).min(b.y + b.height);
    (x1 > x0 && y1 > y0).then(|| Rect::new(x0, y0, x1 - x0, y1 - y0))
}

fn rgb(r: u8, g: u8, b: u8) -> [f32; 4] {
    rgba(r, g, b, 1.0)
}

fn rgba(r: u8, g: u8, b: u8, a: f32) -> [f32; 4] {
    [
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
        a.clamp(0.0, 1.0),
    ]
}

fn _color_to_rgb(color: Color) -> [f32; 4] {
    [
        color.r.clamp(0.0, 1.0),
        color.g.clamp(0.0, 1.0),
        color.b.clamp(0.0, 1.0),
        color.a.clamp(0.0, 1.0),
    ]
}

fn glyph_pattern(ch: char) -> [u8; 9] {
    match ch {
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001, 0, 0,
        ],
        'B' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110, 0, 0,
        ],
        'C' => [
            0b01111, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b01111, 0, 0,
        ],
        'D' => [
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110, 0, 0,
        ],
        'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111, 0, 0,
        ],
        'F' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000, 0, 0,
        ],
        'G' => [
            0b01111, 0b10000, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111, 0, 0,
        ],
        'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001, 0, 0,
        ],
        'I' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111, 0, 0,
        ],
        'J' => [
            0b00111, 0b00010, 0b00010, 0b00010, 0b10010, 0b10010, 0b01100, 0, 0,
        ],
        'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001, 0, 0,
        ],
        'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111, 0, 0,
        ],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001, 0, 0,
        ],
        'N' => [
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001, 0, 0,
        ],
        'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110, 0, 0,
        ],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000, 0, 0,
        ],
        'Q' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101, 0, 0,
        ],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001, 0, 0,
        ],
        'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110, 0, 0,
        ],
        'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0, 0,
        ],
        'U' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110, 0, 0,
        ],
        'V' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b01010, 0b00100, 0, 0,
        ],
        'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010, 0, 0,
        ],
        'X' => [
            0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b01010, 0b10001, 0, 0,
        ],
        'Y' => [
            0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0, 0,
        ],
        'Z' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111, 0, 0,
        ],
        'a' => [
            0b00000, 0b00000, 0b01110, 0b00001, 0b01111, 0b10001, 0b01111, 0, 0,
        ],
        'b' => [
            0b10000, 0b10000, 0b10110, 0b11001, 0b10001, 0b10001, 0b11110, 0, 0,
        ],
        'c' => [
            0b00000, 0b00000, 0b01110, 0b10000, 0b10000, 0b10000, 0b01110, 0, 0,
        ],
        'd' => [
            0b00001, 0b00001, 0b01101, 0b10011, 0b10001, 0b10001, 0b01111, 0, 0,
        ],
        'e' => [
            0b00000, 0b00000, 0b01110, 0b10001, 0b11111, 0b10000, 0b01110, 0, 0,
        ],
        'f' => [
            0b00110, 0b01001, 0b01000, 0b11110, 0b01000, 0b01000, 0b01000, 0, 0,
        ],
        'g' => [
            0b00000, 0b00000, 0b01110, 0b10001, 0b01111, 0b00001, 0b01110, 0b00001, 0b01110,
        ],
        'h' => [
            0b10000, 0b10000, 0b10110, 0b11001, 0b10001, 0b10001, 0b10001, 0, 0,
        ],
        'i' => [
            0b00100, 0b00000, 0b01100, 0b00100, 0b00100, 0b00100, 0b01110, 0, 0,
        ],
        'j' => [
            0b00010, 0b00000, 0b00110, 0b00010, 0b00010, 0b10010, 0b01100, 0b10010, 0b01100,
        ],
        'k' => [
            0b10000, 0b10000, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0, 0,
        ],
        'l' => [
            0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110, 0, 0,
        ],
        'm' => [
            0b00000, 0b00000, 0b11010, 0b10101, 0b10101, 0b10101, 0b10101, 0, 0,
        ],
        'n' => [
            0b00000, 0b00000, 0b10110, 0b11001, 0b10001, 0b10001, 0b10001, 0, 0,
        ],
        'o' => [
            0b00000, 0b00000, 0b01110, 0b10001, 0b10001, 0b10001, 0b01110, 0, 0,
        ],
        'p' => [
            0b00000, 0b00000, 0b01100, 0b01010, 0b01100, 0b01000, 0b01000, 0b01000, 0b01000,
        ],
        'q' => [
            0b00000, 0b00000, 0b01100, 0b10100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'r' => [
            0b00000, 0b00000, 0b10110, 0b11001, 0b10000, 0b10000, 0b10000, 0, 0,
        ],
        's' => [
            0b00000, 0b00000, 0b01111, 0b10000, 0b01110, 0b00001, 0b11110, 0, 0,
        ],
        't' => [
            0b00100, 0b00100, 0b11110, 0b00100, 0b00100, 0b00100, 0b00011, 0, 0,
        ],
        'u' => [
            0b00000, 0b00000, 0b10001, 0b10001, 0b10001, 0b10011, 0b01101, 0, 0,
        ],
        'v' => [
            0b00000, 0b00000, 0b10001, 0b10001, 0b01010, 0b01010, 0b00100, 0, 0,
        ],
        'w' => [
            0b00000, 0b00000, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010, 0, 0,
        ],
        'x' => [
            0b00000, 0b00000, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0, 0,
        ],
        'y' => [
            0b00000, 0b00000, 0b10001, 0b10001, 0b01111, 0b00001, 0b01110, 0b00001, 0b01110,
        ],
        'z' => [
            0b00000, 0b00000, 0b11111, 0b00010, 0b00100, 0b01000, 0b11111, 0, 0,
        ],
        '0' => [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110, 0, 0,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110, 0, 0,
        ],
        '2' => [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111, 0, 0,
        ],
        '3' => [
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110, 0, 0,
        ],
        '4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010, 0, 0,
        ],
        '5' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b00001, 0b00001, 0b11110, 0, 0,
        ],
        '6' => [
            0b01110, 0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110, 0, 0,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000, 0, 0,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110, 0, 0,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110, 0, 0,
        ],
        '-' => [0, 0, 0, 0b11111, 0, 0, 0, 0, 0],
        '+' => [0, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0, 0, 0],
        '_' => [0, 0, 0, 0, 0, 0, 0b11111, 0, 0],
        '.' => [0, 0, 0, 0, 0, 0b01100, 0b01100, 0, 0],
        ':' => [0, 0b01100, 0b01100, 0, 0b01100, 0b01100, 0, 0, 0],
        '/' => [
            0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b01000, 0b10000, 0, 0,
        ],
        '\\' => [
            0b10000, 0b01000, 0b01000, 0b00100, 0b00010, 0b00010, 0b00001, 0, 0,
        ],
        '(' => [
            0b00010, 0b00100, 0b01000, 0b01000, 0b01000, 0b00100, 0b00010, 0, 0,
        ],
        ')' => [
            0b01000, 0b00100, 0b00010, 0b00010, 0b00010, 0b00100, 0b01000, 0, 0,
        ],
        ',' => [0, 0, 0, 0, 0, 0b01100, 0b00100, 0b01100, 0b00100],
        '!' => [0b00100, 0b00100, 0b00100, 0b00100, 0, 0b00100, 0, 0, 0],
        '?' => [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0, 0b00100, 0, 0,
        ],
        '=' => [0, 0, 0b11111, 0, 0b11111, 0, 0, 0, 0],
        '&' => [
            0b01100, 0b10010, 0b10100, 0b01000, 0b10101, 0b10010, 0b01101, 0, 0,
        ],
        ' ' => [0, 0, 0, 0, 0, 0, 0, 0, 0],
        _ => [
            0b11111, 0b10001, 0b00010, 0b00100, 0b00000, 0b00100, 0b00100, 0, 0,
        ],
    }
}

pub fn modifiers_from_winit(modifiers: winit::keyboard::ModifiersState) -> Modifiers {
    Modifiers {
        shift: modifiers.shift_key(),
        control: modifiers.control_key(),
        alt: modifiers.alt_key(),
        meta: modifiers.super_key(),
    }
}

pub fn winit_to_retro_mouse_button(button: winit::event::MouseButton) -> Option<MouseButton> {
    match button {
        winit::event::MouseButton::Left => Some(MouseButton::Left),
        winit::event::MouseButton::Right => Some(MouseButton::Right),
        winit::event::MouseButton::Middle => Some(MouseButton::Middle),
        winit::event::MouseButton::Back => Some(MouseButton::Back),
        winit::event::MouseButton::Forward => Some(MouseButton::Forward),
        winit::event::MouseButton::Other(_) => None,
    }
}

pub fn winit_to_retro_scroll_delta(delta: winit::event::MouseScrollDelta) -> Point {
    match delta {
        winit::event::MouseScrollDelta::LineDelta(x, y) => Point::new(x * 16.0, y * 16.0),
        winit::event::MouseScrollDelta::PixelDelta(pos) => Point::new(pos.x as f32, pos.y as f32),
    }
}

pub fn winit_to_retro_key(key: winit::keyboard::KeyCode) -> Option<KeyCode> {
    use slopos_kit::event::KeyCode as RKey;
    use winit::keyboard::KeyCode as WKey;

    match key {
        WKey::KeyA => Some(RKey::A),
        WKey::KeyB => Some(RKey::B),
        WKey::KeyC => Some(RKey::C),
        WKey::KeyD => Some(RKey::D),
        WKey::KeyE => Some(RKey::E),
        WKey::KeyF => Some(RKey::F),
        WKey::KeyG => Some(RKey::G),
        WKey::KeyH => Some(RKey::H),
        WKey::KeyI => Some(RKey::I),
        WKey::KeyJ => Some(RKey::J),
        WKey::KeyK => Some(RKey::K),
        WKey::KeyL => Some(RKey::L),
        WKey::KeyM => Some(RKey::M),
        WKey::KeyN => Some(RKey::N),
        WKey::KeyO => Some(RKey::O),
        WKey::KeyP => Some(RKey::P),
        WKey::KeyQ => Some(RKey::Q),
        WKey::KeyR => Some(RKey::R),
        WKey::KeyS => Some(RKey::S),
        WKey::KeyT => Some(RKey::T),
        WKey::KeyU => Some(RKey::U),
        WKey::KeyV => Some(RKey::V),
        WKey::KeyW => Some(RKey::W),
        WKey::KeyX => Some(RKey::X),
        WKey::KeyY => Some(RKey::Y),
        WKey::KeyZ => Some(RKey::Z),
        WKey::Digit0 => Some(RKey::Key0),
        WKey::Digit1 => Some(RKey::Key1),
        WKey::Digit2 => Some(RKey::Key2),
        WKey::Digit3 => Some(RKey::Key3),
        WKey::Digit4 => Some(RKey::Key4),
        WKey::Digit5 => Some(RKey::Key5),
        WKey::Digit6 => Some(RKey::Key6),
        WKey::Digit7 => Some(RKey::Key7),
        WKey::Digit8 => Some(RKey::Key8),
        WKey::Digit9 => Some(RKey::Key9),
        WKey::F1 => Some(RKey::F1),
        WKey::F2 => Some(RKey::F2),
        WKey::F3 => Some(RKey::F3),
        WKey::F4 => Some(RKey::F4),
        WKey::F5 => Some(RKey::F5),
        WKey::F6 => Some(RKey::F6),
        WKey::F7 => Some(RKey::F7),
        WKey::F8 => Some(RKey::F8),
        WKey::F9 => Some(RKey::F9),
        WKey::F10 => Some(RKey::F10),
        WKey::F11 => Some(RKey::F11),
        WKey::F12 => Some(RKey::F12),
        WKey::Escape => Some(RKey::Escape),
        WKey::Tab => Some(RKey::Tab),
        WKey::CapsLock => Some(RKey::CapsLock),
        WKey::ShiftLeft => Some(RKey::ShiftLeft),
        WKey::ShiftRight => Some(RKey::ShiftRight),
        WKey::ControlLeft => Some(RKey::ControlLeft),
        WKey::ControlRight => Some(RKey::ControlRight),
        WKey::AltLeft => Some(RKey::AltLeft),
        WKey::AltRight => Some(RKey::AltRight),
        WKey::Space => Some(RKey::Space),
        WKey::Enter => Some(RKey::Enter),
        WKey::Backspace => Some(RKey::Backspace),
        WKey::Delete => Some(RKey::Delete),
        WKey::Insert => Some(RKey::Insert),
        WKey::Home => Some(RKey::Home),
        WKey::End => Some(RKey::End),
        WKey::PageUp => Some(RKey::PageUp),
        WKey::PageDown => Some(RKey::PageDown),
        WKey::ArrowUp => Some(RKey::ArrowUp),
        WKey::ArrowDown => Some(RKey::ArrowDown),
        WKey::ArrowLeft => Some(RKey::ArrowLeft),
        WKey::ArrowRight => Some(RKey::ArrowRight),
        WKey::SuperLeft => Some(RKey::MetaLeft),
        WKey::SuperRight => Some(RKey::MetaRight),
        WKey::Minus => Some(RKey::Minus),
        WKey::Equal => Some(RKey::Equals),
        WKey::BracketLeft => Some(RKey::LeftBracket),
        WKey::BracketRight => Some(RKey::RightBracket),
        WKey::Backslash => Some(RKey::Backslash),
        WKey::Semicolon => Some(RKey::Semicolon),
        WKey::Quote => Some(RKey::Quote),
        WKey::Comma => Some(RKey::Comma),
        WKey::Period => Some(RKey::Period),
        WKey::Slash => Some(RKey::Slash),
        _ => None,
    }
}

fn distance_squared(a: Point, b: Point) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy
}

#[cfg(test)]
mod tests {
    use super::{
        format_clock_from_seconds, inactive_title_color, is_application_menu_action,
        menu_button_advance, parse_theme_preference, publish_bytes_atomically, status_item_advance,
        theme_accents, ApplicationMenuAction, Canvas, CLASSIC_DARK_GRAY_RGBA,
        COLOR_DARK_TITLE_INACTIVE, DESKTOP_ITEM_WIDTH,
    };
    use slopos_kit::{ImageView, Rect, Widget, WorkspaceGridView};
    use slopos_render::font::{shape_text, TextLayoutOptions};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn image_commands_tile_the_full_source_without_panel_expansion() {
        let image = ImageView::new(2049, 2049, vec![255; 2049 * 2049 * 4]).unwrap();
        let mut canvas = Canvas::new(2049.0, 2049.0);
        canvas.image(&image, Rect::new(0.0, 0.0, 2049.0, 2049.0));
        let draw_data = canvas.finish();

        assert_eq!(
            draw_data
                .commands
                .iter()
                .filter(|command| matches!(command, super::DrawCommand::Image { .. }))
                .count(),
            4
        );
        assert_eq!(draw_data.image_vertices.len(), 24);
    }

    #[test]
    fn image_commands_only_emit_tiles_intersecting_the_clip() {
        let image = ImageView::new(4097, 1, vec![255; 4097 * 4]).unwrap();
        let mut canvas = Canvas::new(4097.0, 1.0);
        canvas.with_clip(Rect::new(2040.0, 0.0, 20.0, 1.0), |canvas| {
            canvas.image(&image, Rect::new(0.0, 0.0, 4097.0, 1.0));
        });
        let draw_data = canvas.finish();

        assert_eq!(
            draw_data
                .commands
                .iter()
                .filter(|command| matches!(command, super::DrawCommand::Image { .. }))
                .count(),
            2
        );
        assert_eq!(draw_data.image_vertices.len(), 12);
    }

    #[test]
    fn rotated_image_vertices_keep_source_dimensions_and_use_clockwise_uvs() {
        let source_pixels = vec![255; 2 * 3 * 4];
        let expected_uvs = [
            [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            [[0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
            [[1.0, 1.0], [0.0, 1.0], [0.0, 0.0], [1.0, 0.0]],
            [[1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.0, 0.0]],
        ];

        for (rotation, expected) in expected_uvs.into_iter().enumerate() {
            let mut image = ImageView::new(2, 3, source_pixels.clone()).unwrap();
            image.set_rotation_quadrants(rotation as u8);
            let (display_width, display_height) = image.display_dimensions();
            assert_eq!(
                (display_width, display_height),
                if rotation % 2 == 0 { (2, 3) } else { (3, 2) }
            );

            let mut canvas = Canvas::new(100.0, 100.0);
            canvas.image(
                &image,
                Rect::new(
                    10.0,
                    20.0,
                    display_width as f32 * 10.0,
                    display_height as f32 * 10.0,
                ),
            );
            let draw_data = canvas.finish();
            assert_eq!(draw_data.image_vertices.len(), 6);
            let actual = [
                draw_data.image_vertices[0].uv,
                draw_data.image_vertices[1].uv,
                draw_data.image_vertices[2].uv,
                draw_data.image_vertices[5].uv,
            ];
            assert_eq!(actual, expected);
            let Some(super::DrawCommand::Image { upload, .. }) = draw_data.commands.first() else {
                panic!("rotated image should emit an image command");
            };
            assert_eq!((upload.source_width, upload.source_height), (2, 3));
            assert_eq!(upload.pixels.as_ref(), source_pixels.as_slice());
        }
    }

    #[test]
    fn rotated_image_clip_selects_only_intersecting_source_tiles() {
        let mut image = ImageView::new(4097, 1, vec![255; 4097 * 4]).unwrap();
        image.set_rotation_quadrants(1);
        let mut canvas = Canvas::new(1.0, 4097.0);
        canvas.with_clip(Rect::new(0.0, 2020.0, 1.0, 20.0), |canvas| {
            canvas.image(&image, Rect::new(0.0, 0.0, 1.0, 4097.0));
        });
        let draw_data = canvas.finish();

        assert_eq!(
            draw_data
                .commands
                .iter()
                .filter(|command| matches!(command, super::DrawCommand::Image { .. }))
                .count(),
            1
        );
        assert_eq!(draw_data.image_vertices.len(), 6);
    }

    #[test]
    fn workspace_grid_paints_compositor_thumbnail_pixels() {
        let mut grid = WorkspaceGridView::new();
        grid.items = vec!["Space 1".into()];
        grid.set_rect(Rect::new(0.0, 0.0, 240.0, 160.0));
        let pixels = vec![10, 20, 30, 255, 40, 50, 60, 255];
        grid.set_thumbnails(vec![Some(ImageView::new(2, 1, pixels.clone()).unwrap())]);

        let mut canvas = Canvas::new(240.0, 160.0);
        super::draw_workspace_grid_view(&mut canvas, grid.rect(), &grid);
        let draw_data = canvas.finish();
        let image_commands = draw_data
            .commands
            .iter()
            .filter_map(|command| match command {
                super::DrawCommand::Image { upload, .. } => Some(upload),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(image_commands.len(), 1);
        assert_eq!(image_commands[0].pixels.as_ref(), pixels.as_slice());
    }

    #[test]
    fn workspace_grid_without_thumbnail_emits_no_image_command() {
        let mut grid = WorkspaceGridView::new();
        grid.items = vec!["Space 1".into()];
        grid.set_rect(Rect::new(0.0, 0.0, 240.0, 160.0));

        let mut canvas = Canvas::new(240.0, 160.0);
        super::draw_workspace_grid_view(&mut canvas, grid.rect(), &grid);
        let draw_data = canvas.finish();
        assert!(!draw_data
            .commands
            .iter()
            .any(|command| matches!(command, super::DrawCommand::Image { .. })));
    }

    #[test]
    fn parses_dark_appearance_preference() {
        assert!(parse_theme_preference("appearance=dark\n").0);
        assert!(parse_theme_preference("appearance=Dark\n").0);
    }

    #[test]
    fn ignores_non_dark_appearance_preferences() {
        assert!(!parse_theme_preference("appearance=light\n").0);
        assert!(!parse_theme_preference("appearance=system\n").0);
        assert!(!parse_theme_preference("other=dark\n").0);
    }

    #[test]
    fn parses_named_theme_grape() {
        let (is_dark, accent) = parse_theme_preference("theme=grape\n");
        assert!(is_dark);
        assert_eq!(accent, theme_accents::GRAPE);
    }

    #[test]
    fn parses_named_theme_strawberry() {
        let (is_dark, accent) = parse_theme_preference("theme=strawberry\n");
        assert!(!is_dark);
        assert_eq!(accent, theme_accents::STRAWBERRY);
    }

    #[test]
    fn theme_key_overrides_appearance_key() {
        let content = "appearance=dark\ntheme=classic\n";
        let (is_dark, accent) = parse_theme_preference(content);
        assert!(!is_dark);
        assert_eq!(accent, theme_accents::CLASSIC);
    }

    #[test]
    fn formats_menu_clock_with_minute_precision() {
        assert_eq!(format_clock_from_seconds(0), "12:00 AM");
        assert_eq!(format_clock_from_seconds(60), "12:01 AM");
        assert_eq!(format_clock_from_seconds(11 * 3600 + 59 * 60), "11:59 AM");
        assert_eq!(format_clock_from_seconds(12 * 3600), "12:00 PM");
        assert_eq!(format_clock_from_seconds(23 * 3600 + 5 * 60), "11:05 PM");
    }

    #[test]
    fn recognizes_sdk_owned_hide_and_quit_menu_actions() {
        assert_eq!(
            is_application_menu_action(
                "com.slopos.finder.finder.quit_finder",
                "com.slopos.finder",
                "Finder",
            ),
            Some(ApplicationMenuAction::Quit)
        );
        assert_eq!(
            is_application_menu_action(
                "com.slopos.finder.finder.hide_finder",
                "com.slopos.finder",
                "Finder",
            ),
            Some(ApplicationMenuAction::Hide)
        );
        assert_eq!(
            is_application_menu_action(
                "com.slopos.finder.file.new_folder",
                "com.slopos.finder",
                "Finder",
            ),
            None
        );
    }

    #[test]
    fn publishes_menu_manifest_bytes_as_one_complete_file() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before Unix epoch")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("slopos-sdk-manifest-{unique}"));
        fs::create_dir_all(&directory).expect("create manifest test directory");
        let path = directory.join("com.slopos.test.json");

        publish_bytes_atomically(&path, br#"{"menus":[{"title":"File"}]}"#)
            .expect("atomically publish manifest");
        assert_eq!(
            fs::read_to_string(&path).expect("read published manifest"),
            r#"{"menus":[{"title":"File"}]}"#
        );
        assert_eq!(
            fs::read_dir(&directory)
                .expect("read manifest directory")
                .count(),
            1,
            "temporary manifest must be renamed away"
        );

        fs::remove_dir_all(directory).expect("remove manifest test directory");
    }

    #[test]
    fn canvas_measurement_uses_the_shared_shaped_layout() {
        let canvas = Canvas::new(320.0, 100.0);
        let text = "Aé e\u{301} fi 日本語";
        let expected =
            shape_text(text, TextLayoutOptions::new(13.0, canvas.pixel_scale())).first_line_width();

        assert!(
            (canvas.measure_text(text) - expected).abs() < 0.01,
            "Canvas measurement {} differs from shaped width {}",
            canvas.measure_text(text),
            expected
        );
    }

    #[test]
    fn menu_and_status_advances_use_shaped_canvas_widths() {
        let canvas = Canvas::new(320.0, 100.0);
        let text = "Wiii 日本語";
        let measured = canvas.measure_text(text);

        assert!((status_item_advance(&canvas, text, 0.0) - (measured + 12.0)).abs() < 0.01);
        assert_eq!(
            status_item_advance(&canvas, text, measured + 20.0),
            measured + 20.0
        );
        assert!((menu_button_advance(&canvas, text) - (measured + 18.0)).abs() < 0.01);
    }

    #[test]
    fn inactive_title_palette_is_readable_in_light_and_dark_modes() {
        assert_eq!(
            inactive_title_color(false),
            CLASSIC_DARK_GRAY_RGBA,
            "Classic inactive titles use the semantic dark-gray role"
        );
        assert_eq!(inactive_title_color(true), COLOR_DARK_TITLE_INACTIVE);
    }

    #[test]
    fn desktop_nameplate_fits_applications_at_one_x() {
        let canvas = Canvas::new(320.0, 100.0);
        assert_eq!(
            canvas.ellipsize_text("Applications", DESKTOP_ITEM_WIDTH),
            "Applications"
        );
    }

    #[test]
    fn canvas_text_does_not_expand_raster_coverage_into_colored_primitives() {
        let text = "fi";
        let layout = shape_text(text, TextLayoutOptions::new(13.0, 1.0));
        let expected_pixels = layout
            .glyphs()
            .iter()
            .filter_map(|glyph| glyph.raster())
            .flat_map(|glyph| glyph.data.iter())
            .filter(|coverage| **coverage as f32 / 255.0 > 0.05)
            .count();
        let mut canvas = Canvas::new(320.0, 100.0);

        canvas.text(text, 10.0, 10.0, [0.0, 0.0, 0.0, 1.0]);

        assert_eq!(
            canvas.vertices.len(),
            0,
            "rasterized text should use the glyph stream, not one colored quad per covered pixel ({} old-path pixels)",
            expected_pixels
        );
    }

    #[test]
    fn canvas_reuses_atlas_entries_and_batches_adjacent_glyphs() {
        let mut canvas = Canvas::new(320.0, 100.0);
        let _ = canvas.glyph('A', 10.0, 10.0, [0.0, 0.0, 0.0, 1.0]);
        let _ = canvas.glyph('A', 10.0, 10.0, [0.0, 0.0, 0.0, 1.0]);

        assert_eq!(canvas.glyph_atlas_entry_count(), 1);
        assert_eq!(canvas.glyph_batch_count(), 1);
        assert_eq!(canvas.glyph_vertices.len(), 12);
    }

    #[test]
    fn canvas_keeps_glyphs_in_order_with_colored_primitives() {
        let mut canvas = Canvas::new(320.0, 100.0);
        canvas.rect(super::Rect::new(0.0, 0.0, 8.0, 8.0), [1.0, 0.0, 0.0, 1.0]);
        let _ = canvas.glyph('A', 10.0, 10.0, [0.0, 0.0, 0.0, 1.0]);
        canvas.rect(super::Rect::new(24.0, 0.0, 8.0, 8.0), [0.0, 1.0, 0.0, 1.0]);

        assert!(matches!(
            canvas.commands[0],
            super::DrawCommand::Color { .. }
        ));
        assert!(matches!(
            canvas.commands[1],
            super::DrawCommand::Glyph { .. }
        ));
        assert!(matches!(
            canvas.commands[2],
            super::DrawCommand::Color { .. }
        ));
    }

    fn synthetic_raster(value: u8, bearing_x: f32) -> slopos_render::font::RasterGlyph {
        slopos_render::font::RasterGlyph {
            data: vec![value],
            width: 1,
            height: 1,
            advance: 1.0,
            bearing_x,
            bearing_y: 0.0,
            top: 0.0,
            ascent: 1.0,
            descent: 0.0,
        }
    }

    #[test]
    fn glyph_atlas_overflow_does_not_emit_per_pixel_color_fallbacks() {
        let mut canvas = Canvas::new(320.0, 100.0);
        for index in 0..=super::GLYPH_ATLAS_MAX_ENTRIES + 1 {
            let raster = synthetic_raster(200, index as f32 / 1024.0);
            canvas.draw_test_raster_glyph(&raster);
        }

        assert_eq!(
            canvas
                .commands
                .iter()
                .filter(|command| matches!(command, super::DrawCommand::Color { .. }))
                .count(),
            0,
            "atlas overflow must remain on the retained glyph path"
        );
    }

    #[test]
    fn glyph_atlas_page_growth_preserves_earlier_glyph_data_in_one_frame() {
        let mut canvas = Canvas::new(320.0, 100.0);
        let first = synthetic_raster(0x7f, 0.0);
        let first_key = super::GlyphAtlasKey::from_raster(&first, 1.0).expect("first key");
        canvas.draw_test_raster_glyph(&first);

        for index in 1..=super::GLYPH_ATLAS_MAX_ENTRIES + 1 {
            let raster = synthetic_raster(200, index as f32 / 1024.0);
            canvas.draw_test_raster_glyph(&raster);
        }

        let first_region = match &canvas.atlas {
            super::CanvasAtlas::Owned(atlas) => atlas
                .entries
                .get(&first_key)
                .copied()
                .expect("first glyph remains indexed"),
            super::CanvasAtlas::Borrowed(_) => panic!("test canvas owns its atlas"),
        };
        assert_eq!(
            match &canvas.atlas {
                super::CanvasAtlas::Owned(atlas) =>
                    atlas.pages[first_region.page as usize].pixels
                        [(first_region.y * super::GLYPH_ATLAS_WIDTH + first_region.x) as usize],
                super::CanvasAtlas::Borrowed(_) => 0,
            },
            0x7f
        );
        assert_eq!(
            canvas
                .commands
                .iter()
                .filter(|command| matches!(command, super::DrawCommand::Color { .. }))
                .count(),
            0,
            "page growth must not invalidate earlier glyphs into pixel quads"
        );
    }
}
