//! Vertical scrolling: offset bookkeeping plus a Classic scrollbar.
//!
//! [`ScrollState`] keeps a content offset inside `0..=content - viewport`.
//! It takes wheel events over its area, track clicks (page up/down) and
//! thumb drags. Lists and other scrolling views own one and draw their
//! content shifted by [`ScrollState::offset`].

use crate::event::{Event, PointerButton};
use crate::foundation::geometry::Rect;
use crate::render::{RasterSurface, RenderError};
use crate::theme::palette;

/// Scrollbar width in logical pixels.
pub const SCROLLBAR_WIDTH: u32 = 15;
/// Smallest thumb so it stays grabbable for very long content.
const MIN_THUMB: u32 = 16;
/// Pixels moved per wheel step.
pub const WHEEL_STEP: i64 = 48;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollOutcome {
    Ignored,
    /// The offset (or thumb press state) changed; repaint.
    Scrolled,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScrollState {
    /// The whole scrolling area (content and bar); wheel events over it count.
    pub area: Rect,
    content: u64,
    offset: u64,
    drag: Option<(i32, u64)>,
    hovered: bool,
}

impl ScrollState {
    pub fn new(area: Rect) -> Self {
        Self {
            area,
            ..Self::default()
        }
    }

    pub fn offset(&self) -> u64 {
        self.offset
    }

    pub fn content(&self) -> u64 {
        self.content
    }

    pub fn viewport(&self) -> u64 {
        u64::from(self.area.height)
    }

    fn max_offset(&self) -> u64 {
        self.content.saturating_sub(self.viewport())
    }

    /// Whether the content is taller than the viewport (the bar is active).
    pub fn scrollable(&self) -> bool {
        self.content > self.viewport()
    }

    /// Set the content height, keeping the offset in range.
    pub fn set_content(&mut self, content: u64) {
        self.content = content;
        self.offset = self.offset.min(self.max_offset());
    }

    pub fn set_area(&mut self, area: Rect) {
        self.area = area;
        self.offset = self.offset.min(self.max_offset());
    }

    pub fn scroll_to(&mut self, offset: u64) -> bool {
        let offset = offset.min(self.max_offset());
        let changed = offset != self.offset;
        self.offset = offset;
        changed
    }

    pub fn scroll_by(&mut self, delta: i64) -> bool {
        let target = if delta < 0 {
            self.offset.saturating_sub(delta.unsigned_abs())
        } else {
            self.offset.saturating_add(delta as u64)
        };
        self.scroll_to(target)
    }

    /// Scroll the least amount that shows `top..top + height`.
    pub fn reveal(&mut self, top: u64, height: u64) -> bool {
        if top < self.offset {
            self.scroll_to(top)
        } else if top + height > self.offset + self.viewport() {
            self.scroll_to((top + height).saturating_sub(self.viewport()))
        } else {
            false
        }
    }

    /// The bar's rectangle at the right edge of the area.
    pub fn bar(&self) -> Rect {
        Rect {
            x: self.area.x + self.area.width.saturating_sub(SCROLLBAR_WIDTH) as i32,
            y: self.area.y,
            width: SCROLLBAR_WIDTH.min(self.area.width),
            height: self.area.height,
        }
    }

    /// The content area left of the bar.
    pub fn content_area(&self) -> Rect {
        Rect {
            width: self.area.width.saturating_sub(SCROLLBAR_WIDTH),
            ..self.area
        }
    }

    /// The thumb rectangle, or `None` when there is nothing to scroll.
    pub fn thumb(&self) -> Option<Rect> {
        if !self.scrollable() {
            return None;
        }
        let bar = self.bar();
        let track = u64::from(bar.height);
        let length = (track * self.viewport() / self.content.max(1))
            .clamp(u64::from(MIN_THUMB).min(track), track);
        let travel = track - length;
        let top = travel * self.offset / self.max_offset().max(1);
        Some(Rect {
            x: bar.x,
            y: bar.y + top as i32,
            width: bar.width,
            height: length as u32,
        })
    }

    pub fn handle(&mut self, event: &Event) -> ScrollOutcome {
        let changed = match event {
            Event::Scroll { dy, .. } if self.hovered => self.scroll_by(i64::from(*dy) * WHEEL_STEP),
            Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
            } if self.bar().contains(*x, *y) => match self.thumb() {
                Some(thumb) if thumb.contains(*x, *y) => {
                    self.drag = Some((*y, self.offset));
                    true
                }
                Some(thumb) => {
                    let page = self.viewport() as i64 - WHEEL_STEP / 2;
                    let delta = if *y < thumb.y { -page } else { page };
                    self.scroll_by(delta);
                    true
                }
                None => true,
            },
            Event::PointerLeft => {
                self.hovered = false;
                false
            }
            Event::PointerMoved { x, y } => match (self.drag, self.thumb()) {
                (Some((start_y, start_offset)), Some(thumb)) => {
                    let travel = i64::from(self.bar().height) - i64::from(thumb.height);
                    if travel <= 0 {
                        return ScrollOutcome::Ignored;
                    }
                    let moved = i64::from(*y - start_y);
                    let delta = moved * self.max_offset() as i64 / travel;
                    let target = (start_offset as i64 + delta).max(0) as u64;
                    self.scroll_to(target)
                }
                _ => {
                    self.hovered = self.area.contains(*x, *y);
                    false
                }
            },
            Event::PointerReleased {
                button: PointerButton::Primary,
                ..
            } if self.drag.is_some() => {
                self.drag = None;
                true
            }
            _ => false,
        };
        if changed {
            ScrollOutcome::Scrolled
        } else {
            ScrollOutcome::Ignored
        }
    }

    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    pub fn paint(&self, surface: &RasterSurface) -> Result<(), RenderError> {
        let colors = palette();
        let bar = self.bar();
        surface.fill_rect(bar, colors.track)?;
        surface.fill_rect(Rect { width: 1, ..bar }, colors.bevel_shadow)?;
        if let Some(thumb) = self.thumb() {
            surface.fill_rect(thumb, colors.thumb)?;
            surface.stroke_rect(thumb, colors.border)?;
            // Three grip lines in the middle of the thumb.
            let middle = thumb.y + thumb.height as i32 / 2;
            for offset in [-3, 0, 3] {
                surface.fill_rect(
                    Rect {
                        x: thumb.x + 4,
                        y: middle + offset,
                        width: thumb.width.saturating_sub(8),
                        height: 1,
                    },
                    colors.bevel_shadow,
                )?;
            }
        }
        Ok(())
    }

    pub fn accessibility_node(&self) -> accesskit::Node {
        let mut node = accesskit::Node::new(accesskit::Role::ScrollBar);
        node.set_bounds(super::bounds_of(self.bar()));
        node.set_numeric_value(self.offset as f64);
        node.set_min_numeric_value(0.0);
        node.set_max_numeric_value(self.max_offset() as f64);
        node
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::geometry::Scale;

    fn state() -> ScrollState {
        let mut state = ScrollState::new(Rect {
            x: 0,
            y: 0,
            width: 200,
            height: 100,
        });
        state.set_content(1000);
        state
    }

    #[test]
    fn offsets_clamp_and_reveal_scrolls_minimally() {
        let mut state = state();
        assert!(!state.scroll_by(-10));
        assert!(state.scroll_by(5000));
        assert_eq!(state.offset(), 900);
        state.set_content(300);
        assert_eq!(state.offset(), 200);
        assert!(state.reveal(0, 20));
        assert_eq!(state.offset(), 0);
        assert!(!state.reveal(50, 20));
        assert!(state.reveal(150, 20));
        assert_eq!(state.offset(), 70);
        state.set_content(50);
        assert!(!state.scrollable());
        assert_eq!(state.thumb(), None);
    }

    #[test]
    fn wheel_track_and_thumb_drag() {
        let mut state = state();
        // Wheel events only count while the pointer is over the area.
        assert_eq!(
            state.handle(&Event::Scroll { dx: 0, dy: 2 }),
            ScrollOutcome::Ignored
        );
        state.handle(&Event::PointerMoved { x: 50, y: 50 });
        assert_eq!(
            state.handle(&Event::Scroll { dx: 0, dy: 2 }),
            ScrollOutcome::Scrolled
        );
        assert_eq!(state.offset(), 96);
        // Page down by clicking the track below the thumb.
        let thumb = state.thumb().unwrap();
        let below = thumb.y + thumb.height as i32 + 5;
        state.handle(&Event::PointerPressed {
            x: 190,
            y: below,
            button: PointerButton::Primary,
        });
        assert_eq!(state.offset(), 96 + 76);
        // Drag the thumb to the bottom.
        let thumb = state.thumb().unwrap();
        state.handle(&Event::PointerPressed {
            x: 190,
            y: thumb.y + 2,
            button: PointerButton::Primary,
        });
        assert!(state.dragging());
        state.handle(&Event::PointerMoved { x: 190, y: 500 });
        assert_eq!(state.offset(), 900);
        state.handle(&Event::PointerReleased {
            x: 190,
            y: 500,
            button: PointerButton::Primary,
        });
        assert!(!state.dragging());
        // A press in the content area is not the bar's.
        assert_eq!(
            state.handle(&Event::PointerPressed {
                x: 10,
                y: 10,
                button: PointerButton::Primary
            }),
            ScrollOutcome::Ignored
        );
    }

    #[test]
    fn thumb_has_a_minimum_and_paints_inside_the_bar() {
        let mut state = state();
        state.set_content(1_000_000_000);
        let thumb = state.thumb().unwrap();
        assert_eq!(thumb.height, MIN_THUMB);
        state.scroll_to(u64::MAX);
        let thumb = state.thumb().unwrap();
        assert_eq!(thumb.y + thumb.height as i32, 100);
        let surface = RasterSurface::new(200, 100, Scale::new(2).unwrap()).unwrap();
        surface
            .clear(crate::foundation::color::Color::WHITE)
            .unwrap();
        state.paint(&surface).unwrap();
        assert_eq!(surface.pixel(10, 10), Some(0xffffffff));
        assert_ne!(surface.pixel(195 * 2, 10), Some(0xffffffff));
    }
}
