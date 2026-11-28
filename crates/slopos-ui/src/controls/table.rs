//! Table view: a header row of columns above virtualised body rows.
//!
//! The body is a [`ListView`], so selection, keyboard movement, scrolling
//! and type-ahead behave as they do in a list. The table adds fixed-width
//! columns, the header, and the cell text. A primary press on the header
//! reports the column ([`TableOutcome::HeaderClicked`]); sorting is the app's
//! job. Cells are drawn clipped to their column.
//!
//! Accessibility: a `Table` node whose children are a header `Row` (with one
//! `ColumnHeader` per column) and one `Row` per visible body row, each with a
//! `Cell` child per column.

use super::list::{ListOutcome, ListView};
use crate::event::{Event, Modifiers, PointerButton};
use crate::foundation::geometry::Rect;
use crate::render::{RasterSurface, RenderError};
use crate::text::{measure, Font};
use crate::theme::palette;

/// Height of the header row, in logical pixels.
const HEADER_HEIGHT: u32 = 20;
/// Horizontal text inset from a cell's left edge.
const PADDING: i32 = 4;
/// The most columns a table keeps; extra columns are dropped.
pub const MAX_COLUMNS: usize = 256;
/// At most this many body rows are exposed to assistive technology at once.
const MAX_ACCESSIBLE_ROWS: usize = 256;

/// Node slots, combined with the table id in the high 32 bits.
const HEADER_ROW_SLOT: u64 = 0x3fff_ffff;
const HEADER_CELL_SLOT: u64 = 0x4000_0000;
const BODY_CELL_SLOT: u64 = 0x8000_0000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableColumn {
    pub title: String,
    /// Width in logical pixels; a zero width hides the column's cells.
    pub width: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableOutcome {
    /// The event did not concern the table.
    Ignored,
    /// Scroll position or focus look changed; repaint.
    Changed,
    /// The selection or cursor moved.
    SelectionChanged,
    /// Enter, or a double-click, on body row `index`.
    Activated(usize),
    /// A secondary click or the Menu key; `row` is `None` below the last row.
    ContextMenu { row: Option<usize>, x: i32, y: i32 },
    /// A primary press on the header of column `index`.
    HeaderClicked(usize),
    /// A press landed in the table body; the app should give it focus.
    FocusRequested,
}

#[derive(Debug, Clone)]
pub struct TableView {
    pub id: u64,
    pub label: String,
    columns: Vec<TableColumn>,
    rows: Vec<Vec<String>>,
    bounds: Rect,
    focused: bool,
    enabled: bool,
    list: ListView,
}

fn clamp_i32(value: i64) -> i32 {
    i32::try_from(value).unwrap_or(if value < 0 { i32::MIN } else { i32::MAX })
}

impl TableView {
    pub fn new(id: u64, label: &str, row_height: u32) -> Self {
        Self {
            id,
            label: label.to_owned(),
            columns: Vec::new(),
            rows: Vec::new(),
            bounds: Rect::default(),
            focused: false,
            enabled: true,
            list: ListView::new(id, label, row_height),
        }
    }

    pub fn bounds(&self) -> Rect {
        self.bounds
    }

    /// Set the whole table's rectangle; the body is the part under the header.
    pub fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
        self.list.set_bounds(Rect {
            x: bounds.x,
            y: bounds.y.saturating_add(HEADER_HEIGHT as i32),
            width: bounds.width,
            height: bounds.height.saturating_sub(HEADER_HEIGHT),
        });
    }

    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        self.list.focused = focused;
    }

    pub fn focused(&self) -> bool {
        self.focused
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.list.enabled = enabled;
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Replace the columns; more than [`MAX_COLUMNS`] are dropped.
    pub fn set_columns(&mut self, mut columns: Vec<TableColumn>) {
        columns.truncate(MAX_COLUMNS);
        self.columns = columns;
    }

    pub fn columns(&self) -> &[TableColumn] {
        &self.columns
    }

    /// Replace the body. The selection and scroll position are kept where
    /// their rows still exist, so a refreshed table does not jump.
    pub fn set_rows(&mut self, rows: Vec<Vec<String>>) {
        self.list.set_len(rows.len());
        self.rows = rows;
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// The text of cell (`row`, `column`); empty when the row is short or
    /// either index is out of range.
    pub fn cell(&self, row: usize, column: usize) -> &str {
        self.rows
            .get(row)
            .and_then(|cells| cells.get(column))
            .map_or("", String::as_str)
    }

    pub fn selection(&self) -> Vec<usize> {
        self.list.selection()
    }

    pub fn select_only(&mut self, row: usize) {
        self.list.select_only(row);
    }

    /// The header rectangle of column `column`, or `None` when out of range.
    /// Columns lie side by side from the table's left edge; a column that
    /// would start beyond the i32 range is clamped to it.
    pub fn column_rect(&self, column: usize) -> Option<Rect> {
        let width = self.columns.get(column)?.width;
        let start: i64 = self.columns[..column]
            .iter()
            .map(|column| i64::from(column.width))
            .sum::<i64>()
            + i64::from(self.bounds.x);
        Some(Rect {
            x: clamp_i32(start),
            y: self.bounds.y,
            width,
            height: HEADER_HEIGHT,
        })
    }

    /// The column whose header contains the point, if any.
    pub fn column_at(&self, x: i32, y: i32) -> Option<usize> {
        let header = Rect {
            x: self.bounds.x,
            y: self.bounds.y,
            width: self.bounds.width,
            height: HEADER_HEIGHT,
        };
        if !header.contains(x, y) {
            return None;
        }
        (0..self.columns.len()).find(|&column| {
            self.column_rect(column)
                .is_some_and(|rect| rect.width > 0 && rect.contains(x, y))
        })
    }

    /// Handle an event. `pointer_modifiers` are the modifiers held during a
    /// pointer event.
    pub fn handle(&mut self, event: &Event, pointer_modifiers: Modifiers) -> TableOutcome {
        if !self.enabled {
            return TableOutcome::Ignored;
        }
        if let Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
        } = event
        {
            if let Some(column) = self.column_at(*x, *y) {
                return TableOutcome::HeaderClicked(column);
            }
        }
        // Type-ahead matches the first column.
        let rows = &self.rows;
        let label_of = |index: usize| {
            rows.get(index)
                .and_then(|cells| cells.first())
                .cloned()
                .unwrap_or_default()
        };
        match self.list.handle(event, pointer_modifiers, &label_of) {
            ListOutcome::Ignored => TableOutcome::Ignored,
            ListOutcome::Changed => TableOutcome::Changed,
            ListOutcome::SelectionChanged => TableOutcome::SelectionChanged,
            ListOutcome::FocusRequested => TableOutcome::FocusRequested,
            ListOutcome::Activated(index) => TableOutcome::Activated(index),
            ListOutcome::ContextMenu { index, x, y } => {
                TableOutcome::ContextMenu { row: index, x, y }
            }
        }
    }

    /// The text rectangle of body row `index`, cell `column`.
    fn cell_rect(&self, index: usize, column: usize) -> Option<Rect> {
        let header = self.column_rect(column)?;
        let row = self.list.row_rect(index);
        Some(Rect {
            x: header.x,
            y: row.y,
            width: header.width,
            height: row.height,
        })
    }

    /// Paint the header, the body and the cells.
    pub fn paint(&self, surface: &RasterSurface, font: &Font) -> Result<(), RenderError> {
        let colors = palette();
        surface.fill_rect(
            Rect {
                x: self.bounds.x,
                y: self.bounds.y,
                width: self.bounds.width,
                height: HEADER_HEIGHT.min(self.bounds.height),
            },
            colors.surface,
        )?;
        for (column, spec) in self.columns.iter().enumerate() {
            let Some(rect) = self.column_rect(column) else {
                continue;
            };
            surface.with_clip(rect, || {
                surface.stroke_rect(rect, colors.bevel_shadow)?;
                let extent = measure(&spec.title, font);
                let y = rect.y + (rect.height as i32 - extent.height as i32) / 2;
                surface.draw_text(&spec.title, rect.x + PADDING, y, font, colors.ink)
            })?;
        }

        let rows = &self.rows;
        let columns = &self.columns;
        self.list
            .paint(surface, |surface, index, row_rect, selected| {
                let ink = if selected {
                    colors.selection_ink
                } else {
                    colors.ink
                };
                let mut x = row_rect.x;
                for (column, spec) in columns.iter().enumerate() {
                    let cell = Rect {
                        x,
                        y: row_rect.y,
                        width: spec.width,
                        height: row_rect.height,
                    };
                    x = x.saturating_add(i32::try_from(spec.width).unwrap_or(i32::MAX));
                    let Some(text) = rows.get(index).and_then(|cells| cells.get(column)) else {
                        continue;
                    };
                    if text.is_empty() || cell.width == 0 {
                        continue;
                    }
                    let extent = measure(text, font);
                    let y = cell.y + (cell.height as i32 - extent.height as i32) / 2;
                    surface.with_clip(cell, || {
                        surface.draw_text(text, cell.x + PADDING, y, font, ink)
                    })?;
                }
                Ok(())
            })
    }

    /// The accessibility id of the header row.
    pub fn header_row_node_id(&self) -> u64 {
        (self.id << 32) | HEADER_ROW_SLOT
    }

    /// The accessibility id of body row `index`.
    pub fn row_node_id(&self, index: usize) -> u64 {
        self.list.row_node_id(index)
    }

    /// The node a screen reader should focus while the table has focus.
    pub fn focus_node(&self) -> u64 {
        self.list.focus_node()
    }

    /// Map an accessibility node id back to a body row index.
    pub fn row_for_node(&self, node: u64) -> Option<usize> {
        self.list.row_for_node(node)
    }

    /// A `Table` node, a header `Row` with `ColumnHeader` cells, and a `Row`
    /// with `Cell` nodes for each visible body row (capped). Cell ids are
    /// derived from the visible position, so they stay within 32 bits.
    pub fn accessibility_nodes(&self) -> Vec<(u64, accesskit::Node)> {
        let columns = self.columns.len();
        let visible: Vec<usize> = self.list.visible_rows().take(MAX_ACCESSIBLE_ROWS).collect();

        let mut table = accesskit::Node::new(accesskit::Role::Table);
        table.set_label(self.label.as_str());
        table.set_bounds(super::bounds_of(self.bounds));
        table.set_row_count(self.rows.len() + 1);
        table.set_column_count(columns);
        table.add_action(accesskit::Action::Focus);
        let mut children = vec![accesskit::NodeId(self.header_row_node_id())];
        children.extend(
            visible
                .iter()
                .map(|&index| accesskit::NodeId(self.list.row_node_id(index))),
        );
        table.set_children(children);
        let mut nodes = vec![(self.id, table)];

        let mut header = accesskit::Node::new(accesskit::Role::Row);
        header.set_row_index(0);
        let mut header_cells = Vec::with_capacity(columns);
        for (column, spec) in self.columns.iter().enumerate() {
            let id = (self.id << 32) | (HEADER_CELL_SLOT + column as u64);
            let mut cell = accesskit::Node::new(accesskit::Role::ColumnHeader);
            cell.set_label(spec.title.as_str());
            cell.set_column_index(column);
            if let Some(rect) = self.column_rect(column) {
                cell.set_bounds(super::bounds_of(rect));
            }
            cell.add_action(accesskit::Action::Click);
            header_cells.push(accesskit::NodeId(id));
            nodes.push((id, cell));
        }
        header.set_children(header_cells);
        nodes.push((self.header_row_node_id(), header));

        for (position, &index) in visible.iter().enumerate() {
            let mut row = accesskit::Node::new(accesskit::Role::Row);
            row.set_row_index(index + 1);
            row.set_selected(self.list.is_selected(index));
            row.add_action(accesskit::Action::Click);
            row.add_action(accesskit::Action::Focus);
            let mut cells = Vec::with_capacity(columns);
            for column in 0..columns {
                let slot = BODY_CELL_SLOT + (position * columns + column) as u64;
                let id = (self.id << 32) | slot;
                let mut cell = accesskit::Node::new(accesskit::Role::Cell);
                cell.set_label(self.cell(index, column));
                cell.set_column_index(column);
                if let Some(rect) = self.cell_rect(index, column) {
                    cell.set_bounds(super::bounds_of(rect));
                }
                cells.push(accesskit::NodeId(id));
                nodes.push((id, cell));
            }
            row.set_children(cells);
            nodes.push((self.list.row_node_id(index), row));
        }
        nodes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Key;
    use crate::foundation::{color::Color, geometry::Scale};

    fn column(title: &str, width: u32) -> TableColumn {
        TableColumn {
            title: title.to_owned(),
            width,
        }
    }

    fn table() -> TableView {
        let mut table = TableView::new(4, "Files", 20);
        table.set_bounds(Rect {
            x: 10,
            y: 10,
            width: 200,
            height: 120,
        });
        table.set_columns(vec![column("Name", 120), column("Size", 60)]);
        table.set_rows(
            ["alpha", "beta", "gamma", "delta", "epsilon"]
                .iter()
                .enumerate()
                .map(|(index, name)| vec![(*name).to_owned(), format!("{}", index * 10)])
                .collect(),
        );
        table.set_focused(true);
        table
    }

    fn key(key: Key) -> Event {
        Event::KeyPressed {
            key,
            modifiers: Modifiers::default(),
        }
    }

    fn press(x: i32, y: i32) -> Event {
        Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
        }
    }

    fn opaque(color: Color) -> u32 {
        0xff00_0000
            | (u32::from(color.red) << 16)
            | (u32::from(color.green) << 8)
            | u32::from(color.blue)
    }

    #[test]
    fn keyboard_moves_and_activates_body_rows() {
        let mut table = table();
        assert_eq!(
            table.handle(&key(Key::Down), Modifiers::default()),
            TableOutcome::SelectionChanged
        );
        assert_eq!(table.selection(), vec![0]);
        table.handle(&key(Key::End), Modifiers::default());
        assert_eq!(table.selection(), vec![4]);
        assert_eq!(
            table.handle(&key(Key::Enter), Modifiers::default()),
            TableOutcome::Activated(4)
        );
        assert_eq!(table.cell(4, 0), "epsilon");
        assert_eq!(table.cell(4, 1), "40");
    }

    #[test]
    fn header_presses_report_the_column_and_body_presses_select() {
        let mut table = table();
        // Header row is y 10..30; columns start at x 10 and 130.
        assert_eq!(
            table.handle(&press(15, 15), Modifiers::default()),
            TableOutcome::HeaderClicked(0)
        );
        assert_eq!(
            table.handle(&press(140, 15), Modifiers::default()),
            TableOutcome::HeaderClicked(1)
        );
        assert_eq!(
            table.handle(&press(300, 15), Modifiers::default()),
            TableOutcome::Ignored
        );
        // The first body row starts at y 30.
        assert_eq!(
            table.handle(&press(40, 35), Modifiers::default()),
            TableOutcome::SelectionChanged
        );
        assert_eq!(table.selection(), vec![0]);
        assert_eq!(table.column_at(15, 15), Some(0));
        assert_eq!(table.column_at(15, 35), None, "the body is not the header");
    }

    #[test]
    fn column_geometry_is_side_by_side_and_bounded() {
        let mut table = table();
        assert_eq!(table.column_rect(0).map(|rect| rect.x), Some(10));
        assert_eq!(table.column_rect(1).map(|rect| rect.x), Some(130));
        assert_eq!(table.column_rect(2), None);
        table.set_columns(vec![column("Huge", u32::MAX), column("Again", u32::MAX)]);
        // Sums past the i32 range clamp rather than wrap or panic.
        assert_eq!(table.column_rect(1).map(|rect| rect.x), Some(i32::MAX));
        table.set_columns(vec![column("x", 1); MAX_COLUMNS + 10]);
        assert_eq!(table.columns().len(), MAX_COLUMNS);
    }

    #[test]
    fn painting_has_reference_pixels_for_header_and_selected_row_at_1x_and_2x() {
        for scale in [1, 2] {
            let mut table = table();
            table.select_only(1);
            let surface = RasterSurface::new(240, 140, Scale::new(scale).unwrap()).unwrap();
            surface.clear(Color::WHITE).unwrap();
            table.paint(&surface, &Font::system(12.0)).unwrap();
            let at = |x: u32, y: u32| (x * scale, y * scale);
            // Header fill, clear of the title (which starts at x 14).
            let (px, py) = at(11, 11);
            let header_fill = surface.pixel(px, py);
            assert_eq!(
                header_fill,
                Some(opaque(palette().surface)),
                "scale {scale}"
            );
            // Body row 1 spans y 50..70 and is selected. Probe in the empty
            // right part of the Size column, clear of the text and the ring.
            let (px, py) = at(175, 60);
            assert_eq!(
                surface.pixel(px, py),
                Some(opaque(palette().selection)),
                "scale {scale}"
            );
            // Row 0 is not selected.
            let (px, py) = at(175, 40);
            assert_ne!(
                surface.pixel(px, py),
                Some(opaque(palette().selection)),
                "scale {scale}"
            );
        }
    }

    #[test]
    fn accessibility_exposes_header_rows_and_cells() {
        let mut table = table();
        table.select_only(2);
        let nodes = table.accessibility_nodes();
        let by_id = |id: u64| {
            nodes
                .iter()
                .find(|(node_id, _)| *node_id == id)
                .map(|(_, node)| node)
        };
        // The table node uses the bare control id, as ListView's node does.
        let root = by_id(4).expect("the table node");
        assert_eq!(root.role(), accesskit::Role::Table);
        assert_eq!(root.row_count(), Some(6));
        assert_eq!(root.column_count(), Some(2));

        let header_cell = by_id((4 << 32) | (HEADER_CELL_SLOT + 1)).expect("Size header");
        assert_eq!(header_cell.role(), accesskit::Role::ColumnHeader);
        assert_eq!(header_cell.label(), Some("Size"));
        assert_eq!(header_cell.column_index(), Some(1));

        let row = by_id(table.row_node_id(2)).expect("gamma row");
        assert_eq!(row.role(), accesskit::Role::Row);
        assert_eq!(row.is_selected(), Some(true));
        assert_eq!(row.row_index(), Some(3));

        // The first visible body row's first cell carries its text.
        let cell = by_id((4 << 32) | (BODY_CELL_SLOT)).expect("alpha cell");
        assert_eq!(cell.role(), accesskit::Role::Cell);
        assert_eq!(cell.label(), Some("alpha"));
        assert_eq!(table.row_for_node(table.row_node_id(2)), Some(2));
        assert_eq!(table.focus_node(), table.row_node_id(2));
    }

    #[test]
    fn malformed_geometry_and_short_rows_never_fail() {
        let surface = RasterSurface::new(240, 140, Scale::new(1).unwrap()).unwrap();
        let mut table = table();
        table.set_columns(vec![column("Zero", 0), column("", 40)]);
        table.set_rows(vec![vec!["only one".to_owned()], Vec::new()]);
        table.set_bounds(Rect {
            x: 10,
            y: 10,
            width: 0,
            height: 5,
        });
        table.paint(&surface, &Font::system(12.0)).unwrap();
        assert_eq!(table.column_at(10, 10), None);
        assert_eq!(table.cell(1, 0), "");
        assert_eq!(table.cell(9, 9), "");

        let mut empty = TableView::new(5, "Empty", 0);
        empty.set_bounds(Rect {
            x: -1_000_000,
            y: -1_000_000,
            width: 2_000_000,
            height: 2_000_000,
        });
        empty.paint(&surface, &Font::system(12.0)).unwrap();
        assert_eq!(empty.column_at(0, 0), None);
        assert!(empty.accessibility_nodes().len() >= 2);
        assert_eq!(
            empty.handle(&key(Key::Down), Modifiers::default()),
            TableOutcome::Ignored
        );
    }
}
