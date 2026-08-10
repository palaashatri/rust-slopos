use crate::{
    dispatch::dispatch_positional, event::MouseButton, theme::ThemeContext, CursorStyle, Event,
    EventResult, LayoutConstraint, Point, Rect, Size, Widget, WidgetState,
};

/// Keeps both panes usably sized: dragging the divider can never collapse
/// either side to nothing.
const MIN_DIVIDER_POSITION: f32 = 0.1;
const MAX_DIVIDER_POSITION: f32 = 0.9;

/// Pointer location carried by positional event variants. Events with no
/// on-screen position (`MouseEnter`, `KeyDown`, ...) have nothing to
/// rect-check against and are left `Ignored` — same as the `Widget` trait
/// default.
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

pub enum SplitDirection {
    Horizontal,
    Vertical,
}

pub struct SplitView {
    state: WidgetState,
    pub first: Option<Box<dyn Widget>>,
    pub second: Option<Box<dyn Widget>>,
    pub direction: SplitDirection,
    pub divider_position: f32,
    pub divider_size: f32,
    /// Set while the divider is being dragged; mirrors `Slider::dragging`.
    pub dragging: bool,
}

impl SplitView {
    pub fn new(direction: SplitDirection) -> Self {
        Self {
            state: WidgetState::new(),
            first: None,
            second: None,
            direction,
            divider_position: 0.5,
            divider_size: 4.0,
            dragging: false,
        }
    }

    pub fn set_first(&mut self, widget: Box<dyn Widget>) {
        self.first = Some(widget);
    }
    pub fn set_second(&mut self, widget: Box<dyn Widget>) {
        self.second = Some(widget);
    }

    /// The divider's own hit rect, in the same coordinate space as
    /// `self.rect()`. Matches the geometry `slopos-sdk`'s painter already
    /// computes inline when drawing the divider.
    pub fn divider_rect(&self) -> Rect {
        let r = self.rect();
        match self.direction {
            SplitDirection::Horizontal => Rect::new(
                r.x + r.width * self.divider_position,
                r.y,
                self.divider_size,
                r.height,
            ),
            SplitDirection::Vertical => Rect::new(
                r.x,
                r.y + r.height * self.divider_position,
                r.width,
                self.divider_size,
            ),
        }
    }

    fn resize_cursor(&self) -> CursorStyle {
        match self.direction {
            SplitDirection::Horizontal => CursorStyle::ResizeHorizontal,
            SplitDirection::Vertical => CursorStyle::ResizeVertical,
        }
    }

    /// Recompute `divider_position` from a drag point, clamped so neither
    /// pane can be crushed to nothing.
    fn set_divider_from_point(&mut self, point: Point) {
        let r = self.rect();
        let raw = match self.direction {
            SplitDirection::Horizontal => {
                if r.width <= f32::EPSILON {
                    return;
                }
                (point.x - r.x) / r.width
            }
            SplitDirection::Vertical => {
                if r.height <= f32::EPSILON {
                    return;
                }
                (point.y - r.y) / r.height
            }
        };
        self.divider_position = raw.clamp(MIN_DIVIDER_POSITION, MAX_DIVIDER_POSITION);
    }

    /// Divider-specific hover/drag handling. Returns `Some` when the divider
    /// claims the event; `None` means "not mine", so the caller falls
    /// through to dispatching the event to `first`/`second`.
    fn handle_divider(&mut self, event: &Event) -> Option<EventResult> {
        match event {
            Event::MouseDown {
                button: MouseButton::Left,
                point,
                ..
            } if self.divider_rect().contains(*point) => {
                self.dragging = true;
                self.widget_state_mut().cursor = self.resize_cursor();
                Some(EventResult::Handled)
            }
            Event::MouseMove { point, .. } if self.dragging => {
                self.set_divider_from_point(*point);
                Some(EventResult::Handled)
            }
            Event::MouseMove { point, .. } if self.divider_rect().contains(*point) => {
                self.widget_state_mut().cursor = self.resize_cursor();
                Some(EventResult::Handled)
            }
            Event::MouseMove { .. } if self.widget_state().cursor != CursorStyle::Default => {
                // Pointer left the divider after hovering it: restore the
                // default cursor, but let the event keep going to
                // first/second — this widget didn't claim it.
                self.widget_state_mut().cursor = CursorStyle::Default;
                None
            }
            Event::MouseUp {
                button: MouseButton::Left,
                point,
                ..
            } if self.dragging => {
                self.dragging = false;
                self.set_divider_from_point(*point);
                Some(EventResult::Handled)
            }
            Event::MouseLeave if self.dragging => {
                self.dragging = false;
                self.widget_state_mut().cursor = CursorStyle::Default;
                Some(EventResult::Handled)
            }
            _ => None,
        }
    }
}

impl Widget for SplitView {
    fn widget_state(&self) -> &WidgetState {
        &self.state
    }
    fn widget_state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        let size = constraint.clamp(Size::new(constraint.max_width, constraint.max_height));
        let r = Rect::new(self.rect().x, self.rect().y, size.width, size.height);
        self.set_rect(r);

        match self.direction {
            SplitDirection::Horizontal => {
                let first_w = r.width * self.divider_position;
                let second_w = r.width - first_w - self.divider_size;
                if let Some(child) = &mut self.first {
                    child.set_rect(Rect::new(r.x, r.y, first_w, r.height));
                    let _ = child.layout(LayoutConstraint::tight(Size::new(first_w, r.height)));
                }
                if let Some(child) = &mut self.second {
                    child.set_rect(Rect::new(
                        r.x + first_w + self.divider_size,
                        r.y,
                        second_w,
                        r.height,
                    ));
                    let _ = child.layout(LayoutConstraint::tight(Size::new(second_w, r.height)));
                }
            }
            SplitDirection::Vertical => {
                let first_h = r.height * self.divider_position;
                let second_h = r.height - first_h - self.divider_size;
                if let Some(child) = &mut self.first {
                    child.set_rect(Rect::new(r.x, r.y, r.width, first_h));
                    let _ = child.layout(LayoutConstraint::tight(Size::new(r.width, first_h)));
                }
                if let Some(child) = &mut self.second {
                    child.set_rect(Rect::new(
                        r.x,
                        r.y + first_h + self.divider_size,
                        r.width,
                        second_h,
                    ));
                    let _ = child.layout(LayoutConstraint::tight(Size::new(r.width, second_h)));
                }
            }
        }

        size
    }

    fn draw(&self, theme: &ThemeContext) {
        if let Some(first) = &self.first {
            first.draw(theme);
        }
        if let Some(second) = &self.second {
            second.draw(theme);
        }
    }

    // Was: forwarded every event to both panes (second, then first) with no
    // rect check at all, so `divider_position` could never be dragged and
    // both panes fought over the same click. Divider hit-test/drag is
    // handled first; anything the divider doesn't claim is routed through
    // `dispatch_positional`, which gates on each pane's own rect (and
    // visibility/enabled) before it is asked to handle the event.
    fn handle_event(&mut self, event: &Event) -> EventResult {
        if let Some(result) = self.handle_divider(event) {
            return result;
        }
        let Some(at) = positional_point(event) else {
            return EventResult::Ignored;
        };
        if !self.rect().contains(at) {
            return EventResult::Ignored;
        }
        let mut children = self.children_mut();
        dispatch_positional(&mut children, at, event)
    }

    fn children(&self) -> Vec<&dyn Widget> {
        let mut result = vec![];
        if let Some(ref f) = self.first {
            result.push(f.as_ref());
        }
        if let Some(ref s) = self.second {
            result.push(s.as_ref());
        }
        result
    }

    fn children_mut(&mut self) -> Vec<&mut dyn Widget> {
        let mut result: Vec<&mut dyn Widget> = vec![];
        if let Some(f) = &mut self.first {
            result.push(f.as_mut());
        }
        if let Some(s) = &mut self.second {
            result.push(s.as_mut());
        }
        result
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
    use crate::button::Button;
    use crate::event::Modifiers;

    #[test]
    fn divider_rect_matches_direction_and_position() {
        let mut sv = SplitView::new(SplitDirection::Horizontal);
        let _ = sv.layout(LayoutConstraint::tight(Size::new(200.0, 100.0)));

        let divider = sv.divider_rect();
        assert_eq!(divider.x, 100.0);
        assert_eq!(divider.y, 0.0);
        assert_eq!(divider.width, 4.0);
        assert_eq!(divider.height, 100.0);
    }

    #[test]
    fn dragging_the_divider_updates_position_and_clamps_to_sane_bounds() {
        let mut sv = SplitView::new(SplitDirection::Horizontal);
        let _ = sv.layout(LayoutConstraint::tight(Size::new(200.0, 100.0)));
        let divider = sv.divider_rect();
        let grab_point = Point::new(divider.x + 1.0, 50.0);

        let down = sv.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point: grab_point,
            modifiers: Modifiers::NONE,
        });
        assert!(matches!(down, EventResult::Handled));
        assert!(sv.dragging);
        assert_eq!(sv.widget_state().cursor, CursorStyle::ResizeHorizontal);

        // Drag far enough left that the raw ratio (2 / 200 = 0.01) would
        // crush the first pane; it must clamp to the minimum instead.
        let drag_low = sv.handle_event(&Event::MouseMove {
            point: Point::new(2.0, 50.0),
            modifiers: Modifiers::NONE,
        });
        assert!(matches!(drag_low, EventResult::Handled));
        assert_eq!(sv.divider_position, MIN_DIVIDER_POSITION);

        // Drag far enough right (195 / 200 = 0.975) that it must clamp high.
        let drag_high = sv.handle_event(&Event::MouseMove {
            point: Point::new(195.0, 50.0),
            modifiers: Modifiers::NONE,
        });
        assert!(matches!(drag_high, EventResult::Handled));
        assert_eq!(sv.divider_position, MAX_DIVIDER_POSITION);

        // A point safely inside the clamp range is used as-is, and MouseUp
        // both finalizes the value and ends the drag.
        let up = sv.handle_event(&Event::MouseUp {
            button: MouseButton::Left,
            point: Point::new(150.0, 50.0),
            modifiers: Modifiers::NONE,
        });
        assert!(matches!(up, EventResult::Handled));
        assert!(!sv.dragging);
        assert_eq!(sv.divider_position, 0.75);
    }

    #[test]
    fn mouse_leave_while_dragging_cancels_the_drag_and_resets_the_cursor() {
        let mut sv = SplitView::new(SplitDirection::Vertical);
        let _ = sv.layout(LayoutConstraint::tight(Size::new(100.0, 200.0)));
        let divider = sv.divider_rect();

        let _ = sv.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point: Point::new(50.0, divider.y + 1.0),
            modifiers: Modifiers::NONE,
        });
        assert!(sv.dragging);

        let result = sv.handle_event(&Event::MouseLeave);
        assert!(matches!(result, EventResult::Handled));
        assert!(!sv.dragging);
        assert_eq!(sv.widget_state().cursor, CursorStyle::Default);
    }

    #[test]
    fn hovering_the_divider_sets_the_resize_cursor_and_moving_off_resets_it() {
        let mut sv = SplitView::new(SplitDirection::Vertical);
        let _ = sv.layout(LayoutConstraint::tight(Size::new(100.0, 200.0)));
        assert_eq!(sv.widget_state().cursor, CursorStyle::Default);

        let divider = sv.divider_rect();
        let mid = Point::new(
            divider.x + divider.width / 2.0,
            divider.y + divider.height / 2.0,
        );
        let hover = sv.handle_event(&Event::MouseMove {
            point: mid,
            modifiers: Modifiers::NONE,
        });
        assert!(matches!(hover, EventResult::Handled));
        assert_eq!(sv.widget_state().cursor, CursorStyle::ResizeVertical);

        // Move well clear of the divider: cursor resets, and since there are
        // no panes attached the event is otherwise ignored.
        let away = sv.handle_event(&Event::MouseMove {
            point: Point::new(5.0, 5.0),
            modifiers: Modifiers::NONE,
        });
        assert!(matches!(away, EventResult::Ignored));
        assert_eq!(sv.widget_state().cursor, CursorStyle::Default);
    }

    #[test]
    fn split_view_only_dispatches_a_click_to_the_pane_under_the_point() {
        let mut sv = SplitView::new(SplitDirection::Horizontal);
        sv.set_first(Box::new(Button::new("L")));
        sv.set_second(Box::new(Button::new("R")));
        let _ = sv.layout(LayoutConstraint::tight(Size::new(200.0, 100.0)));
        // divider_position 0.5, divider_size 4.0 -> first spans [0, 100),
        // divider [100, 104), second [104, 200).

        let result = sv.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point: Point::new(150.0, 50.0),
            modifiers: Modifiers::NONE,
        });

        assert!(matches!(result, EventResult::Handled));
        assert!(!sv.first.as_ref().unwrap().widget_state().hovered);
        assert!(sv.second.as_ref().unwrap().widget_state().hovered);
    }
}
