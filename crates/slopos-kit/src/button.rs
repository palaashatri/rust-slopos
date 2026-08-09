use crate::{
    event::{KeyCode, MouseButton},
    measure_text_width,
    theme::ThemeContext,
    AccessibilityNode, AccessibilityRole, Event, EventResult, LayoutConstraint, Rect, Size,
    Visibility, Widget, WidgetState,
};

pub struct Button {
    state: WidgetState,
    pub label: String,
    pub checked: bool,
    pressed: bool,
    /// Set (and drained) on `MouseUp` inside the rect while `pressed`. Backs
    /// `take_clicked()`, the polling migration lever for apps that are not
    /// yet wired up to `on_click`.
    clicked: bool,
    on_click: Option<Box<dyn FnMut() + Send>>,
}

impl Button {
    pub fn new<S: Into<String>>(label: S) -> Self {
        Self {
            state: WidgetState::new(),
            label: label.into(),
            checked: false,
            pressed: false,
            clicked: false,
            on_click: None,
        }
    }

    /// Register a callback fired on activation (`MouseUp` inside the rect
    /// while pressed). Chainable builder, e.g. `Button::new("OK").on_click(...)`.
    pub fn on_click(mut self, f: impl FnMut() + Send + 'static) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }

    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn set_label<S: Into<String>>(&mut self, label: S) {
        self.label = label.into();
    }

    /// Returns `true` exactly once per activation, then resets. Lets an app's
    /// `update()` poll for clicks instead of registering `on_click`.
    pub fn take_clicked(&mut self) -> bool {
        std::mem::take(&mut self.clicked)
    }
}

impl Widget for Button {
    fn widget_state(&self) -> &WidgetState {
        &self.state
    }
    fn widget_state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        let width = measure_text_width(&self.label) + 20.0;
        let height = 28.0;
        let size = constraint.clamp(Size::new(width, height));
        self.set_rect(Rect::new(
            self.rect().x,
            self.rect().y,
            size.width,
            size.height,
        ));
        size
    }

    fn draw(&self, theme: &ThemeContext) {
        let _bg = if self.state.hovered {
            theme.color(crate::ThemeToken::ButtonHighlight)
        } else {
            theme.color(crate::ThemeToken::ButtonBackground)
        };
        let _text_color = theme.color(crate::ThemeToken::ButtonText);
    }

    fn handle_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::MouseDown {
                button: MouseButton::Left,
                point,
                ..
            } => {
                if !self.rect().contains(*point) {
                    return EventResult::Ignored;
                }
                self.state.hovered = true;
                self.pressed = true;
                EventResult::Handled
            }
            Event::MouseUp {
                button: MouseButton::Left,
                point,
                ..
            } => {
                if !self.rect().contains(*point) {
                    // A release outside only reaches us through pointer
                    // capture; the press is over either way, and it must not
                    // fire later if some future release lands inside.
                    self.pressed = false;
                    return EventResult::Ignored;
                }
                if self.pressed {
                    self.pressed = false;
                    self.clicked = true;
                    if let Some(cb) = self.on_click.as_mut() {
                        cb();
                    }
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
            Event::MouseMove { point, .. } => {
                if !self.rect().contains(*point) {
                    self.state.hovered = false;
                    return EventResult::Ignored;
                }
                self.state.hovered = true;
                EventResult::Handled
            }
            Event::MouseEnter => {
                self.state.hovered = true;
                EventResult::Handled
            }
            Event::MouseLeave => {
                self.state.hovered = false;
                self.pressed = false;
                EventResult::Handled
            }
            Event::KeyDown {
                key: KeyCode::Enter | KeyCode::Space,
                ..
            } => {
                // Keyboard activation. Keys are routed here by
                // `FocusManager::dispatch_key`, but gate on `focused` anyway
                // so a container that broadcasts keys can't trigger every
                // button at once.
                if !self.state.focused {
                    return EventResult::Ignored;
                }
                self.clicked = true;
                if let Some(cb) = self.on_click.as_mut() {
                    cb();
                }
                EventResult::Handled
            }
            _ => EventResult::Ignored,
        }
    }

    fn focusable(&self) -> bool {
        self.state.enabled && self.state.visibility == Visibility::Visible
    }

    fn accessibility(&self) -> Option<AccessibilityNode> {
        Some(AccessibilityNode::new(
            AccessibilityRole::Button,
            &self.label,
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
    use crate::{event::Modifiers, Point};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn down(x: f32, y: f32) -> Event {
        Event::MouseDown {
            button: MouseButton::Left,
            point: Point::new(x, y),
            modifiers: Modifiers::NONE,
        }
    }
    fn up(x: f32, y: f32) -> Event {
        Event::MouseUp {
            button: MouseButton::Left,
            point: Point::new(x, y),
            modifiers: Modifiers::NONE,
        }
    }

    #[test]
    fn layout_uses_shaped_label_width_and_preserves_button_padding() {
        let label = "Wiii 日本語";
        let mut button = Button::new(label);
        let size = button.layout(LayoutConstraint::UNBOUNDED);

        assert!((size.width - (measure_text_width(label) + 20.0)).abs() < 0.01);
        assert_eq!(size.height, 28.0);

        let old_estimate = label.chars().count() as f32 * 7.5 + 20.0;
        assert!(
            (size.width - old_estimate).abs() > 0.5,
            "button geometry must not use the fixed per-character width estimate"
        );
    }

    #[test]
    fn click_outside_rect_is_ignored_and_does_not_press() {
        let mut button = Button::new("OK");
        button.set_rect(Rect::new(0.0, 0.0, 40.0, 28.0));

        assert!(matches!(
            button.handle_event(&down(100.0, 100.0)),
            EventResult::Ignored
        ));
        assert!(!button.pressed);

        // A stray MouseUp inside, with no prior press, must not fire.
        assert!(matches!(
            button.handle_event(&up(10.0, 10.0)),
            EventResult::Ignored
        ));
        assert!(!button.take_clicked());
    }

    #[test]
    fn press_then_release_inside_fires_on_click_and_sets_clicked() {
        let count = Arc::new(AtomicUsize::new(0));
        let counter = count.clone();
        let mut button = Button::new("OK").on_click(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        button.set_rect(Rect::new(0.0, 0.0, 40.0, 28.0));

        assert!(matches!(
            button.handle_event(&down(10.0, 10.0)),
            EventResult::Handled
        ));
        assert!(button.pressed);
        assert!(matches!(
            button.handle_event(&up(10.0, 10.0)),
            EventResult::Handled
        ));
        assert!(!button.pressed);
        assert_eq!(count.load(Ordering::SeqCst), 1);

        // take_clicked() drains exactly once.
        assert!(button.take_clicked());
        assert!(!button.take_clicked());
    }

    #[test]
    fn release_outside_rect_is_ignored_and_does_not_fire() {
        let count = Arc::new(AtomicUsize::new(0));
        let counter = count.clone();
        let mut button = Button::new("OK").on_click(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        button.set_rect(Rect::new(0.0, 0.0, 40.0, 28.0));

        button.handle_event(&down(10.0, 10.0));
        assert!(button.pressed);
        assert!(matches!(
            button.handle_event(&up(500.0, 500.0)),
            EventResult::Ignored
        ));
        assert_eq!(count.load(Ordering::SeqCst), 0);
        assert!(!button.take_clicked());
    }

    #[test]
    fn mouse_leave_cancels_press() {
        let count = Arc::new(AtomicUsize::new(0));
        let counter = count.clone();
        let mut button = Button::new("OK").on_click(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        button.set_rect(Rect::new(0.0, 0.0, 40.0, 28.0));

        button.handle_event(&down(10.0, 10.0));
        assert!(button.pressed);
        assert!(matches!(
            button.handle_event(&Event::MouseLeave),
            EventResult::Handled
        ));
        assert!(!button.pressed);

        // Even if a MouseUp later lands back inside the rect, the cancelled
        // press must not fire.
        assert!(matches!(
            button.handle_event(&up(10.0, 10.0)),
            EventResult::Ignored
        ));
        assert_eq!(count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn release_outside_cancels_press_so_a_later_inside_release_cannot_fire() {
        let mut button = Button::new("OK");
        button.set_rect(Rect::new(0.0, 0.0, 40.0, 28.0));

        // Press inside, release outside (delivered via pointer capture).
        button.handle_event(&down(10.0, 10.0));
        assert!(matches!(
            button.handle_event(&up(500.0, 500.0)),
            EventResult::Ignored
        ));
        assert!(
            !button.pressed,
            "press must be cancelled, not left dangling"
        );

        // A later unrelated release inside must not fire the stale press.
        assert!(matches!(
            button.handle_event(&up(10.0, 10.0)),
            EventResult::Ignored
        ));
        assert!(!button.take_clicked());
    }

    #[test]
    fn button_is_focusable_unless_hidden_or_disabled() {
        let mut button = Button::new("OK");
        assert!(button.focusable());

        button.widget_state_mut().enabled = false;
        assert!(!button.focusable());

        button.widget_state_mut().enabled = true;
        button.widget_state_mut().visibility = crate::Visibility::Hidden;
        assert!(!button.focusable());
    }

    #[test]
    fn enter_and_space_activate_only_when_focused() {
        let count = Arc::new(AtomicUsize::new(0));
        let counter = count.clone();
        let mut button = Button::new("OK").on_click(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });

        let enter = Event::KeyDown {
            key: crate::event::KeyCode::Enter,
            modifiers: Modifiers::NONE,
        };
        let space = Event::KeyDown {
            key: crate::event::KeyCode::Space,
            modifiers: Modifiers::NONE,
        };

        // Unfocused: a broadcast key must not trigger the button.
        assert!(matches!(button.handle_event(&enter), EventResult::Ignored));
        assert_eq!(count.load(Ordering::SeqCst), 0);

        button.widget_state_mut().focused = true;
        assert!(matches!(button.handle_event(&enter), EventResult::Handled));
        assert!(matches!(button.handle_event(&space), EventResult::Handled));
        assert_eq!(count.load(Ordering::SeqCst), 2);
        assert!(button.take_clicked());
    }

    #[test]
    fn mouse_move_gates_hover_on_rect() {
        let mut button = Button::new("OK");
        button.set_rect(Rect::new(0.0, 0.0, 40.0, 28.0));

        let inside = Event::MouseMove {
            point: Point::new(10.0, 10.0),
            modifiers: Modifiers::NONE,
        };
        assert!(matches!(button.handle_event(&inside), EventResult::Handled));
        assert!(button.state.hovered);

        let outside = Event::MouseMove {
            point: Point::new(500.0, 500.0),
            modifiers: Modifiers::NONE,
        };
        assert!(matches!(
            button.handle_event(&outside),
            EventResult::Ignored
        ));
        assert!(!button.state.hovered);
    }
}
