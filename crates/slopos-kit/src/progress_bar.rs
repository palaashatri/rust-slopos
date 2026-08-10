use crate::{
    theme::ThemeContext, AccessibilityNode, AccessibilityRole, LayoutConstraint, Rect, Size,
    Widget, WidgetState,
};

pub struct ProgressBar {
    state: WidgetState,
    pub value: f32,
    pub max: f32,
    pub indeterminate: bool,
}

impl Default for ProgressBar {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressBar {
    pub fn new() -> Self {
        Self {
            state: WidgetState::new(),
            value: 0.0,
            max: 100.0,
            indeterminate: false,
        }
    }
}

impl Widget for ProgressBar {
    fn widget_state(&self) -> &WidgetState {
        &self.state
    }
    fn widget_state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        let width = constraint.max_width.min(200.0);
        let height = 12.0;
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

    fn accessibility(&self) -> Option<AccessibilityNode> {
        Some(AccessibilityNode::new(
            AccessibilityRole::ProgressBar,
            "progress",
        ))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
