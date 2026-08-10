use crate::{
    theme::ThemeContext, AccessibilityNode, AccessibilityRole, Event, EventResult,
    LayoutConstraint, Point, Rect, Size, Widget, WidgetState,
};
use std::any::Any;

#[derive(Debug, Clone)]
pub struct DockViewItem {
    pub label: String,
    pub icon: String,
    pub is_focused: bool,
    pub is_running: bool,
}

pub struct DockView {
    state: WidgetState,
    pub items: Vec<DockViewItem>,
    /// Index of the item whose icon was pressed, drained by
    /// [`DockView::take_clicked`]. The polling migration lever, same shape as
    /// `Button::take_clicked` / `IconView::take_activated`.
    clicked: Option<usize>,
}

/// Item geometry constants. These are the single source of truth for where
/// dock icons sit — the SDK painter draws with [`DockView::item_rect`] and
/// `handle_event` hit-tests with [`DockView::item_at`], so paint and input
/// cannot drift apart (the shell used to keep its own copy of these numbers
/// and hit-test with that).
pub const DOCK_ITEM_SIZE: f32 = 48.0;
pub const DOCK_ITEM_PADDING: f32 = 8.0;
pub const DOCK_ITEM_SPACING: f32 = 6.0;

impl Default for DockView {
    fn default() -> Self {
        Self::new()
    }
}

impl DockView {
    pub fn new() -> Self {
        Self {
            state: WidgetState::new(),
            items: vec![],
            clicked: None,
        }
    }

    /// Total width of the centered item strip.
    fn items_width(&self) -> f32 {
        if self.items.is_empty() {
            return 0.0;
        }
        self.items.len() as f32 * (DOCK_ITEM_SIZE + DOCK_ITEM_SPACING) - DOCK_ITEM_SPACING
            + DOCK_ITEM_PADDING * 2.0
    }

    /// The visible dock strip: the centered, bottom-anchored plate the icons
    /// sit on. Empty (zero-width) when there are no items.
    pub fn strip_rect(&self) -> Rect {
        let rect = self.rect();
        let height = DOCK_ITEM_SIZE + DOCK_ITEM_PADDING * 2.0;
        Rect::new(
            rect.x + (rect.width - self.items_width()) * 0.5,
            rect.y + rect.height - height,
            self.items_width(),
            height,
        )
    }

    /// Screen rect of item `index`'s icon, given the dock's current rect.
    pub fn item_rect(&self, index: usize) -> Rect {
        let strip = self.strip_rect();
        Rect::new(
            strip.x + DOCK_ITEM_PADDING + index as f32 * (DOCK_ITEM_SIZE + DOCK_ITEM_SPACING),
            strip.y + DOCK_ITEM_PADDING,
            DOCK_ITEM_SIZE,
            DOCK_ITEM_SIZE,
        )
    }

    /// Item whose icon contains `point`, if any.
    pub fn item_at(&self, point: Point) -> Option<usize> {
        (0..self.items.len()).find(|&i| self.item_rect(i).contains(point))
    }

    /// Index of the most recently pressed item; drains exactly once.
    pub fn take_clicked(&mut self) -> Option<usize> {
        self.clicked.take()
    }
}

impl Widget for DockView {
    fn widget_state(&self) -> &WidgetState {
        &self.state
    }
    fn widget_state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        let size = constraint.clamp(Size::new(constraint.max_width, 64.0));
        self.set_rect(Rect::new(
            self.rect().x,
            self.rect().y,
            size.width,
            size.height,
        ));
        size
    }

    fn draw(&self, _theme: &ThemeContext) {}

    fn handle_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::MouseDown {
                button: crate::event::MouseButton::Left,
                point,
                ..
            } => {
                if !self.rect().contains(*point) {
                    return EventResult::Ignored;
                }
                // Launch-on-press mirrors the shell's historical behaviour.
                self.clicked = self.item_at(*point);
                // The whole strip is chrome: clicks between icons are
                // swallowed, never passed to whatever is underneath.
                EventResult::Handled
            }
            _ => EventResult::Ignored,
        }
    }

    fn accessibility(&self) -> Option<AccessibilityNode> {
        Some(AccessibilityNode::new(AccessibilityRole::Toolbar, "dock"))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Modifiers, MouseButton};

    fn dock_with_items(n: usize) -> DockView {
        let mut dock = DockView::new();
        dock.items = (0..n)
            .map(|i| DockViewItem {
                label: format!("app{i}"),
                icon: String::new(),
                is_focused: false,
                is_running: false,
            })
            .collect();
        dock.set_rect(Rect::new(0.0, 736.0, 1024.0, 64.0));
        dock
    }

    fn press(x: f32, y: f32) -> Event {
        Event::MouseDown {
            button: MouseButton::Left,
            point: Point::new(x, y),
            modifiers: Modifiers::NONE,
        }
    }

    #[test]
    fn item_rects_are_centered_and_spaced() {
        let dock = dock_with_items(3);
        // strip = 3*48 + 2*6 + 2*8 = 172, centered in 1024 → x = 426.
        let first = dock.item_rect(0);
        assert_eq!(first.x, 426.0 + 8.0);
        assert_eq!(first.width, 48.0);
        let second = dock.item_rect(1);
        assert_eq!(second.x, first.x + 48.0 + 6.0);
    }

    #[test]
    fn click_on_item_records_it_and_drains_once() {
        let mut dock = dock_with_items(3);
        let center = dock.item_rect(1);
        let result = dock.handle_event(&press(
            center.x + center.width * 0.5,
            center.y + center.height * 0.5,
        ));
        assert!(matches!(result, EventResult::Handled));
        assert_eq!(dock.take_clicked(), Some(1));
        assert_eq!(dock.take_clicked(), None, "drains exactly once");
    }

    #[test]
    fn click_between_items_is_swallowed_without_recording() {
        let mut dock = dock_with_items(2);
        let first = dock.item_rect(0);
        // Just right of the first icon, inside the spacing gap.
        let result = dock.handle_event(&press(first.x + first.width + 2.0, first.y + 10.0));
        assert!(
            matches!(result, EventResult::Handled),
            "dock strip is chrome"
        );
        assert_eq!(dock.take_clicked(), None);
    }

    #[test]
    fn click_outside_the_dock_is_ignored() {
        let mut dock = dock_with_items(2);
        let result = dock.handle_event(&press(10.0, 10.0));
        assert!(matches!(result, EventResult::Ignored));
        assert_eq!(dock.take_clicked(), None);
    }

    #[test]
    fn item_at_matches_item_rect() {
        let dock = dock_with_items(4);
        for i in 0..4 {
            let r = dock.item_rect(i);
            assert_eq!(
                dock.item_at(Point::new(r.x + 1.0, r.y + 1.0)),
                Some(i),
                "item {i}"
            );
        }
        assert_eq!(dock.item_at(Point::new(0.0, 750.0)), None);
    }
}
