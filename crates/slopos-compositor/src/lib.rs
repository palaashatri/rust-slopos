//! Shared compositor policy that can be tested without a live Wayland server.

pub mod client_spawn;
pub mod frame_timing;
pub mod hdr;
pub mod output_assignment;
pub mod perf_budget;
pub mod pointer_policy;
pub use pointer_policy::PointerConstraintMotion;
pub mod spaces;
pub mod window_state;
pub mod work_area;
pub mod workspace_focus;

/// Register the Wayland server's internal poll fd with the compositor event loop.
///
/// `ListeningSocketSource` only accepts new client streams. Wayland protocol
/// requests arrive on each accepted client and are surfaced through the
/// server display's own poll fd, so a compositor that blocks in calloop must
/// register this source instead of dispatching clients after an unbounded
/// wait. Keeping this in shared compositor policy makes nested and DRM
/// backends use the same client-transport contract.
#[cfg(target_os = "linux")]
pub fn register_wayland_display_source<'event_loop, State: 'static>(
    loop_handle: &smithay::reexports::calloop::LoopHandle<'event_loop, State>,
    display: smithay::reexports::wayland_server::Display<State>,
) -> anyhow::Result<()> {
    use smithay::reexports::calloop::{generic::Generic, Interest, Mode, PostAction};

    loop_handle
        .insert_source(
            Generic::new(display, Interest::READ, Mode::Level),
            |_, display, state| {
                // `Generic` prevents mutable access because dropping its inner
                // fd while registered would leave calloop with a dangling fd.
                // The source remains registered for the entire compositor
                // lifetime, so mutating the live Display without moving or
                // dropping it is sound here.
                let display = unsafe { display.get_mut() };
                let dispatched = display.dispatch_clients(state).map_err(|error| {
                    tracing::error!(error = %error, "Wayland display dispatch failed");
                    error
                })?;
                if dispatched > 0 {
                    tracing::debug!(dispatched, "Wayland client requests dispatched");
                }
                display.flush_clients().map_err(|error| {
                    tracing::error!(error = %error, "Wayland client flush failed");
                    error
                })?;
                Ok(PostAction::Continue)
            },
        )
        .map(|_| ())
        .map_err(|error| anyhow::anyhow!("register Wayland display source: {error}"))
}

pub use output_assignment::{
    geometries_intersect, intersecting_output_indices, normalize_laid_out_outputs, output_geometry,
    output_index_for_geometry, output_index_for_point, output_layout_bounds,
    plan_window_output_migration, remap_geometry_between_outputs, validated_runtime_output_layout,
    WindowOutputMigration, MAX_RUNTIME_OUTPUTS,
};
pub use spaces::{
    application_target_from_wire, application_target_to_wire, fullscreen_classification_from_wire,
    fullscreen_classification_to_wire, multi_monitor_policy_from_wire,
    multi_monitor_policy_to_wire, new_session_epoch, FullscreenClassification, MultiMonitorPolicy,
    Space, SpaceId, SpaceOverview, SpaceTarget, SpacesCommand, SpacesError, SpacesModel,
    WorkspaceSwipeAction, WorkspaceSwipeRecognizer, WORKSPACE_SWIPE_HORIZONTAL_RATIO,
    WORKSPACE_SWIPE_MIN_DISTANCE,
};
pub use window_state::{
    calculate_presentation_geometry, transition_presentation_state, PresentationTransition,
    TilePlacement, WindowPresentationState, WindowRestoreState, ZoomAction, ZoomPolicyConfig,
};
pub use workspace_focus::{
    activate_workspace_index, assign_new_window_to_active, focus_window_after_workspace_switch,
    hit_test_allowed, move_window_to_index, should_clear_focus_after_workspace_switch,
    visible_paint_order, window_paint_source, WindowPaintSource,
};

/// DRM/KMS + libseat session path (Linux only). Nested X11 lives in the binary.
#[cfg(target_os = "linux")]
pub mod session_drm;

/// Real DRM/KMS property access: HDR metadata blobs, Colorspace, max bpc, VRR.
#[cfg(target_os = "linux")]
pub mod drm_props;

/// Procedural fallback cursor bitmap for `CursorImageStatus::Named` (no XCursor
/// theme dependency).
#[cfg(target_os = "linux")]
pub mod cursor_theme;
#[cfg(target_os = "linux")]
pub mod screenshot;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use frame_timing::RefreshRate;
use hdr::ColorSpace;

/// How the session compositor process is expected to present.
///
/// Pure policy label for Phase A/B honesty: logs and entrypoints must say which
/// path was chosen (nested X11 under an X11 host, real DRM/KMS session, or
/// protocol-only headless mode) rather than implying DRM when only nested X11
/// is running.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum CompositorBackendKind {
    /// Nested Smithay X11 backend (Xvfb / desktop X host). Needs DRI3 for GL.
    NestedX11,
    /// Session DRM/KMS (bare metal / seat) path when prefer_drm && dri3_ok.
    SessionDrm,
    /// Protocol-only compositor path with no host display transport.
    Headless,
}

/// Select compositor session backend kind from capability flags.
///
/// Precedence:
/// 1. `headless` → [`CompositorBackendKind::Headless`]
/// 2. `prefer_drm && dri3_available` → [`CompositorBackendKind::SessionDrm`]
/// 3. otherwise → [`CompositorBackendKind::NestedX11`]
///
/// Nested X11 remains the default when DRM is not preferred or DRI3 is missing;
/// actual GL init may still fail without DRI3, but no third-party compositor is
/// selected as a fallback.
pub fn select_backend_kind(
    prefer_drm: bool,
    dri3_available: bool,
    headless: bool,
) -> CompositorBackendKind {
    if headless {
        return CompositorBackendKind::Headless;
    }
    if prefer_drm && dri3_available {
        return CompositorBackendKind::SessionDrm;
    }
    CompositorBackendKind::NestedX11
}

/// Detect DRI3 availability override from `SLOPOS_DRI3`.
///
/// - `1` / truthy → `Some(true)`
/// - `0` / falsey → `Some(false)`
/// - unset / unrecognised → `None` (caller should probe the real display)
///
/// Intended for tests and CI; production can fall back to X11 extension probe.
pub fn detect_dri3_from_env() -> Option<bool> {
    detect_dri3_from_env_value(std::env::var("SLOPOS_DRI3").ok().as_deref())
}

/// Pure form of [`detect_dri3_from_env`] for unit tests.
pub fn detect_dri3_from_env_value(value: Option<&str>) -> Option<bool> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }
    parse_bool_loose(value)
}

/// One-line honest label for logs (never claims DRM when nested or headless).
pub fn session_mode_summary(kind: CompositorBackendKind) -> String {
    match kind {
        CompositorBackendKind::NestedX11 => {
            "session_mode=nested_x11 (not DRM/KMS; SLOPOS owns the nested compositor)".to_string()
        }
        CompositorBackendKind::SessionDrm => {
            "session_mode=session_drm (DRM/KMS seat path)".to_string()
        }
        CompositorBackendKind::Headless => {
            "session_mode=headless (no host display transport)".to_string()
        }
    }
}

/// Combined honest session note: backend kind + output scale policy.
///
/// Scale is pure compositor policy (logical→physical). Nested X11 may still
/// present a 1:1 framebuffer until the backend applies buffer scale.
pub fn session_mode_note(kind: CompositorBackendKind, scale: OutputScale) -> String {
    format!(
        "{}; {}",
        session_mode_summary(kind),
        output_scale_summary(scale)
    )
}

pub const DEFAULT_OUTPUT_W: i32 = 1024;
pub const DEFAULT_OUTPUT_H: i32 = 768;
pub const DEFAULT_WINDOW_W: i32 = 640;
pub const DEFAULT_WINDOW_H: i32 = 480;
pub const MIN_WINDOW_W: i32 = 160;
pub const MIN_WINDOW_H: i32 = 96;
pub const INITIAL_WINDOW_X: i32 = 64;
pub const INITIAL_WINDOW_Y: i32 = 64;
pub const CASCADE_STEP: i32 = 32;
pub const CASCADE_WRAP: i32 = 256;

/// Publish the private Wayland socket handshake for `slopos-session`.
///
/// The supervisor gives the compositor a unique `XDG_RUNTIME_DIR`, so these
/// files are scoped to one session and cannot race with another login. The
/// readiness payload includes the compositor PID, supervisor token, and the
/// compositor's actual logical output size; the session parent validates the
/// identity before launching any clients and forwards the size to the shell.
#[cfg(target_os = "linux")]
pub fn publish_session_readiness(
    socket_name: &str,
    output_width: i32,
    output_height: i32,
) -> std::io::Result<()> {
    if !socket_name.starts_with("wayland-")
        || socket_name["wayland-".len()..]
            .chars()
            .any(|character| !character.is_ascii_digit())
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Wayland socket name is not a numeric wayland-* handle",
        ));
    }

    let runtime = std::env::var_os("XDG_RUNTIME_DIR").ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "XDG_RUNTIME_DIR is required for the private compositor socket",
        )
    })?;
    let runtime = Path::new(&runtime);
    let token = std::env::var("SLOPOS_SESSION_TOKEN").unwrap_or_default();
    let payload = format!(
        "{socket_name}\npid={}\ntoken={token}\nwidth={}\nheight={}\n",
        std::process::id(),
        output_width.max(1),
        output_height.max(1)
    );

    write_session_file(&runtime.join("readiness"), &payload)?;
    write_session_file(
        &runtime.join("client-wayland-display"),
        &format!("{socket_name}\n"),
    )?;

    // Keep the old names for direct compositor invocations and existing QA
    // tooling. They remain inside this session's private runtime directory.
    write_session_file(
        &runtime.join("wayland-display"),
        &format!("{socket_name}\n"),
    )?;
    write_session_file(
        &runtime.join("slopos-client-wayland-display"),
        &format!("{socket_name}\n"),
    )?;
    // Start with desktop/chrome focus until the first ordinary client is
    // mapped and focused. This also gives the shell a stable, session-owned
    // control-plane record during startup.
    write_session_file(&runtime.join("active-toplevel"), "app_id=\n")
}

/// Publish the compositor-authoritative focused application for the shell.
///
/// `ext_foreign_toplevel_list_v1` intentionally exposes identity and title but
/// not activation state. The shell therefore reads this session-scoped,
/// compositor-written record instead of guessing from list order or its own
/// fake window model. An empty `app_id` means focus is on desktop chrome or
/// there is no application focus.
#[cfg(target_os = "linux")]
pub fn publish_active_toplevel(app_id: Option<&str>) -> std::io::Result<()> {
    let runtime = std::env::var_os("SLOPOS_SESSION_RUNTIME_DIR").ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "SLOPOS_SESSION_RUNTIME_DIR is required for active toplevel state",
        )
    })?;
    let app_id = app_id.unwrap_or_default();
    if app_id.contains('\n') || app_id.contains('\r') {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "active application id contains a line break",
        ));
    }
    write_session_file(
        &Path::new(&runtime).join("active-toplevel"),
        &format!("app_id={app_id}\n"),
    )
}

#[cfg(target_os = "linux")]
fn write_session_file(path: &Path, contents: &str) -> std::io::Result<()> {
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&temporary, contents)?;
    std::fs::rename(temporary, path)
}

/// A discovered DRM render/primary node path (session DRM path).
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct DrmNodePath {
    pub path: PathBuf,
    /// True if filename looks like `cardN` (modesetting primary).
    pub is_primary: bool,
}

/// Discover DRM device nodes under `/dev/dri`.
///
/// Pure filesystem scan — works without opening DRM (host-safe unit tests can
/// pass synthetic directory listings via [`discover_drm_nodes_from_names`]).
pub fn discover_drm_nodes() -> Vec<DrmNodePath> {
    let dir = Path::new("/dev/dri");
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for entry in rd.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            names.push(name.to_string());
        }
    }
    discover_drm_nodes_from_names(dir, &names)
}

/// Pure form of DRM node discovery from a directory path + file names.
pub fn discover_drm_nodes_from_names(dir: &Path, names: &[String]) -> Vec<DrmNodePath> {
    let mut out = Vec::new();
    for name in names {
        if name.starts_with("card") || name.starts_with("renderD") {
            out.push(DrmNodePath {
                path: dir.join(name),
                is_primary: name.starts_with("card"),
            });
        }
    }
    // Prefer primary cards first for session bootstrap.
    out.sort_by_key(|n| (!n.is_primary, n.path.clone()));
    out
}

/// Pick the preferred DRM primary node for session bootstrap.
pub fn preferred_primary_drm_node(nodes: &[DrmNodePath]) -> Option<&DrmNodePath> {
    nodes
        .iter()
        .find(|n| n.is_primary)
        .or_else(|| nodes.first())
}

/// Layer-shell role labels used by shell chrome (bar/dock/notifications).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ChromeLayer {
    Background,
    Bottom,
    Top,
    Overlay,
}

impl ChromeLayer {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::Bottom => "bottom",
            Self::Top => "top",
            Self::Overlay => "overlay",
        }
    }

    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "background" => Some(Self::Background),
            "bottom" => Some(Self::Bottom),
            "top" => Some(Self::Top),
            "overlay" => Some(Self::Overlay),
            _ => None,
        }
    }

    /// z-order key for sorting chrome layers (higher draws above).
    pub fn z_priority(self) -> u8 {
        match self {
            Self::Background => 0,
            Self::Bottom => 1,
            Self::Top => 2,
            Self::Overlay => 3,
        }
    }
}

/// Policy for a layer-shell chrome surface (menu bar, dock, etc.).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LayerChromeSpec {
    pub name: String,
    pub layer: ChromeLayer,
    pub exclusive_zone: i32,
    pub anchor_top: bool,
    pub anchor_bottom: bool,
    pub anchor_left: bool,
    pub anchor_right: bool,
}

impl LayerChromeSpec {
    pub fn menu_bar(height: i32) -> Self {
        Self {
            name: "menu-bar".into(),
            layer: ChromeLayer::Top,
            exclusive_zone: height,
            anchor_top: true,
            anchor_bottom: false,
            anchor_left: true,
            anchor_right: true,
        }
    }

    pub fn dock(height: i32) -> Self {
        Self {
            name: "dock".into(),
            layer: ChromeLayer::Bottom,
            exclusive_zone: height,
            anchor_top: false,
            anchor_bottom: true,
            anchor_left: true,
            anchor_right: true,
        }
    }

    pub fn notification_overlay() -> Self {
        Self {
            name: "notifications".into(),
            layer: ChromeLayer::Overlay,
            exclusive_zone: 0,
            anchor_top: true,
            anchor_bottom: false,
            anchor_left: false,
            anchor_right: true,
        }
    }
}

/// Sort chrome specs by layer priority then name (stable layout order).
pub fn sort_chrome_layers(specs: &mut [LayerChromeSpec]) {
    specs.sort_by(|a, b| {
        a.layer
            .z_priority()
            .cmp(&b.layer.z_priority())
            .then_with(|| a.name.cmp(&b.name))
    });
}

/// Indices for composing surfaces: background/bottom first, then windows, then top/overlay.
///
/// Used by nested `render_frame` so layer-shell chrome is not skipped when buffers commit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComposeOrder {
    /// Indices into the caller's layer surface list, low-to-high paint order.
    pub layer_indices_bottom_first: Vec<usize>,
    /// Whether windows should paint after bottom layers and before top/overlay.
    pub windows_after_bottom: bool,
}

/// Pure planner: given layer z priorities (higher = above), return paint order indices.
///
/// Layers with `z <= 1` (Background/Bottom) paint under windows; `z >= 2` (Top/Overlay)
/// paint above windows.
pub fn plan_compose_order(layer_z: &[u8]) -> ComposeOrder {
    let mut under: Vec<(u8, usize)> = Vec::new();
    let mut over: Vec<(u8, usize)> = Vec::new();
    for (i, &z) in layer_z.iter().enumerate() {
        if z <= 1 {
            under.push((z, i));
        } else {
            over.push((z, i));
        }
    }
    under.sort_by_key(|(z, i)| (*z, *i));
    over.sort_by_key(|(z, i)| (*z, *i));
    let mut layer_indices_bottom_first: Vec<usize> = under.into_iter().map(|(_, i)| i).collect();
    layer_indices_bottom_first.extend(over.into_iter().map(|(_, i)| i));
    ComposeOrder {
        layer_indices_bottom_first,
        windows_after_bottom: true,
    }
}

/// Map a layer name string to z priority (for tests / policy without smithay types).
pub fn layer_name_z_priority(name: &str) -> Option<u8> {
    ChromeLayer::from_str_loose(name).map(|l| l.z_priority())
}

// ---------------------------------------------------------------------------
// DRM presentation plan (pure) — scanout path stages for SessionDrm
// ---------------------------------------------------------------------------

/// Stages of a real DRM presentation pipeline (beyond open-device only).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum DrmPresentationStage {
    OpenSeat,
    OpenPrimaryNode,
    CreateGbmEgl,
    EnumerateConnectors,
    PickConnectorMode,
    CreateDrmSurface,
    PageFlipOrPresent,
    ProtocolLoop,
}

impl DrmPresentationStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenSeat => "open_seat",
            Self::OpenPrimaryNode => "open_primary_node",
            Self::CreateGbmEgl => "create_gbm_egl",
            Self::EnumerateConnectors => "enumerate_connectors",
            Self::PickConnectorMode => "pick_connector_mode",
            Self::CreateDrmSurface => "create_drm_surface",
            Self::PageFlipOrPresent => "pageflip_or_present",
            Self::ProtocolLoop => "protocol_loop",
        }
    }
}

/// Ordered presentation pipeline for SessionDrm bootstrap.
pub fn drm_presentation_pipeline() -> &'static [DrmPresentationStage] {
    use DrmPresentationStage::*;
    &[
        OpenSeat,
        OpenPrimaryNode,
        CreateGbmEgl,
        EnumerateConnectors,
        PickConnectorMode,
        CreateDrmSurface,
        PageFlipOrPresent,
        ProtocolLoop,
    ]
}

/// Result of attempting connector-based modeset (pure bookkeeping for tests/logs).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DrmModesetPlan {
    pub connector_name: String,
    pub mode_w: i32,
    pub mode_h: i32,
    pub refresh_mhz: i32,
    pub crtc_index: usize,
}

/// Pick a modeset plan from discovered connector summaries.
///
/// Prefers the first connected connector with a preferred mode; falls back to env-sized
/// virtual mode when none are connected (nested/test).
#[allow(clippy::type_complexity)]
pub fn plan_drm_modeset(
    connectors: &[(String, bool, Option<(i32, i32, i32)>)],
    fallback_w: i32,
    fallback_h: i32,
    fallback_refresh_mhz: i32,
) -> DrmModesetPlan {
    for (i, (name, connected, mode)) in connectors.iter().enumerate() {
        if *connected {
            if let Some((w, h, refresh)) = mode {
                return DrmModesetPlan {
                    connector_name: name.clone(),
                    mode_w: *w,
                    mode_h: *h,
                    refresh_mhz: *refresh,
                    crtc_index: i,
                };
            }
        }
    }
    DrmModesetPlan {
        connector_name: "virtual-fallback".into(),
        mode_w: fallback_w,
        mode_h: fallback_h,
        refresh_mhz: fallback_refresh_mhz,
        crtc_index: 0,
    }
}

// ---------------------------------------------------------------------------
// Server-side decoration policy (xdg-decoration)
// ---------------------------------------------------------------------------

/// Preferred window decoration mode for first-party vs external clients.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecorationPreference {
    /// Compositor draws decorations (CSD alternative).
    ServerSide,
    /// Client draws its own decorations.
    ClientSide,
}

/// Decide decoration preference from app_id hints (pure).
pub fn decoration_preference_for_app_id(app_id: &str) -> DecorationPreference {
    let id = app_id.to_ascii_lowercase();
    // First-party suite draws own chrome via kit; external apps get SSD when possible.
    if id.starts_with("slopos-i.")
        || id == "finder"
        || id == "textedit"
        || id == "terminal"
        || id == "settings"
        || id == "appstore"
        || id == "slopos-shell"
    {
        DecorationPreference::ClientSide
    } else {
        DecorationPreference::ServerSide
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputConfig {
    pub width: i32,
    pub height: i32,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            width: DEFAULT_OUTPUT_W,
            height: DEFAULT_OUTPUT_H,
        }
    }
}

impl OutputConfig {
    pub fn from_env() -> Self {
        Self::from_env_values(
            std::env::var("SLOPOS_COMPOSITOR_WIDTH").ok(),
            std::env::var("SLOPOS_COMPOSITOR_HEIGHT").ok(),
        )
    }

    pub fn from_env_values(width: Option<String>, height: Option<String>) -> Self {
        Self {
            width: parse_positive_i32(width).unwrap_or(DEFAULT_OUTPUT_W),
            height: parse_positive_i32(height).unwrap_or(DEFAULT_OUTPUT_H),
        }
    }
}

/// One logical output with a compositor-space origin.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LaidOutOutput {
    pub config: OutputConfig,
    pub x: i32,
    pub y: i32,
}

/// Multi-output arrangement policy (pure).
///
/// Default is [`OutputLayoutMode::SideBySide`]. Selected via
/// `SLOPOS_OUTPUT_LAYOUT` (`side` | `stack` | `grid`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum OutputLayoutMode {
    /// Left-to-right along Y=0 (default).
    #[default]
    SideBySide,
    /// Top-to-bottom along X=0.
    Stacked,
    /// Two-column grid: pairs left-to-right per row, then next row.
    Grid,
}

/// Parse `SLOPOS_OUTPUT_LAYOUT` value (`side` | `stack` | `grid`).
///
/// Unset, empty, or unrecognised → [`OutputLayoutMode::SideBySide`] (default).
pub fn parse_layout_mode(value: Option<&str>) -> OutputLayoutMode {
    let Some(raw) = value else {
        return OutputLayoutMode::SideBySide;
    };
    match raw.trim().to_ascii_lowercase().as_str() {
        "side" | "side-by-side" | "side_by_side" | "sbs" | "horizontal" => {
            OutputLayoutMode::SideBySide
        }
        "stack" | "stacked" | "vertical" => OutputLayoutMode::Stacked,
        "grid" | "2col" | "two-column" | "two_column" => OutputLayoutMode::Grid,
        _ => OutputLayoutMode::SideBySide,
    }
}

/// Read layout mode from `SLOPOS_OUTPUT_LAYOUT` (default side-by-side).
pub fn layout_mode_from_env() -> OutputLayoutMode {
    parse_layout_mode(std::env::var("SLOPOS_OUTPUT_LAYOUT").ok().as_deref())
}

/// Parse `SLOPOS_OUTPUTS=WxH,WxH` (comma-separated). Invalid tokens are skipped.
///
/// Returns an empty vec when the string has no valid entries.
pub fn parse_outputs_spec(spec: &str) -> Vec<OutputConfig> {
    let mut out = Vec::new();
    for part in spec.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let Some((w_str, h_str)) = part.split_once('x').or_else(|| part.split_once('X')) else {
            continue;
        };
        let Ok(w) = w_str.trim().parse::<i32>() else {
            continue;
        };
        let Ok(h) = h_str.trim().parse::<i32>() else {
            continue;
        };
        if w > 0 && h > 0 {
            out.push(OutputConfig {
                width: w,
                height: h,
            });
        }
    }
    out
}

/// One entry from shell `SLOPOS_OUTPUTS_LAYOUT`
/// (`name:WIDTHxHEIGHT@x,y:sSCALE`, semicolon-separated).
///
/// Produced by `slopos-shell` display arrange (`EmitLayoutEnv`). Nested compositor
/// places logical `wl_output`s at these positions; scale percent is retained for
/// logging / future per-output scale (global scale still comes from
/// `SLOPOS_OUTPUT_SCALE` on the nested path).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LayoutOutputEntry {
    pub name: String,
    pub config: OutputConfig,
    pub x: i32,
    pub y: i32,
    /// Scale percent (100 = 1×). Invalid/missing in the token → 100.
    pub scale_percent: u32,
}

impl LayoutOutputEntry {
    /// Convert to compositor-space [`LaidOutOutput`] (drops name/scale).
    pub fn to_laid_out(&self) -> LaidOutOutput {
        LaidOutOutput {
            config: self.config,
            x: self.x,
            y: self.y,
        }
    }
}

/// Parse `SLOPOS_OUTPUTS_LAYOUT` from shell display arrange.
///
/// Format (per head, `;`-separated):
/// `name:WIDTHxHEIGHT@x,y:sSCALE`
///
/// Example: `eDP-1:1920x1080@0,0:s100;HDMI-1:2560x1440@1920,0:s100`
///
/// Invalid tokens are skipped. Returns empty when nothing valid remains.
/// Positions may be negative (extended left/up); size must be positive.
pub fn parse_outputs_layout_spec(spec: &str) -> Vec<LayoutOutputEntry> {
    let mut out = Vec::new();
    for part in spec.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(entry) = parse_one_outputs_layout_entry(part) {
            out.push(entry);
        }
    }
    out
}

/// Parse a single `name:WxH@x,y:sNN` token. Returns `None` on garbage.
fn parse_one_outputs_layout_entry(part: &str) -> Option<LayoutOutputEntry> {
    // name:rest — output names are connector-style (eDP-1, HDMI-A-1); first ':' splits.
    let (name_raw, rest) = part.split_once(':')?;
    let name = name_raw.trim();
    if name.is_empty() {
        return None;
    }

    // Optional trailing :sSCALE (shell always emits it; tolerate absence).
    let (geom, scale_percent) = match rest.rsplit_once(":s") {
        Some((g, s)) => {
            let pct = s
                .trim()
                .parse::<u32>()
                .ok()
                .filter(|&p| p > 0)
                .unwrap_or(100);
            (g, pct)
        }
        None => (rest, 100u32),
    };

    // geom = WIDTHxHEIGHT@x,y
    let (size, pos) = geom.split_once('@')?;
    let (w_str, h_str) = size.split_once('x').or_else(|| size.split_once('X'))?;
    let (x_str, y_str) = pos.split_once(',')?;

    let w = w_str.trim().parse::<i32>().ok()?;
    let h = h_str.trim().parse::<i32>().ok()?;
    let x = x_str.trim().parse::<i32>().ok()?;
    let y = y_str.trim().parse::<i32>().ok()?;
    if w <= 0 || h <= 0 {
        return None;
    }

    Some(LayoutOutputEntry {
        name: name.to_string(),
        config: OutputConfig {
            width: w,
            height: h,
        },
        x,
        y,
        scale_percent,
    })
}

/// Convert layout-spec entries to laid-out outputs (positions preserved).
pub fn laid_out_from_layout_entries(entries: &[LayoutOutputEntry]) -> Vec<LaidOutOutput> {
    entries.iter().map(LayoutOutputEntry::to_laid_out).collect()
}

/// Human-readable one-line summary of a parsed layout-spec (for honest startup logs).
pub fn outputs_layout_spec_summary(entries: &[LayoutOutputEntry]) -> String {
    if entries.is_empty() {
        return "layout-spec: (empty)".into();
    }
    let heads: Vec<String> = entries
        .iter()
        .map(|e| {
            format!(
                "{} {}x{}@({},{}) s{}",
                e.name, e.config.width, e.config.height, e.x, e.y, e.scale_percent
            )
        })
        .collect();
    format!(
        "layout-spec: {} head(s): {}",
        entries.len(),
        heads.join("; ")
    )
}

/// Dispatch layout by [`OutputLayoutMode`].
pub fn layout_outputs(configs: &[OutputConfig], mode: OutputLayoutMode) -> Vec<LaidOutOutput> {
    match mode {
        OutputLayoutMode::SideBySide => layout_outputs_side_by_side(configs),
        OutputLayoutMode::Stacked => layout_outputs_stacked(configs),
        OutputLayoutMode::Grid => layout_outputs_grid(configs),
    }
}

/// Lay out outputs left-to-right starting at (0,0). Y is always 0 for the simple
/// side-by-side policy used under the nested X11 backend.
pub fn layout_outputs_side_by_side(outputs: &[OutputConfig]) -> Vec<LaidOutOutput> {
    let mut x = 0;
    let mut result = Vec::with_capacity(outputs.len());
    for config in outputs {
        result.push(LaidOutOutput {
            config: *config,
            x,
            y: 0,
        });
        x = x.saturating_add(config.width);
    }
    result
}

/// Lay out outputs top-to-bottom starting at (0,0). X is always 0.
pub fn layout_outputs_stacked(outputs: &[OutputConfig]) -> Vec<LaidOutOutput> {
    let mut y = 0;
    let mut result = Vec::with_capacity(outputs.len());
    for config in outputs {
        result.push(LaidOutOutput {
            config: *config,
            x: 0,
            y,
        });
        y = y.saturating_add(config.height);
    }
    result
}

/// Lay out outputs in a 2-column grid (left-to-right within each row, then down).
///
/// Pair `(2k, 2k+1)` share a row at the current `y`. The right output is placed
/// at `x = left.width`. Row height is `max(left.height, right.height)` (or just
/// the single output height for a trailing odd entry).
pub fn layout_outputs_grid(outputs: &[OutputConfig]) -> Vec<LaidOutOutput> {
    let mut result = Vec::with_capacity(outputs.len());
    let mut y = 0;
    let mut i = 0;
    while i < outputs.len() {
        let left = outputs[i];
        if i + 1 < outputs.len() {
            let right = outputs[i + 1];
            result.push(LaidOutOutput {
                config: left,
                x: 0,
                y,
            });
            result.push(LaidOutOutput {
                config: right,
                x: left.width,
                y,
            });
            let row_h = left.height.max(right.height);
            y = y.saturating_add(row_h);
            i += 2;
        } else {
            result.push(LaidOutOutput {
                config: left,
                x: 0,
                y,
            });
            y = y.saturating_add(left.height);
            i += 1;
        }
    }
    result
}

/// Total canvas size covering all laid-out outputs (union bounding box).
pub fn total_output_size(laid_out: &[LaidOutOutput]) -> OutputConfig {
    let Some(bounds) = output_layout_bounds(laid_out) else {
        return OutputConfig::default();
    };
    OutputConfig {
        width: bounds.width.max(1),
        height: bounds.height.max(1),
    }
}

/// Resolve output list from the environment.
///
/// - If `SLOPOS_OUTPUTS` parses to one or more sizes, use those.
/// - Otherwise fall back to a single `OutputConfig::from_env()` (WIDTH/HEIGHT).
///
/// Prefer [`resolve_laid_out_outputs_from_env`] when absolute positions from
/// `SLOPOS_OUTPUTS_LAYOUT` should win (shell display arrange).
pub fn outputs_from_env() -> Vec<OutputConfig> {
    outputs_from_env_values(
        std::env::var("SLOPOS_OUTPUTS").ok(),
        std::env::var("SLOPOS_COMPOSITOR_WIDTH").ok(),
        std::env::var("SLOPOS_COMPOSITOR_HEIGHT").ok(),
    )
}

pub fn outputs_from_env_values(
    outputs_spec: Option<String>,
    width: Option<String>,
    height: Option<String>,
) -> Vec<OutputConfig> {
    if let Some(spec) = outputs_spec {
        let parsed = parse_outputs_spec(&spec);
        if !parsed.is_empty() {
            return parsed;
        }
    }
    vec![OutputConfig::from_env_values(width, height)]
}

/// Where nested multi-output geometry came from (for honest logs).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum OutputsLayoutSource {
    /// Shell `SLOPOS_OUTPUTS_LAYOUT` (`name:WxH@x,y:sNN;...`).
    LayoutSpec,
    /// `SLOPOS_OUTPUTS` sizes + `SLOPOS_OUTPUT_LAYOUT` mode.
    OutputsSpec,
    /// Single default / WIDTH×HEIGHT fallback.
    Default,
}

/// Resolved nested logical outputs with origin positions and connector names.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedOutputsLayout {
    pub laid_out: Vec<LaidOutOutput>,
    /// Connector names when known (`eDP-1`, …); synthetic `X11-N` when not.
    pub names: Vec<String>,
    pub source: OutputsLayoutSource,
}

impl ResolvedOutputsLayout {
    /// One-line summary for startup logs.
    pub fn summary(&self) -> String {
        let heads: Vec<String> = self
            .laid_out
            .iter()
            .enumerate()
            .map(|(i, o)| {
                let name = self.names.get(i).map(|s| s.as_str()).unwrap_or("?");
                format!(
                    "{} {}x{}@({},{})",
                    name, o.config.width, o.config.height, o.x, o.y
                )
            })
            .collect();
        let src = match self.source {
            OutputsLayoutSource::LayoutSpec => "SLOPOS_OUTPUTS_LAYOUT",
            OutputsLayoutSource::OutputsSpec => "SLOPOS_OUTPUTS+layout-mode",
            OutputsLayoutSource::Default => "default",
        };
        format!(
            "outputs source={src} {} head(s): {}",
            self.laid_out.len(),
            if heads.is_empty() {
                "(none)".into()
            } else {
                heads.join("; ")
            }
        )
    }
}

/// Resolve laid-out outputs for nested startup (pure).
///
/// Preference:
/// 1. `layout_spec` (`SLOPOS_OUTPUTS_LAYOUT`) when it parses to ≥1 entry
/// 2. else `outputs_spec` / width / height via [`outputs_from_env_values`] + `layout_mode`
pub fn resolve_laid_out_outputs_from_env_values(
    layout_spec: Option<&str>,
    outputs_spec: Option<String>,
    width: Option<String>,
    height: Option<String>,
    layout_mode: OutputLayoutMode,
) -> ResolvedOutputsLayout {
    if let Some(spec) = layout_spec {
        let entries = parse_outputs_layout_spec(spec);
        if !entries.is_empty() {
            let names: Vec<String> = entries.iter().map(|e| e.name.clone()).collect();
            let laid_out = normalize_laid_out_outputs(&laid_out_from_layout_entries(&entries));
            return ResolvedOutputsLayout {
                laid_out,
                names,
                source: OutputsLayoutSource::LayoutSpec,
            };
        }
    }

    let had_outputs_spec = outputs_spec
        .as_ref()
        .map(|s| !parse_outputs_spec(s).is_empty())
        .unwrap_or(false);
    let had_size_env = width.is_some() || height.is_some();
    let source = if had_outputs_spec || had_size_env {
        OutputsLayoutSource::OutputsSpec
    } else {
        OutputsLayoutSource::Default
    };

    let configs = outputs_from_env_values(outputs_spec, width, height);
    let laid_out = normalize_laid_out_outputs(&layout_outputs(&configs, layout_mode));
    let names: Vec<String> = (0..laid_out.len())
        .map(|i| format!("X11-{}", i + 1))
        .collect();
    ResolvedOutputsLayout {
        laid_out,
        names,
        source,
    }
}

/// Nested startup: read env and resolve laid-out outputs.
///
/// Prefers `SLOPOS_OUTPUTS_LAYOUT`, else `SLOPOS_OUTPUTS` + layout mode,
/// else WIDTH/HEIGHT defaults.
pub fn resolve_laid_out_outputs_from_env() -> ResolvedOutputsLayout {
    resolve_laid_out_outputs_from_env_values(
        std::env::var("SLOPOS_OUTPUTS_LAYOUT").ok().as_deref(),
        std::env::var("SLOPOS_OUTPUTS").ok(),
        std::env::var("SLOPOS_COMPOSITOR_WIDTH").ok(),
        std::env::var("SLOPOS_COMPOSITOR_HEIGHT").ok(),
        layout_mode_from_env(),
    )
}

// ---------------------------------------------------------------------------
// HiDPI / output scale (pure policy)
// ---------------------------------------------------------------------------

/// Fractional output scale as a reduced rational (Wayland-style buffer scale).
///
/// Examples: 1× → `1/1`, 2× → `2/1`, 1.5× → `3/2`. Pure value type — no I/O.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct OutputScale {
    pub numerator: u32,
    pub denominator: u32,
}

impl Default for OutputScale {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl OutputScale {
    /// 1× scale (no HiDPI).
    pub const IDENTITY: Self = Self {
        numerator: 1,
        denominator: 1,
    };

    /// Construct a scale if both parts are non-zero; reduces by GCD.
    pub fn new(numerator: u32, denominator: u32) -> Option<Self> {
        if numerator == 0 || denominator == 0 {
            return None;
        }
        Some(
            Self {
                numerator,
                denominator,
            }
            .reduced(),
        )
    }

    /// Floating-point scale factor (`numerator / denominator`).
    pub fn as_f64(self) -> f64 {
        self.numerator as f64 / self.denominator as f64
    }

    /// Pure env parse: `Some("2")` / `Some("1.5")` / `Some("3/2")` → scale;
    /// unset / empty / invalid → `None` (caller uses [`OutputScale::IDENTITY`]).
    pub fn from_env_value(value: Option<&str>) -> Option<Self> {
        let value = value?.trim();
        if value.is_empty() {
            return None;
        }
        parse_output_scale(value)
    }

    /// Reduce by greatest common divisor (always non-zero parts).
    pub fn reduced(self) -> Self {
        let g = gcd_u32(self.numerator, self.denominator);
        Self {
            numerator: self.numerator / g,
            denominator: self.denominator / g,
        }
    }

    /// True when scale is exactly 1×.
    pub fn is_identity(self) -> bool {
        self.reduced() == Self::IDENTITY
    }

    /// Return the integer buffer scale the current Wayland backends can honor.
    ///
    /// `wl_output.scale` and `wl_surface.set_buffer_scale` are integer-only.
    /// Until SLOPOS advertises the fractional-scale and viewporter protocols,
    /// a requested fractional scale is therefore quantized to the nearest
    /// integer rather than being applied a second time in the compositor.
    /// Examples: 1.25× → 1×, 1.5× → 2×.
    pub fn integer_buffer_scale(self) -> i32 {
        self.as_f64().round().max(1.0).min(i32::MAX as f64) as i32
    }

    /// The effective integer scale represented by [`Self::integer_buffer_scale`].
    pub fn quantized_integer(self) -> Self {
        Self::new(self.integer_buffer_scale() as u32, 1).unwrap_or(Self::IDENTITY)
    }
}

/// Parse an output scale string.
///
/// Accepted forms:
/// - integer: `"2"` → 2/1
/// - decimal: `"1.5"` → 3/2 (up to 3 fractional digits, reduced)
/// - fraction: `"3/2"` → 3/2
///
/// Rejects empty, non-positive, zero denominator, and non-finite values.
pub fn parse_output_scale(raw: &str) -> Option<OutputScale> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }

    if let Some((num_s, den_s)) = s.split_once('/') {
        let num: u32 = num_s.trim().parse().ok()?;
        let den: u32 = den_s.trim().parse().ok()?;
        return OutputScale::new(num, den);
    }

    // Integer without decimal point.
    if !s.contains('.') {
        let n: u32 = s.parse().ok()?;
        return OutputScale::new(n, 1);
    }

    // Decimal: convert via fixed-point (max 3 fractional digits) then reduce.
    let v: f64 = s.parse().ok()?;
    if !v.is_finite() || v <= 0.0 {
        return None;
    }
    // Cap to a sane compositor range (Wayland scale is typically ≤ 8).
    if v > 64.0 {
        return None;
    }
    const PLACE: u32 = 1000;
    let num = (v * f64::from(PLACE)).round();
    if num <= 0.0 || num > f64::from(u32::MAX) {
        return None;
    }
    OutputScale::new(num as u32, PLACE)
}

/// Read `SLOPOS_OUTPUT_SCALE` (e.g. `2`, `1.5`, `3/2`).
///
/// Returns `None` when unset or invalid so callers can default to 1×.
pub fn detect_output_scale_from_env() -> Option<OutputScale> {
    OutputScale::from_env_value(std::env::var("SLOPOS_OUTPUT_SCALE").ok().as_deref())
}

/// Scale a logical size to physical pixels (ceil, never undersized).
///
/// `physical = ceil(logical * numerator / denominator)`.
pub fn scale_logical_to_physical(size: (i32, i32), scale: OutputScale) -> (i32, i32) {
    (
        scale_dim_logical_to_physical(size.0, scale),
        scale_dim_logical_to_physical(size.1, scale),
    )
}

/// Scale a physical size to logical coordinates (floor).
///
/// `logical = floor(physical * denominator / numerator)`.
pub fn scale_physical_to_logical(size: (i32, i32), scale: OutputScale) -> (i32, i32) {
    (
        scale_dim_physical_to_logical(size.0, scale),
        scale_dim_physical_to_logical(size.1, scale),
    )
}

/// Apply scale to an [`OutputConfig`] treated as **logical** dimensions.
///
/// Returns physical width/height for framebuffer / buffer allocation. Pure:
/// does not mutate global state or store scale on the config (config remains a
/// size only). Identity scale is a no-op.
pub fn apply_scale_to_output_config(cfg: OutputConfig, scale: OutputScale) -> OutputConfig {
    let (width, height) = scale_logical_to_physical((cfg.width, cfg.height), scale);
    OutputConfig {
        width: width.max(1),
        height: height.max(1),
    }
}

/// One-line log label for output scale policy.
pub fn output_scale_summary(scale: OutputScale) -> String {
    let s = scale.reduced();
    format!(
        "output_scale={}/{} ({:.2}x)",
        s.numerator,
        s.denominator,
        s.as_f64()
    )
}

fn scale_dim_logical_to_physical(logical: i32, scale: OutputScale) -> i32 {
    if logical <= 0 {
        return logical.min(0);
    }
    let num = i64::from(scale.numerator);
    let den = i64::from(scale.denominator).max(1);
    let v = (i64::from(logical) * num + den - 1) / den;
    i32::try_from(v).unwrap_or(i32::MAX).max(1)
}

fn scale_dim_physical_to_logical(physical: i32, scale: OutputScale) -> i32 {
    if physical <= 0 {
        return physical.min(0);
    }
    let num = i64::from(scale.numerator).max(1);
    let den = i64::from(scale.denominator);
    let v = (i64::from(physical) * den) / num;
    i32::try_from(v).unwrap_or(i32::MAX).max(0)
}

fn gcd_u32(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a.max(1)
}

/// Compositor display policy (HDR / VRR / refresh / color space).
///
/// Resolved from optional `settings.conf` keys then overridden by environment
/// variables. Nested X11/Xvfb has no real HDR path; `hdr_supported` stays false
/// unless hardware detection (elsewhere) proves otherwise.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayPolicy {
    pub hdr_requested: bool,
    pub vrr_adaptive: bool,
    pub refresh_rate: RefreshRate,
    pub color_space: ColorSpace,
}

impl Default for DisplayPolicy {
    fn default() -> Self {
        Self {
            hdr_requested: false,
            vrr_adaptive: false,
            refresh_rate: RefreshRate::Hz60,
            color_space: ColorSpace::SRgb,
        }
    }
}

impl DisplayPolicy {
    /// Full resolution order: defaults → settings file → environment (env wins).
    pub fn resolve() -> Self {
        let mut policy = Self::default();
        if let Some(path) = settings_conf_path() {
            if let Ok(text) = std::fs::read_to_string(&path) {
                policy.apply_settings_text(&text);
            }
        }
        policy.apply_env_map(std::env::vars().collect());
        policy
    }

    /// Apply flat `key=value` lines from settings.conf (or tests).
    ///
    /// Recognised keys: `hdr_requested` / `hdr_request`, `vrr_adaptive`,
    /// `refresh_rate`, `color_space`.
    pub fn apply_settings_text(&mut self, text: &str) {
        for (key, value) in parse_key_value_conf(text) {
            match key.as_str() {
                "hdr_requested" | "hdr_request" => {
                    if let Some(b) = parse_bool_loose(&value) {
                        self.hdr_requested = b;
                    }
                }
                "vrr_adaptive" | "vrr_enabled" => {
                    if let Some(b) = parse_bool_loose(&value) {
                        self.vrr_adaptive = b;
                    }
                }
                "refresh_rate" => {
                    if let Some(r) = RefreshRate::parse_flexible(&value) {
                        self.refresh_rate = r;
                    }
                }
                "color_space" => {
                    if let Some(cs) = ColorSpace::from_str_flexible(&value) {
                        self.color_space = cs;
                    }
                }
                _ => {}
            }
        }
    }

    /// Apply environment overrides.
    ///
    /// - `SLOPOS_HDR` — truthy enables HDR request
    /// - `SLOPOS_VRR` — truthy enables adaptive VRR
    /// - `SLOPOS_REFRESH` — e.g. `60`, `60hz`, `adaptive`
    /// - `SLOPOS_COLOR_SPACE` — `srgb` / `rec2020` / `scrgb`
    pub fn apply_env_map(&mut self, env: HashMap<String, String>) {
        if let Some(v) = env.get("SLOPOS_HDR") {
            if let Some(b) = parse_bool_loose(v) {
                self.hdr_requested = b;
            }
        }
        if let Some(v) = env.get("SLOPOS_VRR") {
            if let Some(b) = parse_bool_loose(v) {
                self.vrr_adaptive = b;
            }
        }
        if let Some(v) = env.get("SLOPOS_REFRESH") {
            if let Some(r) = RefreshRate::parse_flexible(v) {
                self.refresh_rate = r;
            }
        }
        if let Some(v) = env.get("SLOPOS_COLOR_SPACE") {
            if let Some(cs) = ColorSpace::from_str_flexible(v) {
                self.color_space = cs;
            }
        }
    }

    /// Effective refresh rate after VRR policy (Adaptive when vrr_adaptive).
    pub fn effective_refresh_rate(&self) -> RefreshRate {
        if self.vrr_adaptive {
            RefreshRate::Adaptive
        } else {
            self.refresh_rate
        }
    }

    /// Human-readable one-line summary for logging.
    pub fn summary_line(&self, hdr_supported: bool) -> String {
        format!(
            "hdr_requested={} hdr_supported={} vrr_adaptive={} refresh={} color_space={}",
            self.hdr_requested,
            hdr_supported,
            self.vrr_adaptive,
            self.effective_refresh_rate().as_str(),
            self.color_space.as_str(),
        )
    }
}

/// Look up mime payload bytes in a selection store. Returns `None` when missing
/// (callers should close the fd for EOF without hanging the client).
pub fn selection_bytes_for_mime<'a>(
    store: &'a HashMap<String, Vec<u8>>,
    mime_type: &str,
) -> Option<&'a [u8]> {
    store.get(mime_type).map(|v| v.as_slice())
}

/// Prefer exact mime match; fall back to `text/plain` / `TEXT` / `STRING` for text clients.
pub fn selection_bytes_for_mime_with_text_fallback<'a>(
    store: &'a HashMap<String, Vec<u8>>,
    mime_type: &str,
) -> Option<&'a [u8]> {
    if let Some(b) = selection_bytes_for_mime(store, mime_type) {
        return Some(b);
    }
    const TEXT_FALLBACKS: &[&str] = &[
        "text/plain;charset=utf-8",
        "text/plain",
        "UTF8_STRING",
        "STRING",
        "TEXT",
    ];
    if mime_type.starts_with("text/")
        || mime_type.eq_ignore_ascii_case("STRING")
        || mime_type.eq_ignore_ascii_case("TEXT")
        || mime_type.eq_ignore_ascii_case("UTF8_STRING")
    {
        for candidate in TEXT_FALLBACKS {
            if let Some(b) = selection_bytes_for_mime(store, candidate) {
                return Some(b);
            }
        }
    }
    None
}

/// Canonical mime offer list for a server-set text selection (Wayland + X11 bridge).
pub fn text_selection_mime_offers() -> &'static [&'static str] {
    &[
        "text/plain;charset=utf-8",
        "text/plain",
        "UTF8_STRING",
        "TEXT",
        "STRING",
        "text/html",
    ]
}

/// Build a selection store from UTF-8 text with standard mime offers.
pub fn selection_store_from_utf8_text(text: &str) -> HashMap<String, Vec<u8>> {
    let bytes = text.as_bytes().to_vec();
    let mut store = HashMap::new();
    for mime in text_selection_mime_offers() {
        if *mime == "text/html" {
            let html = format!(
                "<pre>{}</pre>",
                text.replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;")
            );
            store.insert((*mime).to_string(), html.into_bytes());
        } else {
            store.insert((*mime).to_string(), bytes.clone());
        }
    }
    store
}

/// Pure: whether a client mime request is satisfied by store (with text fallback).
pub fn selection_can_satisfy(store: &HashMap<String, Vec<u8>>, mime: &str) -> bool {
    selection_bytes_for_mime_with_text_fallback(store, mime).is_some()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowGeometry {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl WindowGeometry {
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn contains_f64(self, x: f64, y: f64) -> bool {
        let x = x as i32;
        let y = y as i32;
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}

/// Clamp a normal window to the compositor-owned work area.
///
/// Client-requested sizes are logical pixels.  The work area is the only
/// authoritative rectangle after output scale and layer-shell exclusive zones
/// have been resolved, so a default 640×480 request must not cover the Dock on
/// a small logical HiDPI output.  Both nested and DRM backends use this helper
/// before sending the initial configure and when chrome changes the work area.
pub fn clamp_window_to_work_area(
    desired: WindowGeometry,
    work_area: WindowGeometry,
) -> WindowGeometry {
    let work_width = work_area.width.max(1);
    let work_height = work_area.height.max(1);
    let width = desired
        .width
        .max(MIN_WINDOW_W.min(work_width))
        .min(work_width);
    let height = desired
        .height
        .max(MIN_WINDOW_H.min(work_height))
        .min(work_height);
    let max_x = work_area.x.saturating_add(work_width.saturating_sub(width));
    let max_y = work_area
        .y
        .saturating_add(work_height.saturating_sub(height));
    WindowGeometry::new(
        desired.x.clamp(work_area.x, max_x),
        desired.y.clamp(work_area.y, max_y),
        width,
        height,
    )
}

/// Edges involved in a compositor-owned interactive resize operation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ResizeEdges {
    pub top: bool,
    pub bottom: bool,
    pub left: bool,
    pub right: bool,
}

impl ResizeEdges {
    pub const TOP: Self = Self {
        top: true,
        bottom: false,
        left: false,
        right: false,
    };
    pub const BOTTOM: Self = Self {
        top: false,
        bottom: true,
        left: false,
        right: false,
    };
    pub const LEFT: Self = Self {
        top: false,
        bottom: false,
        left: true,
        right: false,
    };
    pub const RIGHT: Self = Self {
        top: false,
        bottom: false,
        left: false,
        right: true,
    };
    pub const TOP_LEFT: Self = Self {
        top: true,
        bottom: false,
        left: true,
        right: false,
    };
    pub const TOP_RIGHT: Self = Self {
        top: true,
        bottom: false,
        left: false,
        right: true,
    };
    pub const BOTTOM_LEFT: Self = Self {
        top: false,
        bottom: true,
        left: true,
        right: false,
    };
    pub const BOTTOM_RIGHT: Self = Self {
        top: false,
        bottom: true,
        left: false,
        right: true,
    };

    pub fn is_empty(self) -> bool {
        !(self.top || self.bottom || self.left || self.right)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InteractiveGrabKind {
    Move,
    Resize(ResizeEdges),
}

/// Pure compositor state captured at the start of an xdg_toplevel move/resize.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InteractiveGrab {
    pub window_id: String,
    pub kind: InteractiveGrabKind,
    pub start_pointer_x: i32,
    pub start_pointer_y: i32,
    pub start_geometry: WindowGeometry,
}

impl InteractiveGrab {
    pub fn moving(
        window_id: impl Into<String>,
        pointer_x: i32,
        pointer_y: i32,
        geometry: WindowGeometry,
    ) -> Self {
        Self {
            window_id: window_id.into(),
            kind: InteractiveGrabKind::Move,
            start_pointer_x: pointer_x,
            start_pointer_y: pointer_y,
            start_geometry: geometry,
        }
    }

    pub fn resizing(
        window_id: impl Into<String>,
        edges: ResizeEdges,
        pointer_x: i32,
        pointer_y: i32,
        geometry: WindowGeometry,
    ) -> Option<Self> {
        if edges.is_empty() {
            return None;
        }
        Some(Self {
            window_id: window_id.into(),
            kind: InteractiveGrabKind::Resize(edges),
            start_pointer_x: pointer_x,
            start_pointer_y: pointer_y,
            start_geometry: geometry,
        })
    }
}

/// Validate the authorization preconditions for an xdg_toplevel move/resize.
///
/// Wayland clients must submit the serial from a live pointer button press on
/// the same surface and through a seat owned by the compositor. Keeping this
/// predicate independent of Smithay makes the security boundary testable
/// without constructing a live Wayland display.
pub fn pointer_grab_request_is_valid(
    request_serial: u32,
    pressed_serial: Option<u32>,
    same_surface: bool,
    left_button_down: bool,
    seat_owned: bool,
) -> bool {
    request_serial != 0
        && seat_owned
        && left_button_down
        && same_surface
        && pressed_serial == Some(request_serial)
}

/// Validate an xdg_toplevel move/resize against the mapped window that owned
/// the initiating pointer press.
///
/// Backends must derive `pressed_window_id` only by resolving the hit surface
/// through a known mapped toplevel or one of its tracked popup surface trees.
/// A same-client surface that is not in either tree therefore supplies `None`
/// and cannot authorize a grab.
pub fn pointer_grab_request_is_valid_for_window(
    request_serial: u32,
    pressed_serial: Option<u32>,
    requested_window_id: &str,
    pressed_window_id: Option<&str>,
    left_button_down: bool,
    seat_owned: bool,
    same_client: bool,
) -> bool {
    same_client
        && pressed_window_id == Some(requested_window_id)
        && pointer_grab_request_is_valid(
            request_serial,
            pressed_serial,
            true,
            left_button_down,
            seat_owned,
        )
}

/// Climb Smithay's committed subsurface ancestry to the role-bearing tree
/// root. Mapping that root to a known toplevel or tracked popup remains the
/// backend's responsibility.
#[cfg(target_os = "linux")]
pub fn surface_tree_root(
    surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
) -> smithay::reexports::wayland_server::protocol::wl_surface::WlSurface {
    use smithay::wayland::compositor::get_parent;

    let mut root = surface.clone();
    while let Some(parent) = get_parent(&root) {
        root = parent;
    }
    root
}

/// Clear the shared state that is valid only while the initiating left-button
/// press is held. The backend performs any live-surface configure cleanup
/// after taking the grab returned here.
pub fn clear_interactive_grab_state<T>(
    grab: &mut Option<InteractiveGrab>,
    pointer_press: &mut Option<T>,
    left_button_down: &mut bool,
) -> Option<InteractiveGrab> {
    *left_button_down = false;
    pointer_press.take();
    grab.take()
}

/// Resolve an interactive move/resize against the current pointer position.
///
/// Geometry remains inside the compositor output and never shrinks below the
/// provided minimum.  Keeping this policy pure makes nested and DRM backends
/// share exactly the same window-management behaviour.
pub fn geometry_for_interactive_grab(
    grab: &InteractiveGrab,
    pointer_x: i32,
    pointer_y: i32,
    min_width: i32,
    min_height: i32,
    output_width: i32,
    output_height: i32,
) -> WindowGeometry {
    let dx = pointer_x - grab.start_pointer_x;
    let dy = pointer_y - grab.start_pointer_y;
    let min_width = min_width.max(1);
    let min_height = min_height.max(1);
    let output_width = output_width.max(min_width);
    let output_height = output_height.max(min_height);
    let start = grab.start_geometry;

    match grab.kind {
        InteractiveGrabKind::Move => WindowGeometry::new(
            start
                .x
                .saturating_add(dx)
                .clamp(0, output_width.saturating_sub(start.width.max(0))),
            start
                .y
                .saturating_add(dy)
                .clamp(0, output_height.saturating_sub(start.height.max(0))),
            start.width,
            start.height,
        ),
        InteractiveGrabKind::Resize(edges) => {
            let (left, width) = resize_axis(
                start.x,
                start.width,
                dx,
                min_width,
                output_width,
                edges.left,
                edges.right,
            );
            let (top, height) = resize_axis(
                start.y,
                start.height,
                dy,
                min_height,
                output_height,
                edges.top,
                edges.bottom,
            );
            WindowGeometry::new(left, top, width, height)
        }
    }
}

fn resize_axis(
    start_position: i32,
    start_size: i32,
    delta: i32,
    min_size: i32,
    output_size: i32,
    leading: bool,
    trailing: bool,
) -> (i32, i32) {
    let start_end = start_position.saturating_add(start_size.max(0));
    let mut position = start_position;
    let mut end = start_end;

    if leading {
        let max_position = output_size
            .saturating_sub(min_size)
            .min(start_end.saturating_sub(min_size))
            .max(0);
        position = start_position.saturating_add(delta).clamp(0, max_position);
    }
    if trailing {
        let min_end = position.saturating_add(min_size).min(output_size);
        end = start_end.saturating_add(delta).clamp(min_end, output_size);
    }

    (
        position,
        end.saturating_sub(position).clamp(min_size, output_size),
    )
}

pub fn cascade_position(offset: i32) -> (i32, i32) {
    (INITIAL_WINDOW_X + offset, INITIAL_WINDOW_Y + offset)
}

pub fn next_cascade_offset(offset: i32) -> i32 {
    (offset + CASCADE_STEP) % CASCADE_WRAP
}

pub fn topmost_window_at(windows: &[WindowGeometry], x: f64, y: f64) -> Option<usize> {
    windows
        .iter()
        .enumerate()
        .rev()
        .find(|(_, window)| window.contains_f64(x, y))
        .map(|(idx, _)| idx)
}

pub fn move_to_top<T>(windows: &mut Vec<T>, idx: usize) {
    let window = windows.remove(idx);
    windows.push(window);
}

/// Identifier for a compositor-managed client surface (independent process).
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct ClientSurfaceId(pub u64);

/// One mapped client window in compositor space (multi-client session model).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MappedClientWindow {
    pub id: ClientSurfaceId,
    pub title: String,
    pub geometry: WindowGeometry,
    /// Process id of the Wayland/X11 client when known (0 = unknown).
    pub pid: u32,
}

/// Focus and z-order stack for independent client windows.
///
/// Back is bottom; front is topmost / focused. Pure policy — used by the
/// Linux compositor runtime and host unit tests.
#[derive(Clone, Debug, Default)]
pub struct ClientWindowStack {
    windows: Vec<MappedClientWindow>,
    next_id: u64,
    cascade_offset: i32,
}

impl ClientWindowStack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.windows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }

    pub fn windows(&self) -> &[MappedClientWindow] {
        &self.windows
    }

    /// Map a new client surface; returns its id. Cascades position like classic DE.
    pub fn map_window(&mut self, title: impl Into<String>, pid: u32) -> ClientSurfaceId {
        let (x, y) = cascade_position(self.cascade_offset);
        self.cascade_offset = next_cascade_offset(self.cascade_offset);
        self.map_window_at(
            title,
            pid,
            WindowGeometry::new(x, y, DEFAULT_WINDOW_W, DEFAULT_WINDOW_H),
        )
    }

    /// Map a client at an explicit geometry (tests / multi-output placement).
    pub fn map_window_at(
        &mut self,
        title: impl Into<String>,
        pid: u32,
        geometry: WindowGeometry,
    ) -> ClientSurfaceId {
        let id = ClientSurfaceId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.windows.push(MappedClientWindow {
            id: id.clone(),
            title: title.into(),
            geometry,
            pid,
        });
        id
    }

    /// Remove a mapped window; returns true if found.
    pub fn unmap(&mut self, id: &ClientSurfaceId) -> bool {
        if let Some(idx) = self.windows.iter().position(|w| &w.id == id) {
            self.windows.remove(idx);
            true
        } else {
            false
        }
    }

    /// Focus / raise by id (moves to top of z-order).
    pub fn focus(&mut self, id: &ClientSurfaceId) -> bool {
        if let Some(idx) = self.windows.iter().position(|w| &w.id == id) {
            move_to_top(&mut self.windows, idx);
            true
        } else {
            false
        }
    }

    /// Focus topmost window containing the point (click-to-raise).
    pub fn focus_at(&mut self, x: f64, y: f64) -> Option<ClientSurfaceId> {
        let geos: Vec<WindowGeometry> = self.windows.iter().map(|w| w.geometry).collect();
        let idx = topmost_window_at(&geos, x, y)?;
        let id = self.windows[idx].id.clone();
        move_to_top(&mut self.windows, idx);
        Some(id)
    }

    /// Currently focused window (top of stack), if any.
    pub fn focused(&self) -> Option<&MappedClientWindow> {
        self.windows.last()
    }

    /// Z-order from bottom to top (ids only).
    pub fn z_order_ids(&self) -> Vec<ClientSurfaceId> {
        self.windows.iter().map(|w| w.id.clone()).collect()
    }
}

fn parse_positive_i32(value: Option<String>) -> Option<i32> {
    value?.parse::<i32>().ok().filter(|value| *value > 0)
}

fn parse_bool_loose(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

/// Parse flat `key=value` lines; `#` comments and blank lines ignored.
pub fn parse_key_value_conf(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let k = k.trim();
        let v = v.trim();
        if !k.is_empty() {
            out.push((k.to_string(), v.to_string()));
        }
    }
    out
}

fn settings_conf_path() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("SLOPOS_CONFIG_DIR") {
        return Some(Path::new(&dir).join("settings.conf"));
    }
    if let Ok(home) = std::env::var("HOME") {
        return Some(
            Path::new(&home)
                .join(".config")
                .join("slopos-i")
                .join("settings.conf"),
        );
    }
    None
}

// ---------------------------------------------------------------------------
// Text-input / IME capability policy (pure)
// ---------------------------------------------------------------------------

/// Compositor preference for text-input-v3 / input-method availability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextInputCapability {
    /// No IME; clients use raw key events only.
    None,
    /// text-input-v3 global advertised without input-method-v2.
    TextInputV3,
    /// Smithay-backed input-method-v2 + text-input-v3 lifecycle.
    InputMethodAndTextInput,
}

/// Pure policy: which text-input features the session claims.
pub fn text_input_capability_from_env(value: Option<&str>) -> TextInputCapability {
    match value.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("im" | "input-method" | "full") => TextInputCapability::InputMethodAndTextInput,
        Some("text-input" | "v3" | "1" | "true" | "on") => TextInputCapability::TextInputV3,
        _ => TextInputCapability::None,
    }
}

pub fn text_input_capability_summary(cap: TextInputCapability) -> &'static str {
    match cap {
        TextInputCapability::None => "text_input=none",
        TextInputCapability::TextInputV3 => "text_input=text-input-v3",
        TextInputCapability::InputMethodAndTextInput => "text_input=im+text-input-v3",
    }
}

/// One client surface placement for DRM scanout composition planning (pure).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScanoutElement {
    pub id: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    /// Higher paints above.
    pub z: i32,
}

/// Pure: sort scanout elements back-to-front for a DRM present pass.
pub fn plan_scanout_paint_order(elements: &mut [ScanoutElement]) {
    elements.sort_by(|a, b| a.z.cmp(&b.z).then_with(|| a.id.cmp(&b.id)));
}

/// Pure: clip element rect to output bounds; returns None if fully outside.
pub fn clip_scanout_element_to_output(
    el: &ScanoutElement,
    out_w: i32,
    out_h: i32,
) -> Option<(i32, i32, i32, i32)> {
    let x0 = el.x.max(0);
    let y0 = el.y.max(0);
    let x1 = (el.x + el.w).min(out_w);
    let y1 = (el.y + el.h).min(out_h);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some((x0, y0, x1 - x0, y1 - y0))
}

/// Damage rectangle for partial present (pure).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DamageRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl DamageRect {
    pub fn from_xywh(x: i32, y: i32, w: i32, h: i32) -> Option<Self> {
        if w <= 0 || h <= 0 {
            return None;
        }
        Some(Self { x, y, w, h })
    }

    pub fn union(self, other: Self) -> Self {
        let x0 = self.x.min(other.x);
        let y0 = self.y.min(other.y);
        let x1 = (self.x + self.w).max(other.x + other.w);
        let y1 = (self.y + self.h).max(other.y + other.h);
        Self {
            x: x0,
            y: y0,
            w: x1 - x0,
            h: y1 - y0,
        }
    }

    pub fn area(self) -> i64 {
        i64::from(self.w) * i64::from(self.h)
    }
}

/// Pure: damage region from a rectangular area (window bounds, chrome strip, …).
///
/// Returns `None` when `w` or `h` is non-positive.
pub fn damage_region(x: i32, y: i32, w: i32, h: i32) -> Option<DamageRect> {
    DamageRect::from_xywh(x, y, w, h)
}

/// Pure: damage covering both old and new geometries after a window move/resize.
pub fn damage_region_for_geometry_change(
    old: WindowGeometry,
    new: WindowGeometry,
) -> Option<DamageRect> {
    let a = damage_region(old.x, old.y, old.width, old.height);
    let b = damage_region(new.x, new.y, new.width, new.height);
    match (a, b) {
        (Some(a), Some(b)) => Some(a.union(b)),
        (a, b) => a.or(b),
    }
}

/// Pure: fold a damage rect into an optional accumulator.
pub fn accumulate_damage_rect(acc: Option<DamageRect>, region: DamageRect) -> DamageRect {
    match acc {
        None => region,
        Some(a) => a.union(region),
    }
}

/// Pure: accumulate damage from dirty scanout elements into a single rect.
pub fn accumulate_damage(elements: &[ScanoutElement], dirty_ids: &[&str]) -> Option<DamageRect> {
    let mut acc: Option<DamageRect> = None;
    for el in elements {
        if !dirty_ids.iter().any(|id| *id == el.id) {
            continue;
        }
        let Some(r) = damage_region(el.x, el.y, el.w, el.h) else {
            continue;
        };
        acc = Some(accumulate_damage_rect(acc, r));
    }
    acc
}

/// Pure: when a window moves, mark its (old+new) extents dirty via scanout ids.
///
/// Builds temporary scanout elements for `old`/`new` under `window_id` and
/// returns [`accumulate_damage`] over both — used by the live compositor when
/// geometry changes so partial present has a real dirty rect.
pub fn accumulate_damage_for_window_move(
    window_id: &str,
    old: WindowGeometry,
    new: WindowGeometry,
) -> Option<DamageRect> {
    let old_id = format!("{window_id}:old");
    let new_id = format!("{window_id}:new");
    let elements = [
        ScanoutElement {
            id: old_id.clone(),
            x: old.x,
            y: old.y,
            w: old.width,
            h: old.height,
            z: 0,
        },
        ScanoutElement {
            id: new_id.clone(),
            x: new.x,
            y: new.y,
            w: new.width,
            h: new.height,
            z: 0,
        },
    ];
    accumulate_damage(&elements, &[old_id.as_str(), new_id.as_str()])
}

/// Whether a full redraw is cheaper than partial (heuristic).
pub fn prefer_full_redraw(damage: DamageRect, output_w: i32, output_h: i32) -> bool {
    let out = i64::from(output_w.max(1)) * i64::from(output_h.max(1));
    damage.area() * 2 >= out
}

/// Session counters for solid-placeholder present honesty.
///
/// Prefer real SHM/surface trees; when a frame falls back to placeholders,
/// [`PlaceholderPresentStats::note_frame_with_placeholders`] increments the
/// counter and returns `true` **once per session** so the compositor can log.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PlaceholderPresentStats {
    /// Frames that painted at least one solid placeholder rect.
    pub frames_with_placeholders: u64,
    /// Whether the one-shot session log has already been requested.
    pub logged_once: bool,
}

impl PlaceholderPresentStats {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a frame that used placeholders.
    ///
    /// Returns `true` exactly once (first placeholder frame) so the caller can
    /// emit a single session log line including the running counter.
    pub fn note_frame_with_placeholders(&mut self) -> bool {
        self.frames_with_placeholders = self.frames_with_placeholders.saturating_add(1);
        if !self.logged_once {
            self.logged_once = true;
            true
        } else {
            false
        }
    }
}

// ---------------------------------------------------------------------------
// Virtual workspaces (pure) — compositor-backed desktop model
// ---------------------------------------------------------------------------

/// Fixed number of virtual workspaces (indices `0..WORKSPACE_COUNT`).
pub const WORKSPACE_COUNT: u8 = 8;

/// Workspace index in `0..WORKSPACE_COUNT` (eight desktops).
///
/// Construct via [`WorkspaceId::new`] to reject out-of-range values. The raw
/// field is public for pattern matching / serialization, but methods that take
/// a [`WorkspaceId`] still validate `0..WORKSPACE_COUNT`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct WorkspaceId(pub u8);

impl WorkspaceId {
    /// First workspace (`0`).
    pub const FIRST: Self = Self(0);
    /// Last workspace (`WORKSPACE_COUNT - 1`).
    pub const LAST: Self = Self(WORKSPACE_COUNT - 1);

    /// Valid id in `0..WORKSPACE_COUNT`, or `None`.
    pub fn new(id: u8) -> Option<Self> {
        if id < WORKSPACE_COUNT {
            Some(Self(id))
        } else {
            None
        }
    }

    /// Raw index (`0..7` when valid).
    pub fn get(self) -> u8 {
        self.0
    }

    /// `usize` form for indexing vectors / shell interop.
    pub fn as_usize(self) -> usize {
        usize::from(self.0)
    }

    /// True when `0 <= id < WORKSPACE_COUNT`.
    pub fn is_valid(self) -> bool {
        self.0 < WORKSPACE_COUNT
    }

    /// Next workspace, wrapping `LAST → FIRST`. Invalid ids normalize to `FIRST`.
    pub fn next_wrapping(self) -> Self {
        if !self.is_valid() {
            return Self::FIRST;
        }
        Self((self.0 + 1) % WORKSPACE_COUNT)
    }

    /// Previous workspace, wrapping `FIRST → LAST`. Invalid ids normalize to `LAST`.
    pub fn prev_wrapping(self) -> Self {
        if !self.is_valid() {
            return Self::LAST;
        }
        if self.0 == 0 {
            Self::LAST
        } else {
            Self(self.0 - 1)
        }
    }

    /// All workspace ids in order (`0..WORKSPACE_COUNT`).
    pub fn all() -> impl Iterator<Item = Self> {
        (0..WORKSPACE_COUNT).map(Self)
    }
}

impl std::fmt::Display for WorkspaceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Pure virtual-desktop state: which workspace is active and which workspace
/// each mapped window lives on.
///
/// Window keys are opaque id strings (compositor surface id, uuid, app+title,
/// etc.). Visibility is membership on the active workspace only — untracked
/// keys are not visible.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceState {
    pub active: WorkspaceId,
    /// `window_id → workspace`.
    pub windows: HashMap<String, WorkspaceId>,
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceState {
    /// Empty mapping, active workspace `0`.
    pub fn new() -> Self {
        Self {
            active: WorkspaceId::FIRST,
            windows: HashMap::new(),
        }
    }

    /// Insert or update a window's workspace assignment.
    ///
    /// Invalid `workspace` is clamped via reject: returns `false` and leaves
    /// state unchanged. Valid assignment returns `true`.
    pub fn assign_window(&mut self, window_id: impl Into<String>, workspace: WorkspaceId) -> bool {
        if !workspace.is_valid() {
            return false;
        }
        self.windows.insert(window_id.into(), workspace);
        true
    }

    /// Move an **already tracked** window to another workspace.
    ///
    /// Returns `false` if the window is unknown or `workspace` is invalid.
    pub fn move_to_workspace(&mut self, window_id: &str, workspace: WorkspaceId) -> bool {
        if !workspace.is_valid() {
            return false;
        }
        match self.windows.get_mut(window_id) {
            Some(slot) => {
                *slot = workspace;
                true
            }
            None => false,
        }
    }

    /// Remove a window from tracking. Returns its previous workspace if known.
    pub fn remove_window(&mut self, window_id: &str) -> Option<WorkspaceId> {
        self.windows.remove(window_id)
    }

    /// Switch the active workspace. Returns `false` if `workspace` is invalid.
    pub fn activate(&mut self, workspace: WorkspaceId) -> bool {
        if !workspace.is_valid() {
            return false;
        }
        self.active = workspace;
        true
    }

    /// Window ids currently assigned to `workspace` (sorted for stable logs/tests).
    pub fn windows_on(&self, workspace: WorkspaceId) -> Vec<&str> {
        let mut out: Vec<&str> = self
            .windows
            .iter()
            .filter(|(_, ws)| **ws == workspace)
            .map(|(id, _)| id.as_str())
            .collect();
        out.sort_unstable();
        out
    }

    /// True when the window is tracked and lives on the active workspace.
    pub fn is_visible(&self, window_id: &str) -> bool {
        match self.windows.get(window_id) {
            Some(ws) => *ws == self.active && self.active.is_valid(),
            None => false,
        }
    }

    /// Workspace of a tracked window, if any.
    pub fn workspace_of(&self, window_id: &str) -> Option<WorkspaceId> {
        self.windows.get(window_id).copied()
    }

    /// Cycle active workspace forward (`0→1→…→7→0`).
    pub fn cycle_next(&mut self) {
        self.active = self.active.next_wrapping();
    }

    /// Cycle active workspace backward (`0→7→…→1→0`).
    pub fn cycle_prev(&mut self) {
        self.active = self.active.prev_wrapping();
    }

    /// Count of windows on each workspace (`len == WORKSPACE_COUNT`).
    pub fn counts_per_workspace(&self) -> [usize; WORKSPACE_COUNT as usize] {
        let mut counts = [0usize; WORKSPACE_COUNT as usize];
        for ws in self.windows.values() {
            if ws.is_valid() {
                counts[ws.as_usize()] += 1;
            }
        }
        counts
    }

    /// One-line label for compositor / session logs.
    pub fn summary_line(&self) -> String {
        let counts = self.counts_per_workspace();
        let visible = counts.get(self.active.as_usize()).copied().unwrap_or(0);
        let dist: String = counts
            .iter()
            .enumerate()
            .map(|(i, c)| format!("{i}:{c}"))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "workspace active={}/{} windows={} visible={} dist=[{}]",
            self.active.get(),
            WORKSPACE_COUNT,
            self.windows.len(),
            visible,
            dist
        )
    }

    /// Window ids that should be painted / presented on the active workspace
    /// (sorted). Used by the frame path to hide surfaces on other desktops.
    pub fn visible_window_ids(&self) -> Vec<&str> {
        self.windows_on(self.active)
    }

    /// Pure composition filter: keep only `candidate_ids` that are visible
    /// on the active workspace. Untracked ids are dropped (strict mode).
    pub fn filter_visible<'a>(&self, candidate_ids: &[&'a str]) -> Vec<&'a str> {
        let mut out: Vec<&str> = candidate_ids
            .iter()
            .copied()
            .filter(|id| self.is_visible(id))
            .collect();
        out.sort_unstable();
        out
    }

    /// Lenient filter: untracked ids pass through (shell-internal surfaces).
    pub fn filter_visible_or_untracked<'a>(&self, candidate_ids: &[&'a str]) -> Vec<&'a str> {
        candidate_ids
            .iter()
            .copied()
            .filter(|id| match self.windows.get(*id) {
                Some(ws) => *ws == self.active && self.active.is_valid(),
                None => true,
            })
            .collect()
    }

    /// Apply a workspace assignment from window rules (clamped id).
    pub fn apply_rule_workspace(
        &mut self,
        window_id: impl Into<String>,
        workspace_index: u8,
    ) -> bool {
        match WorkspaceId::new(workspace_index) {
            Some(ws) => self.assign_window(window_id, ws),
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn wayland_display_source_dispatches_client_requests_when_fd_is_ready() {
        use std::io::{Read, Write};
        use std::os::unix::net::UnixStream;
        use std::sync::Arc;
        use std::time::Duration;

        use smithay::reexports::calloop::EventLoop;
        use smithay::reexports::wayland_server::backend::{ClientData, ClientId, DisconnectReason};
        use smithay::reexports::wayland_server::Display;

        struct TestClientData;

        impl ClientData for TestClientData {
            fn initialized(&self, _client_id: ClientId) {}

            fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
        }

        let display = Display::<()>::new().expect("create Wayland display");
        let (server_stream, mut client_stream) =
            UnixStream::pair().expect("create test Wayland socket pair");
        client_stream
            .set_nonblocking(true)
            .expect("set client socket nonblocking");
        display
            .handle()
            .insert_client(server_stream, Arc::new(TestClientData))
            .expect("insert test Wayland client");

        let mut event_loop = EventLoop::<()>::try_new().expect("create calloop event loop");
        register_wayland_display_source(&event_loop.handle(), display)
            .expect("register Wayland display poll fd");

        // wl_display.sync(new_id=2), encoded as the real wire request.
        client_stream
            .write_all(&[1, 0, 0, 0, 0, 0, 12, 0, 2, 0, 0, 0])
            .expect("send wl_display.sync request");

        event_loop
            .dispatch(Some(Duration::from_millis(100)), &mut ())
            .expect("dispatch Wayland display source");

        let mut response = [0; 12];
        client_stream
            .read_exact(&mut response)
            .expect("read wl_callback.done response");
        assert_eq!(&response[0..4], &[2, 0, 0, 0]);
        assert_eq!(&response[4..8], &[0, 0, 12, 0]);
    }

    #[test]
    fn parse_outputs_spec_single_and_multi() {
        assert_eq!(
            parse_outputs_spec("1280x800"),
            vec![OutputConfig {
                width: 1280,
                height: 800
            }]
        );
        assert_eq!(
            parse_outputs_spec("1024x768,800x600"),
            vec![
                OutputConfig {
                    width: 1024,
                    height: 768
                },
                OutputConfig {
                    width: 800,
                    height: 600
                },
            ]
        );
        assert_eq!(
            parse_outputs_spec(" 640x480 , 320x240 "),
            vec![
                OutputConfig {
                    width: 640,
                    height: 480
                },
                OutputConfig {
                    width: 320,
                    height: 240
                },
            ]
        );
    }

    #[test]
    fn parse_outputs_spec_rejects_garbage() {
        assert!(parse_outputs_spec("").is_empty());
        assert!(parse_outputs_spec("nope").is_empty());
        assert!(parse_outputs_spec("0x0,-1x10,10x-1").is_empty());
        // partial: keep valid entries only
        assert_eq!(
            parse_outputs_spec("bad,800x600,also-bad"),
            vec![OutputConfig {
                width: 800,
                height: 600
            }]
        );
    }

    #[test]
    fn parse_outputs_layout_spec_dual_and_positions() {
        let spec = "eDP-1:1920x1080@0,0:s100;HDMI-1:2560x1440@1920,0:s100";
        let entries = parse_outputs_layout_spec(spec);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "eDP-1");
        assert_eq!(
            entries[0].config,
            OutputConfig {
                width: 1920,
                height: 1080
            }
        );
        assert_eq!((entries[0].x, entries[0].y), (0, 0));
        assert_eq!(entries[0].scale_percent, 100);
        assert_eq!(entries[1].name, "HDMI-1");
        assert_eq!(
            entries[1].config,
            OutputConfig {
                width: 2560,
                height: 1440
            }
        );
        assert_eq!((entries[1].x, entries[1].y), (1920, 0));
        assert_eq!(entries[1].scale_percent, 100);

        let laid = laid_out_from_layout_entries(&entries);
        assert_eq!(laid.len(), 2);
        assert_eq!(laid[1].x, 1920);
        assert_eq!(
            total_output_size(&laid),
            OutputConfig {
                width: 1920 + 2560,
                height: 1440
            }
        );

        let summary = outputs_layout_spec_summary(&entries);
        assert!(summary.contains("eDP-1"), "summary={summary}");
        assert!(summary.contains("HDMI-1"), "summary={summary}");
    }

    #[test]
    fn parse_outputs_layout_spec_stacked_negative_and_scale() {
        // Extended down + HiDPI scale on secondary.
        let spec = "eDP-1:1280x800@0,0:s100;DP-1:1920x1080@0,800:s200";
        let entries = parse_outputs_layout_spec(spec);
        assert_eq!(entries.len(), 2);
        assert_eq!((entries[1].x, entries[1].y), (0, 800));
        assert_eq!(entries[1].scale_percent, 200);

        // Negative origin (secondary to the left).
        let left = parse_outputs_layout_spec("HDMI-1:800x600@-800,0:s100;eDP-1:1920x1080@0,0:s100");
        assert_eq!(left.len(), 2);
        assert_eq!(left[0].x, -800);
        assert_eq!(left[0].name, "HDMI-1");
    }

    #[test]
    fn parse_outputs_layout_spec_skips_garbage_tolerates_missing_scale() {
        assert!(parse_outputs_layout_spec("").is_empty());
        assert!(parse_outputs_layout_spec("nope").is_empty());
        assert!(parse_outputs_layout_spec("bad;also-bad").is_empty());
        // Missing :sNN → default scale 100.
        let no_scale = parse_outputs_layout_spec("eDP-1:800x600@10,20");
        assert_eq!(no_scale.len(), 1);
        assert_eq!(no_scale[0].scale_percent, 100);
        assert_eq!((no_scale[0].x, no_scale[0].y), (10, 20));
        // Mixed: keep valid only.
        let mixed = parse_outputs_layout_spec("junk;eDP-1:640x480@0,0:s100;0x0@0,0:s100");
        assert_eq!(mixed.len(), 1);
        assert_eq!(mixed[0].name, "eDP-1");
        assert_eq!(mixed[0].config.width, 640);
    }

    #[test]
    fn resolve_laid_out_prefers_layout_spec_over_outputs() {
        let resolved = resolve_laid_out_outputs_from_env_values(
            Some("eDP-1:1920x1080@0,0:s100;HDMI-1:1280x720@1920,100:s150"),
            Some("800x600,640x480".into()), // would win only if layout missing
            Some("9999".into()),
            Some("9999".into()),
            OutputLayoutMode::SideBySide,
        );
        assert_eq!(resolved.source, OutputsLayoutSource::LayoutSpec);
        assert_eq!(resolved.laid_out.len(), 2);
        assert_eq!(
            resolved.names,
            vec!["eDP-1".to_string(), "HDMI-1".to_string()]
        );
        assert_eq!(
            (resolved.laid_out[1].x, resolved.laid_out[1].y),
            (1920, 100)
        );
        assert_eq!(resolved.laid_out[1].config.width, 1280);
        let s = resolved.summary();
        assert!(s.contains("SLOPOS_OUTPUTS_LAYOUT"), "summary={s}");
        assert!(s.contains("eDP-1"), "summary={s}");
    }

    #[test]
    fn resolve_laid_out_falls_back_to_outputs_spec_and_default() {
        let multi = resolve_laid_out_outputs_from_env_values(
            Some("garbage"),
            Some("100x50,200x80".into()),
            None,
            None,
            OutputLayoutMode::Stacked,
        );
        assert_eq!(multi.source, OutputsLayoutSource::OutputsSpec);
        assert_eq!(multi.laid_out.len(), 2);
        assert_eq!(multi.laid_out[1].y, 50); // stacked
        assert_eq!(multi.names[0], "X11-1");

        let def = resolve_laid_out_outputs_from_env_values(
            None,
            None,
            None,
            None,
            OutputLayoutMode::SideBySide,
        );
        assert_eq!(def.source, OutputsLayoutSource::Default);
        assert_eq!(def.laid_out.len(), 1);
        assert_eq!(def.laid_out[0].config, OutputConfig::default());
    }

    #[test]
    fn layout_side_by_side_and_total_size() {
        let outs = parse_outputs_spec("100x50,200x80");
        let laid = layout_outputs_side_by_side(&outs);
        assert_eq!(laid.len(), 2);
        assert_eq!(laid[0].x, 0);
        assert_eq!(laid[1].x, 100);
        assert_eq!(
            total_output_size(&laid),
            OutputConfig {
                width: 300,
                height: 80
            }
        );
    }

    #[test]
    fn parse_layout_mode_side_stack_grid_and_default() {
        assert_eq!(parse_layout_mode(None), OutputLayoutMode::SideBySide);
        assert_eq!(parse_layout_mode(Some("")), OutputLayoutMode::SideBySide);
        assert_eq!(
            parse_layout_mode(Some("nope")),
            OutputLayoutMode::SideBySide
        );
        assert_eq!(
            parse_layout_mode(Some("side")),
            OutputLayoutMode::SideBySide
        );
        assert_eq!(
            parse_layout_mode(Some("SIDE")),
            OutputLayoutMode::SideBySide
        );
        assert_eq!(parse_layout_mode(Some("stack")), OutputLayoutMode::Stacked);
        assert_eq!(
            parse_layout_mode(Some("stacked")),
            OutputLayoutMode::Stacked
        );
        assert_eq!(parse_layout_mode(Some("grid")), OutputLayoutMode::Grid);
        assert_eq!(OutputLayoutMode::default(), OutputLayoutMode::SideBySide);
    }

    #[test]
    fn layout_outputs_dispatches_to_mode() {
        let outs = parse_outputs_spec("100x50,200x80");
        assert_eq!(
            layout_outputs(&outs, OutputLayoutMode::SideBySide),
            layout_outputs_side_by_side(&outs)
        );
        assert_eq!(
            layout_outputs(&outs, OutputLayoutMode::Stacked),
            layout_outputs_stacked(&outs)
        );
        assert_eq!(
            layout_outputs(&outs, OutputLayoutMode::Grid),
            layout_outputs_grid(&outs)
        );
    }

    #[test]
    fn layout_stacked_origins_and_sizes() {
        let outs = parse_outputs_spec("100x50,200x80");
        let laid = layout_outputs_stacked(&outs);
        assert_eq!(laid.len(), 2);
        assert_eq!(laid[0].x, 0);
        assert_eq!(laid[0].y, 0);
        assert_eq!(laid[0].config, outs[0]);
        assert_eq!(laid[1].x, 0);
        assert_eq!(laid[1].y, 50);
        assert_eq!(laid[1].config, outs[1]);
        assert_eq!(
            total_output_size(&laid),
            OutputConfig {
                width: 200,
                height: 130
            }
        );

        // Three outputs stack strictly by height.
        let three = parse_outputs_spec("10x20,30x40,50x60");
        let laid3 = layout_outputs(&three, OutputLayoutMode::Stacked);
        assert_eq!(laid3[0].y, 0);
        assert_eq!(laid3[1].y, 20);
        assert_eq!(laid3[2].y, 60);
        assert_eq!(
            total_output_size(&laid3),
            OutputConfig {
                width: 50,
                height: 120
            }
        );
    }

    #[test]
    fn layout_grid_origins_and_sizes() {
        // Two outputs → single row: left at (0,0), right at (left.w, 0).
        let two = parse_outputs_spec("100x50,200x80");
        let laid2 = layout_outputs_grid(&two);
        assert_eq!(laid2.len(), 2);
        assert_eq!(laid2[0].x, 0);
        assert_eq!(laid2[0].y, 0);
        assert_eq!(laid2[0].config.width, 100);
        assert_eq!(laid2[0].config.height, 50);
        assert_eq!(laid2[1].x, 100);
        assert_eq!(laid2[1].y, 0);
        assert_eq!(laid2[1].config.width, 200);
        assert_eq!(laid2[1].config.height, 80);
        assert_eq!(
            total_output_size(&laid2),
            OutputConfig {
                width: 300,
                height: 80
            }
        );

        // Three outputs → row0 pair + trailing single on next row.
        // Row height = max(50, 80) = 80; third at (0, 80).
        let three = parse_outputs_spec("100x50,200x80,120x40");
        let laid3 = layout_outputs_grid(&three);
        assert_eq!(laid3.len(), 3);
        assert_eq!((laid3[0].x, laid3[0].y), (0, 0));
        assert_eq!((laid3[1].x, laid3[1].y), (100, 0));
        assert_eq!((laid3[2].x, laid3[2].y), (0, 80));
        assert_eq!(laid3[2].config, three[2]);
        assert_eq!(
            total_output_size(&laid3),
            OutputConfig {
                width: 300,
                height: 120
            }
        );

        // Four outputs → two full rows.
        let four = parse_outputs_spec("10x20,30x40,50x60,70x80");
        let laid4 = layout_outputs(&four, OutputLayoutMode::Grid);
        assert_eq!((laid4[0].x, laid4[0].y), (0, 0));
        assert_eq!((laid4[1].x, laid4[1].y), (10, 0));
        // row_h0 = max(20,40)=40
        assert_eq!((laid4[2].x, laid4[2].y), (0, 40));
        assert_eq!((laid4[3].x, laid4[3].y), (50, 40));
        // total w = max(10+30, 50+70)=120; h = 40+max(60,80)=120
        assert_eq!(
            total_output_size(&laid4),
            OutputConfig {
                width: 120,
                height: 120
            }
        );
    }

    #[test]
    fn outputs_from_env_values_prefers_outputs_spec() {
        let multi = outputs_from_env_values(
            Some("800x600,640x480".into()),
            Some("9999".into()),
            Some("9999".into()),
        );
        assert_eq!(multi.len(), 2);
        assert_eq!(multi[0].width, 800);

        let single = outputs_from_env_values(None, Some("1280".into()), Some("720".into()));
        assert_eq!(
            single,
            vec![OutputConfig {
                width: 1280,
                height: 720
            }]
        );

        let fallback = outputs_from_env_values(Some("garbage".into()), None, None);
        assert_eq!(fallback, vec![OutputConfig::default()]);
    }

    #[test]
    fn client_window_stack_map_focus_z_order() {
        let mut stack = ClientWindowStack::new();
        // Non-overlapping geometries so click-to-raise is unambiguous.
        let a = stack.map_window_at("Finder", 101, WindowGeometry::new(0, 0, 100, 100));
        let b = stack.map_window_at("Terminal", 102, WindowGeometry::new(200, 0, 100, 100));
        assert_eq!(stack.len(), 2);
        assert_eq!(stack.focused().map(|w| w.id.clone()), Some(b.clone()));
        assert_eq!(stack.z_order_ids(), vec![a.clone(), b.clone()]);

        assert!(stack.focus(&a));
        assert_eq!(stack.focused().map(|w| w.id.clone()), Some(a.clone()));
        assert_eq!(stack.z_order_ids(), vec![b.clone(), a.clone()]);

        let hit = stack.focus_at(210.0, 10.0).expect("hit terminal");
        assert_eq!(hit, b);
        assert_eq!(stack.focused().map(|w| w.title.as_str()), Some("Terminal"));

        assert!(stack.unmap(&a));
        assert_eq!(stack.len(), 1);
        assert!(!stack.unmap(&a));
    }

    #[test]
    fn client_window_stack_independent_of_shell_paint_rects() {
        // Two clients → two mapped surfaces in compositor stack (multi-client model).
        let mut stack = ClientWindowStack::new();
        stack.map_window("settings", 1);
        stack.map_window("textedit", 2);
        assert_eq!(stack.windows().len(), 2);
        assert_ne!(stack.windows()[0].pid, 0);
        assert_ne!(stack.windows()[0].id, stack.windows()[1].id);
    }

    #[test]
    fn display_policy_settings_and_env() {
        let mut p = DisplayPolicy::default();
        p.apply_settings_text(
            "hdr_requested=true\nvrr_adaptive=true\nrefresh_rate=120hz\ncolor_space=rec2020\n",
        );
        assert!(p.hdr_requested);
        assert!(p.vrr_adaptive);
        assert_eq!(p.refresh_rate, RefreshRate::Hz120);
        assert_eq!(p.color_space, ColorSpace::Rec2020);
        assert_eq!(p.effective_refresh_rate(), RefreshRate::Adaptive);

        let mut env = HashMap::new();
        env.insert("SLOPOS_HDR".into(), "0".into());
        env.insert("SLOPOS_VRR".into(), "false".into());
        env.insert("SLOPOS_REFRESH".into(), "60".into());
        env.insert("SLOPOS_COLOR_SPACE".into(), "srgb".into());
        p.apply_env_map(env);
        assert!(!p.hdr_requested);
        assert!(!p.vrr_adaptive);
        assert_eq!(p.refresh_rate, RefreshRate::Hz60);
        assert_eq!(p.color_space, ColorSpace::SRgb);
        assert_eq!(p.effective_refresh_rate(), RefreshRate::Hz60);
    }

    #[test]
    fn display_policy_accepts_hdr_request_alias() {
        let mut p = DisplayPolicy::default();
        p.apply_settings_text("hdr_request=true\n");
        assert!(p.hdr_requested);
    }

    #[test]
    fn selection_mime_lookup_and_fallback() {
        let mut store = HashMap::new();
        store.insert("text/plain".into(), b"hello".to_vec());
        assert_eq!(
            selection_bytes_for_mime(&store, "text/plain"),
            Some(b"hello".as_slice())
        );
        assert_eq!(selection_bytes_for_mime(&store, "image/png"), None);
        assert_eq!(
            selection_bytes_for_mime_with_text_fallback(&store, "text/plain;charset=utf-8"),
            Some(b"hello".as_slice())
        );
        assert_eq!(
            selection_bytes_for_mime_with_text_fallback(&store, "image/png"),
            None
        );
    }

    #[test]
    fn selection_store_from_utf8_offers_and_satisfies() {
        let store = selection_store_from_utf8_text("hi <b>");
        assert!(selection_can_satisfy(&store, "text/plain"));
        assert!(selection_can_satisfy(&store, "UTF8_STRING"));
        assert!(selection_can_satisfy(&store, "text/html"));
        let html = selection_bytes_for_mime(&store, "text/html").unwrap();
        assert!(std::str::from_utf8(html).unwrap().contains("&lt;b&gt;"));
        assert!(!selection_can_satisfy(&store, "image/png"));
    }

    #[test]
    fn select_backend_kind_headless_wins() {
        assert_eq!(
            select_backend_kind(true, true, true),
            CompositorBackendKind::Headless
        );
        assert_eq!(
            select_backend_kind(false, false, true),
            CompositorBackendKind::Headless
        );
        assert_eq!(
            select_backend_kind(true, false, true),
            CompositorBackendKind::Headless
        );
    }

    #[test]
    fn select_backend_kind_session_drm_when_prefer_and_dri3() {
        assert_eq!(
            select_backend_kind(true, true, false),
            CompositorBackendKind::SessionDrm
        );
    }

    #[test]
    fn select_backend_kind_nested_x11_otherwise() {
        // prefer_drm but no DRI3 → nested (honest default; may fail GL later)
        assert_eq!(
            select_backend_kind(true, false, false),
            CompositorBackendKind::NestedX11
        );
        // no prefer_drm even with DRI3 → nested
        assert_eq!(
            select_backend_kind(false, true, false),
            CompositorBackendKind::NestedX11
        );
        assert_eq!(
            select_backend_kind(false, false, false),
            CompositorBackendKind::NestedX11
        );
    }

    #[test]
    fn detect_dri3_from_env_value_parses_0_1() {
        assert_eq!(detect_dri3_from_env_value(Some("1")), Some(true));
        assert_eq!(detect_dri3_from_env_value(Some("0")), Some(false));
        assert_eq!(detect_dri3_from_env_value(Some("true")), Some(true));
        assert_eq!(detect_dri3_from_env_value(Some("false")), Some(false));
        assert_eq!(detect_dri3_from_env_value(None), None);
        assert_eq!(detect_dri3_from_env_value(Some("")), None);
        assert_eq!(detect_dri3_from_env_value(Some("maybe")), None);
    }

    #[test]
    fn session_mode_summary_is_honest() {
        let nested = session_mode_summary(CompositorBackendKind::NestedX11);
        assert!(nested.contains("nested_x11"));
        assert!(!nested.contains("session_drm"));

        let drm = session_mode_summary(CompositorBackendKind::SessionDrm);
        assert!(drm.contains("session_drm"));
        assert!(drm.contains("DRM"));

        let headless = session_mode_summary(CompositorBackendKind::Headless);
        assert!(headless.contains("headless"));
        assert!(headless.contains("no host display"));
    }

    #[test]
    fn parse_output_scale_integer_fraction_decimal() {
        assert_eq!(
            parse_output_scale("2"),
            Some(OutputScale {
                numerator: 2,
                denominator: 1
            })
        );
        assert_eq!(
            parse_output_scale("1"),
            Some(OutputScale {
                numerator: 1,
                denominator: 1
            })
        );
        assert_eq!(
            parse_output_scale("3/2"),
            Some(OutputScale {
                numerator: 3,
                denominator: 2
            })
        );
        assert_eq!(
            parse_output_scale(" 4 / 2 "),
            Some(OutputScale {
                numerator: 2,
                denominator: 1
            })
        );
        assert_eq!(
            parse_output_scale("1.5"),
            Some(OutputScale {
                numerator: 3,
                denominator: 2
            })
        );
        assert_eq!(
            parse_output_scale("1.25"),
            Some(OutputScale {
                numerator: 5,
                denominator: 4
            })
        );
        assert_eq!(
            parse_output_scale("2.0"),
            Some(OutputScale {
                numerator: 2,
                denominator: 1
            })
        );
    }

    #[test]
    fn parse_output_scale_rejects_invalid() {
        assert_eq!(parse_output_scale(""), None);
        assert_eq!(parse_output_scale("   "), None);
        assert_eq!(parse_output_scale("0"), None);
        assert_eq!(parse_output_scale("0/1"), None);
        assert_eq!(parse_output_scale("1/0"), None);
        assert_eq!(parse_output_scale("-1"), None);
        assert_eq!(parse_output_scale("nope"), None);
        assert_eq!(parse_output_scale("1.5.0"), None);
        // Integer path allows any positive u32
        assert_eq!(
            parse_output_scale("8"),
            Some(OutputScale {
                numerator: 8,
                denominator: 1
            })
        );
        // Decimal above 64 rejected; bare integer "100" still accepted
        assert_eq!(parse_output_scale("65.0"), None);
        assert_eq!(
            parse_output_scale("100"),
            Some(OutputScale {
                numerator: 100,
                denominator: 1
            })
        );
    }

    #[test]
    fn output_scale_as_f64_and_from_env_value() {
        let s = OutputScale::new(3, 2).unwrap();
        assert!((s.as_f64() - 1.5).abs() < 1e-9);
        assert!(OutputScale::IDENTITY.is_identity());
        assert!(!s.is_identity());

        assert_eq!(
            OutputScale::from_env_value(Some("2")),
            Some(OutputScale {
                numerator: 2,
                denominator: 1
            })
        );
        assert_eq!(
            OutputScale::from_env_value(Some("1.5")),
            Some(OutputScale {
                numerator: 3,
                denominator: 2
            })
        );
        assert_eq!(
            OutputScale::from_env_value(Some("3/2")),
            Some(OutputScale {
                numerator: 3,
                denominator: 2
            })
        );
        assert_eq!(OutputScale::from_env_value(None), None);
        assert_eq!(OutputScale::from_env_value(Some("")), None);
        assert_eq!(OutputScale::from_env_value(Some("  ")), None);
        assert_eq!(OutputScale::from_env_value(Some("bogus")), None);
    }

    #[test]
    fn scale_logical_to_physical_and_back() {
        let two = OutputScale::new(2, 1).unwrap();
        assert_eq!(scale_logical_to_physical((100, 50), two), (200, 100));
        assert_eq!(scale_physical_to_logical((200, 100), two), (100, 50));

        let half_extra = OutputScale::new(3, 2).unwrap(); // 1.5×
        assert_eq!(scale_logical_to_physical((100, 50), half_extra), (150, 75));
        assert_eq!(scale_physical_to_logical((150, 75), half_extra), (100, 50));

        // Ceil on odd logical under 1.5×: ceil(101 * 3 / 2) = ceil(151.5) = 152
        assert_eq!(scale_logical_to_physical((101, 1), half_extra), (152, 2));

        let id = OutputScale::IDENTITY;
        assert_eq!(scale_logical_to_physical((1024, 768), id), (1024, 768));
        assert_eq!(scale_physical_to_logical((1024, 768), id), (1024, 768));
    }

    #[test]
    fn apply_scale_to_output_config_physical_size() {
        let cfg = OutputConfig {
            width: 1024,
            height: 768,
        };
        assert_eq!(
            apply_scale_to_output_config(cfg, OutputScale::IDENTITY),
            cfg
        );
        assert_eq!(
            apply_scale_to_output_config(cfg, OutputScale::new(2, 1).unwrap()),
            OutputConfig {
                width: 2048,
                height: 1536
            }
        );
        assert_eq!(
            apply_scale_to_output_config(cfg, OutputScale::new(3, 2).unwrap()),
            OutputConfig {
                width: 1536,
                height: 1152
            }
        );
        // Pure: original cfg unchanged semantics (Copy; re-check identity)
        assert_eq!(cfg.width, 1024);
    }

    #[test]
    fn output_scale_summary_and_session_mode_note() {
        let scale = OutputScale::new(3, 2).unwrap();
        let sum = output_scale_summary(scale);
        assert!(sum.contains("output_scale=3/2"));
        assert!(sum.contains("1.50") || sum.contains("1.5"));

        let note = session_mode_note(CompositorBackendKind::NestedX11, scale);
        assert!(note.contains("nested_x11"));
        assert!(note.contains("output_scale=3/2"));
        assert!(note.contains("session_mode="));

        let drm_note = session_mode_note(
            CompositorBackendKind::SessionDrm,
            OutputScale::new(2, 1).unwrap(),
        );
        assert!(drm_note.contains("session_drm"));
        assert!(drm_note.contains("2/1"));
    }

    #[test]
    fn scale_zero_and_negative_dims_do_not_inflate() {
        let two = OutputScale::new(2, 1).unwrap();
        assert_eq!(scale_logical_to_physical((0, 0), two), (0, 0));
        assert_eq!(scale_logical_to_physical((-4, 10), two), (-4, 20));
    }

    #[test]
    fn discover_drm_nodes_from_names_orders_primary_first() {
        let names = vec![
            "renderD128".into(),
            "card1".into(),
            "card0".into(),
            "controlD64".into(),
        ];
        let nodes = discover_drm_nodes_from_names(Path::new("/dev/dri"), &names);
        assert_eq!(nodes.len(), 3); // controlD ignored
        assert!(nodes[0].is_primary);
        assert!(nodes[0].path.ends_with("card0") || nodes[0].path.ends_with("card1"));
        assert!(nodes.iter().any(|n| n.path.ends_with("renderD128")));
        assert_eq!(
            preferred_primary_drm_node(&nodes).map(|n| n.is_primary),
            Some(true)
        );
    }

    #[test]
    fn plan_compose_order_puts_overlay_after_under() {
        let z = vec![
            ChromeLayer::Overlay.z_priority(),
            ChromeLayer::Background.z_priority(),
            ChromeLayer::Top.z_priority(),
            ChromeLayer::Bottom.z_priority(),
        ];
        let order = plan_compose_order(&z);
        // Under: Background(1), Bottom(3) then Over indices included in full list
        assert!(order.windows_after_bottom);
        // First under layers should be background then bottom (indices 1 then 3)
        assert_eq!(order.layer_indices_bottom_first[0], 1);
        assert_eq!(order.layer_indices_bottom_first[1], 3);
        // Then top then overlay
        assert_eq!(order.layer_indices_bottom_first[2], 2);
        assert_eq!(order.layer_indices_bottom_first[3], 0);
    }

    #[test]
    fn drm_presentation_pipeline_includes_scanout_stages() {
        let p = drm_presentation_pipeline();
        assert!(p.contains(&DrmPresentationStage::EnumerateConnectors));
        assert!(p.contains(&DrmPresentationStage::CreateDrmSurface));
        assert!(p.contains(&DrmPresentationStage::PageFlipOrPresent));
        assert_eq!(p.first(), Some(&DrmPresentationStage::OpenSeat));
        assert_eq!(p.last(), Some(&DrmPresentationStage::ProtocolLoop));
    }

    #[test]
    fn plan_drm_modeset_prefers_connected() {
        let connectors = vec![
            ("HDMI-A-1".into(), false, Some((1920, 1080, 60_000))),
            ("eDP-1".into(), true, Some((2560, 1600, 60_000))),
        ];
        let plan = plan_drm_modeset(&connectors, 1024, 768, 60_000);
        assert_eq!(plan.connector_name, "eDP-1");
        assert_eq!(plan.mode_w, 2560);
        assert_eq!(plan.mode_h, 1600);
    }

    #[test]
    fn plan_drm_modeset_fallback_when_none() {
        let plan = plan_drm_modeset(&[], 800, 600, 60_000);
        assert_eq!(plan.connector_name, "virtual-fallback");
        assert_eq!(plan.mode_w, 800);
    }

    #[test]
    fn decoration_preference_first_party_csd_external_ssd() {
        assert_eq!(
            decoration_preference_for_app_id("slopos-i.finder"),
            DecorationPreference::ClientSide
        );
        assert_eq!(
            decoration_preference_for_app_id("firefox"),
            DecorationPreference::ServerSide
        );
        assert_eq!(
            decoration_preference_for_app_id("org.gnome.Nautilus"),
            DecorationPreference::ServerSide
        );
    }

    #[test]
    fn chrome_layer_specs_sort_and_parse() {
        assert_eq!(ChromeLayer::from_str_loose("TOP"), Some(ChromeLayer::Top));
        let mut specs = vec![
            LayerChromeSpec::notification_overlay(),
            LayerChromeSpec::dock(48),
            LayerChromeSpec::menu_bar(28),
        ];
        sort_chrome_layers(&mut specs);
        assert_eq!(specs[0].name, "dock");
        assert_eq!(specs[1].name, "menu-bar");
        assert_eq!(specs[2].name, "notifications");
        assert!(specs[2].layer.z_priority() > specs[0].layer.z_priority());
    }

    #[test]
    fn text_input_capability_env_parses() {
        assert_eq!(
            text_input_capability_from_env(None),
            TextInputCapability::None
        );
        assert_eq!(
            text_input_capability_from_env(Some("v3")),
            TextInputCapability::TextInputV3
        );
        assert_eq!(
            text_input_capability_from_env(Some("full")),
            TextInputCapability::InputMethodAndTextInput
        );
        assert!(text_input_capability_summary(TextInputCapability::TextInputV3).contains("v3"));
    }

    #[test]
    fn accumulate_damage_and_full_redraw_heuristic() {
        let els = vec![
            ScanoutElement {
                id: "a".into(),
                x: 0,
                y: 0,
                w: 100,
                h: 100,
                z: 0,
            },
            ScanoutElement {
                id: "b".into(),
                x: 200,
                y: 200,
                w: 50,
                h: 50,
                z: 1,
            },
        ];
        let d = accumulate_damage(&els, &["a", "b"]).unwrap();
        assert_eq!(d, DamageRect::from_xywh(0, 0, 250, 250).unwrap());
        assert!(prefer_full_redraw(d, 300, 300));
        assert!(!prefer_full_redraw(
            DamageRect::from_xywh(0, 0, 10, 10).unwrap(),
            1000,
            1000
        ));
    }

    #[test]
    fn damage_region_and_window_move_accumulate() {
        assert_eq!(
            damage_region(10, 20, 100, 50),
            DamageRect::from_xywh(10, 20, 100, 50)
        );
        assert!(damage_region(0, 0, 0, 10).is_none());
        let old = WindowGeometry::new(0, 0, 100, 100);
        let new = WindowGeometry::new(50, 50, 100, 100);
        let moved = damage_region_for_geometry_change(old, new).unwrap();
        assert_eq!(moved, DamageRect::from_xywh(0, 0, 150, 150).unwrap());
        let via_scanout = accumulate_damage_for_window_move("win", old, new).unwrap();
        assert_eq!(via_scanout, moved);
        let acc = accumulate_damage_rect(None, damage_region(0, 0, 10, 10).unwrap());
        let acc = accumulate_damage_rect(Some(acc), damage_region(20, 20, 10, 10).unwrap());
        assert_eq!(acc, DamageRect::from_xywh(0, 0, 30, 30).unwrap());
    }

    #[test]
    fn placeholder_present_stats_logs_once() {
        let mut stats = PlaceholderPresentStats::new();
        assert!(stats.note_frame_with_placeholders());
        assert_eq!(stats.frames_with_placeholders, 1);
        assert!(!stats.note_frame_with_placeholders());
        assert_eq!(stats.frames_with_placeholders, 2);
        assert!(stats.logged_once);
    }

    // -----------------------------------------------------------------------
    // Virtual workspaces
    // -----------------------------------------------------------------------

    #[test]
    fn workspace_id_new_validates_range() {
        assert_eq!(WorkspaceId::new(0), Some(WorkspaceId(0)));
        assert_eq!(WorkspaceId::new(7), Some(WorkspaceId(7)));
        assert_eq!(WorkspaceId::new(8), None);
        assert_eq!(WorkspaceId::new(255), None);
        assert!(WorkspaceId::FIRST.is_valid());
        assert!(WorkspaceId::LAST.is_valid());
        assert_eq!(WorkspaceId::LAST.get(), WORKSPACE_COUNT - 1);
        assert_eq!(WorkspaceId::all().count(), usize::from(WORKSPACE_COUNT));
        assert_eq!(
            WorkspaceId::all().map(|w| w.get()).collect::<Vec<_>>(),
            (0..WORKSPACE_COUNT).collect::<Vec<_>>()
        );
    }

    #[test]
    fn workspace_id_cycle_wrapping() {
        assert_eq!(WorkspaceId(0).next_wrapping(), WorkspaceId(1));
        assert_eq!(WorkspaceId(6).next_wrapping(), WorkspaceId(7));
        assert_eq!(WorkspaceId(7).next_wrapping(), WorkspaceId(0));
        assert_eq!(WorkspaceId(0).prev_wrapping(), WorkspaceId(7));
        assert_eq!(WorkspaceId(1).prev_wrapping(), WorkspaceId(0));
        assert_eq!(WorkspaceId(7).prev_wrapping(), WorkspaceId(6));
        // Invalid raw ids normalize rather than panic
        assert_eq!(WorkspaceId(9).next_wrapping(), WorkspaceId::FIRST);
        assert_eq!(WorkspaceId(9).prev_wrapping(), WorkspaceId::LAST);
    }

    #[test]
    fn workspace_assign_move_and_visibility() {
        let mut st = WorkspaceState::new();
        assert_eq!(st.active, WorkspaceId::FIRST);
        assert!(st.windows.is_empty());

        assert!(st.assign_window("finder", WorkspaceId(0)));
        assert!(st.assign_window("term", WorkspaceId(2)));
        assert!(st.assign_window("edit", WorkspaceId(2)));
        // invalid workspace rejected
        assert!(!st.assign_window("ghost", WorkspaceId(8)));
        assert!(!st.windows.contains_key("ghost"));

        assert!(st.is_visible("finder"));
        assert!(!st.is_visible("term"));
        assert!(!st.is_visible("missing"));

        assert_eq!(st.windows_on(WorkspaceId(2)), vec!["edit", "term"]);
        assert_eq!(st.windows_on(WorkspaceId(0)), vec!["finder"]);
        assert!(st.windows_on(WorkspaceId(1)).is_empty());

        assert!(st.move_to_workspace("term", WorkspaceId(0)));
        assert!(st.is_visible("term"));
        assert_eq!(st.workspace_of("term"), Some(WorkspaceId(0)));
        assert!(!st.move_to_workspace("nope", WorkspaceId(1)));
        assert!(!st.move_to_workspace("term", WorkspaceId(99)));

        assert_eq!(st.remove_window("edit"), Some(WorkspaceId(2)));
        assert_eq!(st.remove_window("edit"), None);
    }

    #[test]
    fn workspace_activate_and_cycle() {
        let mut st = WorkspaceState::new();
        assert!(st.activate(WorkspaceId(3)));
        assert_eq!(st.active, WorkspaceId(3));
        assert!(!st.activate(WorkspaceId(8)));
        assert_eq!(st.active, WorkspaceId(3));

        st.cycle_next();
        assert_eq!(st.active, WorkspaceId(4));
        assert!(st.activate(WorkspaceId(7)));
        st.cycle_next();
        assert_eq!(st.active, WorkspaceId(0));
        st.cycle_prev();
        assert_eq!(st.active, WorkspaceId(7));

        // Full wrap tour: 8 next/prev steps from 0 returns to 0
        assert!(st.activate(WorkspaceId(0)));
        for _ in 0..WORKSPACE_COUNT {
            st.cycle_next();
        }
        assert_eq!(st.active, WorkspaceId(0));
        for _ in 0..WORKSPACE_COUNT {
            st.cycle_prev();
        }
        assert_eq!(st.active, WorkspaceId(0));
    }

    #[test]
    fn workspace_summary_line_and_counts() {
        let mut st = WorkspaceState::new();
        st.assign_window("a", WorkspaceId(0));
        st.assign_window("b", WorkspaceId(0));
        st.assign_window("c", WorkspaceId(3));
        assert!(st.activate(WorkspaceId(0)));
        let line = st.summary_line();
        assert!(line.contains("active=0/8"), "line={line}");
        assert!(line.contains("windows=3"), "line={line}");
        assert!(line.contains("visible=2"), "line={line}");
        assert!(line.contains("0:2"), "line={line}");
        assert!(line.contains("3:1"), "line={line}");

        let counts = st.counts_per_workspace();
        assert_eq!(counts[0], 2);
        assert_eq!(counts[3], 1);
        assert_eq!(counts.iter().sum::<usize>(), 3);

        // Reassign updates counts; visibility follows active
        assert!(st.assign_window("a", WorkspaceId(3)));
        assert!(!st.is_visible("a"));
        assert!(st.activate(WorkspaceId(3)));
        assert!(st.is_visible("a"));
        assert!(st.is_visible("c"));
        assert!(!st.is_visible("b"));
    }

    #[test]
    fn workspace_state_default_and_display() {
        let st = WorkspaceState::default();
        assert_eq!(st, WorkspaceState::new());
        assert_eq!(format!("{}", WorkspaceId(5)), "5");
        // assign overwrites previous workspace
        let mut st = WorkspaceState::new();
        assert!(st.assign_window("w", WorkspaceId(1)));
        assert!(st.assign_window("w", WorkspaceId(4)));
        assert_eq!(st.workspace_of("w"), Some(WorkspaceId(4)));
        assert_eq!(st.windows.len(), 1);
    }

    #[test]
    fn workspace_composition_filter() {
        let mut st = WorkspaceState::new();
        assert!(st.assign_window("a", WorkspaceId(0)));
        assert!(st.assign_window("b", WorkspaceId(1)));
        assert!(st.apply_rule_workspace("c", 0));
        assert_eq!(st.visible_window_ids(), vec!["a", "c"]);
        let filtered = st.filter_visible(&["a", "b", "c", "ghost"]);
        assert_eq!(filtered, vec!["a", "c"]);
        let lenient = st.filter_visible_or_untracked(&["a", "b", "ghost"]);
        assert!(lenient.contains(&"a"));
        assert!(lenient.contains(&"ghost"));
        assert!(!lenient.contains(&"b"));
        st.cycle_next();
        assert!(st.is_visible("b"));
        assert!(!st.is_visible("a"));
    }

    #[test]
    fn plan_scanout_paint_order_and_clip() {
        let mut els = vec![
            ScanoutElement {
                id: "top".into(),
                x: 10,
                y: 10,
                w: 100,
                h: 100,
                z: 2,
            },
            ScanoutElement {
                id: "bot".into(),
                x: 0,
                y: 0,
                w: 50,
                h: 50,
                z: 0,
            },
        ];
        plan_scanout_paint_order(&mut els);
        assert_eq!(els[0].id, "bot");
        assert_eq!(els[1].id, "top");
        assert_eq!(
            clip_scanout_element_to_output(&els[1], 80, 80),
            Some((10, 10, 70, 70))
        );
        assert_eq!(
            clip_scanout_element_to_output(
                &ScanoutElement {
                    id: "out".into(),
                    x: 100,
                    y: 100,
                    w: 10,
                    h: 10,
                    z: 0
                },
                50,
                50
            ),
            None
        );
    }

    #[test]
    fn interactive_move_clamps_to_output() {
        let grab = InteractiveGrab::moving("finder", 10, 10, WindowGeometry::new(50, 40, 300, 200));
        assert_eq!(
            geometry_for_interactive_grab(&grab, 210, 110, 120, 80, 1024, 768),
            WindowGeometry::new(250, 140, 300, 200)
        );
        assert_eq!(
            geometry_for_interactive_grab(&grab, -500, -500, 120, 80, 1024, 768),
            WindowGeometry::new(0, 0, 300, 200)
        );
    }

    #[test]
    fn interactive_move_uses_the_pointer_delta() {
        let grab =
            InteractiveGrab::moving("finder", 200, 150, WindowGeometry::new(80, 60, 320, 240));
        assert_eq!(
            geometry_for_interactive_grab(&grab, 245, 115, 160, 96, 1024, 768),
            WindowGeometry::new(125, 25, 320, 240)
        );
    }

    #[test]
    fn normal_window_is_clamped_to_scaled_work_area() {
        let desired = WindowGeometry::new(64, 64, DEFAULT_WINDOW_W, DEFAULT_WINDOW_H);
        let work_area = WindowGeometry::new(0, 19, 640, 317);
        assert_eq!(
            clamp_window_to_work_area(desired, work_area),
            WindowGeometry::new(0, 19, 640, 317)
        );
    }

    #[test]
    fn normal_window_keeps_cascade_geometry_when_it_fits() {
        let desired = WindowGeometry::new(64, 64, DEFAULT_WINDOW_W, DEFAULT_WINDOW_H);
        let work_area = WindowGeometry::new(0, 19, 1280, 712);
        assert_eq!(clamp_window_to_work_area(desired, work_area), desired);
    }

    #[test]
    fn normal_window_respects_minimums_in_tiny_work_area() {
        let work_area = WindowGeometry::new(0, 0, 80, 60);
        assert_eq!(
            clamp_window_to_work_area(WindowGeometry::new(20, 20, 1, 1), work_area),
            work_area
        );
    }

    #[test]
    fn interactive_resize_honours_edges_and_minimum() {
        let grab = InteractiveGrab::resizing(
            "textedit",
            ResizeEdges::BOTTOM_RIGHT,
            100,
            100,
            WindowGeometry::new(50, 40, 300, 200),
        )
        .unwrap();
        assert_eq!(
            geometry_for_interactive_grab(&grab, 250, 180, 160, 120, 1024, 768),
            WindowGeometry::new(50, 40, 450, 280)
        );

        let left = InteractiveGrab::resizing(
            "textedit",
            ResizeEdges::LEFT,
            100,
            100,
            WindowGeometry::new(50, 40, 300, 200),
        )
        .unwrap();
        assert_eq!(
            geometry_for_interactive_grab(&left, 500, 100, 160, 120, 1024, 768),
            WindowGeometry::new(190, 40, 160, 200)
        );
    }

    #[test]
    fn interactive_resize_honours_each_edge_and_corner() {
        let start = WindowGeometry::new(100, 100, 300, 200);
        let cases = [
            (
                ResizeEdges::TOP,
                (0, 50),
                WindowGeometry::new(100, 150, 300, 150),
            ),
            (
                ResizeEdges::BOTTOM,
                (0, 50),
                WindowGeometry::new(100, 100, 300, 250),
            ),
            (
                ResizeEdges::LEFT,
                (50, 0),
                WindowGeometry::new(150, 100, 250, 200),
            ),
            (
                ResizeEdges::RIGHT,
                (50, 0),
                WindowGeometry::new(100, 100, 350, 200),
            ),
            (
                ResizeEdges::TOP_LEFT,
                (50, 50),
                WindowGeometry::new(150, 150, 250, 150),
            ),
            (
                ResizeEdges::TOP_RIGHT,
                (50, 50),
                WindowGeometry::new(100, 150, 350, 150),
            ),
            (
                ResizeEdges::BOTTOM_LEFT,
                (50, 50),
                WindowGeometry::new(150, 100, 250, 250),
            ),
            (
                ResizeEdges::BOTTOM_RIGHT,
                (50, 50),
                WindowGeometry::new(100, 100, 350, 250),
            ),
        ];

        for (edges, (dx, dy), expected) in cases {
            let grab = InteractiveGrab::resizing("textedit", edges, 200, 200, start).unwrap();
            assert_eq!(
                geometry_for_interactive_grab(&grab, 200 + dx, 200 + dy, 160, 96, 1024, 768),
                expected,
                "resize edges {edges:?}"
            );
        }
    }

    #[test]
    fn interactive_resize_preserves_fixed_edges_at_output_boundaries() {
        let start = WindowGeometry::new(100, 100, 300, 200);
        let left =
            InteractiveGrab::resizing("textedit", ResizeEdges::LEFT, 200, 200, start).unwrap();
        assert_eq!(
            geometry_for_interactive_grab(&left, -1_000, 200, 160, 96, 800, 600),
            WindowGeometry::new(0, 100, 400, 200)
        );

        let top = InteractiveGrab::resizing("textedit", ResizeEdges::TOP, 200, 200, start).unwrap();
        assert_eq!(
            geometry_for_interactive_grab(&top, 200, -1_000, 160, 96, 800, 600),
            WindowGeometry::new(100, 0, 300, 300)
        );
    }

    #[test]
    fn interactive_resize_clamps_every_edge_and_corner_to_minimum() {
        let start = WindowGeometry::new(100, 100, 300, 200);
        let cases = [
            (
                ResizeEdges::TOP,
                (0, 250),
                WindowGeometry::new(100, 180, 300, 120),
            ),
            (
                ResizeEdges::BOTTOM,
                (0, -250),
                WindowGeometry::new(100, 100, 300, 120),
            ),
            (
                ResizeEdges::LEFT,
                (250, 0),
                WindowGeometry::new(240, 100, 160, 200),
            ),
            (
                ResizeEdges::RIGHT,
                (-250, 0),
                WindowGeometry::new(100, 100, 160, 200),
            ),
            (
                ResizeEdges::TOP_LEFT,
                (250, 250),
                WindowGeometry::new(240, 180, 160, 120),
            ),
            (
                ResizeEdges::TOP_RIGHT,
                (-250, 250),
                WindowGeometry::new(100, 180, 160, 120),
            ),
            (
                ResizeEdges::BOTTOM_LEFT,
                (250, -250),
                WindowGeometry::new(240, 100, 160, 120),
            ),
            (
                ResizeEdges::BOTTOM_RIGHT,
                (-250, -250),
                WindowGeometry::new(100, 100, 160, 120),
            ),
        ];

        for (edges, (dx, dy), expected) in cases {
            let grab = InteractiveGrab::resizing("textedit", edges, 200, 200, start).unwrap();
            assert_eq!(
                geometry_for_interactive_grab(&grab, 200 + dx, 200 + dy, 160, 120, 800, 600),
                expected,
                "minimum size for resize edges {edges:?}"
            );
        }
    }

    #[test]
    fn pointer_grab_requires_live_same_surface_press_and_owned_seat() {
        assert!(pointer_grab_request_is_valid(
            42,
            Some(42),
            true,
            true,
            true
        ));
        assert!(!pointer_grab_request_is_valid(
            43,
            Some(42),
            true,
            true,
            true
        ));
        assert!(!pointer_grab_request_is_valid(
            42,
            Some(42),
            false,
            true,
            true
        ));
        assert!(!pointer_grab_request_is_valid(
            42,
            Some(42),
            true,
            false,
            true
        ));
        assert!(!pointer_grab_request_is_valid(
            42,
            Some(42),
            true,
            true,
            false
        ));
        assert!(!pointer_grab_request_is_valid(42, None, true, true, true));
        assert!(!pointer_grab_request_is_valid(0, Some(0), true, true, true));
    }

    #[test]
    fn pointer_grab_accepts_only_the_normalized_requesting_window_owner() {
        // A child/subsurface press is normalized by the backend to its mapped
        // window owner before this policy boundary.
        assert!(pointer_grab_request_is_valid_for_window(
            42,
            Some(42),
            "finder-window",
            Some("finder-window"),
            true,
            true,
            true,
        ));
        // An arbitrary same-client surface has no mapped owner; a surface from
        // another mapped window has a different owner. Neither may authorize.
        assert!(!pointer_grab_request_is_valid_for_window(
            42,
            Some(42),
            "finder-window",
            None,
            true,
            true,
            true,
        ));
        assert!(!pointer_grab_request_is_valid_for_window(
            42,
            Some(42),
            "finder-window",
            Some("settings-window"),
            true,
            true,
            true,
        ));
        assert!(!pointer_grab_request_is_valid_for_window(
            42,
            Some(42),
            "finder-window",
            Some("finder-window"),
            true,
            true,
            false,
        ));
    }

    #[test]
    fn releasing_interactive_grab_clears_grab_press_and_button_state() {
        let mut grab = Some(InteractiveGrab::moving(
            "finder",
            10,
            20,
            WindowGeometry::new(40, 50, 300, 200),
        ));
        let mut pointer_press = Some(42_u32);
        let mut left_button_down = true;

        let released =
            clear_interactive_grab_state(&mut grab, &mut pointer_press, &mut left_button_down);

        assert!(released.is_some());
        assert!(grab.is_none());
        assert!(pointer_press.is_none());
        assert!(!left_button_down);
    }
}
