//! Tab strip: a row of labelled tabs with one selected at a time.
//!
//! The strip is one stop in the window's focus order. While it has focus,
//! Left and Right move the selection (wrapping, as the focus chain does),
//! Home and End jump to the first and last tab. A primary press on a tab
//! selects it. Selection follows focus, so the app can switch its content
//! as the selection moves; the strip reports the change as
//! [`TabsOutcome::Selected`].
//!
//! Accessibility: the strip is a `TabList` node (id `id`) whose children are
//! one `Tab` node per tab (ids `id + 1 + index`). Each tab node carries its
//! selected state; clicking or focusing a tab node selects it.

use crate::event::{AccessibilityRequest, Event, Key, PointerButton};
use crate::foundation::geometry::Rect;
use crate::primitives::button_plate::{
    paint_button_plate_in, ButtonPlateState, ButtonPlateVariant,
};
use crate::render::{RasterSurface, RenderError};
use crate::text::{measure, Font};
use crate::theme::palette;

use super::button::paint_focus_ring;

const HEIGHT: u32 = 22;
const MIN_WIDTH: u32 = 70;
const PADDING: u32 = 28;
const GAP: i32 = 6;

/// What a [`Tabs`] event did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabsOutcome {
    /// The event did not concern the strip.
    Ignored,
    /// The strip took the event (focus moved, or the press landed on the
    /// already selected tab) without changing the selection.
    Changed,
    /// The selection moved to this tab index.
    Selected(usize),
}

#[derive(Debug, Clone)]
pub struct Tabs {
    /// Accessibility id of the strip; tab `index` is `id + 1 + index`.
    pub id: u64,
    /// Accessible name of the strip, such as "Software sources".
    pub label: String,
    /// Whether the strip currently has keyboard focus. Set by the app from
    /// its focus chain.
    pub focused: bool,
    /// Where the strip is drawn; set by [`Tabs::layout`].
    pub bounds: Rect,
    labels: Vec<String>,
    selected: usize,
    tab_rects: Vec<Rect>,
}

impl Tabs {
    /// A strip of `labels` with `selected` chosen (clamped to the last tab).
    pub fn new(id: u64, label: &str, labels: &[&str], selected: usize) -> Self {
        let mut tabs = Self {
            id,
            label: label.to_owned(),
            focused: false,
            bounds: Rect::default(),
            labels: labels.iter().map(|text| (*text).to_owned()).collect(),
            selected: 0,
            tab_rects: Vec::new(),
        };
        tabs.selected = tabs.clamp(selected);
        tabs
    }

    fn clamp(&self, index: usize) -> usize {
        index.min(self.labels.len().saturating_sub(1))
    }

    pub fn labels(&self) -> &[String] {
        &self.labels
    }

    pub fn len(&self) -> usize {
        self.labels.len()
    }

    pub fn is_empty(&self) -> bool {
        self.labels.is_empty()
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Replace the labels. The selection stays on the same index, clamped to
    /// the new last tab.
    pub fn set_labels(&mut self, labels: &[String]) {
        self.labels = labels.to_vec();
        self.selected = self.clamp(self.selected);
    }

    /// Select `index` without an event. Returns whether the selection moved.
    pub fn select(&mut self, index: usize) -> bool {
        if index >= self.labels.len() || index == self.selected {
            return false;
        }
        self.selected = index;
        true
    }

    /// The accessibility id of tab `index`.
    pub fn tab_id(&self, index: usize) -> u64 {
        self.id + 1 + index as u64
    }

    /// The index of the tab whose accessibility id is `id`.
    pub fn tab_at_node(&self, id: u64) -> Option<usize> {
        (id > self.id && id <= self.id + self.labels.len() as u64)
            .then(|| (id - self.id - 1) as usize)
    }

    /// The node focus should land on: the selected tab.
    pub fn focus_node(&self) -> u64 {
        self.tab_id(self.selected)
    }

    /// Place the strip with its top-left corner at `(x, y)`, sizing each tab
    /// to its label. Call again when the labels or font change.
    pub fn layout(&mut self, x: i32, y: i32, font: &Font) {
        let mut cursor = x;
        self.tab_rects = self
            .labels
            .iter()
            .map(|label| {
                let width = (measure(label, font).width as u32 + PADDING).max(MIN_WIDTH);
                let rect = Rect {
                    x: cursor,
                    y,
                    width,
                    height: HEIGHT,
                };
                cursor += width as i32 + GAP;
                rect
            })
            .collect();
        let right = self
            .tab_rects
            .last()
            .map_or(x, |rect| rect.x + rect.width as i32);
        self.bounds = Rect {
            x,
            y,
            width: (right - x).max(0) as u32,
            height: HEIGHT,
        };
    }

    /// The bounds of tab `index` after the last [`Tabs::layout`].
    pub fn tab_bounds(&self, index: usize) -> Option<Rect> {
        self.tab_rects.get(index).copied()
    }

    fn index_at(&self, x: i32, y: i32) -> Option<usize> {
        self.tab_rects.iter().position(|rect| rect.contains(x, y))
    }

    /// Feed the strip an event from the window.
    pub fn handle(&mut self, event: &Event) -> TabsOutcome {
        if self.labels.is_empty() {
            return TabsOutcome::Ignored;
        }
        let target = match event {
            Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
            } => match self.index_at(*x, *y) {
                Some(index) => index,
                None => return TabsOutcome::Ignored,
            },
            Event::Accessibility(AccessibilityRequest::Click(id))
            | Event::Accessibility(AccessibilityRequest::Focus(id)) => {
                match self.tab_at_node(*id) {
                    Some(index) => index,
                    None => return TabsOutcome::Ignored,
                }
            }
            Event::KeyPressed { key, .. } if self.focused => {
                let last = self.labels.len() - 1;
                let next = match key {
                    Key::Left => {
                        if self.selected == 0 {
                            last
                        } else {
                            self.selected - 1
                        }
                    }
                    Key::Right => {
                        if self.selected == last {
                            0
                        } else {
                            self.selected + 1
                        }
                    }
                    Key::Home => 0,
                    Key::End => last,
                    _ => return TabsOutcome::Ignored,
                };
                return if self.select(next) {
                    TabsOutcome::Selected(next)
                } else {
                    TabsOutcome::Changed
                };
            }
            _ => return TabsOutcome::Ignored,
        };
        if self.select(target) {
            TabsOutcome::Selected(target)
        } else {
            TabsOutcome::Changed
        }
    }

    pub fn paint(&self, surface: &RasterSurface, font: &Font) -> Result<(), RenderError> {
        let colors = palette();
        for (index, (label, rect)) in self.labels.iter().zip(&self.tab_rects).enumerate() {
            let selected = index == self.selected;
            let variant = if selected {
                ButtonPlateVariant::Default
            } else {
                ButtonPlateVariant::Regular
            };
            paint_button_plate_in(surface, *rect, variant, ButtonPlateState::Rest)?;
            let extent = measure(label, font);
            let x = rect.x + (rect.width as i32 - extent.width as i32) / 2;
            let y = rect.y + (rect.height as i32 - extent.height as i32) / 2;
            surface.draw_text(label, x, y, font, colors.ink)?;
            if selected && self.focused {
                let ring = Rect {
                    x: rect.x - 2,
                    y: rect.y - 2,
                    width: rect.width + 4,
                    height: rect.height + 4,
                };
                paint_focus_ring(surface, ring)?;
            }
        }
        Ok(())
    }

    /// The strip node and one node per tab, for [`crate::accessibility::window_tree`].
    pub fn accessibility_nodes(&self) -> Vec<(u64, accesskit::Node)> {
        let mut strip = accesskit::Node::new(accesskit::Role::TabList);
        strip.set_label(self.label.as_str());
        strip.set_bounds(super::bounds_of(self.bounds));
        strip.set_children(
            (0..self.labels.len())
                .map(|index| accesskit::NodeId(self.tab_id(index)))
                .collect::<Vec<_>>(),
        );
        strip.add_action(accesskit::Action::Focus);
        let mut nodes = vec![(self.id, strip)];
        for (index, label) in self.labels.iter().enumerate() {
            let mut node = accesskit::Node::new(accesskit::Role::Tab);
            node.set_label(label.as_str());
            if let Some(rect) = self.tab_bounds(index) {
                node.set_bounds(super::bounds_of(rect));
            }
            node.set_selected(index == self.selected);
            node.add_action(accesskit::Action::Click);
            node.add_action(accesskit::Action::Focus);
            nodes.push((self.tab_id(index), node));
        }
        nodes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Modifiers;
    use crate::foundation::geometry::Scale;

    fn strip() -> Tabs {
        let mut tabs = Tabs::new(40, "Software sources", &["Native", "SLOPOS", "Sources"], 0);
        tabs.layout(14, 30, &Font::system(12.0));
        tabs
    }

    fn key(key: Key) -> Event {
        Event::KeyPressed {
            key,
            modifiers: Modifiers::default(),
        }
    }

    fn click_tab(tabs: &Tabs, index: usize) -> Event {
        let rect = tabs.tab_bounds(index).unwrap();
        Event::PointerPressed {
            x: rect.x + 2,
            y: rect.y + 2,
            button: PointerButton::Primary,
        }
    }

    #[test]
    fn tabs_lay_out_left_to_right_without_overlap() {
        let tabs = strip();
        let rects: Vec<Rect> = (0..3).map(|i| tabs.tab_bounds(i).unwrap()).collect();
        assert_eq!(rects[0].x, 14);
        for pair in rects.windows(2) {
            assert!(pair[0].x + pair[0].width as i32 <= pair[1].x);
        }
        assert!(rects.iter().all(|rect| rect.width >= MIN_WIDTH));
        let last = rects[2];
        assert_eq!(
            tabs.bounds.x + tabs.bounds.width as i32,
            last.x + last.width as i32
        );
    }

    #[test]
    fn left_and_right_move_the_selection_and_wrap() {
        let mut tabs = strip();
        tabs.focused = true;
        assert_eq!(tabs.handle(&key(Key::Right)), TabsOutcome::Selected(1));
        assert_eq!(tabs.handle(&key(Key::Right)), TabsOutcome::Selected(2));
        assert_eq!(tabs.handle(&key(Key::Right)), TabsOutcome::Selected(0));
        assert_eq!(tabs.handle(&key(Key::Left)), TabsOutcome::Selected(2));
        assert_eq!(tabs.handle(&key(Key::Home)), TabsOutcome::Selected(0));
        assert_eq!(tabs.handle(&key(Key::End)), TabsOutcome::Selected(2));
        assert_eq!(tabs.selected(), 2);
    }

    #[test]
    fn arrows_need_focus_and_other_keys_are_ignored() {
        let mut tabs = strip();
        assert_eq!(tabs.handle(&key(Key::Right)), TabsOutcome::Ignored);
        assert_eq!(tabs.selected(), 0);
        tabs.focused = true;
        assert_eq!(tabs.handle(&key(Key::Enter)), TabsOutcome::Ignored);
        assert_eq!(tabs.handle(&key(Key::Character('a'))), TabsOutcome::Ignored);
        assert_eq!(tabs.selected(), 0);
    }

    #[test]
    fn a_single_tab_stays_put_and_reports_no_change() {
        let mut tabs = Tabs::new(40, "One", &["Only"], 0);
        tabs.layout(0, 0, &Font::system(12.0));
        tabs.focused = true;
        assert_eq!(tabs.handle(&key(Key::Right)), TabsOutcome::Changed);
        assert_eq!(tabs.handle(&key(Key::Left)), TabsOutcome::Changed);
        assert_eq!(tabs.selected(), 0);
    }

    #[test]
    fn a_click_selects_the_tab_under_the_pointer() {
        let mut tabs = strip();
        assert_eq!(
            tabs.handle(&click_tab(&tabs, 1)),
            TabsOutcome::Selected(1),
            "a press selects"
        );
        assert_eq!(
            tabs.handle(&click_tab(&tabs, 1)),
            TabsOutcome::Changed,
            "pressing the selected tab changes nothing"
        );
        let outside = Event::PointerPressed {
            x: 1000,
            y: 1000,
            button: PointerButton::Primary,
        };
        assert_eq!(tabs.handle(&outside), TabsOutcome::Ignored);
        let secondary = Event::PointerPressed {
            x: tabs.tab_bounds(2).unwrap().x + 2,
            y: tabs.bounds.y + 2,
            button: PointerButton::Secondary,
        };
        assert_eq!(tabs.handle(&secondary), TabsOutcome::Ignored);
        assert_eq!(tabs.selected(), 1);
    }

    #[test]
    fn accessibility_clicks_and_focus_select_a_tab() {
        let mut tabs = strip();
        let third = tabs.tab_id(2);
        assert_eq!(
            tabs.handle(&Event::Accessibility(AccessibilityRequest::Focus(third))),
            TabsOutcome::Selected(2)
        );
        assert_eq!(
            tabs.handle(&Event::Accessibility(AccessibilityRequest::Click(
                tabs.tab_id(0)
            ))),
            TabsOutcome::Selected(0)
        );
        // Another control's node is not the strip's business.
        assert_eq!(
            tabs.handle(&Event::Accessibility(AccessibilityRequest::Click(999))),
            TabsOutcome::Ignored
        );
    }

    #[test]
    fn labels_can_change_and_keep_a_valid_selection() {
        let mut tabs = strip();
        tabs.select(2);
        tabs.set_labels(&["Native".to_owned(), "SLOPOS".to_owned()]);
        assert_eq!(
            tabs.selected(),
            1,
            "the selection is clamped to the last tab"
        );
        assert_eq!(tabs.len(), 2);
        assert!(!tabs.select(5), "an index past the end is refused");
        assert_eq!(tabs.focus_node(), tabs.tab_id(1));
    }

    #[test]
    fn accessibility_tree_has_a_tablist_and_one_selected_tab() {
        let mut tabs = strip();
        tabs.select(1);
        let nodes = tabs.accessibility_nodes();
        assert_eq!(nodes.len(), 4);
        let strip_node = &nodes[0].1;
        assert_eq!(strip_node.role(), accesskit::Role::TabList);
        assert_eq!(strip_node.label(), Some("Software sources"));
        assert_eq!(strip_node.children().len(), 3);
        let selected: Vec<&str> = nodes[1..]
            .iter()
            .filter(|(_, node)| node.is_selected() == Some(true))
            .filter_map(|(_, node)| node.label())
            .collect();
        assert_eq!(selected, ["SLOPOS"]);
        assert!(nodes[1..]
            .iter()
            .all(|(_, node)| node.role() == accesskit::Role::Tab));
    }

    #[test]
    fn the_strip_paints_at_one_and_two_times_scale() {
        for factor in [1, 2] {
            let surface = RasterSurface::new(300, 40, Scale::new(factor).unwrap()).unwrap();
            let mut tabs = strip();
            tabs.focused = true;
            tabs.paint(&surface, &Font::system(12.0)).unwrap();
        }
    }
}
