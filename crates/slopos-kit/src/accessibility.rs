//! AT-SPI2 accessibility export and pure a11y helpers for SLOPOS-I.
//!
//! # What works today
//! - Role name + numeric `AtspiRole` mapping for kit chrome and widgets
//! - Flat top-level + recursively nested live-widget `AccessibilityTree` → D-Bus `Accessible` objects
//! - `org.a11y.atspi.Action` on actionable roles (Activate / Press / Focus)
//! - Pure keyboard chrome focus-order policy (menu bar → desktop icons → dock)
//! - Session / a11y-bus registration with best-effort registry `Socket.Embed`
//! - Pure [`serialize_event_for_dbus`] + best-effort D-Bus signal emission via
//!   [`try_emit_atspi_dbus_event`] when registration holds a connection
//! - In-process [`AccessibilityEventBus`] always works (tests + toolkit path)
//!
//! # Orca-incomplete (honest — do not claim “Orca complete”)
//! - **D-Bus event emission is best-effort only**: may fail without an AT-SPI
//!   registry / a11y bus / prior [`register_at_spi_app_with_tree`]. In-process
//!   queue always succeeds. Orca may still miss events if registry Embed failed
//!   or listeners never attached.
//! - **Tree synchronization is explicit**: [`sync_at_spi_registered_tree`]
//!   replaces the exported object graph when the owning application supplies a
//!   new widget snapshot.  Applications without an update loop remain
//!   register-time snapshots.
//! - **`org.a11y.atspi.Text` is exported on D-Bus** for text-bearing roles at
//!   registration (label snapshot + caret props). **Live typing is not synced**
//!   until re-register; Orca may still miss mid-session edits.
//! - **`org.a11y.atspi.Component` is exported** with GetExtents/Contains; extents
//!   are often zero until layout bounds are filled into the tree.
//! - **`DoAction`**: valid indices push onto [`drain_pending_actions`] **and**
//!   invoke process-local handlers from [`register_action_invoke_handler`]
//!   (`OnceLock`/`Mutex`/`Vec`). Shell drains the queue in `update()` into real
//!   chrome/session handlers. Still not Orca-complete (snapshot tree, no live
//!   widget drive for every control).
//! - **Nested snapshots**: authored and live-widget child nodes are exported recursively.
//! - **Selection / Table / Value** pure helpers are partial; full Collection not shipped.
//! - **Relation set** pure helpers only ([`AccessibleRelation`]).
//! - **Shell chrome tree is structural**, not bound to live menu/dock widgets.
//!
//! Raising Orca-usable domain means keeping roles/actions present and testable,
//! not claiming full assistive-tech parity.

use crate::{Rect, Visibility, Widget, WidgetId};
use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};

use zbus::blocking::connection::Builder as ConnectionBuilder;
use zbus::blocking::{Connection, ObjectServer};
use zbus::zvariant::OwnedObjectPath;
use zbus::{fdo, interface};

// ---------------------------------------------------------------------------
// In-process pending DoAction queue (live bridge for shell / tests)
// ---------------------------------------------------------------------------

/// One AT-SPI `DoAction` request retained for the toolkit/shell to drain.
///
/// Pure + live: D-Bus `DoAction` and test helpers both push here. The shell
/// drains in `update()` and maps paths / object names onto real handlers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingAccessibleAction {
    /// Accessible object path that received DoAction.
    pub path: String,
    /// Accessible `Name` at register time (menu item label, button label, …).
    pub object_name: String,
    /// Action index passed to DoAction.
    pub action_index: i32,
    /// Canonical action name (`Activate`, `Press`, `Focus`, …).
    pub action_name: String,
}

fn pending_action_queue() -> &'static Mutex<VecDeque<PendingAccessibleAction>> {
    static Q: OnceLock<Mutex<VecDeque<PendingAccessibleAction>>> = OnceLock::new();
    Q.get_or_init(|| Mutex::new(VecDeque::new()))
}

/// Push a DoAction request onto the process-local queue (always works in-process).
pub fn push_pending_action(action: PendingAccessibleAction) {
    if let Ok(mut q) = pending_action_queue().lock() {
        q.push_back(action);
    }
}

/// Drain all pending DoAction requests (FIFO). Used by shell `update()` and tests.
pub fn drain_pending_actions() -> Vec<PendingAccessibleAction> {
    pending_action_queue()
        .lock()
        .map(|mut q| q.drain(..).collect())
        .unwrap_or_default()
}

/// Number of queued DoAction requests.
pub fn pending_action_count() -> usize {
    pending_action_queue().lock().map(|q| q.len()).unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Process-local DoAction callback registry (host-safe; no D-Bus required)
// ---------------------------------------------------------------------------

/// Handler for AT-SPI `DoAction`: `(action_name, object_path) -> handled`.
///
/// First handler that returns `true` stops the chain. Shell may register
/// handlers for action names; tests register pure counters. Safe on macOS.
pub type ActionInvokeHandler = Box<dyn FnMut(&str, &str) -> bool + Send>;

fn action_invoke_handlers() -> &'static Mutex<Vec<ActionInvokeHandler>> {
    static HANDLERS: OnceLock<Mutex<Vec<ActionInvokeHandler>>> = OnceLock::new();
    HANDLERS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Register a process-local handler invoked from D-Bus / pure `DoAction`.
pub fn register_action_invoke_handler<F>(handler: F)
where
    F: FnMut(&str, &str) -> bool + Send + 'static,
{
    if let Ok(mut guard) = action_invoke_handlers().lock() {
        guard.push(Box::new(handler));
    }
}

/// Remove all process-local action handlers (tests / shell teardown).
pub fn clear_action_invoke_handlers() {
    if let Ok(mut guard) = action_invoke_handlers().lock() {
        guard.clear();
    }
}

/// Number of registered invoke handlers (tests).
pub fn action_invoke_handler_count() -> usize {
    action_invoke_handlers()
        .lock()
        .map(|g| g.len())
        .unwrap_or(0)
}

/// Invoke registered handlers for `(action_name, object_path)`.
///
/// Returns `true` if any handler reported handled. Host-safe (no D-Bus).
pub fn try_invoke_registered_action(action_name: &str, object_path: &str) -> bool {
    match action_invoke_handlers().lock() {
        Ok(mut guard) => {
            for handler in guard.iter_mut() {
                if handler(action_name, object_path) {
                    return true;
                }
            }
            false
        }
        Err(_) => false,
    }
}

// ---------------------------------------------------------------------------
// AT-SPI2 constants & pure helpers (no D-Bus required)
// ---------------------------------------------------------------------------

/// Canonical object path for an application's accessible root.
pub const ATSPI_ROOT_PATH: &str = "/org/a11y/atspi/accessible/root";

/// Null parent reference path used by AT-SPI for the application root.
pub const ATSPI_NULL_PATH: &str = "/org/a11y/atspi/null";

/// Prefix for per-node accessible object paths.
pub const ATSPI_ACCESSIBLE_PREFIX: &str = "/org/a11y/atspi/accessible";

/// AT-SPI Action name: primary activation (menu item, list item, default click).
pub const ACTION_ACTIVATE: &str = "Activate";

/// AT-SPI Action name: press (buttons and similar push controls).
pub const ACTION_PRESS: &str = "Press";

/// AT-SPI Action name: move keyboard focus to the object.
pub const ACTION_FOCUS: &str = "Focus";

/// Interface name advertised for actionable accessibles.
pub const ATSPI_ACTION_IFACE: &str = "org.a11y.atspi.Action";

/// Interface name for every accessible object.
pub const ATSPI_ACCESSIBLE_IFACE: &str = "org.a11y.atspi.Accessible";

/// Interface name for the application root.
pub const ATSPI_APPLICATION_IFACE: &str = "org.a11y.atspi.Application";

/// AT-SPI Object event interface (`StateChanged`, `BoundsChanged`, …).
pub const ATSPI_EVENT_OBJECT_IFACE: &str = "org.a11y.atspi.Event.Object";

/// AT-SPI Focus event interface (signal `Focus`).
pub const ATSPI_EVENT_FOCUS_IFACE: &str = "org.a11y.atspi.Event.Focus";

/// Build the D-Bus object path for the Nth flat accessibility-tree node.
///
/// Paths are `/org/a11y/atspi/accessible/{index}` (0-based).
pub fn atspi_object_path(index: usize) -> String {
    format!("{ATSPI_ACCESSIBLE_PREFIX}/{index}")
}

/// Sanitize a label for use in an optional path-segment form.
///
/// Keeps alphanumerics, converts other runs to `_`, trims edges, and falls
/// back to `"node"` when empty.
pub fn sanitize_path_segment(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut prev_us = false;
    for ch in label.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_us = false;
        } else if !prev_us {
            out.push('_');
            prev_us = true;
        }
    }
    let trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "node".to_string()
    } else {
        trimmed
    }
}

/// Optional human-readable path including role + sanitized label.
///
/// Format: `/org/a11y/atspi/accessible/{index}/{role}_{label}`
pub fn atspi_object_path_with_label(index: usize, role_name: &str, label: &str) -> String {
    let role_seg = sanitize_path_segment(role_name);
    let label_seg = sanitize_path_segment(label);
    format!("{ATSPI_ACCESSIBLE_PREFIX}/{index}/{role_seg}_{label_seg}")
}

/// Map a SLOPOS-I role to the numeric `AtspiRole` enum value.
///
/// Values from atspi-constants.h / org.a11y.atspi.Accessible.GetRole docs.
/// Exhaustive match — adding a role without a mapping is a compile error.
pub fn role_to_atspi_role(role: AccessibilityRole) -> u32 {
    match role {
        AccessibilityRole::Window => 23,        // FRAME
        AccessibilityRole::Button => 43,        // PUSH_BUTTON
        AccessibilityRole::Checkbox => 7,       // CHECK_BOX
        AccessibilityRole::RadioButton => 44,   // RADIO_BUTTON
        AccessibilityRole::TextField => 79,     // ENTRY
        AccessibilityRole::Label => 29,         // LABEL
        AccessibilityRole::List => 31,          // LIST
        AccessibilityRole::ListItem => 32,      // LIST_ITEM
        AccessibilityRole::Tree => 65,          // TREE
        AccessibilityRole::TreeItem => 91,      // TREE_ITEM
        AccessibilityRole::Menu => 33,          // MENU
        AccessibilityRole::MenuItem => 35,      // MENU_ITEM
        AccessibilityRole::MenuBar => 34,       // MENU_BAR
        AccessibilityRole::Toolbar => 63,       // TOOL_BAR
        AccessibilityRole::ScrollBar => 48,     // SCROLL_BAR
        AccessibilityRole::Slider => 51,        // SLIDER
        AccessibilityRole::ProgressBar => 42,   // PROGRESS_BAR
        AccessibilityRole::Dialog => 16,        // DIALOG
        AccessibilityRole::Tab => 37,           // PAGE_TAB
        AccessibilityRole::TabGroup => 38,      // PAGE_TAB_LIST
        AccessibilityRole::Image => 27,         // IMAGE
        AccessibilityRole::Link => 88,          // LINK
        AccessibilityRole::Group => 39,         // PANEL
        AccessibilityRole::Table => 55,         // TABLE
        AccessibilityRole::TableCell => 56,     // TABLE_CELL
        AccessibilityRole::TableRow => 90,      // TABLE_ROW
        AccessibilityRole::Column => 57,        // TABLE_COLUMN_HEADER
        AccessibilityRole::Row => 58,           // TABLE_ROW_HEADER
        AccessibilityRole::StaticText => 116,   // STATIC
        AccessibilityRole::ComboBox => 11,      // COMBO_BOX
        AccessibilityRole::SplitView => 53,     // SPLIT_PANE
        AccessibilityRole::Notification => 101, // NOTIFICATION
        // Dock has no dedicated AtspiRole; TOOL_BAR is the closest structural match.
        AccessibilityRole::Dock => 63,    // TOOL_BAR
        AccessibilityRole::Desktop => 14, // DESKTOP_FRAME
        AccessibilityRole::Unknown => 67, // UNKNOWN
    }
}

/// Build the AT-SPI state bitset (two `u32`s) from an accessibility state.
///
/// Bits correspond to `AtspiStateType` indices in atspi-constants.h.
pub fn state_to_atspi_bitset(state: &AccessibilityState, role: AccessibilityRole) -> [u32; 2] {
    let mut bits: u64 = 0;

    // Common always-useful bits for visible interactive UI.
    if state.enabled {
        bits |= 1u64 << 8; // ENABLED
        bits |= 1u64 << 24; // SENSITIVE
    }
    if state.visible {
        bits |= 1u64 << 30; // VISIBLE
        bits |= 1u64 << 25; // SHOWING
    }
    if role.is_focusable() || role.is_chrome_focus_target() {
        bits |= 1u64 << 11; // FOCUSABLE
    }
    if state.focused {
        bits |= 1u64 << 12; // FOCUSED
    }
    if state.selected {
        bits |= 1u64 << 23; // SELECTED
    }
    if state.busy {
        bits |= 1u64 << 3; // BUSY
    }
    if let Some(true) = state.checked {
        bits |= 1u64 << 4; // CHECKED
        bits |= 1u64 << 41; // CHECKABLE
    } else if state.checked == Some(false) {
        bits |= 1u64 << 41; // CHECKABLE
    }
    if let Some(true) = state.expanded {
        bits |= 1u64 << 10; // EXPANDED
        bits |= 1u64 << 9; // EXPANDABLE
    } else if state.expanded == Some(false) {
        bits |= 1u64 << 5; // COLLAPSED
        bits |= 1u64 << 9; // EXPANDABLE
    }

    [
        (bits & 0xffff_ffff) as u32,
        ((bits >> 32) & 0xffff_ffff) as u32,
    ]
}

/// Minimal tree used when the caller has no live widget tree yet.
pub fn default_accessibility_tree(app_name: &str) -> AccessibilityTree {
    let mut tree = AccessibilityTree::new();
    tree.add(AccessibilityNode::new(AccessibilityRole::Window, app_name));
    tree
}

/// Structural shell chrome tree for desktop AT-SPI export.
///
/// Order of top-level nodes matches [`ChromeFocusRegion::ORDER`]:
/// menu bar → desktop (with icon list items) → dock (with launch buttons),
/// plus a frame named `app_name` and a notification region.
///
/// This is **not** wired to live widgets; it deepens the exported tree for
/// ATs and unit tests. Orca still incomplete (see module docs).
pub fn shell_chrome_accessibility_tree(app_name: &str) -> AccessibilityTree {
    let mut tree = AccessibilityTree::new();

    let mut menu_bar = AccessibilityNode::new(AccessibilityRole::MenuBar, "Menu Bar");
    // Retro system menu with session actions discoverable by ATs.
    let mut retro = AccessibilityNode::new(AccessibilityRole::Menu, "SLOPOS");
    for item in [
        "About SLOPOS-I",
        "System Settings…",
        "Lock Screen",
        "Sleep",
        "Restart…",
        "Shut Down…",
        "Log Out…",
        "Quit SLOPOS-I",
    ] {
        retro
            .children
            .push(AccessibilityNode::new(AccessibilityRole::MenuItem, item));
    }
    menu_bar.children.push(retro);
    for title in ["File", "Edit", "View", "Window", "Help"] {
        let mut menu = AccessibilityNode::new(AccessibilityRole::Menu, title);
        menu.children
            .push(AccessibilityNode::new(AccessibilityRole::MenuItem, title));
        menu_bar.children.push(menu);
    }
    tree.add(menu_bar);

    let mut desktop = AccessibilityNode::new(AccessibilityRole::Desktop, "Desktop");
    for icon in ["Home", "Trash", "Applications"] {
        desktop
            .children
            .push(AccessibilityNode::new(AccessibilityRole::ListItem, icon));
    }
    tree.add(desktop);

    let mut dock = AccessibilityNode::new(AccessibilityRole::Dock, "Dock");
    for app in ["Finder", "Terminal", "Settings", "TextEdit", "Calculator"] {
        dock.children
            .push(AccessibilityNode::new(AccessibilityRole::Button, app));
    }
    tree.add(dock);

    tree.add(AccessibilityNode::new(AccessibilityRole::Window, app_name));

    let mut notify = AccessibilityNode::new(AccessibilityRole::Notification, "Notification Center");
    notify.children.push(AccessibilityNode::new(
        AccessibilityRole::List,
        "Notifications",
    ));
    tree.add(notify);

    tree
}

// ---------------------------------------------------------------------------
// AccessibleAction — pure Activate / Press / Focus set
// ---------------------------------------------------------------------------

/// Canonical AT-SPI actions SLOPOS-I exposes on interactive nodes.
///
/// Names match common AT-SPI / ATK action strings so Orca and similar ATs can
/// discover them via `org.a11y.atspi.Action`. Invoking `DoAction` on the bus
/// queues a [`PendingAccessibleAction`] for the shell to drain (see module notes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AccessibleAction {
    /// Primary activation (default click / open / invoke).
    Activate,
    /// Press for push-button style controls.
    Press,
    /// Move keyboard focus to the object.
    Focus,
}

impl AccessibleAction {
    /// All shipped action variants (order is stable for regression tests).
    pub const ALL: [AccessibleAction; 3] = [
        AccessibleAction::Activate,
        AccessibleAction::Press,
        AccessibleAction::Focus,
    ];

    /// AT-SPI action name string.
    pub fn name(self) -> &'static str {
        match self {
            Self::Activate => ACTION_ACTIVATE,
            Self::Press => ACTION_PRESS,
            Self::Focus => ACTION_FOCUS,
        }
    }

    /// Human-readable description for `GetDescription`.
    pub fn description(self) -> &'static str {
        match self {
            Self::Activate => "Activates the accessible object",
            Self::Press => "Presses the control",
            Self::Focus => "Gives keyboard focus to the object",
        }
    }

    /// Key binding string for AT-SPI (empty when toolkit-owned / unknown).
    pub fn key_binding(self) -> &'static str {
        match self {
            Self::Activate => "Return",
            Self::Press => "space",
            Self::Focus => "",
        }
    }

    /// Parse a canonical action name (case-sensitive AT-SPI form).
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            ACTION_ACTIVATE => Some(Self::Activate),
            ACTION_PRESS => Some(Self::Press),
            ACTION_FOCUS => Some(Self::Focus),
            _ => None,
        }
    }
}

/// Actions advertised for a role on the AT-SPI Action interface.
///
/// Pure policy — no D-Bus. Removing Activate/Press/Focus coverage for these
/// roles should fail unit tests in this module.
pub fn actions_for_role(role: AccessibilityRole) -> Vec<AccessibleAction> {
    use AccessibleAction::*;
    match role {
        AccessibilityRole::Button | AccessibilityRole::Link => {
            vec![Activate, Press, Focus]
        }
        AccessibilityRole::MenuItem
        | AccessibilityRole::ListItem
        | AccessibilityRole::TreeItem
        | AccessibilityRole::Checkbox
        | AccessibilityRole::RadioButton
        | AccessibilityRole::Tab => {
            vec![Activate, Focus]
        }
        AccessibilityRole::TextField
        | AccessibilityRole::Slider
        | AccessibilityRole::ComboBox
        | AccessibilityRole::MenuBar
        | AccessibilityRole::Dock
        | AccessibilityRole::Desktop
        | AccessibilityRole::Menu
        | AccessibilityRole::Toolbar => {
            vec![Focus]
        }
        AccessibilityRole::Window
        | AccessibilityRole::Label
        | AccessibilityRole::List
        | AccessibilityRole::Tree
        | AccessibilityRole::ScrollBar
        | AccessibilityRole::ProgressBar
        | AccessibilityRole::Dialog
        | AccessibilityRole::TabGroup
        | AccessibilityRole::Image
        | AccessibilityRole::Group
        | AccessibilityRole::Table
        | AccessibilityRole::TableCell
        | AccessibilityRole::TableRow
        | AccessibilityRole::Column
        | AccessibilityRole::Row
        | AccessibilityRole::StaticText
        | AccessibilityRole::SplitView
        | AccessibilityRole::Notification
        | AccessibilityRole::Unknown => Vec::new(),
    }
}

/// Whether this role should advertise `org.a11y.atspi.Action`.
pub fn role_has_actions(role: AccessibilityRole) -> bool {
    !actions_for_role(role).is_empty()
}

/// AT-SPI Text interface name.
pub const ATSPI_TEXT_IFACE: &str = "org.a11y.atspi.Text";

/// AT-SPI Component interface name.
pub const ATSPI_COMPONENT_IFACE: &str = "org.a11y.atspi.Component";

/// Pure in-process text content + caret/selection for AT-SPI Text queries.
///
/// Not exported on D-Bus yet — callers use this for toolkit tests and future
/// bus wiring. Character offsets are UTF-8 byte indices for simplicity in tests
/// (real AT-SPI uses UTF-16 offsets; conversion can land with bus export).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AccessibleTextState {
    pub text: String,
    pub caret_offset: usize,
    pub selection_start: usize,
    pub selection_end: usize,
}

impl AccessibleTextState {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let len = text.len();
        Self {
            text,
            caret_offset: len,
            selection_start: 0,
            selection_end: 0,
        }
    }

    pub fn character_count(&self) -> i32 {
        self.text.chars().count() as i32
    }

    pub fn get_text(&self, start: usize, end: usize) -> String {
        let end = end.min(self.text.len());
        let start = start.min(end);
        // Byte-oriented slice with char boundary clamp.
        let start = floor_char_boundary(&self.text, start);
        let end = floor_char_boundary(&self.text, end);
        self.text[start..end].to_string()
    }

    pub fn set_caret(&mut self, offset: usize) {
        self.caret_offset = floor_char_boundary(&self.text, offset.min(self.text.len()));
    }

    pub fn set_selection(&mut self, start: usize, end: usize) {
        let start = floor_char_boundary(&self.text, start.min(self.text.len()));
        let end = floor_char_boundary(&self.text, end.min(self.text.len()));
        if start <= end {
            self.selection_start = start;
            self.selection_end = end;
        } else {
            self.selection_start = end;
            self.selection_end = start;
        }
    }

    pub fn selected_text(&self) -> String {
        self.get_text(self.selection_start, self.selection_end)
    }
}

fn floor_char_boundary(s: &str, mut i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Pure Component extents (screen-relative logical pixels).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AccessibleComponent {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl AccessibleComponent {
    pub fn from_rect(r: Rect) -> Self {
        Self {
            x: r.x as i32,
            y: r.y as i32,
            width: r.width.max(0.0) as i32,
            height: r.height.max(0.0) as i32,
        }
    }

    pub fn contains_point(self, px: i32, py: i32) -> bool {
        px >= self.x
            && py >= self.y
            && px < self.x.saturating_add(self.width)
            && py < self.y.saturating_add(self.height)
    }

    /// AT-SPI GetExtents-style 4-tuple.
    pub fn extents(self) -> (i32, i32, i32, i32) {
        (self.x, self.y, self.width, self.height)
    }
}

/// AT-SPI relation type (subset used by shell).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AccessibleRelationKind {
    LabelFor,
    LabelledBy,
    ControlledBy,
    ControllerFor,
    MemberOf,
}

impl AccessibleRelationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LabelFor => "label-for",
            Self::LabelledBy => "labelled-by",
            Self::ControlledBy => "controlled-by",
            Self::ControllerFor => "controller-for",
            Self::MemberOf => "member-of",
        }
    }
}

/// One relation edge: `from_path` —kind→ `to_path`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessibleRelation {
    pub kind: AccessibleRelationKind,
    pub from_path: String,
    pub to_path: String,
}

/// Pure Selection model for lists (selected child indices).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AccessibleSelectionState {
    pub selected: Vec<usize>,
    pub multi: bool,
}

impl AccessibleSelectionState {
    pub fn select(&mut self, index: usize) {
        if self.multi {
            if !self.selected.contains(&index) {
                self.selected.push(index);
            }
        } else {
            self.selected = vec![index];
        }
    }

    pub fn deselect(&mut self, index: usize) {
        self.selected.retain(|&i| i != index);
    }

    pub fn clear(&mut self) {
        self.selected.clear();
    }

    pub fn is_selected(&self, index: usize) -> bool {
        self.selected.contains(&index)
    }

    pub fn n_selected(&self) -> usize {
        self.selected.len()
    }
}

/// Whether this role should advertise Text (pure; bus export separate).
pub fn role_has_text(role: AccessibilityRole) -> bool {
    matches!(
        role,
        AccessibilityRole::TextField
            | AccessibilityRole::Label
            | AccessibilityRole::StaticText
            | AccessibilityRole::MenuItem
    )
}

/// Whether this role should advertise Component extents.
pub fn role_has_component(role: AccessibilityRole) -> bool {
    !matches!(role, AccessibilityRole::Unknown)
}

/// Interface names for a node with the given role (Accessible ± Action ± Text ± Component).
pub fn interfaces_for_role(role: AccessibilityRole) -> Vec<String> {
    let mut ifaces = vec![ATSPI_ACCESSIBLE_IFACE.to_string()];
    if role_has_actions(role) {
        ifaces.push(ATSPI_ACTION_IFACE.to_string());
    }
    if role_has_text(role) {
        ifaces.push(ATSPI_TEXT_IFACE.to_string());
    }
    if role_has_component(role) {
        ifaces.push(ATSPI_COMPONENT_IFACE.to_string());
    }
    ifaces
}

// ---------------------------------------------------------------------------
// Keyboard-only chrome navigation policy (pure)
// ---------------------------------------------------------------------------

/// Shell chrome regions in keyboard-only focus cycle order.
///
/// Pure policy used by shell keyboard paths (F6-style) and tests. Not a full
/// focus manager — live routing still belongs in the shell event loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChromeFocusRegion {
    /// Global menu bar.
    MenuBar,
    /// Desktop icon field (list items under the desktop frame).
    DesktopIcons,
    /// Application dock.
    Dock,
}

impl ChromeFocusRegion {
    /// Fixed keyboard cycle: menu bar → desktop icons → dock → (wrap).
    pub const ORDER: [ChromeFocusRegion; 3] = [
        ChromeFocusRegion::MenuBar,
        ChromeFocusRegion::DesktopIcons,
        ChromeFocusRegion::Dock,
    ];

    /// Next region in the chrome cycle (wraps).
    pub fn next(self) -> Self {
        match self {
            Self::MenuBar => Self::DesktopIcons,
            Self::DesktopIcons => Self::Dock,
            Self::Dock => Self::MenuBar,
        }
    }

    /// Previous region in the chrome cycle (wraps).
    pub fn prev(self) -> Self {
        match self {
            Self::MenuBar => Self::Dock,
            Self::DesktopIcons => Self::MenuBar,
            Self::Dock => Self::DesktopIcons,
        }
    }

    /// Primary accessibility role for this chrome region.
    pub fn primary_role(self) -> AccessibilityRole {
        match self {
            Self::MenuBar => AccessibilityRole::MenuBar,
            Self::DesktopIcons => AccessibilityRole::Desktop,
            Self::Dock => AccessibilityRole::Dock,
        }
    }

    /// True if `role` belongs to this chrome region (container or children).
    pub fn matches_role(self, role: AccessibilityRole) -> bool {
        match self {
            Self::MenuBar => matches!(
                role,
                AccessibilityRole::MenuBar | AccessibilityRole::Menu | AccessibilityRole::MenuItem
            ),
            Self::DesktopIcons => matches!(
                role,
                AccessibilityRole::Desktop | AccessibilityRole::ListItem | AccessibilityRole::Image
            ),
            Self::Dock => matches!(
                role,
                AccessibilityRole::Dock | AccessibilityRole::Button | AccessibilityRole::Toolbar
            ),
        }
    }

    /// Map a role to its chrome region, if any.
    pub fn from_role(role: AccessibilityRole) -> Option<Self> {
        for region in Self::ORDER {
            // Prefer primary container roles for ambiguous children (Button also
            // appears outside the dock). Container match first.
            if role == region.primary_role() {
                return Some(region);
            }
        }
        match role {
            AccessibilityRole::Menu | AccessibilityRole::MenuItem => Some(Self::MenuBar),
            AccessibilityRole::ListItem | AccessibilityRole::Image => Some(Self::DesktopIcons),
            // Buttons are not uniquely dock; callers should use tree position.
            _ => None,
        }
    }
}

/// Advance chrome focus region for keyboard-only navigation.
///
/// `current = None` starts at the first region (menu bar).
pub fn next_chrome_focus_region(current: Option<ChromeFocusRegion>) -> ChromeFocusRegion {
    match current {
        None => ChromeFocusRegion::ORDER[0],
        Some(r) => r.next(),
    }
}

/// Step backward through chrome regions.
pub fn prev_chrome_focus_region(current: Option<ChromeFocusRegion>) -> ChromeFocusRegion {
    match current {
        None => *ChromeFocusRegion::ORDER.last().unwrap(),
        Some(r) => r.prev(),
    }
}

/// Flat indices of top-level tree nodes matching chrome cycle order.
///
/// Looks for the first top-level node whose role is the region's primary role,
/// in [`ChromeFocusRegion::ORDER`]. Used to test keyboard path wiring against
/// a structural tree without a live compositor.
pub fn chrome_focus_indices(tree: &AccessibilityTree) -> Vec<(ChromeFocusRegion, usize)> {
    let mut out = Vec::new();
    for region in ChromeFocusRegion::ORDER {
        if let Some((idx, _)) = tree
            .nodes()
            .iter()
            .enumerate()
            .find(|(_, n)| n.role == region.primary_role())
        {
            out.push((region, idx));
        }
    }
    out
}

/// Next chrome flat index after `current` (wraps within available chrome nodes).
pub fn next_chrome_focus_index(tree: &AccessibilityTree, current: Option<usize>) -> Option<usize> {
    let order = chrome_focus_indices(tree);
    if order.is_empty() {
        return None;
    }
    let positions: Vec<usize> = order.iter().map(|(_, i)| *i).collect();
    match current {
        None => Some(positions[0]),
        Some(cur) => {
            if let Some(pos) = positions.iter().position(|&i| i == cur) {
                Some(positions[(pos + 1) % positions.len()])
            } else {
                Some(positions[0])
            }
        }
    }
}

/// Focusable node indices in tree order (flat nodes only, not nested children).
pub fn focusable_indices(tree: &AccessibilityTree) -> Vec<usize> {
    tree.nodes()
        .iter()
        .enumerate()
        .filter(|(_, n)| n.role.is_focusable() || n.role.is_chrome_focus_target())
        .map(|(i, _)| i)
        .collect()
}

// ---------------------------------------------------------------------------
// In-process AT-SPI-shaped events + pure D-Bus serialization
// ---------------------------------------------------------------------------

/// Kind of accessibility event mirrored after common AT-SPI Object / Focus events.
///
/// Toolkit-facing tags; use [`serialize_event_for_dbus`] for exact
/// `org.a11y.atspi.Event.*` interface / member names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AccessibleEventKind {
    /// Keyboard / AT focus moved to (or left) an accessible.
    Focus,
    /// A state bit changed (e.g. focused, selected, checked).
    StateChanged,
    /// Component extents changed.
    BoundsChanged,
    /// Active descendant within a container changed.
    ActiveDescendantChanged,
    /// Accessible object created / added to the tree.
    ObjectCreated,
    /// Accessible object destroyed / removed from the tree.
    ObjectDestroyed,
}

impl AccessibleEventKind {
    /// Stable string tag for tests and logs (not always the D-Bus member name).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Focus => "Focus",
            Self::StateChanged => "StateChanged",
            Self::BoundsChanged => "BoundsChanged",
            Self::ActiveDescendantChanged => "ActiveDescendantChanged",
            Self::ObjectCreated => "ObjectCreated",
            Self::ObjectDestroyed => "ObjectDestroyed",
        }
    }
}

/// Pure D-Bus signal fields for an AT-SPI accessibility event.
///
/// Body layout matches the AT-SPI2 event tuple `(s, i, i, …)` detail fields as
/// plain strings / `i32`s — no zvariant dependency on the pure path. Full bus
/// emission wraps `any_data` as a D-Bus variant and appends an empty properties
/// dict (`a{sv}`) in [`try_emit_atspi_dbus_event`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerializedAtspiEvent {
    /// D-Bus interface, e.g. `org.a11y.atspi.Event.Object`.
    pub interface: &'static str,
    /// Signal member name, e.g. `StateChanged` or `Focus`.
    pub member: &'static str,
    /// Object path of the emitting accessible.
    pub path: String,
    /// String detail (state name, ChildrenChanged "add"/"remove", …).
    pub detail: String,
    pub detail1: i32,
    pub detail2: i32,
    /// Free-form string payload (serialized as D-Bus variant on emit).
    pub any_data: String,
}

/// Map a pure [`AccessibleEvent`] to AT-SPI D-Bus signal coordinates.
///
/// Pure helper — does **not** touch D-Bus. Emission is separate and best-effort.
///
/// # Mapping notes
/// - `Focus` → `org.a11y.atspi.Event.Focus` / `Focus`
/// - `StateChanged` → `Event.Object` / `StateChanged` (`detail` = state name)
/// - `BoundsChanged` / `ActiveDescendantChanged` → matching `Event.Object` members
/// - `ObjectCreated` / `ObjectDestroyed` → `ChildrenChanged` with `detail` `add`/`remove`
///   (AT-SPI has no Create/Destroy object signals)
pub fn serialize_event_for_dbus(event: &AccessibleEvent) -> SerializedAtspiEvent {
    match event.kind {
        AccessibleEventKind::Focus => SerializedAtspiEvent {
            interface: ATSPI_EVENT_FOCUS_IFACE,
            member: "Focus",
            path: event.path.clone(),
            detail: String::new(),
            detail1: event.detail1,
            detail2: event.detail2,
            any_data: event.any_data.clone(),
        },
        AccessibleEventKind::StateChanged => SerializedAtspiEvent {
            interface: ATSPI_EVENT_OBJECT_IFACE,
            member: "StateChanged",
            path: event.path.clone(),
            // AT-SPI puts the state name in the detail string field.
            detail: event.any_data.clone(),
            detail1: event.detail1,
            detail2: event.detail2,
            any_data: String::new(),
        },
        AccessibleEventKind::BoundsChanged => SerializedAtspiEvent {
            interface: ATSPI_EVENT_OBJECT_IFACE,
            member: "BoundsChanged",
            path: event.path.clone(),
            detail: String::new(),
            detail1: event.detail1,
            detail2: event.detail2,
            any_data: event.any_data.clone(),
        },
        AccessibleEventKind::ActiveDescendantChanged => SerializedAtspiEvent {
            interface: ATSPI_EVENT_OBJECT_IFACE,
            member: "ActiveDescendantChanged",
            path: event.path.clone(),
            detail: String::new(),
            detail1: event.detail1,
            detail2: event.detail2,
            any_data: event.any_data.clone(),
        },
        AccessibleEventKind::ObjectCreated => SerializedAtspiEvent {
            interface: ATSPI_EVENT_OBJECT_IFACE,
            member: "ChildrenChanged",
            path: event.path.clone(),
            detail: "add".to_string(),
            detail1: event.detail1,
            detail2: event.detail2,
            any_data: event.any_data.clone(),
        },
        AccessibleEventKind::ObjectDestroyed => SerializedAtspiEvent {
            interface: ATSPI_EVENT_OBJECT_IFACE,
            member: "ChildrenChanged",
            path: event.path.clone(),
            detail: "remove".to_string(),
            detail1: event.detail1,
            detail2: event.detail2,
            any_data: event.any_data.clone(),
        },
    }
}

/// One queued accessibility event (AT-SPI-shaped payload).
///
/// Field layout follows the common AT-SPI event tuple:
/// `(path, detail1, detail2, any_data)` plus a typed `kind`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessibleEvent {
    /// D-Bus-style object path of the accessible that emitted the event.
    pub path: String,
    pub kind: AccessibleEventKind,
    /// Primary integer detail (e.g. 1/0 for state on/off).
    pub detail1: i32,
    /// Secondary integer detail.
    pub detail2: i32,
    /// Free-form string payload (state name, role, etc.).
    pub any_data: String,
}

impl AccessibleEvent {
    /// Construct a Focus event for `path` (detail1 = 1, focused).
    pub fn focus(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            kind: AccessibleEventKind::Focus,
            detail1: 1,
            detail2: 0,
            any_data: String::new(),
        }
    }

    /// Construct a StateChanged event (e.g. `any_data = "focused"`, detail1 = 1/0).
    pub fn state_changed(path: impl Into<String>, state_name: &str, enabled: bool) -> Self {
        Self {
            path: path.into(),
            kind: AccessibleEventKind::StateChanged,
            detail1: if enabled { 1 } else { 0 },
            detail2: 0,
            any_data: state_name.to_string(),
        }
    }

    /// Construct a BoundsChanged event.
    pub fn bounds_changed(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            kind: AccessibleEventKind::BoundsChanged,
            detail1: 0,
            detail2: 0,
            any_data: String::new(),
        }
    }

    /// Construct an ActiveDescendantChanged event; `any_data` is the descendant path.
    pub fn active_descendant_changed(
        path: impl Into<String>,
        descendant_path: impl Into<String>,
    ) -> Self {
        Self {
            path: path.into(),
            kind: AccessibleEventKind::ActiveDescendantChanged,
            detail1: 0,
            detail2: 0,
            any_data: descendant_path.into(),
        }
    }

    /// Construct an ObjectCreated event.
    pub fn object_created(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            kind: AccessibleEventKind::ObjectCreated,
            detail1: 0,
            detail2: 0,
            any_data: String::new(),
        }
    }

    /// Construct an ObjectDestroyed event.
    pub fn object_destroyed(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            kind: AccessibleEventKind::ObjectDestroyed,
            detail1: 0,
            detail2: 0,
            any_data: String::new(),
        }
    }
}

/// Pure in-process event queue for accessibility notifications.
///
/// # Honest split: in-process vs D-Bus
///
/// Pushing here **always** works in-process. It does **not** by itself emit
/// AT-SPI D-Bus signals. For best-effort bus emission (when registration holds
/// a connection), use [`try_emit_atspi_dbus_event`] or the shell
/// `atspi_bus` wrapper that queues here **and** tries D-Bus.
///
/// Without an AT-SPI registry / session bus, D-Bus emission fails silently and
/// only this queue retains events. Orca will not see in-process-only events.
///
/// Alias-style name: callers may also think of this as an `EventQueue`.
#[derive(Debug, Default, Clone)]
pub struct AccessibilityEventBus {
    queue: VecDeque<AccessibleEvent>,
}

/// Type alias for callers that prefer queue naming.
pub type EventQueue = AccessibilityEventBus;

impl AccessibilityEventBus {
    /// Empty bus.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of pending events.
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// True when no events are queued.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Append an event to the tail of the queue.
    pub fn push(&mut self, event: AccessibleEvent) {
        self.queue.push_back(event);
    }

    /// Remove and return the oldest event, if any.
    pub fn pop(&mut self) -> Option<AccessibleEvent> {
        self.queue.pop_front()
    }

    /// Drain all pending events in FIFO order, leaving the bus empty.
    pub fn drain(&mut self) -> Vec<AccessibleEvent> {
        self.queue.drain(..).collect()
    }

    /// Peek at pending events without removing them.
    pub fn events(&self) -> impl Iterator<Item = &AccessibleEvent> {
        self.queue.iter()
    }

    /// Push a Focus event for `path`.
    ///
    /// This is the pure helper used by tree focus-path hooks. Still not D-Bus.
    pub fn focus_changed(&mut self, path: impl Into<String>) {
        self.push(AccessibleEvent::focus(path));
    }

    /// Push StateChanged for focus on/off (any_data = `"focused"`).
    pub fn focused_state_changed(&mut self, path: impl Into<String>, focused: bool) {
        self.push(AccessibleEvent::state_changed(path, "focused", focused));
    }

    /// Push BoundsChanged for `path`.
    pub fn bounds_changed(&mut self, path: impl Into<String>) {
        self.push(AccessibleEvent::bounds_changed(path));
    }

    /// Push ActiveDescendantChanged.
    pub fn active_descendant_changed(
        &mut self,
        path: impl Into<String>,
        descendant_path: impl Into<String>,
    ) {
        self.push(AccessibleEvent::active_descendant_changed(
            path,
            descendant_path,
        ));
    }

    /// Push ObjectCreated for `path`.
    pub fn object_created(&mut self, path: impl Into<String>) {
        self.push(AccessibleEvent::object_created(path));
    }

    /// Push ObjectDestroyed for `path`.
    pub fn object_destroyed(&mut self, path: impl Into<String>) {
        self.push(AccessibleEvent::object_destroyed(path));
    }
}

/// Parse a flat node index from a canonical AT-SPI path, if present.
///
/// Accepts `/org/a11y/atspi/accessible/{n}` and optional label suffixes
/// (`…/{n}/…`). Nested `…/{n}/c{j}` paths resolve to the flat parent index `n`.
pub fn flat_index_from_atspi_path(path: &str) -> Option<usize> {
    let prefix = format!("{ATSPI_ACCESSIBLE_PREFIX}/");
    let rest = path.strip_prefix(&prefix)?;
    let first = rest.split('/').next()?;
    if first == "root" {
        return None;
    }
    first.parse().ok()
}

/// Free helper: push a Focus event onto `bus` for `path`.
///
/// Same as [`AccessibilityEventBus::focus_changed`]. Does not emit D-Bus;
/// pair with [`try_emit_atspi_dbus_event`] when a bus connection is desired.
pub fn focus_changed(bus: &mut AccessibilityEventBus, path: impl Into<String>) {
    bus.focus_changed(path);
}

/// Best-effort emit of `event` on the process AT-SPI registration connection.
///
/// Returns `true` only when a D-Bus signal was successfully sent. Returns
/// `false` when no registration connection exists (common on macOS CI, or when
/// session/a11y bus was unavailable at register time), the path is invalid, or
/// `emit_signal` fails. Callers must still push to an in-process bus if they
/// need guaranteed local delivery.
///
/// # Honest limitation
/// Registry listeners / Orca may still not receive the signal if Embed failed
/// or no AT is listening — emission success only means the message left our
/// connection.
pub fn try_emit_atspi_dbus_event(event: &AccessibleEvent) -> bool {
    let serialized = serialize_event_for_dbus(event);
    let Ok(guard) = REGISTRATION.lock() else {
        return false;
    };
    let Some(reg) = guard.as_ref() else {
        return false;
    };
    emit_serialized_on_connection(&reg.connection, &serialized)
}

fn emit_serialized_on_connection(conn: &Connection, s: &SerializedAtspiEvent) -> bool {
    use std::collections::HashMap;
    use zbus::zvariant::Value;

    // AT-SPI2 event body: s i i v a{sv}
    let props: HashMap<&str, Value<'_>> = HashMap::new();
    let body = (
        s.detail.as_str(),
        s.detail1,
        s.detail2,
        Value::from(s.any_data.as_str()),
        props,
    );
    match conn.emit_signal(None::<&str>, s.path.as_str(), s.interface, s.member, &body) {
        Ok(()) => {
            tracing::debug!(
                path = %s.path,
                interface = s.interface,
                member = s.member,
                detail = %s.detail,
                detail1 = s.detail1,
                "AT-SPI D-Bus accessibility event emitted"
            );
            true
        }
        Err(err) => {
            tracing::debug!(
                path = %s.path,
                interface = s.interface,
                member = s.member,
                error = %err,
                "AT-SPI D-Bus accessibility event emit failed (best-effort)"
            );
            false
        }
    }
}

// ---------------------------------------------------------------------------
// Role / state / node / tree
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AccessibilityRole {
    Window,
    Button,
    Checkbox,
    RadioButton,
    TextField,
    Label,
    List,
    ListItem,
    Tree,
    TreeItem,
    Menu,
    MenuItem,
    MenuBar,
    Toolbar,
    ScrollBar,
    Slider,
    ProgressBar,
    Dialog,
    Tab,
    TabGroup,
    Image,
    Link,
    Group,
    Table,
    TableCell,
    TableRow,
    Column,
    Row,
    StaticText,
    ComboBox,
    SplitView,
    Notification,
    Dock,
    Desktop,
    Unknown,
}

impl AccessibilityRole {
    /// Every known role (stable order for exhaustive regression tests).
    pub const ALL: [AccessibilityRole; 35] = [
        Self::Window,
        Self::Button,
        Self::Checkbox,
        Self::RadioButton,
        Self::TextField,
        Self::Label,
        Self::List,
        Self::ListItem,
        Self::Tree,
        Self::TreeItem,
        Self::Menu,
        Self::MenuItem,
        Self::MenuBar,
        Self::Toolbar,
        Self::ScrollBar,
        Self::Slider,
        Self::ProgressBar,
        Self::Dialog,
        Self::Tab,
        Self::TabGroup,
        Self::Image,
        Self::Link,
        Self::Group,
        Self::Table,
        Self::TableCell,
        Self::TableRow,
        Self::Column,
        Self::Row,
        Self::StaticText,
        Self::ComboBox,
        Self::SplitView,
        Self::Notification,
        Self::Dock,
        Self::Desktop,
        Self::Unknown,
    ];

    /// Returns the AT-SPI2 role name string for this role.
    pub fn role_name(&self) -> &'static str {
        match self {
            Self::Window => "frame",
            Self::Button => "push button",
            Self::Checkbox => "check box",
            Self::RadioButton => "radio button",
            Self::TextField => "text",
            Self::Label => "label",
            Self::List => "list",
            Self::ListItem => "list item",
            Self::Tree => "tree",
            Self::TreeItem => "tree item",
            Self::Menu => "menu",
            Self::MenuItem => "menu item",
            Self::MenuBar => "menu bar",
            Self::Toolbar => "tool bar",
            Self::ScrollBar => "scroll bar",
            Self::Slider => "slider",
            Self::ProgressBar => "progress bar",
            Self::Dialog => "dialog",
            Self::Tab => "page tab",
            Self::TabGroup => "page tab list",
            Self::Image => "image",
            Self::Link => "link",
            Self::Group => "panel",
            Self::Table => "table",
            Self::TableCell => "table cell",
            Self::TableRow => "table row",
            Self::Column => "table column header",
            Self::Row => "table row header",
            Self::StaticText => "label",
            Self::ComboBox => "combo box",
            Self::SplitView => "split pane",
            Self::Notification => "alert",
            // Dock reuses tool-bar naming in AT-SPI (no dock-specific role name).
            Self::Dock => "tool bar",
            Self::Desktop => "desktop frame",
            Self::Unknown => "unknown",
        }
    }

    /// Returns true if an element with this role can receive keyboard focus
    /// as a typical interactive control (not chrome region containers).
    pub fn is_focusable(&self) -> bool {
        matches!(
            self,
            Self::Button
                | Self::Checkbox
                | Self::RadioButton
                | Self::TextField
                | Self::ListItem
                | Self::TreeItem
                | Self::MenuItem
                | Self::Tab
                | Self::Slider
                | Self::ComboBox
                | Self::Link
        )
    }

    /// True for shell chrome containers that participate in the keyboard-only
    /// F6-style cycle even when not classic interactive widgets.
    pub fn is_chrome_focus_target(&self) -> bool {
        matches!(
            self,
            Self::MenuBar | Self::Desktop | Self::Dock | Self::Menu | Self::Toolbar
        )
    }
}

#[derive(Debug, Clone)]
pub struct AccessibilityState {
    pub focused: bool,
    pub enabled: bool,
    pub visible: bool,
    pub selected: bool,
    pub checked: Option<bool>,
    pub expanded: Option<bool>,
    pub busy: bool,
}

impl Default for AccessibilityState {
    fn default() -> Self {
        Self {
            focused: false,
            enabled: true,
            visible: true,
            selected: false,
            checked: None,
            expanded: None,
            busy: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AccessibilityNode {
    pub role: AccessibilityRole,
    pub label: String,
    pub description: String,
    pub rect: Rect,
    pub state: AccessibilityState,
    pub children: Vec<AccessibilityNode>,
    pub index: usize,
    pub parent: Option<usize>,
    /// The live widget that produced this node, when the node came from
    /// [`accessibility_tree_from_widget`].  Hand-authored AT-SPI nodes keep
    /// this as `None`, which preserves their legacy numeric object paths.
    ///
    /// Widget ids are process-local but stable for a widget's lifetime.  The
    /// id is therefore a better identity for a live object than its position
    /// in a frame (which can change when a sibling is inserted or removed).
    pub widget_id: Option<WidgetId>,
}

impl AccessibilityNode {
    pub fn new(role: AccessibilityRole, label: &str) -> Self {
        Self {
            role,
            label: label.to_string(),
            description: String::new(),
            rect: Rect::ZERO,
            state: AccessibilityState::default(),
            children: vec![],
            index: 0,
            parent: None,
            widget_id: None,
        }
    }

    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = desc.to_string();
        self
    }

    /// Convenience wrapper: returns the AT-SPI role name for this node's role.
    pub fn role_name(&self) -> &'static str {
        self.role.role_name()
    }

    /// Returns true if this node's role can receive keyboard focus.
    pub fn is_focusable(&self) -> bool {
        self.role.is_focusable()
    }

    /// Actions advertised for this node's role.
    pub fn actions(&self) -> Vec<AccessibleAction> {
        actions_for_role(self.role)
    }

    /// Attach the identity of the live widget represented by this node.
    pub fn with_widget_id(mut self, widget_id: WidgetId) -> Self {
        self.widget_id = Some(widget_id);
        self
    }
}

/// Build a current accessibility snapshot from a live widget tree.
///
/// Widgets are asked for their authored [`AccessibilityNode`] first.  When a
/// widget supplies explicit children (for example a custom application root),
/// those children remain authoritative; otherwise the widget's live
/// [`Widget::children`] are recursively converted.  A widget without an
/// accessibility node is transparent and contributes any accessible
/// descendants it contains.
///
/// The node's geometry and live state are always refreshed from the widget,
/// while role/label/description and authored state fields remain intact.  A
/// `WidgetId` is retained on every generated node so callers can derive stable
/// AT-SPI paths across frame snapshots even when sibling order changes.
pub fn accessibility_tree_from_widget(root: &dyn Widget) -> AccessibilityTree {
    let mut tree = AccessibilityTree::new();
    tree.nodes.extend(accessibility_nodes_from_widget(root));
    tree
}

fn accessibility_nodes_from_widget(widget: &dyn Widget) -> Vec<AccessibilityNode> {
    let children = widget.children();
    let Some(mut node) = widget.accessibility() else {
        // Containers that have no semantic node are transparent.  This keeps
        // a decorative/layout wrapper from hiding accessible descendants.
        return children
            .into_iter()
            .flat_map(accessibility_nodes_from_widget)
            .collect();
    };

    node.rect = widget.rect();
    node.state.enabled = widget.enabled();
    node.state.visible = widget.visibility() == Visibility::Visible;
    node.state.focused = widget.widget_state().focused;
    node.widget_id = Some(widget.id());

    // An authored child list is intentional.  Settings and other custom
    // roots use it to expose semantic children that are not one-to-one with
    // the rendering widgets; deriving Widget::children in that case would
    // duplicate those entries.  Generic widgets with no authored children
    // get their live descendants here.
    if node.children.is_empty() {
        node.children = children
            .into_iter()
            .flat_map(accessibility_nodes_from_widget)
            .collect();
    }

    vec![node]
}

// ---------------------------------------------------------------------------
// AccessibilityTree — flat list of nodes for the current render frame
// ---------------------------------------------------------------------------

/// A flat, ordered collection of `AccessibilityNode` items representing
/// the accessibility tree for the current frame.
#[derive(Debug, Default, Clone)]
pub struct AccessibilityTree {
    nodes: Vec<AccessibilityNode>,
}

/// Stable path segment for a live widget id.  `WidgetId`'s display form is
/// deliberately sanitized so the resulting path remains a valid D-Bus object
/// path component while retaining the numeric id for diagnostics.
fn widget_id_path_segment(widget_id: WidgetId) -> String {
    format!("w_{}", sanitize_path_segment(&widget_id.to_string()))
}

fn widget_id_accessible_id(widget_id: WidgetId) -> String {
    format!("widget_{}", sanitize_path_segment(&widget_id.to_string()))
}

/// Return the object path for a node in its current tree position.
///
/// Live widget nodes use an absolute `w_<WidgetId>` segment.  Authored child
/// nodes have no widget identity and retain the historical `/c{index}` path.
/// Top-level authored nodes retain their canonical numeric index paths.
fn accessibility_node_path(
    node: &AccessibilityNode,
    parent_path: Option<&str>,
    index: usize,
) -> String {
    if let Some(widget_id) = node.widget_id {
        format!(
            "{ATSPI_ACCESSIBLE_PREFIX}/{}",
            widget_id_path_segment(widget_id)
        )
    } else if let Some(parent_path) = parent_path {
        format!("{parent_path}/c{index}")
    } else {
        atspi_object_path(index)
    }
}

impl AccessibilityTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a node to the tree.
    pub fn add(&mut self, node: AccessibilityNode) {
        self.nodes.push(node);
    }

    /// Remove all nodes from the tree.
    pub fn clear(&mut self) {
        self.nodes.clear();
    }

    /// Number of top-level (flat) nodes.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// True when the tree has no nodes.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Borrow the flat node list.
    pub fn nodes(&self) -> &[AccessibilityNode] {
        &self.nodes
    }

    /// Node at a flat index, if any.
    pub fn get(&self, index: usize) -> Option<&AccessibilityNode> {
        self.nodes.get(index)
    }

    /// Return a text representation of each node suitable for AT-SPI object paths.
    /// Format: `"role:<role_name> label:<label>"`.
    pub fn to_atspi_objects(&self) -> Vec<String> {
        self.nodes
            .iter()
            .map(|n| format!("role:{} label:{}", n.role_name(), n.label))
            .collect()
    }

    /// D-Bus object paths for each top-level node.
    ///
    /// Hand-authored nodes retain canonical numeric paths.  Nodes produced
    /// from live widgets use their stable `WidgetId` path segment.
    pub fn to_atspi_paths(&self) -> Vec<String> {
        self.nodes
            .iter()
            .enumerate()
            .map(|(index, node)| accessibility_node_path(node, None, index))
            .collect()
    }

    /// Return the stable object path for a top-level node at `index`.
    pub fn path_for_index(&self, index: usize) -> Option<String> {
        self.nodes
            .get(index)
            .map(|node| accessibility_node_path(node, None, index))
    }

    /// Apply a focus change for the accessible at `path` and emit events.
    ///
    /// Updates flat-node `state.focused` when `path` maps to a flat index via
    /// [`flat_index_from_atspi_path`], then pushes a Focus event (and
    /// StateChanged focused on/off transitions) onto `bus`.
    ///
    /// # Honest limitation
    /// Events are pushed on the in-process [`AccessibilityEventBus`] only.
    /// Callers that want best-effort D-Bus emission should also invoke
    /// [`try_emit_atspi_dbus_event`] (or the shell `atspi_bus` helper) for each
    /// drained event — this method does not touch the bus connection.
    pub fn focus_changed(&mut self, path: &str, bus: &mut AccessibilityEventBus) {
        let target = flat_index_from_atspi_path(path).or_else(|| {
            self.nodes
                .iter()
                .enumerate()
                .find(|(index, node)| accessibility_node_path(node, None, *index) == path)
                .map(|(index, _)| index)
        });

        // A stable widget path can identify a generated descendant directly.
        // Numeric paths retain their historical flat-parent semantics, but a
        // live child should receive the focused state on its own object.
        if target.is_none() {
            let matched = self
                .nodes
                .iter_mut()
                .enumerate()
                .fold(false, |matched, (index, node)| {
                    set_focus_path(node, None, index, path, bus) || matched
                });
            if matched {
                bus.focus_changed(path);
                return;
            }
        }

        // Emit StateChanged for nodes whose focused bit flips.
        for (i, node) in self.nodes.iter_mut().enumerate() {
            let now = target == Some(i);
            if node.state.focused != now {
                let node_path = atspi_object_path(i);
                bus.focused_state_changed(&node_path, now);
                node.state.focused = now;
            }
        }

        // Always emit the Focus event for the requested path (AT focus path).
        bus.focus_changed(path);
    }

    /// Focus the flat node at `index` (canonical path) and emit events.
    pub fn focus_changed_index(&mut self, index: usize, bus: &mut AccessibilityEventBus) {
        if let Some(path) = self.path_for_index(index) {
            self.focus_changed(&path, bus);
        }
    }
}

fn set_focus_path(
    node: &mut AccessibilityNode,
    parent_path: Option<&str>,
    index: usize,
    target_path: &str,
    bus: &mut AccessibilityEventBus,
) -> bool {
    let path = accessibility_node_path(node, parent_path, index);
    let is_target = path == target_path;
    if node.state.focused != is_target {
        bus.focused_state_changed(&path, is_target);
        node.state.focused = is_target;
    }

    let mut matched = is_target;
    for (child_index, child) in node.children.iter_mut().enumerate() {
        matched =
            set_focus_path(child, Some(path.as_str()), child_index, target_path, bus) || matched;
    }
    matched
}

// ---------------------------------------------------------------------------
// D-Bus AT-SPI interfaces
// ---------------------------------------------------------------------------

/// Object reference `(bus_name, object_path)` used throughout AT-SPI.
type ObjectRef = (String, OwnedObjectPath);

fn owned_path(path: &str) -> fdo::Result<OwnedObjectPath> {
    OwnedObjectPath::try_from(path).map_err(|e| fdo::Error::Failed(e.to_string()))
}

fn object_ref(bus: &str, path: &str) -> fdo::Result<ObjectRef> {
    Ok((bus.to_string(), owned_path(path)?))
}

/// `org.a11y.atspi.Accessible` — base interface for every accessible object.
struct AtspiAccessible {
    name: String,
    description: String,
    role: u32,
    role_name: String,
    bus_name: String,
    /// Parent `(bus_name, path)`. Empty bus + null path for the app root.
    parent_bus: String,
    parent_path: String,
    child_paths: Vec<String>,
    index_in_parent: i32,
    state: [u32; 2],
    accessible_id: String,
    /// Interfaces advertised by GetInterfaces (excluding Accessible itself is ok; include it).
    interfaces: Vec<String>,
}

#[interface(name = "org.a11y.atspi.Accessible")]
impl AtspiAccessible {
    #[zbus(property, name = "version")]
    fn version(&self) -> u32 {
        2
    }

    #[zbus(property, name = "Name")]
    fn name(&self) -> &str {
        &self.name
    }

    #[zbus(property, name = "Description")]
    fn description(&self) -> &str {
        &self.description
    }

    #[zbus(property, name = "Parent")]
    fn parent(&self) -> fdo::Result<ObjectRef> {
        object_ref(&self.parent_bus, &self.parent_path)
    }

    #[zbus(property, name = "ChildCount")]
    fn child_count(&self) -> i32 {
        self.child_paths.len() as i32
    }

    #[zbus(property, name = "Locale")]
    fn locale(&self) -> &str {
        ""
    }

    #[zbus(property, name = "AccessibleId")]
    fn accessible_id(&self) -> &str {
        &self.accessible_id
    }

    #[zbus(property, name = "HelpText")]
    fn help_text(&self) -> &str {
        ""
    }

    fn get_child_at_index(&self, index: i32) -> fdo::Result<ObjectRef> {
        if index < 0 {
            return Err(fdo::Error::InvalidArgs(format!(
                "child index {index} out of range"
            )));
        }
        let idx = index as usize;
        match self.child_paths.get(idx) {
            Some(path) => object_ref(&self.bus_name, path),
            None => Err(fdo::Error::InvalidArgs(format!(
                "child index {index} out of range (count={})",
                self.child_paths.len()
            ))),
        }
    }

    fn get_children(&self) -> fdo::Result<Vec<ObjectRef>> {
        self.child_paths
            .iter()
            .map(|p| object_ref(&self.bus_name, p))
            .collect()
    }

    fn get_index_in_parent(&self) -> i32 {
        self.index_in_parent
    }

    fn get_relation_set(&self) -> Vec<(u32, Vec<ObjectRef>)> {
        Vec::new()
    }

    fn get_role(&self) -> u32 {
        self.role
    }

    fn get_role_name(&self) -> String {
        self.role_name.clone()
    }

    fn get_localized_role_name(&self) -> String {
        self.role_name.clone()
    }

    fn get_state(&self) -> Vec<u32> {
        self.state.to_vec()
    }

    fn get_attributes(&self) -> std::collections::HashMap<String, String> {
        std::collections::HashMap::new()
    }

    fn get_application(&self) -> fdo::Result<ObjectRef> {
        object_ref(&self.bus_name, ATSPI_ROOT_PATH)
    }

    fn get_interfaces(&self) -> Vec<String> {
        self.interfaces.clone()
    }
}

/// `org.a11y.atspi.Application` — required on the application root object.
struct AtspiApplication {
    id: i32,
    bus_address: String,
}

#[interface(name = "org.a11y.atspi.Application")]
impl AtspiApplication {
    #[zbus(property, name = "ToolkitName")]
    fn toolkit_name(&self) -> &str {
        "SLOPOS-I"
    }

    #[zbus(property, name = "Version")]
    fn version(&self) -> &str {
        "0.1.0"
    }

    #[zbus(property, name = "ToolkitVersion")]
    fn toolkit_version(&self) -> &str {
        "0.1.0"
    }

    #[zbus(property, name = "AtspiVersion")]
    fn atspi_version(&self) -> &str {
        "2.1"
    }

    #[zbus(property, name = "InterfaceVersion")]
    fn interface_version(&self) -> u32 {
        2
    }

    #[zbus(property, name = "Id")]
    fn id(&self) -> i32 {
        self.id
    }

    #[zbus(property, name = "Id")]
    fn set_id(&mut self, id: i32) {
        self.id = id;
    }

    fn get_locale(&self, _lctype: u32) -> String {
        String::new()
    }

    fn get_application_bus_address(&self) -> String {
        self.bus_address.clone()
    }
}

/// `org.a11y.atspi.Action` — Activate / Press / Focus for actionable nodes.
///
/// `DoAction` validates the index, queues a [`PendingAccessibleAction`], calls
/// process-local invoke handlers, and returns `true` when in range.
struct AtspiAction {
    path: String,
    object_name: String,
    actions: Vec<AccessibleAction>,
}

impl AtspiAction {
    fn from_role(
        role: AccessibilityRole,
        path: impl Into<String>,
        object_name: impl Into<String>,
    ) -> Self {
        Self {
            path: path.into(),
            object_name: object_name.into(),
            actions: actions_for_role(role),
        }
    }

    fn get(&self, index: i32) -> fdo::Result<&AccessibleAction> {
        if index < 0 {
            return Err(fdo::Error::InvalidArgs(format!(
                "action index {index} out of range"
            )));
        }
        self.actions.get(index as usize).ok_or_else(|| {
            fdo::Error::InvalidArgs(format!(
                "action index {index} out of range (count={})",
                self.actions.len()
            ))
        })
    }
}

#[interface(name = "org.a11y.atspi.Action")]
impl AtspiAction {
    #[zbus(property, name = "NActions")]
    fn n_actions(&self) -> i32 {
        self.actions.len() as i32
    }

    fn get_description(&self, index: i32) -> fdo::Result<String> {
        Ok(self.get(index)?.description().to_string())
    }

    fn get_name(&self, index: i32) -> fdo::Result<String> {
        Ok(self.get(index)?.name().to_string())
    }

    fn get_key_binding(&self, index: i32) -> fdo::Result<String> {
        Ok(self.get(index)?.key_binding().to_string())
    }

    /// Queue pending action + call process-local invoke handlers (host-safe).
    fn do_action(&self, index: i32) -> fdo::Result<bool> {
        let action = self.get(index)?;
        let action_name = action.name().to_string();
        push_pending_action(PendingAccessibleAction {
            path: self.path.clone(),
            object_name: self.object_name.clone(),
            action_index: index,
            action_name: action_name.clone(),
        });
        let handled = try_invoke_registered_action(&action_name, &self.path);
        if handled {
            tracing::debug!(
                action = %action_name,
                path = %self.path,
                "AT-SPI DoAction claimed by process-local callback"
            );
        }
        Ok(true)
    }

    /// `(name, description, keybinding)` triples for all actions.
    fn get_actions(&self) -> Vec<(String, String, String)> {
        self.actions
            .iter()
            .map(|a| {
                (
                    a.name().to_string(),
                    a.description().to_string(),
                    a.key_binding().to_string(),
                )
            })
            .collect()
    }
}

/// D-Bus `org.a11y.atspi.Text` — snapshot of label/entry text at register time.
///
/// Caret/selection are initialized to end-of-text / empty. Live typing is not
/// mirrored until re-register/sync lands (Orca may still miss live edits).
struct AtspiText {
    state: AccessibleTextState,
}

impl AtspiText {
    fn from_label(label: &str) -> Self {
        Self {
            state: AccessibleTextState::new(label),
        }
    }
}

#[interface(name = "org.a11y.atspi.Text")]
impl AtspiText {
    #[zbus(property, name = "CharacterCount")]
    fn character_count(&self) -> i32 {
        self.state.character_count()
    }

    #[zbus(property, name = "CaretOffset")]
    fn caret_offset(&self) -> i32 {
        self.state.caret_offset as i32
    }

    fn get_text(&self, start_offset: i32, end_offset: i32) -> String {
        let start = start_offset.max(0) as usize;
        let end = if end_offset < 0 {
            self.state.text.len()
        } else {
            end_offset as usize
        };
        self.state.get_text(start, end)
    }

    fn get_n_selections(&self) -> i32 {
        if self.state.selection_start != self.state.selection_end {
            1
        } else {
            0
        }
    }

    fn get_selection(&self, selection_num: i32) -> fdo::Result<(i32, i32)> {
        if selection_num != 0 || self.state.selection_start == self.state.selection_end {
            return Err(fdo::Error::InvalidArgs("no such selection".into()));
        }
        Ok((
            self.state.selection_start as i32,
            self.state.selection_end as i32,
        ))
    }
}

/// D-Bus `org.a11y.atspi.Component` — extents snapshot (often 0 until layout sync).
struct AtspiComponent {
    component: AccessibleComponent,
}

impl AtspiComponent {
    fn from_node(node: &AccessibilityNode) -> Self {
        let component = if node.rect.width > 0.0 || node.rect.height > 0.0 {
            AccessibleComponent::from_rect(node.rect)
        } else {
            AccessibleComponent::default()
        };
        Self { component }
    }
}

#[interface(name = "org.a11y.atspi.Component")]
impl AtspiComponent {
    /// GetExtents(coord_type) → (x, y, width, height). coord_type ignored (screen).
    fn get_extents(&self, _coord_type: u32) -> (i32, i32, i32, i32) {
        self.component.extents()
    }

    fn get_position(&self, _coord_type: u32) -> (i32, i32) {
        (self.component.x, self.component.y)
    }

    fn get_size(&self) -> (i32, i32) {
        (self.component.width, self.component.height)
    }

    fn contains(&self, x: i32, y: i32, _coord_type: u32) -> bool {
        self.component.contains_point(x, y)
    }
}

// ---------------------------------------------------------------------------
// Registration (session / a11y bus)
// ---------------------------------------------------------------------------

/// Keeps the a11y (or session) bus connection alive for the process lifetime.
struct AtSpiRegistration {
    /// Retained so exported objects remain available to ATs, and reused for
    /// best-effort [`try_emit_atspi_dbus_event`] signal emission.
    connection: Connection,
    app_name: String,
    child_count: usize,
    bus_name: String,
    embedded: bool,
    /// Last widget snapshot exported on the registered object graph.  The
    /// shell compares incoming snapshots against this value before replacing
    /// D-Bus objects, so an idle frame does not churn the AT-SPI registry.
    tree: AccessibilityTree,
}

static REGISTRATION: Mutex<Option<AtSpiRegistration>> = Mutex::new(None);

/// Result of a successful AT-SPI registration attempt.
#[derive(Debug, Clone)]
pub struct AtSpiRegistrationInfo {
    pub app_name: String,
    pub bus_name: String,
    pub child_count: usize,
    pub embedded_with_registry: bool,
}

/// Returns info about the active registration, if any.
pub fn at_spi_registration_info() -> Option<AtSpiRegistrationInfo> {
    REGISTRATION.lock().ok().and_then(|g| {
        g.as_ref().map(|r| AtSpiRegistrationInfo {
            app_name: r.app_name.clone(),
            bus_name: r.bus_name.clone(),
            child_count: r.child_count,
            embedded_with_registry: r.embedded,
        })
    })
}

/// Register this process as an AT-SPI2 application with a default tree.
///
/// Builds a minimal one-window tree named `app_name`. See
/// [`register_at_spi_app_with_tree`] for details. Prefer
/// [`register_at_spi_shell_chrome`] for the desktop shell process.
pub fn register_at_spi_app(app_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let tree = default_accessibility_tree(app_name);
    register_at_spi_app_with_tree(app_name, &tree)
}

/// Register with the structural shell chrome tree (menu bar / desktop / dock).
///
/// Still Orca-incomplete (events best-effort; text/component are register-time
/// snapshots, not live layout/typing). Richer than a single window node.
pub fn register_at_spi_shell_chrome(app_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let tree = shell_chrome_accessibility_tree(app_name);
    register_at_spi_app_with_tree(app_name, &tree)
}

/// Replace the D-Bus object graph for an already registered application.
///
/// AT-SPI applications own their accessible objects, so the toolkit can
/// publish a current widget snapshot by removing the old interfaces and
/// registering the new ones at the same stable numeric paths.  This is a
/// synchronized snapshot path, not an assertion that a registry or assistive
/// technology is present: when registration was skipped, the function returns
/// `Ok(false)` and leaves the in-process tree/event helpers available.
pub fn sync_at_spi_registered_tree(
    tree: &AccessibilityTree,
) -> Result<bool, Box<dyn std::error::Error>> {
    let events = {
        let mut guard = REGISTRATION
            .lock()
            .map_err(|_| "AT-SPI registration mutex poisoned")?;
        let Some(registration) = guard.as_mut() else {
            return Ok(false);
        };

        let tree = normalized_accessibility_tree(&registration.app_name, tree);
        if accessibility_tree_signature(&registration.tree) == accessibility_tree_signature(&tree) {
            return Ok(true);
        }

        let events = accessibility_tree_diff_events(&registration.tree, &tree);
        let server = registration.connection.object_server();
        remove_atspi_tree(&server, &registration.tree)?;
        let child_count = export_atspi_tree(
            &server,
            &registration.app_name,
            &registration.bus_name,
            &tree,
        )?;
        registration.child_count = child_count;
        registration.tree = tree;
        events
    };

    // Emit after releasing REGISTRATION: the public emitter takes the same
    // mutex to borrow the live zbus connection. This keeps the update path
    // deadlock-free while ensuring AT-SPI listeners see ChildrenChanged,
    // StateChanged, Focus and BoundsChanged after the new objects exist.
    for event in &events {
        let _ = try_emit_atspi_dbus_event(event);
    }
    Ok(true)
}

/// Return a non-empty tree so the application root always has a usable
/// accessible child.  The normalized value is stored in the registration and
/// is therefore also the tree used by later synchronization comparisons.
fn normalized_accessibility_tree(app_name: &str, tree: &AccessibilityTree) -> AccessibilityTree {
    if tree.is_empty() {
        default_accessibility_tree(app_name)
    } else {
        tree.clone()
    }
}

/// Stable, cheap comparison key for the exported graph.  Rectangles and state
/// are included because a live overview must update selection/focus and
/// component geometry even when labels and row counts are unchanged.
fn accessibility_tree_signature(tree: &AccessibilityTree) -> String {
    fn append_node(node: &AccessibilityNode, out: &mut String) {
        use std::fmt::Write as _;

        let _ = write!(
            out,
            "{}\u{1f}{}\u{1f}{}\u{1f}{:.3},{:.3},{:.3},{:.3}\u{1f}{:?}\u{1f}{:?}\u{1f}{:?}\u{1f}{:?};",
            node.role_name(),
            node.label,
            node.description,
            node.rect.x,
            node.rect.y,
            node.rect.width,
            node.rect.height,
            node.state,
            node.index,
            node.parent,
            node.widget_id,
        );
        for child in &node.children {
            append_node(child, out);
        }
    }

    let mut signature = String::new();
    for node in tree.nodes() {
        append_node(node, &mut signature);
    }
    signature
}

struct AccessibilityNodeEntry<'a> {
    path: String,
    parent_path: String,
    child_index: usize,
    node: &'a AccessibilityNode,
}

fn collect_accessibility_nodes<'a>(tree: &'a AccessibilityTree) -> Vec<AccessibilityNodeEntry<'a>> {
    fn visit<'a>(
        node: &'a AccessibilityNode,
        path: String,
        parent_path: String,
        child_index: usize,
        entries: &mut Vec<AccessibilityNodeEntry<'a>>,
    ) {
        entries.push(AccessibilityNodeEntry {
            path: path.clone(),
            parent_path,
            child_index,
            node,
        });
        for (index, child) in node.children.iter().enumerate() {
            let child_path = accessibility_node_path(child, Some(path.as_str()), index);
            visit(child, child_path, path.clone(), index, entries);
        }
    }

    let mut entries = Vec::new();
    for (index, node) in tree.nodes.iter().enumerate() {
        let path = accessibility_node_path(node, None, index);
        visit(node, path, ATSPI_ROOT_PATH.to_string(), index, &mut entries);
    }
    entries
}

fn rect_changed(previous: Rect, current: Rect) -> bool {
    previous.x.to_bits() != current.x.to_bits()
        || previous.y.to_bits() != current.y.to_bits()
        || previous.width.to_bits() != current.width.to_bits()
        || previous.height.to_bits() != current.height.to_bits()
}

fn children_changed_event(
    kind: AccessibleEventKind,
    parent_path: &str,
    child_index: usize,
    child_path: &str,
) -> AccessibleEvent {
    AccessibleEvent {
        path: parent_path.to_string(),
        kind,
        detail1: i32::try_from(child_index).unwrap_or(i32::MAX),
        detail2: 0,
        any_data: child_path.to_string(),
    }
}

fn accessibility_tree_diff_events(
    previous: &AccessibilityTree,
    current: &AccessibilityTree,
) -> Vec<AccessibleEvent> {
    let previous_entries = collect_accessibility_nodes(previous);
    let current_entries = collect_accessibility_nodes(current);
    let previous_by_path: HashMap<&str, &AccessibilityNodeEntry<'_>> = previous_entries
        .iter()
        .map(|entry| (entry.path.as_str(), entry))
        .collect();
    let current_by_path: HashMap<&str, &AccessibilityNodeEntry<'_>> = current_entries
        .iter()
        .map(|entry| (entry.path.as_str(), entry))
        .collect();

    let mut events = Vec::new();
    for current_entry in &current_entries {
        let Some(previous_entry) = previous_by_path.get(current_entry.path.as_str()) else {
            events.push(children_changed_event(
                AccessibleEventKind::ObjectCreated,
                &current_entry.parent_path,
                current_entry.child_index,
                &current_entry.path,
            ));
            continue;
        };

        let previous_state = &previous_entry.node.state;
        let current_state = &current_entry.node.state;
        let state_changes = [
            ("focused", previous_state.focused, current_state.focused),
            ("enabled", previous_state.enabled, current_state.enabled),
            ("visible", previous_state.visible, current_state.visible),
            ("selected", previous_state.selected, current_state.selected),
            (
                "checked",
                previous_state.checked.unwrap_or(false),
                current_state.checked.unwrap_or(false),
            ),
            (
                "expanded",
                previous_state.expanded.unwrap_or(false),
                current_state.expanded.unwrap_or(false),
            ),
            ("busy", previous_state.busy, current_state.busy),
        ];
        for (state_name, previous_value, current_value) in state_changes {
            if previous_value != current_value {
                events.push(AccessibleEvent::state_changed(
                    &current_entry.path,
                    state_name,
                    current_value,
                ));
                if state_name == "focused" && current_value {
                    events.push(AccessibleEvent::focus(&current_entry.path));
                }
            }
        }
        if rect_changed(previous_entry.node.rect, current_entry.node.rect) {
            events.push(AccessibleEvent::bounds_changed(&current_entry.path));
        }
    }

    for previous_entry in &previous_entries {
        if !current_by_path.contains_key(previous_entry.path.as_str()) {
            events.push(children_changed_event(
                AccessibleEventKind::ObjectDestroyed,
                &previous_entry.parent_path,
                previous_entry.child_index,
                &previous_entry.path,
            ));
        }
    }
    events
}

/// Remove every interface exported for `tree`, including the root object.
/// Removal is deliberately best-effort per interface: older snapshots may not
/// have advertised Action/Text/Component, and zbus reports those as absent.
fn remove_atspi_tree(
    server: &ObjectServer,
    tree: &AccessibilityTree,
) -> Result<(), Box<dyn std::error::Error>> {
    fn remove_node(server: &ObjectServer, node: &AccessibilityNode, path: &str) {
        for (child_index, child) in node.children.iter().enumerate() {
            let child_path = accessibility_node_path(child, Some(path), child_index);
            remove_node(server, child, &child_path);
        }
        let _ = server.remove::<AtspiComponent, _>(path);
        let _ = server.remove::<AtspiText, _>(path);
        let _ = server.remove::<AtspiAction, _>(path);
        let _ = server.remove::<AtspiAccessible, _>(path);
    }

    for (index, node) in tree.nodes().iter().enumerate() {
        let path = accessibility_node_path(node, None, index);
        remove_node(server, node, &path);
    }
    let _ = server.remove::<AtspiApplication, _>(ATSPI_ROOT_PATH);
    let _ = server.remove::<AtspiAccessible, _>(ATSPI_ROOT_PATH);
    Ok(())
}

/// Export one complete tree onto an existing zbus object server.
fn export_atspi_tree(
    server: &ObjectServer,
    app_name: &str,
    bus_name: &str,
    tree: &AccessibilityTree,
) -> Result<usize, Box<dyn std::error::Error>> {
    let child_paths: Vec<String> = tree.to_atspi_paths();

    let root = AtspiAccessible {
        name: app_name.to_string(),
        description: format!("SLOPOS-I application {app_name}"),
        role: 75, // ATSPI_ROLE_APPLICATION
        role_name: "application".to_string(),
        bus_name: bus_name.to_string(),
        parent_bus: String::new(),
        parent_path: ATSPI_NULL_PATH.to_string(),
        child_paths: child_paths.clone(),
        index_in_parent: -1,
        state: state_to_atspi_bitset(&AccessibilityState::default(), AccessibilityRole::Window),
        accessible_id: "root".to_string(),
        interfaces: vec![
            ATSPI_ACCESSIBLE_IFACE.to_string(),
            ATSPI_APPLICATION_IFACE.to_string(),
        ],
    };
    server.at(ATSPI_ROOT_PATH, root)?;
    server.at(
        ATSPI_ROOT_PATH,
        AtspiApplication {
            id: 0,
            bus_address: String::new(),
        },
    )?;

    fn export_node(
        server: &ObjectServer,
        bus_name: &str,
        node: &AccessibilityNode,
        path: &str,
        parent_path: &str,
        index_in_parent: usize,
        accessible_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let child_paths: Vec<String> = node
            .children
            .iter()
            .enumerate()
            .map(|(child_index, child)| accessibility_node_path(child, Some(path), child_index))
            .collect();
        let object = AtspiAccessible {
            name: node.label.clone(),
            description: node.description.clone(),
            role: role_to_atspi_role(node.role),
            role_name: node.role_name().to_string(),
            bus_name: bus_name.to_string(),
            parent_bus: bus_name.to_string(),
            parent_path: parent_path.to_string(),
            child_paths,
            index_in_parent: i32::try_from(index_in_parent).unwrap_or(i32::MAX),
            state: state_to_atspi_bitset(&node.state, node.role),
            accessible_id: accessible_id.to_string(),
            interfaces: interfaces_for_role(node.role),
        };
        server.at(path, object)?;
        if role_has_actions(node.role) {
            server.at(path, AtspiAction::from_role(node.role, path, &node.label))?;
        }
        if role_has_text(node.role) {
            server.at(path, AtspiText::from_label(&node.label))?;
        }
        if role_has_component(node.role) {
            server.at(path, AtspiComponent::from_node(node))?;
        }
        for (child_index, child) in node.children.iter().enumerate() {
            let child_path = accessibility_node_path(child, Some(path), child_index);
            let child_id = child
                .widget_id
                .map(widget_id_accessible_id)
                .unwrap_or_else(|| format!("{accessible_id}_c{child_index}"));
            export_node(
                server,
                bus_name,
                child,
                &child_path,
                path,
                child_index,
                &child_id,
            )?;
        }
        Ok(())
    }

    for (index, node) in tree.nodes().iter().enumerate() {
        let path = accessibility_node_path(node, None, index);
        let accessible_id = node
            .widget_id
            .map(widget_id_accessible_id)
            .unwrap_or_else(|| format!("n{index}"));
        export_node(
            server,
            bus_name,
            node,
            &path,
            ATSPI_ROOT_PATH,
            index,
            &accessible_id,
        )?;
    }

    Ok(child_paths.len())
}

/// Register this process as an AT-SPI2 application exposing `tree`.
///
/// Steps:
/// 1. Connect to the session bus (skip with log if unavailable — e.g. macOS CI).
/// 2. Prefer the dedicated accessibility bus via `org.a11y.Bus.GetAddress`; fall
///    back to the session bus when the a11y bus is not running.
/// 3. Export `/org/a11y/atspi/accessible/root` with Application + Accessible.
/// 4. Export each flat tree node as `/org/a11y/atspi/accessible/{i}` with
///    Action when [`role_has_actions`].
/// 5. Best-effort `Socket.Embed` with the AT-SPI registry.
///
/// Returns `Ok(())` after a successful object export. When no D-Bus session is
/// available, returns `Ok(())` after logging a skip — never claims registration
/// in that case. Hard failures after a bus is available are returned as `Err`.
pub fn register_at_spi_app_with_tree(
    app_name: &str,
    tree: &AccessibilityTree,
) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Session bus
    let session = match Connection::session() {
        Ok(c) => c,
        Err(err) => {
            tracing::warn!(
                app = app_name,
                error = %err,
                "AT-SPI2 registration skipped: D-Bus session bus not available"
            );
            return Ok(());
        }
    };

    // 2. Prefer a11y bus; fall back to session.
    let (conn, bus_kind) = match a11y_bus_address(&session) {
        Ok(addr) => match ConnectionBuilder::address(addr.as_str())?.build() {
            Ok(a11y) => {
                tracing::debug!(app = app_name, address = %addr, "Connected to accessibility bus");
                (a11y, "a11y")
            }
            Err(err) => {
                tracing::warn!(
                    app = app_name,
                    error = %err,
                    "AT-SPI2 a11y bus connect failed; falling back to session bus"
                );
                (session, "session")
            }
        },
        Err(err) => {
            tracing::info!(
                app = app_name,
                error = %err,
                "AT-SPI2 a11y bus unavailable; exporting on session bus"
            );
            (session, "session")
        }
    };

    let bus_name = conn
        .unique_name()
        .map(|n| n.as_str().to_string())
        .unwrap_or_default();

    if bus_name.is_empty() {
        tracing::warn!(
            app = app_name,
            "AT-SPI2 registration skipped: connection has no unique bus name"
        );
        return Ok(());
    }

    // Ensure non-empty children for demo/AT visibility when caller passes an
    // empty tree. Keep the normalized clone in the registration so later
    // synchronization compares against exactly what was exported.
    let tree = normalized_accessibility_tree(app_name, tree);
    let child_count = tree.len();

    // 3–4. Export root + children. Scope the object_server borrow so `conn`
    // can be moved into the process-lifetime registration handle afterward.
    {
        let server = conn.object_server();
        export_atspi_tree(&server, app_name, &bus_name, &tree)?;
    }

    // 5. Best-effort registry Embed
    let embedded = match embed_with_registry(&conn, &bus_name) {
        Ok(()) => {
            tracing::info!(
                app = app_name,
                bus = %bus_name,
                bus_kind,
                children = child_count,
                "AT-SPI2 registered with accessibility registry (Socket.Embed)"
            );
            true
        }
        Err(err) => {
            tracing::warn!(
                app = app_name,
                bus = %bus_name,
                bus_kind,
                children = child_count,
                error = %err,
                "AT-SPI2 objects exported but registry Embed failed (registry may be absent)"
            );
            false
        }
    };

    tracing::info!(
        app = app_name,
        bus = %bus_name,
        bus_kind,
        root = ATSPI_ROOT_PATH,
        children = child_count,
        embedded,
        "AT-SPI2 Accessible tree exported (Action on actionable roles; still Orca-incomplete)"
    );

    if let Ok(mut guard) = REGISTRATION.lock() {
        *guard = Some(AtSpiRegistration {
            connection: conn,
            app_name: app_name.to_string(),
            child_count,
            bus_name,
            embedded,
            tree: tree.clone(),
        });
    }

    Ok(())
}

/// True when a process-lifetime AT-SPI connection is held (objects may be on bus).
///
/// Does not guarantee registry Embed or that ATs are listening.
pub fn at_spi_connection_available() -> bool {
    REGISTRATION
        .lock()
        .ok()
        .map(|g| g.is_some())
        .unwrap_or(false)
}

fn a11y_bus_address(session: &Connection) -> Result<String, Box<dyn std::error::Error>> {
    let reply = session.call_method(
        Some("org.a11y.Bus"),
        "/org/a11y/bus",
        Some("org.a11y.Bus"),
        "GetAddress",
        &(),
    )?;
    let address: String = reply.body().deserialize()?;
    if address.is_empty() {
        return Err("org.a11y.Bus.GetAddress returned empty address".into());
    }
    Ok(address)
}

fn embed_with_registry(
    conn: &Connection,
    bus_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Socket.Embed(IN plug (so), OUT socket (so))
    let plug: ObjectRef = object_ref(bus_name, ATSPI_ROOT_PATH)?;
    let reply = conn.call_method(
        Some("org.a11y.atspi.Registry"),
        "/org/a11y/atspi/accessible/root",
        Some("org.a11y.atspi.Socket"),
        "Embed",
        &plug,
    )?;
    // Registry returns its own (so); we don't need it beyond acknowledging success.
    let _socket: ObjectRef = reply.body().deserialize()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Unit tests (pure helpers — no D-Bus)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atspi_object_path_generation() {
        assert_eq!(atspi_object_path(0), "/org/a11y/atspi/accessible/0");
        assert_eq!(atspi_object_path(12), "/org/a11y/atspi/accessible/12");
        assert_eq!(ATSPI_ROOT_PATH, "/org/a11y/atspi/accessible/root");
    }

    #[test]
    fn atspi_object_path_with_label_sanitizes() {
        let p = atspi_object_path_with_label(0, "push button", "Save As...");
        assert_eq!(p, "/org/a11y/atspi/accessible/0/push_button_save_as");
        let empty = atspi_object_path_with_label(1, "!!!", "???");
        assert_eq!(empty, "/org/a11y/atspi/accessible/1/node_node");
    }

    #[test]
    fn sanitize_path_segment_edges() {
        assert_eq!(sanitize_path_segment(""), "node");
        assert_eq!(sanitize_path_segment("OK"), "ok");
        assert_eq!(sanitize_path_segment("Hello World"), "hello_world");
    }

    #[test]
    fn role_to_atspi_role_known_values() {
        // Core chrome + widget roles required for shell a11y.
        assert_eq!(role_to_atspi_role(AccessibilityRole::Button), 43);
        assert_eq!(role_to_atspi_role(AccessibilityRole::Window), 23);
        assert_eq!(role_to_atspi_role(AccessibilityRole::TextField), 79);
        assert_eq!(role_to_atspi_role(AccessibilityRole::MenuBar), 34);
        assert_eq!(role_to_atspi_role(AccessibilityRole::ListItem), 32);
        assert_eq!(role_to_atspi_role(AccessibilityRole::Dock), 63);
        assert_eq!(role_to_atspi_role(AccessibilityRole::Desktop), 14);
        assert_eq!(role_to_atspi_role(AccessibilityRole::Unknown), 67);
    }

    #[test]
    fn role_to_atspi_role_covers_all_roles() {
        // Fails at compile time if ALL grows without match arms; at runtime
        // ensures every role maps to a non-zero / known-ish value.
        for role in AccessibilityRole::ALL {
            let n = role_to_atspi_role(role);
            assert!(n > 0, "role {role:?} mapped to 0");
            assert!(!role.role_name().is_empty(), "empty role_name for {role:?}");
        }
        assert_eq!(AccessibilityRole::ALL.len(), 35);
    }

    #[test]
    fn role_names_for_chrome_and_core_widgets() {
        assert_eq!(AccessibilityRole::MenuBar.role_name(), "menu bar");
        assert_eq!(AccessibilityRole::Dock.role_name(), "tool bar");
        assert_eq!(AccessibilityRole::Button.role_name(), "push button");
        assert_eq!(AccessibilityRole::TextField.role_name(), "text");
        assert_eq!(AccessibilityRole::Window.role_name(), "frame");
        assert_eq!(AccessibilityRole::ListItem.role_name(), "list item");
        assert_eq!(AccessibilityRole::Desktop.role_name(), "desktop frame");
    }

    #[test]
    fn state_bitset_sets_enabled_visible() {
        let state = AccessibilityState::default();
        let bits = state_to_atspi_bitset(&state, AccessibilityRole::Button);
        // ENABLED bit 8, SENSITIVE 24, VISIBLE 30, SHOWING 25, FOCUSABLE 11
        assert_ne!(bits[0] & (1 << 8), 0);
        assert_ne!(bits[0] & (1 << 11), 0);
        assert_ne!(bits[0] & (1 << 24), 0);
        assert_ne!(bits[0] & (1 << 25), 0);
        assert_ne!(bits[0] & (1 << 30), 0);
    }

    #[test]
    fn state_bitset_chrome_targets_are_focusable() {
        let state = AccessibilityState::default();
        for role in [
            AccessibilityRole::MenuBar,
            AccessibilityRole::Desktop,
            AccessibilityRole::Dock,
        ] {
            let bits = state_to_atspi_bitset(&state, role);
            assert_ne!(
                bits[0] & (1 << 11),
                0,
                "{role:?} should set FOCUSABLE for chrome keyboard path"
            );
        }
    }

    #[test]
    fn tree_to_atspi_paths_matches_indices() {
        let mut tree = AccessibilityTree::new();
        tree.add(AccessibilityNode::new(AccessibilityRole::Button, "A"));
        tree.add(AccessibilityNode::new(AccessibilityRole::Label, "B"));
        assert_eq!(
            tree.to_atspi_paths(),
            vec![
                "/org/a11y/atspi/accessible/0".to_string(),
                "/org/a11y/atspi/accessible/1".to_string(),
            ]
        );
        assert_eq!(tree.len(), 2);
        assert!(!tree.is_empty());
        assert_eq!(tree.get(0).map(|n| n.label.as_str()), Some("A"));
    }

    #[test]
    fn accessibility_tree_signature_tracks_live_state_without_dbus() {
        let mut tree = AccessibilityTree::new();
        let mut spaces = AccessibilityNode::new(AccessibilityRole::List, "Spaces");
        let mut first = AccessibilityNode::new(AccessibilityRole::ListItem, "Personal");
        first.state.selected = true;
        spaces.children.push(first);
        tree.add(spaces);

        let before = accessibility_tree_signature(&tree);
        tree.nodes[0].children[0].state.selected = false;
        tree.nodes[0].children[0].state.focused = true;
        let after = accessibility_tree_signature(&tree);

        assert_ne!(before, after, "selection/focus changes must trigger sync");
    }

    #[test]
    fn sync_without_registration_reports_unavailable_external_bus() {
        if at_spi_registration_info().is_none() {
            let tree = default_accessibility_tree("test");
            assert!(!sync_at_spi_registered_tree(&tree).unwrap());
        }
    }

    #[test]
    fn registered_tree_sync_updates_exported_child_count() {
        struct RegistrationReset;
        impl Drop for RegistrationReset {
            fn drop(&mut self) {
                if let Ok(mut registration) = REGISTRATION.lock() {
                    *registration = None;
                }
            }
        }

        let _reset = RegistrationReset;
        let mut initial = AccessibilityTree::new();
        initial.add(AccessibilityNode::new(AccessibilityRole::List, "Spaces"));
        register_at_spi_app_with_tree("sync-test", &initial).unwrap();
        let Some(info) = at_spi_registration_info() else {
            // Headless CI without a session bus is explicitly supported; the
            // no-registration test above covers that truthful fallback.
            return;
        };
        assert_eq!(info.child_count, 1);

        let mut updated = initial.clone();
        updated.add(AccessibilityNode::new(
            AccessibilityRole::ListItem,
            "Personal",
        ));
        assert!(sync_at_spi_registered_tree(&updated).unwrap());
        assert_eq!(at_spi_registration_info().unwrap().child_count, 2);
    }

    #[test]
    fn accessibility_tree_diff_reports_live_children_state_and_bounds() {
        let mut previous = AccessibilityTree::new();
        let mut list = AccessibilityNode::new(AccessibilityRole::List, "Spaces");
        list.rect = Rect::new(10.0, 20.0, 300.0, 200.0);
        let mut personal = AccessibilityNode::new(AccessibilityRole::ListItem, "Personal");
        personal.rect = Rect::new(20.0, 30.0, 100.0, 80.0);
        list.children.push(personal);
        previous.add(list);

        let mut current = previous.clone();
        current.nodes[0].rect.width = 320.0;
        current.nodes[0].children[0].state.selected = true;
        current.nodes[0]
            .children
            .push(AccessibilityNode::new(AccessibilityRole::ListItem, "Work"));

        let events = accessibility_tree_diff_events(&previous, &current);
        assert!(events.iter().any(|event| {
            event.kind == AccessibleEventKind::BoundsChanged && event.path == atspi_object_path(0)
        }));
        assert!(events.iter().any(|event| {
            event.kind == AccessibleEventKind::StateChanged
                && event.path == format!("{}/c0", atspi_object_path(0))
                && event.any_data == "selected"
                && event.detail1 == 1
        }));
        assert!(events.iter().any(|event| {
            event.kind == AccessibleEventKind::ObjectCreated
                && event.path == atspi_object_path(0)
                && event.detail1 == 1
                && event.any_data == format!("{}/c1", atspi_object_path(0))
        }));
    }

    #[test]
    fn accessibility_tree_diff_reports_removals_and_is_quiet_without_changes() {
        let mut previous = AccessibilityTree::new();
        let mut list = AccessibilityNode::new(AccessibilityRole::List, "Spaces");
        list.children.push(AccessibilityNode::new(
            AccessibilityRole::ListItem,
            "Personal",
        ));
        list.children
            .push(AccessibilityNode::new(AccessibilityRole::ListItem, "Work"));
        previous.add(list);

        let mut current = previous.clone();
        current.nodes[0].children.pop();
        let events = accessibility_tree_diff_events(&previous, &current);
        assert!(events.iter().any(|event| {
            event.kind == AccessibleEventKind::ObjectDestroyed
                && event.path == atspi_object_path(0)
                && event.detail1 == 1
                && event.any_data == format!("{}/c1", atspi_object_path(0))
        }));
        assert!(accessibility_tree_diff_events(&current, &current).is_empty());
    }

    #[test]
    fn default_tree_is_non_empty() {
        let tree = default_accessibility_tree("Demo");
        assert_eq!(tree.len(), 1);
        assert_eq!(tree.nodes()[0].label, "Demo");
        assert_eq!(tree.nodes()[0].role, AccessibilityRole::Window);
    }

    struct TestWidget {
        state: crate::WidgetState,
        role: AccessibilityRole,
        label: String,
        children: Vec<Box<dyn Widget>>,
        explicit_children: Vec<AccessibilityNode>,
    }

    impl TestWidget {
        fn new(role: AccessibilityRole, label: &str) -> Self {
            Self {
                state: crate::WidgetState::new(),
                role,
                label: label.to_string(),
                children: Vec::new(),
                explicit_children: Vec::new(),
            }
        }
    }

    impl Widget for TestWidget {
        fn widget_state(&self) -> &crate::WidgetState {
            &self.state
        }

        fn widget_state_mut(&mut self) -> &mut crate::WidgetState {
            &mut self.state
        }

        fn layout(&mut self, constraint: crate::LayoutConstraint) -> crate::Size {
            constraint.clamp(crate::Size::new(self.rect().width, self.rect().height))
        }

        fn draw(&self, _theme: &crate::ThemeContext) {}

        fn accessibility(&self) -> Option<AccessibilityNode> {
            let mut node = AccessibilityNode::new(self.role, &self.label);
            node.children = self.explicit_children.clone();
            Some(node)
        }

        fn children(&self) -> Vec<&dyn Widget> {
            self.children
                .iter()
                .map(|child| child.as_ref() as &dyn Widget)
                .collect()
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
    }

    fn path_for_label(tree: &AccessibilityTree, label: &str) -> String {
        collect_accessibility_nodes(tree)
            .into_iter()
            .find(|entry| entry.node.label == label)
            .map(|entry| entry.path)
            .unwrap_or_else(|| panic!("missing accessibility node {label:?}"))
    }

    #[test]
    fn widget_accessibility_tree_recurses_and_enriches_live_state() {
        let mut root = TestWidget::new(AccessibilityRole::Window, "root");
        let mut group = TestWidget::new(AccessibilityRole::Group, "group");
        group.children.push(Box::new(TestWidget::new(
            AccessibilityRole::Button,
            "nested",
        )));
        root.children.push(Box::new(group));
        root.set_rect(Rect::new(10.0, 20.0, 300.0, 200.0));
        root.set_enabled(false);
        root.set_visibility(Visibility::Hidden);
        root.widget_state_mut().focused = true;

        let tree = accessibility_tree_from_widget(&root);
        assert_eq!(tree.len(), 1);
        let node = tree.get(0).expect("root node");
        assert_eq!(node.children.len(), 1);
        assert_eq!(node.children[0].children[0].label, "nested");
        assert_eq!(node.rect.x, 10.0);
        assert_eq!(node.rect.y, 20.0);
        assert_eq!(node.rect.width, 300.0);
        assert_eq!(node.rect.height, 200.0);
        assert!(!node.state.enabled);
        assert!(!node.state.visible);
        assert!(node.state.focused);
        assert_eq!(node.widget_id, Some(root.id()));
        assert!(node.children[0].widget_id.is_some());
    }

    #[test]
    fn authored_accessibility_children_are_not_duplicated_from_widgets() {
        let mut root = TestWidget::new(AccessibilityRole::Window, "Settings");
        root.explicit_children
            .push(AccessibilityNode::new(AccessibilityRole::List, "Sidebar"));
        root.children.push(Box::new(TestWidget::new(
            AccessibilityRole::Button,
            "render-only child",
        )));

        let tree = accessibility_tree_from_widget(&root);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree.get(0).unwrap().children.len(), 1);
        assert_eq!(tree.get(0).unwrap().children[0].label, "Sidebar");
    }

    #[test]
    fn widget_paths_remain_stable_when_dynamic_children_reorder_add_and_remove() {
        let mut root = TestWidget::new(AccessibilityRole::Window, "root");
        root.children
            .push(Box::new(TestWidget::new(AccessibilityRole::Button, "A")));
        root.children
            .push(Box::new(TestWidget::new(AccessibilityRole::Button, "B")));

        let first = accessibility_tree_from_widget(&root);
        let path_a = path_for_label(&first, "A");
        let path_b = path_for_label(&first, "B");
        assert!(path_a.contains("/w_widgetid_"));
        assert!(path_b.contains("/w_widgetid_"));
        assert_ne!(path_a, path_b);

        root.children.swap(0, 1);
        root.children
            .push(Box::new(TestWidget::new(AccessibilityRole::Button, "C")));
        let reordered = accessibility_tree_from_widget(&root);
        assert_eq!(path_for_label(&reordered, "A"), path_a);
        assert_eq!(path_for_label(&reordered, "B"), path_b);
        let path_c = path_for_label(&reordered, "C");
        assert_ne!(path_c, path_a);
        assert_ne!(path_c, path_b);

        root.children.remove(1);
        let removed = accessibility_tree_from_widget(&root);
        assert!(collect_accessibility_nodes(&removed)
            .iter()
            .all(|entry| entry.node.label != "A"));
        assert_eq!(path_for_label(&removed, "B"), path_b);
        assert_eq!(path_for_label(&removed, "C"), path_c);
    }

    #[test]
    fn text_field_snapshot_tracks_text_and_keeps_widget_path() {
        let mut field = crate::TextField::new();
        field.set_text("before");
        field.set_rect(Rect::new(4.0, 8.0, 120.0, 26.0));
        field.widget_state_mut().focused = true;

        let before = accessibility_tree_from_widget(&field);
        let path = before.to_atspi_paths()[0].clone();
        assert_eq!(before.get(0).unwrap().label, "before");
        assert_eq!(before.get(0).unwrap().rect.x, 4.0);
        assert_eq!(before.get(0).unwrap().rect.y, 8.0);
        assert_eq!(before.get(0).unwrap().rect.width, 120.0);
        assert_eq!(before.get(0).unwrap().rect.height, 26.0);
        assert!(before.get(0).unwrap().state.focused);

        field.set_text("after");
        let after = accessibility_tree_from_widget(&field);
        assert_eq!(after.get(0).unwrap().label, "after");
        assert_eq!(after.to_atspi_paths()[0], path);
    }

    #[test]
    fn stable_descendant_path_focuses_the_descendant() {
        let mut root = TestWidget::new(AccessibilityRole::Window, "root");
        root.children.push(Box::new(TestWidget::new(
            AccessibilityRole::Button,
            "child",
        )));
        let mut tree = accessibility_tree_from_widget(&root);
        let child_path = path_for_label(&tree, "child");
        let mut bus = AccessibilityEventBus::new();

        tree.focus_changed(&child_path, &mut bus);

        assert!(!tree.get(0).unwrap().state.focused);
        assert!(tree.get(0).unwrap().children[0].state.focused);
        assert!(bus
            .drain()
            .iter()
            .any(|event| event.kind == AccessibleEventKind::Focus && event.path == child_path));
    }

    // ----- AccessibleAction -------------------------------------------------

    #[test]
    fn accessible_action_names_are_stable() {
        // Removing or renaming these breaks AT clients — fail hard.
        assert_eq!(AccessibleAction::Activate.name(), "Activate");
        assert_eq!(AccessibleAction::Press.name(), "Press");
        assert_eq!(AccessibleAction::Focus.name(), "Focus");
        assert_eq!(AccessibleAction::ALL.len(), 3);
        for a in AccessibleAction::ALL {
            assert_eq!(AccessibleAction::from_name(a.name()), Some(a));
            assert!(!a.description().is_empty());
        }
        assert_eq!(ACTION_ACTIVATE, "Activate");
        assert_eq!(ACTION_PRESS, "Press");
        assert_eq!(ACTION_FOCUS, "Focus");
    }

    #[test]
    fn actions_for_button_include_activate_press_focus() {
        let actions = actions_for_role(AccessibilityRole::Button);
        assert!(actions.contains(&AccessibleAction::Activate));
        assert!(actions.contains(&AccessibleAction::Press));
        assert!(actions.contains(&AccessibleAction::Focus));
        assert_eq!(actions.len(), 3);
    }

    #[test]
    fn actions_for_text_field_include_focus() {
        let actions = actions_for_role(AccessibilityRole::TextField);
        assert_eq!(actions, vec![AccessibleAction::Focus]);
    }

    #[test]
    fn actions_for_list_item_include_activate_and_focus() {
        let actions = actions_for_role(AccessibilityRole::ListItem);
        assert!(actions.contains(&AccessibleAction::Activate));
        assert!(actions.contains(&AccessibleAction::Focus));
        assert!(!actions.contains(&AccessibleAction::Press));
    }

    #[test]
    fn actions_for_chrome_regions_include_focus() {
        for role in [
            AccessibilityRole::MenuBar,
            AccessibilityRole::Desktop,
            AccessibilityRole::Dock,
        ] {
            let actions = actions_for_role(role);
            assert!(
                actions.contains(&AccessibleAction::Focus),
                "{role:?} must advertise Focus for keyboard-only chrome path"
            );
        }
    }

    #[test]
    fn actions_for_static_roles_are_empty() {
        assert!(actions_for_role(AccessibilityRole::Label).is_empty());
        assert!(actions_for_role(AccessibilityRole::StaticText).is_empty());
        assert!(actions_for_role(AccessibilityRole::Window).is_empty());
        assert!(actions_for_role(AccessibilityRole::Unknown).is_empty());
    }

    #[test]
    fn pending_action_queue_push_drain() {
        // Isolate from other tests that may have left items.
        let _ = drain_pending_actions();
        assert_eq!(pending_action_count(), 0);
        push_pending_action(PendingAccessibleAction {
            path: "/org/a11y/atspi/accessible/0".into(),
            object_name: "Menu Bar".into(),
            action_index: 0,
            action_name: ACTION_FOCUS.into(),
        });
        assert_eq!(pending_action_count(), 1);
        let drained = drain_pending_actions();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].object_name, "Menu Bar");
        assert_eq!(pending_action_count(), 0);
    }

    #[test]
    fn action_invoke_handler_registry() {
        clear_action_invoke_handlers();
        assert_eq!(action_invoke_handler_count(), 0);
        assert!(!try_invoke_registered_action(ACTION_ACTIVATE, "/x"));

        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        let hits = Arc::new(AtomicUsize::new(0));
        let hits2 = hits.clone();
        register_action_invoke_handler(move |name, path| {
            if name == ACTION_ACTIVATE && path.ends_with("/2") {
                hits2.fetch_add(1, Ordering::SeqCst);
                true
            } else {
                false
            }
        });
        assert_eq!(action_invoke_handler_count(), 1);
        assert!(try_invoke_registered_action(
            ACTION_ACTIVATE,
            "/org/a11y/atspi/accessible/2"
        ));
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        assert!(!try_invoke_registered_action(
            ACTION_FOCUS,
            "/org/a11y/atspi/accessible/2"
        ));
        clear_action_invoke_handlers();
        assert_eq!(action_invoke_handler_count(), 0);
    }

    #[test]
    fn interfaces_for_role_advertise_action_when_present() {
        let button = interfaces_for_role(AccessibilityRole::Button);
        assert!(button.iter().any(|s| s == ATSPI_ACCESSIBLE_IFACE));
        assert!(button.iter().any(|s| s == ATSPI_ACTION_IFACE));
        assert!(button.iter().any(|s| s == ATSPI_COMPONENT_IFACE));

        let label = interfaces_for_role(AccessibilityRole::Label);
        assert!(label.iter().any(|s| s == ATSPI_ACCESSIBLE_IFACE));
        assert!(!label.iter().any(|s| s == ATSPI_ACTION_IFACE));
        assert!(label.iter().any(|s| s == ATSPI_TEXT_IFACE));

        let entry = interfaces_for_role(AccessibilityRole::TextField);
        assert!(entry.iter().any(|s| s == ATSPI_TEXT_IFACE));
    }

    #[test]
    fn accessible_text_state_caret_and_selection() {
        let mut t = AccessibleTextState::new("hello world");
        assert_eq!(t.character_count(), 11);
        t.set_selection(0, 5);
        assert_eq!(t.selected_text(), "hello");
        t.set_caret(6);
        assert_eq!(t.caret_offset, 6);
        assert_eq!(t.get_text(6, 11), "world");
    }

    #[test]
    fn accessible_component_contains_and_extents() {
        let c = AccessibleComponent {
            x: 10,
            y: 20,
            width: 100,
            height: 50,
        };
        assert!(c.contains_point(10, 20));
        assert!(c.contains_point(109, 69));
        assert!(!c.contains_point(110, 20));
        assert_eq!(c.extents(), (10, 20, 100, 50));
    }

    #[test]
    fn accessible_selection_single_and_multi() {
        let mut s = AccessibleSelectionState {
            multi: false,
            ..Default::default()
        };
        s.select(2);
        s.select(5);
        assert_eq!(s.selected, vec![5]);
        s.multi = true;
        s.select(2);
        assert_eq!(s.n_selected(), 2);
        s.deselect(5);
        assert!(s.is_selected(2));
    }

    #[test]
    fn shell_chrome_includes_session_menu_items() {
        let tree = shell_chrome_accessibility_tree("SLOPOS-I");
        // menu bar is first node; Retro submenu has Lock Screen
        let menu_bar = &tree.nodes[0];
        let retro = menu_bar
            .children
            .iter()
            .find(|n| n.label == "SLOPOS")
            .expect("Retro menu");
        assert!(retro.children.iter().any(|c| c.label == "Lock Screen"));
        assert!(tree
            .nodes
            .iter()
            .any(|n| n.role == AccessibilityRole::Notification));
    }

    #[test]
    fn node_actions_delegate_to_role() {
        let node = AccessibilityNode::new(AccessibilityRole::Button, "OK");
        assert_eq!(node.actions(), actions_for_role(AccessibilityRole::Button));
    }

    // ----- Chrome keyboard policy ------------------------------------------

    #[test]
    fn chrome_focus_region_order_is_menu_desktop_dock() {
        assert_eq!(
            ChromeFocusRegion::ORDER,
            [
                ChromeFocusRegion::MenuBar,
                ChromeFocusRegion::DesktopIcons,
                ChromeFocusRegion::Dock,
            ]
        );
        assert_eq!(
            ChromeFocusRegion::MenuBar.next(),
            ChromeFocusRegion::DesktopIcons
        );
        assert_eq!(
            ChromeFocusRegion::DesktopIcons.next(),
            ChromeFocusRegion::Dock
        );
        assert_eq!(ChromeFocusRegion::Dock.next(), ChromeFocusRegion::MenuBar);
        assert_eq!(ChromeFocusRegion::MenuBar.prev(), ChromeFocusRegion::Dock);
    }

    #[test]
    fn next_chrome_focus_region_starts_at_menu_bar() {
        assert_eq!(next_chrome_focus_region(None), ChromeFocusRegion::MenuBar);
        assert_eq!(
            next_chrome_focus_region(Some(ChromeFocusRegion::MenuBar)),
            ChromeFocusRegion::DesktopIcons
        );
        assert_eq!(prev_chrome_focus_region(None), ChromeFocusRegion::Dock);
    }

    #[test]
    fn chrome_region_primary_roles() {
        assert_eq!(
            ChromeFocusRegion::MenuBar.primary_role(),
            AccessibilityRole::MenuBar
        );
        assert_eq!(
            ChromeFocusRegion::DesktopIcons.primary_role(),
            AccessibilityRole::Desktop
        );
        assert_eq!(
            ChromeFocusRegion::Dock.primary_role(),
            AccessibilityRole::Dock
        );
    }

    #[test]
    fn shell_chrome_tree_has_menu_desktop_dock_window() {
        let tree = shell_chrome_accessibility_tree("SLOPOS-I");
        assert!(tree.len() >= 4);
        assert_eq!(tree.nodes()[0].role, AccessibilityRole::MenuBar);
        assert_eq!(tree.nodes()[1].role, AccessibilityRole::Desktop);
        assert_eq!(tree.nodes()[2].role, AccessibilityRole::Dock);
        assert_eq!(tree.nodes()[3].role, AccessibilityRole::Window);
        assert_eq!(tree.nodes()[3].label, "SLOPOS-I");

        // Nested structure for AT depth
        assert!(!tree.nodes()[0].children.is_empty());
        assert!(tree.nodes()[1]
            .children
            .iter()
            .all(|c| c.role == AccessibilityRole::ListItem));
        assert!(tree.nodes()[2]
            .children
            .iter()
            .all(|c| c.role == AccessibilityRole::Button));
    }

    #[test]
    fn chrome_focus_indices_follow_cycle_order() {
        let tree = shell_chrome_accessibility_tree("SLOPOS-I");
        let indices = chrome_focus_indices(&tree);
        assert_eq!(indices.len(), 3);
        assert_eq!(indices[0].0, ChromeFocusRegion::MenuBar);
        assert_eq!(indices[1].0, ChromeFocusRegion::DesktopIcons);
        assert_eq!(indices[2].0, ChromeFocusRegion::Dock);
        assert_eq!(indices[0].1, 0);
        assert_eq!(indices[1].1, 1);
        assert_eq!(indices[2].1, 2);
    }

    #[test]
    fn next_chrome_focus_index_cycles() {
        let tree = shell_chrome_accessibility_tree("SLOPOS-I");
        assert_eq!(next_chrome_focus_index(&tree, None), Some(0));
        assert_eq!(next_chrome_focus_index(&tree, Some(0)), Some(1));
        assert_eq!(next_chrome_focus_index(&tree, Some(1)), Some(2));
        assert_eq!(next_chrome_focus_index(&tree, Some(2)), Some(0));
        // Unknown current → start of cycle
        assert_eq!(next_chrome_focus_index(&tree, Some(99)), Some(0));
    }

    #[test]
    fn next_chrome_focus_index_empty_tree() {
        let tree = AccessibilityTree::new();
        assert_eq!(next_chrome_focus_index(&tree, None), None);
    }

    #[test]
    fn focusable_indices_includes_chrome_and_interactive() {
        let mut tree = AccessibilityTree::new();
        tree.add(AccessibilityNode::new(AccessibilityRole::MenuBar, "MB"));
        tree.add(AccessibilityNode::new(AccessibilityRole::Label, "L"));
        tree.add(AccessibilityNode::new(AccessibilityRole::Button, "B"));
        let idx = focusable_indices(&tree);
        assert!(idx.contains(&0)); // chrome MenuBar
        assert!(!idx.contains(&1)); // Label
        assert!(idx.contains(&2)); // Button
    }

    // ----- In-process event bus / focus path hooks -------------------------

    #[test]
    fn accessible_event_kind_tags_are_stable() {
        assert_eq!(AccessibleEventKind::Focus.as_str(), "Focus");
        assert_eq!(AccessibleEventKind::StateChanged.as_str(), "StateChanged");
        assert_eq!(AccessibleEventKind::BoundsChanged.as_str(), "BoundsChanged");
        assert_eq!(
            AccessibleEventKind::ActiveDescendantChanged.as_str(),
            "ActiveDescendantChanged"
        );
        assert_eq!(AccessibleEventKind::ObjectCreated.as_str(), "ObjectCreated");
        assert_eq!(
            AccessibleEventKind::ObjectDestroyed.as_str(),
            "ObjectDestroyed"
        );
    }

    // ----- Pure D-Bus serialize path ---------------------------------------

    #[test]
    fn serialize_focus_event_for_dbus() {
        let path = atspi_object_path(0);
        let event = AccessibleEvent::focus(&path);
        let s = serialize_event_for_dbus(&event);
        assert_eq!(s.interface, ATSPI_EVENT_FOCUS_IFACE);
        assert_eq!(s.member, "Focus");
        assert_eq!(s.path, path);
        assert_eq!(s.detail, "");
        assert_eq!(s.detail1, 1);
        assert_eq!(s.detail2, 0);
        assert_eq!(s.any_data, "");
    }

    #[test]
    fn serialize_state_changed_puts_state_name_in_detail() {
        let path = atspi_object_path(1);
        let event = AccessibleEvent::state_changed(&path, "focused", true);
        let s = serialize_event_for_dbus(&event);
        assert_eq!(s.interface, ATSPI_EVENT_OBJECT_IFACE);
        assert_eq!(s.member, "StateChanged");
        assert_eq!(s.path, path);
        assert_eq!(s.detail, "focused");
        assert_eq!(s.detail1, 1);
        assert_eq!(s.detail2, 0);
        assert_eq!(s.any_data, "");
    }

    #[test]
    fn serialize_bounds_and_active_descendant() {
        let bounds = serialize_event_for_dbus(&AccessibleEvent::bounds_changed("/p"));
        assert_eq!(bounds.interface, ATSPI_EVENT_OBJECT_IFACE);
        assert_eq!(bounds.member, "BoundsChanged");
        assert_eq!(bounds.path, "/p");

        let ad = serialize_event_for_dbus(&AccessibleEvent::active_descendant_changed(
            "/parent", "/child",
        ));
        assert_eq!(ad.member, "ActiveDescendantChanged");
        assert_eq!(ad.path, "/parent");
        assert_eq!(ad.any_data, "/child");
    }

    #[test]
    fn serialize_object_lifecycle_as_children_changed() {
        let created = serialize_event_for_dbus(&AccessibleEvent::object_created("/n"));
        assert_eq!(created.interface, ATSPI_EVENT_OBJECT_IFACE);
        assert_eq!(created.member, "ChildrenChanged");
        assert_eq!(created.detail, "add");

        let destroyed = serialize_event_for_dbus(&AccessibleEvent::object_destroyed("/n"));
        assert_eq!(destroyed.member, "ChildrenChanged");
        assert_eq!(destroyed.detail, "remove");
    }

    #[test]
    fn try_emit_without_registration_returns_false() {
        // Pure host/CI path: no AT-SPI registration held → no D-Bus emit.
        // (If some other test registered, availability may be true; still must
        // not panic. When unavailable, emit is definitively false.)
        if !at_spi_connection_available() {
            let event = AccessibleEvent::focus(atspi_object_path(0));
            assert!(!try_emit_atspi_dbus_event(&event));
        }
    }

    #[test]
    fn event_bus_push_pop_drain_fifo() {
        let mut bus = AccessibilityEventBus::new();
        assert!(bus.is_empty());
        bus.push(AccessibleEvent::object_created(
            "/org/a11y/atspi/accessible/0",
        ));
        bus.push(AccessibleEvent::object_destroyed(
            "/org/a11y/atspi/accessible/1",
        ));
        assert_eq!(bus.len(), 2);

        let first = bus.pop().expect("first event");
        assert_eq!(first.kind, AccessibleEventKind::ObjectCreated);
        assert_eq!(first.path, "/org/a11y/atspi/accessible/0");

        let rest = bus.drain();
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].kind, AccessibleEventKind::ObjectDestroyed);
        assert!(bus.is_empty());
        assert!(bus.pop().is_none());
    }

    #[test]
    fn free_focus_changed_helper_pushes_focus_event() {
        let mut bus = EventQueue::new();
        let path = atspi_object_path(0);
        focus_changed(&mut bus, &path);
        let events = bus.drain();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, AccessibleEventKind::Focus);
        assert_eq!(events[0].path, path);
        assert_eq!(events[0].detail1, 1);
        assert_eq!(events[0].detail2, 0);
        assert!(events[0].any_data.is_empty());
    }

    #[test]
    fn bus_focus_changed_emits_on_focus_path_helper() {
        let mut bus = AccessibilityEventBus::new();
        let path = atspi_object_path(2);
        bus.focus_changed(&path);
        let e = bus.pop().expect("Focus event");
        assert_eq!(e, AccessibleEvent::focus(path));
    }

    #[test]
    fn tree_focus_changed_pushes_focus_and_updates_state() {
        let mut tree = AccessibilityTree::new();
        tree.add(AccessibilityNode::new(AccessibilityRole::Button, "A"));
        tree.add(AccessibilityNode::new(AccessibilityRole::Button, "B"));
        let mut bus = AccessibilityEventBus::new();

        let path0 = atspi_object_path(0);
        tree.focus_changed(&path0, &mut bus);

        assert!(tree.get(0).unwrap().state.focused);
        assert!(!tree.get(1).unwrap().state.focused);

        let events = bus.drain();
        // StateChanged(focused=true) for index 0 + Focus event
        assert!(
            events
                .iter()
                .any(|e| e.kind == AccessibleEventKind::Focus && e.path == path0),
            "expected Focus event for {path0}, got {events:?}"
        );
        assert!(
            events.iter().any(|e| {
                e.kind == AccessibleEventKind::StateChanged
                    && e.path == path0
                    && e.any_data == "focused"
                    && e.detail1 == 1
            }),
            "expected StateChanged focused=1 for path0, got {events:?}"
        );

        // Move focus to index 1 — previous loses focused, new gains it + Focus.
        let path1 = atspi_object_path(1);
        tree.focus_changed(&path1, &mut bus);
        assert!(!tree.get(0).unwrap().state.focused);
        assert!(tree.get(1).unwrap().state.focused);

        let events = bus.drain();
        assert!(events
            .iter()
            .any(|e| e.kind == AccessibleEventKind::Focus && e.path == path1));
        assert!(events.iter().any(|e| {
            e.kind == AccessibleEventKind::StateChanged
                && e.path == path0
                && e.any_data == "focused"
                && e.detail1 == 0
        }));
        assert!(events.iter().any(|e| {
            e.kind == AccessibleEventKind::StateChanged
                && e.path == path1
                && e.any_data == "focused"
                && e.detail1 == 1
        }));
    }

    #[test]
    fn tree_focus_changed_index_uses_canonical_path() {
        let mut tree = shell_chrome_accessibility_tree("SLOPOS-I");
        let mut bus = AccessibilityEventBus::new();
        // Menu bar is index 0 in shell chrome tree.
        tree.focus_changed_index(0, &mut bus);
        assert!(tree.get(0).unwrap().state.focused);
        let events = bus.drain();
        assert!(events
            .iter()
            .any(|e| { e.kind == AccessibleEventKind::Focus && e.path == atspi_object_path(0) }));
    }

    #[test]
    fn chrome_focus_path_emits_focus_events_for_cycle() {
        // Keyboard chrome path: next_chrome_focus_index + tree focus hooks.
        let mut tree = shell_chrome_accessibility_tree("SLOPOS-I");
        let mut bus = AccessibilityEventBus::new();
        let mut current: Option<usize> = None;
        let mut focused_paths = Vec::new();

        for _ in 0..3 {
            let idx = next_chrome_focus_index(&tree, current).expect("chrome index");
            tree.focus_changed_index(idx, &mut bus);
            focused_paths.push(atspi_object_path(idx));
            current = Some(idx);
        }

        let events = bus.drain();
        let focus_paths: Vec<&str> = events
            .iter()
            .filter(|e| e.kind == AccessibleEventKind::Focus)
            .map(|e| e.path.as_str())
            .collect();
        assert_eq!(
            focus_paths,
            vec![
                focused_paths[0].as_str(),
                focused_paths[1].as_str(),
                focused_paths[2].as_str(),
            ]
        );
        // Cycle order: menu bar (0) → desktop (1) → dock (2)
        assert_eq!(focused_paths[0], atspi_object_path(0));
        assert_eq!(focused_paths[1], atspi_object_path(1));
        assert_eq!(focused_paths[2], atspi_object_path(2));
        assert!(tree.get(2).unwrap().state.focused);
        assert!(!tree.get(0).unwrap().state.focused);
    }

    #[test]
    fn flat_index_from_atspi_path_parses_canonical_and_nested() {
        assert_eq!(
            flat_index_from_atspi_path("/org/a11y/atspi/accessible/3"),
            Some(3)
        );
        assert_eq!(
            flat_index_from_atspi_path("/org/a11y/atspi/accessible/3/c0"),
            Some(3)
        );
        assert_eq!(
            flat_index_from_atspi_path("/org/a11y/atspi/accessible/root"),
            None
        );
        assert_eq!(flat_index_from_atspi_path("/other"), None);
    }

    #[test]
    fn other_event_helpers_push_expected_kinds() {
        let mut bus = AccessibilityEventBus::new();
        bus.bounds_changed("/p");
        bus.active_descendant_changed("/parent", "/child");
        bus.object_created("/new");
        bus.object_destroyed("/old");
        let kinds: Vec<_> = bus.drain().into_iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            vec![
                AccessibleEventKind::BoundsChanged,
                AccessibleEventKind::ActiveDescendantChanged,
                AccessibleEventKind::ObjectCreated,
                AccessibleEventKind::ObjectDestroyed,
            ]
        );
    }
}
