use crate::theme::ThemeContext;
use crate::{
    event::MouseButton, AccessibilityNode, AccessibilityRole, Event, EventResult, LayoutConstraint,
    Rect, Size, Widget, WidgetState,
};
use std::any::Any;

/// Height of each row in the open dropdown list, in logical pixels. Matches
/// `MenuBar`'s item pitch so popup/menu dropdowns feel consistent.
const ITEM_HEIGHT: f32 = 20.0;

pub struct PopupButton {
    state: WidgetState,
    pub items: Vec<String>,
    pub selected_index: usize,
    pub open: bool,
    dropdown_rect: Rect,
    item_rects: Vec<Rect>,
    /// Index chosen by the most recent click, drained by `take_selected` --
    /// the same polling lever `Button::take_clicked` uses so apps can adopt
    /// this without restructuring around `on_select`.
    pending_selection: Option<usize>,
    pub on_select: Option<Box<dyn FnMut(usize) + Send>>,
}

impl Default for PopupButton {
    fn default() -> Self {
        Self::new()
    }
}

impl PopupButton {
    pub fn new() -> Self {
        Self {
            state: WidgetState::new(),
            items: vec![],
            selected_index: 0,
            open: false,
            dropdown_rect: Rect::ZERO,
            item_rects: vec![],
            pending_selection: None,
            on_select: None,
        }
    }

    pub fn add_item(&mut self, item: &str) {
        self.items.push(item.to_string());
    }

    pub fn select_item(&mut self, index: usize) -> bool {
        if index < self.items.len() {
            self.selected_index = index;
            true
        } else {
            false
        }
    }

    pub fn selected_title(&self) -> Option<&str> {
        self.items.get(self.selected_index).map(|s| s.as_str())
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    /// The rect of the open dropdown list, computed in `layout()`. Zero-sized
    /// (but positioned) when there are no items; meaningless -- but harmless
    /// to read -- while `open` is false, same as `MenuBar::dropdown_rect`.
    pub fn dropdown_rect(&self) -> Rect {
        self.dropdown_rect
    }

    /// Per-item rects within the open dropdown, in the same order as
    /// `items`, so the SDK painter can draw the list and highlight rows.
    pub fn item_rects(&self) -> &[Rect] {
        &self.item_rects
    }

    pub fn item_rect(&self, index: usize) -> Option<Rect> {
        self.item_rects.get(index).copied()
    }

    /// The index selected by the most recent click, if any, consuming it so a
    /// caller's poll loop sees each selection exactly once.
    pub fn take_selected(&mut self) -> Option<usize> {
        self.pending_selection.take()
    }

    fn item_at_point(&self, point: crate::Point) -> Option<usize> {
        self.item_rects.iter().position(|r| r.contains(point))
    }
}

impl Widget for PopupButton {
    fn widget_state(&self) -> &WidgetState {
        &self.state
    }
    fn widget_state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        let size = constraint.clamp(Size::new(150.0, 26.0));
        self.set_rect(Rect::new(
            self.rect().x,
            self.rect().y,
            size.width,
            size.height,
        ));

        let rect = self.rect();
        self.dropdown_rect = Rect::new(
            rect.x,
            rect.y + rect.height,
            rect.width,
            self.items.len() as f32 * ITEM_HEIGHT,
        );
        self.item_rects = (0..self.items.len())
            .map(|index| {
                Rect::new(
                    self.dropdown_rect.x,
                    self.dropdown_rect.y + index as f32 * ITEM_HEIGHT,
                    self.dropdown_rect.width,
                    ITEM_HEIGHT,
                )
            })
            .collect();

        size
    }

    fn draw(&self, _theme: &ThemeContext) {}

    fn handle_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::MouseDown {
                button: MouseButton::Left,
                point,
                ..
            } => {
                if self.open {
                    if let Some(index) = self.item_at_point(*point) {
                        self.select_item(index);
                        self.pending_selection = Some(index);
                        if let Some(on_select) = &mut self.on_select {
                            on_select(index);
                        }
                        self.open = false;
                        return EventResult::Handled;
                    }
                }

                if self.rect().contains(*point) {
                    self.toggle();
                    return EventResult::Handled;
                }

                if self.open {
                    // Click outside both the button and the open list: close
                    // without changing the selection.
                    self.open = false;
                    return EventResult::Handled;
                }

                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
    }

    fn accessibility(&self) -> Option<AccessibilityNode> {
        let title = self.selected_title().unwrap_or("Popup Button");
        Some(AccessibilityNode::new(AccessibilityRole::ComboBox, title))
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
    use crate::{event::Modifiers, Point};

    fn test_popup() -> PopupButton {
        let mut popup = PopupButton::new();
        popup.add_item("One");
        popup.add_item("Two");
        popup.add_item("Three");
        popup
    }

    fn mouse_down(point: Point) -> Event {
        Event::MouseDown {
            button: MouseButton::Left,
            point,
            modifiers: Modifiers::NONE,
        }
    }

    #[test]
    fn layout_computes_dropdown_and_item_rects() {
        let mut popup = test_popup();
        popup.layout(LayoutConstraint::tight(Size::new(150.0, 26.0)));

        let button_rect = popup.rect();
        let dropdown = popup.dropdown_rect();
        assert_eq!(dropdown.x, button_rect.x);
        assert_eq!(dropdown.y, button_rect.y + button_rect.height);
        assert_eq!(dropdown.width, button_rect.width);
        assert_eq!(dropdown.height, 3.0 * ITEM_HEIGHT);

        assert_eq!(popup.item_rects().len(), 3);
        assert_eq!(popup.item_rect(0).unwrap().y, dropdown.y);
        assert_eq!(popup.item_rect(1).unwrap().y, dropdown.y + ITEM_HEIGHT);
        assert!(popup.item_rect(3).is_none());
    }

    #[test]
    fn click_inside_button_rect_toggles_open() {
        let mut popup = test_popup();
        popup.layout(LayoutConstraint::tight(Size::new(150.0, 26.0)));
        let inside = Point::new(popup.rect().x + 4.0, popup.rect().y + 4.0);

        assert!(!popup.open);
        assert!(matches!(
            popup.handle_event(&mouse_down(inside)),
            EventResult::Handled
        ));
        assert!(popup.open);

        // Clicking the button again while open closes it.
        assert!(matches!(
            popup.handle_event(&mouse_down(inside)),
            EventResult::Handled
        ));
        assert!(!popup.open);
    }

    #[test]
    fn click_elsewhere_while_closed_is_ignored() {
        let mut popup = test_popup();
        popup.layout(LayoutConstraint::tight(Size::new(150.0, 26.0)));
        let far_away = Point::new(9000.0, 9000.0);

        assert!(matches!(
            popup.handle_event(&mouse_down(far_away)),
            EventResult::Ignored
        ));
        assert!(!popup.open);
    }

    #[test]
    fn click_on_item_selects_it_and_closes() {
        let mut popup = test_popup();
        popup.layout(LayoutConstraint::tight(Size::new(150.0, 26.0)));
        popup.toggle();
        assert!(popup.open);

        let item_rect = popup.item_rect(1).unwrap();
        let point = Point::new(item_rect.x + 4.0, item_rect.y + 4.0);

        assert!(matches!(
            popup.handle_event(&mouse_down(point)),
            EventResult::Handled
        ));
        assert!(!popup.open, "selecting an item closes the popup");
        assert_eq!(popup.selected_index, 1);
        assert_eq!(popup.take_selected(), Some(1));
        assert_eq!(popup.take_selected(), None, "selection is drained once");
    }

    #[test]
    fn on_select_callback_fires_with_the_chosen_index() {
        use std::sync::{Arc, Mutex};

        let mut popup = test_popup();
        popup.layout(LayoutConstraint::tight(Size::new(150.0, 26.0)));
        popup.toggle();

        let seen: Arc<Mutex<Option<usize>>> = Arc::new(Mutex::new(None));
        let seen_in_closure = Arc::clone(&seen);
        popup.on_select = Some(Box::new(move |index: usize| {
            *seen_in_closure.lock().unwrap() = Some(index);
        }));

        let item_rect = popup.item_rect(2).unwrap();
        let point = Point::new(item_rect.x + 2.0, item_rect.y + 2.0);
        let _ = popup.handle_event(&mouse_down(point));

        assert_eq!(*seen.lock().unwrap(), Some(2));
    }

    #[test]
    fn click_outside_while_open_closes_without_selecting() {
        let mut popup = test_popup();
        popup.layout(LayoutConstraint::tight(Size::new(150.0, 26.0)));
        popup.toggle();
        assert!(popup.open);

        let outside = Point::new(9000.0, 9000.0);
        assert!(matches!(
            popup.handle_event(&mouse_down(outside)),
            EventResult::Handled
        ));
        assert!(!popup.open);
        assert_eq!(popup.take_selected(), None);
    }
}
