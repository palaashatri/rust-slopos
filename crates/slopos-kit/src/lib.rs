pub mod accessibility;
pub mod button;
pub mod clipboard;
pub mod design_tokens;
pub mod dialog;
pub mod dispatch;
pub mod dnd;
pub mod dock_view;
pub mod event;
pub mod focus;
pub mod icon_view;
pub mod image_view;
pub mod label;
pub mod layout;
pub mod list_view;
pub mod menu;
pub mod menu_bar;
pub mod monospace_view;
pub mod panel;
pub mod popup_button;
pub mod progress_bar;
pub mod scroll_view;
pub mod slider;
pub mod split_view;
pub mod status_bar;
pub mod tab_view;
pub mod text_field;
pub mod theme;
pub mod toolbar;
pub mod tree_view;
pub mod widget;
pub mod window;
pub mod workspace_grid_view;

pub use accessibility::{
    accessibility_tree_from_widget, action_invoke_handler_count, actions_for_role,
    at_spi_connection_available, at_spi_registration_info, atspi_object_path,
    atspi_object_path_with_label, chrome_focus_indices, clear_action_invoke_handlers,
    default_accessibility_tree, drain_pending_actions, flat_index_from_atspi_path, focus_changed,
    focusable_indices, interfaces_for_role, next_chrome_focus_index, next_chrome_focus_region,
    pending_action_count, prev_chrome_focus_region, push_pending_action,
    register_action_invoke_handler, register_at_spi_app, register_at_spi_app_with_tree,
    register_at_spi_shell_chrome, role_has_actions, role_to_atspi_role, sanitize_path_segment,
    serialize_event_for_dbus, shell_chrome_accessibility_tree, state_to_atspi_bitset,
    sync_at_spi_registered_tree, try_emit_atspi_dbus_event, try_invoke_registered_action,
    AccessibilityEventBus, AccessibilityNode, AccessibilityRole, AccessibilityState,
    AccessibilityTree, AccessibleAction, AccessibleEvent, AccessibleEventKind, AccessibleTextState,
    ActionInvokeHandler, AtSpiRegistrationInfo, ChromeFocusRegion, EventQueue,
    PendingAccessibleAction, SerializedAtspiEvent, ACTION_ACTIVATE, ACTION_FOCUS, ACTION_PRESS,
    ATSPI_ACCESSIBLE_IFACE, ATSPI_ACCESSIBLE_PREFIX, ATSPI_ACTION_IFACE, ATSPI_APPLICATION_IFACE,
    ATSPI_EVENT_FOCUS_IFACE, ATSPI_EVENT_OBJECT_IFACE, ATSPI_NULL_PATH, ATSPI_ROOT_PATH,
};
pub use button::Button;
pub use clipboard::Clipboard;
pub use design_tokens::*;
pub use dialog::Dialog;
pub use dispatch::{
    deliver_to, dispatch_pointer, dispatch_positional, for_each_widget_mut, hit_test, widget_at,
    widget_by_id, PointerDispatcher,
};
pub use dnd::{DragData, DragSession, DragSource, DropTarget};
pub use dock_view::{DockView, DockViewItem};
pub use event::{Event, EventHandler, EventResult};
pub use focus::FocusManager;
pub use icon_view::IconView;
pub use image_view::ImageView;
pub use label::Label;
pub use layout::{Layout, LayoutConstraint, LayoutHints, LayoutView};
pub use list_view::ListView;
pub use menu::{Menu, MenuItem};
pub use menu_bar::MenuBar;
pub use monospace_view::{MonospaceCell, MonospaceView};
pub use panel::Panel;
pub use popup_button::PopupButton;
pub use progress_bar::ProgressBar;
pub use scroll_view::ScrollView;
pub use slider::Slider;
pub use slopos_render::Color;
pub use split_view::SplitView;
pub use status_bar::{StatusBar, StatusBarAlignment, StatusBarItem};
pub use tab_view::{Tab, TabView};
pub use text_field::TextField;
pub use theme::{ThemeContext, ThemeToken, ThemeValue};
pub use toolbar::Toolbar;
pub use tree_view::TreeView;
pub use widget::{Widget, WidgetId, WidgetState};
pub use window::{
    hit_test_window_chrome, hit_test_window_chrome_with_metrics, Window, WindowChromeHit,
};
pub use workspace_grid_view::WorkspaceGridView;

pub type Result<T> = std::result::Result<T, KitError>;

#[derive(Debug)]
pub enum KitError {
    WidgetNotFound(WidgetId),
    Layout(String),
    Theme(String),
}

impl std::fmt::Display for KitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KitError::WidgetNotFound(id) => write!(f, "widget not found: {}", id),
            KitError::Layout(msg) => write!(f, "layout error: {}", msg),
            KitError::Theme(msg) => write!(f, "theme error: {}", msg),
        }
    }
}

impl std::error::Error for KitError {}

#[derive(Debug, Clone, Copy)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const ZERO: Point = Point { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const ZERO: Size = Size {
        width: 0.0,
        height: 0.0,
    };

    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const ZERO: Rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
    };

    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn contains(&self, point: Point) -> bool {
        point.x >= self.x
            && point.x <= self.x + self.width
            && point.y >= self.y
            && point.y <= self.y + self.height
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorStyle {
    Default,
    Pointer,
    Text,
    Crosshair,
    Move,
    NotAllowed,
    ResizeHorizontal,
    ResizeVertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Visible,
    Hidden,
    Collapsed,
}
