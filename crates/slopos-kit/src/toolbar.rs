use crate::{
    dispatch::dispatch_positional, theme::ThemeContext, Event, EventResult, LayoutConstraint,
    Point, Rect, Size, Widget, WidgetState,
};

/// Pointer location carried by positional event variants. Events with no
/// on-screen position (`MouseEnter`, `KeyDown`, ...) have nothing to
/// rect-check against, so the toolbar has nothing to dispatch and leaves
/// them `Ignored` — same as the `Widget` trait default.
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

pub struct Toolbar {
    state: WidgetState,
    pub items: Vec<Box<dyn Widget>>,
}

impl Default for Toolbar {
    fn default() -> Self {
        Self::new()
    }
}

impl Toolbar {
    pub fn new() -> Self {
        Self {
            state: WidgetState::new(),
            items: vec![],
        }
    }

    pub fn add(&mut self, widget: Box<dyn Widget>) {
        self.items.push(widget);
    }
}

impl Widget for Toolbar {
    fn widget_state(&self) -> &WidgetState {
        &self.state
    }
    fn widget_state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        let height = 32.0;
        let mut width = 0.0;
        for child in &mut self.items {
            let size = child.layout(LayoutConstraint::loose(Size::new(180.0, height)));
            width += size.width;
        }
        let preferred_width = if constraint.max_width.is_finite() {
            constraint.max_width
        } else {
            width.max(constraint.min_width)
        };
        let size = constraint.clamp(Size::new(preferred_width, height));
        self.set_rect(Rect::new(
            self.rect().x,
            self.rect().y,
            size.width,
            size.height,
        ));
        let mut x = self.rect().x;
        let y = self.rect().y;
        for child in &mut self.items {
            let rect = child.rect();
            child.set_rect(Rect::new(x, y, rect.width, height));
            x += rect.width;
        }
        size
    }

    fn draw(&self, theme: &ThemeContext) {
        for item in &self.items {
            item.draw(theme);
        }
    }

    // Was: forwarded every event to every item in reverse with no rect
    // check, so — combined with `Button` returning `Handled` unconditionally
    // — the last toolbar item swallowed every left click in the window (see
    // AGENTS.md P2/P5). `dispatch_positional` gates on
    // each item's own rect (and visibility/enabled) before it is ever asked
    // to handle the event.
    fn handle_event(&mut self, event: &Event) -> EventResult {
        let Some(at) = positional_point(event) else {
            return EventResult::Ignored;
        };
        if !self.rect().contains(at) {
            return EventResult::Ignored;
        }
        // Built with an explicit loop rather than `.iter_mut().map(...).collect()`:
        // the closure form defeats lifetime elision here (the `dyn Widget`
        // trait-object bound ends up unconstrained), so rustc asks for a `'_`
        // annotation on the trait method itself. A plain loop ties each
        // reference straight to `self`'s borrow, matching the rest of the crate.
        let mut children: Vec<&mut dyn Widget> = vec![];
        for w in self.items.iter_mut() {
            children.push(w.as_mut());
        }
        dispatch_positional(&mut children, at, event)
    }

    fn children(&self) -> Vec<&dyn Widget> {
        let mut result: Vec<&dyn Widget> = vec![];
        for w in self.items.iter() {
            result.push(w.as_ref());
        }
        result
    }

    fn children_mut(&mut self) -> Vec<&mut dyn Widget> {
        let mut result: Vec<&mut dyn Widget> = vec![];
        for w in self.items.iter_mut() {
            result.push(w.as_mut());
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
    use crate::event::{Modifiers, MouseButton};

    #[test]
    fn toolbar_only_delivers_click_to_the_item_under_the_point() {
        let mut toolbar = Toolbar::new();
        toolbar.add(Box::new(Button::new("A")));
        toolbar.add(Box::new(Button::new("B")));
        let _ = toolbar.layout(LayoutConstraint::loose(Size::new(200.0, 32.0)));

        // "A" and "B" are both single-character labels -> 27.5px wide each
        // (see Button::layout), laid out left to right. This point is inside B.
        let point = Point::new(50.0, 10.0);
        let result = toolbar.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point,
            modifiers: Modifiers::NONE,
        });

        assert!(matches!(result, EventResult::Handled));
        assert!(
            !toolbar.items[0].widget_state().hovered,
            "A is outside the click point and must be left untouched"
        );
        assert!(
            toolbar.items[1].widget_state().hovered,
            "B contains the click point and should have handled it"
        );
    }

    #[test]
    fn toolbar_ignores_a_click_outside_every_item() {
        let mut toolbar = Toolbar::new();
        toolbar.add(Box::new(Button::new("A")));
        let _ = toolbar.layout(LayoutConstraint::loose(Size::new(200.0, 32.0)));

        // Toolbar itself spans [0, 200) (loose max_width), but "A" only
        // spans only its natural width: 190 is inside the toolbar and outside every item.
        let result = toolbar.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point: Point::new(190.0, 10.0),
            modifiers: Modifiers::NONE,
        });

        assert!(matches!(result, EventResult::Ignored));
        assert!(!toolbar.items[0].widget_state().hovered);
    }

    #[test]
    fn toolbar_ignores_events_with_no_position() {
        let mut toolbar = Toolbar::new();
        toolbar.add(Box::new(Button::new("A")));
        let _ = toolbar.layout(LayoutConstraint::loose(Size::new(200.0, 32.0)));

        let result = toolbar.handle_event(&Event::MouseEnter);

        assert!(matches!(result, EventResult::Ignored));
        assert!(!toolbar.items[0].widget_state().hovered);
    }
}
