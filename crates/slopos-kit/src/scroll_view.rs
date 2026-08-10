use crate::{
    event::MouseButton, theme::ThemeContext, Event, EventResult, LayoutConstraint, Point, Rect,
    Size, Widget, WidgetState,
};

/// Width of the vertical scrollbar track/thumb, matching `[scrollbar].width`
/// in `Metrics.toml` so the painter's geometry lines up with what it reads
/// from the theme.
const SCROLLBAR_WIDTH: f32 = 12.0;
/// Floor on thumb length, matching `[scrollbar].min_thumb_size`, so a very
/// tall content area never shrinks the thumb to something undraggable.
const MIN_THUMB_SIZE: f32 = 24.0;

pub struct ScrollView {
    state: WidgetState,
    pub content: Option<Box<dyn Widget>>,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub scrollable_x: bool,
    pub scrollable_y: bool,
    /// Natural size of `content`, measured the last time `layout()` ran.
    /// Needed to know how far `scroll_x`/`scroll_y` are allowed to go.
    content_size: Size,
    /// Last cursor position observed via `MouseMove`, in the same
    /// coordinate space as `rect()`. `Scroll` events carry no point of their
    /// own (see `Event::Scroll`), so this is what lets the scroll handler
    /// hit-test against `self.rect()` instead of reacting to every wheel
    /// event delivered anywhere in the tree.
    last_pointer: Option<Point>,
    dragging_thumb: bool,
    /// (pointer y, scroll_y) captured when the thumb drag started, so drag
    /// deltas are computed from the original grab rather than accumulated
    /// per-event, which would drift under rounding.
    drag_anchor: (f32, f32),
}

impl Default for ScrollView {
    fn default() -> Self {
        Self::new()
    }
}

impl ScrollView {
    pub fn new() -> Self {
        Self {
            state: WidgetState::new(),
            content: None,
            scroll_x: 0.0,
            scroll_y: 0.0,
            scrollable_x: false,
            scrollable_y: true,
            content_size: Size::ZERO,
            last_pointer: None,
            dragging_thumb: false,
            drag_anchor: (0.0, 0.0),
        }
    }

    pub fn set_content(&mut self, widget: Box<dyn Widget>) {
        self.content = Some(widget);
    }

    /// Current scroll position (`scroll_x`, `scroll_y`) as a `Point`, for
    /// callers that want a single value rather than the two fields.
    pub fn scroll_offset(&self) -> Point {
        Point::new(self.scroll_x, self.scroll_y)
    }

    /// Geometry of the vertical scrollbar thumb, for the painter to draw and
    /// for hit-testing thumb drags. `None` when there's nothing to scroll:
    /// `scrollable_y` is off, or `content` already fits the viewport.
    pub fn scrollbar_rect(&self) -> Option<Rect> {
        if !self.scrollable_y {
            return None;
        }
        let max_scroll = self.max_scroll_y();
        if max_scroll <= 0.0 {
            return None;
        }
        let rect = self.rect();
        let thumb_height = self.thumb_height();
        let travel = (rect.height - thumb_height).max(0.0);
        let thumb_y = rect.y + travel * (self.scroll_y / max_scroll).clamp(0.0, 1.0);
        Some(Rect::new(
            rect.x + rect.width - SCROLLBAR_WIDTH,
            thumb_y,
            SCROLLBAR_WIDTH,
            thumb_height,
        ))
    }

    fn max_scroll_x(&self) -> f32 {
        (self.content_size.width - self.rect().width).max(0.0)
    }

    fn max_scroll_y(&self) -> f32 {
        (self.content_size.height - self.rect().height).max(0.0)
    }

    fn thumb_height(&self) -> f32 {
        let viewport = self.rect().height.max(0.0);
        if self.content_size.height <= viewport || self.content_size.height <= 0.0 {
            return viewport;
        }
        let proportional = viewport * (viewport / self.content_size.height);
        proportional.clamp(MIN_THUMB_SIZE.min(viewport), viewport)
    }

    /// Re-derives the drag delta from the original grab point (`drag_anchor`)
    /// rather than the previous frame's position, then clamps and repositions
    /// `content` to match.
    fn drag_thumb_to(&mut self, pointer_y: f32) {
        let max_scroll = self.max_scroll_y();
        if max_scroll <= 0.0 {
            return;
        }
        let travel = (self.rect().height - self.thumb_height()).max(1.0);
        let (anchor_y, anchor_scroll) = self.drag_anchor;
        let scroll_per_pixel = max_scroll / travel;
        self.scroll_y =
            (anchor_scroll + (pointer_y - anchor_y) * scroll_per_pixel).clamp(0.0, max_scroll);
        self.reposition_content();
    }

    /// Moves `content`'s rect to match the current `scroll_x`/`scroll_y`
    /// without re-running its layout — used after a scroll or drag changes
    /// the offset outside of a full `layout()` pass.
    fn reposition_content(&mut self) {
        let rect = self.rect();
        let (scroll_x, scroll_y) = (self.scroll_x, self.scroll_y);
        if let Some(content) = &mut self.content {
            let content_rect = content.rect();
            content.set_rect(Rect::new(
                rect.x - scroll_x,
                rect.y - scroll_y,
                content_rect.width,
                content_rect.height,
            ));
        }
    }

    fn forward_to_content(&mut self, event: &Event) -> EventResult {
        if let Some(content) = &mut self.content {
            content.handle_event(event)
        } else {
            EventResult::Ignored
        }
    }
}

impl Widget for ScrollView {
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

        if let Some(content) = &mut self.content {
            let content_constraint =
                LayoutConstraint::loose(Size::new(size.width * 2.0, size.height * 2.0));
            self.content_size = content.layout(content_constraint);
        } else {
            self.content_size = Size::ZERO;
        }

        // Clamp to (content - viewport).max(0): scroll can never sit past
        // the end of the content, and shrinking the content or growing the
        // viewport (window resize, content swap) pulls a stale offset back
        // into range instead of leaving it scrolled past the end.
        self.scroll_x = self.scroll_x.clamp(0.0, self.max_scroll_x());
        self.scroll_y = self.scroll_y.clamp(0.0, self.max_scroll_y());
        self.reposition_content();
        size
    }

    fn draw(&self, theme: &ThemeContext) {
        if let Some(content) = &self.content {
            content.draw(theme);
        }
    }

    fn handle_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::MouseDown {
                button: MouseButton::Left,
                point,
                ..
            } => {
                if let Some(thumb) = self.scrollbar_rect() {
                    if thumb.contains(*point) {
                        self.dragging_thumb = true;
                        self.drag_anchor = (point.y, self.scroll_y);
                        return EventResult::Handled;
                    }
                }
                self.forward_to_content(event)
            }
            Event::MouseMove { point, .. } => {
                self.last_pointer = Some(*point);
                if self.dragging_thumb {
                    self.drag_thumb_to(point.y);
                    return EventResult::Handled;
                }
                self.forward_to_content(event)
            }
            Event::MouseUp {
                button: MouseButton::Left,
                ..
            } if self.dragging_thumb => {
                self.dragging_thumb = false;
                EventResult::Handled
            }
            Event::MouseLeave => {
                self.last_pointer = None;
                self.dragging_thumb = false;
                self.forward_to_content(event)
            }
            Event::Scroll { delta, .. } => {
                // The rect gate this widget was missing: `Scroll` carries no
                // point of its own, so hit-test against the last position we
                // actually observed the cursor at, rather than reacting to
                // every wheel event delivered anywhere in the tree.
                let hit = match self.last_pointer {
                    Some(p) => self.rect().contains(p),
                    None => false,
                };
                if !hit {
                    return EventResult::Ignored;
                }
                if self.scrollable_y {
                    let max = self.max_scroll_y();
                    self.scroll_y = (self.scroll_y - delta.y).clamp(0.0, max);
                }
                if self.scrollable_x {
                    let max = self.max_scroll_x();
                    self.scroll_x = (self.scroll_x - delta.x).clamp(0.0, max);
                }
                self.reposition_content();
                EventResult::Handled
            }
            _ => self.forward_to_content(event),
        }
    }

    fn children(&self) -> Vec<&dyn Widget> {
        match &self.content {
            Some(c) => vec![c.as_ref()],
            None => vec![],
        }
    }

    fn children_mut(&mut self) -> Vec<&mut dyn Widget> {
        match &mut self.content {
            Some(c) => vec![c.as_mut()],
            None => vec![],
        }
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
    use crate::event::Modifiers;

    /// Minimal content widget with a fixed natural size, for exercising
    /// `ScrollView::layout()`'s content-size tracking without any real
    /// window, GPU, or filesystem dependency.
    struct FixedContent {
        state: WidgetState,
        size: Size,
    }

    impl FixedContent {
        fn new(width: f32, height: f32) -> Self {
            Self {
                state: WidgetState::new(),
                size: Size::new(width, height),
            }
        }
    }

    impl Widget for FixedContent {
        fn widget_state(&self) -> &WidgetState {
            &self.state
        }
        fn widget_state_mut(&mut self) -> &mut WidgetState {
            &mut self.state
        }
        fn layout(&mut self, constraint: LayoutConstraint) -> Size {
            let size = constraint.clamp(self.size);
            self.set_rect(Rect::new(
                self.rect().x,
                self.rect().y,
                size.width,
                size.height,
            ));
            size
        }
        fn draw(&self, _theme: &ThemeContext) {}
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
    }

    fn mouse_move(x: f32, y: f32) -> Event {
        Event::MouseMove {
            point: Point::new(x, y),
            modifiers: Modifiers::NONE,
        }
    }

    fn wheel(dx: f32, dy: f32) -> Event {
        Event::Scroll {
            delta: Point::new(dx, dy),
            modifiers: Modifiers::NONE,
        }
    }

    // `layout()` hands content a *loose* constraint capped at 2x the
    // viewport (`size * 2.0`, see `ScrollView::layout`), so a content height
    // must stay under that cap for these tests to observe its true natural
    // size rather than the cap itself. 180 in a 100-tall viewport (cap 200)
    // does that while still overflowing the viewport.
    fn view_with_tall_content() -> ScrollView {
        let mut view = ScrollView::new();
        view.set_content(Box::new(FixedContent::new(100.0, 180.0)));
        view.set_rect(Rect::new(0.0, 0.0, 100.0, 100.0));
        view.layout(LayoutConstraint::tight(Size::new(100.0, 100.0)));
        view
    }

    #[test]
    fn scroll_is_ignored_when_the_cursor_was_never_over_the_view() {
        let mut view = view_with_tall_content();

        // No prior MouseMove into this widget's rect: a wheel event delivered
        // while the cursor is elsewhere in the tree must not scroll it. This
        // is the exact bug: "the first ScrollView in the tree consumes every
        // wheel event wherever the cursor is."
        let result = view.handle_event(&wheel(0.0, -50.0));
        assert!(matches!(result, EventResult::Ignored));
        assert_eq!(view.scroll_y, 0.0);
    }

    #[test]
    fn scroll_is_ignored_when_the_cursor_has_moved_outside_the_rect() {
        let mut view = view_with_tall_content();
        let _ = view.handle_event(&mouse_move(500.0, 500.0));

        let result = view.handle_event(&wheel(0.0, -50.0));
        assert!(matches!(result, EventResult::Ignored));
        assert_eq!(view.scroll_y, 0.0);
    }

    #[test]
    fn scroll_moves_and_clamps_to_content_minus_viewport_when_cursor_is_over_it() {
        let mut view = view_with_tall_content();
        let _ = view.handle_event(&mouse_move(50.0, 50.0));

        let result = view.handle_event(&wheel(0.0, -50.0));
        assert!(matches!(result, EventResult::Handled));
        assert_eq!(view.scroll_y, 50.0);

        // Content is 180 tall in a 100-tall viewport: max scroll is 80, no
        // matter how large the wheel delta is.
        let result = view.handle_event(&wheel(0.0, -10_000.0));
        assert!(matches!(result, EventResult::Handled));
        assert_eq!(view.scroll_y, 80.0);
    }

    #[test]
    fn layout_pulls_a_stale_offset_back_in_range_when_content_shrinks() {
        let mut view = view_with_tall_content();
        view.scroll_y = 80.0;
        view.layout(LayoutConstraint::tight(Size::new(100.0, 100.0)));
        assert_eq!(
            view.scroll_y, 80.0,
            "still in range against the original content"
        );

        // Swap in shorter content; re-laying out must pull the now
        // out-of-range offset back to (content - viewport).max(0) rather
        // than leaving the view scrolled past the end of its (now shorter)
        // content.
        view.set_content(Box::new(FixedContent::new(100.0, 120.0)));
        view.layout(LayoutConstraint::tight(Size::new(100.0, 100.0)));
        assert_eq!(view.scroll_y, 20.0);
    }

    #[test]
    fn scrollbar_rect_is_none_when_content_fits_the_viewport() {
        let mut view = ScrollView::new();
        view.set_content(Box::new(FixedContent::new(100.0, 50.0)));
        view.set_rect(Rect::new(0.0, 0.0, 100.0, 100.0));
        view.layout(LayoutConstraint::tight(Size::new(100.0, 100.0)));
        assert!(view.scrollbar_rect().is_none());
    }

    #[test]
    fn scrollbar_rect_tracks_the_scroll_position() {
        let mut view = ScrollView::new();
        view.set_content(Box::new(FixedContent::new(100.0, 150.0)));
        view.set_rect(Rect::new(0.0, 0.0, 100.0, 100.0));
        view.layout(LayoutConstraint::tight(Size::new(100.0, 100.0)));

        let at_top = view.scrollbar_rect().expect("content overflows viewport");
        assert_eq!(at_top.y, 0.0);

        view.scroll_y = view.max_scroll_y();
        view.reposition_content();
        let at_bottom = view.scrollbar_rect().expect("still overflowing");
        assert_eq!(at_bottom.y, 100.0 - at_bottom.height);
    }

    #[test]
    fn dragging_the_thumb_moves_the_scroll_offset() {
        let mut view = ScrollView::new();
        view.set_content(Box::new(FixedContent::new(100.0, 300.0)));
        view.set_rect(Rect::new(0.0, 0.0, 100.0, 100.0));
        view.layout(LayoutConstraint::tight(Size::new(100.0, 100.0)));

        let thumb = view.scrollbar_rect().expect("content overflows viewport");
        let grab = Point::new(thumb.x + thumb.width / 2.0, thumb.y + 1.0);

        let result = view.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point: grab,
            modifiers: Modifiers::NONE,
        });
        assert!(matches!(result, EventResult::Handled));

        let result = view.handle_event(&Event::MouseMove {
            point: Point::new(grab.x, grab.y + 40.0),
            modifiers: Modifiers::NONE,
        });
        assert!(matches!(result, EventResult::Handled));
        assert!(
            view.scroll_y > 0.0,
            "dragging the thumb down should scroll the content down"
        );

        let result = view.handle_event(&Event::MouseUp {
            button: MouseButton::Left,
            point: grab,
            modifiers: Modifiers::NONE,
        });
        assert!(matches!(result, EventResult::Handled));
        assert!(!view.dragging_thumb);
    }

    #[test]
    fn mouse_down_off_the_thumb_still_reaches_content() {
        let handled = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

        struct CountingWidget {
            state: WidgetState,
            handled: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        }
        impl Widget for CountingWidget {
            fn widget_state(&self) -> &WidgetState {
                &self.state
            }
            fn widget_state_mut(&mut self) -> &mut WidgetState {
                &mut self.state
            }
            fn layout(&mut self, constraint: LayoutConstraint) -> Size {
                constraint.clamp(Size::new(100.0, 300.0))
            }
            fn draw(&self, _theme: &ThemeContext) {}
            fn handle_event(&mut self, _event: &Event) -> EventResult {
                self.handled
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                EventResult::Handled
            }
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
            fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
                self
            }
        }

        let mut view = ScrollView::new();
        view.set_content(Box::new(CountingWidget {
            state: WidgetState::new(),
            handled: handled.clone(),
        }));
        view.set_rect(Rect::new(0.0, 0.0, 100.0, 100.0));
        view.layout(LayoutConstraint::tight(Size::new(100.0, 100.0)));

        let result = view.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point: Point::new(10.0, 10.0),
            modifiers: Modifiers::NONE,
        });
        assert!(matches!(result, EventResult::Handled));
        assert_eq!(handled.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(!view.dragging_thumb);
    }

    #[test]
    fn scroll_offset_reports_the_current_position() {
        let mut view = ScrollView::new();
        view.scroll_x = 3.0;
        view.scroll_y = 4.0;
        let offset = view.scroll_offset();
        assert_eq!(offset.x, 3.0);
        assert_eq!(offset.y, 4.0);
    }
}
