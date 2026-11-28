//! Tab bar: a row of labelled tabs with one selected at a time, with
//! overflow arrows when the tabs are wider than the space given to it.
//!
//! This module holds the [`Tabs`] control only. The equal-width
//! `TabStrip` control lives in [`super::tabs`] and is a separate control.
//!
//! The strip is one stop in the window's focus order. While it has focus,
//! Left and Right move the selection (wrapping, as the focus chain does),
//! Home and End jump to the first and last tab. A primary press on a tab
//! selects it. Selection follows focus, so the app can switch its content
//! as the selection moves; the strip reports the change as
//! [`TabsOutcome::Selected`].
//!
//! When the tabs are wider than the space given to [`Tabs::layout`], the
//! strip shows a run of them that always includes the selected tab, with a
//! previous and a next arrow at its ends. The arrows select the neighbouring
//! tab, wrapping like the keys do, so every tab stays reachable by pointer,
//! keyboard and accessibility.
//!
//! Accessibility: the strip is a `TabList` node (id `id`) whose children are
//! one `Tab` node per tab (ids `id + 1 + index`). Each tab node carries its
//! selected state; clicking or focusing a tab node selects it. A tab that is
//! scrolled out of the run has no bounds but is still a node.

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
/// Width of the previous and next arrows shown when the tabs overflow.
const ARROW_WIDTH: u32 = 22;

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
    /// Each tab's width from the last layout; empty until one happens.
    widths: Vec<u32>,
    /// Where the strip was placed and how much width it may use.
    origin: (i32, i32),
    available: u32,
    /// The first tab in the run when the tabs overflow.
    first: usize,
    /// Each tab's bounds, or `None` for a tab outside the run.
    tab_rects: Vec<Option<Rect>>,
    /// The previous and next arrows, present only when the tabs overflow.
    arrows: Option<(Rect, Rect)>,
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
            widths: Vec::new(),
            origin: (0, 0),
            available: 0,
            first: 0,
            tab_rects: Vec::new(),
            arrows: None,
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
    /// the new last tab. The strip has no tab positions until the next
    /// [`Tabs::layout`], because the new labels have no widths yet.
    pub fn set_labels(&mut self, labels: &[String]) {
        self.labels = labels.to_vec();
        self.selected = self.clamp(self.selected);
        self.widths.clear();
        self.fit();
    }

    /// Select `index` without an event. Returns whether the selection moved.
    pub fn select(&mut self, index: usize) -> bool {
        if index >= self.labels.len() || index == self.selected {
            return false;
        }
        self.selected = index;
        self.fit();
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

    /// The node focus should land on: the selected tab, or the strip when
    /// there are no tabs (so the focus target is always a node).
    pub fn focus_node(&self) -> u64 {
        if self.labels.is_empty() {
            return self.id;
        }
        self.tab_id(self.selected)
    }

    /// Place the strip with its top-left corner at `(x, y)` in `available`
    /// width, sizing each tab to its label. Call again when the labels, the
    /// font or the width change.
    pub fn layout(&mut self, x: i32, y: i32, available: u32, font: &Font) {
        self.origin = (x, y);
        self.available = available;
        self.widths = self
            .labels
            .iter()
            .map(|label| (measure(label, font).width as u32 + PADDING).max(MIN_WIDTH))
            .collect();
        self.fit();
    }

    /// Work out where each tab is drawn from the widths, the origin, the
    /// available width and the selection. Tabs that fit are all shown.
    /// Otherwise the run is the first tab that keeps the selected tab in view,
    /// extended to the right as far as it fits, between the two arrows.
    fn fit(&mut self) {
        let (x, y) = self.origin;
        let count = self.labels.len();
        self.tab_rects = vec![None; count];
        self.arrows = None;
        self.bounds = Rect {
            x,
            y,
            width: 0,
            height: HEIGHT,
        };
        if count == 0 || self.widths.len() != count {
            return;
        }
        let full = run_width(&self.widths, 0, count);
        let (first, end, start) = if full <= self.available {
            self.first = 0;
            (0, count, x)
        } else {
            let arrow_space = ARROW_WIDTH + GAP as u32;
            let room = self.available.saturating_sub(2 * arrow_space);
            self.first = self.first.min(self.selected);
            while self.first < self.selected
                && run_width(&self.widths, self.first, self.selected + 1) > room
            {
                self.first += 1;
            }
            let mut end = self.first + 1;
            while end < count && run_width(&self.widths, self.first, end + 1) <= room {
                end += 1;
            }
            let right = x + self.available.saturating_sub(ARROW_WIDTH) as i32;
            self.arrows = Some((
                Rect {
                    x,
                    y,
                    width: ARROW_WIDTH,
                    height: HEIGHT,
                },
                Rect {
                    x: right,
                    y,
                    width: ARROW_WIDTH,
                    height: HEIGHT,
                },
            ));
            (self.first, end, x + arrow_space as i32)
        };
        let mut cursor = start;
        // With arrows, a tab stops at the left edge of the right arrow, so a
        // wide tab cannot paint over it. Tabs narrower than the space keep their
        // width.
        let limit = self.arrows.map(|(_, right)| right.x - GAP);
        for (index, rect) in self.tab_rects.iter_mut().enumerate().take(end).skip(first) {
            let mut width = self.widths[index];
            if let Some(limit) = limit {
                width = width.min((limit - cursor).max(0) as u32);
            }
            *rect = Some(Rect {
                x: cursor,
                y,
                width,
                height: HEIGHT,
            });
            cursor += width as i32 + GAP;
        }
        self.bounds = Rect {
            x,
            y,
            width: if self.arrows.is_some() {
                self.available
            } else {
                full
            },
            height: HEIGHT,
        };
    }

    /// The bounds of tab `index` after the last [`Tabs::layout`], or `None`
    /// when the tab is outside the run.
    pub fn tab_bounds(&self, index: usize) -> Option<Rect> {
        self.tab_rects.get(index).copied().flatten()
    }

    /// The index of the visible tab under `(x, y)`.
    fn index_at(&self, x: i32, y: i32) -> Option<usize> {
        self.tab_rects
            .iter()
            .position(|rect| rect.is_some_and(|rect| rect.contains(x, y)))
    }

    /// The tab `step` away from the selection, wrapping at either end.
    fn neighbour(&self, forward: bool) -> usize {
        let last = self.labels.len() - 1;
        match (forward, self.selected) {
            (true, selected) if selected == last => 0,
            (true, selected) => selected + 1,
            (false, 0) => last,
            (false, selected) => selected - 1,
        }
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
            } => match self.arrows {
                Some((back, _)) if back.contains(*x, *y) => self.neighbour(false),
                Some((_, forward)) if forward.contains(*x, *y) => self.neighbour(true),
                _ => match self.index_at(*x, *y) {
                    Some(index) => index,
                    None => return TabsOutcome::Ignored,
                },
            },
            Event::Accessibility(AccessibilityRequest::Click(id))
            | Event::Accessibility(AccessibilityRequest::Focus(id)) => {
                match self.tab_at_node(*id) {
                    Some(index) => index,
                    None => return TabsOutcome::Ignored,
                }
            }
            Event::KeyPressed { key, .. } if self.focused => {
                let next = match key {
                    Key::Left => self.neighbour(false),
                    Key::Right => self.neighbour(true),
                    Key::Home => 0,
                    Key::End => self.labels.len() - 1,
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
        if let Some((back, forward)) = self.arrows {
            for (rect, glyph) in [(back, "<"), (forward, ">")] {
                paint_button_plate_in(
                    surface,
                    rect,
                    ButtonPlateVariant::Regular,
                    ButtonPlateState::Rest,
                )?;
                let extent = measure(glyph, font);
                let x = rect.x + (rect.width as i32 - extent.width as i32) / 2;
                let y = rect.y + (rect.height as i32 - extent.height as i32) / 2;
                surface.draw_text(glyph, x, y, font, colors.ink)?;
            }
        }
        for (index, (label, rect)) in self.labels.iter().zip(&self.tab_rects).enumerate() {
            let Some(rect) = rect else { continue };
            // A tab capped to nothing by a narrow strip has no area to draw in.
            if rect.width == 0 || rect.height == 0 {
                continue;
            }
            let selected = index == self.selected;
            let variant = if selected {
                ButtonPlateVariant::Default
            } else {
                ButtonPlateVariant::Regular
            };
            // The plate and label are clipped to the tab's rectangle, so a label
            // wider than a capped tab cannot paint past its edge.
            surface.with_clip(*rect, || {
                paint_button_plate_in(surface, *rect, variant, ButtonPlateState::Rest)?;
                let extent = measure(label, font);
                let x = rect.x + (rect.width as i32 - extent.width as i32) / 2;
                let y = rect.y + (rect.height as i32 - extent.height as i32) / 2;
                surface.draw_text(label, x, y, font, colors.ink)
            })?;
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

/// The width of tabs `first..end` placed side by side with their gaps.
fn run_width(widths: &[u32], first: usize, end: usize) -> u32 {
    let tabs = &widths[first..end];
    tabs.iter().sum::<u32>() + GAP as u32 * tabs.len().saturating_sub(1) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Modifiers;
    use crate::foundation::geometry::Scale;

    fn strip() -> Tabs {
        let mut tabs = Tabs::new(40, "Software sources", &["Native", "SLOPOS", "Sources"], 0);
        tabs.layout(14, 30, 600, &Font::system(12.0));
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
        tabs.layout(0, 0, 600, &Font::system(12.0));
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

    const SIX: [&str; 6] = [
        "Native (apt)",
        "SLOPOS",
        "Flatpak",
        "Snap",
        "AUR",
        "Sources",
    ];

    /// A strip of six tabs laid out `available` wide, with `selected` chosen.
    fn narrow(available: u32, selected: usize) -> Tabs {
        let mut tabs = Tabs::new(40, "Software sources", &SIX, 0);
        tabs.layout(14, 30, available, &Font::system(12.0));
        tabs.select(selected);
        tabs
    }

    /// The indices of the tabs that have bounds.
    fn visible(tabs: &Tabs) -> Vec<usize> {
        (0..tabs.len())
            .filter(|index| tabs.tab_bounds(*index).is_some())
            .collect()
    }

    #[test]
    fn tabs_that_fit_have_no_arrows() {
        let tabs = narrow(2000, 2);
        assert!(tabs.arrows.is_none());
        assert_eq!(visible(&tabs), [0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn overflowing_tabs_keep_the_selected_tab_in_the_strip() {
        let wide = narrow(2000, 0);
        let full = wide.bounds.width;
        let available = full / 3;
        for selected in 0..SIX.len() {
            let tabs = narrow(available, selected);
            let shown = visible(&tabs);
            assert!(
                shown.contains(&selected),
                "tab {selected} is in view: {shown:?}"
            );
            assert!(shown.len() < SIX.len(), "some tabs are out of the run");
            assert!(
                shown.windows(2).all(|pair| pair[1] == pair[0] + 1),
                "the run is contiguous: {shown:?}"
            );
            let right_edge = 14 + available as i32;
            for index in &shown {
                let rect = tabs.tab_bounds(*index).unwrap();
                assert!(
                    rect.x >= 14 && rect.x + rect.width as i32 <= right_edge,
                    "tab {index} fits the strip: {rect:?}"
                );
            }
            assert!(tabs.arrows.is_some());
        }
    }

    #[test]
    fn the_arrows_select_the_neighbouring_tab_and_wrap() {
        let mut tabs = narrow(200, 0);
        let (back, forward) = tabs.arrows.expect("the tabs overflow");
        let press = |rect: Rect| Event::PointerPressed {
            x: rect.x + 2,
            y: rect.y + 2,
            button: PointerButton::Primary,
        };
        assert_eq!(tabs.handle(&press(forward)), TabsOutcome::Selected(1));
        assert_eq!(tabs.handle(&press(back)), TabsOutcome::Selected(0));
        assert_eq!(
            tabs.handle(&press(back)),
            TabsOutcome::Selected(5),
            "the previous arrow wraps to the last tab"
        );
        assert!(tabs.tab_bounds(5).is_some(), "the selection is in view");
    }

    #[test]
    fn keyboard_and_accessibility_reach_every_hidden_tab() {
        let mut tabs = narrow(200, 0);
        tabs.focused = true;
        for step in 1..SIX.len() {
            assert_eq!(tabs.handle(&key(Key::Right)), TabsOutcome::Selected(step));
            assert!(
                tabs.tab_bounds(step).is_some(),
                "tab {step} is in view once selected"
            );
        }
        let first_again = tabs.tab_id(0);
        assert_eq!(
            tabs.handle(&Event::Accessibility(AccessibilityRequest::Focus(
                first_again
            ))),
            TabsOutcome::Selected(0)
        );
        assert!(tabs.tab_bounds(0).is_some());
        // Every tab stays a node, even the ones outside the run.
        assert_eq!(tabs.accessibility_nodes().len(), 1 + SIX.len());
    }

    #[test]
    fn a_wide_selected_tab_stops_before_the_right_arrow() {
        // The selected tab is the widest label, in a strip too narrow for it.
        let tabs = narrow(150, 5);
        let (left, right) = tabs.arrows.expect("the strip overflows");
        let arrow_edge = right.x;
        for index in visible(&tabs) {
            let rect = tabs.tab_bounds(index).expect("visible tabs have bounds");
            assert!(
                rect.x >= left.x + left.width as i32,
                "tab {index} is under the left arrow"
            );
            assert!(
                rect.x + rect.width as i32 <= arrow_edge,
                "tab {index} reaches the right arrow"
            );
        }
        let selected = tabs.tab_bounds(5).expect("the selected tab is shown");
        assert!(selected.width <= tabs.widths[5]);
    }

    #[test]
    fn the_selected_tab_paints_its_plate_and_nothing_past_the_strip() {
        for factor in [1, 2] {
            let scale = Scale::new(factor).unwrap();
            // The strip ends at x=214, so the surface is wider than that.
            let blank = RasterSurface::new(400, 80, scale).unwrap();
            let surface = RasterSurface::new(400, 80, scale).unwrap();
            let tabs = narrow(200, 4);
            tabs.paint(&surface, &Font::system(12.0)).unwrap();
            let rect = tabs.tab_bounds(4).expect("the selected tab is shown");
            let f = factor as i32;
            // A device pixel inside the selected tab's plate, at its left edge.
            let inside = (
                ((rect.x + 2) * f) as u32,
                ((rect.y + rect.height as i32 / 2) * f) as u32,
            );
            assert_ne!(
                surface.pixel(inside.0, inside.1),
                blank.pixel(inside.0, inside.1),
                "the selected plate paints at {factor}x"
            );
            // Just past the strip's right edge, nothing is drawn.
            let past = (
                ((tabs.bounds.x + tabs.bounds.width as i32 + 2) * f) as u32,
                (50 * f) as u32,
            );
            assert_eq!(
                surface.pixel(past.0, past.1),
                blank.pixel(past.0, past.1),
                "nothing paints past the strip at {factor}x"
            );
        }
    }

    #[test]
    fn a_strip_too_narrow_for_its_arrows_paints_without_error() {
        for available in [0, 1, 12, 40] {
            let surface = RasterSurface::new(80, 40, Scale::new(1).unwrap()).unwrap();
            let tabs = narrow(available, 3);
            tabs.paint(&surface, &Font::system(12.0)).unwrap();
            if let Some(rect) = tabs.tab_bounds(3) {
                assert!(
                    rect.width <= available.max(1),
                    "the tab stays inside {available}"
                );
            }
        }
    }

    #[test]
    fn overflowing_tabs_paint_at_one_and_two_times_scale() {
        for factor in [1, 2] {
            let surface = RasterSurface::new(200, 40, Scale::new(factor).unwrap()).unwrap();
            let tabs = narrow(200, 4);
            tabs.paint(&surface, &Font::system(12.0)).unwrap();
        }
    }

    #[test]
    fn focus_lands_on_the_strip_when_there_are_no_tabs() {
        let mut tabs = Tabs::new(40, "Empty", &[], 0);
        tabs.layout(0, 0, 100, &Font::system(12.0));
        assert_eq!(tabs.focus_node(), 40);
        assert!(tabs
            .accessibility_nodes()
            .iter()
            .any(|(id, _)| *id == tabs.focus_node()));
    }
}
