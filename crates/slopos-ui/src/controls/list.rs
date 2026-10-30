//! Virtualized list with single or multiple selection.
//!
//! A [`ListView`] knows only how many rows there are, how tall they are and
//! which are selected. Painting asks the app to draw just the visible rows,
//! so a directory with 100 000 entries costs the same per frame as one with
//! ten. Rows are activated by double-click or Enter; Secondary click asks for
//! a context menu. Keyboard: Up/Down/Home/End/PageUp/PageDown move the
//! cursor (Shift extends, Ctrl moves without selecting), Space toggles with
//! Ctrl, Ctrl+A selects all, and a printable key jumps to the next row whose
//! label starts with it.

use std::collections::BTreeSet;

use super::scroll::{ScrollOutcome, ScrollState};
use crate::event::{Event, Key, Modifiers, PointerButton};
use crate::foundation::geometry::Rect;
use crate::render::{RasterSurface, RenderError};
use crate::theme::palette;

/// At most this many row nodes are exposed to assistive technology at once
/// (the visible rows, capped), keeping tree updates bounded.
const MAX_ACCESSIBLE_ROWS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListOutcome {
    Ignored,
    /// Scroll position or focus look changed; repaint.
    Changed,
    /// The selection or cursor changed.
    SelectionChanged,
    /// Double-click or Enter on a row.
    Activated(usize),
    /// Secondary click (or the Menu key): show a context menu at `x, y` for
    /// `index` (`None` when the click was below the last row).
    ContextMenu {
        index: Option<usize>,
        x: i32,
        y: i32,
    },
    /// A press landed in the list; give it keyboard focus.
    FocusRequested,
}

#[derive(Debug, Clone)]
pub struct ListView {
    pub id: u64,
    pub label: String,
    pub row_height: u32,
    pub focused: bool,
    pub enabled: bool,
    /// Allow Shift/Ctrl multiple selection.
    pub multiple: bool,
    scroll: ScrollState,
    len: usize,
    selected: BTreeSet<usize>,
    cursor: Option<usize>,
    anchor: Option<usize>,
}

impl ListView {
    pub fn new(id: u64, label: &str, row_height: u32) -> Self {
        Self {
            id,
            label: label.to_owned(),
            row_height: row_height.max(1),
            focused: false,
            enabled: true,
            multiple: false,
            scroll: ScrollState::default(),
            len: 0,
            selected: BTreeSet::new(),
            cursor: None,
            anchor: None,
        }
    }

    pub fn bounds(&self) -> Rect {
        self.scroll.area
    }

    pub fn set_bounds(&mut self, bounds: Rect) {
        self.scroll.set_area(bounds);
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Change the row count; selection and cursor beyond it are dropped.
    pub fn set_len(&mut self, len: usize) {
        self.len = len;
        self.selected.retain(|&index| index < len);
        self.cursor = self.cursor.filter(|&index| index < len);
        self.anchor = self.anchor.filter(|&index| index < len);
        self.scroll
            .set_content(len as u64 * u64::from(self.row_height));
    }

    /// Clear everything for new content of `len` rows, scrolled to the top.
    pub fn reset(&mut self, len: usize) {
        self.selected.clear();
        self.cursor = None;
        self.anchor = None;
        self.set_len(len);
        self.scroll.scroll_to(0);
    }

    pub fn selection(&self) -> Vec<usize> {
        self.selected.iter().copied().collect()
    }

    pub fn is_selected(&self, index: usize) -> bool {
        self.selected.contains(&index)
    }

    pub fn cursor(&self) -> Option<usize> {
        self.cursor
    }

    pub fn scroll(&self) -> &ScrollState {
        &self.scroll
    }

    /// Select exactly `index`, move the cursor there and scroll to it.
    pub fn select_only(&mut self, index: usize) {
        if index >= self.len {
            return;
        }
        self.selected.clear();
        self.selected.insert(index);
        self.cursor = Some(index);
        self.anchor = Some(index);
        self.reveal(index);
    }

    pub fn select_all(&mut self) {
        if self.multiple {
            self.selected = (0..self.len).collect();
        }
    }

    pub fn clear_selection(&mut self) {
        self.selected.clear();
    }

    fn reveal(&mut self, index: usize) {
        let height = u64::from(self.row_height);
        self.scroll.reveal(index as u64 * height, height);
    }

    /// Rows that are at least partly visible.
    pub fn visible_rows(&self) -> std::ops::Range<usize> {
        let height = u64::from(self.row_height);
        let first = (self.scroll.offset() / height) as usize;
        let last = (self.scroll.offset() + self.scroll.viewport()).div_ceil(height) as usize;
        first.min(self.len)..last.min(self.len)
    }

    /// Where row `index` is drawn (may extend past the view when partial).
    pub fn row_rect(&self, index: usize) -> Rect {
        let area = self.scroll.content_area();
        let top = index as i64 * i64::from(self.row_height) - self.scroll.offset() as i64;
        Rect {
            x: area.x,
            y: area.y + top.clamp(i64::from(i32::MIN / 2), i64::from(i32::MAX / 2)) as i32,
            width: area.width,
            height: self.row_height,
        }
    }

    pub fn row_at(&self, x: i32, y: i32) -> Option<usize> {
        let area = self.scroll.content_area();
        if !area.contains(x, y) {
            return None;
        }
        let offset = (y - area.y) as u64 + self.scroll.offset();
        let index = (offset / u64::from(self.row_height)) as usize;
        (index < self.len).then_some(index)
    }

    fn move_cursor(&mut self, target: usize, modifiers: Modifiers) {
        let target = target.min(self.len.saturating_sub(1));
        self.cursor = Some(target);
        if self.multiple && modifiers.shift {
            let anchor = *self.anchor.get_or_insert(target);
            self.selected = (anchor.min(target)..=anchor.max(target)).collect();
        } else if !(self.multiple && modifiers.control) {
            self.selected.clear();
            self.selected.insert(target);
            self.anchor = Some(target);
        }
        self.reveal(target);
    }

    /// Handle an event. `pointer_modifiers` are the modifiers held during a
    /// pointer event (see [`crate::platform::Control::modifiers`]); `label_of`
    /// gives a row's text for type-ahead.
    pub fn handle(
        &mut self,
        event: &Event,
        pointer_modifiers: Modifiers,
        label_of: &dyn Fn(usize) -> String,
    ) -> ListOutcome {
        if !self.enabled {
            return ListOutcome::Ignored;
        }
        if self.scroll.handle(event) == ScrollOutcome::Scrolled {
            return ListOutcome::Changed;
        }
        match event {
            Event::PointerPressed { x, y, button }
                if self.scroll.content_area().contains(*x, *y) =>
            {
                let row = self.row_at(*x, *y);
                let focus = if self.focused {
                    None
                } else {
                    Some(ListOutcome::FocusRequested)
                };
                match (button, row) {
                    (PointerButton::Secondary, _) => {
                        if let Some(index) = row.filter(|index| !self.selected.contains(index)) {
                            self.select_only(index);
                        }
                        ListOutcome::ContextMenu {
                            index: row,
                            x: *x,
                            y: *y,
                        }
                    }
                    (PointerButton::Primary, Some(index)) => {
                        if self.multiple && pointer_modifiers.control && !pointer_modifiers.shift {
                            if !self.selected.remove(&index) {
                                self.selected.insert(index);
                            }
                            self.cursor = Some(index);
                            self.anchor = Some(index);
                        } else {
                            self.move_cursor(index, pointer_modifiers);
                        }
                        focus.unwrap_or(ListOutcome::SelectionChanged)
                    }
                    (PointerButton::Primary, None) => {
                        self.selected.clear();
                        focus.unwrap_or(ListOutcome::SelectionChanged)
                    }
                    _ => ListOutcome::Ignored,
                }
            }
            Event::DoubleClicked { x, y } => match self.row_at(*x, *y) {
                Some(index) => ListOutcome::Activated(index),
                None => ListOutcome::Ignored,
            },
            Event::KeyPressed { key, modifiers } if self.focused && self.len > 0 => {
                let page = (self.scroll.viewport() / u64::from(self.row_height)).max(1) as usize;
                let cursor = self.cursor;
                let target = match key {
                    Key::Down => Some(cursor.map_or(0, |index| index + 1)),
                    Key::Up => Some(cursor.map_or(0, |index| index.saturating_sub(1))),
                    Key::Home => Some(0),
                    Key::End => Some(self.len - 1),
                    Key::PageDown => Some(cursor.map_or(0, |index| index + page)),
                    Key::PageUp => Some(cursor.map_or(0, |index| index.saturating_sub(page))),
                    _ => None,
                };
                if let Some(target) = target {
                    self.move_cursor(target, *modifiers);
                    return ListOutcome::SelectionChanged;
                }
                match key {
                    Key::Enter => match cursor {
                        Some(index) => ListOutcome::Activated(index),
                        None => ListOutcome::Ignored,
                    },
                    Key::Space if modifiers.control && self.multiple => {
                        if let Some(index) = cursor {
                            if !self.selected.remove(&index) {
                                self.selected.insert(index);
                            }
                        }
                        ListOutcome::SelectionChanged
                    }
                    Key::Character('a' | 'A') if modifiers.control => {
                        self.select_all();
                        ListOutcome::SelectionChanged
                    }
                    Key::Character(c) if !modifiers.control && !modifiers.alt => {
                        let wanted = c.to_lowercase().to_string();
                        let start = cursor.map_or(0, |index| index + 1);
                        let found = (start..self.len)
                            .chain(0..start)
                            .find(|&index| label_of(index).to_lowercase().starts_with(&wanted));
                        match found {
                            Some(index) => {
                                self.select_only(index);
                                ListOutcome::SelectionChanged
                            }
                            None => ListOutcome::Ignored,
                        }
                    }
                    Key::Other(0xff67) => {
                        // XK_Menu: context menu at the cursor row.
                        let rect = self.row_rect(cursor.unwrap_or(0));
                        ListOutcome::ContextMenu {
                            index: cursor,
                            x: rect.x + 16,
                            y: rect.y + rect.height as i32,
                        }
                    }
                    _ => ListOutcome::Ignored,
                }
            }
            _ => ListOutcome::Ignored,
        }
    }

    /// Paint the background, the selection and the scrollbar, and call
    /// `paint_row(surface, index, rect, selected)` for each visible row with
    /// drawing clipped to the list.
    pub fn paint(
        &self,
        surface: &RasterSurface,
        mut paint_row: impl FnMut(&RasterSurface, usize, Rect, bool) -> Result<(), RenderError>,
    ) -> Result<(), RenderError> {
        let colors = palette();
        let area = self.scroll.content_area();
        surface.fill_rect(self.scroll.area, colors.content)?;
        surface.with_clip(area, || {
            for index in self.visible_rows() {
                let rect = self.row_rect(index);
                let selected = self.selected.contains(&index);
                if selected {
                    surface.fill_rect(
                        rect,
                        if self.focused {
                            colors.selection
                        } else {
                            colors.selection_inactive
                        },
                    )?;
                }
                paint_row(surface, index, rect, selected)?;
                if self.focused && self.cursor == Some(index) {
                    super::button::paint_focus_ring(surface, rect)?;
                }
            }
            Ok(())
        })?;
        self.scroll.paint(surface)?;
        let border = Rect {
            x: self.scroll.area.x - 1,
            y: self.scroll.area.y - 1,
            width: self.scroll.area.width + 2,
            height: self.scroll.area.height + 2,
        };
        surface.stroke_rect(
            border,
            if self.focused {
                colors.focus
            } else {
                colors.bevel_shadow
            },
        )
    }

    /// The accessibility id of row `index`'s node.
    pub fn row_node_id(&self, index: usize) -> u64 {
        (self.id << 32) | (index as u64 + 1)
    }

    /// The node the screen reader should focus while the list has focus.
    pub fn focus_node(&self) -> u64 {
        self.cursor
            .filter(|index| self.visible_rows().contains(index))
            .map_or(self.id, |index| self.row_node_id(index))
    }

    /// A `ListBox` node and one `ListBoxOption` per visible row (capped).
    pub fn accessibility_nodes(
        &self,
        label_of: &dyn Fn(usize) -> String,
    ) -> Vec<(u64, accesskit::Node)> {
        let mut list = accesskit::Node::new(accesskit::Role::ListBox);
        list.set_label(self.label.as_str());
        list.set_bounds(super::bounds_of(self.scroll.area));
        if self.multiple {
            list.set_multiselectable();
        }
        list.add_action(accesskit::Action::Focus);
        let rows: Vec<usize> = self.visible_rows().take(MAX_ACCESSIBLE_ROWS).collect();
        list.set_children(
            rows.iter()
                .map(|&index| accesskit::NodeId(self.row_node_id(index)))
                .collect::<Vec<_>>(),
        );
        let mut nodes = vec![(self.id, list)];
        for index in rows {
            let mut row = accesskit::Node::new(accesskit::Role::ListBoxOption);
            row.set_label(label_of(index));
            row.set_bounds(super::bounds_of(self.row_rect(index)));
            row.set_selected(self.selected.contains(&index));
            row.add_action(accesskit::Action::Click);
            row.add_action(accesskit::Action::Focus);
            nodes.push((self.row_node_id(index), row));
        }
        nodes
    }

    /// Map an accessibility node id back to a row of this list.
    pub fn row_for_node(&self, node: u64) -> Option<usize> {
        if node >> 32 != self.id || node & 0xffff_ffff == 0 {
            return None;
        }
        let index = (node & 0xffff_ffff) as usize - 1;
        (index < self.len).then_some(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::color::Color;
    use crate::foundation::geometry::Scale;

    fn names() -> Vec<String> {
        (0..1000)
            .map(|i| format!("{}{i}", ["apple", "banana", "cherry"][i % 3]))
            .collect()
    }

    fn list(len: usize) -> ListView {
        let mut list = ListView::new(3, "Files", 20);
        list.set_bounds(Rect {
            x: 10,
            y: 10,
            width: 200,
            height: 100,
        });
        list.set_len(len);
        list.focused = true;
        list
    }

    fn key(key: Key, shift: bool, control: bool) -> Event {
        Event::KeyPressed {
            key,
            modifiers: Modifiers {
                shift,
                control,
                ..Modifiers::default()
            },
        }
    }

    impl ListView {
        fn handle_key(&mut self, event: &Event, label: &dyn Fn(usize) -> String) -> ListOutcome {
            self.handle(event, Modifiers::default(), label)
        }
    }

    #[test]
    fn control_and_shift_click_extend_the_selection() {
        let label = |_: usize| String::new();
        let mut list = list(10);
        list.multiple = true;
        let click = |y| Event::PointerPressed {
            x: 20,
            y,
            button: PointerButton::Primary,
        };
        let control = Modifiers {
            control: true,
            ..Modifiers::default()
        };
        let shift = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        list.handle(&click(15), Modifiers::default(), &label);
        list.handle(&click(55), control, &label);
        assert_eq!(list.selection(), vec![0, 2]);
        list.handle(&click(95), shift, &label);
        assert_eq!(list.selection(), vec![2, 3, 4]);
        list.handle(&click(55), control, &label);
        assert_eq!(list.selection(), vec![3, 4]);
    }

    #[test]
    fn only_visible_rows_are_painted() {
        let names = names();
        let mut list = list(names.len());
        let surface = RasterSurface::new(220, 120, Scale::new(1).unwrap()).unwrap();
        surface.clear(Color::WHITE).unwrap();
        let mut painted = Vec::new();
        list.paint(&surface, |_, index, _, _| {
            painted.push(index);
            Ok(())
        })
        .unwrap();
        assert_eq!(painted, (0..5).collect::<Vec<_>>());
        list.select_only(500);
        painted.clear();
        list.paint(&surface, |_, index, _, _| {
            painted.push(index);
            Ok(())
        })
        .unwrap();
        assert!(painted.contains(&500) && painted.len() <= 6);
    }

    #[test]
    fn keyboard_navigation_selection_and_activation() {
        let names = names();
        let label = |index: usize| names[index].clone();
        let mut list = list(names.len());
        list.multiple = true;
        assert_eq!(
            list.handle_key(&key(Key::Down, false, false), &label),
            ListOutcome::SelectionChanged
        );
        assert_eq!(list.selection(), vec![0]);
        list.handle_key(&key(Key::Down, true, false), &label);
        list.handle_key(&key(Key::Down, true, false), &label);
        assert_eq!(list.selection(), vec![0, 1, 2]);
        list.handle_key(&key(Key::End, false, false), &label);
        assert_eq!(list.selection(), vec![999]);
        assert!(list.scroll().offset() > 0);
        list.handle_key(&key(Key::PageUp, false, false), &label);
        assert_eq!(list.cursor(), Some(994));
        assert_eq!(
            list.handle_key(&key(Key::Enter, false, false), &label),
            ListOutcome::Activated(994)
        );
        // Type-ahead jumps to the next row starting with the letter.
        list.handle_key(&key(Key::Character('c'), false, false), &label);
        assert_eq!(list.cursor(), Some(995));
        list.handle_key(&key(Key::Character('a'), true, true), &label);
        assert_eq!(list.selection().len(), 1000);
        // Ctrl+Space toggles the cursor row.
        list.handle_key(&key(Key::Space, false, true), &label);
        assert!(!list.is_selected(995));
    }

    #[test]
    fn pointer_select_double_click_and_context_menu() {
        let label = |_: usize| String::new();
        let mut list = list(3);
        list.focused = false;
        let press = |x, y, button| Event::PointerPressed { x, y, button };
        assert_eq!(
            list.handle_key(&press(20, 35, PointerButton::Primary), &label),
            ListOutcome::FocusRequested
        );
        assert_eq!(list.selection(), vec![1]);
        assert_eq!(
            list.handle_key(&Event::DoubleClicked { x: 20, y: 35 }, &label),
            ListOutcome::Activated(1)
        );
        assert_eq!(
            list.handle_key(&press(20, 55, PointerButton::Secondary), &label),
            ListOutcome::ContextMenu {
                index: Some(2),
                x: 20,
                y: 55
            }
        );
        assert_eq!(list.selection(), vec![2]);
        list.focused = true;
        // Below the last row clears the selection.
        list.handle_key(&press(20, 100, PointerButton::Primary), &label);
        assert!(list.selection().is_empty());
        // Outside the list is ignored.
        assert_eq!(
            list.handle_key(&press(300, 300, PointerButton::Primary), &label),
            ListOutcome::Ignored
        );
    }

    #[test]
    fn shrinking_drops_stale_selection_and_empty_lists_ignore_keys() {
        let label = |_: usize| String::new();
        let mut list = list(10);
        list.select_only(8);
        list.set_len(5);
        assert!(list.selection().is_empty());
        assert_eq!(list.cursor(), None);
        list.set_len(0);
        assert_eq!(
            list.handle_key(&key(Key::Down, false, false), &label),
            ListOutcome::Ignored
        );
        assert_eq!(list.visible_rows(), 0..0);
    }

    #[test]
    fn accessibility_is_bounded_and_maps_back() {
        let names = names();
        let label = |index: usize| names[index].clone();
        let mut list = list(names.len());
        list.select_only(2);
        let nodes = list.accessibility_nodes(&label);
        assert_eq!(nodes[0].1.role(), accesskit::Role::ListBox);
        assert_eq!(nodes.len(), 1 + list.visible_rows().len());
        assert_eq!(nodes[3].1.label(), Some("cherry2"));
        assert!(nodes[3].1.is_selected().unwrap_or(false));
        assert_eq!(list.focus_node(), list.row_node_id(2));
        assert_eq!(list.row_for_node(list.row_node_id(2)), Some(2));
        assert_eq!(list.row_for_node(list.id), None);
        assert_eq!(list.row_for_node(list.row_node_id(5000)), None);
    }
}
