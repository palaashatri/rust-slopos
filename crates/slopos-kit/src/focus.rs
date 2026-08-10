//! Keyboard focus over a `Widget` tree.
//!
//! `WidgetState.focused` has existed since the beginning and nothing ever set
//! it (see `AGENTS.md`, P2/P5). `FocusManager` is the
//! thing that drives it: it owns exactly one notion of "which widget has
//! focus" and is responsible for keeping `WidgetState.focused` in sync with
//! that single value everywhere in the tree. That one invariant — `focus()`
//! sets `focused` on exactly the target widget and clears it on every other
//! widget — is what stops two `TextField`s in the same window from both
//! reacting to every keystroke.
//!
//! Like `dispatch.rs`, this is additive: nothing calls it yet.

use crate::dispatch::deliver_to;
use crate::{Event, EventResult, Widget, WidgetId};

/// Sets `widget_state_mut().focused` to whether this widget is `target`, on
/// `widget` and every descendant, in one tree walk. Returns whether `target`
/// was found anywhere in the tree (so callers can tell a real target from a
/// stale/foreign `WidgetId`).
fn set_focus_flags(widget: &mut dyn Widget, target: Option<WidgetId>) -> bool {
    let is_target = target == Some(widget.id());
    widget.widget_state_mut().focused = is_target;

    let mut found = is_target;
    for child in widget.children_mut() {
        if set_focus_flags(child, target) {
            found = true;
        }
    }
    found
}

/// Tree order (pre-order: a widget before its children, children in their
/// declared order) of every widget whose `focusable()` is true. This is tab
/// order. Note this does not currently skip hidden/disabled widgets — the
/// same way `dispatch.rs` skips those subtrees for hit-testing — because
/// nothing wires this into real widgets yet; that filter can be added
/// alongside the first real adoption without changing this module's API.
fn focus_order(root: &dyn Widget) -> Vec<WidgetId> {
    let mut order = Vec::new();
    collect_focus_order(root, &mut order);
    order
}

fn collect_focus_order(widget: &dyn Widget, order: &mut Vec<WidgetId>) {
    if widget.focusable() {
        order.push(widget.id());
    }
    for child in widget.children() {
        collect_focus_order(child, order);
    }
}

/// Owns the single focused `WidgetId` for a tree and keeps
/// `WidgetState.focused` consistent with it.
pub struct FocusManager {
    focused: Option<WidgetId>,
}

impl Default for FocusManager {
    fn default() -> Self {
        Self::new()
    }
}

impl FocusManager {
    pub fn new() -> Self {
        Self { focused: None }
    }

    /// The currently focused widget, if any.
    pub fn focused(&self) -> Option<WidgetId> {
        self.focused
    }

    /// Focus `id`: sets `WidgetState.focused` on exactly that widget and
    /// clears it everywhere else in `root`'s tree. If `id` isn't found in the
    /// tree, every widget's `focused` flag is cleared and nothing ends up
    /// focused, rather than tracking a dangling id.
    pub fn focus(&mut self, root: &mut dyn Widget, id: WidgetId) {
        let found = set_focus_flags(root, Some(id));
        self.focused = if found { Some(id) } else { None };
    }

    /// Clear focus: no widget in `root`'s tree ends up with `focused` set.
    pub fn clear(&mut self, root: &mut dyn Widget) {
        set_focus_flags(root, None);
        self.focused = None;
    }

    /// Tab: move to the next focusable widget in tree order, wrapping around.
    /// If nothing is currently focused (or the focused id fell out of the
    /// tree), this lands on the first focusable widget. A tree with no
    /// focusable widgets leaves focus cleared.
    pub fn focus_next(&mut self, root: &mut dyn Widget) {
        let order = focus_order(root);
        if order.is_empty() {
            self.clear(root);
            return;
        }
        let next = match self.index_in(&order) {
            Some(idx) => order[(idx + 1) % order.len()],
            None => order[0],
        };
        self.focus(root, next);
    }

    /// Shift+Tab: move to the previous focusable widget in tree order,
    /// wrapping around. If nothing is currently focused, this lands on the
    /// *last* focusable widget (the natural "wrap backward into the tree"
    /// starting point).
    pub fn focus_prev(&mut self, root: &mut dyn Widget) {
        let order = focus_order(root);
        if order.is_empty() {
            self.clear(root);
            return;
        }
        let prev = match self.index_in(&order) {
            Some(idx) => order[(idx + order.len() - 1) % order.len()],
            None => order[order.len() - 1],
        };
        self.focus(root, prev);
    }

    fn index_in(&self, order: &[WidgetId]) -> Option<usize> {
        let current = self.focused?;
        order.iter().position(|&id| id == current)
    }

    /// Route a key event to the focused widget; `Ignored` if none is focused
    /// (or the focused id is no longer in the tree).
    pub fn dispatch_key(&mut self, root: &mut dyn Widget, ev: &Event) -> EventResult {
        match self.focused {
            Some(target) => deliver_to(root, target, ev).unwrap_or(EventResult::Ignored),
            None => EventResult::Ignored,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{theme::ThemeContext, LayoutConstraint, Size, WidgetState};

    /// Test-only widget: configurable focusability and children, and a hit
    /// counter + canned response so tests can prove exactly which widget in a
    /// tree actually received a key event.
    struct TestWidget {
        state: WidgetState,
        children: Vec<TestWidget>,
        can_focus: bool,
        responds: bool,
        hits: u32,
    }

    impl TestWidget {
        fn new() -> Self {
            Self {
                state: WidgetState::new(),
                children: vec![],
                can_focus: false,
                responds: true,
                hits: 0,
            }
        }

        fn with_child(mut self, child: TestWidget) -> Self {
            self.children.push(child);
            self
        }

        fn with_focusable(mut self) -> Self {
            self.can_focus = true;
            self
        }

        fn ignoring(mut self) -> Self {
            self.responds = false;
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
            constraint.clamp(Size::ZERO)
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

        fn focusable(&self) -> bool {
            self.can_focus
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

    #[test]
    fn focus_sets_exactly_one_widget_and_clears_the_rest() {
        let a = TestWidget::new().with_focusable();
        let b = TestWidget::new().with_focusable();
        let a_id = a.id();
        let b_id = b.id();
        let mut root = TestWidget::new().with_child(a).with_child(b);
        let mut fm = FocusManager::new();

        fm.focus(&mut root, b_id);
        assert_eq!(fm.focused(), Some(b_id));
        assert!(!root.children[0].widget_state().focused);
        assert!(root.children[1].widget_state().focused);

        // Focusing a different widget must clear the previous one -- the
        // single invariant that fixes two `TextField`s both eating every
        // keystroke.
        fm.focus(&mut root, a_id);
        assert_eq!(fm.focused(), Some(a_id));
        assert!(root.children[0].widget_state().focused);
        assert!(!root.children[1].widget_state().focused);
    }

    #[test]
    fn focus_with_id_outside_the_tree_focuses_nothing() {
        let a = TestWidget::new().with_focusable();
        let mut root = TestWidget::new().with_child(a);
        let mut fm = FocusManager::new();

        let stray_id = TestWidget::new().id(); // never inserted into `root`
        fm.focus(&mut root, stray_id);

        assert_eq!(fm.focused(), None);
        assert!(!root.children[0].widget_state().focused);
    }

    #[test]
    fn clear_unsets_focus_everywhere() {
        let a = TestWidget::new().with_focusable();
        let a_id = a.id();
        let mut root = TestWidget::new().with_child(a);
        let mut fm = FocusManager::new();
        fm.focus(&mut root, a_id);
        assert!(root.children[0].widget_state().focused);

        fm.clear(&mut root);
        assert_eq!(fm.focused(), None);
        assert!(!root.children[0].widget_state().focused);
    }

    #[test]
    fn focus_next_visits_focusable_widgets_in_tree_order_and_wraps() {
        let a = TestWidget::new().with_focusable();
        let container = TestWidget::new(); // not focusable itself
        let b = TestWidget::new().with_focusable();
        let a_id = a.id();
        let b_id = b.id();
        let mut root = TestWidget::new()
            .with_child(a)
            .with_child(container)
            .with_child(b);
        let mut fm = FocusManager::new();

        fm.focus_next(&mut root);
        assert_eq!(fm.focused(), Some(a_id));

        fm.focus_next(&mut root);
        assert_eq!(fm.focused(), Some(b_id));

        fm.focus_next(&mut root);
        assert_eq!(
            fm.focused(),
            Some(a_id),
            "wraps back to the first focusable widget"
        );
    }

    #[test]
    fn focus_prev_visits_backward_and_wraps() {
        let a = TestWidget::new().with_focusable();
        let b = TestWidget::new().with_focusable();
        let a_id = a.id();
        let b_id = b.id();
        let mut root = TestWidget::new().with_child(a).with_child(b);
        let mut fm = FocusManager::new();

        fm.focus_prev(&mut root);
        assert_eq!(
            fm.focused(),
            Some(b_id),
            "Shift+Tab with nothing focused lands on the last focusable widget"
        );

        fm.focus_prev(&mut root);
        assert_eq!(fm.focused(), Some(a_id));

        fm.focus_prev(&mut root);
        assert_eq!(
            fm.focused(),
            Some(b_id),
            "wraps back around to the last focusable widget"
        );
    }

    #[test]
    fn focus_next_on_tree_with_no_focusable_widgets_stays_unfocused() {
        let mut root = TestWidget::new().with_child(TestWidget::new());
        let mut fm = FocusManager::new();

        fm.focus_next(&mut root);
        assert_eq!(fm.focused(), None);

        fm.focus_prev(&mut root);
        assert_eq!(fm.focused(), None);
    }

    #[test]
    fn dispatch_key_routes_only_to_the_focused_widget() {
        let a = TestWidget::new().with_focusable();
        let b = TestWidget::new().with_focusable();
        let a_id = a.id();
        let b_id = b.id();
        let mut root = TestWidget::new().with_child(a).with_child(b);
        let mut fm = FocusManager::new();

        let ev = Event::Char { character: 'x' };

        // Nothing focused yet: the key goes nowhere.
        assert!(matches!(
            fm.dispatch_key(&mut root, &ev),
            EventResult::Ignored
        ));
        assert_eq!(root.children[0].hits, 0);
        assert_eq!(root.children[1].hits, 0);

        fm.focus(&mut root, a_id);
        assert!(matches!(
            fm.dispatch_key(&mut root, &ev),
            EventResult::Handled
        ));
        assert_eq!(
            root.children[0].hits, 1,
            "only the focused widget receives it"
        );
        assert_eq!(root.children[1].hits, 0);

        // Re-focusing the other widget stops the first one from receiving
        // anything further -- this is the "two TextFields eat every
        // keystroke" bug, fixed.
        fm.focus(&mut root, b_id);
        let _ = fm.dispatch_key(&mut root, &ev);
        assert_eq!(root.children[0].hits, 1, "unchanged");
        assert_eq!(root.children[1].hits, 1);
    }

    #[test]
    fn dispatch_key_returns_ignored_when_focused_widget_declines_it() {
        let a = TestWidget::new().with_focusable().ignoring();
        let a_id = a.id();
        let mut root = TestWidget::new().with_child(a);
        let mut fm = FocusManager::new();

        fm.focus(&mut root, a_id);
        let ev = Event::Char { character: 'x' };
        assert!(matches!(
            fm.dispatch_key(&mut root, &ev),
            EventResult::Ignored
        ));
        assert_eq!(
            root.children[0].hits, 1,
            "was tried, but declined the event"
        );
    }
}
