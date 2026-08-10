use crate::{
    theme::ThemeContext, AccessibilityNode, AccessibilityRole, LayoutConstraint, Rect, Size,
    Widget, WidgetState,
};

/// A filled rectangle used for overlays, cards, and scrims.
///
/// Actual pixels are painted by `slopos-sdk::draw_widget` (kit `draw()` is a
/// no-op by design — same as other kit widgets).
pub struct Panel {
    state: WidgetState,
    /// When true, SDK fills with theme `window_bg`; otherwise uses [`Self::fill`].
    pub themed: bool,
    pub fill: [f32; 4],
    pub beveled: bool,
    pub raised: bool,
    pub bordered: bool,
}

impl Default for Panel {
    fn default() -> Self {
        Self::new()
    }
}

impl Panel {
    pub fn new() -> Self {
        Self {
            state: WidgetState::new(),
            themed: true,
            fill: [0.9, 0.9, 0.88, 1.0],
            beveled: true,
            raised: true,
            bordered: true,
        }
    }

    /// Full-screen dimming overlay (semi-transparent black).
    pub fn scrim() -> Self {
        Self {
            state: WidgetState::new(),
            themed: false,
            fill: [0.0, 0.0, 0.0, 0.45],
            beveled: false,
            raised: false,
            bordered: false,
        }
    }

    /// Raised card / dialog-like surface using theme window background.
    pub fn card() -> Self {
        Self {
            state: WidgetState::new(),
            themed: true,
            fill: [0.9, 0.9, 0.88, 1.0],
            beveled: true,
            raised: true,
            bordered: true,
        }
    }
}

impl Widget for Panel {
    fn widget_state(&self) -> &WidgetState {
        &self.state
    }
    fn widget_state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        let size = constraint.clamp(Size::new(constraint.max_width, constraint.max_height));
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
        Some(AccessibilityNode::new(AccessibilityRole::Group, "panel"))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
