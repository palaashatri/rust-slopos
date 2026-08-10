use crate::{
    event::MouseButton, theme::ThemeContext, AccessibilityNode, AccessibilityRole, Event,
    EventResult, LayoutConstraint, Rect, Size, Widget, WidgetState,
};

pub struct Slider {
    state: WidgetState,
    pub value: f32,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub dragging: bool,
}

impl Default for Slider {
    fn default() -> Self {
        Self::new()
    }
}

impl Slider {
    pub fn new() -> Self {
        Self {
            state: WidgetState::new(),
            value: 0.5,
            min: 0.0,
            max: 1.0,
            step: 0.01,
            dragging: false,
        }
    }

    pub fn normalized_value(&self) -> f32 {
        let range = self.max - self.min;
        if range <= f32::EPSILON {
            return 0.0;
        }
        ((self.value - self.min) / range).clamp(0.0, 1.0)
    }

    pub fn set_value(&mut self, value: f32) {
        let mut next = value.clamp(self.min, self.max);
        if self.step > f32::EPSILON {
            next = ((next - self.min) / self.step).round() * self.step + self.min;
        }
        self.value = next.clamp(self.min, self.max);
    }

    fn set_from_point(&mut self, x: f32) {
        let rect = self.rect();
        let usable = (rect.width - 18.0).max(1.0);
        let normalized = ((x - rect.x - 9.0) / usable).clamp(0.0, 1.0);
        self.set_value(self.min + normalized * (self.max - self.min));
    }
}

impl Widget for Slider {
    fn widget_state(&self) -> &WidgetState {
        &self.state
    }
    fn widget_state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        let width = constraint.max_width.min(150.0);
        let height = 24.0;
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
        match event {
            Event::MouseDown {
                button: MouseButton::Left,
                point,
                ..
            } if self.rect().contains(*point) => {
                self.dragging = true;
                self.set_from_point(point.x);
                EventResult::Handled
            }
            Event::MouseMove { point, .. } if self.dragging => {
                self.set_from_point(point.x);
                EventResult::Handled
            }
            Event::MouseUp {
                button: MouseButton::Left,
                point,
                ..
            } if self.dragging => {
                self.dragging = false;
                self.set_from_point(point.x);
                EventResult::Handled
            }
            Event::MouseLeave if self.dragging => {
                self.dragging = false;
                EventResult::Handled
            }
            _ => EventResult::Ignored,
        }
    }

    fn accessibility(&self) -> Option<AccessibilityNode> {
        Some(AccessibilityNode::new(AccessibilityRole::Slider, "slider"))
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
    use crate::{event::Modifiers, Point};

    #[test]
    fn slider_snaps_and_clamps_values() {
        let mut slider = Slider::new();
        slider.min = 0.0;
        slider.max = 10.0;
        slider.step = 2.0;

        slider.set_value(7.1);
        assert_eq!(slider.value, 8.0);

        slider.set_value(-5.0);
        assert_eq!(slider.value, 0.0);

        slider.set_value(99.0);
        assert_eq!(slider.value, 10.0);
    }

    #[test]
    fn slider_updates_while_dragging() {
        let mut slider = Slider::new();
        slider.min = 0.0;
        slider.max = 100.0;
        slider.step = 1.0;
        slider.set_rect(Rect::new(10.0, 10.0, 118.0, 24.0));

        assert!(matches!(
            slider.handle_event(&Event::MouseDown {
                button: MouseButton::Left,
                point: Point::new(69.0, 20.0),
                modifiers: Modifiers::NONE,
            }),
            EventResult::Handled
        ));
        assert!(slider.dragging);
        assert_eq!(slider.value, 50.0);

        assert!(matches!(
            slider.handle_event(&Event::MouseMove {
                point: Point::new(119.0, 20.0),
                modifiers: Modifiers::NONE,
            }),
            EventResult::Handled
        ));
        assert_eq!(slider.value, 100.0);

        assert!(matches!(
            slider.handle_event(&Event::MouseUp {
                button: MouseButton::Left,
                point: Point::new(19.0, 20.0),
                modifiers: Modifiers::NONE,
            }),
            EventResult::Handled
        ));
        assert!(!slider.dragging);
        assert_eq!(slider.value, 0.0);
    }
}
