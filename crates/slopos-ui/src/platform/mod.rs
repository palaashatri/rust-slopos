//! Window hosting. An [`App`] receives [`Event`]s and paints into a
//! [`RasterSurface`]; the platform host owns the native window.

mod keys;
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

    /// The accessibility tree for screen readers, if the app provides one.
    fn accessibility(&self) -> Option<accesskit::TreeUpdate> {
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

    /// The integer scale used for rendering, available after the first event.
    pub fn scale(&self) -> Option<Scale> {
        self.scale
    }
}

/// Integer UI scale from `SLOPOS_SCALE`, then `GDK_SCALE`, else 1.
pub fn scale_from_environment() -> Scale {
    ["SLOPOS_SCALE", "GDK_SCALE"]
        .iter()
        .find_map(|name| std::env::var(name).ok()?.trim().parse::<u32>().ok())
        .filter(|factor| (1..=4).contains(factor))
        .and_then(Scale::new)
        .unwrap_or_else(|| Scale::new(1).expect("1 is a valid scale"))
}
