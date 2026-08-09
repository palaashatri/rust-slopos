use crate::{
    dispatch::dispatch_positional,
    event::{KeyCode, Modifiers, MouseButton},
    measure_text_width,
    theme::ThemeContext,
    AccessibilityNode, AccessibilityRole, Button, Event, EventResult, LayoutConstraint, Point,
    Rect, Size, Widget, WidgetState,
};

/// Pointer location carried by positional event variants. Events with no
/// on-screen position (`MouseEnter`, `KeyDown`, ...) have nothing to
/// rect-check against and are left `Ignored` — same as the `Widget` trait
/// default. Mirrors the identical helper in `toolbar.rs` / `split_view.rs`;
/// kept local rather than shared because `dispatch.rs` isn't owned here.
fn positional_point(event: &Event) -> Option<Point> {
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

pub struct Dialog {
    state: WidgetState,
    pub title: String,
    pub message: String,
    pub buttons: Vec<Button>,
    /// Geometry of each button in `buttons` (same order, same length),
    /// computed by `layout()`. Kept in sync with each `Button`'s own rect so
    /// `Button::handle_event`'s internal rect check works when events are
    /// forwarded to it.
    button_rects: Vec<Rect>,
    /// Index into `buttons` of the most recent activation — a click, or
    /// Enter/Escape mapped to the default/cancel button. Drained by
    /// `take_result`.
    result: Option<usize>,
}

impl Dialog {
    pub fn new<S: Into<String>>(title: S, message: S) -> Self {
        Self {
            state: WidgetState::new(),
            title: title.into(),
            message: message.into(),
            buttons: vec![],
            button_rects: vec![],
            result: None,
        }
    }

    pub fn add_button(&mut self, label: &str) {
        self.buttons.push(Button::new(label));
    }

    /// Geometry of each button in `buttons` (same order, same length).
    pub fn button_rects(&self) -> &[Rect] {
        &self.button_rects
    }

    /// Drains the result of the most recent button activation. `None` once
    /// read, until another click or Enter/Escape produces one.
    pub fn take_result(&mut self) -> Option<usize> {
        self.result.take()
    }

    /// `Enter` activates this button: the last one added. `draw_dialog` in
    /// `slopos-sdk` lays buttons out back-to-front from the last, so the last
    /// button is rightmost — the conventional default/affirmative slot.
    fn default_button(&self) -> Option<usize> {
        self.buttons.len().checked_sub(1)
    }

    /// `Escape` activates this button: the first one added — the leftmost
    /// slot, conventionally "Cancel". A single-button dialog has only one
    /// button, so it serves as both default and cancel.
    fn cancel_button(&self) -> Option<usize> {
        if self.buttons.is_empty() {
            None
        } else {
            Some(0)
        }
    }

    /// Drive `buttons[index]` through a synthetic press+release at its own
    /// (already laid-out) rect center, so keyboard activation goes through
    /// exactly the same path as a real click — `Button`'s own `pressed` /
    /// `on_click` / `take_clicked` — without Dialog needing any privileged
    /// access to `Button`'s internals.
    fn activate(&mut self, index: usize) {
        let Some(btn) = self.buttons.get_mut(index) else {
            return;
        };
        let rect = btn.rect();
        let point = Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
        let _ = btn.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point,
            modifiers: Modifiers::NONE,
        });
        let _ = btn.handle_event(&Event::MouseUp {
            button: MouseButton::Left,
            point,
            modifiers: Modifiers::NONE,
        });
        if btn.take_clicked() {
            self.result = Some(index);
        }
    }
}

impl Widget for Dialog {
    fn widget_state(&self) -> &WidgetState {
        &self.state
    }
    fn widget_state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        let width = 400.0;
        let height = 150.0;
        let size = constraint.clamp(Size::new(width, height));
        self.set_rect(Rect::new(
            self.rect().x,
            self.rect().y,
            size.width,
            size.height,
        ));

        // Mirror `slopos-sdk`'s `draw_dialog` button geometry exactly (right
        // aligned, 24px tall, 10px inset from the edge, 8px gaps, laid out
        // back-to-front starting from the last button) so hit-testing lines
        // up with what is actually painted.
        let rect = self.rect();
        let btn_h = 24.0;
        let btn_y = rect.y + rect.height - btn_h - 10.0;
        let mut btn_x = rect.x + rect.width - 10.0;
        let mut rects = vec![Rect::ZERO; self.buttons.len()];
        for (index, button) in self.buttons.iter().enumerate().rev() {
            let label = button.label();
            let btn_w = (measure_text_width(label) + 20.0).max(72.0);
            btn_x -= btn_w;
            rects[index] = Rect::new(btn_x, btn_y, btn_w, btn_h);
            btn_x -= 8.0;
        }
        for (button, rect) in self.buttons.iter_mut().zip(rects.iter()) {
            button.set_rect(*rect);
        }
        self.button_rects = rects;

        size
    }

    fn draw(&self, _theme: &ThemeContext) {}

    // Was: no override at all, inheriting the `Widget` trait default
    // (`Ignored`) — no button was ever clickable and Enter/Escape did
    // nothing (see AGENTS.md, P2/P5). Enter/Escape resolve to the
    // default/cancel button; positional events are hit-tested against
    // `buttons` via the same `dispatch_positional` helper `Toolbar` and
    // `SplitView` use, then `take_clicked()` (Button's own polling lever)
    // says which button, if any, just fired.
    fn handle_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::KeyDown {
                key: KeyCode::Enter,
                ..
            } => {
                if let Some(index) = self.default_button() {
                    self.activate(index);
                }
                EventResult::Handled
            }
            Event::KeyDown {
                key: KeyCode::Escape,
                ..
            } => {
                if let Some(index) = self.cancel_button() {
                    self.activate(index);
                }
                EventResult::Handled
            }
            _ => {
                let Some(at) = positional_point(event) else {
                    return EventResult::Ignored;
                };
                if !self.rect().contains(at) {
                    return EventResult::Ignored;
                }
                let mut children: Vec<&mut dyn Widget> = self
                    .buttons
                    .iter_mut()
                    .map(|b| b as &mut dyn Widget)
                    .collect();
                let result = dispatch_positional(&mut children, at, event);
                if !matches!(result, EventResult::Ignored) {
                    for (index, button) in self.buttons.iter_mut().enumerate() {
                        if button.take_clicked() {
                            self.result = Some(index);
                            break;
                        }
                    }
                }
                result
            }
        }
    }

    fn accessibility(&self) -> Option<AccessibilityNode> {
        Some(AccessibilityNode::new(
            AccessibilityRole::Dialog,
            &self.title,
        ))
    }

    fn children(&self) -> Vec<&dyn Widget> {
        self.buttons.iter().map(|b| b as &dyn Widget).collect()
    }

    fn children_mut(&mut self) -> Vec<&mut dyn Widget> {
        self.buttons
            .iter_mut()
            .map(|b| b as &mut dyn Widget)
            .collect()
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
    fn layout_computes_button_rects_right_aligned_like_the_painter() {
        let mut dialog = Dialog::new("Confirm", "Are you sure?");
        dialog.add_button("Cancel");
        dialog.add_button("OK");
        dialog.layout(LayoutConstraint::UNBOUNDED);

        let rect = dialog.rect();
        let rects = dialog.button_rects().to_vec();
        assert_eq!(rects.len(), 2);

        // "OK" (last-added) is rightmost, flush 10px from the dialog's edge.
        assert_eq!(rects[1].x + rects[1].width, rect.x + rect.width - 10.0);
        // "Cancel" sits to its left with an 8px gap.
        assert_eq!(rects[1].x - (rects[0].x + rects[0].width), 8.0);
        assert_eq!(rects[0].height, 24.0);
        assert_eq!(rects[1].height, 24.0);

        // Each Button's own rect is kept in sync so its rect-gated handling works.
        assert_eq!(dialog.buttons[0].rect().x, rects[0].x);
        assert_eq!(dialog.buttons[1].rect().x, rects[1].x);
    }

    #[test]
    fn layout_button_width_uses_shaped_unicode_label_width() {
        let label = "日本語".repeat(6);
        let mut dialog = Dialog::new("Confirm", "Are you sure?");
        dialog.add_button(&label);
        dialog.layout(LayoutConstraint::UNBOUNDED);

        let measured_width = measure_text_width(&label);
        let actual = dialog.button_rects()[0].width;
        assert!((actual - (measured_width + 20.0).max(72.0)).abs() < 0.01);

        let byte_count_estimate = label.len() as f32 * 7.0 + 20.0;
        assert!(
            (actual - byte_count_estimate).abs() > 0.5,
            "dialog button geometry must not use a UTF-8 byte-count estimate"
        );
    }

    #[test]
    fn click_on_button_sets_take_result_on_release() {
        let mut dialog = Dialog::new("Confirm", "Are you sure?");
        dialog.add_button("Cancel");
        dialog.add_button("OK");
        dialog.layout(LayoutConstraint::UNBOUNDED);

        let ok_rect = dialog.button_rects()[1];
        let point = Point::new(
            ok_rect.x + ok_rect.width / 2.0,
            ok_rect.y + ok_rect.height / 2.0,
        );

        assert!(matches!(
            dialog.handle_event(&Event::MouseDown {
                button: MouseButton::Left,
                point,
                modifiers: Modifiers::NONE,
            }),
            EventResult::Handled
        ));
        assert_eq!(
            dialog.take_result(),
            None,
            "activation fires on release, not press"
        );

        assert!(matches!(
            dialog.handle_event(&Event::MouseUp {
                button: MouseButton::Left,
                point,
                modifiers: Modifiers::NONE,
            }),
            EventResult::Handled
        ));
        assert_eq!(dialog.take_result(), Some(1));
        assert_eq!(dialog.take_result(), None, "drains exactly once");
    }

    #[test]
    fn click_outside_every_button_is_ignored() {
        let mut dialog = Dialog::new("Confirm", "Are you sure?");
        dialog.add_button("OK");
        dialog.layout(LayoutConstraint::UNBOUNDED);

        // Inside the dialog rect, but nowhere near the button.
        let result = dialog.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point: Point::new(5.0, 5.0),
            modifiers: Modifiers::NONE,
        });

        assert!(matches!(result, EventResult::Ignored));
        assert_eq!(dialog.take_result(), None);
    }

    #[test]
    fn events_with_no_position_are_ignored() {
        let mut dialog = Dialog::new("Confirm", "Are you sure?");
        dialog.add_button("OK");
        dialog.layout(LayoutConstraint::UNBOUNDED);

        assert!(matches!(
            dialog.handle_event(&Event::MouseEnter),
            EventResult::Ignored
        ));
    }

    #[test]
    fn enter_activates_default_button_and_escape_activates_cancel_button() {
        let mut dialog = Dialog::new("Confirm", "Are you sure?");
        dialog.add_button("Cancel");
        dialog.add_button("OK");
        dialog.layout(LayoutConstraint::UNBOUNDED);

        assert!(matches!(
            dialog.handle_event(&Event::KeyDown {
                key: KeyCode::Enter,
                modifiers: Modifiers::NONE,
            }),
            EventResult::Handled
        ));
        assert_eq!(
            dialog.take_result(),
            Some(1),
            "Enter activates the last (rightmost/default) button"
        );

        assert!(matches!(
            dialog.handle_event(&Event::KeyDown {
                key: KeyCode::Escape,
                modifiers: Modifiers::NONE,
            }),
            EventResult::Handled
        ));
        assert_eq!(
            dialog.take_result(),
            Some(0),
            "Escape activates the first (leftmost/cancel) button"
        );
    }

    #[test]
    fn single_button_dialog_is_both_default_and_cancel() {
        let mut dialog = Dialog::new("Notice", "Saved.");
        dialog.add_button("OK");
        dialog.layout(LayoutConstraint::UNBOUNDED);

        let _ = dialog.handle_event(&Event::KeyDown {
            key: KeyCode::Enter,
            modifiers: Modifiers::NONE,
        });
        assert_eq!(dialog.take_result(), Some(0));

        let _ = dialog.handle_event(&Event::KeyDown {
            key: KeyCode::Escape,
            modifiers: Modifiers::NONE,
        });
        assert_eq!(dialog.take_result(), Some(0));
    }

    #[test]
    fn dialog_with_no_buttons_does_not_panic_on_enter_or_escape() {
        let mut dialog = Dialog::new("Empty", "No buttons.");
        dialog.layout(LayoutConstraint::UNBOUNDED);

        assert!(matches!(
            dialog.handle_event(&Event::KeyDown {
                key: KeyCode::Enter,
                modifiers: Modifiers::NONE,
            }),
            EventResult::Handled
        ));
        assert_eq!(dialog.take_result(), None);

        assert!(matches!(
            dialog.handle_event(&Event::KeyDown {
                key: KeyCode::Escape,
                modifiers: Modifiers::NONE,
            }),
            EventResult::Handled
        ));
        assert_eq!(dialog.take_result(), None);
    }
}
