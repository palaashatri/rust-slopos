use crate::{
    event::MouseButton, theme::ThemeContext, AccessibilityNode, AccessibilityRole, Event,
    EventResult, LayoutConstraint, Rect, Size, Widget, WidgetState,
};

pub struct ListView {
    state: WidgetState,
    pub items: Vec<String>,
    pub selected_index: Option<usize>,
    pub multi_select: bool,
    pub on_select: Option<Box<dyn FnMut(Option<usize>) + Send>>,
}

impl Default for ListView {
    fn default() -> Self {
        Self::new()
    }
}

impl ListView {
    pub fn new() -> Self {
        Self {
            state: WidgetState::new(),
            items: vec![],
            selected_index: None,
            multi_select: false,
            on_select: None,
        }
    }

    pub fn with_items(mut self, items: Vec<String>) -> Self {
        self.items = items;
        self
    }

    pub fn add_item<S: Into<String>>(&mut self, item: S) {
        self.items.push(item.into());
    }
}

impl Widget for ListView {
    fn widget_state(&self) -> &WidgetState {
        &self.state
    }
    fn widget_state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        let width = constraint.max_width.min(200.0);
        let height = constraint.max_height.min(300.0);
        let size = constraint.clamp(Size::new(width, height));
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
        if let Event::MouseDown {
            button: MouseButton::Left,
            point,
            ..
        } = event
        {
            if !self.rect().contains(*point) {
                return EventResult::Ignored;
            }

            let row = ((point.y - self.rect().y - 3.0) / 18.0).floor() as usize;
            if row < self.items.len() {
                self.selected_index = Some(row);
                if let Some(on_select) = &mut self.on_select {
                    on_select(self.selected_index);
                }
                return EventResult::Handled;
            }
        }

        EventResult::Ignored
    }

    fn accessibility(&self) -> Option<AccessibilityNode> {
        let mut node = AccessibilityNode::new(AccessibilityRole::List, "list");
        for item in &self.items {
            node.children
                .push(AccessibilityNode::new(AccessibilityRole::ListItem, item));
        }
        Some(node)
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
    use crate::{event::Modifiers, LayoutConstraint, Point};

    #[test]
    fn list_view_selects_clicked_row() {
        let mut list = ListView::new().with_items(vec![
            "first".to_string(),
            "second".to_string(),
            "third".to_string(),
        ]);
        list.layout(LayoutConstraint::tight(Size::new(200.0, 120.0)));

        let result = list.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point: Point::new(10.0, 25.0),
            modifiers: Modifiers::NONE,
        });

        assert!(matches!(result, EventResult::Handled));
        assert_eq!(list.selected_index, Some(1));
    }
}
