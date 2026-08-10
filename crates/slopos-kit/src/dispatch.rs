//! Generic hit-test and pointer-event dispatch over a `Widget` tree.
//!
//! This is additive infrastructure: nothing in the toolkit or the apps calls
//! it yet (see `AGENTS.md`, P2/P5, and the migration
//! order in section 4). It exists so containers (`Toolbar`, `Layout`,
//! `SplitView`, `Window`) can stop forwarding positional events to every
//! child unconditionally — the bug that lets one `Button` swallow every left
//! click in the window.

use crate::{Event, EventResult, Point, Visibility, Widget, WidgetId};

/// A widget is eligible for hit-testing only if it (not necessarily its
/// ancestors) is visible and enabled. Callers skip a widget's entire subtree
/// once this is false for it, per the remediation doc's "skip subtrees that
/// are hidden or disabled".
fn is_live(widget: &dyn Widget) -> bool {
    widget.visibility() == Visibility::Visible && widget.enabled()
}

/// `widget` itself is a valid hit target at `at`: live and its rect contains
/// the point. Public so an owner running policy *around* dispatch (the
/// shell's window-manager layer asking "is this point on chrome?") uses the
/// dispatcher's own eligibility predicate instead of hand-rolling
/// `rect().contains`.
pub fn hit_test(widget: &dyn Widget, at: Point) -> bool {
    is_live(widget) && widget.rect().contains(at)
}

fn hits(widget: &dyn Widget, at: Point) -> bool {
    hit_test(widget, at)
}

/// Deepest visible, enabled widget whose rect contains `at`.
///
/// Depth-first: `root` is checked first (no hit anywhere in a subtree whose
/// root doesn't contain the point — children are expected to lie within
/// their parent's rect), then children are tested in reverse declaration
/// order so the last child (topmost, since it paints last) wins ties on
/// overlap. Hidden or disabled widgets, and everything under them, are
/// skipped entirely.
pub fn widget_at(root: &dyn Widget, at: Point) -> Option<WidgetId> {
    if !hits(root, at) {
        return None;
    }
    for child in root.children().into_iter().rev() {
        if let Some(id) = widget_at(child, at) {
            return Some(id);
        }
    }
    Some(root.id())
}

/// Shared positional-forwarding helper for container widgets: `Toolbar`,
/// `Layout`, `SplitView`, and `Window` all currently forward pointer events
/// to every child with no rect check at all, which is why a `Button`
/// anywhere in the tree eats every click in the window. A container adopts
/// this by replacing its hand-rolled forwarding loop with a call here over
/// its own direct children.
///
/// Children are tried in reverse (topmost first). For each live child whose
/// rect contains `at`, the event is first offered to *that child's own*
/// subtree (recursively, so a grandchild's rect-checked hit is found before
/// falling back to the child itself — "deepest hit wins"); if the whole
/// subtree returns `Ignored`, the event bubbles up: dispatch tries the next
/// (previously drawn) sibling in the same way. A child whose rect does not
/// contain `at`, or that is hidden/disabled, is skipped without ever being
/// asked to handle the event.
pub fn dispatch_positional(children: &mut [&mut dyn Widget], at: Point, ev: &Event) -> EventResult {
    match dispatch_positional_traced(children, at, ev) {
        Some((_, result)) => result,
        None => EventResult::Ignored,
    }
}

/// Same walk as [`dispatch_positional`], but also reports *which* widget
/// handled the event, which is what pointer capture needs.
fn dispatch_positional_traced(
    children: &mut [&mut dyn Widget],
    at: Point,
    ev: &Event,
) -> Option<(WidgetId, EventResult)> {
    for child in children.iter_mut().rev() {
        // `children.iter_mut()` over `&mut [&mut dyn Widget]` yields
        // `&mut &mut dyn Widget`; reborrow down to a plain `&mut dyn Widget`
        // so the rest of this body only ever juggles one level of reference.
        let child: &mut dyn Widget = &mut **child;
        if !hits(child, at) {
            continue;
        }
        let mut grandchildren = child.children_mut();
        if let Some(hit) = dispatch_positional_traced(&mut grandchildren, at, ev) {
            return Some(hit);
        }
        match child.handle_event(ev) {
            EventResult::Ignored => continue,
            other => return Some((child.id(), other)),
        }
    }
    None
}

/// Depth-first lookup of the widget with id `target`. Lets an app interrogate
/// the widget a dispatch outcome named — e.g. "was the widget that captured
/// this press focusable?" for click-to-focus — without walking the tree
/// itself.
pub fn widget_by_id(root: &dyn Widget, target: WidgetId) -> Option<&dyn Widget> {
    if root.id() == target {
        return Some(root);
    }
    for child in root.children() {
        if let Some(found) = widget_by_id(child, target) {
            return Some(found);
        }
    }
    None
}

/// Depth-first search for the widget with id `target`, delivering `ev` to it
/// via `handle_event` if found. Returns `None` (rather than `Ignored`) when
/// `target` isn't in the tree at all, so callers can tell "no such widget"
/// apart from "the widget ignored the event". Used by `FocusManager` for key
/// routing and by [`PointerDispatcher`] for capture delivery.
pub fn deliver_to(widget: &mut dyn Widget, target: WidgetId, ev: &Event) -> Option<EventResult> {
    if widget.id() == target {
        return Some(widget.handle_event(ev));
    }
    for child in widget.children_mut() {
        if let Some(result) = deliver_to(child, target, ev) {
            return Some(result);
        }
    }
    None
}

/// Stateful pointer routing over a widget tree: rect-checked dispatch plus
/// the two behaviours plain [`dispatch_positional`] cannot provide on its
/// own —
///
/// - **implicit capture**: the widget that handles a `MouseDown` receives
///   every subsequent `MouseMove`/`MouseUp` until release, even when the
///   pointer leaves its rect. Without this a `Slider` drag dies the moment
///   the cursor drifts off the track, and a `Button` pressed-then-released
///   outside never learns its press was cancelled.
/// - **hover tracking**: `MouseEnter`/`MouseLeave` are synthesized as the
///   pointer moves between widgets, so `WidgetState.hovered` reflects the
///   widget actually under the cursor instead of every widget that ever saw
///   a `MouseMove`.
///
/// Events are delivered only to `root`'s *descendants*, never to `root`
/// itself, so a root widget may call this from inside its own
/// `handle_event` without recursing.
#[derive(Default)]
pub struct PointerDispatcher {
    captured: Option<WidgetId>,
    hover: Option<WidgetId>,
}

impl PointerDispatcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// The widget currently holding the implicit pointer capture, if any.
    pub fn captured(&self) -> Option<WidgetId> {
        self.captured
    }

    pub fn dispatch(&mut self, root: &mut dyn Widget, ev: &Event) -> EventResult {
        match ev {
            Event::MouseDown { point, .. } | Event::DoubleClick { point, .. } => {
                let mut children = root.children_mut();
                match dispatch_positional_traced(&mut children, *point, ev) {
                    Some((id, result)) => {
                        self.captured = Some(id);
                        result
                    }
                    None => EventResult::Ignored,
                }
            }
            Event::MouseMove { point, .. } => {
                if let Some(target) = self.captured {
                    return deliver_to(root, target, ev).unwrap_or(EventResult::Ignored);
                }
                self.update_hover(root, *point);
                let mut children = root.children_mut();
                dispatch_positional(&mut children, *point, ev)
            }
            Event::MouseUp { point, .. } => {
                if let Some(target) = self.captured.take() {
                    return deliver_to(root, target, ev).unwrap_or(EventResult::Ignored);
                }
                let mut children = root.children_mut();
                dispatch_positional(&mut children, *point, ev)
            }
            Event::MouseLeave => {
                // Pointer left the window: end hover and cancel any capture.
                // The hovered and captured widget are often the same one, so
                // dedupe rather than deliver MouseLeave to it twice.
                let mut result = EventResult::Ignored;
                let mut delivered: Option<WidgetId> = None;
                for target in [self.hover.take(), self.captured.take()]
                    .into_iter()
                    .flatten()
                {
                    if delivered == Some(target) {
                        continue;
                    }
                    delivered = Some(target);
                    if let Some(EventResult::Handled) = deliver_to(root, target, ev) {
                        result = EventResult::Handled;
                    }
                }
                result
            }
            _ => EventResult::Ignored,
        }
    }

    fn update_hover(&mut self, root: &mut dyn Widget, at: Point) {
        // Hover only over strict descendants: `widget_at` falls back to the
        // root itself, which is not a hover target here.
        let now = widget_at(root, at).filter(|&id| id != root.id());
        if now != self.hover {
            if let Some(prev) = self.hover {
                let _ = deliver_to(root, prev, &Event::MouseLeave);
            }
            if let Some(next) = now {
                let _ = deliver_to(root, next, &Event::MouseEnter);
            }
            self.hover = now;
        }
    }
}

/// Depth-first mutable visit of `root` and every descendant. This is the
/// drain walk: after a dispatch, an owner of a *dynamic* widget tree (one
/// built as boxed `Layout` children rather than named struct fields — the
/// shell's dialog windows) uses this to collect `take_clicked()` /
/// `take_activated()` style activations without knowing the tree's shape.
pub fn for_each_widget_mut(root: &mut dyn Widget, f: &mut dyn FnMut(&mut dyn Widget)) {
    f(root);
    for child in root.children_mut() {
        for_each_widget_mut(child, f);
    }
}

/// Deliver a pointer event to the deepest hit widget under `root`, bubbling
/// toward `root` while handlers return `EventResult::Ignored`.
///
/// `root` itself must contain `at` (mirroring `widget_at`); if it doesn't,
/// or `root` is hidden/disabled, this returns `Ignored` without touching the
/// tree. Otherwise `root`'s children are searched via [`dispatch_positional`]
/// and, only if none of them (nor anything under them) handled the event,
/// `root.handle_event(ev)` is finally given the chance to.
pub fn dispatch_pointer(root: &mut dyn Widget, at: Point, ev: &Event) -> EventResult {
    if !hits(root, at) {
        return EventResult::Ignored;
    }
    let mut children = root.children_mut();
    match dispatch_positional(&mut children, at, ev) {
        EventResult::Ignored => root.handle_event(ev),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{theme::ThemeContext, LayoutConstraint, Rect, Size, WidgetState};

    /// Test-only widget for synthetic trees: a fixed rect, owned children,
    /// and a recorded hit count so tests can prove *which* widget in a tree
    /// actually received an event (not just what the top-level call returned).
    struct TestWidget {
        state: WidgetState,
        children: Vec<TestWidget>,
        /// What `handle_event` returns when it is actually invoked.
        responds: bool,
        /// Incremented every time `handle_event` runs on this widget.
        hits: u32,
    }

    impl TestWidget {
        fn new(rect: Rect) -> Self {
            let mut state = WidgetState::new();
            state.rect = rect;
            Self {
                state,
                children: vec![],
                responds: true,
                hits: 0,
            }
        }

        fn with_child(mut self, child: TestWidget) -> Self {
            self.children.push(child);
            self
        }

        /// `handle_event` returns `Ignored` instead of `Handled` when invoked.
        fn ignoring(mut self) -> Self {
            self.responds = false;
            self
        }

        fn hidden(mut self) -> Self {
            self.state.visibility = Visibility::Hidden;
            self
        }

        fn disabled(mut self) -> Self {
            self.state.enabled = false;
            self
        }
    }

    impl Widget for TestWidget {
        fn widget_state(&self) -> &WidgetState {
            &self.state
        }
        fn widget_state_mut(&mut self) -> &mut WidgetState {
            &mut self.state
        }

        fn layout(&mut self, constraint: LayoutConstraint) -> Size {
            constraint.clamp(Size::new(self.state.rect.width, self.state.rect.height))
        }
        fn draw(&self, _theme: &ThemeContext) {}

        fn handle_event(&mut self, _event: &Event) -> EventResult {
            self.hits += 1;
            if self.responds {
                EventResult::Handled
            } else {
                EventResult::Ignored
            }
        }

        fn children(&self) -> Vec<&dyn Widget> {
            self.children.iter().map(|c| c as &dyn Widget).collect()
        }
        fn children_mut(&mut self) -> Vec<&mut dyn Widget> {
            self.children
                .iter_mut()
                .map(|c| c as &mut dyn Widget)
                .collect()
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
    }

    // ---- widget_at ---------------------------------------------------

    #[test]
    fn widget_at_finds_deepest_child() {
        let grandchild = TestWidget::new(Rect::new(0.0, 0.0, 20.0, 20.0));
        let grandchild_id = grandchild.id();
        let child = TestWidget::new(Rect::new(0.0, 0.0, 50.0, 100.0)).with_child(grandchild);
        let root = TestWidget::new(Rect::new(0.0, 0.0, 100.0, 100.0)).with_child(child);

        assert_eq!(
            widget_at(&root, Point::new(10.0, 10.0)),
            Some(grandchild_id)
        );
    }

    #[test]
    fn widget_at_falls_back_to_parent_when_no_child_matches() {
        let child = TestWidget::new(Rect::new(0.0, 0.0, 50.0, 100.0));
        let root = TestWidget::new(Rect::new(0.0, 0.0, 100.0, 100.0)).with_child(child);
        let root_id = root.id();

        // Inside root but outside the child's rect (x=60 > child width 50).
        assert_eq!(widget_at(&root, Point::new(60.0, 10.0)), Some(root_id));
    }

    #[test]
    fn widget_at_returns_none_outside_root() {
        let root = TestWidget::new(Rect::new(0.0, 0.0, 100.0, 100.0));
        assert_eq!(widget_at(&root, Point::new(-5.0, -5.0)), None);
    }

    #[test]
    fn widget_at_skips_hidden_and_disabled_subtrees() {
        let hidden_child = TestWidget::new(Rect::new(0.0, 0.0, 100.0, 100.0)).hidden();
        let root = TestWidget::new(Rect::new(0.0, 0.0, 100.0, 100.0)).with_child(hidden_child);
        let root_id = root.id();
        assert_eq!(widget_at(&root, Point::new(10.0, 10.0)), Some(root_id));

        let disabled_child = TestWidget::new(Rect::new(0.0, 0.0, 100.0, 100.0)).disabled();
        let root2 = TestWidget::new(Rect::new(0.0, 0.0, 100.0, 100.0)).with_child(disabled_child);
        let root2_id = root2.id();
        assert_eq!(widget_at(&root2, Point::new(10.0, 10.0)), Some(root2_id));
    }

    #[test]
    fn widget_at_prefers_topmost_overlapping_child() {
        let below = TestWidget::new(Rect::new(0.0, 0.0, 50.0, 50.0));
        let above = TestWidget::new(Rect::new(0.0, 0.0, 50.0, 50.0));
        let above_id = above.id();
        // `above` is added last, i.e. drawn last/on top; reverse-order testing
        // must find it before `below`.
        let root = TestWidget::new(Rect::new(0.0, 0.0, 100.0, 100.0))
            .with_child(below)
            .with_child(above);

        assert_eq!(widget_at(&root, Point::new(10.0, 10.0)), Some(above_id));
    }

    // ---- dispatch_pointer --------------------------------------------

    #[test]
    fn dispatch_pointer_deepest_hit_wins_and_ancestors_are_untouched() {
        let grandchild = TestWidget::new(Rect::new(0.0, 0.0, 50.0, 50.0));
        let child = TestWidget::new(Rect::new(0.0, 0.0, 100.0, 100.0)).with_child(grandchild);
        let mut root = TestWidget::new(Rect::new(0.0, 0.0, 100.0, 100.0)).with_child(child);

        let ev = Event::MouseEnter;
        let result = dispatch_pointer(&mut root, Point::new(10.0, 10.0), &ev);

        assert!(matches!(result, EventResult::Handled));
        assert_eq!(root.hits, 0, "root must not run: a descendant handled it");
        assert_eq!(root.children[0].hits, 0, "child must not run either");
        assert_eq!(root.children[0].children[0].hits, 1, "grandchild ran once");
    }

    #[test]
    fn dispatch_pointer_bubbles_to_immediate_parent_when_child_ignores() {
        let grandchild = TestWidget::new(Rect::new(0.0, 0.0, 50.0, 50.0)).ignoring();
        let child = TestWidget::new(Rect::new(0.0, 0.0, 100.0, 100.0)).with_child(grandchild);
        let mut root = TestWidget::new(Rect::new(0.0, 0.0, 100.0, 100.0)).with_child(child);

        let ev = Event::MouseEnter;
        let result = dispatch_pointer(&mut root, Point::new(10.0, 10.0), &ev);

        assert!(matches!(result, EventResult::Handled));
        assert_eq!(root.hits, 0);
        assert_eq!(
            root.children[0].hits, 1,
            "child handled after grandchild ignored"
        );
        assert_eq!(
            root.children[0].children[0].hits, 1,
            "grandchild was tried first"
        );
    }

    #[test]
    fn dispatch_pointer_bubbles_all_the_way_to_root() {
        let grandchild = TestWidget::new(Rect::new(0.0, 0.0, 50.0, 50.0)).ignoring();
        let child = TestWidget::new(Rect::new(0.0, 0.0, 100.0, 100.0))
            .with_child(grandchild)
            .ignoring();
        let mut root = TestWidget::new(Rect::new(0.0, 0.0, 100.0, 100.0)).with_child(child);

        let ev = Event::MouseEnter;
        let result = dispatch_pointer(&mut root, Point::new(10.0, 10.0), &ev);

        assert!(matches!(result, EventResult::Handled));
        assert_eq!(
            root.hits, 1,
            "everything below ignored; root finally handles it"
        );
        assert_eq!(root.children[0].hits, 1);
        assert_eq!(root.children[0].children[0].hits, 1);
    }

    #[test]
    fn dispatch_pointer_ignores_point_outside_root_and_touches_nothing() {
        let child = TestWidget::new(Rect::new(0.0, 0.0, 100.0, 100.0));
        let mut root = TestWidget::new(Rect::new(0.0, 0.0, 100.0, 100.0)).with_child(child);

        let ev = Event::MouseEnter;
        let result = dispatch_pointer(&mut root, Point::new(-5.0, -5.0), &ev);

        assert!(matches!(result, EventResult::Ignored));
        assert_eq!(root.hits, 0);
        assert_eq!(root.children[0].hits, 0);
    }

    #[test]
    fn dispatch_pointer_skips_hidden_child_entirely_and_bubbles_to_root() {
        // This is the systemic bug from the audit: a container used to
        // forward to every child with no rect *or* visibility check. A
        // hidden child sitting right on top of the point must never see the
        // event at all, and the root must get a chance instead.
        let hidden_child = TestWidget::new(Rect::new(0.0, 0.0, 100.0, 100.0)).hidden();
        let mut root = TestWidget::new(Rect::new(0.0, 0.0, 100.0, 100.0)).with_child(hidden_child);

        let ev = Event::MouseEnter;
        let result = dispatch_pointer(&mut root, Point::new(10.0, 10.0), &ev);

        assert!(matches!(result, EventResult::Handled));
        assert_eq!(root.hits, 1);
        assert_eq!(
            root.children[0].hits, 0,
            "hidden child must never be invoked"
        );
    }

    // ---- dispatch_positional (direct) ----------------------------------

    #[test]
    fn dispatch_positional_delivers_only_to_the_child_under_the_point() {
        let mut left = TestWidget::new(Rect::new(0.0, 0.0, 50.0, 50.0));
        let mut right = TestWidget::new(Rect::new(50.0, 0.0, 50.0, 50.0));
        let mut children: Vec<&mut dyn Widget> = vec![&mut left, &mut right];

        let ev = Event::MouseEnter;
        let result = dispatch_positional(&mut children, Point::new(60.0, 10.0), &ev);

        assert!(matches!(result, EventResult::Handled));
        assert_eq!(left.hits, 0);
        assert_eq!(right.hits, 1);
    }

    #[test]
    fn dispatch_positional_falls_through_to_earlier_sibling_when_topmost_ignores() {
        let mut under = TestWidget::new(Rect::new(0.0, 0.0, 50.0, 50.0));
        let mut over = TestWidget::new(Rect::new(0.0, 0.0, 50.0, 50.0)).ignoring();
        // `over` is later in the slice, i.e. drawn on top and tried first.
        let mut children: Vec<&mut dyn Widget> = vec![&mut under, &mut over];

        let ev = Event::MouseEnter;
        let result = dispatch_positional(&mut children, Point::new(10.0, 10.0), &ev);

        assert!(matches!(result, EventResult::Handled));
        assert_eq!(over.hits, 1, "topmost was tried first");
        assert_eq!(under.hits, 1, "fell through to the widget underneath");
    }

    #[test]
    fn dispatch_positional_returns_ignored_when_no_child_matches() {
        let mut only = TestWidget::new(Rect::new(0.0, 0.0, 50.0, 50.0));
        let mut children: Vec<&mut dyn Widget> = vec![&mut only];

        let ev = Event::MouseEnter;
        let result = dispatch_positional(&mut children, Point::new(90.0, 90.0), &ev);

        assert!(matches!(result, EventResult::Ignored));
        assert_eq!(only.hits, 0);
    }

    // ---- PointerDispatcher -------------------------------------------

    fn mouse_down(x: f32, y: f32) -> Event {
        Event::MouseDown {
            button: crate::event::MouseButton::Left,
            point: Point::new(x, y),
            modifiers: crate::event::Modifiers::NONE,
        }
    }
    fn mouse_up(x: f32, y: f32) -> Event {
        Event::MouseUp {
            button: crate::event::MouseButton::Left,
            point: Point::new(x, y),
            modifiers: crate::event::Modifiers::NONE,
        }
    }
    fn mouse_move(x: f32, y: f32) -> Event {
        Event::MouseMove {
            point: Point::new(x, y),
            modifiers: crate::event::Modifiers::NONE,
        }
    }

    #[test]
    fn capture_routes_motion_and_release_to_the_pressed_widget() {
        let slider = TestWidget::new(Rect::new(0.0, 0.0, 50.0, 20.0));
        let slider_id = slider.id();
        let mut root = TestWidget::new(Rect::new(0.0, 0.0, 200.0, 200.0)).with_child(slider);
        let mut pd = PointerDispatcher::new();

        assert!(matches!(
            pd.dispatch(&mut root, &mouse_down(10.0, 10.0)),
            EventResult::Handled
        ));
        assert_eq!(pd.captured(), Some(slider_id));
        assert_eq!(root.children[0].hits, 1);

        // Motion far outside the widget's rect must still reach it while
        // captured — this is what keeps a slider drag alive.
        assert!(matches!(
            pd.dispatch(&mut root, &mouse_move(190.0, 190.0)),
            EventResult::Handled
        ));
        assert_eq!(root.children[0].hits, 2);

        // Release also goes to the captured widget, then capture ends.
        assert!(matches!(
            pd.dispatch(&mut root, &mouse_up(190.0, 190.0)),
            EventResult::Handled
        ));
        assert_eq!(root.children[0].hits, 3);
        assert_eq!(pd.captured(), None);

        // With capture released, motion outside every widget goes nowhere.
        let _ = pd.dispatch(&mut root, &mouse_move(190.0, 190.0));
        assert_eq!(root.children[0].hits, 3);
    }

    #[test]
    fn press_ignored_by_everything_captures_nothing() {
        let deaf = TestWidget::new(Rect::new(0.0, 0.0, 50.0, 50.0)).ignoring();
        let mut root = TestWidget::new(Rect::new(0.0, 0.0, 100.0, 100.0)).with_child(deaf);
        let mut pd = PointerDispatcher::new();

        assert!(matches!(
            pd.dispatch(&mut root, &mouse_down(10.0, 10.0)),
            EventResult::Ignored
        ));
        assert_eq!(pd.captured(), None);
        // Root is never a dispatch target for its own PointerDispatcher.
        assert_eq!(root.hits, 0);
    }

    #[test]
    fn hover_synthesizes_enter_and_leave_between_widgets() {
        let left = TestWidget::new(Rect::new(0.0, 0.0, 50.0, 50.0));
        let right = TestWidget::new(Rect::new(50.0, 0.0, 50.0, 50.0));
        let mut root = TestWidget::new(Rect::new(0.0, 0.0, 100.0, 100.0))
            .with_child(left)
            .with_child(right);
        let mut pd = PointerDispatcher::new();

        // Move over `left`: it receives MouseEnter + the MouseMove itself.
        let _ = pd.dispatch(&mut root, &mouse_move(10.0, 10.0));
        assert_eq!(root.children[0].hits, 2);
        assert_eq!(root.children[1].hits, 0);

        // Cross to `right`: `left` gets MouseLeave, `right` Enter + move.
        let _ = pd.dispatch(&mut root, &mouse_move(60.0, 10.0));
        assert_eq!(root.children[0].hits, 3);
        assert_eq!(root.children[1].hits, 2);
    }

    #[test]
    fn window_mouse_leave_notifies_hovered_widget_and_clears_capture() {
        let child = TestWidget::new(Rect::new(0.0, 0.0, 50.0, 50.0));
        let mut root = TestWidget::new(Rect::new(0.0, 0.0, 100.0, 100.0)).with_child(child);
        let mut pd = PointerDispatcher::new();

        let _ = pd.dispatch(&mut root, &mouse_move(10.0, 10.0)); // hover (2 hits)
        let _ = pd.dispatch(&mut root, &mouse_down(10.0, 10.0)); // capture (3 hits)

        // Hovered and captured are the same widget: exactly one MouseLeave.
        assert!(matches!(
            pd.dispatch(&mut root, &Event::MouseLeave),
            EventResult::Handled
        ));
        assert_eq!(root.children[0].hits, 4);
        assert_eq!(pd.captured(), None);
    }
}
