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
    /// Icon-grid layout: cells this wide, as many across as the width allows.
    /// `None` is the one-column list.
    cell_width: Option<u32>,
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
            cell_width: None,
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
        // The column count depends on the width, so the height does too.
        self.set_len(self.len);
    }

    /// Show the rows as an icon grid of `cell_width`-wide cells (each
    /// `row_height` tall, filled left to right) or, with `None`, as the
    /// one-column list. The same selection and keyboard rules apply in both.
    pub fn set_cell_width(&mut self, cell_width: Option<u32>) {
        self.cell_width = cell_width.map(|width| width.max(1));
        self.set_len(self.len);
    }

    /// Cells across: always 1 for the list.
    pub fn columns(&self) -> usize {
        match self.cell_width {
            None => 1,
            Some(width) => ((self.scroll.content_area().width / width) as usize).max(1),
        }
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
        let lines = len.div_ceil(self.columns());
        self.scroll
            .set_content(lines as u64 * u64::from(self.row_height));
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

    /// Like [`Self::select_only`], but the scroll position stays where it is.
    /// For a list that its app refreshes while the user reads it (a live
    /// table whose rows move), so the view does not jump back each refresh.
    pub fn select_without_scrolling(&mut self, index: usize) {
        if index >= self.len {
            return;
        }
        self.selected.clear();
        self.selected.insert(index);
        self.cursor = Some(index);
        self.anchor = Some(index);
    }

    /// Select exactly `indices` (out-of-range ones are dropped); the cursor
    /// moves to the first and is scrolled into view.
    pub fn set_selection(&mut self, indices: &[usize]) {
        self.selected = indices.iter().copied().filter(|&i| i < self.len).collect();
        self.cursor = self.selected.first().copied();
        self.anchor = self.cursor;
        if let Some(first) = self.cursor {
            self.reveal(first);
        }
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
        let line = (index / self.columns()) as u64;
        self.scroll.reveal(line * height, height);
    }

    /// Rows that are at least partly visible.
    pub fn visible_rows(&self) -> std::ops::Range<usize> {
        let height = u64::from(self.row_height);
        let columns = self.columns();
        let first = (self.scroll.offset() / height) as usize * columns;
        let last =
            (self.scroll.offset() + self.scroll.viewport()).div_ceil(height) as usize * columns;
        first.min(self.len)..last.min(self.len)
    }

    /// Where row `index` is drawn (may extend past the view when partial).
    pub fn row_rect(&self, index: usize) -> Rect {
        let area = self.scroll.content_area();
        let columns = self.columns();
        let line = index / columns;
        let column = index % columns;
        let top = line as i64 * i64::from(self.row_height) - self.scroll.offset() as i64;
        let (x, width) = match self.cell_width {
            None => (area.x, area.width),
            Some(cell) => (area.x + (column as u32 * cell) as i32, cell),
        };
        Rect {
            x,
            y: area.y + top.clamp(i64::from(i32::MIN / 2), i64::from(i32::MAX / 2)) as i32,
            width,
            height: self.row_height,
        }
    }

    pub fn row_at(&self, x: i32, y: i32) -> Option<usize> {
        let area = self.scroll.content_area();
        if !area.contains(x, y) {
            return None;
        }
        let offset = (y - area.y) as u64 + self.scroll.offset();
        let line = (offset / u64::from(self.row_height)) as usize;
        let column = match self.cell_width {
            None => 0,
            Some(cell) => ((x - area.x) as u32 / cell) as usize,
        };
        if column >= self.columns() {
            return None;
        }
        let index = line * self.columns() + column;
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
                // Up and Down step a whole line of cells; in the grid Left and
                // Right step one cell. The list has one cell per line.
                let columns = self.columns();
                let grid = self.cell_width.is_some();
                let target = match key {
                    Key::Down => Some(cursor.map_or(0, |index| index + columns)),
                    Key::Up => Some(cursor.map_or(0, |index| index.saturating_sub(columns))),
                    Key::Left if grid => Some(cursor.map_or(0, |index| index.saturating_sub(1))),
                    Key::Right if grid => Some(cursor.map_or(0, |index| index + 1)),
                    Key::Home => Some(0),
                    Key::End => Some(self.len - 1),
                    Key::PageDown => Some(cursor.map_or(0, |index| index + page * columns)),
                    Key::PageUp => {
                        Some(cursor.map_or(0, |index| index.saturating_sub(page * columns)))
                    }
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

    /// A 350×100 grid of 100-wide, 20-tall cells: three across (50 px to spare).
    fn grid(len: usize) -> ListView {
        let mut list = ListView::new(4, "Icons", 20);
        list.set_cell_width(Some(100));
        list.set_bounds(Rect {
            x: 10,
            y: 10,
            width: 350,
            height: 100,
        });
        list.set_len(len);
        list.focused = true;
        list
    }

    #[test]
    fn grid_fills_cells_left_to_right_and_the_list_keeps_one_column() {
        let grid_view = grid(10);
        assert_eq!(grid_view.columns(), 3);
        // Index 4 is the second cell of the second line.
        assert_eq!(grid_view.row_rect(4).x, 10 + 100);
        assert_eq!(grid_view.row_rect(4).y, 10 + 20);
        assert_eq!(grid_view.row_rect(4).width, 100);
        assert_eq!(grid_view.row_at(10 + 100 + 5, 10 + 20 + 5), Some(4));
        // Past the last column of a line is no cell at all.
        assert_eq!(grid_view.row_at(10 + 330, 10 + 5), None);
        // The last, partial line exists but nothing sits past the end.
        assert_eq!(grid_view.row_at(10 + 5, 10 + 60 + 5), Some(9));
        assert_eq!(grid_view.row_at(10 + 105, 10 + 60 + 5), None);
        let plain = list(10);
        assert_eq!(plain.columns(), 1);
        assert_eq!(plain.row_rect(4).y, 10 + 4 * 20);
        // A scrollbar takes its strip, so rows span the content width, not the bounds.
        assert_eq!(plain.row_rect(4).width, plain.scroll().content_area().width);
    }

    #[test]
    fn grid_keyboard_moves_by_line_and_cell_and_list_ignores_left_right() {
        let mut grid = grid(10);
        let labels = |_: usize| String::new();
        let key_of = |key| key_event(key);
        grid.handle(&key_of(Key::Down), Modifiers::default(), &labels);
        assert_eq!(grid.cursor(), Some(0));
        grid.handle(&key_of(Key::Down), Modifiers::default(), &labels);
        assert_eq!(grid.cursor(), Some(3), "Down moves one line of three");
        grid.handle(&key_of(Key::Right), Modifiers::default(), &labels);
        assert_eq!(grid.cursor(), Some(4), "Right moves one cell");
        grid.handle(&key_of(Key::Up), Modifiers::default(), &labels);
        assert_eq!(grid.cursor(), Some(1));
        grid.handle(&key_of(Key::Left), Modifiers::default(), &labels);
        grid.handle(&key_of(Key::Left), Modifiers::default(), &labels);
        assert_eq!(grid.cursor(), Some(0), "Left stops at the first cell");
        grid.handle(&key_of(Key::End), Modifiers::default(), &labels);
        assert_eq!(grid.cursor(), Some(9));
        assert_eq!(grid.selection(), vec![9], "selection follows the cursor");

        let mut plain = list(10);
        plain.handle(&key_of(Key::Down), Modifiers::default(), &labels);
        plain.handle(&key_of(Key::Right), Modifiers::default(), &labels);
        assert_eq!(plain.cursor(), Some(0), "Right does nothing in the list");
    }

    #[test]
    fn pointer_selection_in_the_grid_selects_the_cell_under_the_pointer() {
        let mut grid = grid(10);
        let labels = |_: usize| String::new();
        let press = Event::PointerPressed {
            x: 10 + 205,
            y: 10 + 25,
            button: PointerButton::Primary,
        };
        grid.handle(&press, Modifiers::default(), &labels);
        assert_eq!(grid.selection(), vec![5]);
    }

    #[test]
    fn the_grid_scrolls_by_lines_and_visible_rows_are_whole_lines() {
        // 30 cells over 3 across is 10 lines of 20 px; a 100 px view shows 5.
        let mut grid = grid(30);
        assert_eq!(grid.visible_rows(), 0..15);
        grid.handle(&key_event(Key::End), Modifiers::default(), &|_| {
            String::new()
        });
        let visible = grid.visible_rows();
        assert!(visible.contains(&29), "the revealed last cell is on screen");
        assert_eq!(
            visible.start % 3,
            0,
            "a visible range starts at a line boundary"
        );
        assert_eq!(visible.end, 30);
    }

    fn key_event(key: Key) -> Event {
        Event::KeyPressed {
            key,
            modifiers: Modifiers::default(),
        }
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

    #[test]
    fn select_without_scrolling_keeps_the_view_where_it_is() {
        let mut list = list(100);
        list.select_only(0);
        let before = list.scroll().offset();
        list.select_without_scrolling(60);
        assert_eq!(list.scroll().offset(), before);
        assert_eq!(list.selection(), vec![60]);
        assert_eq!(list.cursor(), Some(60));
        // Out of range is ignored, like select_only.
        list.select_without_scrolling(500);
        assert_eq!(list.selection(), vec![60]);
    }
}
