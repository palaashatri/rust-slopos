//! Window hosting. An [`App`] receives [`Event`]s and paints into a
//! [`RasterSurface`]; the platform host owns the native window.

pub mod global_menu;
mod keys;
#[cfg(feature = "global-menu")]
pub mod menu_import;
pub mod x11;

use std::time::Duration;

use crate::event::Event;
use crate::foundation::geometry::{Rect, Scale};
use crate::render::RasterSurface;

pub use x11::{run, PlatformError, Proxy};

/// What kind of top-level window to create.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowKind {
    /// An ordinary application window managed by the window manager.
    Normal,
    /// A desktop panel docked to an edge; reserves `strut` logical pixels.
    Dock { edge: Edge, strut: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Top,
    Bottom,
}

#[derive(Debug, Clone)]
pub struct WindowOptions {
    pub title: String,
    /// `WM_CLASS` class name, e.g. "slopos-calculator".
    pub app_id: String,
    pub width: u32,
    pub height: u32,
    pub kind: WindowKind,
    pub resizable: bool,
    /// Logical position for docks and fixed-position windows.
    pub position: Option<(i32, i32)>,
    /// Integer UI scale; `None` reads `SLOPOS_SCALE` / `GDK_SCALE`.
    pub scale: Option<Scale>,
    /// Desktop-wide shortcuts (X keysym and modifiers) this window owns,
    /// such as Super+Space for the launcher or the media keys. Each press
    /// arrives as [`Event::GlobalKey`]. A shortcut another client already
    /// grabbed is skipped and logged.
    pub global_keys: Vec<(u32, crate::event::Modifiers)>,
}

impl WindowOptions {
    pub fn new(title: &str, app_id: &str, width: u32, height: u32) -> Self {
        Self {
            title: title.to_owned(),
            app_id: app_id.to_owned(),
            width,
            height,
            kind: WindowKind::Normal,
            resizable: true,
            position: None,
            scale: None,
            global_keys: Vec::new(),
        }
    }
}

/// A SLOPOS application.
pub trait App {
    /// Handle one event. Use `control` to ask for a redraw, start timers,
    /// change the window or quit.
    fn event(&mut self, event: &Event, control: &mut Control);

    /// Paint the whole window. The surface matches the current logical size.
    fn paint(&mut self, surface: &RasterSurface);

    /// Paint popup `id` (opened with [`Control::open_popup`]); the surface
    /// matches the popup's logical size.
    fn paint_popup(&mut self, id: u64, surface: &RasterSurface) {
        let _ = (id, surface);
    }

    /// The accessibility tree for screen readers, if the app provides one.
    fn accessibility(&self) -> Option<accesskit::TreeUpdate> {
        None
    }

    /// The app's menus. The host exports them to the desktop's global menu
    /// bar when one is running (see [`global_menu`]) and keeps the export in
    /// step with enabled/checked changes after every event.
    fn menu_bar(&self) -> Option<&crate::controls::menu::MenuBar> {
        None
    }
}

/// Requests an [`App`] makes of its window during one event.
#[derive(Debug, Default)]
pub struct Control {
    pub(crate) redraw: bool,
    pub(crate) quit: bool,
    pub(crate) timers: Vec<(Duration, u64)>,
    pub(crate) title: Option<String>,
    pub(crate) geometry: Option<Rect>,
    pub(crate) accessibility_changed: bool,
    pub(crate) scale: Option<Scale>,
    pub(crate) clipboard: Option<String>,
    pub(crate) paste: bool,
    pub(crate) modifiers: crate::event::Modifiers,
    pub(crate) popups: Vec<PopupRequest>,
}

/// A popup change asked for during one event, applied by the host in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupRequest {
    /// Show popup `id` at `rect` (logical, relative to the app window's
    /// top-left; it may extend past the window and is kept on screen), or
    /// move/resize it if it is already open. `grab` takes the pointer and
    /// keyboard, as a menu does, until the popup closes.
    Open {
        id: u64,
        rect: Rect,
        grab: bool,
    },
    Close {
        id: u64,
    },
}

impl Control {
    pub fn request_redraw(&mut self) {
        self.redraw = true;
        self.accessibility_changed = true;
    }

    pub fn quit(&mut self) {
        self.quit = true;
    }

    /// Fire [`Event::Timer`]`(id)` once after `delay`.
    pub fn start_timer(&mut self, delay: Duration, id: u64) {
        self.timers.push((delay, id));
    }

    pub fn set_title(&mut self, title: &str) {
        self.title = Some(title.to_owned());
    }

    /// Move and resize the window (logical pixels).
    pub fn set_geometry(&mut self, rect: Rect) {
        self.geometry = Some(rect);
    }

    /// Offer `text` on the system clipboard (the X11 `CLIPBOARD` selection).
    /// Text above [`MAX_CLIPBOARD_BYTES`] is truncated at a char boundary.
    pub fn set_clipboard(&mut self, text: &str) {
        let mut end = text.len().min(MAX_CLIPBOARD_BYTES);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        self.clipboard = Some(text[..end].to_owned());
    }

    /// Ask for the clipboard text; it arrives as [`Event::Paste`]. Nothing
    /// arrives when the clipboard is empty or holds no text.
    pub fn request_paste(&mut self) {
        self.paste = true;
    }

    /// Text handed to [`Self::set_clipboard`] during this event (for tests).
    pub fn pending_clipboard(&self) -> Option<&str> {
        self.clipboard.as_deref()
    }

    /// Whether [`Self::request_paste`] was called during this event.
    pub fn paste_requested(&self) -> bool {
        self.paste
    }

    /// Modifier keys held during the current input event. Pointer events
    /// carry no modifiers of their own; controls such as lists use this for
    /// Ctrl/Shift-click.
    pub fn modifiers(&self) -> crate::event::Modifiers {
        self.modifiers
    }

    /// Set the modifiers [`Self::modifiers`] reports (hosts and tests).
    pub fn set_modifiers(&mut self, modifiers: crate::event::Modifiers) {
        self.modifiers = modifiers;
    }

    /// Show (or move) popup `id`: menus, popovers and toasts get their own
    /// window, so they can extend outside a small parent such as a top bar.
    /// Paint it in [`App::paint_popup`]; its input arrives as
    /// [`Event::Popup`].
    pub fn open_popup(&mut self, id: u64, rect: Rect, grab: bool) {
        self.popups.push(PopupRequest::Open { id, rect, grab });
        self.redraw = true;
    }

    pub fn close_popup(&mut self, id: u64) {
        self.popups.push(PopupRequest::Close { id });
    }

    /// Popup changes asked for so far (for tests).
    pub fn popup_requests(&self) -> &[PopupRequest] {
        &self.popups
    }

    /// Whether the app asked for a redraw (for tests).
    pub fn redraw_requested(&self) -> bool {
        self.redraw
    }

    /// Whether the app asked to quit (for tests).
    pub fn quit_requested(&self) -> bool {
        self.quit
    }

    /// The integer scale used for rendering, available after the first event.
    pub fn scale(&self) -> Option<Scale> {
        self.scale
    }
}

/// Where a popup lands on screen, in device pixels: `rect` (logical, relative
/// to the parent window at device `origin`) scaled by `factor`, then shifted
/// so it stays inside a `screen`-sized display wherever possible.
pub fn place_popup(origin: (i32, i32), rect: Rect, factor: u32, screen: (u32, u32)) -> Rect {
    let factor_i = factor as i32;
    let width = (rect.width * factor).clamp(1, screen.0.max(1));
    let height = (rect.height * factor).clamp(1, screen.1.max(1));
    let x = origin.0 + rect.x * factor_i;
    let y = origin.1 + rect.y * factor_i;
    let max_x = screen.0 as i32 - width as i32;
    let max_y = screen.1 as i32 - height as i32;
    Rect {
        x: x.clamp(0, max_x.max(0)),
        y: y.clamp(0, max_y.max(0)),
        width,
        height,
    }
}

/// The largest clipboard text SLOPOS offers in one X11 property (no INCR).
pub const MAX_CLIPBOARD_BYTES: usize = 256 * 1024;

/// Integer UI scale from `SLOPOS_SCALE`, then `GDK_SCALE`, else 1.
pub fn scale_from_environment() -> Scale {
    ["SLOPOS_SCALE", "GDK_SCALE"]
        .iter()
        .find_map(|name| std::env::var(name).ok()?.trim().parse::<u32>().ok())
        .filter(|factor| (1..=4).contains(factor))
        .and_then(Scale::new)
        .unwrap_or_else(|| Scale::new(1).expect("1 is a valid scale"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popups_are_scaled_and_kept_on_screen() {
        let rect = |x, y, width, height| Rect {
            x,
            y,
            width,
            height,
        };
        // A menu under a top-bar title, at 1x and 2x.
        assert_eq!(
            place_popup((0, 0), rect(60, 26, 180, 120), 1, (1280, 800)),
            rect(60, 26, 180, 120)
        );
        assert_eq!(
            place_popup((0, 0), rect(60, 26, 180, 120), 2, (2560, 1600)),
            rect(120, 52, 360, 240)
        );
        // Past the right and bottom edges it slides back on screen.
        assert_eq!(
            place_popup((1200, 700), rect(50, 50, 200, 200), 1, (1280, 800)),
            rect(1080, 600, 200, 200)
        );
        // Relative to a window that is not at the origin; negative clamps to 0.
        assert_eq!(
            place_popup((100, 100), rect(-150, -150, 10, 10), 1, (1280, 800)),
            rect(0, 0, 10, 10)
        );
        // Larger than the screen: clipped to it.
        assert_eq!(
            place_popup((0, 0), rect(0, 0, 5000, 10), 1, (1280, 800)).width,
            1280
        );
    }

    #[test]
    fn popup_requests_are_recorded_in_order() {
        let mut control = Control::default();
        let r = Rect {
            x: 1,
            y: 2,
            width: 3,
            height: 4,
        };
        control.open_popup(7, r, true);
        control.close_popup(7);
        assert_eq!(
            control.popup_requests(),
            [
                PopupRequest::Open {
                    id: 7,
                    rect: r,
                    grab: true
                },
                PopupRequest::Close { id: 7 }
            ]
        );
        assert!(control.redraw_requested());
    }
}
