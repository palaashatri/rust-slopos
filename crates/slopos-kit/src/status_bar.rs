use crate::theme::ThemeContext;
use crate::{
    AccessibilityNode, AccessibilityRole, Event, EventResult, LayoutConstraint, Rect, Size, Widget,
    WidgetState,
};
use std::any::Any;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusBarAlignment {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone)]
pub struct StatusBarItem {
    pub text: String,
    pub icon: Option<String>,
    pub alignment: StatusBarAlignment,
    pub width: f32,
}

pub struct StatusBar {
    state: WidgetState,
    pub items: Vec<StatusBarItem>,
}

impl Default for StatusBar {
    fn default() -> Self {
        Self::new()
    }
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
            state: WidgetState::new(),
            items: vec![],
        }
    }

    pub fn add_item(&mut self, text: &str, alignment: StatusBarAlignment, width: f32) {
        self.items.push(StatusBarItem {
            text: text.to_string(),
            icon: None,
            alignment,
            width,
        });
    }
}

impl Widget for StatusBar {
    fn widget_state(&self) -> &WidgetState {
        &self.state
    }
    fn widget_state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        let size = constraint.clamp(Size::new(constraint.max_width, 24.0));
        self.set_rect(Rect::new(
            self.rect().x,
            self.rect().y,
            size.width,
            size.height,
        ));
        size
    }

    fn draw(&self, _theme: &ThemeContext) {}

    fn handle_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }

    fn accessibility(&self) -> Option<AccessibilityNode> {
        Some(AccessibilityNode::new(
            AccessibilityRole::Unknown,
            "Status Bar",
        ))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
