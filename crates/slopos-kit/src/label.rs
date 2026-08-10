use crate::{
    measure_text_width, theme::ThemeContext, AccessibilityNode, AccessibilityRole,
    LayoutConstraint, Rect, Size, Widget, WidgetState,
};

pub struct Label {
    state: WidgetState,
    pub text: String,
}

impl Label {
    pub fn new<S: Into<String>>(text: S) -> Self {
        Self {
            state: WidgetState::new(),
            text: text.into(),
        }
    }
}

impl Widget for Label {
    fn widget_state(&self) -> &WidgetState {
        &self.state
    }
    fn widget_state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        // The presenter paints labels with a 2px leading inset.  Reserve the
        // same inset on both sides and measure shaped glyph advances rather
        // than UTF-8 byte counts, so variable-width and localized text gets a
        // natural rect that cannot overlap its sibling.
        let width = measure_text_width(&self.text) + 4.0;
        let height = 20.0;
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
            AccessibilityRole::StaticText,
            &self.text,
        ))
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

    #[test]
    fn layout_uses_shaped_label_width_and_paint_insets() {
        let text = "Wiii 日本語";
        let mut label = Label::new(text);
        let size = label.layout(LayoutConstraint::UNBOUNDED);

        let expected = measure_text_width(text) + 4.0;
        assert!((size.width - expected).abs() < 0.01);

        let byte_count_estimate = text.len() as f32 * 8.0;
        assert!(
            (size.width - byte_count_estimate).abs() > 0.5,
            "label geometry must not use a UTF-8 byte-count estimate"
        );
    }
}
