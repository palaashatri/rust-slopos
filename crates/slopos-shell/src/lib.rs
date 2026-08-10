#![allow(
    dead_code,
    unused_imports,
    clippy::manual_clamp,
    clippy::io_other_error,
    clippy::if_same_then_else
)]

pub mod a11y_actions;
pub mod a11y_prefs;
pub mod application_registry;
pub mod atspi_bus;
pub mod audio;
pub mod bundle;
pub mod capture;
pub mod chrome_protocol;
pub mod desktop_manager;
pub mod display_arrange;
pub mod display_settings;
pub mod dock;
pub mod fdo_notifications;
pub mod foreign_toplevel;
pub mod foreign_toplevel_client;
pub mod i18n;
pub mod idle_policy;
pub mod keyboard_nav;
pub mod launch_services;
pub mod layer_desktop;
pub mod layer_shell_client;
pub mod menu_server;
pub mod mime_open;
pub mod network_connect;
pub mod network_manager;
pub mod notification_center;
pub mod polkit_agent;
pub mod portal;
pub mod portal_dbus;
pub mod portal_extra;
pub mod power;
pub mod screencast_pw;
pub mod session_actions;
pub mod session_clients;
pub mod session_manager;
pub mod session_packaging;
pub mod session_recovery;
pub mod shell_scale;
pub mod spotlight;
pub mod spotlight_ui;
pub mod startup_budget;
pub mod theme_manager;
pub mod window_manager;
pub mod window_rules;
pub mod workspace_manager;

pub use a11y_actions::{
    a11y_invoke_is_live, actions_for_chrome, chrome_target_for_atspi_path, classify_a11y_invoke,
    invoke_id_for_object_name, plan_invoke, primary_invoke_for_chrome, resolve_pending_invoke,
    session_action_for_invoke, session_root_actions, summarize_actions, A11yDispatchTarget,
    AccessibleAction, ActionInterfaceSummary, InvokePlan,
};
pub use a11y_prefs::{
    apply_a11y_prefs_to_theme_name, effective_animation_ms, A11yPrefs, ContrastPreference,
    MotionPreference,
};
pub use application_registry::ApplicationRegistry;
pub use atspi_bus::{
    atspi_dbus_connection_available, chrome_focus_atspi_path, drain_in_process_events,
    emit_chrome_focus, in_process_event_count, serialize_chrome_focus_for_dbus,
    try_emit_accessible_event, EmitAccessibleResult,
};
pub use audio::{
    get_volume, set_volume, volume_pactl_get_plan, volume_pactl_set_plan, volume_status_label,
    volume_wpctl_get_plan, volume_wpctl_set_plan, AudioBackend, AudioError,
};
pub use capture::{start_recording, stop_recording, take_screenshot};
pub use chrome_protocol::{
    chrome_focus_order, next_chrome_focus, session_output_size, should_paint_kit_chrome,
    ChromeFocusTarget, ChromeRole, ChromeSession, ProtocolChromeSurface,
};
pub use desktop_manager::DesktopManager;
pub use display_arrange::{
    apply_display_plan_env, arrange_mode_from_env_value, arrangement_bounds, normalize_arrangement,
    place_outputs, plan_display_apply, ArrangeMode, DisplayApplyPlan, DisplayApplyStep,
    DisplayArrangement, DisplayOutput, PlacedOutput,
};
pub use display_settings::DisplayConfig;
pub use dock::Dock;
pub use fdo_notifications::{
    try_register_session_bus as try_register_fdo_notifications, NotificationDaemon,
    NotificationPayload, NotificationServerState, NotifySendStyle, ServerInformation, Urgency,
    FDO_NOTIFICATIONS_BUS_NAME, FDO_NOTIFICATIONS_INTERFACE, FDO_NOTIFICATIONS_PATH,
};
pub use foreign_toplevel::{
    apply_toplevel_force_quit, parse_toplevel_force_quit, ForeignToplevelEntry,
    ForeignToplevelRegistry, ToplevelForceQuit,
};
pub use foreign_toplevel_client::{
    apply_foreign_toplevel_list_event, apply_foreign_toplevel_list_events,
    try_sync_foreign_toplevels, ForeignToplevelListEvent,
};
pub use i18n::{
    format_message, is_rtl_language, text_direction_for_locale, tr, LocaleId, LocalePrefs,
    MessageCatalog, TextDirection,
};
pub use idle_policy::{
    idle_phase, recommended_action, secs_until_next_phase, IdleConfig, IdleInhibitState, IdlePhase,
    IdleRecommendedAction, InhibitReason,
};
pub use keyboard_nav::{
    apply_chrome_nav, is_dismissable_window_title, keyboard_nav_intent, KeyboardNavIntent,
};
pub use launch_services::LaunchServices;
pub use layer_shell_client::{
    chrome_to_layer_shell_requests, layer_shell_bind_summary, try_map_layer_shell_chrome,
    LayerShellBindResult, LayerShellChromeRequest,
};
pub use menu_server::{
    battery_status_label, network_status_label, MenuServer, StatusItem,
    STATUS_REFRESH_INTERVAL_SECS,
};
pub use mime_open::{
    first_party_binary_for_app_id, mime_from_path, open_plan, open_plan_for_file_uri,
    parse_desktop_exec, path_from_file_uri, seed_slopos_defaults, spawn_argv, DesktopAppEntry,
    MimeOpenRegistry, OpenPlan,
};
pub use network_connect::{
    connect_wifi, describe_nm_connect_plan, execute_nm_connect_plan, nm_connect_plan,
    nm_connect_plan_validated, validate_nm_connect_request, NmConnectRequest,
};
pub use network_manager::{get_network_status, NetworkStatus};
pub use notification_center::{NotificationCenter, NotificationPriority};
pub use polkit_agent::{
    handle_polkit_auth, try_register_polkit_agent, validate_polkit_request, PolkitAgentState,
    PolkitAuthDecision, PolkitAuthRequest, POLKIT_AGENT_BUS_NAME, POLKIT_AGENT_INTERFACE,
    POLKIT_AGENT_PATH,
};
pub use portal::{
    apply_screencast_readiness, create_screencast_session,
    create_screencast_session_with_backend_note, handle_file_chooser_open,
    handle_file_chooser_save, handle_open_uri, handle_portal_screenshot_request, plan_open_uri,
    portal_screenshot_filename, portal_screenshot_uri_for, portal_screenshots_dir,
    read_all_portal_settings, read_portal_setting, screencast_backend_note,
    screencast_backend_note_from_socket, select_screencast_sources, start_screencast,
    start_screencast_with_readiness, take_portal_style_screenshot,
    take_portal_style_screenshot_with, validate_file_chooser_request, OpenUriAction,
    PortalFileChooserRequest, PortalFileChooserResult, PortalScreencastRequest,
    PortalScreencastSession, PortalScreenshotRequest, PortalScreenshotResult,
    PortalSettingsNamespace, ScreencastStartOutcome, ScreencastStream, PORTAL_BUS_NAME,
    PORTAL_FILECHOOSER_INTERFACE, PORTAL_OPENURI_INTERFACE, PORTAL_PATH,
    PORTAL_SCREENCAST_INTERFACE, PORTAL_SCREENSHOT_INTERFACE, PORTAL_SETTINGS_INTERFACE,
    SCREENCAST_DEFAULT_HEIGHT, SCREENCAST_DEFAULT_WIDTH, SCREENCAST_NOTE_PIPEWIRE_SOCKET,
    SCREENCAST_NOTE_PORTAL_STUB, SCREENCAST_PLACEHOLDER_NODE_ID, SCREENCAST_SOURCE_TYPE_MONITOR,
    SCREENCAST_SOURCE_TYPE_WINDOW,
};
pub use portal_dbus::try_register_portal_session_bus;
pub use portal_extra::{
    active_idle_inhibit_state, active_inhibits, clear_inhibit_store_for_tests, handle_inhibit,
    handle_inhibit_and_register, handle_print_request, handle_secret_retrieve, inhibit_blocks_idle,
    inhibit_to_idle_reason, portal_blocks_idle, register_inhibit_cookie, release_inhibit_cookie,
    InhibitFlag, PortalInhibitCookie, PortalInhibitRequest, PortalPrintRequest, PortalPrintResult,
    PortalSecretRequest, PortalSecretResult,
};
pub use power::{battery_info, BatteryInfo};
pub use screencast_pw::{
    can_claim_live_streams, default_pipewire_socket, plan_list_pipewire_nodes,
    probe_screencast_readiness, probe_screencast_readiness_host, source_ids_for_portal,
    sources_from_outputs, sources_from_windows, PwListNodesPlan, ScreencastBackend,
    ScreencastReadiness, ScreencastSource, ScreencastSourceType,
};
pub use session_actions::{
    confirm_prompt, confirm_prompt_i18n_key, describe_plan, plan_requires_privileges,
    plan_session_action, plan_session_action_with, requires_confirmation, shell_delta_for_plan,
    PowerBackend, SessionAction, SessionActionPlan, ShellSessionDelta, LOGIND_BUS,
    LOGIND_MANAGER_IFACE, LOGIND_PATH,
};
pub use session_clients::{
    binary_name_for_bundle, parse_force_quit_entry, resolve_app_binary, spawn_app_client,
    spawn_open_plan, ForceQuitTarget, SessionClientRegistry,
};
pub use session_manager::SessionManager;
pub use session_packaging::{
    check_greeter_session_readiness, check_packaging_health, parse_desktop_keys,
    session_entry_smoke_report, validate_session_desktop, GreeterSessionReadiness, PackagingHealth,
    SessionEntrySmokeReport, SessionPackagingLayout,
};
pub use session_recovery::{
    recovery_plan, should_attempt_recovery, CheckpointClient, RecoveryStep, SessionCheckpoint,
};
pub use shell_scale::{
    detect_shell_scale_from_env, parse_shell_scale, scale_layout_dim, scaled_chrome_insets,
    ShellScale,
};
pub use spotlight::{SearchBackend, SearchResult, Spotlight, SpotlightState};
pub use spotlight_ui::SpotlightUI;
pub use startup_budget::{
    default_desktop_budget, overall_ok, record_phase, total_elapsed_ms, PhaseResult, StartupBudget,
    StartupPhase,
};
pub use theme_manager::ThemeManager;
pub use window_manager::WindowManager;
pub use window_rules::{
    default_session_rules, evaluate_rules, field_matches, parse_rules_simple, rule_matches,
    MatchField, MatchKind, WindowInfo, WindowMatch, WindowRule, WindowRuleActions,
};
pub use workspace_manager::{WorkspaceManager, COMPOSITOR_WORKSPACE_COUNT, SHELL_DESKTOP_COUNT};

use image::{ImageFormat, ImageReader, Limits};
use parking_lot::RwLock;
use slopos_bus::{
    read_space_thumbnail_manifest, read_spaces_snapshot, send_application_menu_action,
    send_session_control, session_space_thumbnail_path, SessionControlRequest, SpaceTargetWire,
    SpacesControlCommand, WindowPresentationAction, MAX_SPACE_THUMBNAIL_HEIGHT,
    MAX_SPACE_THUMBNAIL_WIDTH,
};
use slopos_kit::button::Button;
use slopos_kit::design_tokens::{MENU_BAR_HEIGHT, MENU_BAR_HEIGHT_PX, WINDOW_TITLE_BAR_HEIGHT};
use slopos_kit::dispatch::{for_each_widget_mut, hit_test};
use slopos_kit::event::MouseButton;
use slopos_kit::icon_view::{IconItem, IconView, IconViewLayoutMode};
use slopos_kit::label::Label;
use slopos_kit::layout::LayoutView;
use slopos_kit::list_view::ListView;
use slopos_kit::menu::{Menu, MenuItemKind};
use slopos_kit::menu_bar::MenuBar;
use slopos_kit::text_field::TextField;
use slopos_kit::theme::ThemeContext;
use slopos_kit::window::Window;
use slopos_kit::workspace_grid_view::WorkspaceGridView;
use slopos_kit::PointerDispatcher;
use slopos_kit::{
    AccessibilityNode, AccessibilityTree, DockView, Event, EventResult, ImageView, Layout,
    LayoutConstraint, Point, Rect, Size, Widget, WidgetState,
};
use std::fs::{self, OpenOptions};
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

pub type Result<T> = std::result::Result<T, ShellError>;

#[derive(Debug, thiserror::Error)]
pub enum ShellError {
    #[error("service error: {0}")]
    Service(String),
    #[error("window error: {0}")]
    Window(String),
    #[error("launch error: {0}")]
    Launch(String),
    #[error("theme error: {0}")]
    Theme(String),
    #[error("menu error: {0}")]
    Menu(String),
}

const MAX_SPACE_THUMBNAIL_BYTES: u64 = 8 * 1024 * 1024;

/// Decode one compositor-owned PNG only when it is a bounded regular file.
///
/// The compositor is the sole producer. A missing, malformed, oversized, or
/// symlinked file is treated as unavailable, so the shell never substitutes a
/// fabricated window image in the overview.
fn load_space_thumbnail(id: u64) -> Option<ImageView> {
    let path = session_space_thumbnail_path(id)?;
    load_space_thumbnail_path(&path)
}

fn load_space_thumbnail_path(path: &Path) -> Option<ImageView> {
    let bytes = read_bounded_thumbnail_file(path)?;
    let mut reader = ImageReader::with_format(Cursor::new(bytes), ImageFormat::Png);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SPACE_THUMBNAIL_WIDTH);
    limits.max_image_height = Some(MAX_SPACE_THUMBNAIL_HEIGHT);
    limits.max_alloc = Some(MAX_SPACE_THUMBNAIL_BYTES);
    reader.limits(limits);
    let decoded = reader.decode().ok()?;
    let rgba = decoded.to_rgba8();
    let (width, height) = rgba.dimensions();
    if width == 0
        || height == 0
        || width > MAX_SPACE_THUMBNAIL_WIDTH
        || height > MAX_SPACE_THUMBNAIL_HEIGHT
    {
        return None;
    }
    ImageView::new(width, height, rgba.into_raw()).ok()
}

#[cfg(unix)]
fn read_bounded_thumbnail_file(path: &Path) -> Option<Vec<u8>> {
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .ok()?;
    let before = file.metadata().ok()?;
    if !before.file_type().is_file()
        || before.len() == 0
        || before.len() > MAX_SPACE_THUMBNAIL_BYTES
    {
        return None;
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(MAX_SPACE_THUMBNAIL_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .ok()?;
    let after = file.metadata().ok()?;
    if before.len() != after.len() || bytes.len() as u64 != after.len() {
        return None;
    }
    Some(bytes)
}

#[cfg(not(unix))]
fn read_bounded_thumbnail_file(path: &Path) -> Option<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if metadata.file_type().is_symlink()
        || !metadata.file_type().is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_SPACE_THUMBNAIL_BYTES
    {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    (bytes.len() as u64 == metadata.len()).then_some(bytes)
}

fn load_space_thumbnails(
    ids: &[u64],
    expected_session_epoch: u64,
    expected_revision: u64,
) -> Vec<Option<ImageView>> {
    let Ok(manifest) = read_space_thumbnail_manifest() else {
        return ids.iter().map(|_| None).collect();
    };
    if manifest.session_epoch != expected_session_epoch || manifest.generation != expected_revision
    {
        return ids.iter().map(|_| None).collect();
    }
    ids.iter()
        .copied()
        .map(|id| {
            let entry = manifest
                .captures
                .iter()
                .find(|entry| entry.space_id == id)?;
            let image = load_space_thumbnail(id)?;
            (image.width() == entry.width && image.height() == entry.height).then_some(image)
        })
        .collect()
}

pub struct SloposI {
    pub menu_server: Arc<RwLock<MenuServer>>,
    pub window_manager: Arc<RwLock<WindowManager>>,
    pub desktop_manager: Arc<RwLock<DesktopManager>>,
    pub dock: Arc<RwLock<Dock>>,
    pub notification_center: Arc<RwLock<NotificationCenter>>,
    pub workspace_manager: Arc<RwLock<WorkspaceManager>>,
    pub launch_services: Arc<RwLock<LaunchServices>>,
    pub session_manager: Arc<RwLock<SessionManager>>,
    pub theme_manager: Arc<RwLock<ThemeManager>>,
    pub application_registry: Arc<RwLock<ApplicationRegistry>>,
}

impl Default for SloposI {
    fn default() -> Self {
        Self::new()
    }
}

impl SloposI {
    pub fn new() -> Self {
        Self {
            menu_server: Arc::new(RwLock::new(MenuServer::new())),
            window_manager: Arc::new(RwLock::new(WindowManager::new())),
            desktop_manager: Arc::new(RwLock::new(DesktopManager::new())),
            dock: Arc::new(RwLock::new(Dock::new())),
            notification_center: Arc::new(RwLock::new(NotificationCenter::new())),
            workspace_manager: Arc::new(RwLock::new(WorkspaceManager::new())),
            launch_services: Arc::new(RwLock::new(LaunchServices::new())),
            session_manager: Arc::new(RwLock::new(SessionManager::new())),
            theme_manager: Arc::new(RwLock::new(ThemeManager::new())),
            application_registry: Arc::new(RwLock::new(ApplicationRegistry::new())),
        }
    }

    pub fn theme_context(&self) -> ThemeContext {
        self.theme_manager.read().current_context()
    }

    pub fn startup() -> Result<Self> {
        let shell = Self::new();
        shell.launch_services.write().scan_applications();
        {
            let mut tm = shell.theme_manager.write();
            tm.load_default();
            // Load named theme + a11y prefs (high_contrast / reduced_motion) from settings.conf.
            tm.load_theme_from_settings();
            if let Ok(t) = std::env::var("SLOPOS_THEME") {
                tm.set_theme(&t);
            } else if std::env::var_os("SLOPOS_DARK_MODE").is_some() {
                tm.set_theme("dark");
            }
        }
        // Locale from LANG + optional settings.conf; drive system menu chrome strings.
        {
            let conf_text = read_settings_conf_text();
            let prefs = if conf_text.is_empty() {
                LocalePrefs::parse_from_env_lang(std::env::var("LANG").ok().as_deref())
            } else {
                let mut p = LocalePrefs::parse_from_conf(&conf_text);
                // Env LANG still wins when conf has no locale key.
                if p.locale.language == "en"
                    && p.locale.region.as_deref() == Some("US")
                    && !conf_text.lines().any(|l| {
                        let t = l.trim();
                        t.starts_with("locale=")
                            || t.starts_with("lang=")
                            || t.starts_with("language=")
                    })
                {
                    p = LocalePrefs::parse_from_env_lang(std::env::var("LANG").ok().as_deref());
                }
                p
            };
            shell.menu_server.write().apply_locale_labels(&prefs);
            tracing::info!(locale = %prefs.locale.tag(), "shell menu locale applied");
        }
        // Multi-monitor arrange: plan + live EmitLayoutEnv (SLOPOS_OUTPUTS_LAYOUT).
        apply_display_config_from_settings();
        // Best-effort FreeDesktop Notifications on the session bus (Linux).
        // Failure is non-fatal: pure NotificationCenter still works in-process.
        let _ = fdo_notifications::try_register_session_bus(shell.notification_center.clone());
        // Best-effort portal Screenshot/Settings/OpenURI on the session bus (Linux).
        let _ = portal_dbus::try_register_portal_session_bus();
        // Best-effort polkit authentication agent (Linux).
        let _ = polkit_agent::try_register_polkit_agent();
        Ok(shell)
    }

    /// Bind protocol session chrome (layer-shell) and sync foreign-toplevel list.
    ///
    /// Call after `WAYLAND_DISPLAY` is set (compositor/labwc running). Non-fatal.
    pub(crate) fn attach_wayland_session_protocols(desktop: &mut ShellDesktop) {
        // Linux session chrome is compositor-owned by construction. The old
        // opt-in environment gate let production silently fall back to a
        // 640x480 ordinary XDG desktop window, which violated the session
        // topology and made the menu bar local to that fake window.
        #[cfg(target_os = "linux")]
        {
            tracing::debug!(
                "layer_desktop will bind exclusive chrome surfaces after protocol discovery"
            );
        }
        #[cfg(not(target_os = "linux"))]
        tracing::debug!("layer-shell chrome unavailable on this host");
        if let Some(n) =
            foreign_toplevel_client::try_sync_foreign_toplevels(&mut desktop.foreign_toplevels)
        {
            tracing::info!(toplevels = n, "shell synced foreign-toplevel-list");
            desktop.foreign_toplevel_synced = true;
        }
    }

    pub fn run(&self) -> Result<()> {
        let (out_w, out_h) = session_output_size();
        run_platform(self, out_w, out_h)
    }
}

#[cfg(target_os = "linux")]
fn run_platform(shell: &SloposI, out_w: i32, out_h: i32) -> Result<()> {
    // Linux production uses the real compositor-owned shell surfaces. The
    // desktop background, global menu, dock, and menu overlays are separate
    // layer-shell surfaces spanning the compositor output; ordinary apps
    // remain independent XDG toplevel clients underneath/above them.
    let content = Box::new(ShellDesktop::new(
        shell.menu_server.clone(),
        shell.launch_services.clone(),
        shell.window_manager.clone(),
        shell.notification_center.clone(),
        shell.workspace_manager.clone(),
        shell.dock.clone(),
        shell.session_manager.clone(),
    ));
    crate::layer_desktop::run_layer_desktop(content, out_w as u32, out_h as u32)
        .map_err(|e| ShellError::Window(format!("layer-shell desktop: {}", e)))
}

#[cfg(not(target_os = "linux"))]
fn run_platform(shell: &SloposI, out_w: i32, out_h: i32) -> Result<()> {
    // Non-Linux development fallback: use the default winit xdg-toplevel path.
    let mut app = slopos_sdk::Application::new("SLOPOS-I", "com.slopos.shell");
    app.set_initial_size(Size::new(out_w as f32, out_h as f32));

    let desktop_view = ShellDesktop::new(
        shell.menu_server.clone(),
        shell.launch_services.clone(),
        shell.window_manager.clone(),
        shell.notification_center.clone(),
        shell.workspace_manager.clone(),
        shell.dock.clone(),
        shell.session_manager.clone(),
    );

    let mut window = Window::new("SLOPOS-I Desktop");
    window.set_content(Box::new(desktop_view));
    app.set_main_window(window);
    app.run();
    Ok(())
}

struct ShellDesktop {
    state: WidgetState,
    menu_bar: MenuBar,
    desktop: IconView,
    windows: Vec<ShellWindow>,
    window_interaction: Option<WindowInteraction>,
    /// Generic pointer routing (implicit capture + hover synthesis) over the
    /// shell's widget tree. Window-manager geometry — z-order picking,
    /// titlebar chrome, drag interactions — runs before it; everything that
    /// is a widget is dispatched through this.
    pointer: PointerDispatcher,
    menu_server: Arc<RwLock<MenuServer>>,
    launch_services: Arc<RwLock<LaunchServices>>,
    window_manager: Arc<RwLock<WindowManager>>,
    notification_center: Arc<RwLock<NotificationCenter>>,
    workspace_manager: Arc<RwLock<WorkspaceManager>>,
    dock: Arc<RwLock<Dock>>,
    session_manager: Arc<RwLock<SessionManager>>,
    dock_view: DockView,
    bundle_ids: Vec<String>,
    /// Notification banner pop-up windows, rebuilt each update() from visible notifications.
    notification_popup_windows: Vec<Window>,
    /// Last application-launch error, if any. Set by `launch_external_app` on failure.
    /// Intended for display in the status bar (rendering integration pending).
    last_error: Option<String>,
    /// Whether the screen is currently locked.
    locked: bool,
    /// Lock screen overlay widget, shown when `locked` is true.
    lock_screen_widget: Window,
    /// Password field for the lock screen.
    lock_password_field: TextField,
    /// Error message to display on lock screen (e.g., "Incorrect password").
    lock_error_message: Option<String>,
    /// The expected lock password (from env or config).
    expected_lock_password: Option<String>,
    /// Independent first-party app processes (compositor/labwc clients).
    session_clients: SessionClientRegistry,
    /// Protocol-backed session chrome (layer-shell menu bar / dock roles).
    chrome: ChromeSession,
    /// Foreign-toplevel registry for task list / Force Quit (compositor-synced when possible).
    foreign_toplevels: ForeignToplevelRegistry,
    /// True after a successful `zwlr_layer_shell_v1` chrome bind.
    layer_shell_bound: bool,
    /// True after a successful `ext_foreign_toplevel_list_v1` sync.
    foreign_toplevel_synced: bool,
    /// Keyboard-only chrome focus region (Tab cycle).
    chrome_focus: ChromeFocusTarget,
    /// Monotonic instant of last user input (for idle auto-lock).
    last_input_at: std::time::Instant,
    /// Idle lock/suspend policy (from defaults / settings.conf).
    idle_config: IdleConfig,
    /// Portal / media idle inhibit tokens.
    idle_inhibit: IdleInhibitState,
    /// MIME open registry (seeded with SLOPOS-I defaults).
    mime_registry: MimeOpenRegistry,
    /// When false, file open records [`Self::last_mime_open`] only (unit tests).
    mime_open_spawn: bool,
    /// Last MIME open plan produced by folder double-click / open path (tests + status).
    last_mime_open: Option<OpenPlan>,
    /// Last menu-bar status refresh (battery / volume / network).
    last_status_refresh: std::time::Instant,
    /// Last network connect attempt outcome (tests + status UI).
    last_network_connect: Option<std::result::Result<String, String>>,
    /// When false, network connect validates/plans only (unit tests; no nmcli spawn).
    network_connect_spawn: bool,
    /// Destructive session action awaiting a second explicit menu activation.
    pending_session_confirmation: Option<session_actions::SessionAction>,
    /// Which subset of the desktop to paint (layer-shell Phase 3 multi-surface).
    paint_filter: ShellPaintFilter,
    /// Temporary event-routing target for a layer-shell input surface. This is
    /// separate from `paint_filter`: protocol chrome must receive events from
    /// its own surface without being painted into the background surface.
    input_filter: Option<ShellPaintFilter>,
    /// Spotlight search overlay (Super+Space) — interior-mutable for layout during draw.
    spotlight_ui: spotlight_ui::SpotlightUI,
    /// Whether at least one compositor snapshot has been accepted.  This
    /// distinguishes an initial revision `0` snapshot from a duplicate/stale
    /// revision after a local compatibility switch.
    spaces_snapshot_initialized: bool,
    /// Shell-owned Spaces overview shown on the live Background layer.
    ///
    /// Ordinary application windows remain compositor-owned and are never
    /// placed here.  The legacy non-layer path keeps its overview in
    /// `windows` so existing in-process tests and indexed controls continue
    /// to exercise the old shell window policy.
    workspace_overview: Option<Window>,
}

/// Paint subset for multi-surface layer-shell chrome (Phase 3).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum ShellPaintFilter {
    /// Full desktop including kit menu/dock (winit path).
    #[default]
    All,
    /// Wallpaper + icons + in-shell windows (no menu/dock).
    Background,
    /// Menu bar only (Top exclusive surface).
    MenuBar,
    /// Open menu dropdown only (Overlay surface, origin-local).
    MenuPopup,
    /// Live SLOPOS Spaces overview (Overlay surface, origin-local).
    SpacesOverview,
    /// Dock only (Bottom exclusive surface).
    Dock,
}

impl ShellDesktop {
    /// Select which chrome/content subset the next `draw` emits.
    pub(crate) fn set_paint_filter(&mut self, filter: ShellPaintFilter) {
        self.paint_filter = filter;
    }

    /// Mark the compositor-owned layer-shell chrome as live. This is called by
    /// the layer driver only after it has discovered and configured the
    /// protocol, not while constructing the shell model or in unit tests.
    pub(crate) fn set_layer_shell_bound(&mut self, bound: bool) {
        self.layer_shell_bound = bound;
        if bound {
            tracing::info!(
                "compositor owns ordinary application windows; shell background is chrome-only"
            );
        }
    }

    /// Route the next input dispatch through the widget set belonging to a
    /// particular layer surface. Painting remains governed by `paint_filter`.
    pub(crate) fn set_input_filter(&mut self, filter: Option<ShellPaintFilter>) {
        self.input_filter = filter;
    }

    /// Lay out the dock at the origin for painting onto a dock-height strip surface.
    pub(crate) fn prepare_dock_strip_layout(&mut self, width: f32, dock_h: f32) {
        self.dock_view.set_rect(Rect::new(0.0, 0.0, width, dock_h));
        let _ = self
            .dock_view
            .layout(LayoutConstraint::tight(Size::new(width, dock_h)));
    }

    /// Lay out the menu bar at the origin for a menu-height strip surface.
    pub(crate) fn prepare_menu_strip_layout(&mut self, width: f32, menu_h: f32) {
        self.menu_bar.set_rect(Rect::new(0.0, 0.0, width, menu_h));
        let _ = self
            .menu_bar
            .layout(LayoutConstraint::tight(Size::new(width, menu_h)));
    }

    /// Pixel height the Top menu layer must cover: bar only, or bar + open dropdown.
    pub(crate) fn menu_layer_height_px(&self) -> u32 {
        const BAR: u32 = MENU_BAR_HEIGHT_PX;
        let Some(idx) = self.menu_bar.open_menu else {
            return BAR;
        };
        let Some(dropdown) = self.menu_bar.dropdown_rect(idx) else {
            return BAR;
        };
        let bottom = (dropdown.y + dropdown.height).ceil().max(0.0) as u32;
        bottom.max(BAR)
    }

    /// Open dropdown geometry in output pixels: `(x, y, w, h)`, if a menu is open.
    pub(crate) fn open_menu_dropdown_geo(&self) -> Option<(i32, i32, u32, u32)> {
        let idx = self.menu_bar.open_menu?;
        let dd = self.menu_bar.dropdown_rect(idx)?;
        Some((
            dd.x.floor() as i32,
            dd.y.floor() as i32,
            dd.width.ceil().max(1.0) as u32,
            dd.height.ceil().max(1.0) as u32,
        ))
    }

    pub(crate) fn set_menu_popup_origin(&mut self, enabled: bool) {
        self.menu_bar.layer_popup_origin = enabled;
    }

    pub(crate) fn set_suppress_dropdown_paint(&mut self, enabled: bool) {
        self.menu_bar.suppress_dropdown_paint = enabled;
    }

    /// Return the live overview geometry in output coordinates.
    pub(crate) fn workspace_overview_geo(&self) -> Option<(i32, i32, u32, u32)> {
        let rect = self.workspace_overview.as_ref()?.rect();
        Some((
            rect.x.floor() as i32,
            rect.y.floor() as i32,
            rect.width.ceil().max(1.0) as u32,
            rect.height.ceil().max(1.0) as u32,
        ))
    }

    /// Lay out the live overview at the local origin of its overlay surface.
    pub(crate) fn prepare_workspace_overview_layout(&mut self, width: f32, height: f32) {
        let Some(window) = self.workspace_overview.as_mut() else {
            return;
        };
        window.set_rect(Rect::new(0.0, 0.0, width, height));
        let _ = window.layout(LayoutConstraint::tight(Size::new(width, height)));
    }
}

struct ShellWindow {
    id: Uuid,
    window: Window,
    folder_path: Option<PathBuf>,
    restore_rect: Option<Rect>,
    mode: ShellWindowMode,
    workspace: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShellWindowMode {
    Normal,
    Minimized,
    Zoomed,
    Fullscreen,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DockActivation {
    ActivateExisting,
    LaunchNew,
}

#[derive(Debug, Clone, Copy)]
enum WindowInteraction {
    Move {
        window_id: Uuid,
        pointer_offset: Point,
    },
    Resize {
        window_id: Uuid,
        start_point: Point,
        start_rect: Rect,
    },
}

impl ShellDesktop {
    /// True only after the Linux layer-shell driver has successfully bound the
    /// compositor-owned desktop surfaces. In that mode ordinary application
    /// windows are XDG toplevels managed by `slopos-compositor`; the shell's
    /// `ShellWindow` model remains available solely for the non-layer test and
    /// fallback path and must not participate in live desktop painting or hit
    /// testing.
    fn compositor_owns_ordinary_windows(&self) -> bool {
        self.layer_shell_bound
    }

    fn new(
        menu_server: Arc<RwLock<MenuServer>>,
        launch_services: Arc<RwLock<LaunchServices>>,
        window_manager: Arc<RwLock<WindowManager>>,
        notification_center: Arc<RwLock<NotificationCenter>>,
        workspace_manager: Arc<RwLock<WorkspaceManager>>,
        dock: Arc<RwLock<Dock>>,
        session_manager: Arc<RwLock<SessionManager>>,
    ) -> Self {
        let mut desktop = IconView::new();
        desktop.layout_mode = IconViewLayoutMode::Desktop;
        desktop.icon_size = 56.0;
        desktop.spacing = 18.0;
        desktop.items = vec![
            IconItem {
                label: "Hard Disk".to_string(),
                icon: Some("drive".to_string()),
                selected: false,
                rect: Rect::ZERO,
            },
            IconItem {
                label: "Home".to_string(),
                icon: Some("home".to_string()),
                selected: false,
                rect: Rect::ZERO,
            },
            IconItem {
                label: "Applications".to_string(),
                icon: Some("applications".to_string()),
                selected: false,
                rect: Rect::ZERO,
            },
            IconItem {
                label: "Trash".to_string(),
                icon: Some("trash".to_string()),
                selected: false,
                rect: Rect::ZERO,
            },
        ];

        let mut bundle_ids = Vec::new();
        let mut bundles = launch_services
            .read()
            .bundles
            .values()
            .cloned()
            .collect::<Vec<_>>();
        bundles.sort_by(|left, right| left.name.cmp(&right.name));
        for bundle in bundles.iter().take(6) {
            bundle_ids.push(bundle.bundle_id.clone());
            desktop.items.push(IconItem {
                label: bundle.name.clone(),
                icon: Some(bundle.bundle_id.clone()),
                selected: false,
                rect: Rect::ZERO,
            });
        }

        let menus = menu_server.read().menus.clone();
        let lock_screen_widget = build_lock_screen_window();
        let expected_lock_password = get_lock_password();
        let mut lock_password_field = TextField::new().with_placeholder("Enter password");
        lock_password_field.is_password = true;
        let mut shell = Self {
            state: WidgetState::new(),
            menu_bar: MenuBar::new(menus),
            desktop,
            windows: Vec::new(),
            window_interaction: None,
            pointer: PointerDispatcher::new(),
            menu_server,
            launch_services,
            window_manager,
            notification_center,
            workspace_manager,
            dock: dock.clone(),
            session_manager,
            dock_view: DockView::new(),
            bundle_ids,
            notification_popup_windows: Vec::new(),
            last_error: None,
            locked: false,
            lock_screen_widget,
            lock_password_field,
            lock_error_message: None,
            expected_lock_password,
            session_clients: SessionClientRegistry::new(),
            // Protocol chrome: kit-matched menu bar (19) + dock (64).
            chrome: ChromeSession::bootstrap_default(
                session_output_size().0,
                session_output_size().1,
                MENU_BAR_HEIGHT_PX as i32,
                64,
            ),
            foreign_toplevels: ForeignToplevelRegistry::new(),
            layer_shell_bound: false,
            foreign_toplevel_synced: false,
            chrome_focus: ChromeFocusTarget::MenuBar,
            last_input_at: std::time::Instant::now(),
            // settings.conf flat keys (idle_lock_secs, …) when present; else defaults.
            idle_config: IdleConfig::parse_from_conf(&read_settings_conf_text()),
            idle_inhibit: IdleInhibitState::new(),
            mime_registry: {
                let mut reg = MimeOpenRegistry::new();
                seed_slopos_defaults(&mut reg);
                reg
            },
            mime_open_spawn: true,
            last_mime_open: None,
            last_status_refresh: std::time::Instant::now(),
            last_network_connect: None,
            network_connect_spawn: true,
            pending_session_confirmation: None,
            paint_filter: ShellPaintFilter::All,
            input_filter: None,
            spotlight_ui: spotlight_ui::SpotlightUI::new(),
            spaces_snapshot_initialized: false,
            workspace_overview: None,
        };
        // Map layer-shell chrome + sync foreign-toplevel list when a compositor is live.
        SloposI::attach_wayland_session_protocols(&mut shell);
        if let Ok(app) = std::env::var("SLOPOS_START_APP") {
            if app == "finder" || app == "com.slopos.finder" {
                // Finder is a first-party Wayland client, not an in-shell
                // painted window. Keep the shell responsible for chrome only.
                shell.launch_external_app("com.slopos.finder");
            } else {
                shell.launch_external_app(&app);
            }
        }
        if std::env::var_os("SLOPOS_START_SPOTLIGHT").is_some() {
            shell.spotlight_ui.show();
        }
        shell
    }

    /// Refresh foreign-toplevel list from the compositor before showing Force Quit.
    fn refresh_foreign_toplevels_from_compositor(&mut self) {
        if let Some(n) =
            foreign_toplevel_client::try_sync_foreign_toplevels(&mut self.foreign_toplevels)
        {
            self.foreign_toplevel_synced = true;
            tracing::debug!(toplevels = n, "Force Quit: foreign-toplevel-list refreshed");
            self.apply_foreign_rule_workspaces_to_shell_windows();
        }
    }

    /// When foreign-toplevel window rules assign a workspace, move matching
    /// [`ShellWindow`]s to that workspace index (clamped to 0..7).
    fn apply_foreign_rule_workspaces_to_shell_windows(&mut self) {
        let assignments: Vec<(String, String, usize)> = self
            .foreign_toplevels
            .entries()
            .filter_map(|e| {
                e.workspace.map(|ws| {
                    (
                        e.title.clone(),
                        e.app_id.clone(),
                        (ws as usize).min(SHELL_DESKTOP_COUNT.saturating_sub(1)),
                    )
                })
            })
            .collect();
        if assignments.is_empty() {
            return;
        }
        let mut moved = 0usize;
        for (title, app_id, ws) in assignments {
            for shell_window in &mut self.windows {
                let title_match = shell_window.window.title() == title;
                // Session-client foreign entries use binary name as title and
                // bundle_id as app_id; also match title substring for internal windows.
                let app_title_match = !app_id.is_empty()
                    && shell_window
                        .window
                        .title()
                        .to_ascii_lowercase()
                        .contains(&app_id.to_ascii_lowercase());
                if (title_match || app_title_match) && shell_window.workspace != ws {
                    shell_window.workspace = ws;
                    self.window_manager
                        .write()
                        .assign_workspace(shell_window.id, ws);
                    moved += 1;
                }
            }
        }
        if moved > 0 {
            tracing::debug!(moved, "window rules: moved ShellWindows to rule workspaces");
        }
    }

    /// Drain kit AT-SPI pending DoAction queue into real shell handlers.
    fn drain_a11y_pending_actions(&mut self) {
        let pending = slopos_kit::drain_pending_actions();
        for action in pending {
            let plan = resolve_pending_invoke(
                &action.path,
                &action.object_name,
                action.action_index,
                &action.action_name,
            );
            if !plan.valid {
                tracing::debug!(
                    path = %action.path,
                    name = %action.object_name,
                    "a11y DoAction: no shell invoke mapping"
                );
                continue;
            }
            tracing::info!(
                invoke_id = %plan.invoke_id,
                path = %action.path,
                "a11y DoAction → shell handler"
            );
            self.dispatch_a11y_invoke(&plan.invoke_id);
        }
    }

    /// Handle chrome.* and shell.* invoke ids from a11y / keyboard Activate.
    ///
    /// Routing is pure-classified via [`classify_a11y_invoke`]; side effects run here.
    fn dispatch_a11y_invoke(&mut self, invoke_id: &str) {
        match classify_a11y_invoke(invoke_id) {
            A11yDispatchTarget::ChromeMenuActivate => {
                // Open the system/SLOPOS menu (index 0). Prefer title "SLOPOS" when present.
                if let Some(idx) = self.menu_bar.menus.iter().position(|m| m.title == "SLOPOS") {
                    let _ = self.menu_bar.open_menu_at(idx);
                } else {
                    let _ = self.menu_bar.open_first_menu();
                }
            }
            A11yDispatchTarget::ChromeDockActivate => {
                if let Some(bundle) = self.bundle_ids.first().cloned() {
                    self.launch_external_app(&bundle);
                }
            }
            A11yDispatchTarget::ChromeDesktopOpen => {
                if let Some(item) = self.desktop.items.iter().position(|i| i.selected) {
                    self.launch_item(item);
                } else if !self.desktop.items.is_empty() {
                    self.launch_item(0);
                }
            }
            A11yDispatchTarget::ChromeWindowActivateNext => {
                // Focus next non-minimized window on the active workspace (Cmd+Tab parity).
                self.focus_next_window();
            }
            A11yDispatchTarget::ChromeWindowClose => self.close_active_window(),
            A11yDispatchTarget::ChromeWindowMinimize => {
                if self.compositor_owns_ordinary_windows() {
                    self.request_focused_window_action(WindowPresentationAction::Minimize);
                } else if let Some(id) = self.active_window_id() {
                    self.toggle_window_minimized(id);
                }
            }
            A11yDispatchTarget::ChromeDockMenu => {
                self.open_dock_context_menu_window();
            }
            A11yDispatchTarget::ChromeDesktopMenu => {
                self.open_desktop_context_menu_window();
            }
            A11yDispatchTarget::MenuAction(action) => {
                // shell.lock / shell.log_out / shell.notification_center /
                // shell.force_quit / workspace.next / workspace.previous / …
                self.handle_menu_action(action);
            }
            A11yDispatchTarget::MenuActionOwned(action) => {
                self.handle_menu_action(&action);
            }
            A11yDispatchTarget::Unknown => {
                // Fall through: keep prior behaviour for any unmapped but menu-like ids.
                self.handle_menu_action(invoke_id);
            }
        }
    }

    /// Cycle focus to the next non-minimized window on the active workspace.
    fn focus_next_window(&mut self) {
        let active_workspace = self.active_workspace();
        let workspace_window_ids: Vec<Uuid> = self
            .windows
            .iter()
            .filter(|w| w.workspace == active_workspace && w.mode != ShellWindowMode::Minimized)
            .map(|w| w.id)
            .collect();
        if workspace_window_ids.is_empty() {
            return;
        }
        let next_id = if let Some(current_id) = self.active_window_id() {
            let pos = workspace_window_ids
                .iter()
                .position(|&id| id == current_id)
                .unwrap_or(0);
            workspace_window_ids[(pos + 1) % workspace_window_ids.len()]
        } else {
            workspace_window_ids[0]
        };
        self.focus_window(next_id);
    }

    fn launch_item(&mut self, index: usize) {
        let item = match self.desktop.items.get(index) {
            Some(item) => item,
            None => return,
        };

        if let Some(bundle_id) = item.icon.as_deref() {
            if self.bundle_ids.iter().any(|id| id == bundle_id) {
                let bundle_id = bundle_id.to_string();
                self.launch_external_app(&bundle_id);
                return;
            }
        }

        match item.label.as_str() {
            "Applications" => {
                let bundle_id = self
                    .launch_services
                    .read()
                    .bundle_for_id("com.slopos.finder")
                    .map(|bundle| bundle.bundle_id.clone());
                if let Some(bundle_id) = bundle_id {
                    self.launch_external_app(&bundle_id);
                }
            }
            "Home" => {
                self.open_folder_window("Home", home_dir());
            }
            "Hard Disk" => {
                self.open_folder_window("Hard Disk", PathBuf::from("/"));
            }
            "Trash" => {
                self.open_folder_window("Trash", trash_dir());
            }
            _ => {}
        }
    }

    fn content_bounds(&self) -> Rect {
        // Prefer protocol chrome exclusive zones (menu bar top / dock bottom).
        // HiDPI: scale insets when SLOPOS_OUTPUT_SCALE / SHELL_SCALE > 1.
        let scale = detect_shell_scale_from_env();
        let menu_height = self
            .chrome
            .surfaces()
            .iter()
            .find(|s| s.role == ChromeRole::MenuBar && s.mapped)
            .map(|s| s.height as f64)
            .unwrap_or(MENU_BAR_HEIGHT as f64);
        let dock_height = self
            .chrome
            .surfaces()
            .iter()
            .find(|s| s.role == ChromeRole::Dock && s.mapped)
            .map(|s| s.height as f64)
            .unwrap_or(64.0);
        let (menu_height, dock_height) = scaled_chrome_insets(scale, menu_height, dock_height);
        let menu_height = menu_height as f32;
        let dock_height = dock_height as f32;
        Rect::new(
            self.rect().x,
            self.rect().y + menu_height,
            self.rect().width,
            (self.rect().height - menu_height - dock_height).max(0.0),
        )
    }

    fn next_finder_rect(&self) -> Rect {
        let base = if self.rect().width > 0.0 && self.rect().height > 0.0 {
            default_finder_rect(self.rect())
        } else {
            Rect::new(66.0, 66.0, 520.0, 320.0)
        };
        let offset = (self.windows.len() as f32 * 22.0) % 132.0;
        clamp_window_rect(
            Rect::new(base.x + offset, base.y + offset, base.width, base.height),
            self.content_bounds(),
        )
    }

    fn open_finder_window(&mut self) -> Uuid {
        self.open_folder_window("SLOPOS-I", PathBuf::from("/"))
    }

    /// Reconcile the shell's render/input mirror with the latest compositor
    /// projection.  A missing or malformed state file is expected during
    /// startup and leaves the compatibility mirror untouched; revision
    /// ordering and validation are enforced by `WorkspaceManager::apply_snapshot`.
    fn reconcile_spaces_snapshot(&mut self) {
        let snapshot = match read_spaces_snapshot() {
            Ok(snapshot) => snapshot,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
                ) =>
            {
                return
            }
            Err(error) => {
                tracing::debug!(%error, "could not read compositor Spaces snapshot");
                return;
            }
        };

        let (current_revision, current_epoch) = {
            let manager = self.workspace_manager.read();
            (manager.revision, manager.session_epoch)
        };
        let compositor_restarted = snapshot.session_epoch != 0
            && current_epoch != 0
            && snapshot.session_epoch != current_epoch;
        if self.spaces_snapshot_initialized
            && !compositor_restarted
            && snapshot.revision <= current_revision
        {
            return;
        }
        let applied = self.workspace_manager.write().apply_snapshot(&snapshot);
        if applied {
            self.spaces_snapshot_initialized = true;
            self.refresh_workspace_overview();
        }
    }

    fn active_workspace(&self) -> usize {
        self.workspace_manager.read().active
    }

    /// Find the live Spaces grid inside a shell-owned overview window. The
    /// returned node is produced by the actual widget, so labels, bounds and
    /// selected/focused state follow the same model used for painting and
    /// keyboard dispatch.
    fn workspace_grid_accessibility(widget: &dyn Widget) -> Option<AccessibilityNode> {
        if let Some(grid) = widget.as_any().downcast_ref::<WorkspaceGridView>() {
            return grid.accessibility();
        }
        for child in widget.children() {
            if let Some(node) = Self::workspace_grid_accessibility(child) {
                return Some(node);
            }
        }
        None
    }

    /// Build the current shell accessibility snapshot. The static chrome
    /// nodes preserve the existing AT-SPI focus/action paths; when the live
    /// Spaces overview exists, its real WorkspaceGridView is appended as a
    /// dynamic list node.
    fn accessibility_tree(&self) -> AccessibilityTree {
        let mut tree = slopos_kit::shell_chrome_accessibility_tree("SLOPOS-I");

        let grid = self
            .workspace_overview
            .as_ref()
            .and_then(|window| Self::workspace_grid_accessibility(window))
            .or_else(|| {
                self.windows
                    .iter()
                    .find(|window| window.window.title() == "Workspace")
                    .and_then(|window| Self::workspace_grid_accessibility(&window.window))
            });
        if let Some(grid) = grid {
            tree.add(grid);
        }
        tree
    }

    /// Synchronize the live widget snapshot with the process AT-SPI export.
    /// When no session/a11y bus was available at startup the kit returns
    /// `Ok(false)`; the shell still retains its in-process accessibility tree
    /// and action queue without claiming an external registration.
    fn sync_accessibility_tree(&self) {
        let tree = self.accessibility_tree();
        if let Err(error) = slopos_kit::sync_at_spi_registered_tree(&tree) {
            tracing::debug!(%error, "could not synchronize live AT-SPI tree");
        }
    }

    /// Return the current ordered Space IDs, labels and counts for the overview.
    ///
    /// IDs are carried alongside the render labels so accessibility and input
    /// paths can address the compositor by stable identity rather than by a
    /// transient row index.
    fn workspace_overview_data(&self) -> (usize, String, Vec<u64>, Vec<String>, Vec<usize>) {
        let manager = self.workspace_manager.read();
        let active = manager.active;
        let name = manager
            .active_workspace()
            .map(|workspace| workspace.name.clone())
            .unwrap_or_else(|| format!("Desktop {}", active + 1));
        let ids = manager
            .workspaces
            .iter()
            .map(|workspace| workspace.id)
            .collect::<Vec<_>>();
        let labels = manager
            .workspaces
            .iter()
            .map(|workspace| workspace.name.clone())
            .collect::<Vec<_>>();
        let counts = manager
            .workspaces
            .iter()
            .map(|workspace| workspace.window_count)
            .collect::<Vec<_>>();
        (active, name, ids, labels, counts)
    }

    fn update_workspace_grid(
        widget: &mut dyn Widget,
        active: usize,
        ids: &[u64],
        labels: &[String],
        counts: &[usize],
        session_epoch: u64,
        revision: u64,
    ) {
        if let Some(grid) = widget.as_any_mut().downcast_mut::<WorkspaceGridView>() {
            grid.active_index = active;
            grid.space_ids.clear();
            grid.space_ids.extend(ids.iter().copied());
            grid.items.clear();
            grid.items.extend(labels.iter().cloned());
            grid.window_counts.clear();
            grid.window_counts.extend(counts.iter().copied());
            grid.set_thumbnails(load_space_thumbnails(ids, session_epoch, revision));
            grid.normalize_focus();
        }
    }

    fn request_space_thumbnails(&self) {
        if !self.compositor_owns_ordinary_windows() {
            return;
        }
        let request = SessionControlRequest::Spaces {
            command: SpacesControlCommand::RefreshThumbnails,
        };
        if let Err(error) = send_session_control(&request) {
            tracing::debug!(%error, "could not request compositor Space thumbnails");
        }
    }

    /// Keep an already-open overview in sync with a newly reconciled snapshot.
    fn refresh_workspace_overview(&mut self) {
        let (active, _, ids, labels, counts) = self.workspace_overview_data();
        let (session_epoch, revision) = {
            let manager = self.workspace_manager.read();
            (manager.session_epoch, manager.revision)
        };
        for shell_window in &mut self.windows {
            if shell_window.window.title() == "Workspace" {
                let mut update = |widget: &mut dyn Widget| {
                    Self::update_workspace_grid(
                        widget,
                        active,
                        &ids,
                        &labels,
                        &counts,
                        session_epoch,
                        revision,
                    )
                };
                for_each_widget_mut(&mut shell_window.window, &mut update);
            }
        }
        if let Some(window) = self.workspace_overview.as_mut() {
            let mut update = |widget: &mut dyn Widget| {
                Self::update_workspace_grid(
                    widget,
                    active,
                    &ids,
                    &labels,
                    &counts,
                    session_epoch,
                    revision,
                )
            };
            for_each_widget_mut(window, &mut update);
        }
        self.layout_workspace_overview_overlay();
    }

    fn workspace_overview_rect(&self) -> Rect {
        let rows = self
            .workspace_manager
            .read()
            .workspaces
            .len()
            .div_ceil(slopos_kit::workspace_grid_view::GRID_COLS);
        let height = 260.0 + rows.saturating_sub(4) as f32 * 44.0;
        clamp_window_rect(
            Rect::new(
                self.content_bounds().x + 180.0,
                self.content_bounds().y + 120.0,
                300.0,
                height,
            ),
            self.content_bounds(),
        )
    }

    fn layout_workspace_overview_overlay(&mut self) {
        let rect = self.workspace_overview_rect();
        if let Some(window) = self.workspace_overview.as_mut() {
            window.set_rect(rect);
            let _ = window.layout(LayoutConstraint::tight(Size::new(rect.width, rect.height)));
        }
    }

    /// Select a grid cell.  Indexed switching remains the compatibility path;
    /// a live overview always addresses the compositor by stable Space ID.
    fn select_workspace_cell(&mut self, cell: usize) -> bool {
        let id = self
            .workspace_manager
            .read()
            .workspaces
            .get(cell)
            .map(|workspace| workspace.id);
        let Some(id) = id else {
            return false;
        };

        if self.compositor_owns_ordinary_windows() {
            let request = SessionControlRequest::Spaces {
                command: SpacesControlCommand::Select { id },
            };
            if let Err(error) = send_session_control(&request) {
                tracing::warn!(space_id = id, %error, "could not send Spaces selection to compositor");
                return false;
            }
            self.input_filter = None;
            tracing::info!(
                space_id = id,
                "sent stable-ID Spaces selection to compositor"
            );
            self.workspace_overview = None;
            true
        } else {
            let selected = self.switch_workspace(cell);
            if selected {
                self.input_filter = None;
            }
            selected
        }
    }

    /// Request that the compositor move its currently focused real window to
    /// the focused Space in the live overview.  The shell does not update its
    /// Space mirror optimistically; the compositor's next snapshot remains
    /// authoritative.  A failed send leaves the modal overview in place so
    /// the user can retry or dismiss it explicitly.
    fn move_active_window_to_workspace_cell(&mut self, cell: usize) -> bool {
        let id = self
            .workspace_manager
            .read()
            .workspaces
            .get(cell)
            .map(|workspace| workspace.id);
        let Some(id) = id else {
            return false;
        };
        if !self.compositor_owns_ordinary_windows() {
            return false;
        }

        let request = SessionControlRequest::Spaces {
            command: SpacesControlCommand::MoveActiveWindow {
                target: SpaceTargetWire::Id { id },
            },
        };
        match send_session_control(&request) {
            Ok(()) => {
                self.input_filter = None;
                self.workspace_overview = None;
                tracing::info!(
                    space_id = id,
                    "sent active-window move request to compositor"
                );
                true
            }
            Err(error) => {
                tracing::warn!(space_id = id, %error, "could not send active-window move to compositor");
                false
            }
        }
    }

    fn open_folder_window<S: Into<String>>(&mut self, title: S, path: PathBuf) -> Uuid {
        if self.compositor_owns_ordinary_windows() {
            // Directory browsing is a real Finder client operation in the
            // compositor-owned session. `open_path_with_mime` builds the
            // validated Finder argv and registers the resulting client; it
            // must not create a shell-painted stand-in window.
            self.open_path_with_mime(path);
            return Uuid::nil();
        }

        let rect = self.next_finder_rect();
        let title = title.into();
        let mut window = build_folder_window(&title, &path);
        window.set_rect(rect);
        let workspace = self.active_workspace();
        let id =
            self.window_manager
                .write()
                .create_window("com.slopos.finder", window.title(), rect);
        self.window_manager.write().assign_workspace(id, workspace);
        self.windows.push(ShellWindow {
            id,
            window,
            folder_path: Some(path),
            restore_rect: None,
            mode: ShellWindowMode::Normal,
            workspace,
        });
        self.focus_window(id);
        self.layout_window(id);
        id
    }

    fn open_message_window<S: Into<String>>(
        &mut self,
        title: S,
        lines: impl IntoIterator<Item = String>,
    ) -> Uuid {
        let title = title.into();
        let lines: Vec<String> = lines.into_iter().collect();
        if self.compositor_owns_ordinary_windows() {
            // About/status/Force Quit content is shell-owned overlay content,
            // not an ordinary application window. Until the dedicated shell
            // overlay layer is expanded, surface it through the existing
            // notification service instead of painting a fake XDG window into
            // the desktop background.
            self.record_notification(
                "com.slopos.shell",
                &title,
                &lines.join("\n"),
                NotificationPriority::Normal,
            );
            return Uuid::nil();
        }

        let rect = clamp_window_rect(
            Rect::new(
                self.content_bounds().x + 112.0,
                self.content_bounds().y + 72.0,
                540.0,
                240.0,
            ),
            self.content_bounds(),
        );
        let mut window = build_message_window(&title, lines);
        window.set_rect(rect);
        let workspace = self.active_workspace();
        let id =
            self.window_manager
                .write()
                .create_window("com.slopos.shell", window.title(), rect);
        self.window_manager.write().assign_workspace(id, workspace);
        self.windows.push(ShellWindow {
            id,
            window,
            folder_path: None,
            restore_rect: None,
            mode: ShellWindowMode::Normal,
            workspace,
        });
        self.focus_window(id);
        self.layout_window(id);
        id
    }

    fn close_active_window(&mut self) {
        if self.compositor_owns_ordinary_windows() {
            self.request_focused_window_action(WindowPresentationAction::Close);
            return;
        }
        let Some(id) = self.active_window_id() else {
            return;
        };
        self.close_window(id);
    }

    fn close_window(&mut self, id: Uuid) {
        self.windows.retain(|window| window.id != id);
        self.window_manager.write().close_window(id);
        if let Some(active) = self.windows.last_mut() {
            active.window.is_active = true;
        }
        if matches!(
            self.window_interaction,
            Some(WindowInteraction::Move { window_id, .. } | WindowInteraction::Resize { window_id, .. })
            if window_id == id
        ) {
            self.window_interaction = None;
        }
        self.sync_global_menu_to_active_window();
    }

    fn toggle_window_zoom(&mut self, id: Uuid) {
        let Some(index) = self.window_index(id) else {
            return;
        };

        if self.windows[index].mode == ShellWindowMode::Minimized {
            self.restore_minimized_window(id);
            return;
        }

        if self.windows[index].mode == ShellWindowMode::Zoomed {
            let Some(restore_rect) = self.windows[index].restore_rect.take() else {
                return;
            };
            let restore_rect = clamp_window_rect(restore_rect, self.content_bounds());
            self.windows[index].window.set_rect(restore_rect);
            self.windows[index].mode = ShellWindowMode::Normal;
            self.window_manager.write().restore_window(id);
        } else {
            let current = self.windows[index].window.rect();
            let zoom_rect = zoomed_window_rect(self.content_bounds(), self.windows.len());
            self.windows[index].restore_rect = Some(current);
            self.windows[index].mode = ShellWindowMode::Zoomed;
            self.windows[index].window.set_rect(zoom_rect);
            self.window_manager.write().maximize_window(id);
        }

        self.layout_window(id);
    }

    fn toggle_window_minimized(&mut self, id: Uuid) {
        let Some(index) = self.window_index(id) else {
            return;
        };

        if self.windows[index].mode == ShellWindowMode::Minimized {
            self.restore_minimized_window(id);
            return;
        }

        let current = self.windows[index].window.rect();
        let minimized_rect = minimized_window_rect(self.content_bounds(), index);
        self.windows[index].restore_rect = Some(current);
        self.windows[index].mode = ShellWindowMode::Minimized;
        self.windows[index].window.set_rect(minimized_rect);
        self.window_manager.write().minimize_window(id);
        self.layout_window(id);
    }

    fn restore_minimized_window(&mut self, id: Uuid) {
        let Some(index) = self.window_index(id) else {
            return;
        };
        let restore_rect = self.windows[index]
            .restore_rect
            .take()
            .unwrap_or_else(|| default_finder_rect(self.rect()));
        let restore_rect = clamp_window_rect(restore_rect, self.content_bounds());
        self.windows[index].window.set_rect(restore_rect);
        self.windows[index].mode = ShellWindowMode::Normal;
        self.window_manager.write().restore_window(id);
        self.layout_window(id);
    }

    fn toggle_window_fullscreen(&mut self, id: Uuid) {
        let Some(index) = self.window_index(id) else {
            return;
        };

        if self.windows[index].mode == ShellWindowMode::Minimized {
            self.restore_minimized_window(id);
            return;
        }

        if self.windows[index].mode == ShellWindowMode::Fullscreen {
            let Some(restore_rect) = self.windows[index].restore_rect.take() else {
                return;
            };
            let restore_rect = clamp_window_rect(restore_rect, self.content_bounds());
            self.windows[index].window.set_rect(restore_rect);
            self.windows[index].mode = ShellWindowMode::Normal;
            self.window_manager.write().restore_window(id);
        } else {
            let current = self.windows[index].window.rect();
            let fullscreen_rect = fullscreen_window_rect(self.content_bounds());
            self.windows[index].restore_rect = Some(current);
            self.windows[index].mode = ShellWindowMode::Fullscreen;
            self.windows[index].window.set_rect(fullscreen_rect);
            self.window_manager.write().set_fullscreen(id);
        }

        self.window_interaction = None;
        self.focus_window(id);
        self.layout_window(id);
    }

    fn focus_window(&mut self, id: Uuid) {
        let Some(index) = self.window_index(id) else {
            return;
        };
        let mut shell_window = self.windows.remove(index);
        shell_window.window.is_active = true;
        for w in &mut self.windows {
            w.window.is_active = false;
        }
        self.windows.push(shell_window);
        self.window_manager.write().focus_window(id);
        self.sync_global_menu_to_active_window();
    }

    fn sync_global_menu_to_active_window(&mut self) {
        let active_app = self.window_manager.read().active_window.and_then(|id| {
            self.window_manager
                .read()
                .windows
                .get(&id)
                .map(|window| window.app_id.clone())
        });

        if let Some(app_id) = active_app {
            self.refresh_menu_manifests();
            self.menu_server.write().set_active_app_menus(&app_id);
        } else {
            self.menu_server.write().reset_to_shell_menus();
        }
        self.menu_bar.menus = self.menu_server.read().menus.clone();
    }

    fn activate_app_menu(&mut self, bundle_id: &str) {
        self.refresh_menu_manifests();
        self.menu_server.write().set_active_app_menus(bundle_id);
        self.menu_bar.menus = self.menu_server.read().menus.clone();
    }

    /// Activate a Spotlight search result (app, file, or setting).
    fn activate_spotlight_result(&mut self, result: &spotlight::SearchResult) {
        match result {
            spotlight::SearchResult::App(app) => {
                self.launch_external_app(&app.bundle_id);
            }
            spotlight::SearchResult::File { path, .. } => {
                if let Ok(plan) = open_plan_for_file_uri(
                    &self.mime_registry,
                    &format!("file://{}", path.display()),
                ) {
                    if self.mime_open_spawn {
                        let _ = spawn_argv(&plan);
                    } else {
                        self.last_mime_open = Some(plan);
                    }
                }
            }
            spotlight::SearchResult::Setting { .. } => {
                self.launch_external_app("com.slopos.settings");
            }
        }
    }

    fn refresh_menu_manifests(&mut self) {
        let Some(dir) = slopos_sdk::menu_manifest_dir() else {
            return;
        };
        if let Err(err) = self.menu_server.write().load_menu_manifests_from_dir(dir) {
            tracing::warn!("failed to load menu manifests: {err}");
        }
    }

    /// Map compositor foreign-toplevel `app_id` to a SLOPOS-I bundle id.
    fn bundle_id_for_foreign_app_id(app_id: &str) -> Option<&'static str> {
        let id = app_id.trim().to_ascii_lowercase();
        match id.as_str() {
            "finder" | "com.slopos.finder" => Some("com.slopos.finder"),
            "settings" | "com.slopos.settings" => Some("com.slopos.settings"),
            "terminal" | "com.slopos.terminal" => Some("com.slopos.terminal"),
            "textedit" | "com.slopos.textedit" => Some("com.slopos.textedit"),
            "appstore" | "com.slopos.appstore" => Some("com.slopos.appstore"),
            _ => None,
        }
    }

    /// Match a compositor foreign-toplevel app id to a canonical SLOPOS
    /// bundle id without treating arbitrary strings as launch commands.
    fn foreign_toplevel_matches_bundle(app_id: &str, bundle_id: &str) -> bool {
        app_id.trim() == bundle_id
    }

    fn dock_activation_for_existing_client(existing_client: bool) -> DockActivation {
        if existing_client {
            DockActivation::ActivateExisting
        } else {
            DockActivation::LaunchNew
        }
    }

    /// Read the compositor-owned focused client record.
    ///
    /// The outer `Option` distinguishes “the compositor has not published a
    /// record yet” from a published record whose `app_id` is empty because
    /// focus is on shell chrome or the desktop. This is deliberately scoped to
    /// the session runtime; arbitrary global files are never consulted.
    fn compositor_active_app_id() -> Option<Option<String>> {
        let path = std::env::var_os("SLOPOS_ACTIVE_TOPLEVEL_FILE")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("SLOPOS_SESSION_RUNTIME_DIR")
                    .map(PathBuf::from)
                    .map(|dir| dir.join("active-toplevel"))
            })?;
        let contents = fs::read_to_string(path).ok()?;
        let app_id = contents
            .lines()
            .find_map(|line| line.strip_prefix("app_id="))
            .unwrap_or_default()
            .trim();
        if app_id.is_empty() {
            Some(None)
        } else if app_id.chars().all(|ch| !ch.is_control()) {
            Some(Some(app_id.to_string()))
        } else {
            None
        }
    }

    /// Synchronize the global menu to the compositor-focused client.
    ///
    /// The old implementation chose the first launched client or first foreign
    /// toplevel, which made the bar appear to belong to the shell's fake Finder
    /// window. The compositor focus record is authoritative; list order is only
    /// a compatibility fallback for direct/non-SLOPOS compositors.
    fn maybe_sync_foreign_app_menu(&mut self) {
        foreign_toplevel_client::try_sync_foreign_toplevels(&mut self.foreign_toplevels);

        if let Some(active) = Self::compositor_active_app_id() {
            match active {
                Some(app_id) => {
                    let bundle = Self::bundle_id_for_foreign_app_id(&app_id)
                        .map(str::to_owned)
                        .unwrap_or(app_id);
                    self.activate_app_menu(&bundle);
                }
                None => {
                    self.menu_server.write().reset_to_shell_menus();
                    self.menu_bar.menus = self.menu_server.read().menus.clone();
                }
            }
            return;
        }

        let active_internal = if self.compositor_owns_ordinary_windows() {
            None
        } else {
            self.window_manager.read().active_window.and_then(|id| {
                self.window_manager
                    .read()
                    .windows
                    .get(&id)
                    .map(|w| w.app_id.clone())
            })
        };

        if let Some(app_id) = active_internal {
            if app_id != "com.slopos.finder" {
                return;
            }
        }

        let session_bundle = self
            .session_clients
            .clients()
            .map(|c| c.bundle_id.clone())
            .next();

        let foreign_bundle = self
            .foreign_toplevels
            .entries()
            .find_map(|e| Self::bundle_id_for_foreign_app_id(&e.app_id))
            .map(str::to_owned);

        let bundle = session_bundle.or(foreign_bundle);
        if let Some(bundle) = bundle {
            self.activate_app_menu(&bundle);
        }
    }

    fn switch_workspace(&mut self, workspace: usize) -> bool {
        let mut workspace_manager = self.workspace_manager.write();
        if workspace >= workspace_manager.total {
            return false;
        }
        if self.compositor_owns_ordinary_windows() {
            let Some(id) = workspace_manager
                .workspaces
                .get(workspace)
                .map(|space| space.id)
            else {
                return false;
            };
            let request = SessionControlRequest::Spaces {
                command: SpacesControlCommand::Select { id },
            };
            if let Err(error) = send_session_control(&request) {
                tracing::warn!(workspace, %error, "could not send workspace switch to compositor");
                return false;
            }
            // The compositor publishes the new active Space.  Do not mutate
            // the shell mirror until that authoritative snapshot is observed.
            tracing::info!(
                workspace,
                space_id = id,
                "sent stable-ID Spaces selection to compositor"
            );
            return true;
        }
        if !workspace_manager.switch_to(workspace) {
            return false;
        }
        drop(workspace_manager);
        let active_workspace = self.active_workspace();
        for shell_window in &mut self.windows {
            shell_window.window.is_active = false;
        }
        let active_id = self
            .windows
            .iter()
            .rev()
            .find(|window| window.workspace == active_workspace)
            .map(|window| window.id);
        if let Some(id) = active_id {
            if let Some(index) = self.window_index(id) {
                self.windows[index].window.is_active = true;
            }
            self.window_manager.write().focus_window(id);
        } else {
            self.window_manager.write().active_window = None;
        }
        self.open_workspace_status_window();
        true
    }

    fn switch_to_next_workspace(&mut self) {
        let next = {
            let manager = self.workspace_manager.read();
            (manager.total > 0).then(|| (manager.active + 1) % manager.total)
        };
        if let Some(next) = next {
            let _ = self.switch_workspace(next);
        }
    }

    fn switch_to_previous_workspace(&mut self) {
        let previous = {
            let manager = self.workspace_manager.read();
            (manager.total > 0).then(|| {
                if manager.active == 0 {
                    manager.total - 1
                } else {
                    manager.active - 1
                }
            })
        };
        if let Some(previous) = previous {
            let _ = self.switch_workspace(previous);
        }
    }

    fn open_workspace_status_window(&mut self) {
        if self.compositor_owns_ordinary_windows() {
            if self.workspace_overview.is_some() {
                self.refresh_workspace_overview();
                return;
            }

            // The compositor owns thumbnail pixels. Request a refresh before
            // constructing the overview; the next authoritative Spaces
            // snapshot causes `refresh_workspace_overview` to load the newly
            // committed files. Captures are only accepted when their manifest
            // matches that authoritative session epoch and revision.
            self.request_space_thumbnails();

            let (active, name, ids, labels, counts) = self.workspace_overview_data();
            let (session_epoch, revision) = {
                let manager = self.workspace_manager.read();
                (manager.session_epoch, manager.revision)
            };
            let visible_count = counts.get(active).copied().unwrap_or(0);
            let mut layout = Layout::vertical(12.0);
            layout.add(Box::new(Label::new("Select/Switch Workspace:")));

            let mut grid = WorkspaceGridView::new();
            grid.active_index = active;
            grid.focused_index = active;
            grid.widget_state_mut().focused = true;
            grid.space_ids = ids;
            grid.items = labels;
            grid.window_counts = counts;
            grid.set_thumbnails(load_space_thumbnails(
                &grid.space_ids,
                session_epoch,
                revision,
            ));
            layout.add(Box::new(grid));

            let desc = format!("Active: {} ({} windows)", name, visible_count);
            layout.add(Box::new(Label::new(desc)));

            let rect = self.workspace_overview_rect();
            let mut window = Window::new("Workspace");
            window.set_content(Box::new(LayoutView::new(layout)));
            window.set_rect(rect);
            let _ = window.layout(LayoutConstraint::tight(Size::new(rect.width, rect.height)));
            self.workspace_overview = Some(window);
            return;
        }

        for window in &self.windows {
            if window.window.title() == "Workspace" {
                self.focus_window(window.id);
                return;
            }
        }

        let (active, name, ids, labels, counts) = self.workspace_overview_data();

        let visible_count = self
            .windows
            .iter()
            .filter(|window| window.workspace == active)
            .count();

        let rect = self.workspace_overview_rect();

        let mut layout = Layout::vertical(12.0);
        layout.add(Box::new(Label::new("Select/Switch Workspace:")));

        let mut grid = WorkspaceGridView::new();
        grid.active_index = active;
        grid.space_ids = ids;
        grid.items = labels;
        grid.window_counts = counts;
        layout.add(Box::new(grid));

        let desc = format!("Active: {} ({} windows)", name, visible_count);
        layout.add(Box::new(Label::new(desc)));

        let mut window = Window::new("Workspace");
        window.set_content(Box::new(LayoutView::new(layout)));
        window.set_rect(rect);

        let workspace = self.active_workspace();
        let id =
            self.window_manager
                .write()
                .create_window("com.slopos.shell", window.title(), rect);
        self.window_manager.write().assign_workspace(id, workspace);
        self.windows.push(ShellWindow {
            id,
            window,
            folder_path: None,
            restore_rect: None,
            mode: ShellWindowMode::Normal,
            workspace,
        });
        self.focus_window(id);
        self.layout_window(id);
    }

    fn launch_external_app(&mut self, bundle_id: &str) {
        // Reap exited clients first so the registry reflects the live multi-client set.
        let _ = self.session_clients.reap();
        let scanned = self
            .launch_services
            .read()
            .bundle_for_id(bundle_id)
            .cloned();
        let spawn_result = match scanned.as_ref() {
            Some(bundle) if session_clients::bundle_entrypoint_exists(bundle) => {
                session_clients::spawn_app_client_for_bundle(bundle)
            }
            _ => session_clients::spawn_app_client(bundle_id),
        };
        match spawn_result {
            Ok(client) => {
                let pid = client.pid;
                let binary_name = client.binary_name.clone();
                tracing::info!(
                    "Launched multi-client app {bundle_id} as pid {pid} (compositor-managed surface)"
                );
                // Foreign-toplevel mirror for Force Quit / task list (with pid).
                self.foreign_toplevels.add(ForeignToplevelEntry::new(
                    format!("session-client-{pid}"),
                    binary_name,
                    bundle_id,
                    Some(pid),
                ));
                self.apply_foreign_rule_workspaces_to_shell_windows();
                self.session_clients.register(client);
                self.last_error = None;
                self.activate_app_menu(bundle_id);
                self.record_notification(
                    bundle_id,
                    "Application Launched",
                    &format!(
                        "Started process pid={pid} ({} client(s) active).",
                        self.session_clients.len()
                    ),
                    NotificationPriority::Normal,
                );
            }
            Err(msg) => {
                tracing::error!("launch_external_app failed for {bundle_id}: {msg}");
                self.last_error = Some(msg.clone());
                self.record_notification(
                    bundle_id,
                    "Launch Failed",
                    &msg,
                    NotificationPriority::Normal,
                );
            }
        }
    }

    /// Apply a Force Quit list selection (window title, external client, or foreign toplevel).
    /// Returns true if a shell window closed or a client/toplevel was force-quit.
    fn apply_force_quit_entry(&mut self, entry: &str) -> bool {
        if let Some(target) = parse_toplevel_force_quit(entry) {
            let ok = apply_toplevel_force_quit(&mut self.foreign_toplevels, &target);
            if let Some(pid) = target.pid {
                // Keep session client registry in sync when quitting by pid.
                let client_ok = self.session_clients.force_quit_pid(pid);
                return ok || client_ok;
            }
            return ok;
        }
        match parse_force_quit_entry(entry) {
            Some(ForceQuitTarget::WindowTitle(title)) => {
                let target_id = self
                    .windows
                    .iter()
                    .find(|w| w.window.title() == title)
                    .map(|w| w.id);
                if let Some(tid) = target_id {
                    self.close_window(tid);
                    true
                } else {
                    false
                }
            }
            Some(ForceQuitTarget::ClientPid(pid)) => {
                // Drop matching foreign-toplevel entry if present (match by pid).
                let _ = self.foreign_toplevels.remove_match(&ToplevelForceQuit {
                    title: String::new(),
                    app_id: None,
                    pid: Some(pid),
                });
                self.session_clients.force_quit_pid(pid)
            }
            None => false,
        }
    }

    fn active_window_id(&self) -> Option<Uuid> {
        let active_workspace = self.active_workspace();
        self.windows
            .iter()
            .rev()
            .find(|window| window.workspace == active_workspace)
            .map(|window| window.id)
    }

    fn window_index(&self, id: Uuid) -> Option<usize> {
        self.windows.iter().position(|window| window.id == id)
    }

    /// Topmost window on the active workspace whose frame contains `point`.
    ///
    /// This is window-manager geometry, not widget hit-testing, and it stays
    /// geometric by design (the AGENTS.md P2/P5 remediation path):
    /// z-order lives in `self.windows` order and workspace membership lives in
    /// `ShellWindow.workspace` — neither is knowable from the widget tree,
    /// which is exactly why generic dispatch must not make this call.
    fn top_window_index_at(&self, point: Point) -> Option<usize> {
        let active_workspace = self.active_workspace();
        self.windows
            .iter()
            .enumerate()
            .rev()
            .find(|(_, window)| {
                window.workspace == active_workspace && hit_test(&window.window, point)
            })
            .map(|(index, _)| index)
    }

    /// Route a pointer event through generic dispatch (implicit capture +
    /// hover synthesis over the shell's children, topmost first), then drain
    /// whatever activations the widgets recorded.
    fn dispatch_pointer_event(&mut self, event: &Event) -> EventResult {
        let mut pointer = std::mem::take(&mut self.pointer);
        let result = pointer.dispatch(self, event);
        self.pointer = pointer;
        self.process_pointer_activations();
        result
    }

    /// Drain widget activations recorded during dispatch and apply their
    /// shell-level meaning. This is the replacement for the old downcast
    /// hit-test chains: the widget decides *that* it was activated (with real
    /// press/release semantics), the shell only decides what that means.
    fn process_pointer_activations(&mut self) {
        // Dock: activate a live compositor-owned client, or launch only when
        // no matching client is present. The shell never changes client
        // geometry or focus directly.
        if let Some(item_idx) = self.dock_view.take_clicked() {
            let app_id = self.dock.write().launch_app(item_idx);
            if let Some(app_id) = app_id {
                let existing_client = if self.compositor_owns_ordinary_windows() {
                    self.refresh_foreign_toplevels_from_compositor();
                    self.foreign_toplevels
                        .entries()
                        .any(|entry| Self::foreign_toplevel_matches_bundle(&entry.app_id, &app_id))
                } else {
                    false
                };

                match Self::dock_activation_for_existing_client(existing_client) {
                    DockActivation::ActivateExisting => {
                        // A failed control send must not create a second
                        // process for an app that is already mapped.
                        let _ = self.request_application_activation(&app_id);
                    }
                    DockActivation::LaunchNew => {
                        self.launch_external_app(&app_id);
                    }
                }
            }
        }

        // Desktop icons: double-click launches.
        if let Some(item_idx) = self.desktop.take_activated() {
            self.launch_item(item_idx);
        }

        // Shell windows: buttons, the workspace grid, and folder-window icon
        // views record activations inside the tree; collect the resulting
        // actions first, then apply them (no borrows held across mutations).
        if self.compositor_owns_ordinary_windows() {
            // The live path has no shell-owned ordinary app windows to inspect,
            // but the Spaces overview is shell chrome and must remain
            // interactive on the Background layer.
            let mut grid_cell = None;
            let mut grid_drop = None;
            if let Some(overview) = self.workspace_overview.as_mut() {
                for_each_widget_mut(overview, &mut |widget| {
                    if let Some(grid) = widget.as_any_mut().downcast_mut::<WorkspaceGridView>() {
                        grid_drop = grid.take_dropped();
                        grid_cell = grid.take_activated();
                    }
                });
            }
            if let Some(cell) = grid_drop {
                // A pointer drag in the overview means “move the currently
                // focused compositor window to this Space”. The compositor
                // remains the sole authority; the shell only sends the typed
                // request and waits for its next authoritative snapshot.
                let _ = self.move_active_window_to_workspace_cell(cell);
            } else if let Some(cell) = grid_cell {
                let _ = self.select_workspace_cell(cell);
            }
            return;
        }
        enum WindowAction {
            Close(Uuid),
            ForceQuit { id: Uuid, entry: Option<String> },
            SwitchWorkspace { id: Uuid, cell: usize },
            OpenFolder { title: String, path: PathBuf },
            OpenFile(PathBuf),
        }
        let mut actions: Vec<WindowAction> = Vec::new();
        for shell_window in &mut self.windows {
            let id = shell_window.id;
            let title = shell_window.window.title().to_string();

            let mut clicked_buttons: Vec<String> = Vec::new();
            let mut grid_cell: Option<usize> = None;
            let mut activated_icon: Option<(String, Option<String>)> = None;
            for_each_widget_mut(&mut shell_window.window, &mut |widget| {
                if let Some(button) = widget.as_any_mut().downcast_mut::<Button>() {
                    if button.take_clicked() {
                        clicked_buttons.push(button.label().to_string());
                    }
                } else if let Some(grid) = widget.as_any_mut().downcast_mut::<WorkspaceGridView>() {
                    // The legacy shell has no compositor-owned active-window
                    // move path; consume any pointer drop without turning it
                    // into an unrelated Space selection.
                    let _ = grid.take_dropped();
                    if let Some(cell) = grid.take_activated() {
                        grid_cell = Some(cell);
                    }
                } else if let Some(icons) = widget.as_any_mut().downcast_mut::<IconView>() {
                    if let Some(item_idx) = icons.take_activated() {
                        activated_icon = icons
                            .items
                            .get(item_idx)
                            .map(|item| (item.label.clone(), item.icon.clone()));
                    }
                }
            });

            for label in clicked_buttons {
                match (title.as_str(), label.as_str()) {
                    ("Force Quit", "Cancel") | ("About SLOPOS-I", "OK") => {
                        actions.push(WindowAction::Close(id));
                    }
                    ("Force Quit", "Force Quit") => {
                        let entry = shell_window.window.content.as_deref().and_then(|content| {
                            let layout_view = content.as_any().downcast_ref::<LayoutView>()?;
                            let Layout::Vertical { children, .. } = &layout_view.layout else {
                                return None;
                            };
                            let list = children.get(1)?.as_any().downcast_ref::<ListView>()?;
                            list.selected_index.and_then(|i| list.items.get(i).cloned())
                        });
                        actions.push(WindowAction::ForceQuit { id, entry });
                    }
                    _ => {}
                }
            }

            if let Some(cell) = grid_cell {
                if title == "Workspace" {
                    actions.push(WindowAction::SwitchWorkspace { id, cell });
                }
            }

            if let (Some((label, icon)), Some(folder)) =
                (activated_icon, shell_window.folder_path.clone())
            {
                let path = folder.join(&label);
                if icon.as_deref() == Some("folder") {
                    if path.is_dir() {
                        actions.push(WindowAction::OpenFolder { title: label, path });
                    }
                } else if path.exists() && !path.is_dir() {
                    actions.push(WindowAction::OpenFile(path));
                }
            }
        }

        for action in actions {
            match action {
                WindowAction::Close(id) => self.close_window(id),
                WindowAction::ForceQuit { id, entry } => {
                    if let Some(entry) = entry {
                        let _ = self.apply_force_quit_entry(&entry);
                    }
                    self.close_window(id);
                }
                WindowAction::SwitchWorkspace { id, cell } => {
                    self.handle_menu_action(&format!("workspace.switch.{cell}"));
                    self.close_window(id);
                }
                WindowAction::OpenFolder { title, path } => {
                    self.open_folder_window(title, path);
                }
                WindowAction::OpenFile(path) => self.open_path_with_mime(path),
            }
        }
    }

    /// Route keyboard input to the live Spaces overview before generic shell
    /// navigation. The overlay is a compositor-owned modal surface: Escape
    /// dismisses it, arrows move a local focus cell, and Enter/Space commit
    /// the focused cell through the same stable-ID request used by pointer
    /// activation. No local Space mirror is changed optimistically.
    fn handle_live_workspace_overview_key(&mut self, event: &Event) -> Option<EventResult> {
        if !self.compositor_owns_ordinary_windows() || self.workspace_overview.is_none() {
            return None;
        }

        let (key, modifiers) = match event {
            Event::KeyDown { key, modifiers } => (*key, *modifiers),
            _ => return None,
        };
        if key == slopos_kit::event::KeyCode::Escape {
            self.workspace_overview = None;
            self.input_filter = None;
            return Some(EventResult::Handled);
        }
        if !matches!(
            key,
            slopos_kit::event::KeyCode::ArrowLeft
                | slopos_kit::event::KeyCode::ArrowRight
                | slopos_kit::event::KeyCode::ArrowUp
                | slopos_kit::event::KeyCode::ArrowDown
                | slopos_kit::event::KeyCode::Enter
                | slopos_kit::event::KeyCode::Space
        ) {
            return None;
        }

        let move_active_window = key == slopos_kit::event::KeyCode::Enter
            && modifiers.shift
            && !modifiers.meta
            && !modifiers.control
            && !modifiers.alt;

        let mut result = EventResult::Ignored;
        let mut activated = None;
        if let Some(overview) = self.workspace_overview.as_mut() {
            for_each_widget_mut(overview, &mut |widget| {
                if !matches!(result, EventResult::Ignored) {
                    return;
                }
                if let Some(grid) = widget.as_any_mut().downcast_mut::<WorkspaceGridView>() {
                    result = grid.handle_event(event);
                    activated = grid.take_activated();
                }
            });
        }
        if let Some(cell) = activated {
            // Invalid/stale cells are intentionally ignored by either helper;
            // in particular they do not emit an IPC request or close the
            // overlay.
            if move_active_window {
                let _ = self.move_active_window_to_workspace_cell(cell);
            } else {
                let _ = self.select_workspace_cell(cell);
            }
        }

        // Keep recognized overview keys modal even when the grid is empty or
        // stale, so they cannot fall through to generic workspace shortcuts.
        Some(if matches!(result, EventResult::StopPropagation) {
            EventResult::StopPropagation
        } else {
            EventResult::Handled
        })
    }

    /// Open a filesystem path via MIME registry (`open_plan` + optional live spawn).
    ///
    /// Folders should use [`Self::open_folder_window`] instead. When
    /// `mime_open_spawn` is false (unit tests), only records the plan.
    fn open_path_with_mime(&mut self, path: PathBuf) {
        match open_plan(&self.mime_registry, &path) {
            Ok(plan) => {
                tracing::info!(
                    app_id = %plan.app_id,
                    argv = ?spawn_argv(&plan),
                    path = %path.display(),
                    "MIME open plan"
                );
                self.last_mime_open = Some(plan.clone());
                if !self.mime_open_spawn {
                    return;
                }
                let _ = self.session_clients.reap();
                match session_clients::spawn_open_plan(&plan) {
                    Ok(client) => {
                        let pid = client.pid;
                        let binary_name = client.binary_name.clone();
                        let bundle_id = client.bundle_id.clone();
                        self.foreign_toplevels.add(ForeignToplevelEntry::new(
                            format!("session-client-{pid}"),
                            binary_name,
                            &bundle_id,
                            Some(pid),
                        ));
                        self.apply_foreign_rule_workspaces_to_shell_windows();
                        self.session_clients.register(client);
                        self.last_error = None;
                        self.activate_app_menu(&bundle_id);
                        self.record_notification(
                            &bundle_id,
                            "Opened",
                            &format!("{} (pid={pid})", path.display()),
                            NotificationPriority::Normal,
                        );
                    }
                    Err(msg) => {
                        tracing::error!(
                            path = %path.display(),
                            error = %msg,
                            "MIME open spawn failed"
                        );
                        self.last_error = Some(msg.clone());
                        self.record_notification(
                            "com.slopos.shell",
                            "Open Failed",
                            &msg,
                            NotificationPriority::Normal,
                        );
                    }
                }
            }
            Err(err) => {
                tracing::warn!(path = %path.display(), error = %err, "MIME open: no handler");
                self.last_mime_open = None;
                self.last_error = Some(err.clone());
                self.record_notification(
                    "com.slopos.shell",
                    "No Application",
                    &format!("{}: {err}", path.display()),
                    NotificationPriority::Normal,
                );
            }
        }
    }

    fn layout_window(&mut self, id: Uuid) {
        let Some(index) = self.window_index(id) else {
            return;
        };
        let rect = self.windows[index].window.rect();
        let _ = self.windows[index]
            .window
            .layout(LayoutConstraint::tight(Size::new(rect.width, rect.height)));
        self.window_manager.write().move_window(id, rect);
    }

    fn layout_windows(&mut self) {
        if self.compositor_owns_ordinary_windows() {
            return;
        }
        let bounds = self.content_bounds();
        for index in 0..self.windows.len() {
            let rect = self.windows[index].window.rect();
            let rect = if self.windows[index].mode == ShellWindowMode::Fullscreen {
                fullscreen_window_rect(bounds)
            } else if self.windows[index].mode == ShellWindowMode::Minimized {
                minimized_window_rect(bounds, index)
            } else if self.windows[index].mode == ShellWindowMode::Zoomed {
                zoomed_window_rect(bounds, self.windows.len())
            } else if rect.width <= 1.0 || rect.height <= 1.0 {
                let base = default_finder_rect(self.rect());
                let offset = (index as f32 * 22.0) % 132.0;
                Rect::new(base.x + offset, base.y + offset, base.width, base.height)
            } else {
                rect
            };
            let rect = clamp_window_rect(rect, bounds);
            let id = self.windows[index].id;
            self.windows[index].window.set_rect(rect);
            self.layout_window(id);
        }
    }

    fn handle_menu_action(&mut self, action: &str) {
        match action {
            "shell.new_finder_window" => {
                self.open_finder_window();
            }
            "shell.close_finder_window" => {
                if self.compositor_owns_ordinary_windows() {
                    self.request_focused_window_action(WindowPresentationAction::Close);
                } else {
                    self.close_active_window();
                }
            }
            "shell.zoom_window" => {
                if self.compositor_owns_ordinary_windows() {
                    self.request_focused_window_action(WindowPresentationAction::ToggleZoom);
                } else if let Some(id) = self.active_window_id() {
                    self.toggle_window_zoom(id);
                }
            }
            "shell.toggle_fullscreen" => {
                if self.compositor_owns_ordinary_windows() {
                    self.request_focused_window_action(
                        WindowPresentationAction::ToggleFullscreen,
                    );
                } else if let Some(id) = self.active_window_id() {
                    self.toggle_window_fullscreen(id);
                }
            }
            "shell.open_home" => {
                self.open_folder_window("Home", home_dir());
            }
            "shell.open_computer" => {
                self.open_folder_window("Hard Disk", PathBuf::from("/"));
            }
            "shell.open_finder" => self.launch_external_app("com.slopos.finder"),
            "shell.settings" => self.launch_external_app("com.slopos.settings"),
            "shell.software_catalog" => self.launch_external_app("com.slopos.appstore"),
            "shell.about" => {
                self.open_about_window();
            }
            "shell.notification_center" => self.open_notification_center_window(),
            "shell.clear_notifications" => self.clear_notifications(),
            "shell.recent_items" => self.open_shell_status_window(
                "Recent Items",
                [
                    "Recent item tracking is not populated yet.".to_string(),
                    "Finder and app launches will be recorded here once session history is wired."
                        .to_string(),
                ],
            ),
            "shell.force_quit" => self.open_force_quit_window(),
            "shell.lock" => self.handle_session_action(session_actions::SessionAction::Lock),
            "shell.log_out" | "shell.logout" => {
                self.handle_session_action(session_actions::SessionAction::Logout)
            }
            "shell.suspend" | "shell.sleep" => {
                self.handle_session_action(session_actions::SessionAction::Suspend)
            }
            "shell.reboot" | "shell.restart" => {
                self.handle_session_action(session_actions::SessionAction::Reboot)
            }
            "shell.power_off" | "shell.shutdown" | "shell.poweroff" => {
                self.handle_session_action(session_actions::SessionAction::PowerOff)
            }
            "shell.network_connect" => self.handle_network_connect_menu(),
            "shell.save" => self.open_shell_status_window(
                "Save",
                ["The active shell window has no document to save.".to_string()],
            ),
            "shell.print" => self.open_shell_status_window(
                "Print",
                ["Printing is not connected to a system print service yet.".to_string()],
            ),
            "shell.screenshot" | "shell.portal_screenshot" => {
                // shell.portal_screenshot is the FreeDesktop portal-facing path;
                // until xdg-desktop-portal is wired it uses the same local capture.
                let result = if action == "shell.portal_screenshot" {
                    portal::take_portal_style_screenshot()
                } else {
                    capture::take_screenshot()
                };
                match result {
                    Ok(path) => {
                        self.record_notification(
                            "com.slopos.shell",
                            "Screenshot Saved",
                            &format!("Saved to {}", path.display()),
                            NotificationPriority::Normal,
                        );
                    }
                    Err(err) => {
                        self.record_notification(
                            "com.slopos.shell",
                            "Screenshot Failed",
                            &err.to_string(),
                            NotificationPriority::High,
                        );
                    }
                }
            }
            "shell.start_recording" => match capture::start_recording() {
                Ok(path) => {
                    self.record_notification(
                        "com.slopos.shell",
                        "Screen Recording",
                        &format!("Recording to {}", path.display()),
                        NotificationPriority::Normal,
                    );
                }
                Err(err) => {
                    self.record_notification(
                        "com.slopos.shell",
                        "Recording Failed",
                        &err.to_string(),
                        NotificationPriority::High,
                    );
                }
            },
            "shell.stop_recording" => match capture::stop_recording() {
                Ok(path) => {
                    self.record_notification(
                        "com.slopos.shell",
                        "Recording Saved",
                        &format!("Saved to {}", path.display()),
                        NotificationPriority::Normal,
                    );
                }
                Err(err) => {
                    self.record_notification(
                        "com.slopos.shell",
                        "Stop Recording Failed",
                        &err.to_string(),
                        NotificationPriority::High,
                    );
                }
            },
            "shell.undo" | "shell.redo" | "shell.cut" | "shell.copy" | "shell.paste"
            | "shell.select_all" => self.open_shell_status_window(
                "Edit",
                ["This edit command is only available inside document-aware apps.".to_string()],
            ),
            "shell.show_toolbar" => self.open_shell_status_window(
                "Toolbar",
                [
                    "Finder toolbar controls are already visible in shell folder windows."
                        .to_string(),
                ],
            ),
            "shell.show_sidebar" => self.open_shell_status_window(
                "Sidebar",
                ["The internal shell Finder view does not have a sidebar yet.".to_string()],
            ),
            "shell.help_search" => self.open_shell_status_window(
                "Help",
                [
                    "Help search is not indexed yet.".to_string(),
                    "Use README.md for setup, AGENTS.md for the development plan, and TRUTH.md for current status."
                        .to_string(),
                ],
            ),
            "workspace.previous" => self.switch_to_previous_workspace(),
            "workspace.next" => self.switch_to_next_workspace(),
            action if action.starts_with("workspace.switch.") => {
                if let Some(index) = action
                    .strip_prefix("workspace.switch.")
                    .and_then(|value| value.parse::<usize>().ok())
                {
                    let _ = self.switch_workspace(index);
                }
            }
            "shell.quit" => {
                std::process::exit(0);
            }
            "finder.new_folder" => self.handle_new_folder(),
            "finder.get_info" => self.handle_get_info(),
            "finder.rename" => self.handle_rename(),
            "finder.move_to_trash" => self.handle_move_to_trash(),
            _ if self.handle_sdk_app_menu_action(action) => {}
            _ => tracing::info!("Unhandled menu action: {action}"),
        }
    }

    fn handle_sdk_app_menu_action(&mut self, action: &str) -> bool {
        let active_app = self.menu_server.read().active_app.clone();
        let Some(active_app) = active_app else {
            return false;
        };
        if !action.starts_with(&format!("{active_app}.")) {
            return false;
        }

        // Application manifests may include a conventional Window menu. These
        // operations are still compositor semantics, never application-owned
        // geometry, even though the action id is namespaced by the app.
        let presentation_action = if action.ends_with(".window.minimize") {
            Some(WindowPresentationAction::Minimize)
        } else if action.ends_with(".window.zoom") {
            Some(WindowPresentationAction::ToggleZoom)
        } else if action.ends_with(".window.fullscreen") {
            Some(WindowPresentationAction::ToggleFullscreen)
        } else {
            None
        };
        if let Some(presentation_action) = presentation_action {
            if self.compositor_owns_ordinary_windows() {
                self.request_focused_window_action(presentation_action);
            } else if let Some(id) = self.active_window_id() {
                match presentation_action {
                    WindowPresentationAction::Minimize => self.toggle_window_minimized(id),
                    WindowPresentationAction::ToggleZoom => self.toggle_window_zoom(id),
                    WindowPresentationAction::ToggleFullscreen => self.toggle_window_fullscreen(id),
                    _ => {}
                }
            }
            return true;
        }

        if self.compositor_owns_ordinary_windows() {
            match send_application_menu_action(&active_app, action) {
                Ok(()) => tracing::info!(
                    bundle_id = %active_app,
                    action_id = %action,
                    "global menu action sent to focused application"
                ),
                Err(error) => {
                    tracing::warn!(
                        bundle_id = %active_app,
                        action_id = %action,
                        %error,
                        "focused application has no live menu endpoint"
                    );
                }
            }
        } else {
            let action_label =
                menu_action_label(&self.menu_bar.menus, action).unwrap_or_else(|| {
                    action
                        .rsplit('.')
                        .next()
                        .unwrap_or(action)
                        .replace('_', " ")
                });
            self.open_shell_status_window(
                "Application Menu Action",
                [
                    format!("Application: {active_app}"),
                    format!("Action: {action_label}"),
                    format!("Identifier: {action}"),
                    "Cross-process dispatch is available only in a live SLOPOS session."
                        .to_string(),
                ],
            );
        }
        true
    }

    fn request_focused_window_action(&self, action: WindowPresentationAction) {
        let request = SessionControlRequest::FocusedWindow { action };
        if let Err(error) = send_session_control(&request) {
            tracing::warn!(?action, %error, "could not send focused-window action to compositor");
        } else {
            tracing::info!(?action, "sent focused-window action to compositor");
        }
    }

    fn request_application_activation(&self, bundle_id: &str) -> bool {
        let request = SessionControlRequest::ActivateApplication {
            bundle_id: bundle_id.to_string(),
        };
        match send_session_control(&request) {
            Ok(()) => {
                tracing::info!(%bundle_id, "sent application activation request to compositor");
                true
            }
            Err(error) => {
                tracing::warn!(%bundle_id, %error, "could not send application activation request to compositor");
                false
            }
        }
    }

    /// Pure API / UI path: validate → nmcli plan → best-effort spawn (like systemctl).
    ///
    /// When `network_connect_spawn` is false, only validation + plan are recorded
    /// (unit tests). Missing `nmcli` returns `Err` without panicking.
    pub fn request_network_connect(&mut self, req: NmConnectRequest) {
        match nm_connect_plan_validated(&req) {
            Ok(plan) => {
                let summary = describe_nm_connect_plan(&plan);
                if !self.network_connect_spawn {
                    self.last_network_connect = Some(Ok(summary.clone()));
                    tracing::info!(%summary, "network connect plan (spawn disabled)");
                    return;
                }
                match execute_nm_connect_plan(&plan) {
                    Ok(()) => {
                        tracing::info!(%summary, "network connect spawned");
                        self.last_network_connect = Some(Ok(summary.clone()));
                        self.record_notification(
                            "com.slopos.shell",
                            "Network",
                            &format!("Connecting: {}", req.ssid),
                            NotificationPriority::Normal,
                        );
                        self.menu_server.write().refresh_status_items();
                        self.last_status_refresh = std::time::Instant::now();
                    }
                    Err(err) => {
                        tracing::warn!(%err, "network connect spawn failed");
                        self.last_network_connect = Some(Err(err.clone()));
                        self.record_notification(
                            "com.slopos.shell",
                            "Network Connect Failed",
                            &err,
                            NotificationPriority::High,
                        );
                    }
                }
            }
            Err(err) => {
                self.last_network_connect = Some(Err(err.clone()));
                self.record_notification(
                    "com.slopos.shell",
                    "Network Connect Invalid",
                    &err,
                    NotificationPriority::High,
                );
            }
        }
    }

    /// Menu action: connect using `SLOPOS_WIFI_SSID` (+ optional password env).
    fn handle_network_connect_menu(&mut self) {
        let ssid = std::env::var("SLOPOS_WIFI_SSID").unwrap_or_default();
        if ssid.trim().is_empty() {
            self.open_shell_status_window(
                "Network Connect",
                [
                    "Set SLOPOS_WIFI_SSID to connect from the menu.".to_string(),
                    "Optional: SLOPOS_WIFI_PASSWORD.".to_string(),
                    "API: ShellDesktop::request_network_connect(NmConnectRequest).".to_string(),
                ],
            );
            return;
        }
        let mut req = NmConnectRequest::new(ssid.trim());
        if let Ok(pw) = std::env::var("SLOPOS_WIFI_PASSWORD") {
            if !pw.is_empty() {
                req = req.with_password(pw);
            }
        }
        self.request_network_connect(req);
        if let Some(Ok(summary)) = &self.last_network_connect {
            self.open_shell_status_window(
                "Network Connect",
                [
                    format!(
                        "SSID: {}",
                        std::env::var("SLOPOS_WIFI_SSID").unwrap_or_default()
                    ),
                    summary.clone(),
                    "NetworkManager accepted the connection request.".to_string(),
                ],
            );
        } else if let Some(Err(err)) = &self.last_network_connect {
            self.open_shell_status_window(
                "Network Connect",
                [
                    err.clone(),
                    "Install NetworkManager (nmcli) on the session host.".to_string(),
                ],
            );
        }
    }

    /// Apply system volume via pactl/wpctl and refresh menu-bar status (best-effort).
    pub fn request_set_volume(&mut self, percent: u8) {
        match set_volume(percent) {
            Ok(()) => {
                self.menu_server.write().refresh_status_items();
                self.last_status_refresh = std::time::Instant::now();
            }
            Err(err) => {
                tracing::debug!(%err, "set_volume failed (status will show placeholder)");
                // Still refresh so volume label reflects unavailability.
                self.menu_server.write().refresh_status_items();
                self.last_status_refresh = std::time::Instant::now();
            }
        }
    }

    /// If the App Store left `~/Applications/.slopos-rescan`, rescan `.app` bundles.
    fn maybe_rescan_applications(&mut self) {
        let home = match std::env::var("HOME") {
            Ok(h) if !h.is_empty() => h,
            _ => return,
        };
        let marker = PathBuf::from(&home)
            .join("Applications")
            .join(".slopos-rescan");
        if !marker.is_file() {
            return;
        }
        let _ = std::fs::remove_file(&marker);
        self.launch_services.write().scan_applications();
        tracing::info!("rescanned Applications after App Store install marker");
    }

    /// Execute a session power/logout action via a typed plan and checked side effect.
    fn handle_session_action(&mut self, action: session_actions::SessionAction) {
        use session_actions::{
            confirm_prompt, describe_plan, plan_requires_privileges, plan_session_action,
            shell_delta_for_plan, SessionActionPlan,
        };

        let plan = plan_session_action(action);
        if confirm_prompt(action).is_none() {
            self.pending_session_confirmation = None;
        }

        // Destructive actions: show a confirmation status window first. The
        // same menu action must be activated again before side effects run.
        if let Some(prompt) = confirm_prompt(action) {
            if self.pending_session_confirmation != Some(action) {
                self.pending_session_confirmation = Some(action);
                self.open_shell_status_window(
                    match action {
                        session_actions::SessionAction::Logout => "Log Out",
                        session_actions::SessionAction::Reboot => "Restart",
                        session_actions::SessionAction::PowerOff => "Shut Down",
                        _ => "Session Action",
                    },
                    [
                        prompt.to_string(),
                        format!("Plan: {}", describe_plan(&plan)),
                        if plan_requires_privileges(&plan) {
                            "This will invoke system power management (systemctl/logind)."
                                .to_string()
                        } else {
                            String::new()
                        },
                        "Activate the same menu command again to confirm.".to_string(),
                    ],
                );
                return;
            }
            self.pending_session_confirmation = None;
            if !matches!(
                action,
                session_actions::SessionAction::Logout
                    | session_actions::SessionAction::Reboot
                    | session_actions::SessionAction::PowerOff
            ) {
                // unreachable for current confirm_prompt set
            } else {
                // For logout we still proceed (session exit is the product intent).
                // Reboot/PowerOff stay gated: show plan + require explicit systemctl execution
                // only after confirm status — we present the plan and run system commands
                // when privileges path is available; otherwise notify.
                if matches!(
                    action,
                    session_actions::SessionAction::Reboot
                        | session_actions::SessionAction::PowerOff
                ) {
                    self.open_shell_status_window(
                        match action {
                            session_actions::SessionAction::Reboot => "Restart",
                            _ => "Shut Down",
                        },
                        [
                            prompt.to_string(),
                            format!("Plan: {}", describe_plan(&plan)),
                            if plan_requires_privileges(&plan) {
                                "This will invoke system power management (systemctl/logind)."
                                    .to_string()
                            } else {
                                String::new()
                            },
                            "Executing now…".to_string(),
                        ],
                    );
                }
            }
        }

        let delta = shell_delta_for_plan(&plan);
        if delta.lock {
            if self.expected_lock_password.is_some() {
                self.session_manager.write().lock_screen();
                self.locked = true;
                self.lock_password_field.set_text("");
                // `TextField` now gates keyboard input on focus (see
                // AGENTS.md P2/P5); the lock screen has no other
                // widget to hand focus to, so it is always the one focused
                // widget while locked.
                self.lock_password_field.widget_state_mut().focused = true;
                self.lock_error_message = None;
            } else {
                self.notification_center.write().post(
                    "com.slopos.shell",
                    "Lock Password Not Set",
                    "Configure SLOPOS_LOCK_PASSWORD env var or lock_password in ~/.config/slopos-i/settings.conf",
                    NotificationPriority::High,
                );
            }
            return;
        }

        match plan {
            SessionActionPlan::ShellExit { code } => {
                tracing::info!(code, "session logout: shell exit");
                self.session_manager.write().logout_without_exit();
                std::process::exit(code);
            }
            SessionActionPlan::SystemCommand { argv } => {
                tracing::info!(?argv, "session power action");
                match std::process::Command::new(&argv[0])
                    .args(&argv[1..])
                    .status()
                {
                    Ok(status) if status.success() => {
                        self.record_notification(
                            "com.slopos.shell",
                            "Session",
                            &format!("Completed: {}", argv.join(" ")),
                            NotificationPriority::Normal,
                        );
                    }
                    Ok(status) => {
                        let error = format!(
                            "{} exited with status {status}",
                            describe_plan(&SessionActionPlan::SystemCommand { argv: argv.clone() })
                        );
                        self.record_notification(
                            "com.slopos.shell",
                            "Session Action Failed",
                            &error,
                            NotificationPriority::High,
                        );
                        self.open_shell_status_window(
                            "Session Action",
                            [
                                describe_plan(&SessionActionPlan::SystemCommand {
                                    argv: argv.clone(),
                                }),
                                error,
                                "Check logind/polkit permissions on the session host.".to_string(),
                            ],
                        );
                    }
                    Err(err) => {
                        self.record_notification(
                            "com.slopos.shell",
                            "Session Action Failed",
                            &format!(
                                "{} ({err})",
                                describe_plan(&SessionActionPlan::SystemCommand {
                                    argv: argv.clone()
                                })
                            ),
                            NotificationPriority::High,
                        );
                        self.open_shell_status_window(
                            "Session Action",
                            [
                                describe_plan(&SessionActionPlan::SystemCommand { argv }),
                                format!("Could not execute: {err}"),
                                "Install systemd/logind or run on a real session host.".to_string(),
                            ],
                        );
                    }
                }
            }
            SessionActionPlan::LogindMethod {
                method,
                interactive,
            } => {
                self.open_shell_status_window(
                    "Session Action",
                    [
                        format!("logind method: {method} (interactive={interactive})"),
                        format!("bus: {}", session_actions::LOGIND_BUS),
                        "D-Bus logind invoke is planned; use systemctl backend in this build."
                            .to_string(),
                    ],
                );
            }
            SessionActionPlan::ShellLock => {}
        }
    }

    fn handle_new_folder(&mut self) {
        let Some(id) = self.active_window_id() else {
            return;
        };
        let Some(index) = self.window_index(id) else {
            return;
        };
        let Some(folder_path) = self.windows[index].folder_path.clone() else {
            return;
        };
        let mut name = "untitled folder".to_string();
        let mut counter = 1;
        while folder_path.join(&name).exists() {
            name = format!("untitled folder {counter}");
            counter += 1;
        }
        if let Err(err) = fs::create_dir_all(folder_path.join(&name)) {
            tracing::error!("Failed to create folder: {err}");
            return;
        }
        self.refresh_active_folder_window();
    }

    fn handle_get_info(&mut self) {
        let Some(id) = self.active_window_id() else {
            return;
        };
        let Some(index) = self.window_index(id) else {
            return;
        };
        let title = self.windows[index].window.title().to_string();
        // Try to get info for the selected file first; fall back to the folder window itself.
        let selected_name = self.selected_file_name(index);
        let lines = if let Some(ref sel) = selected_name {
            if let Some(ref folder_path) = self.windows[index].folder_path.clone() {
                folder_info_lines(sel, &folder_path.join(sel))
            } else {
                vec![
                    format!("Name: {sel}"),
                    "Kind: SLOPOS-I window".to_string(),
                    "Location: Internal shell workspace".to_string(),
                ]
            }
        } else if let Some(ref path) = self.windows[index].folder_path.clone() {
            folder_info_lines(&title, path)
        } else {
            vec![
                format!("Name: {title}"),
                "Kind: SLOPOS-I window".to_string(),
                "Location: Internal shell workspace".to_string(),
            ]
        };
        let info_title = selected_name.unwrap_or(title);
        self.open_message_window(format!("{info_title} Info"), lines);
    }

    /// Returns the label of the currently selected icon item in the active folder window, if any.
    fn selected_file_name(&self, window_index: usize) -> Option<String> {
        let shell_window = self.windows.get(window_index)?;
        let icon_view = shell_window
            .window
            .content
            .as_ref()
            .and_then(|content| content.as_any().downcast_ref::<IconView>())?;
        icon_view
            .items
            .iter()
            .find(|item| item.selected)
            .map(|item| item.label.clone())
    }

    fn handle_rename(&mut self) {
        let Some(id) = self.active_window_id() else {
            return;
        };
        let Some(index) = self.window_index(id) else {
            return;
        };
        let folder_path_opt = self.windows[index].folder_path.clone();
        let Some(folder_path) = folder_path_opt else {
            self.open_shell_status_window(
                "Rename",
                ["Select a file in a folder window first.".to_string()],
            );
            return;
        };
        let Some(old_name) = self.selected_file_name(index) else {
            self.open_shell_status_window(
                "Rename",
                [
                    "No file selected. Click a file icon to select it, then choose Rename."
                        .to_string(),
                ],
            );
            return;
        };

        // Derive a new name: append " copy" or increment a counter if "copy" already present.
        let new_name = derive_rename_suggestion(&old_name);
        let old_path = folder_path.join(&old_name);
        let new_path = folder_path.join(&new_name);

        match fs::rename(&old_path, &new_path) {
            Ok(()) => {
                tracing::info!(
                    "Renamed '{}' -> '{}'",
                    old_path.display(),
                    new_path.display()
                );
                self.refresh_active_folder_window();
                self.open_shell_status_window(
                    "Rename",
                    [
                        format!("Renamed: {old_name}"),
                        format!("New name: {new_name}"),
                        "Note: a text-input prompt is not yet available; a suggested name was applied automatically.".to_string(),
                    ],
                );
            }
            Err(err) => {
                tracing::error!("Rename failed: {err}");
                self.open_shell_status_window(
                    "Rename Failed",
                    [
                        format!("Could not rename '{old_name}'."),
                        format!("Error: {err}"),
                    ],
                );
            }
        }
    }

    fn handle_move_to_trash(&mut self) {
        let Some(id) = self.active_window_id() else {
            return;
        };
        let Some(index) = self.window_index(id) else {
            return;
        };
        let folder_path_opt = self.windows[index].folder_path.clone();
        let Some(folder_path) = folder_path_opt else {
            self.open_shell_status_window(
                "Move to Trash",
                ["Select a file in a folder window first.".to_string()],
            );
            return;
        };
        let Some(file_name) = self.selected_file_name(index) else {
            self.open_shell_status_window(
                "Move to Trash",
                [
                    "No file selected. Click a file icon to select it, then choose Move to Trash."
                        .to_string(),
                ],
            );
            return;
        };

        let trash = trash_dir();
        if let Err(err) = fs::create_dir_all(&trash) {
            tracing::error!("Could not create trash directory: {err}");
            self.open_shell_status_window(
                "Move to Trash",
                [format!("Could not create Trash directory: {err}")],
            );
            return;
        }

        let src = folder_path.join(&file_name);
        // Avoid overwriting existing trash items with the same name.
        let mut dest = trash.join(&file_name);
        let mut counter = 1u32;
        while dest.exists() {
            let stem = std::path::Path::new(&file_name)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&file_name);
            let ext = std::path::Path::new(&file_name)
                .extension()
                .and_then(|s| s.to_str());
            let candidate = if let Some(ext) = ext {
                format!("{stem} {counter}.{ext}")
            } else {
                format!("{stem} {counter}")
            };
            dest = trash.join(&candidate);
            counter += 1;
        }

        match fs::rename(&src, &dest) {
            Ok(()) => {
                tracing::info!("Moved '{}' to trash ('{}')", src.display(), dest.display());
                self.refresh_active_folder_window();
                self.open_shell_status_window(
                    "Move to Trash",
                    [format!("'{file_name}' moved to Trash.")],
                );
            }
            Err(err) => {
                tracing::error!("Move to trash failed: {err}");
                self.open_shell_status_window(
                    "Move to Trash Failed",
                    [
                        format!("Could not move '{file_name}' to Trash."),
                        format!("Error: {err}"),
                    ],
                );
            }
        }
    }

    fn open_about_window(&mut self) {
        for window in &self.windows {
            if window.window.title() == "About SLOPOS-I" {
                self.focus_window(window.id);
                return;
            }
        }

        let rect = clamp_window_rect(
            Rect::new(
                self.content_bounds().x + 180.0,
                self.content_bounds().y + 120.0,
                400.0,
                320.0,
            ),
            self.content_bounds(),
        );

        // Gather live system info
        let host = session_manager::hostname();
        let uptime = format_uptime(session_manager::uptime_seconds());
        let (used_kb, total_kb) = session_manager::memory_usage();
        let mem_line = if total_kb > 0 {
            format!(
                "Memory: {} / {}",
                format_mem_gb(used_kb),
                format_mem_gb(total_kb)
            )
        } else {
            "Memory: Not available".to_string()
        };
        let battery_line = power::battery_info().summary_line();
        let network_line = network_manager::get_network_status().summary_line();

        let mut layout = Layout::vertical(12.0);
        layout.add(Box::new(Label::new("          SLOPOS-I   ")));
        layout.add(Box::new(Label::new(
            "----------------------------------------",
        )));
        layout.add(Box::new(Label::new("    Classic Desktop Environment")));
        layout.add(Box::new(Label::new("    Built in Rust with wgpu")));
        layout.add(Box::new(Label::new("    Version 1.0.0 (Production)")));
        layout.add(Box::new(Label::new(
            "----------------------------------------",
        )));
        layout.add(Box::new(Label::new(format!("Hostname: {host}"))));
        layout.add(Box::new(Label::new(format!("Uptime: {uptime}"))));
        layout.add(Box::new(Label::new(mem_line)));
        layout.add(Box::new(Label::new(battery_line)));
        layout.add(Box::new(Label::new(network_line)));
        let _ = self.session_clients.reap();
        layout.add(Box::new(Label::new(format!(
            "External clients: {}",
            self.session_clients.len()
        ))));

        let mut btn_layout = Layout::horizontal(10.0);
        btn_layout.add(Box::new(Button::new("OK")));
        layout.add(Box::new(LayoutView::new(btn_layout)));

        let rect = fit_dialog_rect(&mut layout, rect, self.content_bounds());
        let mut window = Window::new("About SLOPOS-I");
        window.set_content(Box::new(LayoutView::new(layout)));
        window.set_rect(rect);

        let workspace = self.active_workspace();
        let id =
            self.window_manager
                .write()
                .create_window("com.slopos.shell", window.title(), rect);
        self.window_manager.write().assign_workspace(id, workspace);
        self.windows.push(ShellWindow {
            id,
            window,
            folder_path: None,
            restore_rect: None,
            mode: ShellWindowMode::Normal,
            workspace,
        });
        self.focus_window(id);
        self.layout_window(id);
    }

    fn record_notification(
        &mut self,
        app_id: &str,
        title: &str,
        message: &str,
        priority: NotificationPriority,
    ) -> String {
        self.notification_center
            .write()
            .post(app_id, title, message, priority)
    }

    fn open_notification_center_window(&mut self) {
        let visible = self
            .notification_center
            .read()
            .visible()
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        let mut lines = vec!["Notification Center".to_string()];
        if visible.is_empty() {
            lines.push("No active notifications.".to_string());
        } else {
            for notification in visible {
                lines.push(format!(
                    "{} - {} ({:?})",
                    notification.id, notification.title, notification.priority
                ));
                lines.push(format!("  App: {}", notification.app_id));
                lines.push(format!("  {}", notification.message));
            }
        }
        self.open_message_window("Notification Center", lines);
    }

    fn clear_notifications(&mut self) {
        self.notification_center.write().dismiss_all();
        self.open_message_window(
            "Notification Center",
            ["All active notifications dismissed.".to_string()],
        );
    }

    fn open_force_quit_window(&mut self) {
        // Pull live compositor-owned toplevels into Force Quit list.
        self.refresh_foreign_toplevels_from_compositor();
        for window in &self.windows {
            if window.window.title() == "Force Quit" {
                self.focus_window(window.id);
                return;
            }
        }

        let rect = clamp_window_rect(
            Rect::new(
                self.content_bounds().x + 150.0,
                self.content_bounds().y + 100.0,
                400.0,
                300.0,
            ),
            self.content_bounds(),
        );

        let mut layout = Layout::vertical(10.0);
        layout.add(Box::new(Label::new(
            "Shell windows, session clients, and compositor foreign-toplevels:",
        )));

        let mut items = Vec::new();
        for w in &self.windows {
            if w.window.title() != "SLOPOS-I Desktop" && w.window.title() != "Force Quit" {
                items.push(format!("window: {}", w.window.title()));
            }
        }
        let _ = self.session_clients.reap();
        for client in self.session_clients.clients() {
            items.push(format!(
                "client: {} (pid {})",
                client.binary_name, client.pid
            ));
        }
        items.extend(self.foreign_toplevels.force_quit_labels());

        let mut list_view = ListView::new();
        list_view.items = items;
        list_view.selected_index = if list_view.items.is_empty() {
            None
        } else {
            Some(0)
        };
        layout.add(Box::new(list_view));

        let mut btn_layout = Layout::horizontal(10.0);
        btn_layout.add(Box::new(Button::new("Cancel")));
        btn_layout.add(Box::new(Button::new("Force Quit")));
        layout.add(Box::new(LayoutView::new(btn_layout)));

        let rect = fit_dialog_rect(&mut layout, rect, self.content_bounds());
        let mut window = Window::new("Force Quit");
        window.set_content(Box::new(LayoutView::new(layout)));
        window.set_rect(rect);

        let workspace = self.active_workspace();
        let id =
            self.window_manager
                .write()
                .create_window("com.slopos.shell", window.title(), rect);
        self.window_manager.write().assign_workspace(id, workspace);
        self.windows.push(ShellWindow {
            id,
            window,
            folder_path: None,
            restore_rect: None,
            mode: ShellWindowMode::Normal,
            workspace,
        });
        self.focus_window(id);
        self.layout_window(id);
    }

    fn open_shell_status_window<S: Into<String>>(
        &mut self,
        title: S,
        lines: impl IntoIterator<Item = String>,
    ) {
        self.open_message_window(title, lines);
    }

    /// a11y `chrome.dock.menu`: status/context shell window listing dock items.
    fn open_dock_context_menu_window(&mut self) {
        const TITLE: &str = "Dock Menu";
        for window in &self.windows {
            if window.window.title() == TITLE {
                self.focus_window(window.id);
                return;
            }
        }
        let dock = self.dock.read();
        let mut lines = vec!["Dock items:".to_string()];
        if dock.items.is_empty() {
            lines.push("(no dock items)".to_string());
        } else {
            for (i, item) in dock.items.iter().enumerate() {
                let state = format!("{:?}", item.state);
                lines.push(format!(
                    "{}. {} [{}] {}",
                    i + 1,
                    item.label,
                    item.app_id,
                    state
                ));
            }
        }
        drop(dock);
        self.open_shell_status_window(TITLE, lines);
    }

    /// a11y `chrome.desktop.menu`: status/context shell window listing desktop icons.
    fn open_desktop_context_menu_window(&mut self) {
        const TITLE: &str = "Desktop Menu";
        for window in &self.windows {
            if window.window.title() == TITLE {
                self.focus_window(window.id);
                return;
            }
        }
        let mut lines = vec!["Desktop icons:".to_string()];
        if self.desktop.items.is_empty() {
            lines.push("(no desktop icons)".to_string());
        } else {
            for (i, item) in self.desktop.items.iter().enumerate() {
                let selected = if item.selected { " [selected]" } else { "" };
                let icon = item.icon.as_deref().unwrap_or("-");
                lines.push(format!("{}. {} ({}){}", i + 1, item.label, icon, selected));
            }
        }
        self.open_shell_status_window(TITLE, lines);
    }

    fn refresh_active_folder_window(&mut self) {
        let Some(id) = self.active_window_id() else {
            return;
        };
        let Some(index) = self.window_index(id) else {
            return;
        };
        let Some(ref path) = self.windows[index].folder_path.clone() else {
            return;
        };
        let mut files = slopos_kit::icon_view::IconView::new();
        files.icon_size = 76.0;
        files.spacing = 10.0;
        files.items = folder_items_for_path(path);
        self.windows[index].window.set_content(Box::new(files));
        self.layout_window(id);
    }

    fn move_window_to(&mut self, id: Uuid, point: Point, pointer_offset: Point) {
        let Some(index) = self.window_index(id) else {
            return;
        };
        if self.windows[index].mode == ShellWindowMode::Minimized {
            return;
        }
        self.windows[index].restore_rect = None;
        self.windows[index].mode = ShellWindowMode::Normal;
        self.window_manager.write().restore_window(id);
        let current = self.windows[index].window.rect();
        let moved = Rect::new(
            point.x - pointer_offset.x,
            point.y - pointer_offset.y,
            current.width,
            current.height,
        );
        let moved = clamp_window_rect(moved, self.content_bounds());
        self.windows[index].window.set_rect(moved);
        self.layout_window(id);
    }

    fn resize_window_to(&mut self, id: Uuid, point: Point, start_point: Point, start_rect: Rect) {
        let Some(index) = self.window_index(id) else {
            return;
        };
        if self.windows[index].mode == ShellWindowMode::Minimized {
            return;
        }
        self.windows[index].restore_rect = None;
        self.windows[index].mode = ShellWindowMode::Normal;
        self.window_manager.write().restore_window(id);
        let resized = Rect::new(
            start_rect.x,
            start_rect.y,
            (start_rect.width + point.x - start_point.x).max(320.0),
            (start_rect.height + point.y - start_point.y).max(220.0),
        );
        let resized = clamp_window_rect(resized, self.content_bounds());
        self.windows[index].window.set_rect(resized);
        self.layout_window(id);
    }
}

fn default_finder_rect(shell_rect: Rect) -> Rect {
    let window_width = (shell_rect.width * 0.52).clamp(360.0, 560.0);
    let window_height = (shell_rect.height * 0.46).clamp(260.0, 380.0);
    Rect::new(
        shell_rect.x + 66.0,
        shell_rect.y + 66.0,
        window_width.min((shell_rect.width - 160.0).max(260.0)),
        window_height.min((shell_rect.height - 120.0).max(220.0)),
    )
}

fn titlebar_rect(window_rect: Rect) -> Rect {
    Rect::new(
        window_rect.x,
        window_rect.y,
        window_rect.width,
        WINDOW_TITLE_BAR_HEIGHT,
    )
}

fn close_box_rect(window_rect: Rect) -> Rect {
    Rect::new(window_rect.x + 8.0, window_rect.y + 7.0, 11.0, 11.0)
}

fn minimize_box_rect(window_rect: Rect) -> Rect {
    Rect::new(window_rect.x + 22.0, window_rect.y + 7.0, 11.0, 11.0)
}

fn zoom_box_rect(window_rect: Rect) -> Rect {
    Rect::new(
        window_rect.x + window_rect.width - 19.0,
        window_rect.y + 7.0,
        11.0,
        11.0,
    )
}

fn resize_handle_rect(window_rect: Rect) -> Rect {
    Rect::new(
        window_rect.x + window_rect.width - 18.0,
        window_rect.y + window_rect.height - 18.0,
        18.0,
        18.0,
    )
}

fn zoomed_window_rect(bounds: Rect, window_count: usize) -> Rect {
    let margin = if window_count > 1 { 10.0 } else { 0.0 };
    Rect::new(
        bounds.x + margin,
        bounds.y + margin,
        (bounds.width - margin * 2.0).max(320.0),
        (bounds.height - margin * 2.0).max(220.0),
    )
}

fn fullscreen_window_rect(bounds: Rect) -> Rect {
    Rect::new(
        bounds.x,
        bounds.y,
        bounds.width.max(320.0),
        bounds.height.max(220.0),
    )
}

fn minimized_window_rect(bounds: Rect, slot: usize) -> Rect {
    let width = bounds.width.clamp(220.0, 360.0);
    let height = 24.0;
    let gap = 8.0;
    let x = bounds.x + gap + (slot as f32 * (width + gap)) % (bounds.width - width - gap).max(1.0);
    let y = bounds.y + bounds.height - height - gap;
    Rect::new(x, y, width, height)
}

/// Grow a dialog's frame to hold its content's natural height (then clamp to
/// the shell bounds). The old dialog rects were fixed guesses; a too-small
/// frame left the button row arranged *below* the window's bottom edge, which
/// the old geometry chains happily hit-tested (an invisible click zone
/// outside the drawn window) but rect-checked dispatch correctly refuses.
fn fit_dialog_rect(layout: &mut Layout, rect: Rect, bounds: Rect) -> Rect {
    let natural = layout.layout_size(LayoutConstraint {
        min_width: 0.0,
        max_width: (rect.width - 2.0).max(0.0),
        min_height: 0.0,
        max_height: f32::INFINITY,
    });
    // Window frame: 25px titlebar + 1px border top, 1px border elsewhere
    // (content rect is `y + 25, height - 26` in `Window::layout`).
    clamp_window_rect(
        Rect::new(rect.x, rect.y, rect.width, natural.height + 26.0),
        bounds,
    )
}

fn clamp_window_rect(rect: Rect, bounds: Rect) -> Rect {
    let min_width = rect.width.min(bounds.width.max(1.0));
    let min_height = rect.height.min(bounds.height.max(1.0));
    let width = min_width.max(1.0);
    let height = min_height.max(1.0);
    let max_x = bounds.x + (bounds.width - width).max(0.0);
    let max_y = bounds.y + (bounds.height - height).max(0.0);

    Rect::new(
        rect.x.clamp(bounds.x, max_x),
        rect.y.clamp(bounds.y, max_y),
        width,
        height,
    )
}

fn build_folder_window(title: &str, path: &PathBuf) -> Window {
    let mut files = IconView::new();
    files.icon_size = 48.0;
    files.spacing = 10.0;
    files.items = folder_items_for_path(path);

    let mut window = Window::new(title);
    window.set_content(Box::new(files));
    window
}

fn settings_conf_path() -> PathBuf {
    std::env::var_os("SLOPOS_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".config/slopos-i"))
        })
        .unwrap_or_else(|| PathBuf::from("/tmp/slopos-i"))
        .join("settings.conf")
}

fn read_settings_conf_text() -> String {
    fs::read_to_string(settings_conf_path()).unwrap_or_default()
}

/// Load DisplayConfig and apply arrangement env (live nested layout bridge).
fn apply_display_config_from_settings() {
    let path = settings_conf_path();
    // DisplayConfig::load expects a TOML map with a `[display]` table; also
    // accept flat arrange_mode= in settings.conf (Settings app write path).
    let mut config = DisplayConfig::load(&path);
    config.merge_flat_settings_conf(&read_settings_conf_text());
    let outputs = DisplayConfig::session_outputs();
    match config.apply_arrangement_env(&outputs) {
        Ok(applied) => {
            if !applied.is_empty() {
                tracing::info!(
                    mode = %config.arrange_mode,
                    scale = config.scale_percent,
                    outputs = outputs.len(),
                    env = ?applied,
                    "display arrange plan applied (EmitLayoutEnv)"
                );
            }
        }
        Err(err) => {
            tracing::warn!(%err, "display arrangement plan failed");
        }
    }
}

fn get_lock_password() -> Option<String> {
    // First, check environment variable
    if let Ok(password) = std::env::var("SLOPOS_LOCK_PASSWORD") {
        let password = password.trim();
        if !password.is_empty() {
            return Some(password.to_string());
        }
    }

    // Then, check config file
    if let Ok(contents) = fs::read_to_string(settings_conf_path()) {
        for line in contents.lines() {
            if let Some(value) = line.strip_prefix("lock_password=") {
                let value = value.trim();
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }

    None
}

/// Pure password check used by the lock screen (and unit tests).
/// Empty entered password never unlocks. Unlock only on exact match.
pub fn verify_lock_password(entered: &str, expected: &str) -> bool {
    !entered.is_empty() && entered == expected
}

fn shell_locale_prefs() -> LocalePrefs {
    LocalePrefs::parse_from_env_lang(std::env::var("LANG").ok().as_deref())
}

fn build_lock_screen_window() -> Window {
    let locale = shell_locale_prefs();
    let mut layout = Layout::vertical(24.0);
    layout.add(Box::new(Label::new("SLOPOS-I")));
    layout.add(Box::new(Label::new(tr("lock.prompt", &locale.locale))));
    let mut window = Window::new(tr("menu.lock_screen", &locale.locale));
    window.set_content(Box::new(LayoutView::new(layout)));
    window
}

fn build_message_window(title: &str, lines: impl IntoIterator<Item = String>) -> Window {
    let mut layout = Layout::vertical(8.0);
    for line in lines {
        layout.add(Box::new(Label::new(line)));
    }

    let mut window = Window::new(title);
    window.set_content(Box::new(LayoutView::new(layout)));
    window
}

fn folder_info_lines(title: &str, path: &PathBuf) -> Vec<String> {
    let metadata = fs::metadata(path).ok();
    let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);

    let item_count = if is_dir {
        fs::read_dir(path)
            .map(|entries| entries.filter_map(|entry| entry.ok()).count())
            .ok()
    } else {
        None
    };

    let kind = metadata
        .as_ref()
        .map(|m| {
            if m.is_dir() {
                "Folder"
            } else if m.is_file() {
                "Document"
            } else {
                "Filesystem item"
            }
        })
        .unwrap_or("Unavailable");

    let writable = metadata
        .as_ref()
        .map(|m| {
            if m.permissions().readonly() {
                "No"
            } else {
                "Yes"
            }
        })
        .unwrap_or("Unknown");

    let file_size = metadata
        .as_ref()
        .filter(|m| m.is_file())
        .map(|m| human_readable_size(m.len()));

    let mut lines = vec![
        format!("Name: {title}"),
        format!("Kind: {kind}"),
        format!("Location: {}", path.display()),
    ];

    if let Some(size) = file_size {
        lines.push(format!("Size: {size}"));
    }

    if let Some(count) = item_count {
        lines.push(format!("Items: {count}"));
    }

    lines.push(format!("Writable: {writable}"));
    lines
}

fn human_readable_size(bytes: u64) -> String {
    const KB: u64 = 1_024;
    const MB: u64 = 1_024 * KB;
    const GB: u64 = 1_024 * MB;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} bytes")
    }
}

fn derive_rename_suggestion(name: &str) -> String {
    let path = std::path::Path::new(name);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or(name);
    let ext = path.extension().and_then(|s| s.to_str());

    // If the stem already ends with " copy N", increment N.
    // Otherwise append " copy".
    let new_stem = if let Some(idx) = stem.rfind(" copy") {
        let suffix = &stem[idx + 5..];
        if suffix.is_empty() {
            format!("{} copy 2", &stem[..idx])
        } else if let Ok(n) = suffix.trim().parse::<u32>() {
            format!("{} copy {}", &stem[..idx], n + 1)
        } else {
            format!("{stem} copy")
        }
    } else {
        format!("{stem} copy")
    };

    if let Some(ext) = ext {
        format!("{new_stem}.{ext}")
    } else {
        new_stem
    }
}

fn menu_action_label(menus: &[Menu], action_id: &str) -> Option<String> {
    for menu in menus {
        for item in &menu.items {
            if item.action_id == action_id {
                return Some(item.label.clone());
            }
            if matches!(item.kind, MenuItemKind::Submenu) {
                if let Some(submenu) = &item.submenu {
                    if let Some(label) = menu_action_label(std::slice::from_ref(submenu), action_id)
                    {
                        return Some(label);
                    }
                }
            }
        }
    }
    None
}

fn folder_items_for_path(path: &PathBuf) -> Vec<IconItem> {
    let Ok(entries) = fs::read_dir(path) else {
        return vec![IconItem {
            label: format!("⚠ Unable to read: {}", path.display()),
            icon: Some("document".to_string()),
            selected: false,
            rect: Rect::ZERO,
        }];
    };

    let mut entries = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                return None;
            }
            let is_dir = entry
                .path()
                .metadata()
                .map(|m| m.is_dir())
                .or_else(|_| entry.file_type().map(|k| k.is_dir()))
                .unwrap_or(false);
            Some((name, is_dir))
        })
        .collect::<Vec<_>>();

    entries.sort_by(|left, right| {
        right
            .1
            .cmp(&left.1)
            .then_with(|| left.0.to_lowercase().cmp(&right.0.to_lowercase()))
    });

    let mut items: Vec<IconItem> = entries
        .into_iter()
        .map(|(label, is_dir)| IconItem {
            label,
            icon: Some(if is_dir { "folder" } else { "document" }.to_string()),
            selected: false,
            rect: Rect::ZERO,
        })
        .collect();

    if items.is_empty() {
        items.push(IconItem {
            label: "This folder is empty".to_string(),
            icon: Some("document".to_string()),
            selected: false,
            rect: Rect::ZERO,
        });
    }

    items
}

/// Format uptime seconds as a human-readable string like "2d 4h" or "1h 23m".
fn format_uptime(secs: u64) -> String {
    let minutes = secs / 60;
    let hours = minutes / 60;
    let days = hours / 24;
    if days > 0 {
        format!("{}d {}h", days, hours % 24)
    } else if hours > 0 {
        format!("{}h {}m", hours, minutes % 60)
    } else {
        format!("{}m", minutes)
    }
}

/// Format a kilobyte count as a GB string (e.g. "2.1 GB") or MB if small.
fn format_mem_gb(kb: u64) -> String {
    const MB: u64 = 1024;
    const GB: u64 = 1024 * MB;
    if kb >= GB {
        format!("{:.1} GB", kb as f64 / GB as f64)
    } else if kb >= MB {
        format!("{:.0} MB", kb as f64 / MB as f64)
    } else {
        format!("{} KB", kb)
    }
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn trash_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".local/share"))
        .join("Trash/files")
}

impl Widget for ShellDesktop {
    fn widget_state(&self) -> &WidgetState {
        &self.state
    }

    fn widget_state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        let size = constraint.clamp(Size::new(constraint.max_width, constraint.max_height));
        self.set_rect(Rect::new(
            self.rect().x,
            self.rect().y,
            size.width,
            size.height,
        ));

        // Always keep the lock screen widget sized to fill the desktop
        self.lock_screen_widget.set_rect(Rect::new(
            self.rect().x,
            self.rect().y,
            size.width,
            size.height,
        ));
        let _ = self
            .lock_screen_widget
            .layout(LayoutConstraint::tight(Size::new(size.width, size.height)));

        if self.locked {
            return size;
        }

        self.menu_bar.set_rect(Rect::new(
            self.rect().x,
            self.rect().y,
            size.width,
            MENU_BAR_HEIGHT,
        ));
        let _ = self.menu_bar.layout(LayoutConstraint::tight(Size::new(
            size.width,
            MENU_BAR_HEIGHT,
        )));

        self.desktop.set_rect(Rect::new(
            self.rect().x,
            self.rect().y + MENU_BAR_HEIGHT,
            size.width,
            (size.height - MENU_BAR_HEIGHT - 64.0).max(0.0),
        ));
        let _ = self.desktop.layout(LayoutConstraint::tight(Size::new(
            size.width,
            (size.height - MENU_BAR_HEIGHT - 64.0).max(0.0),
        )));

        self.dock_view.set_rect(Rect::new(
            self.rect().x,
            self.rect().y + size.height - 64.0,
            size.width,
            64.0,
        ));
        let _ = self
            .dock_view
            .layout(LayoutConstraint::tight(Size::new(size.width, 64.0)));

        self.layout_windows();
        self.layout_workspace_overview_overlay();

        if self.spotlight_ui.is_visible() {
            self.spotlight_ui.layout_for_screen(size.width, size.height);
        }

        size
    }

    fn draw(&self, theme: &ThemeContext) {
        match self.paint_filter {
            ShellPaintFilter::MenuBar => {
                self.menu_bar.draw(theme);
                return;
            }
            ShellPaintFilter::MenuPopup => {
                self.menu_bar.draw(theme);
                return;
            }
            ShellPaintFilter::SpacesOverview => {
                if let Some(overview) = &self.workspace_overview {
                    overview.draw(theme);
                }
                return;
            }
            ShellPaintFilter::Dock => {
                self.dock_view.draw(theme);
                return;
            }
            ShellPaintFilter::Background | ShellPaintFilter::All => {}
        }

        if self.locked {
            self.lock_screen_widget.draw(theme);
            return;
        }
        self.desktop.draw(theme);
        if self.compositor_owns_ordinary_windows() {
            // The compositor paints every ordinary XDG toplevel above this
            // background surface. The shell paints wallpaper/icons and
            // shell-only overlays here, never a second application window.
            for popup in &self.notification_popup_windows {
                popup.draw(theme);
            }
            return;
        }
        let active_workspace = self.active_workspace();
        // Draw non-active windows first
        for shell_window in self
            .windows
            .iter()
            .filter(|window| window.workspace == active_workspace)
            .rev()
            .skip(1)
        {
            shell_window.window.draw(theme);
        }
        // Draw active window last (on top)
        if let Some(active) = self
            .windows
            .iter()
            .rev()
            .find(|window| window.workspace == active_workspace)
        {
            active.window.draw(theme);
        }
        if let Some(overview) = &self.workspace_overview {
            overview.draw(theme);
        }
        // When layer-shell chrome is bound, menu bar / dock are protocol surfaces —
        // skip kit dual-paint so chrome is not overdrawn in the shell canvas.
        if should_paint_kit_chrome(self.layer_shell_bound)
            && matches!(self.paint_filter, ShellPaintFilter::All)
        {
            self.menu_bar.draw(theme);
            self.dock_view.draw(theme);
        }
    }

    fn handle_event(&mut self, event: &Event) -> EventResult {
        // Any pointer/key activity resets idle timer (auto-lock policy).
        match event {
            Event::KeyDown { .. }
            | Event::KeyUp { .. }
            | Event::MouseDown { .. }
            | Event::MouseUp { .. }
            | Event::MouseMove { .. }
            | Event::Scroll { .. } => {
                self.last_input_at = std::time::Instant::now();
            }
            _ => {}
        }

        // When locked, handle password entry
        if self.locked {
            match event {
                Event::KeyDown {
                    key: slopos_kit::event::KeyCode::Escape,
                    ..
                } => {
                    // Escape key: clear the field and error
                    self.lock_password_field.set_text("");
                    self.lock_error_message = None;
                    return EventResult::Handled;
                }
                Event::KeyDown {
                    key: slopos_kit::event::KeyCode::Enter,
                    ..
                } => {
                    // Enter key: attempt to unlock (never unlock on empty / wrong / non-Enter keys)
                    let entered_password = self.lock_password_field.text().to_string();
                    if let Some(ref expected) = self.expected_lock_password {
                        if verify_lock_password(&entered_password, expected) {
                            self.session_manager.write().unlock();
                            self.locked = false;
                            self.lock_password_field.set_text("");
                            self.lock_error_message = None;
                            return EventResult::Handled;
                        } else {
                            self.lock_error_message = Some("Incorrect password".to_string());
                            self.lock_password_field.set_text("");
                            return EventResult::Handled;
                        }
                    }
                    return EventResult::Handled;
                }
                Event::Char { .. }
                | Event::KeyDown {
                    key: slopos_kit::event::KeyCode::Backspace,
                    ..
                } => {
                    // Pass character/backspace events to the password field
                    self.lock_password_field.handle_event(event);
                    self.lock_error_message = None;
                    return EventResult::Handled;
                }
                _ => {
                    // Swallow all other events while locked
                    return EventResult::Handled;
                }
            }
        }

        // Spotlight overlay (Super+Space) — modal layer that intercepts before menu bar.
        // When visible, all events route to the overlay; when invisible, events pass through.
        if let Event::KeyDown {
            key: slopos_kit::event::KeyCode::Space,
            modifiers,
        } = event
        {
            if modifiers.meta && !modifiers.control && !modifiers.alt {
                // Super+Space toggles the overlay
                if self.spotlight_ui.is_visible() {
                    self.spotlight_ui.hide();
                } else {
                    self.spotlight_ui.show();
                    let apps = self
                        .launch_services
                        .read()
                        .bundles
                        .values()
                        .cloned()
                        .collect::<Vec<_>>();
                    self.spotlight_ui.update_results(&apps);
                }
                return EventResult::Handled;
            }
        }

        // If Spotlight is visible, route events to it
        if self.spotlight_ui.is_visible() {
            if let Event::KeyDown { key, modifiers } = event {
                if *key == slopos_kit::event::KeyCode::Enter {
                    if let Some(selected) = self.spotlight_ui.selected_result() {
                        let selected_clone = selected.clone();
                        self.activate_spotlight_result(&selected_clone);
                        return EventResult::Handled;
                    }
                }

                let result = self.spotlight_ui.handle_overlay_key(*key, modifiers);
                if matches!(result, EventResult::Handled) {
                    if *key != slopos_kit::event::KeyCode::Escape
                        && *key != slopos_kit::event::KeyCode::Enter
                    {
                        let apps = self
                            .launch_services
                            .read()
                            .bundles
                            .values()
                            .cloned()
                            .collect::<Vec<_>>();
                        self.spotlight_ui.update_results(&apps);
                    }
                    return EventResult::Handled;
                }
            } else if let Event::Char { character } = event {
                self.spotlight_ui.append_char(*character);
                let apps = self
                    .launch_services
                    .read()
                    .bundles
                    .values()
                    .cloned()
                    .collect::<Vec<_>>();
                self.spotlight_ui.update_results(&apps);
                return EventResult::Handled;
            } else {
                return EventResult::Handled;
            }
        }

        if let Some(result) = self.handle_live_workspace_overview_key(event) {
            return result;
        }

        let result = self.menu_bar.handle_event(event);
        if matches!(result, EventResult::Handled | EventResult::StopPropagation) {
            return result;
        }

        if let Event::KeyDown { key, modifiers } = event {
            // Unified keyboard-only nav policy (Tab / Shift+Tab / Escape / Enter / lock / workspaces).
            let key_name = match key {
                slopos_kit::event::KeyCode::Tab => "tab",
                slopos_kit::event::KeyCode::Escape => "escape",
                slopos_kit::event::KeyCode::Enter => "enter",
                slopos_kit::event::KeyCode::Space => "space",
                slopos_kit::event::KeyCode::L => "l",
                slopos_kit::event::KeyCode::Q => "q",
                slopos_kit::event::KeyCode::LeftBracket => "[",
                slopos_kit::event::KeyCode::RightBracket => "]",
                _ => "",
            };
            if !key_name.is_empty() {
                if let Some(intent) = keyboard_nav_intent(
                    key_name,
                    modifiers.shift,
                    modifiers.meta,
                    modifiers.control,
                    modifiers.alt,
                ) {
                    match intent {
                        KeyboardNavIntent::NextChromeRegion
                        | KeyboardNavIntent::PrevChromeRegion => {
                            self.chrome_focus = apply_chrome_nav(self.chrome_focus, intent);
                            // Best-effort AT-SPI Focus: in-process always; D-Bus if registered.
                            let emit = crate::atspi_bus::emit_chrome_focus(self.chrome_focus);
                            tracing::debug!(
                                focus = ?self.chrome_focus,
                                ?intent,
                                dbus = emit.dbus_emitted,
                                "chrome focus"
                            );
                            return EventResult::Handled;
                        }
                        KeyboardNavIntent::Dismiss => {
                            // Close topmost dismissable transient window.
                            if let Some(id) = self
                                .windows
                                .iter()
                                .rev()
                                .find(|w| is_dismissable_window_title(w.window.title()))
                                .map(|w| w.id)
                            {
                                self.close_window(id);
                                return EventResult::Handled;
                            }
                        }
                        KeyboardNavIntent::Activate => {
                            // When chrome has focus, drain primary invoke from a11y_actions.
                            let plan = primary_invoke_for_chrome(self.chrome_focus);
                            if plan.valid {
                                tracing::debug!(
                                    invoke_id = %plan.invoke_id,
                                    focus = ?self.chrome_focus,
                                    "chrome Activate → a11y invoke"
                                );
                                self.dispatch_a11y_invoke(&plan.invoke_id);
                                return EventResult::Handled;
                            }
                            // Enter on Force Quit list is handled by window widgets below.
                        }
                        KeyboardNavIntent::NextWindow => {
                            // fall through to Meta+Tab block
                        }
                        KeyboardNavIntent::LockScreen => {
                            self.handle_session_action(session_actions::SessionAction::Lock);
                            return EventResult::Handled;
                        }
                        KeyboardNavIntent::LogOut => {
                            self.handle_session_action(session_actions::SessionAction::Logout);
                            return EventResult::Handled;
                        }
                        KeyboardNavIntent::NextWorkspace => {
                            self.switch_to_next_workspace();
                            return EventResult::Handled;
                        }
                        KeyboardNavIntent::PrevWorkspace => {
                            self.switch_to_previous_workspace();
                            return EventResult::Handled;
                        }
                    }
                }
            }
            // Cmd+Tab: cycle focus through non-minimized windows on the active workspace
            if modifiers.meta && *key == slopos_kit::event::KeyCode::Tab {
                self.focus_next_window();
                return EventResult::Handled;
            }

            // Cmd+W: close the front window on the active workspace
            if modifiers.meta && *key == slopos_kit::event::KeyCode::W {
                if let Some(id) = self.active_window_id() {
                    self.close_window(id);
                    return EventResult::Handled;
                }
            }

            let action = self
                .menu_server
                .read()
                .action_for_shortcut(*key, *modifiers);
            if let Some(action) = action {
                self.handle_menu_action(&action);
                return EventResult::Handled;
            }
        }

        match event {
            Event::MouseDown {
                button: MouseButton::Left,
                point,
                ..
            } => {
                // Window-manager policy first: raise the window under the
                // pointer and run its frame chrome (close/minimize/zoom
                // boxes, resize handle, titlebar drag). The dock strip is
                // chrome stacked above windows, so it exempts the WM pass —
                // generic dispatch below will route the click to `DockView`.
                if !hit_test(&self.dock_view, *point) {
                    if let Some(index) = self.top_window_index_at(*point) {
                        let window_id = self.windows[index].id;
                        self.focus_window(window_id);
                        let Some(index) = self.window_index(window_id) else {
                            return EventResult::Ignored;
                        };
                        let window_rect = self.windows[index].window.rect();
                        if close_box_rect(window_rect).contains(*point) {
                            self.close_window(window_id);
                            return EventResult::Handled;
                        }

                        if minimize_box_rect(window_rect).contains(*point) {
                            self.toggle_window_minimized(window_id);
                            return EventResult::Handled;
                        }

                        if zoom_box_rect(window_rect).contains(*point) {
                            self.toggle_window_zoom(window_id);
                            return EventResult::Handled;
                        }

                        if resize_handle_rect(window_rect).contains(*point) {
                            self.window_interaction = Some(WindowInteraction::Resize {
                                window_id,
                                start_point: *point,
                                start_rect: window_rect,
                            });
                            return EventResult::Handled;
                        }

                        if titlebar_rect(window_rect).contains(*point) {
                            self.window_interaction = Some(WindowInteraction::Move {
                                window_id,
                                pointer_offset: Point::new(
                                    point.x - window_rect.x,
                                    point.y - window_rect.y,
                                ),
                            });
                            return EventResult::Handled;
                        }
                    }
                }

                // Everything that is a widget — dock icons, dialog buttons,
                // workspace grid cells, desktop icons, window content — goes
                // through generic dispatch; activations drain afterwards.
                self.dispatch_pointer_event(event)
            }
            Event::MouseMove { point, .. } => {
                if let Some(interaction) = self.window_interaction {
                    match interaction {
                        WindowInteraction::Move {
                            window_id,
                            pointer_offset,
                        } => {
                            self.move_window_to(window_id, *point, pointer_offset);
                        }
                        WindowInteraction::Resize {
                            window_id,
                            start_point,
                            start_rect,
                        } => self.resize_window_to(window_id, *point, start_point, start_rect),
                    }
                    return EventResult::Handled;
                }

                self.dispatch_pointer_event(event)
            }
            Event::MouseUp {
                button: MouseButton::Left,
                ..
            } => {
                if self.window_interaction.take().is_some() {
                    return EventResult::Handled;
                }

                self.dispatch_pointer_event(event)
            }
            Event::DoubleClick { point, .. } => {
                // WM: a double-click raises the window under it, then the
                // widgets decide what it means (folder-window and desktop
                // `IconView`s record activations that drain below).
                if let Some(index) = self.top_window_index_at(*point) {
                    let window_id = self.windows[index].id;
                    self.focus_window(window_id);
                }

                self.dispatch_pointer_event(event)
            }
            Event::MouseLeave => self.dispatch_pointer_event(event),
            _ => self.desktop.handle_event(event),
        }
    }

    fn update(&mut self) {
        // The compositor is the sole Spaces authority.  Read its atomic
        // projection before rebuilding menu/overview chrome so a reorder,
        // rename, active selection, or window-count update is reflected in
        // the same shell tick.  WorkspaceManager rejects stale revisions.
        self.reconcile_spaces_snapshot();
        let workspace_items = self
            .workspace_manager
            .read()
            .workspaces
            .iter()
            .enumerate()
            .map(|(index, workspace)| (index, workspace.name.clone()))
            .collect::<Vec<_>>();
        self.menu_server
            .write()
            .set_workspace_items(&workspace_items);

        // App Store install marker → rescan ~/Applications/*.app (Stage 3).
        self.maybe_rescan_applications();

        // Visual QA hook: auto-open Spotlight with an optional query.
        // Example: SLOPOS_QA_SPOTLIGHT=vol
        if !self.spotlight_ui.is_visible() {
            if let Ok(query) = std::env::var("SLOPOS_QA_SPOTLIGHT") {
                self.spotlight_ui.show();
                for ch in query.chars() {
                    self.spotlight_ui.append_char(ch);
                }
                let apps = self
                    .launch_services
                    .read()
                    .bundles
                    .values()
                    .cloned()
                    .collect::<Vec<_>>();
                self.spotlight_ui.update_results(&apps);
                let (w, h) = (self.rect().width.max(1280.0), self.rect().height.max(800.0));
                self.spotlight_ui.layout_for_screen(w, h);
                // Consume so we do not re-show every frame after Escape.
                std::env::remove_var("SLOPOS_QA_SPOTLIGHT");
            }
        }

        // Sync lock state from SessionManager
        if self.session_manager.read().state == session_manager::SessionState::Locked
            && !self.locked
        {
            self.locked = true;
            self.lock_password_field.widget_state_mut().focused = true;
        }

        // AT-SPI DoAction queue → real shell handlers (lock / log out / chrome.*).
        self.drain_a11y_pending_actions();

        // Idle auto-lock: pure policy → shell lock when password configured.
        // Merge process-wide portal Inhibit cookies (Idle/Suspend flags) without
        // permanently mutating shell-local idle_inhibit (so UnInhibit clears).
        if !self.locked {
            let idle_secs = self.last_input_at.elapsed().as_secs();
            let mut inhibit = self.idle_inhibit.clone();
            for reason in portal_extra::active_idle_inhibit_state().reasons() {
                inhibit.add(*reason);
            }
            let phase = idle_phase(&self.idle_config, idle_secs, self.locked, &inhibit);
            if recommended_action(phase, self.locked) == IdleRecommendedAction::Lock
                && self.expected_lock_password.is_some()
            {
                tracing::info!(idle_secs, "idle policy: auto-lock");
                self.handle_session_action(session_actions::SessionAction::Lock);
            }
        }

        // Battery / volume / network menu status: refresh on idle update (throttled).
        if self.last_status_refresh.elapsed()
            >= std::time::Duration::from_secs(STATUS_REFRESH_INTERVAL_SECS)
        {
            self.menu_server.write().refresh_status_items();
            self.last_status_refresh = std::time::Instant::now();
        }

        self.maybe_sync_foreign_app_menu();

        // Update lock screen widget with current password field state (i18n strings).
        if self.locked {
            let locale = shell_locale_prefs();
            let mut layout = Layout::vertical(12.0);
            layout.add(Box::new(Label::new("SLOPOS-I")));
            layout.add(Box::new(Label::new("")));
            layout.add(Box::new(Label::new(tr("lock.prompt", &locale.locale))));

            // Add a copy of the password field for display
            let mut field = TextField::new().with_placeholder(tr("lock.prompt", &locale.locale));
            field.is_password = true;
            field.set_text(self.lock_password_field.text());
            layout.add(Box::new(field));

            if let Some(ref error) = self.lock_error_message {
                let msg = if error.contains("Incorrect") {
                    tr("lock.error", &locale.locale)
                } else {
                    error.clone()
                };
                layout.add(Box::new(Label::new(msg)));
            }

            self.lock_screen_widget
                .set_content(Box::new(LayoutView::new(layout)));
        }

        self.menu_bar.menus = self.menu_server.read().menus.clone();

        if let Some(action) = self.menu_bar.last_action.take() {
            tracing::info!("Menu action: {action}");
            self.handle_menu_action(&action);
        }

        // Sync DockView items from shared Dock
        let dock_read = self.dock.read();
        let mut dock_view_items = Vec::new();
        for item in &dock_read.items {
            dock_view_items.push(slopos_kit::dock_view::DockViewItem {
                label: item.label.clone(),
                icon: item.icon.clone().unwrap_or_default(),
                is_focused: item.state == crate::dock::AppState::Focused,
                is_running: item.state == crate::dock::AppState::Running
                    || item.state == crate::dock::AppState::Focused,
            });
        }
        self.dock_view.items = dock_view_items;

        // Expire old notifications (older than 5 seconds)
        {
            self.notification_center
                .write()
                .clear_expired(std::time::Duration::from_secs(5));
        }

        // Rebuild notification popup windows from currently visible notifications
        let notifications: Vec<(String, String)> = self
            .notification_center
            .read()
            .visible()
            .into_iter()
            .map(|n| (n.title.clone(), n.message.clone()))
            .collect();

        self.notification_popup_windows.clear();
        let right_margin = 12.0;
        let popup_w = 280.0;
        let popup_h = 80.0;
        let menu_bar_h = MENU_BAR_HEIGHT;
        let gap = 8.0;
        let desktop_width = self.rect().width;

        for (i, (title, message)) in notifications.iter().enumerate() {
            let x = desktop_width - popup_w - right_margin;
            let y = menu_bar_h + gap + i as f32 * (popup_h + gap);
            let rect = Rect::new(x, y, popup_w, popup_h);

            let mut layout = Layout::vertical(4.0);
            layout.add(Box::new(Label::new(format!("[!] {title}"))));
            layout.add(Box::new(Label::new(message.clone())));

            let mut popup = Window::new(title.as_str());
            popup.set_content(Box::new(LayoutView::new(layout)));
            popup.set_rect(rect);
            let _ = popup.layout(LayoutConstraint::tight(Size::new(popup_w, popup_h)));

            self.notification_popup_windows.push(popup);
        }

        // Export the current Spaces overview metadata to the retained AT-SPI
        // object graph. The kit compares the semantic snapshot and performs
        // no D-Bus churn when nothing changed; when the overview opens,
        // closes, or changes selection, assistive technologies receive the
        // same stable list/item structure used by the live widget.
        self.sync_accessibility_tree();
    }

    fn children(&self) -> Vec<&dyn Widget> {
        if self.locked {
            return vec![&self.lock_screen_widget as &dyn Widget];
        }
        // During layer-shell input dispatch, use the surface that received the
        // event as the routing scope. The paint filter remains unchanged so a
        // menu/dock widget is never accidentally drawn into the background
        // surface just because it received input there.
        let active_filter = self.input_filter.unwrap_or(self.paint_filter);
        match active_filter {
            ShellPaintFilter::MenuBar | ShellPaintFilter::MenuPopup => {
                return vec![&self.menu_bar as &dyn Widget];
            }
            ShellPaintFilter::SpacesOverview => {
                return self
                    .workspace_overview
                    .as_ref()
                    .map(|overview| vec![overview as &dyn Widget])
                    .unwrap_or_default();
            }
            ShellPaintFilter::Dock => return vec![&self.dock_view as &dyn Widget],
            ShellPaintFilter::Background | ShellPaintFilter::All => {}
        }
        let shell_window_count = if self.compositor_owns_ordinary_windows() {
            0
        } else {
            self.windows.len()
        };
        let capacity = shell_window_count + 3 + self.notification_popup_windows.len();
        let mut children: Vec<&dyn Widget> = Vec::with_capacity(capacity);
        children.push(&self.desktop);
        let active_workspace = self.active_workspace();
        if !self.compositor_owns_ordinary_windows() {
            for shell_window in &self.windows {
                if shell_window.workspace == active_workspace {
                    children.push(&shell_window.window);
                }
            }
        }
        if !self.compositor_owns_ordinary_windows() {
            if let Some(overview) = self.workspace_overview.as_ref() {
                children.push(overview as &dyn Widget);
            }
        }
        if matches!(active_filter, ShellPaintFilter::All)
            && (self.input_filter.is_some() || should_paint_kit_chrome(self.layer_shell_bound))
        {
            children.push(&self.dock_view);
        }
        for popup in &self.notification_popup_windows {
            children.push(popup as &dyn Widget);
        }
        if matches!(active_filter, ShellPaintFilter::All)
            && (self.input_filter.is_some() || should_paint_kit_chrome(self.layer_shell_bound))
        {
            children.push(&self.menu_bar);
        }
        if self.spotlight_ui.is_visible()
            && matches!(
                active_filter,
                ShellPaintFilter::Background | ShellPaintFilter::All
            )
        {
            children.push(&self.spotlight_ui.scrim);
            children.push(&self.spotlight_ui.card);
            children.push(&self.spotlight_ui.search_field);
            children.push(&self.spotlight_ui.results_list);
        }
        children
    }

    fn children_mut(&mut self) -> Vec<&mut dyn Widget> {
        if self.locked {
            return vec![&mut self.lock_screen_widget as &mut dyn Widget];
        }
        let active_filter = self.input_filter.unwrap_or(self.paint_filter);
        match active_filter {
            ShellPaintFilter::MenuBar | ShellPaintFilter::MenuPopup => {
                return vec![&mut self.menu_bar as &mut dyn Widget];
            }
            ShellPaintFilter::SpacesOverview => {
                return self
                    .workspace_overview
                    .as_mut()
                    .map(|overview| vec![overview as &mut dyn Widget])
                    .unwrap_or_default();
            }
            ShellPaintFilter::Dock => return vec![&mut self.dock_view as &mut dyn Widget],
            ShellPaintFilter::Background | ShellPaintFilter::All => {}
        }
        let paint_chrome = matches!(active_filter, ShellPaintFilter::All)
            && (self.input_filter.is_some() || should_paint_kit_chrome(self.layer_shell_bound));
        let compositor_owns_windows = self.compositor_owns_ordinary_windows();
        let shell_window_count = if compositor_owns_windows {
            0
        } else {
            self.windows.len()
        };
        let capacity = shell_window_count + 3 + self.notification_popup_windows.len();
        let mut children: Vec<&mut dyn Widget> = Vec::with_capacity(capacity);
        children.push(&mut self.desktop);
        let active_workspace = self.workspace_manager.read().active;
        if !compositor_owns_windows {
            for shell_window in &mut self.windows {
                if shell_window.workspace == active_workspace {
                    children.push(&mut shell_window.window);
                }
            }
        }
        if !compositor_owns_windows {
            if let Some(overview) = self.workspace_overview.as_mut() {
                children.push(overview as &mut dyn Widget);
            }
        }
        if paint_chrome {
            children.push(&mut self.dock_view);
        }
        for popup in &mut self.notification_popup_windows {
            children.push(popup as &mut dyn Widget);
        }
        if paint_chrome {
            children.push(&mut self.menu_bar);
        }
        if self.spotlight_ui.is_visible()
            && matches!(
                active_filter,
                ShellPaintFilter::Background | ShellPaintFilter::All
            )
        {
            children.push(&mut self.spotlight_ui.scrim);
            children.push(&mut self.spotlight_ui.card);
            children.push(&mut self.spotlight_ui.search_field);
            children.push(&mut self.spotlight_ui.results_list);
        }
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
    use slopos_kit::event::Modifiers;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    static MENU_MANIFEST_ENV_LOCK: Mutex<()> = Mutex::new(());
    static LOCK_PASSWORD_ENV_LOCK: Mutex<()> = Mutex::new(());
    static SESSION_RUNTIME_ENV_LOCK: Mutex<()> = Mutex::new(());

    struct MenuManifestEnvGuard {
        previous: Option<std::ffi::OsString>,
        directory: std::path::PathBuf,
    }

    impl MenuManifestEnvGuard {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let directory =
                std::env::temp_dir().join(format!("slopos-i_menu_manifest_test_{unique}"));
            fs::create_dir_all(&directory).unwrap();
            let previous = std::env::var_os("SLOPOS_MENU_MANIFEST_DIR");
            std::env::set_var("SLOPOS_MENU_MANIFEST_DIR", &directory);
            Self {
                previous,
                directory,
            }
        }
    }

    impl Drop for MenuManifestEnvGuard {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(previous) => std::env::set_var("SLOPOS_MENU_MANIFEST_DIR", previous),
                None => std::env::remove_var("SLOPOS_MENU_MANIFEST_DIR"),
            }
            let _ = fs::remove_dir_all(&self.directory);
        }
    }

    #[test]
    fn foreign_toplevel_matching_accepts_only_known_or_exact_bundle_ids() {
        assert!(ShellDesktop::foreign_toplevel_matches_bundle(
            "com.slopos.settings",
            "com.slopos.settings"
        ));
        assert!(!ShellDesktop::foreign_toplevel_matches_bundle(
            "SETTINGS",
            "com.slopos.settings"
        ));
        assert!(!ShellDesktop::foreign_toplevel_matches_bundle(
            "com.slopos.settings-malicious",
            "com.slopos.settings"
        ));
        assert!(!ShellDesktop::foreign_toplevel_matches_bundle(
            "/tmp/settings",
            "com.slopos.settings"
        ));
        assert_eq!(
            ShellDesktop::dock_activation_for_existing_client(true),
            DockActivation::ActivateExisting
        );
        assert_eq!(
            ShellDesktop::dock_activation_for_existing_client(false),
            DockActivation::LaunchNew
        );
    }

    fn temp_shell_root() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("slopos-i_shell_folder_{unique}"));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn test_desktop() -> (ShellDesktop, Arc<RwLock<WindowManager>>) {
        let menu_server = Arc::new(RwLock::new(MenuServer::new()));
        let launch_services = Arc::new(RwLock::new(LaunchServices::new()));
        let window_manager = Arc::new(RwLock::new(WindowManager::new()));
        let notification_center = Arc::new(RwLock::new(NotificationCenter::new()));
        let workspace_manager = Arc::new(RwLock::new(WorkspaceManager::new()));
        let dock = Arc::new(RwLock::new(Dock::new()));
        let session_manager = Arc::new(RwLock::new(SessionManager::new()));
        let mut desktop = ShellDesktop::new(
            menu_server,
            launch_services,
            window_manager.clone(),
            notification_center,
            workspace_manager,
            dock,
            session_manager,
        );
        desktop.layout(LayoutConstraint::tight(Size::new(960.0, 640.0)));
        // Production no longer creates a fake Finder window in the shell.
        // Keep the legacy in-process window coverage explicit in unit tests
        // that exercise shell-owned dialog/window policy.
        desktop.open_finder_window();
        // Unit tests: plan MIME open / nmcli without spawning real processes.
        desktop.mime_open_spawn = false;
        desktop.network_connect_spawn = false;
        (desktop, window_manager)
    }

    fn assert_rect_eq(actual: Rect, expected: Rect) {
        assert_eq!(actual.x, expected.x);
        assert_eq!(actual.y, expected.y);
        assert_eq!(actual.width, expected.width);
        assert_eq!(actual.height, expected.height);
    }

    fn rect_eq(left: Rect, right: Rect) -> bool {
        left.x == right.x
            && left.y == right.y
            && left.width == right.width
            && left.height == right.height
    }

    fn message_window_lines(window: &ShellWindow) -> Vec<String> {
        let layout_view = window
            .window
            .content
            .as_ref()
            .and_then(|content| content.as_any().downcast_ref::<LayoutView>())
            .expect("message window uses layout view");
        let Layout::Vertical { children, .. } = &layout_view.layout else {
            panic!("message window uses vertical layout");
        };
        children
            .iter()
            .filter_map(|child| {
                child
                    .as_any()
                    .downcast_ref::<Label>()
                    .map(|l| l.text.clone())
            })
            .collect()
    }

    fn icon_item_center(window: &ShellWindow, label: &str) -> Point {
        let icon_view = window
            .window
            .content
            .as_ref()
            .and_then(|content| content.as_any().downcast_ref::<IconView>())
            .expect("shell folder window has icon content");
        let rect = icon_view
            .items
            .iter()
            .find(|item| item.label == label)
            .expect("folder item exists")
            .rect;
        Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
    }

    #[test]
    fn default_finder_rect_stays_inside_shell() {
        let shell = Rect::new(0.0, 0.0, 960.0, 640.0);
        let rect = default_finder_rect(shell);

        assert!(rect.x >= shell.x);
        assert!(rect.y >= shell.y);
        assert!(rect.x + rect.width <= shell.x + shell.width);
        assert!(rect.y + rect.height <= shell.y + shell.height);
        assert!(rect.width >= 360.0);
        assert!(rect.height >= 260.0);
    }

    #[test]
    fn clamp_window_rect_keeps_window_visible() {
        let bounds = Rect::new(0.0, 24.0, 960.0, 616.0);
        let rect = Rect::new(-200.0, 900.0, 420.0, 280.0);
        let clamped = clamp_window_rect(rect, bounds);

        assert_eq!(clamped.x, bounds.x);
        assert_eq!(clamped.y, bounds.y + bounds.height - clamped.height);
        assert_eq!(clamped.width, rect.width);
        assert_eq!(clamped.height, rect.height);
    }

    #[test]
    fn resize_handle_tracks_bottom_right_corner() {
        let window = Rect::new(66.0, 66.0, 500.0, 300.0);
        let handle = resize_handle_rect(window);

        assert!(handle.contains(Point::new(565.0, 365.0)));
        assert!(!handle.contains(Point::new(540.0, 340.0)));
    }

    #[test]
    fn folder_items_sort_directories_first_and_hide_dotfiles() {
        let root = temp_shell_root();
        fs::create_dir_all(root.join("Folder")).unwrap();
        fs::write(root.join("note.txt"), "hello").unwrap();
        fs::write(root.join(".hidden"), "secret").unwrap();

        let items = folder_items_for_path(&root);

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].label, "Folder");
        assert_eq!(items[0].icon.as_deref(), Some("folder"));
        assert_eq!(items[1].label, "note.txt");
        assert_eq!(items[1].icon.as_deref(), Some("document"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn desktop_home_icon_opens_managed_folder_window() {
        let (mut desktop, window_manager) = test_desktop();
        let initial_count = desktop.windows.len();
        let home_index = desktop
            .desktop
            .items
            .iter()
            .position(|item| item.label == "Home")
            .expect("home desktop icon exists");

        desktop.launch_item(home_index);

        assert_eq!(desktop.windows.len(), initial_count + 1);
        let active = desktop.windows.last().expect("active home window");
        assert_eq!(active.window.title(), "Home");
        assert_eq!(window_manager.read().active_window, Some(active.id));
    }

    #[test]
    fn shell_global_menu_switches_to_focused_finder_window() {
        let (mut desktop, _) = test_desktop();

        let titles = desktop
            .menu_bar
            .menus
            .iter()
            .map(|menu| menu.title.as_str())
            .collect::<Vec<_>>();

        assert!(titles.contains(&"SLOPOS"));
        assert!(titles.contains(&"Finder"));
        assert_eq!(
            desktop.menu_server.read().active_app.as_deref(),
            Some("com.slopos.finder")
        );

        let second_id = desktop.open_finder_window();
        desktop.focus_window(second_id);
        let titles = desktop
            .menu_bar
            .menus
            .iter()
            .map(|menu| menu.title.as_str())
            .collect::<Vec<_>>();
        assert!(titles.contains(&"Finder"));
        assert!(titles.contains(&"Go"));
    }

    #[test]
    fn shell_global_menu_switches_to_launched_sdk_app() {
        let _guard = MENU_MANIFEST_ENV_LOCK.lock().unwrap();
        std::env::remove_var("SLOPOS_MENU_MANIFEST_DIR");
        let (mut desktop, _) = test_desktop();

        desktop.activate_app_menu("com.slopos.textedit");

        let titles = desktop
            .menu_bar
            .menus
            .iter()
            .map(|menu| menu.title.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            desktop.menu_server.read().active_app.as_deref(),
            Some("com.slopos.textedit")
        );
        assert!(titles.contains(&"TextEdit"));
        assert!(titles.contains(&"File"));
        assert!(titles.contains(&"Edit"));
    }

    #[test]
    fn shell_global_menu_uses_loaded_sdk_manifest_for_active_app() {
        let _guard = MENU_MANIFEST_ENV_LOCK.lock().unwrap();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("slopos-i_menu_manifest_shell_{unique}"));
        fs::create_dir_all(&dir).unwrap();
        std::env::set_var("SLOPOS_MENU_MANIFEST_DIR", &dir);

        let mut textedit_file = slopos_kit::menu::Menu::new("File");
        textedit_file
            .add_action("Save As...")
            .with_action("com.slopos.textedit.file.save_as");
        let manifest = slopos_sdk::MenuManifest {
            app_name: "TextEdit".to_string(),
            bundle_id: "com.slopos.textedit".to_string(),
            menus: vec![textedit_file],
            updated_at_millis: 1,
        };
        fs::write(
            dir.join("com_retro_textedit.json"),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let (mut desktop, _) = test_desktop();
        desktop.activate_app_menu("com.slopos.textedit");

        assert_eq!(
            desktop.menu_server.read().active_app.as_deref(),
            Some("com.slopos.textedit")
        );
        assert_eq!(
            desktop
                .menu_bar
                .menus
                .iter()
                .find(|menu| menu.title == "File")
                .unwrap()
                .items[0]
                .action_id,
            "com.slopos.textedit.file.save_as"
        );

        let _ = fs::remove_dir_all(&dir);
        std::env::remove_var("SLOPOS_MENU_MANIFEST_DIR");
    }

    #[test]
    fn loaded_sdk_menu_action_opens_visible_dispatch_status() {
        let _guard = MENU_MANIFEST_ENV_LOCK.lock().unwrap();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("slopos-i_menu_action_shell_{unique}"));
        fs::create_dir_all(&dir).unwrap();
        std::env::set_var("SLOPOS_MENU_MANIFEST_DIR", &dir);

        let mut textedit_file = slopos_kit::menu::Menu::new("File");
        textedit_file
            .add_action("Save As...")
            .with_action("com.slopos.textedit.file.save_as");
        let manifest = slopos_sdk::MenuManifest {
            app_name: "TextEdit".to_string(),
            bundle_id: "com.slopos.textedit".to_string(),
            menus: vec![textedit_file],
            updated_at_millis: 1,
        };
        fs::write(
            dir.join("com_retro_textedit.json"),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let (mut desktop, _) = test_desktop();
        desktop.activate_app_menu("com.slopos.textedit");
        desktop.handle_menu_action("com.slopos.textedit.file.save_as");

        let active = desktop.windows.last().expect("dispatch status window");
        assert_eq!(active.window.title(), "Application Menu Action");
        let lines = message_window_lines(active);
        assert!(lines.contains(&"Application: com.slopos.textedit".to_string()));
        assert!(lines.contains(&"Action: Save As...".to_string()));
        assert!(lines.contains(&"Identifier: com.slopos.textedit.file.save_as".to_string()));

        let _ = fs::remove_dir_all(&dir);
        std::env::remove_var("SLOPOS_MENU_MANIFEST_DIR");
    }

    #[test]
    fn shell_global_menu_resets_when_last_window_closes() {
        let (mut desktop, _) = test_desktop();
        let ids = desktop
            .windows
            .iter()
            .map(|window| window.id)
            .collect::<Vec<_>>();

        for id in ids {
            desktop.close_window(id);
        }

        assert!(desktop.windows.is_empty());
        assert_eq!(desktop.menu_server.read().active_app, None);
        assert!(!desktop
            .menu_bar
            .menus
            .iter()
            .any(|menu| menu.title == "Finder"));
    }

    #[test]
    fn shell_folder_window_double_click_opens_child_folder() {
        let root = temp_shell_root();
        fs::create_dir_all(root.join("Projects")).unwrap();
        fs::write(root.join("note.txt"), "hello").unwrap();
        let (mut desktop, window_manager) = test_desktop();
        let initial_count = desktop.windows.len();
        let root_id = desktop.open_folder_window("Root", root.clone());
        let index = desktop.window_index(root_id).unwrap();
        let point = icon_item_center(&desktop.windows[index], "Projects");

        let result = desktop.handle_event(&Event::DoubleClick {
            button: MouseButton::Left,
            point,
            modifiers: Modifiers::NONE,
        });

        assert!(matches!(result, EventResult::Handled));
        assert_eq!(desktop.windows.len(), initial_count + 2);
        let active = desktop.windows.last().expect("child folder window");
        assert_eq!(active.window.title(), "Projects");
        assert_eq!(
            active.folder_path.as_deref(),
            Some(root.join("Projects").as_path())
        );
        assert_eq!(window_manager.read().active_window, Some(active.id));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn shell_folder_window_double_click_file_does_not_open_window() {
        let root = temp_shell_root();
        fs::write(root.join("note.txt"), "hello").unwrap();
        let (mut desktop, _) = test_desktop();
        let root_id = desktop.open_folder_window("Root", root.clone());
        let index = desktop.window_index(root_id).unwrap();
        let point = icon_item_center(&desktop.windows[index], "note.txt");
        let initial_count = desktop.windows.len();

        let result = desktop.handle_event(&Event::DoubleClick {
            button: MouseButton::Left,
            point,
            modifiers: Modifiers::NONE,
        });

        assert!(matches!(result, EventResult::Handled));
        // MIME open is external process — no new shell-managed window.
        assert_eq!(desktop.windows.len(), initial_count);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn shell_folder_window_double_click_file_plans_mime_open_textedit() {
        let root = temp_shell_root();
        let note = root.join("note.txt");
        fs::write(&note, "hello").unwrap();
        let (mut desktop, _) = test_desktop();
        assert!(!desktop.mime_open_spawn, "tests must not spawn GUI");
        let root_id = desktop.open_folder_window("Root", root.clone());
        let index = desktop.window_index(root_id).unwrap();
        let point = icon_item_center(&desktop.windows[index], "note.txt");
        let initial_count = desktop.windows.len();

        let result = desktop.handle_event(&Event::DoubleClick {
            button: MouseButton::Left,
            point,
            modifiers: Modifiers::NONE,
        });

        assert!(matches!(result, EventResult::Handled));
        assert_eq!(desktop.windows.len(), initial_count);
        let plan = desktop
            .last_mime_open
            .as_ref()
            .expect("MIME open plan recorded");
        assert_eq!(plan.app_id, "com.slopos.textedit");
        assert_eq!(
            spawn_argv(plan),
            vec!["textedit".to_string(), note.to_string_lossy().into_owned()]
        );
        // No live spawn in unit tests.
        assert_eq!(desktop.session_clients.len(), 0);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn shell_folder_window_double_click_unknown_file_records_no_handler() {
        let root = temp_shell_root();
        fs::write(root.join("blob.bin"), [0u8, 1, 2]).unwrap();
        let (mut desktop, _) = test_desktop();
        let root_id = desktop.open_folder_window("Root", root.clone());
        let index = desktop.window_index(root_id).unwrap();
        let point = icon_item_center(&desktop.windows[index], "blob.bin");

        let result = desktop.handle_event(&Event::DoubleClick {
            button: MouseButton::Left,
            point,
            modifiers: Modifiers::NONE,
        });

        assert!(matches!(result, EventResult::Handled));
        assert!(desktop.last_mime_open.is_none());
        let err = desktop.last_error.as_deref().unwrap_or("");
        assert!(err.contains("no handler"), "{err}");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn classic_titlebar_controls_match_drawn_chrome() {
        let window = Rect::new(66.0, 66.0, 500.0, 300.0);

        assert!(close_box_rect(window).contains(Point::new(78.0, 78.0)));
        assert!(minimize_box_rect(window).contains(Point::new(92.0, 78.0)));
        assert!(!close_box_rect(window).contains(Point::new(92.0, 78.0)));
        assert!(zoom_box_rect(window).contains(Point::new(554.0, 78.0)));
        assert!(!titlebar_rect(window).contains(Point::new(554.0, 96.0)));
    }

    #[test]
    fn mutable_pointer_routing_uses_active_layer_input_filter() {
        let (mut desktop, _) = test_desktop();
        desktop.set_layer_shell_bound(true);
        desktop.prepare_dock_strip_layout(960.0, 80.0);
        desktop.set_paint_filter(ShellPaintFilter::Background);
        desktop.set_input_filter(Some(ShellPaintFilter::Dock));

        let children = desktop.children_mut();

        assert_eq!(children.len(), 1);
        assert_rect_eq(children[0].rect(), Rect::new(0.0, 0.0, 960.0, 80.0));
    }

    #[test]
    fn shell_menu_actions_create_and_close_managed_windows() {
        let (mut desktop, window_manager) = test_desktop();

        assert_eq!(desktop.windows.len(), 1);
        let first_id = desktop.windows[0].id;
        assert_eq!(window_manager.read().active_window, Some(first_id));

        desktop.handle_menu_action("shell.new_finder_window");
        assert_eq!(desktop.windows.len(), 2);
        let second_id = desktop.windows[1].id;
        assert_ne!(first_id, second_id);
        assert_eq!(window_manager.read().active_window, Some(second_id));

        desktop.handle_menu_action("shell.close_finder_window");
        assert_eq!(desktop.windows.len(), 1);
        assert_eq!(desktop.windows[0].id, first_id);
        assert_eq!(window_manager.read().active_window, Some(first_id));
    }

    #[test]
    fn workspace_switch_hides_windows_from_other_workspaces() {
        let (mut desktop, window_manager) = test_desktop();
        let first_id = desktop.windows[0].id;

        assert_eq!(desktop.active_workspace(), 0);
        assert_eq!(desktop.children().len(), 4);

        desktop.handle_menu_action("workspace.switch.1");
        assert_eq!(desktop.active_workspace(), 1);
        assert_ne!(desktop.active_window_id(), Some(first_id));
        assert_eq!(
            window_manager.read().active_window,
            Some(desktop.windows.last().unwrap().id)
        );
        assert_eq!(desktop.windows.last().unwrap().window.title(), "Workspace");
        assert_eq!(desktop.windows.last().unwrap().workspace, 1);
        assert!(desktop
            .children()
            .iter()
            .any(|child| rect_eq(child.rect(), desktop.windows.last().unwrap().window.rect())));

        desktop.handle_menu_action("workspace.switch.0");
        assert_eq!(desktop.active_workspace(), 0);
        assert!(desktop.windows.iter().any(|window| window.id == first_id));
        assert!(desktop
            .children()
            .iter()
            .any(|child| rect_eq(child.rect(), desktop.windows[0].window.rect())));
    }

    #[test]
    fn workspace_shortcut_actions_cycle_active_workspace() {
        let (mut desktop, _) = test_desktop();

        desktop.handle_menu_action("workspace.next");
        assert_eq!(desktop.active_workspace(), 1);

        desktop.handle_menu_action("workspace.previous");
        assert_eq!(desktop.active_workspace(), 0);
    }

    #[test]
    fn compositor_thumbnail_loader_accepts_bounded_png_and_rejects_missing_or_symlink() {
        let root = temp_shell_root();
        let path = root.join("spaces-thumbnail-1.png");
        let image = image::RgbaImage::from_pixel(2, 1, image::Rgba([1, 2, 3, 255]));
        image.save_with_format(&path, ImageFormat::Png).unwrap();

        let decoded = load_space_thumbnail_path(&path).expect("valid compositor PNG");
        assert_eq!(decoded.width(), 2);
        assert_eq!(decoded.height(), 1);
        assert_eq!(decoded.pixels(), &[1, 2, 3, 255, 1, 2, 3, 255]);
        assert!(load_space_thumbnail_path(&root.join("missing.png")).is_none());

        #[cfg(unix)]
        {
            let symlink = root.join("spaces-thumbnail-link.png");
            std::os::unix::fs::symlink(&path, &symlink).unwrap();
            assert!(
                load_space_thumbnail_path(&symlink).is_none(),
                "shell must not follow a thumbnail symlink"
            );
        }

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn compositor_thumbnail_loader_rejects_non_png_and_oversized_dimensions() {
        let root = temp_shell_root();
        let non_png = root.join("not-png.png");
        fs::write(&non_png, b"not a PNG").unwrap();
        assert!(load_space_thumbnail_path(&non_png).is_none());

        let oversized = root.join("oversized.png");
        let image = image::RgbaImage::from_pixel(
            MAX_SPACE_THUMBNAIL_WIDTH + 1,
            1,
            image::Rgba([0, 0, 0, 255]),
        );
        image
            .save_with_format(&oversized, ImageFormat::Png)
            .unwrap();
        assert!(load_space_thumbnail_path(&oversized).is_none());

        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn thumbnail_loader_requires_the_current_atomic_manifest() {
        let _environment_guard = SESSION_RUNTIME_ENV_LOCK.lock().unwrap();
        let root = temp_shell_root();
        let previous_runtime = std::env::var_os("SLOPOS_SESSION_RUNTIME_DIR");
        std::env::set_var("SLOPOS_SESSION_RUNTIME_DIR", &root);

        let path = root.join("spaces-thumbnail-1.png");
        image::RgbaImage::from_pixel(2, 1, image::Rgba([9, 8, 7, 255]))
            .save_with_format(&path, ImageFormat::Png)
            .unwrap();
        slopos_bus::write_space_thumbnail_manifest(&slopos_bus::SpaceThumbnailManifest {
            session_epoch: 3,
            generation: 4,
            captures: vec![slopos_bus::SpaceThumbnailEntry {
                space_id: 1,
                width: 2,
                height: 1,
            }],
        })
        .unwrap();

        let loaded = load_space_thumbnails(&[1, 2], 3, 4);
        assert!(loaded[0].is_some(), "manifest-listed capture is accepted");
        assert!(loaded[1].is_none(), "unlisted Space has no capture");

        slopos_bus::write_space_thumbnail_manifest(&slopos_bus::SpaceThumbnailManifest {
            session_epoch: 3,
            generation: 5,
            captures: Vec::new(),
        })
        .unwrap();
        assert!(load_space_thumbnails(&[1], 3, 5)[0].is_none());

        slopos_bus::write_space_thumbnail_manifest(&slopos_bus::SpaceThumbnailManifest {
            session_epoch: 99,
            generation: 4,
            captures: vec![slopos_bus::SpaceThumbnailEntry {
                space_id: 1,
                width: 2,
                height: 1,
            }],
        })
        .unwrap();
        assert!(
            load_space_thumbnails(&[1], 3, 4)[0].is_none(),
            "captures from a prior compositor session must not be reused"
        );

        if let Some(previous_runtime) = previous_runtime {
            std::env::set_var("SLOPOS_SESSION_RUNTIME_DIR", previous_runtime);
        } else {
            std::env::remove_var("SLOPOS_SESSION_RUNTIME_DIR");
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn live_workspace_switch_sends_stable_id_without_optimistic_mirror_mutation() {
        use slopos_bus::{SessionControlListener, SessionControlRequest};

        let _environment_guard = SESSION_RUNTIME_ENV_LOCK.lock().unwrap();
        let runtime = std::env::temp_dir().join(format!(
            "slo-shell-ws-{}-{}",
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

        let (mut desktop, _) = test_desktop();
        desktop.set_layer_shell_bound(true);
        assert!(desktop.switch_workspace(2));
        assert_eq!(desktop.active_workspace(), 0);
        assert_eq!(
            listener.drain(),
            vec![SessionControlRequest::Spaces {
                command: slopos_bus::SpacesControlCommand::Select { id: 3 },
            }]
        );

        if let Some(previous_runtime) = previous_runtime {
            std::env::set_var("SLOPOS_SESSION_RUNTIME_DIR", previous_runtime);
        } else {
            std::env::remove_var("SLOPOS_SESSION_RUNTIME_DIR");
        }
        drop(listener);
        fs::remove_dir_all(runtime).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn failed_live_workspace_switch_keeps_local_mirror_unchanged() {
        let _environment_guard = SESSION_RUNTIME_ENV_LOCK.lock().unwrap();
        let runtime = std::env::temp_dir().join(format!(
            "slo-shell-ws-missing-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let previous_runtime = std::env::var_os("SLOPOS_SESSION_RUNTIME_DIR");
        std::env::set_var("SLOPOS_SESSION_RUNTIME_DIR", &runtime);

        let (mut desktop, _) = test_desktop();
        desktop.set_layer_shell_bound(true);
        assert!(!desktop.switch_workspace(2));
        assert_eq!(desktop.active_workspace(), 0);

        if let Some(previous_runtime) = previous_runtime {
            std::env::set_var("SLOPOS_SESSION_RUNTIME_DIR", previous_runtime);
        } else {
            std::env::remove_var("SLOPOS_SESSION_RUNTIME_DIR");
        }
    }

    #[test]
    fn about_menu_opens_real_message_window() {
        let (mut desktop, window_manager) = test_desktop();

        desktop.handle_menu_action("shell.about");

        let active = desktop.windows.last().expect("about window");
        assert_eq!(active.window.title(), "About SLOPOS-I");
        assert_eq!(active.folder_path, None);
        assert_eq!(window_manager.read().active_window, Some(active.id));
        let lines = message_window_lines(active);
        assert!(lines[0].contains("SLOPOS-I"));
        assert!(lines
            .iter()
            .any(|line| line.contains("Classic Desktop Environment")));
    }

    #[test]
    fn notification_center_lists_and_clears_active_notifications() {
        let (mut desktop, _) = test_desktop();

        let id = desktop.record_notification(
            "com.slopos.textedit",
            "Document Saved",
            "note.txt was written to disk.",
            NotificationPriority::Normal,
        );

        assert_eq!(id, "notif-0");
        assert_eq!(desktop.notification_center.read().visible().len(), 1);

        desktop.handle_menu_action("shell.notification_center");
        let active = desktop.windows.last().expect("notification center window");
        assert_eq!(active.window.title(), "Notification Center");
        let lines = message_window_lines(active);
        assert!(lines
            .iter()
            .any(|line| line.contains("notif-0 - Document Saved")));
        assert!(lines
            .iter()
            .any(|line| line.contains("App: com.slopos.textedit")));

        desktop.handle_menu_action("shell.clear_notifications");
        assert!(desktop.notification_center.read().visible().is_empty());
        let active = desktop.windows.last().expect("clear confirmation");
        assert_eq!(active.window.title(), "Notification Center");
        assert!(message_window_lines(active)
            .iter()
            .any(|line| line.contains("dismissed")));
    }

    #[test]
    fn get_info_menu_opens_folder_metadata_window() {
        let root = temp_shell_root();
        fs::write(root.join("note.txt"), "hello").unwrap();
        let (mut desktop, window_manager) = test_desktop();
        desktop.open_folder_window("Root", root.clone());

        desktop.handle_menu_action("finder.get_info");

        let active = desktop.windows.last().expect("info window");
        assert_eq!(active.window.title(), "Root Info");
        assert_eq!(active.folder_path, None);
        assert_eq!(window_manager.read().active_window, Some(active.id));
        let lines = message_window_lines(active);
        assert!(lines.contains(&"Name: Root".to_string()));
        assert!(lines.contains(&"Kind: Folder".to_string()));
        assert!(lines.contains(&format!("Location: {}", root.display())));
        assert!(lines.contains(&"Items: 1".to_string()));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn force_quit_menu_opens_running_window_list() {
        let (mut desktop, window_manager) = test_desktop();

        desktop.handle_menu_action("shell.force_quit");

        let active = desktop.windows.last().expect("force quit window");
        assert_eq!(active.window.title(), "Force Quit");
        assert_eq!(active.folder_path, None);
        assert_eq!(window_manager.read().active_window, Some(active.id));

        let layout_view = active
            .window
            .content
            .as_deref()
            .and_then(|c| c.as_any().downcast_ref::<LayoutView>())
            .expect("uses layout view");
        if let Layout::Vertical { children, .. } = &layout_view.layout {
            let label = children[0].as_any().downcast_ref::<Label>().expect("label");
            assert_eq!(
                label.text,
                "Shell windows, session clients, and compositor foreign-toplevels:"
            );
            let list = children[1]
                .as_any()
                .downcast_ref::<ListView>()
                .expect("list");
            assert!(list
                .items
                .iter()
                .any(|item| item == "window: SLOPOS-I" || item.contains("SLOPOS-I")));
        } else {
            panic!("not vertical layout");
        }
    }

    #[test]
    fn force_quit_apply_closes_listed_shell_window() {
        let (mut desktop, _) = test_desktop();
        // test_desktop opens a Finder-style "SLOPOS-I" window among others.
        let before = desktop.windows.len();
        assert!(
            desktop
                .windows
                .iter()
                .any(|w| w.window.title() == "SLOPOS-I"),
            "precondition: SLOPOS-I window present"
        );

        // Drive the same path Force Quit button uses (shipped apply helper).
        assert!(desktop.apply_force_quit_entry("window: SLOPOS-I"));
        assert!(
            !desktop
                .windows
                .iter()
                .any(|w| w.window.title() == "SLOPOS-I"),
            "SLOPOS-I must be closed after force quit"
        );
        assert!(desktop.windows.len() < before);
    }

    #[test]
    fn force_quit_apply_kills_registered_client_pid() {
        let (mut desktop, _) = test_desktop();
        desktop
            .session_clients
            .register(session_clients::ExternalClient {
                bundle_id: "com.slopos.finder".into(),
                binary_name: "finder".into(),
                pid: 424_242,
                child: None,
                launched_at_unix: 1,
            });
        assert_eq!(desktop.session_clients.len(), 1);
        assert!(desktop.apply_force_quit_entry("client: finder (pid 424242)"));
        assert_eq!(desktop.session_clients.len(), 0);
    }

    #[test]
    fn help_search_menu_opens_status_window() {
        let (mut desktop, _) = test_desktop();

        desktop.handle_menu_action("shell.help_search");

        let active = desktop.windows.last().expect("help window");
        assert_eq!(active.window.title(), "Help");
        let lines = message_window_lines(active);
        assert!(lines.iter().any(|line| line.contains("not indexed yet")));
    }

    #[test]
    fn focusing_window_raises_it_to_front() {
        let (mut desktop, window_manager) = test_desktop();
        let first_id = desktop.windows[0].id;
        let second_id = desktop.open_finder_window();

        desktop.focus_window(first_id);

        assert_eq!(desktop.active_window_id(), Some(first_id));
        assert_eq!(
            desktop.windows.last().map(|window| window.id),
            Some(first_id)
        );
        assert_eq!(window_manager.read().active_window, Some(first_id));
        assert_ne!(
            desktop.windows.last().map(|window| window.id),
            Some(second_id)
        );
    }

    #[test]
    fn close_box_closes_the_clicked_window() {
        let (mut desktop, window_manager) = test_desktop();
        let first_id = desktop.windows[0].id;
        let point = Point::new(78.0, 78.0);

        let result = desktop.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point,
            modifiers: slopos_kit::event::Modifiers::NONE,
        });

        assert!(matches!(result, EventResult::Handled));
        assert!(desktop.windows.is_empty());
        assert!(!window_manager.read().windows.contains_key(&first_id));
    }

    #[test]
    fn zoom_box_toggles_managed_window_between_zoomed_and_restored() {
        let (mut desktop, window_manager) = test_desktop();
        let id = desktop.windows[0].id;
        let original = desktop.windows[0].window.rect();
        let point = Point::new(original.x + original.width - 14.0, original.y + 12.0);

        let result = desktop.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point,
            modifiers: slopos_kit::event::Modifiers::NONE,
        });

        assert!(matches!(result, EventResult::Handled));
        assert!(desktop.windows[0].restore_rect.is_some());
        assert_rect_eq(desktop.windows[0].restore_rect.unwrap(), original);
        assert_eq!(
            window_manager.read().windows[&id].state,
            window_manager::WindowState::Maximized
        );
        assert!(desktop.windows[0].window.rect().width > original.width);

        let zoomed = desktop.windows[0].window.rect();
        let restore_point = Point::new(zoomed.x + zoomed.width - 14.0, zoomed.y + 12.0);
        let result = desktop.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point: restore_point,
            modifiers: slopos_kit::event::Modifiers::NONE,
        });

        assert!(matches!(result, EventResult::Handled));
        assert!(desktop.windows[0].restore_rect.is_none());
        assert_eq!(
            window_manager.read().windows[&id].state,
            window_manager::WindowState::Normal
        );
        assert_rect_eq(desktop.windows[0].window.rect(), original);
    }

    #[test]
    fn minimize_box_collapses_and_restores_managed_window() {
        let (mut desktop, window_manager) = test_desktop();
        let id = desktop.windows[0].id;
        let original = desktop.windows[0].window.rect();
        let point = Point::new(original.x + 28.0, original.y + 12.0);

        let result = desktop.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point,
            modifiers: slopos_kit::event::Modifiers::NONE,
        });

        assert!(matches!(result, EventResult::Handled));
        assert_eq!(desktop.windows[0].mode, ShellWindowMode::Minimized);
        assert_rect_eq(desktop.windows[0].restore_rect.unwrap(), original);
        assert_eq!(
            window_manager.read().windows[&id].state,
            window_manager::WindowState::Minimized
        );
        assert_eq!(desktop.windows[0].window.rect().height, 24.0);

        let minimized = desktop.windows[0].window.rect();
        let restore_point = Point::new(minimized.x + 28.0, minimized.y + 12.0);
        let result = desktop.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point: restore_point,
            modifiers: slopos_kit::event::Modifiers::NONE,
        });

        assert!(matches!(result, EventResult::Handled));
        assert_eq!(desktop.windows[0].mode, ShellWindowMode::Normal);
        assert!(desktop.windows[0].restore_rect.is_none());
        assert_eq!(
            window_manager.read().windows[&id].state,
            window_manager::WindowState::Normal
        );
        assert_rect_eq(desktop.windows[0].window.rect(), original);
    }

    /// Rect of the first descendant `Button` with `label` in a shell window,
    /// found through the widget tree (no geometry math in the test — the
    /// same tree generic dispatch walks).
    fn button_rect_in_window(window: &ShellWindow, label: &str) -> Rect {
        fn find(widget: &dyn Widget, label: &str) -> Option<Rect> {
            if let Some(button) = widget.as_any().downcast_ref::<Button>() {
                if button.label() == label {
                    return Some(button.rect());
                }
            }
            widget
                .children()
                .into_iter()
                .find_map(|child| find(child, label))
        }
        find(&window.window, label).expect("button exists in window")
    }

    fn center(rect: Rect) -> Point {
        Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
    }

    fn left_down(point: Point) -> Event {
        Event::MouseDown {
            button: MouseButton::Left,
            point,
            modifiers: Modifiers::NONE,
        }
    }

    fn left_up(point: Point) -> Event {
        Event::MouseUp {
            button: MouseButton::Left,
            point,
            modifiers: Modifiers::NONE,
        }
    }

    #[test]
    fn force_quit_cancel_closes_via_generic_dispatch() {
        let (mut desktop, _) = test_desktop();
        desktop.handle_menu_action("shell.force_quit");
        let fq_window = desktop.windows.last().expect("force quit window");
        assert_eq!(fq_window.window.title(), "Force Quit");
        let fq_id = fq_window.id;
        let cancel = center(button_rect_in_window(fq_window, "Cancel"));

        // Real button semantics: the press alone must not activate.
        let _ = desktop.handle_event(&left_down(cancel));
        assert!(
            desktop.windows.iter().any(|w| w.id == fq_id),
            "press alone must not close the dialog"
        );

        let _ = desktop.handle_event(&left_up(cancel));
        assert!(
            !desktop.windows.iter().any(|w| w.id == fq_id),
            "press + release on Cancel closes the dialog"
        );
    }

    #[test]
    fn force_quit_release_outside_button_cancels_the_press() {
        let (mut desktop, _) = test_desktop();
        desktop.handle_menu_action("shell.force_quit");
        let fq_window = desktop.windows.last().expect("force quit window");
        let fq_id = fq_window.id;
        let cancel = center(button_rect_in_window(fq_window, "Cancel"));

        let _ = desktop.handle_event(&left_down(cancel));
        // Implicit capture routes the outside release back to the pressed
        // button, which cancels instead of activating.
        let _ = desktop.handle_event(&left_up(Point::new(500.0, 400.0)));
        assert!(
            desktop.windows.iter().any(|w| w.id == fq_id),
            "release outside the pressed button must not activate it"
        );

        // A later release inside without a fresh press is inert too.
        let _ = desktop.handle_event(&left_up(cancel));
        assert!(desktop.windows.iter().any(|w| w.id == fq_id));
    }

    #[test]
    fn about_ok_closes_via_generic_dispatch() {
        let (mut desktop, _) = test_desktop();
        desktop.handle_menu_action("shell.about");
        let about = desktop.windows.last().expect("about window");
        assert_eq!(about.window.title(), "About SLOPOS-I");
        let about_id = about.id;
        let ok = center(button_rect_in_window(about, "OK"));

        let _ = desktop.handle_event(&left_down(ok));
        let _ = desktop.handle_event(&left_up(ok));
        assert!(!desktop.windows.iter().any(|w| w.id == about_id));
    }

    #[test]
    fn workspace_grid_cell_press_switches_and_closes_overview() {
        let (mut desktop, _) = test_desktop();
        desktop.handle_menu_action("workspace.next");
        assert_eq!(desktop.active_workspace(), 1);
        let overview = desktop.windows.last().expect("workspace overview window");
        assert_eq!(overview.window.title(), "Workspace");
        let overview_id = overview.id;

        fn grid_cell_center(window: &ShellWindow, cell: usize) -> Point {
            fn find(widget: &dyn Widget, cell: usize) -> Option<Point> {
                if let Some(grid) = widget.as_any().downcast_ref::<WorkspaceGridView>() {
                    return Some(center(grid.cell_rect(cell)));
                }
                widget
                    .children()
                    .into_iter()
                    .find_map(|child| find(child, cell))
            }
            find(&window.window, cell).expect("workspace grid exists")
        }
        let cell0 = grid_cell_center(desktop.windows.last().unwrap(), 0);

        let result = desktop.handle_event(&left_down(cell0));
        assert!(matches!(result, EventResult::Handled));
        let result = desktop.handle_event(&left_up(cell0));
        assert!(matches!(result, EventResult::Handled));
        assert_eq!(desktop.active_workspace(), 0, "cell 0 press switches back");
        assert!(
            !desktop.windows.iter().any(|w| w.id == overview_id),
            "overview closes after switching"
        );
    }

    #[test]
    fn accessibility_snapshot_includes_live_workspace_grid_state() {
        let (mut desktop, _) = test_desktop();
        desktop.open_workspace_status_window();

        let tree = desktop.accessibility_tree();
        let spaces = tree
            .nodes()
            .iter()
            .find(|node| node.role == slopos_kit::AccessibilityRole::List && node.label == "Spaces")
            .expect("live Spaces list is exported from the overview widget");
        assert_eq!(
            spaces.children.len(),
            desktop.workspace_manager.read().total
        );
        assert_eq!(spaces.children[0].label, "Desktop 1");
        assert!(spaces.children[0].state.selected);
        assert!(spaces.children[0].description.contains("Stable Space ID 1"));
        assert!(spaces.children[0].description.contains("0 windows"));
        assert!(spaces.children[0].rect.width > 0.0);
        assert!(spaces.children[0].rect.height > 0.0);
    }

    #[cfg(unix)]
    #[test]
    fn live_workspace_overview_selects_by_stable_space_id() {
        use slopos_bus::{SessionControlListener, SessionControlRequest, SpacesControlCommand};

        let _environment_guard = SESSION_RUNTIME_ENV_LOCK.lock().unwrap();
        let runtime = std::env::temp_dir().join(format!(
            "slo-wso-{}-{}",
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

        let (mut desktop, _) = test_desktop();
        desktop
            .workspace_manager
            .write()
            .apply_snapshot(&slopos_bus::SpacesSnapshot {
                session_epoch: 1,
                revision: 9,
                active_space: 11,
                multi_monitor_policy: slopos_bus::SpacesDisplayPolicy::SharedSpan,
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
                        classification: slopos_bus::SpaceClassification::Normal,
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
                        classification: slopos_bus::SpaceClassification::Normal,
                        output_id: None,
                    },
                ],
            });
        desktop.set_layer_shell_bound(true);
        desktop.open_workspace_status_window();
        assert_eq!(
            listener.drain(),
            vec![SessionControlRequest::Spaces {
                command: SpacesControlCommand::RefreshThumbnails,
            }]
        );
        desktop.set_input_filter(Some(ShellPaintFilter::SpacesOverview));

        fn grid_cell_center(window: &Window, cell: usize) -> Point {
            fn find(widget: &dyn Widget, cell: usize) -> Option<Point> {
                if let Some(grid) = widget.as_any().downcast_ref::<WorkspaceGridView>() {
                    return Some(center(grid.cell_rect(cell)));
                }
                widget
                    .children()
                    .into_iter()
                    .find_map(|child| find(child, cell))
            }
            find(window, cell).expect("live workspace overview grid exists")
        }

        let cell = grid_cell_center(desktop.workspace_overview.as_ref().unwrap(), 1);
        assert!(matches!(
            desktop.handle_event(&left_down(cell)),
            EventResult::Handled
        ));
        assert!(matches!(
            desktop.handle_event(&left_up(cell)),
            EventResult::Handled
        ));
        assert_eq!(
            listener.drain(),
            vec![SessionControlRequest::Spaces {
                command: SpacesControlCommand::Select { id: 22 },
            }]
        );
        assert!(desktop.workspace_overview.is_none());

        if let Some(previous_runtime) = previous_runtime {
            std::env::set_var("SLOPOS_SESSION_RUNTIME_DIR", previous_runtime);
        } else {
            std::env::remove_var("SLOPOS_SESSION_RUNTIME_DIR");
        }
        drop(listener);
        fs::remove_dir_all(runtime).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn live_workspace_overview_drag_moves_active_window_by_stable_space_id() {
        use slopos_bus::{SessionControlListener, SessionControlRequest, SpaceTargetWire};

        let _environment_guard = SESSION_RUNTIME_ENV_LOCK.lock().unwrap();
        let runtime = std::env::temp_dir().join(format!(
            "slo-wsd-{}-{}",
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

        let (mut desktop, _) = test_desktop();
        desktop
            .workspace_manager
            .write()
            .apply_snapshot(&slopos_bus::SpacesSnapshot {
                session_epoch: 1,
                revision: 13,
                active_space: 11,
                multi_monitor_policy: slopos_bus::SpacesDisplayPolicy::SharedSpan,
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
                        classification: slopos_bus::SpaceClassification::Normal,
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
                        classification: slopos_bus::SpaceClassification::Normal,
                        output_id: None,
                    },
                ],
            });
        desktop.set_layer_shell_bound(true);
        desktop.set_input_filter(Some(ShellPaintFilter::SpacesOverview));
        desktop.open_workspace_status_window();
        assert_eq!(
            listener.drain(),
            vec![SessionControlRequest::Spaces {
                command: slopos_bus::SpacesControlCommand::RefreshThumbnails,
            }]
        );

        fn grid_cell_center(window: &Window, cell: usize) -> Point {
            fn find(widget: &dyn Widget, cell: usize) -> Option<Point> {
                if let Some(grid) = widget.as_any().downcast_ref::<WorkspaceGridView>() {
                    return Some(center(grid.cell_rect(cell)));
                }
                widget
                    .children()
                    .into_iter()
                    .find_map(|child| find(child, cell))
            }
            find(window, cell).expect("live workspace overview grid exists")
        }

        let source = grid_cell_center(desktop.workspace_overview.as_ref().unwrap(), 0);
        let target = grid_cell_center(desktop.workspace_overview.as_ref().unwrap(), 1);
        assert!(matches!(
            desktop.handle_event(&left_down(source)),
            EventResult::Handled
        ));
        assert!(matches!(
            desktop.handle_event(&Event::MouseMove {
                point: target,
                modifiers: Modifiers::NONE,
            }),
            EventResult::Handled
        ));
        assert!(matches!(
            desktop.handle_event(&left_up(target)),
            EventResult::Handled
        ));
        assert_eq!(
            listener.drain(),
            vec![SessionControlRequest::Spaces {
                command: slopos_bus::SpacesControlCommand::MoveActiveWindow {
                    target: SpaceTargetWire::Id { id: 22 },
                },
            }]
        );
        assert!(desktop.workspace_overview.is_none());
        assert!(desktop.input_filter.is_none());

        if let Some(previous_runtime) = previous_runtime {
            std::env::set_var("SLOPOS_SESSION_RUNTIME_DIR", previous_runtime);
        } else {
            std::env::remove_var("SLOPOS_SESSION_RUNTIME_DIR");
        }
        drop(listener);
        fs::remove_dir_all(runtime).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn live_workspace_overview_keyboard_selects_dismisses_and_rejects_stale() {
        use slopos_bus::{SessionControlListener, SessionControlRequest, SpacesControlCommand};

        let _environment_guard = SESSION_RUNTIME_ENV_LOCK.lock().unwrap();
        let runtime = std::env::temp_dir().join(format!(
            "slo-wsk-{}-{}",
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

        let mut desktop = test_desktop().0;
        desktop
            .workspace_manager
            .write()
            .apply_snapshot(&slopos_bus::SpacesSnapshot {
                session_epoch: 1,
                revision: 10,
                active_space: 11,
                multi_monitor_policy: slopos_bus::SpacesDisplayPolicy::SharedSpan,
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
                        classification: slopos_bus::SpaceClassification::Normal,
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
                        classification: slopos_bus::SpaceClassification::Normal,
                        output_id: None,
                    },
                ],
            });
        desktop.set_layer_shell_bound(true);
        desktop.set_input_filter(Some(ShellPaintFilter::SpacesOverview));
        desktop.open_workspace_status_window();
        assert_eq!(
            listener.drain(),
            vec![SessionControlRequest::Spaces {
                command: SpacesControlCommand::RefreshThumbnails,
            }]
        );

        let key = |key| Event::KeyDown {
            key,
            modifiers: Modifiers::NONE,
        };
        assert!(matches!(
            desktop.handle_event(&key(slopos_kit::event::KeyCode::ArrowRight)),
            EventResult::Handled
        ));
        assert!(listener.drain().is_empty(), "navigation must not send IPC");
        assert!(matches!(
            desktop.handle_event(&key(slopos_kit::event::KeyCode::Enter)),
            EventResult::Handled
        ));
        assert_eq!(
            listener.drain(),
            vec![SessionControlRequest::Spaces {
                command: SpacesControlCommand::Select { id: 22 },
            }]
        );
        assert!(desktop.workspace_overview.is_none());
        assert!(desktop.input_filter.is_none());

        desktop.open_workspace_status_window();
        assert_eq!(
            listener.drain(),
            vec![SessionControlRequest::Spaces {
                command: SpacesControlCommand::RefreshThumbnails,
            }]
        );
        assert!(matches!(
            desktop.handle_event(&key(slopos_kit::event::KeyCode::Escape)),
            EventResult::Handled
        ));
        assert!(desktop.workspace_overview.is_none());
        assert!(desktop.input_filter.is_none());
        assert!(listener.drain().is_empty(), "dismissal must not send IPC");

        desktop.open_workspace_status_window();
        assert_eq!(
            listener.drain(),
            vec![SessionControlRequest::Spaces {
                command: SpacesControlCommand::RefreshThumbnails,
            }]
        );
        {
            let mut manager = desktop.workspace_manager.write();
            manager.workspaces.clear();
            manager.total = 0;
        }
        assert!(matches!(
            desktop.handle_event(&key(slopos_kit::event::KeyCode::Enter)),
            EventResult::Handled
        ));
        assert!(listener.drain().is_empty(), "stale cells must not send IPC");
        assert!(desktop.workspace_overview.is_some());

        if let Some(previous_runtime) = previous_runtime {
            std::env::set_var("SLOPOS_SESSION_RUNTIME_DIR", previous_runtime);
        } else {
            std::env::remove_var("SLOPOS_SESSION_RUNTIME_DIR");
        }
        drop(listener);
        fs::remove_dir_all(runtime).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn live_workspace_overview_shift_enter_moves_active_window_by_stable_space_id() {
        use slopos_bus::{SessionControlListener, SessionControlRequest, SpaceTargetWire};

        let _environment_guard = SESSION_RUNTIME_ENV_LOCK.lock().unwrap();
        let runtime = std::env::temp_dir().join(format!(
            "slo-wsm-{}-{}",
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

        let mut desktop = test_desktop().0;
        desktop
            .workspace_manager
            .write()
            .apply_snapshot(&slopos_bus::SpacesSnapshot {
                session_epoch: 1,
                revision: 11,
                active_space: 11,
                multi_monitor_policy: slopos_bus::SpacesDisplayPolicy::SharedSpan,
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
                        classification: slopos_bus::SpaceClassification::Normal,
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
                        classification: slopos_bus::SpaceClassification::Normal,
                        output_id: None,
                    },
                ],
            });
        desktop.set_layer_shell_bound(true);
        desktop.set_input_filter(Some(ShellPaintFilter::SpacesOverview));
        desktop.open_workspace_status_window();
        assert_eq!(
            listener.drain(),
            vec![SessionControlRequest::Spaces {
                command: slopos_bus::SpacesControlCommand::RefreshThumbnails,
            }]
        );

        let arrow_right = Event::KeyDown {
            key: slopos_kit::event::KeyCode::ArrowRight,
            modifiers: Modifiers::NONE,
        };
        assert!(matches!(
            desktop.handle_event(&arrow_right),
            EventResult::Handled
        ));
        assert!(
            listener.drain().is_empty(),
            "overview focus navigation must precede the move request without IPC"
        );
        let shift_enter = Event::KeyDown {
            key: slopos_kit::event::KeyCode::Enter,
            modifiers: Modifiers {
                shift: true,
                ..Modifiers::NONE
            },
        };
        assert!(matches!(
            desktop.handle_event(&shift_enter),
            EventResult::Handled
        ));
        assert_eq!(
            listener.drain(),
            vec![SessionControlRequest::Spaces {
                command: SpacesControlCommand::MoveActiveWindow {
                    target: SpaceTargetWire::Id { id: 22 },
                },
            }]
        );
        assert!(
            desktop.workspace_overview.is_none(),
            "a successfully queued move closes the modal overview"
        );
        assert!(desktop.input_filter.is_none());

        if let Some(previous_runtime) = previous_runtime {
            std::env::set_var("SLOPOS_SESSION_RUNTIME_DIR", previous_runtime);
        } else {
            std::env::remove_var("SLOPOS_SESSION_RUNTIME_DIR");
        }
        drop(listener);
        fs::remove_dir_all(runtime).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn live_workspace_overview_shift_enter_keeps_modal_state_when_session_send_fails() {
        let _environment_guard = SESSION_RUNTIME_ENV_LOCK.lock().unwrap();
        let runtime = std::env::temp_dir().join(format!(
            "slo-wsm-fail-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&runtime).unwrap();
        let previous_runtime = std::env::var_os("SLOPOS_SESSION_RUNTIME_DIR");
        std::env::set_var("SLOPOS_SESSION_RUNTIME_DIR", &runtime);

        let mut desktop = test_desktop().0;
        desktop
            .workspace_manager
            .write()
            .apply_snapshot(&slopos_bus::SpacesSnapshot {
                session_epoch: 1,
                revision: 12,
                active_space: 11,
                multi_monitor_policy: slopos_bus::SpacesDisplayPolicy::SharedSpan,
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
                        classification: slopos_bus::SpaceClassification::Normal,
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
                        classification: slopos_bus::SpaceClassification::Normal,
                        output_id: None,
                    },
                ],
            });
        desktop.set_layer_shell_bound(true);
        desktop.set_input_filter(Some(ShellPaintFilter::SpacesOverview));
        desktop.open_workspace_status_window();

        let shift_enter = Event::KeyDown {
            key: slopos_kit::event::KeyCode::Enter,
            modifiers: Modifiers {
                shift: true,
                ..Modifiers::NONE
            },
        };
        assert!(matches!(
            desktop.handle_event(&shift_enter),
            EventResult::Handled
        ));
        assert!(
            desktop.workspace_overview.is_some(),
            "failed session sends must not dismiss the modal overview"
        );
        assert_eq!(desktop.input_filter, Some(ShellPaintFilter::SpacesOverview));
        let manager = desktop.workspace_manager.read();
        assert_eq!(manager.active_id, 11);
        assert_eq!(manager.revision, 12);

        if let Some(previous_runtime) = previous_runtime {
            std::env::set_var("SLOPOS_SESSION_RUNTIME_DIR", previous_runtime);
        } else {
            std::env::remove_var("SLOPOS_SESSION_RUNTIME_DIR");
        }
        fs::remove_dir_all(runtime).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn update_reconciles_stale_revision_and_compositor_restart() {
        let _environment_guard = SESSION_RUNTIME_ENV_LOCK.lock().unwrap();
        let runtime = std::env::temp_dir().join(format!(
            "slo-wsu-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&runtime).unwrap();
        let previous_runtime = std::env::var_os("SLOPOS_SESSION_RUNTIME_DIR");
        std::env::set_var("SLOPOS_SESSION_RUNTIME_DIR", &runtime);

        slopos_bus::write_spaces_snapshot(&slopos_bus::SpacesSnapshot {
            session_epoch: 1,
            revision: 12,
            active_space: 22,
            multi_monitor_policy: slopos_bus::SpacesDisplayPolicy::SharedSpan,
            application_policies: Vec::new(),
            spaces: vec![
                slopos_bus::SpaceSnapshot {
                    id: 11,
                    order: 0,
                    name: "Personal".to_string(),
                    active: false,
                    window_count: 1,
                    wallpaper: None,
                    appearance: None,
                    classification: slopos_bus::SpaceClassification::Normal,
                    output_id: None,
                },
                slopos_bus::SpaceSnapshot {
                    id: 22,
                    order: 1,
                    name: "Projects".to_string(),
                    active: true,
                    window_count: 4,
                    wallpaper: None,
                    appearance: None,
                    classification: slopos_bus::SpaceClassification::Normal,
                    output_id: None,
                },
            ],
        })
        .unwrap();

        let (mut desktop, _) = test_desktop();
        desktop.update();
        assert_eq!(desktop.workspace_manager.read().revision, 12);
        assert_eq!(desktop.workspace_manager.read().active_id, 22);
        assert_eq!(desktop.workspace_manager.read().total, 2);
        assert_eq!(
            desktop.workspace_manager.read().workspaces[1].window_count,
            4
        );

        slopos_bus::write_spaces_snapshot(&slopos_bus::SpacesSnapshot {
            session_epoch: 1,
            revision: 12,
            active_space: 11,
            multi_monitor_policy: slopos_bus::SpacesDisplayPolicy::SharedSpan,
            application_policies: Vec::new(),
            spaces: vec![slopos_bus::SpaceSnapshot {
                id: 11,
                order: 0,
                name: "Duplicate revision".to_string(),
                active: true,
                window_count: 0,
                wallpaper: None,
                appearance: None,
                classification: slopos_bus::SpaceClassification::Normal,
                output_id: None,
            }],
        })
        .unwrap();
        desktop.update();
        assert_eq!(desktop.workspace_manager.read().active_id, 22);

        slopos_bus::write_spaces_snapshot(&slopos_bus::SpacesSnapshot {
            session_epoch: 1,
            revision: 11,
            active_space: 11,
            multi_monitor_policy: slopos_bus::SpacesDisplayPolicy::SharedSpan,
            application_policies: Vec::new(),
            spaces: vec![slopos_bus::SpaceSnapshot {
                id: 11,
                order: 0,
                name: "Stale".to_string(),
                active: true,
                window_count: 0,
                wallpaper: None,
                appearance: None,
                classification: slopos_bus::SpaceClassification::Normal,
                output_id: None,
            }],
        })
        .unwrap();
        desktop.update();
        assert_eq!(desktop.workspace_manager.read().revision, 12);
        assert_eq!(desktop.workspace_manager.read().active_id, 22);

        // A restarted compositor starts its revision counter over, but the
        // new session epoch makes that lower revision authoritative again.
        slopos_bus::write_spaces_snapshot(&slopos_bus::SpacesSnapshot {
            session_epoch: 2,
            revision: 1,
            active_space: 11,
            multi_monitor_policy: slopos_bus::SpacesDisplayPolicy::SharedSpan,
            application_policies: Vec::new(),
            spaces: vec![slopos_bus::SpaceSnapshot {
                id: 11,
                order: 0,
                name: "Restarted Desktop".to_string(),
                active: true,
                window_count: 2,
                wallpaper: None,
                appearance: None,
                classification: slopos_bus::SpaceClassification::Normal,
                output_id: None,
            }],
        })
        .unwrap();
        desktop.update();
        let manager = desktop.workspace_manager.read();
        assert_eq!(manager.session_epoch, 2);
        assert_eq!(manager.revision, 1);
        assert_eq!(manager.active_id, 11);
        assert_eq!(manager.workspaces[0].name, "Restarted Desktop");
        assert_eq!(manager.workspaces[0].window_count, 2);

        if let Some(previous_runtime) = previous_runtime {
            std::env::set_var("SLOPOS_SESSION_RUNTIME_DIR", previous_runtime);
        } else {
            std::env::remove_var("SLOPOS_SESSION_RUNTIME_DIR");
        }
        fs::remove_dir_all(runtime).unwrap();
    }

    #[test]
    fn dock_item_press_launches_through_dispatch() {
        let (mut desktop, _) = test_desktop();
        desktop.dock.write().items.clear();
        desktop.dock.write().add_item("com.slopos.ghost", "Ghost");
        desktop.dock_view.items = vec![slopos_kit::dock_view::DockViewItem {
            label: "Ghost".to_string(),
            icon: String::new(),
            is_focused: false,
            is_running: false,
        }];
        let item = center(desktop.dock_view.item_rect(0));

        let result = desktop.handle_event(&left_down(item));

        assert!(matches!(result, EventResult::Handled));
        let err = desktop.last_error.as_deref().unwrap_or("");
        assert!(
            err.contains("No binary registered"),
            "dock press reached launch_external_app: {err}"
        );
    }

    #[test]
    fn dialog_buttons_sit_inside_their_window_frame() {
        // The old fixed dialog rects arranged the button row below the
        // window's bottom edge; the old geometry chains hit-tested that
        // invisible zone, rect-checked dispatch refuses it. `fit_dialog_rect`
        // sizes the frame to the content instead.
        let (mut desktop, _) = test_desktop();

        desktop.handle_menu_action("shell.force_quit");
        let fq = desktop.windows.last().expect("force quit window");
        let frame = fq.window.rect();
        for label in ["Cancel", "Force Quit"] {
            let b = button_rect_in_window(fq, label);
            assert!(
                b.y + b.height <= frame.y + frame.height && b.x >= frame.x,
                "{label} button {b:?} must sit inside the frame {frame:?}"
            );
        }

        desktop.handle_menu_action("shell.about");
        let about = desktop.windows.last().expect("about window");
        let frame = about.window.rect();
        let ok = button_rect_in_window(about, "OK");
        assert!(
            ok.y + ok.height <= frame.y + frame.height,
            "OK button {ok:?} must sit inside the frame {frame:?}"
        );
    }

    #[test]
    fn window_body_click_is_opaque_to_whatever_is_underneath() {
        let (mut desktop, _) = test_desktop();
        let window_rect = desktop.windows[0].window.rect();
        // Inside the window body: below the titlebar, away from every
        // chrome box and the resize handle.
        let body = Point::new(
            window_rect.x + window_rect.width * 0.5,
            window_rect.y + window_rect.height * 0.5,
        );

        let result = desktop.handle_event(&left_down(body));
        assert!(
            matches!(result, EventResult::Handled),
            "a window swallows clicks on its empty area instead of letting \
             them fall through to the desktop"
        );
    }

    #[test]
    fn fullscreen_menu_toggles_active_window_state() {
        let (mut desktop, window_manager) = test_desktop();
        let id = desktop.windows[0].id;
        let original = desktop.windows[0].window.rect();

        desktop.handle_menu_action("shell.toggle_fullscreen");

        assert_eq!(desktop.windows[0].mode, ShellWindowMode::Fullscreen);
        assert!(desktop.windows[0].restore_rect.is_some());
        assert_rect_eq(desktop.windows[0].restore_rect.unwrap(), original);
        assert_eq!(
            window_manager.read().windows[&id].state,
            window_manager::WindowState::Fullscreen
        );
        assert_rect_eq(desktop.windows[0].window.rect(), desktop.content_bounds());

        desktop.handle_menu_action("shell.toggle_fullscreen");

        assert_eq!(desktop.windows[0].mode, ShellWindowMode::Normal);
        assert!(desktop.windows[0].restore_rect.is_none());
        assert_eq!(
            window_manager.read().windows[&id].state,
            window_manager::WindowState::Normal
        );
        assert_rect_eq(desktop.windows[0].window.rect(), original);
    }

    #[test]
    fn global_menu_shortcut_opens_new_finder_window() {
        // Keep this in-process policy test independent of a stale manifest
        // left by a previous session under the ambient XDG runtime directory.
        // Production clients intentionally share that session directory; the
        // test must not accidentally import another app's menu while building
        // its synthetic Finder window.
        let _guard = MENU_MANIFEST_ENV_LOCK.lock().unwrap();
        let _manifest_guard = MenuManifestEnvGuard::new();

        let (mut desktop, _) = test_desktop();
        let initial_count = desktop.windows.len();

        let result = desktop.handle_event(&Event::KeyDown {
            key: slopos_kit::event::KeyCode::N,
            modifiers: Modifiers {
                shift: false,
                control: false,
                alt: false,
                meta: true,
            },
        });

        assert!(matches!(result, EventResult::Handled));
        assert_eq!(desktop.windows.len(), initial_count + 1);
        assert_eq!(
            desktop.menu_server.read().active_app.as_deref(),
            Some("com.slopos.finder")
        );
    }

    #[test]
    fn global_menu_shortcut_closes_active_window() {
        let (mut desktop, _) = test_desktop();
        let initial_count = desktop.windows.len();

        let result = desktop.handle_event(&Event::KeyDown {
            key: slopos_kit::event::KeyCode::W,
            modifiers: Modifiers {
                shift: false,
                control: false,
                alt: false,
                meta: true,
            },
        });

        assert!(matches!(result, EventResult::Handled));
        assert_eq!(desktop.windows.len(), initial_count.saturating_sub(1));
    }

    #[test]
    fn global_menu_go_home_action_opens_home_window() {
        let (mut desktop, _) = test_desktop();
        let initial_count = desktop.windows.len();

        desktop.handle_menu_action("shell.open_home");

        assert_eq!(desktop.windows.len(), initial_count + 1);
        assert_eq!(desktop.windows.last().unwrap().window.title(), "Home");
    }

    #[test]
    fn default_shell_menus_have_routable_action_ids() {
        let server = MenuServer::new();
        for menu in &server.menus {
            for item in &menu.items {
                if matches!(item.kind, slopos_kit::menu::MenuItemKind::Action) {
                    assert!(
                        !item.action_id.is_empty(),
                        "{} > {} has no action id",
                        menu.title,
                        item.label
                    );
                }
            }
        }

        let file = server
            .menus
            .iter()
            .find(|menu| menu.title == "File")
            .expect("file menu exists");

        assert_eq!(file.items[0].action_id, "shell.new_finder_window");
        assert_eq!(file.items[1].action_id, "shell.open_finder");
        assert_eq!(file.items[2].action_id, "shell.close_finder_window");

        let view = server
            .menus
            .iter()
            .find(|menu| menu.title == "View")
            .expect("view menu exists");
        assert!(view
            .items
            .iter()
            .any(|item| item.action_id == "shell.toggle_fullscreen"));
    }

    #[test]
    fn lock_accepts_correct_password() {
        // Drive the shipped verify_lock_password used by Enter-to-unlock.
        assert!(verify_lock_password("test_password", "test_password"));
        assert!(verify_lock_password("s3cret!", "s3cret!"));
    }

    #[test]
    fn lock_rejects_wrong_password() {
        assert!(!verify_lock_password("wrong", "correct_password"));
        assert!(!verify_lock_password("", "correct_password"));
        assert!(!verify_lock_password("correct_password", ""));
        assert!(!verify_lock_password(
            "Correct_password",
            "correct_password"
        ));
    }

    #[test]
    fn lock_password_env_is_source_for_expected_secret() {
        let _lock = LOCK_PASSWORD_ENV_LOCK.lock().unwrap();
        std::env::remove_var("SLOPOS_LOCK_PASSWORD");
        std::env::set_var("SLOPOS_LOCK_PASSWORD", "env_secret");
        let expected = get_lock_password().expect("env secret");
        assert!(verify_lock_password("env_secret", &expected));
        assert!(!verify_lock_password("other", &expected));
        std::env::remove_var("SLOPOS_LOCK_PASSWORD");
    }

    #[test]
    fn a11y_dispatch_shell_session_actions_are_live() {
        let (mut desktop, _) = test_desktop();

        desktop.dispatch_a11y_invoke("shell.notification_center");
        assert!(desktop
            .windows
            .iter()
            .any(|w| w.window.title() == "Notification Center"));

        desktop.dispatch_a11y_invoke("shell.force_quit");
        assert!(desktop
            .windows
            .iter()
            .any(|w| w.window.title() == "Force Quit"));

        desktop.expected_lock_password = Some("secret".into());
        desktop.dispatch_a11y_invoke("shell.lock");
        assert!(desktop.locked);
    }

    #[test]
    fn lock_screen_typed_password_unlocks_and_wrong_one_does_not() {
        let (mut desktop, _) = test_desktop();
        desktop.expected_lock_password = Some("secret".into());
        desktop.dispatch_a11y_invoke("shell.lock");
        assert!(desktop.locked, "lock did not engage");

        // Wrong password: characters must accumulate, Enter must reject.
        for ch in "wrong".chars() {
            desktop.handle_event(&Event::Char { character: ch });
        }
        assert_eq!(desktop.lock_password_field.text(), "wrong");
        desktop.handle_event(&Event::KeyDown {
            key: slopos_kit::event::KeyCode::Enter,
            modifiers: slopos_kit::event::Modifiers::NONE,
        });
        assert!(desktop.locked, "wrong password unlocked the screen");
        assert_eq!(desktop.lock_password_field.text(), "");

        // Correct password must unlock.
        for ch in "secret".chars() {
            desktop.handle_event(&Event::Char { character: ch });
        }
        assert_eq!(desktop.lock_password_field.text(), "secret");
        desktop.handle_event(&Event::KeyDown {
            key: slopos_kit::event::KeyCode::Enter,
            modifiers: slopos_kit::event::Modifiers::NONE,
        });
        assert!(!desktop.locked, "correct password did not unlock");
    }

    #[test]
    fn a11y_dispatch_chrome_window_close_and_activate_next() {
        let (mut desktop, window_manager) = test_desktop();
        // Start with the default Finder; open a second window so activate can cycle.
        desktop.handle_menu_action("shell.new_finder_window");
        assert!(desktop.windows.len() >= 2);

        let first = desktop.windows[0].id;
        let second = desktop.windows[1].id;
        desktop.focus_window(first);
        assert_eq!(desktop.active_window_id(), Some(first));

        desktop.dispatch_a11y_invoke("chrome.window.activate");
        let after_activate = desktop.active_window_id();
        assert_eq!(after_activate, Some(second));
        assert_eq!(window_manager.read().active_window, Some(second));

        let before_close = desktop.windows.len();
        desktop.dispatch_a11y_invoke("chrome.window.close");
        assert_eq!(desktop.windows.len(), before_close - 1);
        assert!(!desktop.windows.iter().any(|w| w.id == second));
    }

    #[test]
    fn a11y_dispatch_workspace_next_previous() {
        let (mut desktop, _) = test_desktop();
        assert_eq!(desktop.active_workspace(), 0);

        desktop.dispatch_a11y_invoke("workspace.next");
        assert_eq!(desktop.active_workspace(), 1);

        desktop.dispatch_a11y_invoke("workspace.previous");
        assert_eq!(desktop.active_workspace(), 0);
    }

    #[test]
    fn a11y_dispatch_chrome_menu_activate_opens_system_menu() {
        let (mut desktop, _) = test_desktop();
        assert!(desktop.menu_bar.open_menu.is_none());
        assert!(
            desktop.menu_bar.menus.iter().any(|m| m.title == "SLOPOS"),
            "precondition: system Retro menu present"
        );

        desktop.dispatch_a11y_invoke("chrome.menu.activate");
        let retro_idx = desktop
            .menu_bar
            .menus
            .iter()
            .position(|m| m.title == "SLOPOS")
            .expect("Retro menu");
        assert_eq!(desktop.menu_bar.open_menu, Some(retro_idx));

        desktop.menu_bar.close();
        assert!(desktop.menu_bar.open_menu.is_none());

        desktop.dispatch_a11y_invoke("chrome.menu.system");
        assert_eq!(desktop.menu_bar.open_menu, Some(retro_idx));
    }

    #[test]
    fn a11y_dispatch_chrome_dock_menu_opens_status_window() {
        let (mut desktop, _) = test_desktop();
        assert!(
            !desktop.dock.read().items.is_empty(),
            "precondition: dock items"
        );

        desktop.dispatch_a11y_invoke("chrome.dock.menu");
        assert!(
            desktop
                .windows
                .iter()
                .any(|w| w.window.title() == "Dock Menu"),
            "chrome.dock.menu should open a Dock Menu status window"
        );

        // Second invoke focuses existing window rather than spawning another.
        let count = desktop
            .windows
            .iter()
            .filter(|w| w.window.title() == "Dock Menu")
            .count();
        desktop.dispatch_a11y_invoke("chrome.dock.menu");
        let count_after = desktop
            .windows
            .iter()
            .filter(|w| w.window.title() == "Dock Menu")
            .count();
        assert_eq!(count, count_after);
        assert_eq!(
            desktop.windows.last().map(|w| w.window.title()),
            Some("Dock Menu")
        );
    }

    #[test]
    fn a11y_dispatch_chrome_desktop_menu_opens_status_window() {
        let (mut desktop, _) = test_desktop();
        assert!(
            !desktop.desktop.items.is_empty(),
            "precondition: desktop icons"
        );

        desktop.dispatch_a11y_invoke("chrome.desktop.menu");
        assert!(
            desktop
                .windows
                .iter()
                .any(|w| w.window.title() == "Desktop Menu"),
            "chrome.desktop.menu should open a Desktop Menu status window"
        );

        let count = desktop
            .windows
            .iter()
            .filter(|w| w.window.title() == "Desktop Menu")
            .count();
        desktop.dispatch_a11y_invoke("chrome.desktop.menu");
        let count_after = desktop
            .windows
            .iter()
            .filter(|w| w.window.title() == "Desktop Menu")
            .count();
        assert_eq!(count, count_after);
        assert_eq!(
            desktop.windows.last().map(|w| w.window.title()),
            Some("Desktop Menu")
        );
    }

    #[test]
    fn a11y_chrome_activate_enter_dispatches_primary_invoke() {
        let (mut desktop, _) = test_desktop();
        desktop.chrome_focus = ChromeFocusTarget::Windows;
        desktop.handle_menu_action("shell.new_finder_window");
        let first = desktop.windows[0].id;
        desktop.focus_window(first);

        let result = desktop.handle_event(&Event::KeyDown {
            key: slopos_kit::event::KeyCode::Enter,
            modifiers: Modifiers::NONE,
        });
        assert!(matches!(result, EventResult::Handled));
        // Windows primary invoke is chrome.window.activate → focus next.
        assert_ne!(desktop.active_window_id(), Some(first));
    }

    #[test]
    fn idle_config_parses_from_settings_conf_keys() {
        let cfg = IdleConfig::parse_from_conf(
            "# comment\nidle_warn_secs=15\nidle_lock_secs=45\nidle_suspend_secs=90\nlock_on_suspend=false\n",
        );
        assert_eq!(cfg.warn_after_secs, 15);
        assert_eq!(cfg.lock_after_secs, 45);
        assert_eq!(cfg.suspend_after_secs, 90);
        assert!(!cfg.lock_on_suspend);
        // Empty conf keeps defaults (used when settings.conf is absent).
        assert_eq!(IdleConfig::parse_from_conf(""), IdleConfig::default());
    }

    #[test]
    fn portal_idle_inhibit_merges_into_phase() {
        clear_inhibit_store_for_tests();
        let cfg = IdleConfig {
            warn_after_secs: 1,
            lock_after_secs: 2,
            suspend_after_secs: 0,
            lock_on_suspend: true,
            inhibited: false,
        };
        let base = IdleInhibitState::new();
        assert_eq!(idle_phase(&cfg, 10, false, &base), IdlePhase::ShouldLock);

        let _ = handle_inhibit_and_register(&PortalInhibitRequest {
            app_id: "player".into(),
            window: String::new(),
            flags: InhibitFlag::Idle as u32,
            reason: "playing".into(),
        });
        let mut merged = base.clone();
        for reason in active_idle_inhibit_state().reasons() {
            merged.add(*reason);
        }
        assert!(merged.is_inhibited());
        assert_eq!(idle_phase(&cfg, 10, false, &merged), IdlePhase::Active);
        clear_inhibit_store_for_tests();
    }

    #[test]
    fn network_connect_api_records_validated_plan_without_spawn() {
        let (mut desktop, _) = test_desktop();
        assert!(!desktop.network_connect_spawn);

        desktop.request_network_connect(NmConnectRequest::new("CafeNet"));
        let outcome = desktop
            .last_network_connect
            .clone()
            .expect("connect outcome recorded");
        let summary = outcome.expect("valid plan");
        assert!(summary.contains("nmcli"));
        assert!(summary.contains("CafeNet"));

        desktop.request_network_connect(NmConnectRequest::new(""));
        let err = desktop
            .last_network_connect
            .clone()
            .expect("error recorded")
            .unwrap_err();
        assert!(err.contains("non-empty"));
    }

    #[test]
    fn network_connect_menu_action_wired() {
        let server = MenuServer::new();
        assert!(server
            .menus
            .iter()
            .flat_map(|m| m.items.iter())
            .any(|item| item.action_id == "shell.network_connect"));

        let (mut desktop, _) = test_desktop();
        // No SLOPOS_WIFI_SSID → status window, no panic.
        desktop.handle_menu_action("shell.network_connect");
        assert!(desktop
            .windows
            .iter()
            .any(|w| w.window.title() == "Network Connect"));
    }

    #[test]
    fn status_refresh_on_update_and_volume_api() {
        let (mut desktop, _) = test_desktop();
        // Force elapsed so update() re-queries status items.
        desktop.last_status_refresh = std::time::Instant::now()
            - std::time::Duration::from_secs(STATUS_REFRESH_INTERVAL_SECS + 1);
        desktop.update();
        let items = desktop.menu_server.read().status_items.clone();
        let ids: Vec<&str> = items.iter().map(|s| s.id.as_str()).collect();
        assert!(ids.contains(&"battery"));
        assert!(ids.contains(&"volume"));
        assert!(ids.contains(&"network"));

        // Best-effort volume path (may fail without pactl; must not panic).
        desktop.request_set_volume(40);
        let items = desktop.menu_server.read().status_items.clone();
        assert!(items.iter().any(|s| s.id == "volume"));
    }

    #[test]
    fn pure_status_and_volume_plans_exported() {
        assert_eq!(battery_status_label(Some(50)), "🔋 50%");
        assert_eq!(network_status_label(false, None, "Unavailable"), "📶 —");
        assert_eq!(volume_status_label(Some(10)), "🔊 10%");
        assert_eq!(
            volume_pactl_set_plan(25),
            vec!["pactl", "set-sink-volume", "@DEFAULT_SINK@", "25%"]
        );
        let plan =
            nm_connect_plan_validated(&NmConnectRequest::new("X").with_password("y")).unwrap();
        assert_eq!(plan[0], "nmcli");
        assert!(describe_nm_connect_plan(&plan).contains("<redacted>"));
        assert!(execute_nm_connect_plan(&[]).is_err());
    }

    #[test]
    fn spotlight_overlay_toggles_with_super_space() {
        let (mut desktop, _) = test_desktop();

        // Precondition: overlay starts hidden
        assert!(!desktop.spotlight_ui.is_visible());

        // Super+Space shows overlay
        let result = desktop.handle_event(&Event::KeyDown {
            key: slopos_kit::event::KeyCode::Space,
            modifiers: slopos_kit::event::Modifiers {
                shift: false,
                control: false,
                alt: false,
                meta: true,
            },
        });

        match result {
            EventResult::Handled => {}
            _ => panic!("Super+Space should be handled"),
        }
        assert!(desktop.spotlight_ui.is_visible());

        // Super+Space hides overlay
        let result = desktop.handle_event(&Event::KeyDown {
            key: slopos_kit::event::KeyCode::Space,
            modifiers: slopos_kit::event::Modifiers {
                shift: false,
                control: false,
                alt: false,
                meta: true,
            },
        });

        match result {
            EventResult::Handled => {}
            _ => panic!("Super+Space should be handled"),
        }
        assert!(!desktop.spotlight_ui.is_visible());
    }

    #[test]
    fn spotlight_char_input_updates_search_results() {
        let (mut desktop, _) = test_desktop();

        // Show overlay
        desktop.spotlight_ui.show();
        let apps = desktop
            .launch_services
            .read()
            .bundles
            .values()
            .cloned()
            .collect::<Vec<_>>();
        desktop.spotlight_ui.update_results(&apps);

        // Type 's' to match settings
        let result = desktop.handle_event(&Event::Char { character: 's' });
        match result {
            EventResult::Handled => {}
            _ => panic!("Event should be handled"),
        }

        let spotlight = &desktop.spotlight_ui;
        assert_eq!(spotlight.query(), "s");
        // Should have settings results (always available)
        assert!(!spotlight.results().is_empty());
    }

    #[test]
    fn spotlight_escape_hides_overlay() {
        let (mut desktop, _) = test_desktop();

        // Show overlay
        desktop.spotlight_ui.show();
        assert!(desktop.spotlight_ui.is_visible());

        // Press Escape
        let result = desktop.handle_event(&Event::KeyDown {
            key: slopos_kit::event::KeyCode::Escape,
            modifiers: slopos_kit::event::Modifiers::NONE,
        });

        match result {
            EventResult::Handled => {}
            _ => panic!("Event should be handled"),
        }
        assert!(!desktop.spotlight_ui.is_visible());
    }

    #[test]
    fn spotlight_arrow_keys_navigate_results() {
        let (mut desktop, _) = test_desktop();

        // Show overlay and populate results
        desktop.spotlight_ui.show();
        let apps = desktop
            .launch_services
            .read()
            .bundles
            .values()
            .cloned()
            .collect::<Vec<_>>();
        desktop.spotlight_ui.update_results(&apps);

        // Initial selection at index 0
        assert_eq!(desktop.spotlight_ui.selected_index(), 0);

        // Arrow down
        let result = desktop.handle_event(&Event::KeyDown {
            key: slopos_kit::event::KeyCode::ArrowDown,
            modifiers: slopos_kit::event::Modifiers::NONE,
        });

        match result {
            EventResult::Handled => {}
            _ => panic!("Event should be handled"),
        }
        let spotlight = &desktop.spotlight_ui;
        let result_count = spotlight.results().len();
        if result_count > 1 {
            assert_eq!(spotlight.selected_index(), 1);
        }
    }

    #[test]
    fn spotlight_enter_launches_selected_app() {
        let (mut desktop, _) = test_desktop();

        // Show overlay
        desktop.spotlight_ui.show();
        let apps = desktop
            .launch_services
            .read()
            .bundles
            .values()
            .cloned()
            .collect::<Vec<_>>();
        desktop.spotlight_ui.update_results(&apps);

        // Search for 'vol' to match Volume setting
        let result = desktop.handle_event(&Event::Char { character: 'v' });
        match result {
            EventResult::Handled => {}
            _ => panic!("Event should be handled"),
        }
        let result = desktop.handle_event(&Event::Char { character: 'o' });
        match result {
            EventResult::Handled => {}
            _ => panic!("Event should be handled"),
        }
        let result = desktop.handle_event(&Event::Char { character: 'l' });
        match result {
            EventResult::Handled => {}
            _ => panic!("Event should be handled"),
        }

        // Verify we have results (Volume setting)
        assert!(
            !desktop.spotlight_ui.results().is_empty(),
            "Volume setting should be found"
        );

        // Press Enter (should activate selected result)
        let result = desktop.handle_event(&Event::KeyDown {
            key: slopos_kit::event::KeyCode::Enter,
            modifiers: slopos_kit::event::Modifiers::NONE,
        });

        match result {
            EventResult::Handled => {}
            _ => panic!("Event should be handled"),
        }
        // Overlay remains visible (user can select another result or press Escape to close)
        assert!(desktop.spotlight_ui.is_visible());
    }
}
