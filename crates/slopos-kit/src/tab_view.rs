use crate::theme::ThemeContext;
use crate::{
    event::{KeyCode, MouseButton},
    measure_text_width, AccessibilityNode, AccessibilityRole, Event, EventResult, LayoutConstraint,
    Point, Rect, Size, Widget, WidgetState,
};
use std::any::Any;

/// Height of the tab-header strip. Must match `draw_tab_view` in
/// `slopos-sdk/src/lib.rs`, which paints the header at this same height and is
/// the reason `layout()` cannot simply pick whatever number it likes.
const HEADER_HEIGHT: f32 = 30.0;

pub struct Tab {
    pub id: String,
    pub title: String,
    pub content: Option<Box<dyn Widget>>,
}

pub struct TabView {
    state: WidgetState,
    pub tabs: Vec<Tab>,
    pub selected_tab_index: usize,
    /// One rect per tab in `tabs`, in the same order, rebuilt by `layout()`.
    /// Geometry mirrors `draw_tab_view` exactly (shaped title width + 24.0
    /// wide, 25px tall, starting at `rect.x + 8.0` / `rect.y + 4.0`) so a
    /// click always lands on the header it visually appears under.
    header_rects: Vec<Rect>,
}

impl Default for TabView {
    fn default() -> Self {
        Self::new()
    }
}

impl TabView {
    pub fn new() -> Self {
        Self {
            state: WidgetState::new(),
            tabs: vec![],
            selected_tab_index: 0,
            header_rects: vec![],
        }
    }

    pub fn add_tab(&mut self, id: &str, title: &str, content: Box<dyn Widget>) {
        self.tabs.push(Tab {
            id: id.to_string(),
            title: title.to_string(),
            content: Some(content),
        });
    }

    pub fn remove_tab(&mut self, id: &str) -> bool {
        if let Some(pos) = self.tabs.iter().position(|t| t.id == id) {
            self.tabs.remove(pos);
            if self.selected_tab_index >= self.tabs.len() && !self.tabs.is_empty() {
                self.selected_tab_index = self.tabs.len() - 1;
            }
            true
        } else {
            false
        }
    }

    pub fn select_tab(&mut self, index: usize) -> bool {
        if index < self.tabs.len() {
            self.selected_tab_index = index;
            true
        } else {
            false
        }
    }

    pub fn selected_content(&self) -> Option<&dyn Widget> {
        if let Some(t) = self.tabs.get(self.selected_tab_index) {
            if let Some(ref c) = t.content {
                return Some(c.as_ref());
            }
        }
        None
    }

    pub fn selected_content_mut(&mut self) -> Option<&mut dyn Widget> {
        if let Some(t) = self.tabs.get_mut(self.selected_tab_index) {
            if let Some(ref mut c) = t.content {
                return Some(c.as_mut());
            }
        }
        None
    }

    /// Per-tab header rects from the most recent `layout()`, in `tabs` order.
    /// Published the same way `MenuBar::menu_rects()` publishes its geometry,
    /// so callers outside this widget can hit-test or draw against it too.
    pub fn header_rects(&self) -> &[Rect] {
        &self.header_rects
    }

    fn recompute_header_rects(&mut self) {
        self.header_rects.clear();
        let rect = self.rect();
        let mut current_x = rect.x + 8.0;
        for tab in &self.tabs {
            let tab_width = measure_text_width(&tab.title) + 24.0;
            self.header_rects
                .push(Rect::new(current_x, rect.y + 4.0, tab_width, 25.0));
            current_x += tab_width + 4.0;
        }
    }

    fn tab_at_point(&self, point: Point) -> Option<usize> {
        self.header_rects.iter().position(|r| r.contains(point))
    }

    /// The area below the header strip, where the selected tab's content is
    /// drawn and should receive pointer events.
    fn content_rect(&self) -> Rect {
        let rect = self.rect();
        Rect::new(
            rect.x,
            rect.y + HEADER_HEIGHT,
            rect.width,
            (rect.height - HEADER_HEIGHT).max(0.0),
        )
    }
}

/// The point carried by pointer/drag events, if any. `MouseEnter`/`MouseLeave`
/// and non-pointer events (keyboard, focus, scroll, layout) have none and are
/// left to their own handling rather than being gated on `content_rect`.
fn pointer_location(event: &Event) -> Option<Point> {
    match event {
        Event::MouseDown { point, .. }
        | Event::MouseUp { point, .. }
        | Event::MouseMove { point, .. }
        | Event::Click { point, .. }
        | Event::DoubleClick { point, .. }
        | Event::DragStart { point }
        | Event::Drag { point }
        | Event::DragEnd { point }
        | Event::Drop { point } => Some(*point),
        _ => None,
    }
}

impl Widget for TabView {
    fn widget_state(&self) -> &WidgetState {
        &self.state
    }
    fn widget_state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }

    fn focusable(&self) -> bool {
        true
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        let mut child_size = Size::ZERO;

        if let Some(tab) = self.tabs.get_mut(self.selected_tab_index) {
            if let Some(content) = &mut tab.content {
                let child_constraint = LayoutConstraint {
                    min_width: constraint.min_width,
                    max_width: constraint.max_width,
                    min_height: (constraint.min_height - HEADER_HEIGHT).max(0.0),
                    max_height: (constraint.max_height - HEADER_HEIGHT).max(0.0),
                };
                child_size = content.layout(child_constraint);
            }
        }

        let size = constraint.clamp(Size::new(
            child_size.width.max(constraint.min_width),
            child_size.height + HEADER_HEIGHT,
        ));
        self.set_rect(Rect::new(
            self.rect().x,
            self.rect().y,
            size.width,
            size.height,
        ));
        self.recompute_header_rects();
        size
    }

    fn draw(&self, _theme: &ThemeContext) {}

    fn handle_event(&mut self, event: &Event) -> EventResult {
        if let Event::MouseDown {
            button: MouseButton::Left,
            point,
            ..
        } = event
        {
            if let Some(index) = self.tab_at_point(*point) {
                self.select_tab(index);
                return EventResult::Handled;
            }
        }

        if let Event::KeyDown { key, .. } = event {
            if self.state.focused {
                match key {
                    KeyCode::ArrowLeft => {
                        if self.selected_tab_index > 0 {
                            self.select_tab(self.selected_tab_index - 1);
                        }
                        return EventResult::Handled;
                    }
                    KeyCode::ArrowRight => {
                        if self.selected_tab_index + 1 < self.tabs.len() {
                            self.select_tab(self.selected_tab_index + 1);
                        }
                        return EventResult::Handled;
                    }
                    _ => {}
                }
            }
        }

        // Anything else carrying a pointer location (including a MouseDown
        // that missed every header) only reaches the content if it actually
        // lands below the header strip.
        if let Some(point) = pointer_location(event) {
            if !self.content_rect().contains(point) {
                return EventResult::Ignored;
            }
        }

        if let Some(tab) = self.tabs.get_mut(self.selected_tab_index) {
            if let Some(content) = &mut tab.content {
                return content.handle_event(event);
            }
        }
        EventResult::Ignored
    }

    fn accessibility(&self) -> Option<AccessibilityNode> {
        Some(AccessibilityNode::new(
            AccessibilityRole::TabGroup,
            "Tab View",
        ))
    }

    fn children(&self) -> Vec<&dyn Widget> {
        let mut result = vec![];
        for tab in &self.tabs {
            if let Some(ref c) = tab.content {
                result.push(c.as_ref());
            }
        }
        result
    }

    fn children_mut(&mut self) -> Vec<&mut dyn Widget> {
        // `Vec::iter_mut()` already hands out `&mut Tab`s whose lifetime is
        // tied to this call's `&mut self` borrow (not to the loop body), so
        // `as_deref_mut()` (`Option<Box<dyn Widget>> -> Option<&mut dyn
        // Widget>`) returns references that are valid for the whole return
        // value with no raw-pointer cast required.
        //
        // Built with an explicit loop rather than `.filter_map(...).collect()`:
        // the closure form defeats lifetime elision on the trait method's
        // return type (rustc asks for an explicit `'_` bound on `dyn Widget`),
        // so this matches the plain-loop style used elsewhere in the crate.
        let mut result: Vec<&mut dyn Widget> = vec![];
        for tab in self.tabs.iter_mut() {
            if let Some(c) = tab.content.as_deref_mut() {
                result.push(c);
            }
        }
        result
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
    use crate::event::Modifiers;

    /// Minimal widget for pure logic tests: records how many events it
    /// received and what its rect was set to, without any window/GPU/painter
    /// dependency.
    struct RecordingWidget {
        state: WidgetState,
        hits: u32,
    }

    impl RecordingWidget {
        fn new() -> Self {
            Self {
                state: WidgetState::new(),
                hits: 0,
            }
        }
    }

    impl Widget for RecordingWidget {
        fn widget_state(&self) -> &WidgetState {
            &self.state
        }
        fn widget_state_mut(&mut self) -> &mut WidgetState {
            &mut self.state
        }
        fn layout(&mut self, constraint: LayoutConstraint) -> Size {
            constraint.clamp(Size::ZERO)
        }
        fn draw(&self, _theme: &ThemeContext) {}
        fn handle_event(&mut self, _event: &Event) -> EventResult {
            self.hits += 1;
            EventResult::Handled
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    }

    fn two_tab_view() -> TabView {
        let mut tv = TabView::new();
        tv.add_tab("one", "One", Box::new(RecordingWidget::new()));
        tv.add_tab("two", "Two", Box::new(RecordingWidget::new()));
        tv
    }

    #[test]
    fn layout_computes_header_rects_matching_the_painter_formula() {
        let mut tv = two_tab_view();
        tv.layout(LayoutConstraint::tight(Size::new(300.0, 200.0)));

        let rects = tv.header_rects();
        assert_eq!(rects.len(), 2);

        let first_width = measure_text_width("One") + 24.0;
        assert_eq!(rects[0].x, 8.0);
        assert_eq!(rects[0].y, 4.0);
        assert!((rects[0].width - first_width).abs() < 0.01);
        assert_eq!(rects[0].height, 25.0);

        // Next tab starts after the first tab's width plus the 4.0 gap.
        assert!((rects[1].x - (8.0 + first_width + 4.0)).abs() < 0.01);
        assert_eq!(rects[1].y, 4.0);
        assert!((rects[1].width - (measure_text_width("Two") + 24.0)).abs() < 0.01);
    }

    #[test]
    fn header_geometry_uses_shaped_unicode_title_width() {
        let mut tv = TabView::new();
        tv.add_tab("unicode", "日本語", Box::new(RecordingWidget::new()));
        tv.layout(LayoutConstraint::tight(Size::new(300.0, 200.0)));

        let actual = tv.header_rects()[0].width;
        let expected = measure_text_width("日本語") + 24.0;
        assert!((actual - expected).abs() < 0.01);

        let byte_count_estimate = "日本語".len() as f32 * 7.0 + 24.0;
        assert!(
            (actual - byte_count_estimate).abs() > 0.5,
            "tab header geometry must not use a UTF-8 byte-count estimate"
        );
    }

    #[test]
    fn mouse_down_on_a_header_selects_that_tab() {
        let mut tv = two_tab_view();
        tv.layout(LayoutConstraint::tight(Size::new(300.0, 200.0)));
        assert_eq!(tv.selected_tab_index, 0);

        let second_header = tv.header_rects()[1];
        let point = Point::new(second_header.x + 5.0, second_header.y + 5.0);

        let result = tv.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point,
            modifiers: Modifiers::NONE,
        });

        assert!(matches!(result, EventResult::Handled));
        assert_eq!(tv.selected_tab_index, 1);
    }

    #[test]
    fn pointer_event_inside_content_rect_reaches_selected_content() {
        let mut tv = two_tab_view();
        tv.layout(LayoutConstraint::tight(Size::new(300.0, 200.0)));

        // Below the header strip (y >= HEADER_HEIGHT): inside the content.
        let point = Point::new(20.0, 100.0);
        let result = tv.handle_event(&Event::MouseMove {
            point,
            modifiers: Modifiers::NONE,
        });

        assert!(matches!(result, EventResult::Handled));
        let content = tv
            .tabs
            .first()
            .and_then(|t| t.content.as_ref())
            .and_then(|c| c.as_any().downcast_ref::<RecordingWidget>())
            .unwrap();
        assert_eq!(content.hits, 1);
    }

    #[test]
    fn pointer_event_over_the_header_does_not_reach_content() {
        let mut tv = two_tab_view();
        tv.layout(LayoutConstraint::tight(Size::new(300.0, 200.0)));

        // Inside the widget's rect but above HEADER_HEIGHT (y < 30, past both
        // tab headers on the x axis too): must not fall through to content.
        let point = Point::new(250.0, 10.0);
        let result = tv.handle_event(&Event::MouseMove {
            point,
            modifiers: Modifiers::NONE,
        });

        assert!(matches!(result, EventResult::Ignored));
        let content = tv
            .tabs
            .first()
            .and_then(|t| t.content.as_ref())
            .and_then(|c| c.as_any().downcast_ref::<RecordingWidget>())
            .unwrap();
        assert_eq!(content.hits, 0);
    }

    #[test]
    fn arrow_keys_change_selection_only_when_focused() {
        let mut tv = two_tab_view();
        tv.layout(LayoutConstraint::tight(Size::new(300.0, 200.0)));

        let right = Event::KeyDown {
            key: KeyCode::ArrowRight,
            modifiers: Modifiers::NONE,
        };

        // Not focused: TabView does not intercept the key; nothing to
        // change since RecordingWidget doesn't care about arrow keys either,
        // but selection must stay put.
        let _ = tv.handle_event(&right);
        assert_eq!(tv.selected_tab_index, 0);

        tv.widget_state_mut().focused = true;
        let result = tv.handle_event(&right);
        assert!(matches!(result, EventResult::Handled));
        assert_eq!(tv.selected_tab_index, 1);

        // Right again at the last tab is a no-op but still handled.
        let result = tv.handle_event(&right);
        assert!(matches!(result, EventResult::Handled));
        assert_eq!(tv.selected_tab_index, 1);

        let left = Event::KeyDown {
            key: KeyCode::ArrowLeft,
            modifiers: Modifiers::NONE,
        };
        let result = tv.handle_event(&left);
        assert!(matches!(result, EventResult::Handled));
        assert_eq!(tv.selected_tab_index, 0);
    }

    #[test]
    fn is_focusable() {
        let tv = TabView::new();
        assert!(tv.focusable());
    }

    #[test]
    fn children_mut_yields_working_mutable_references_without_unsafe() {
        let mut tv = two_tab_view();
        {
            let mut children = tv.children_mut();
            assert_eq!(children.len(), 2);
            children[0].set_rect(Rect::new(1.0, 2.0, 3.0, 4.0));
        }

        let first_rect = tv.tabs[0].content.as_ref().unwrap().rect();
        assert_eq!(first_rect.x, 1.0);
        assert_eq!(first_rect.y, 2.0);
        assert_eq!(first_rect.width, 3.0);
        assert_eq!(first_rect.height, 4.0);
    }
}
