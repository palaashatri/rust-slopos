//! The terminal's cell grid: the visible screen, up to [`SCROLLBACK_LINES`]
//! lines of history above it, a cursor, the current text attributes and a
//! selection.
//!
//! This is plain data with no X11 or PTY dependency. The VT parser in
//! `parser.rs` drives it, and the window paints it.

use std::collections::VecDeque;

use unicode_width::UnicodeWidthChar;

/// Lines kept above the screen. The oldest line is dropped first.
pub const SCROLLBACK_LINES: usize = 2000;

/// Zero-width characters kept per cell. Later marks on the same base
/// character are dropped, which keeps the memory per cell fixed.
pub const MARKS_PER_CELL: usize = 2;

const NO_MARKS: [char; MARKS_PER_CELL] = ['\0'; MARKS_PER_CELL];

/// How many columns a cell occupies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Width {
    /// An ordinary one-column character, or a blank.
    #[default]
    Single,
    /// The first of the two columns a wide (CJK or emoji) character fills.
    Wide,
    /// The second column of a wide character. It has no glyph of its own.
    Spacer,
}

/// A text colour: the theme's default, or an entry of the xterm 256-colour
/// table (0-7 normal, 8-15 bright, 16-231 colour cube, 232-255 greys).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Color {
    #[default]
    Default,
    Indexed(u8),
}

/// The attributes new text is written with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Attr {
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub inverse: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub attr: Attr,
    pub width: Width,
    /// Zero-width characters attached to `ch`; `'\0'` marks an unused slot.
    pub marks: [char; MARKS_PER_CELL],
}

impl Cell {
    pub const BLANK: Cell = Cell {
        ch: ' ',
        attr: Attr {
            fg: Color::Default,
            bg: Color::Default,
            bold: false,
            inverse: false,
        },
        width: Width::Single,
        marks: NO_MARKS,
    };

    /// A one-column character with no combining marks.
    fn single(ch: char, attr: Attr) -> Self {
        Self {
            ch,
            attr,
            width: Width::Single,
            marks: NO_MARKS,
        }
    }

    /// Whether the cell is an unstyled-looking space that trailing-blank
    /// trimming may drop.
    fn is_plain_space(&self) -> bool {
        self.ch == ' ' && self.width == Width::Single && self.marks == NO_MARKS
    }

    /// Appends the cell's characters (base and marks) to `out`. A spacer
    /// contributes nothing: its wide character was already written.
    pub fn push_text(&self, out: &mut String) {
        match self.width {
            Width::Spacer => {}
            Width::Single | Width::Wide => {
                out.push(self.ch);
                for mark in self.marks.iter().take_while(|mark| **mark != '\0') {
                    out.push(*mark);
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Line {
    cells: Vec<Cell>,
    /// The line continues on the next line (soft wrap). Copying and
    /// reflowing join such lines without a newline.
    wrapped: bool,
}

impl Line {
    fn blank(cols: usize) -> Self {
        Self {
            cells: vec![Cell::BLANK; cols],
            wrapped: false,
        }
    }

    fn is_blank(&self) -> bool {
        self.cells.iter().all(Cell::is_plain_space)
    }
}

/// A selection in viewport coordinates `(row, column)`. The anchor is where
/// the drag started; the head follows the pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Selection {
    anchor: (usize, usize),
    head: (usize, usize),
}

/// The main screen, put aside while the alternate screen is active.
#[derive(Debug, Clone)]
struct SavedScreen {
    screen: Vec<Line>,
    cursor_row: usize,
    cursor_col: usize,
    pending_wrap: bool,
    attr: Attr,
    display_offset: usize,
    /// Whether leaving restores the cursor and attributes (DECSC/DECRC, mode
    /// 1049) or only the screen (modes 47 and 1047).
    restore_cursor: bool,
}

#[derive(Debug, Clone)]
pub struct Grid {
    cols: usize,
    rows: usize,
    screen: Vec<Line>,
    scrollback: VecDeque<Line>,
    cursor_row: usize,
    cursor_col: usize,
    /// The cursor sits past the last column; the next printed character
    /// wraps to the next line first.
    pending_wrap: bool,
    attr: Attr,
    cursor_visible: bool,
    /// Lines scrolled back from the bottom; 0 shows the live screen.
    display_offset: usize,
    selection: Option<Selection>,
    /// The main screen while the alternate screen is active. The alternate
    /// screen has no history: its scrolling is lost, not kept.
    alternate: Option<SavedScreen>,
}

impl Grid {
    /// A blank grid of `cols` by `rows` cells (each at least 1).
    pub fn new(cols: usize, rows: usize) -> Self {
        let cols = cols.max(1);
        let rows = rows.max(1);
        Self {
            cols,
            rows,
            screen: (0..rows).map(|_| Line::blank(cols)).collect(),
            scrollback: VecDeque::new(),
            cursor_row: 0,
            cursor_col: 0,
            pending_wrap: false,
            attr: Attr::default(),
            cursor_visible: true,
            display_offset: 0,
            selection: None,
            alternate: None,
        }
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    /// The cursor as `(row, column)` on the live screen.
    pub fn cursor(&self) -> (usize, usize) {
        (self.cursor_row, self.cursor_col)
    }

    pub fn cursor_visible(&self) -> bool {
        self.cursor_visible
    }

    pub fn set_cursor_visible(&mut self, visible: bool) {
        self.cursor_visible = visible;
    }

    pub fn attr(&self) -> Attr {
        self.attr
    }

    pub fn set_attr(&mut self, attr: Attr) {
        self.attr = attr;
    }

    /// Lines in the history; the tests check the cap and the reflow.
    #[cfg(test)]
    pub fn scrollback_len(&self) -> usize {
        self.scrollback.len()
    }

    /// Lines scrolled back from the bottom; 0 when showing the live screen.
    pub fn display_offset(&self) -> usize {
        self.display_offset
    }

    /// Scroll the view: a positive `lines` looks further back into history,
    /// a negative one moves towards the live screen. Clamped to the history.
    pub fn scroll_view(&mut self, lines: isize) {
        self.clear_selection();
        let offset = (self.display_offset as isize).saturating_add(lines);
        self.display_offset = offset.clamp(0, self.history_len() as isize) as usize;
    }

    /// Whether the alternate screen is showing (DECSET 47, 1047 or 1049).
    #[cfg(test)]
    pub fn in_alternate_screen(&self) -> bool {
        self.alternate.is_some()
    }

    /// Switches to a blank alternate screen, putting the main screen aside.
    /// Nothing written there reaches the history. With `save_cursor` (mode
    /// 1049) the cursor and attributes are restored on leaving. Entering
    /// while already on the alternate screen does nothing.
    pub fn enter_alternate_screen(&mut self, save_cursor: bool) {
        if self.alternate.is_some() {
            return;
        }
        self.clear_selection();
        let blank = (0..self.rows).map(|_| Line::blank(self.cols)).collect();
        let screen = std::mem::replace(&mut self.screen, blank);
        self.alternate = Some(SavedScreen {
            screen,
            cursor_row: self.cursor_row,
            cursor_col: self.cursor_col,
            pending_wrap: self.pending_wrap,
            attr: self.attr,
            display_offset: self.display_offset,
            restore_cursor: save_cursor,
        });
        self.display_offset = 0;
    }

    /// Returns to the main screen, restoring what [`Self::enter_alternate_screen`]
    /// put aside. Does nothing when the main screen is showing.
    pub fn leave_alternate_screen(&mut self) {
        let Some(saved) = self.alternate.take() else {
            return;
        };
        self.clear_selection();
        self.screen = saved.screen;
        if saved.restore_cursor {
            self.cursor_row = saved.cursor_row;
            self.cursor_col = saved.cursor_col;
            self.pending_wrap = saved.pending_wrap;
            self.attr = saved.attr;
        }
        self.display_offset = saved.display_offset.min(self.scrollback.len());
    }

    pub fn scroll_to_bottom(&mut self) {
        if self.display_offset != 0 {
            self.clear_selection();
        }
        self.display_offset = 0;
    }

    /// The cell at a viewport position; `row` and `col` must be in range.
    pub fn viewport_cell(&self, row: usize, col: usize) -> Cell {
        self.line_at(row).cells[col]
    }

    /// The viewport as text: rows end in newlines unless soft-wrapped, and
    /// trailing spaces are dropped.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for row in 0..self.rows {
            let line = self.line_at(row);
            let mut text = String::new();
            for cell in &line.cells {
                cell.push_text(&mut text);
            }
            out.push_str(text.trim_end());
            if row + 1 < self.rows && !line.wrapped {
                out.push('\n');
            }
        }
        out
    }

    /// Writes `ch` at the cursor with the current attributes and advances.
    /// A wide character takes two columns; a zero-width character attaches
    /// to the cell before the cursor instead of taking one of its own.
    pub fn put(&mut self, ch: char) {
        let width = match UnicodeWidthChar::width(ch) {
            // Control characters have no width and are not drawn.
            None => return,
            Some(0) => {
                self.attach_mark(ch);
                return;
            }
            Some(2) if self.cols >= 2 => 2,
            Some(_) => 1,
        };
        if self.pending_wrap {
            self.wrap();
        }
        if width == 2 && self.cursor_col + 1 == self.cols {
            // No room for the second column: leave this one blank and
            // start the wide character on the next line.
            let (row, col) = (self.cursor_row, self.cursor_col);
            self.screen[row].cells[col] = self.erased();
            self.wrap();
        }
        let (row, col) = (self.cursor_row, self.cursor_col);
        self.break_wide_pair(row, col);
        if width == 2 {
            self.break_wide_pair(row, col + 1);
            let cells = &mut self.screen[row].cells;
            cells[col] = Cell {
                ch,
                attr: self.attr,
                width: Width::Wide,
                marks: NO_MARKS,
            };
            cells[col + 1] = Cell {
                ch: ' ',
                attr: self.attr,
                width: Width::Spacer,
                marks: NO_MARKS,
            };
        } else {
            self.screen[row].cells[col] = Cell {
                ch,
                attr: self.attr,
                width: Width::Single,
                marks: NO_MARKS,
            };
        }
        let last = col + width - 1;
        if last + 1 < self.cols {
            self.cursor_col = last + 1;
        } else {
            self.cursor_col = last;
            self.pending_wrap = true;
        }
    }

    /// Moves to column 0 of the next row, marking the row as soft-wrapped.
    fn wrap(&mut self) {
        self.pending_wrap = false;
        self.screen[self.cursor_row].wrapped = true;
        self.cursor_col = 0;
        self.line_feed();
    }

    /// Attaches a zero-width character to the cell before the cursor (the
    /// first half of a wide character if that is what precedes it). With no
    /// cell before the cursor, the mark is dropped.
    fn attach_mark(&mut self, mark: char) {
        let col = if self.pending_wrap {
            self.cols - 1
        } else if self.cursor_col > 0 {
            self.cursor_col - 1
        } else {
            return;
        };
        let cells = &mut self.screen[self.cursor_row].cells;
        let col = if cells[col].width == Width::Spacer && col > 0 {
            col - 1
        } else {
            col
        };
        if let Some(slot) = cells[col].marks.iter_mut().find(|slot| **slot == '\0') {
            *slot = mark;
        }
    }

    /// Before column `col` is overwritten, undoes the half of a wide
    /// character that it belongs to: the other half becomes a blank.
    fn break_wide_pair(&mut self, row: usize, col: usize) {
        let cells = &mut self.screen[row].cells;
        if cells[col].width == Width::Spacer && col > 0 {
            cells[col - 1] = Cell::single(' ', cells[col - 1].attr);
        }
        if cells[col].width == Width::Wide && col + 1 < cells.len() {
            cells[col + 1] = Cell::single(' ', cells[col + 1].attr);
        }
    }

    /// Blanks any wide character on the row whose other half was erased,
    /// shifted out or overwritten by a range operation.
    fn repair_row(&mut self, row: usize) {
        let cells = &mut self.screen[row].cells;
        for col in 0..cells.len() {
            let orphan = match cells[col].width {
                Width::Wide => cells
                    .get(col + 1)
                    .is_none_or(|next| next.width != Width::Spacer),
                Width::Spacer => col == 0 || cells[col - 1].width != Width::Wide,
                Width::Single => false,
            };
            if orphan {
                cells[col] = Cell::single(' ', cells[col].attr);
            }
        }
    }

    pub fn carriage_return(&mut self) {
        self.cursor_col = 0;
        self.pending_wrap = false;
    }

    /// Moves down one row, scrolling the screen at the bottom.
    pub fn line_feed(&mut self) {
        self.pending_wrap = false;
        if self.cursor_row + 1 < self.rows {
            self.cursor_row += 1;
        } else {
            self.scroll_up(1);
        }
    }

    pub fn backspace(&mut self) {
        self.cursor_col = self.cursor_col.saturating_sub(1);
        self.pending_wrap = false;
    }

    /// Advances to the next tab stop (every eight columns).
    pub fn tab(&mut self) {
        let next = (self.cursor_col / 8 + 1) * 8;
        self.cursor_col = next.min(self.cols - 1);
        self.pending_wrap = false;
    }

    /// Moves the cursor to a row and column, clamped to the screen.
    pub fn move_cursor(&mut self, row: usize, col: usize) {
        self.cursor_row = row.min(self.rows - 1);
        self.cursor_col = col.min(self.cols - 1);
        self.pending_wrap = false;
    }

    /// Moves the cursor by a signed offset, clamped to the screen.
    pub fn move_cursor_by(&mut self, rows: isize, cols: isize) {
        let row = (self.cursor_row as isize + rows).clamp(0, self.rows as isize - 1);
        let col = (self.cursor_col as isize + cols).clamp(0, self.cols as isize - 1);
        self.move_cursor(row as usize, col as usize);
    }

    /// Erases part of the cursor's line: 0 from the cursor to the end, 1 from
    /// the start to the cursor, 2 the whole line.
    pub fn erase_line(&mut self, mode: u16) {
        let erased = self.erased();
        let cols = self.cols;
        let (start, end) = match mode {
            0 => (self.cursor_col, cols),
            1 => (0, self.cursor_col + 1),
            _ => (0, cols),
        };
        for cell in &mut self.screen[self.cursor_row].cells[start..end] {
            *cell = erased;
        }
        self.repair_row(self.cursor_row);
    }

    /// Erases part of the screen: 0 from the cursor to the end, 1 from the
    /// start to the cursor, 2 the whole screen, 3 the scrollback too.
    pub fn erase_display(&mut self, mode: u16) {
        match mode {
            0 => {
                self.erase_line(0);
                for row in self.cursor_row + 1..self.rows {
                    self.blank_row(row);
                }
            }
            1 => {
                for row in 0..self.cursor_row {
                    self.blank_row(row);
                }
                self.erase_line(1);
            }
            2 => {
                for row in 0..self.rows {
                    self.blank_row(row);
                }
            }
            _ => {
                for row in 0..self.rows {
                    self.blank_row(row);
                }
                self.scrollback.clear();
                self.display_offset = 0;
                self.clear_selection();
            }
        }
    }

    /// Inserts `count` blank cells at the cursor, pushing the rest of the
    /// line right; cells pushed past the edge are lost.
    pub fn insert_chars(&mut self, count: usize) {
        let erased = self.erased();
        let col = self.cursor_col;
        let count = count.min(self.cols - col);
        let cells = &mut self.screen[self.cursor_row].cells;
        for _ in 0..count {
            cells.pop();
            cells.insert(col, erased);
        }
        self.repair_row(self.cursor_row);
    }

    /// Deletes `count` cells at the cursor, pulling the rest of the line
    /// left; blanks come in at the right edge.
    pub fn delete_chars(&mut self, count: usize) {
        let erased = self.erased();
        let col = self.cursor_col;
        let count = count.min(self.cols - col);
        let cells = &mut self.screen[self.cursor_row].cells;
        for _ in 0..count {
            cells.remove(col);
            cells.push(erased);
        }
        self.repair_row(self.cursor_row);
    }

    /// Blanks `count` cells from the cursor without moving the rest.
    pub fn erase_chars(&mut self, count: usize) {
        let erased = self.erased();
        let start = self.cursor_col;
        let end = (start + count).min(self.cols);
        for cell in &mut self.screen[self.cursor_row].cells[start..end] {
            *cell = erased;
        }
        self.repair_row(self.cursor_row);
    }

    /// Inserts `count` blank lines at the cursor row; lines pushed past the
    /// bottom of the screen are lost.
    pub fn insert_lines(&mut self, count: usize) {
        let count = count.min(self.rows - self.cursor_row);
        for _ in 0..count {
            self.screen.pop();
            self.screen.insert(self.cursor_row, Line::blank(self.cols));
        }
        self.cursor_col = 0;
        self.pending_wrap = false;
    }

    /// Deletes `count` lines at the cursor row; blank lines come in at the
    /// bottom.
    pub fn delete_lines(&mut self, count: usize) {
        let count = count.min(self.rows - self.cursor_row);
        for _ in 0..count {
            self.screen.remove(self.cursor_row);
            self.screen.push(Line::blank(self.cols));
        }
        self.cursor_col = 0;
        self.pending_wrap = false;
    }

    /// Starts a selection at a viewport position.
    pub fn begin_selection(&mut self, row: usize, col: usize) {
        let row = row.min(self.rows - 1);
        let col = col.min(self.cols - 1);
        self.selection = Some(Selection {
            anchor: (row, col),
            head: (row, col),
        });
    }

    /// Moves the end of the selection to a viewport position.
    pub fn extend_selection(&mut self, row: usize, col: usize) {
        let row = row.min(self.rows - 1);
        let col = col.min(self.cols - 1);
        if let Some(selection) = &mut self.selection {
            selection.head = (row, col);
        }
    }

    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    /// Whether a non-empty selection exists.
    pub fn has_selection(&self) -> bool {
        self.selection_bounds().is_some()
    }

    /// Whether a viewport cell lies inside the selection.
    pub fn is_selected(&self, row: usize, col: usize) -> bool {
        let Some(((r0, c0), (r1, c1))) = self.selection_bounds() else {
            return false;
        };
        let at = (row, col);
        at >= (r0, c0) && at <= (r1, c1)
    }

    /// The selected text, or `None` when nothing is selected. Soft-wrapped
    /// rows are joined without a newline; trailing spaces are dropped.
    pub fn selected_text(&self) -> Option<String> {
        let ((r0, c0), (r1, c1)) = self.selection_bounds()?;
        let mut out = String::new();
        for row in r0..=r1 {
            let line = self.line_at(row);
            let mut start = if row == r0 { c0 } else { 0 };
            let mut end = if row == r1 { c1 + 1 } else { self.cols };
            // A wide character is selected whole or not at all.
            if start > 0 && line.cells[start].width == Width::Spacer {
                start -= 1;
            }
            if end < self.cols && line.cells[end - 1].width == Width::Wide {
                end += 1;
            }
            let mut piece = String::new();
            for cell in &line.cells[start..end] {
                cell.push_text(&mut piece);
            }
            let kept = piece.trim_end().len();
            piece.truncate(kept);
            out.push_str(&piece);
            if row != r1 && !line.wrapped {
                out.push('\n');
            }
        }
        (!out.is_empty()).then_some(out)
    }

    /// Changes the size. Soft-wrapped lines are re-wrapped to the new width,
    /// the cursor follows its character, and the history is kept (up to
    /// [`SCROLLBACK_LINES`]). A height change moves rows between the screen
    /// and the history so the cursor stays visible. The selection is cleared.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        let cols = cols.max(1);
        let rows = rows.max(1);
        if cols == self.cols && rows == self.rows {
            return;
        }
        self.clear_selection();
        let Some(mut saved) = self.alternate.take() else {
            self.reflow(cols, rows);
            return;
        };
        // The main screen is reflowed as if it were showing, so that leaving
        // the alternate screen finds it at the new size. The alternate screen
        // starts again blank; a full-screen program redraws it after a resize.
        let (alt_row, alt_col) = (self.cursor_row, self.cursor_col);
        std::mem::swap(&mut self.screen, &mut saved.screen);
        self.cursor_row = saved.cursor_row;
        self.cursor_col = saved.cursor_col;
        self.reflow(cols, rows);
        saved.screen = std::mem::replace(
            &mut self.screen,
            (0..rows).map(|_| Line::blank(cols)).collect(),
        );
        saved.cursor_row = self.cursor_row;
        saved.cursor_col = self.cursor_col;
        saved.pending_wrap = self.pending_wrap;
        saved.display_offset = self.display_offset;
        self.cursor_row = alt_row.min(rows - 1);
        self.cursor_col = alt_col.min(cols - 1);
        self.pending_wrap = false;
        self.alternate = Some(saved);
    }

    /// Re-wraps the history and the screen to `cols` by `rows` (both at least
    /// one). The cursor follows its character.
    fn reflow(&mut self, cols: usize, rows: usize) {
        self.pending_wrap = false;
        self.display_offset = 0;

        let cursor_index = self.scrollback.len() + self.cursor_row;
        let all: Vec<Line> = self
            .scrollback
            .drain(..)
            .chain(self.screen.drain(..))
            .collect();

        // Re-wrap each logical line (a run of soft-wrapped rows) at `cols`.
        let mut out: Vec<Line> = Vec::with_capacity(all.len());
        let mut cursor_at = (0usize, 0usize);
        let mut start = 0;
        while start < all.len() {
            let mut end = start;
            while end + 1 < all.len() && all[end].wrapped {
                end += 1;
            }
            let mut cells: Vec<Cell> = Vec::new();
            let mut cursor_offset = None;
            for (position, line) in all[start..=end].iter().enumerate() {
                if start + position == cursor_index {
                    cursor_offset = Some(cells.len() + self.cursor_col);
                }
                cells.extend_from_slice(&line.cells);
            }
            // Trailing blanks are not content, unless the cursor sits on them.
            let keep = cursor_offset.map_or(0, |offset| offset + 1);
            while cells.len() > keep && cells.last().is_some_and(Cell::is_plain_space) {
                cells.pop();
            }
            let packed = pack_rows(&cells, cols, cursor_offset);
            if let Some((row, col)) = packed.cursor {
                cursor_at = (out.len() + row, col);
            }
            let count = packed.rows.len();
            for (chunk, row) in packed.rows.into_iter().enumerate() {
                out.push(Line {
                    cells: row,
                    wrapped: chunk + 1 < count,
                });
            }
            start = end + 1;
        }

        // Blank rows below the cursor are not kept; they would only push the
        // cursor up into the history.
        let (cursor_index, cursor_col) = (cursor_at.0, cursor_at.1);
        while out.len() > cursor_index + 1
            && out
                .last()
                .is_some_and(|line| !line.wrapped && line.is_blank())
        {
            out.pop();
        }

        let screen_start = out.len().saturating_sub(rows).min(cursor_index);
        let mut scrollback: VecDeque<Line> = out.drain(..screen_start).collect();
        let mut screen: Vec<Line> = out.into_iter().take(rows).collect();
        while screen.len() < rows {
            screen.push(Line::blank(cols));
        }
        while scrollback.len() > SCROLLBACK_LINES {
            scrollback.pop_front();
        }

        self.cols = cols;
        self.rows = rows;
        self.screen = screen;
        self.scrollback = scrollback;
        self.cursor_row = cursor_index - screen_start;
        self.cursor_col = cursor_col.min(cols - 1);
    }

    fn erased(&self) -> Cell {
        Cell::single(
            ' ',
            Attr {
                bg: self.attr.bg,
                ..Attr::default()
            },
        )
    }

    fn blank_row(&mut self, row: usize) {
        self.screen[row] = Line::blank(self.cols);
    }

    /// Pushes the top screen row into the history and adds a blank row at
    /// the bottom.
    fn scroll_up(&mut self, count: usize) {
        self.clear_selection();
        for _ in 0..count {
            let top = self.screen.remove(0);
            if self.alternate.is_none() {
                self.scrollback.push_back(top);
                if self.scrollback.len() > SCROLLBACK_LINES {
                    self.scrollback.pop_front();
                }
            }
            self.screen.push(Line::blank(self.cols));
            if self.display_offset > 0 {
                // Keep the scrolled-back view on the same history lines.
                self.display_offset = (self.display_offset + 1).min(self.scrollback.len());
            }
        }
    }

    /// The history lines that can be viewed: none on the alternate screen.
    fn history_len(&self) -> usize {
        if self.alternate.is_some() {
            0
        } else {
            self.scrollback.len()
        }
    }

    fn line_at(&self, row: usize) -> &Line {
        let history = self.history_len();
        let top = history - self.display_offset;
        let index = top + row;
        if index < history {
            &self.scrollback[index]
        } else {
            &self.screen[index - history]
        }
    }

    /// Selection bounds in reading order, or `None` for an empty selection.
    fn selection_bounds(&self) -> Option<((usize, usize), (usize, usize))> {
        let selection = self.selection?;
        if selection.anchor == selection.head {
            return None;
        }
        Some(if selection.anchor <= selection.head {
            (selection.anchor, selection.head)
        } else {
            (selection.head, selection.anchor)
        })
    }
}

/// The rows one logical line occupies at a new width.
struct Packed {
    /// Each row is exactly `cols` cells, blank-padded.
    rows: Vec<Vec<Cell>>,
    /// The cursor's row index within `rows` and its column.
    cursor: Option<(usize, usize)>,
}

/// Packs a logical line's cells into rows of `cols` columns. A wide character
/// is never split across two rows; a row that has no room for one ends early.
/// `cursor` is an index into `cells`. Below two columns a wide character
/// cannot fit at all, so it is narrowed and its spacer dropped.
fn pack_rows(cells: &[Cell], cols: usize, cursor: Option<usize>) -> Packed {
    let mut rows: Vec<Vec<Cell>> = Vec::new();
    let mut row: Vec<Cell> = Vec::with_capacity(cols);
    let mut found = None;
    for (index, cell) in cells.iter().enumerate() {
        let mut cell = *cell;
        if cell.width == Width::Spacer && cols < 2 {
            if cursor == Some(index) {
                found = Some((rows.len(), row.len()));
            }
            continue;
        }
        if cell.width == Width::Wide && cols < 2 {
            cell.width = Width::Single;
        }
        let need = if cell.width == Width::Wide { 2 } else { 1 };
        if row.len() + need > cols {
            rows.push(pad_row(std::mem::take(&mut row), cols));
            row = Vec::with_capacity(cols);
        }
        if cursor == Some(index) {
            found = Some((rows.len(), row.len()));
        }
        row.push(cell);
    }
    rows.push(pad_row(row, cols));
    Packed {
        rows,
        cursor: found,
    }
}

fn pad_row(mut row: Vec<Cell>, cols: usize) -> Vec<Cell> {
    row.resize(cols, Cell::BLANK);
    row
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put_str(grid: &mut Grid, text: &str) {
        for ch in text.chars() {
            match ch {
                '\n' => {
                    grid.carriage_return();
                    grid.line_feed();
                }
                _ => grid.put(ch),
            }
        }
    }

    #[test]
    fn printing_advances_the_cursor_and_wraps_at_the_edge() {
        let mut grid = Grid::new(4, 3);
        put_str(&mut grid, "abcd");
        // The cursor stays on the last column until the next character.
        assert_eq!(grid.cursor(), (0, 3));
        assert_eq!(grid.text(), "abcd\n\n");
        grid.put('e');
        assert_eq!(grid.cursor(), (1, 1));
        // Row 0 is soft-wrapped into row 1, so no newline between them.
        assert_eq!(grid.text(), "abcde\n");
    }

    #[test]
    fn line_feed_at_the_bottom_scrolls_into_history() {
        let mut grid = Grid::new(5, 2);
        put_str(&mut grid, "one\ntwo\nthree");
        assert_eq!(grid.text(), "two\nthree");
        assert_eq!(grid.scrollback_len(), 1);
        grid.scroll_view(1);
        assert_eq!(grid.text(), "one\ntwo");
        grid.scroll_to_bottom();
        assert_eq!(grid.text(), "two\nthree");
    }

    #[test]
    fn scrollback_is_capped_at_two_thousand_lines() {
        let mut grid = Grid::new(10, 2);
        for index in 0..SCROLLBACK_LINES + 500 {
            put_str(&mut grid, &format!("{index}\n"));
        }
        assert_eq!(grid.scrollback_len(), SCROLLBACK_LINES);
        // The view can reach the oldest kept line and no further.
        grid.scroll_view(isize::MAX);
        assert_eq!(grid.display_offset(), SCROLLBACK_LINES);
    }

    #[test]
    fn carriage_return_backspace_and_tab_move_the_cursor() {
        let mut grid = Grid::new(20, 2);
        put_str(&mut grid, "abc");
        grid.backspace();
        grid.put('X');
        assert_eq!(grid.text().trim_end(), "abX");
        grid.carriage_return();
        assert_eq!(grid.cursor(), (0, 0));
        grid.tab();
        assert_eq!(grid.cursor(), (0, 8));
        grid.move_cursor_by(5, -100);
        assert_eq!(grid.cursor(), (1, 0));
    }

    #[test]
    fn erase_line_and_display_blank_the_right_cells() {
        let mut grid = Grid::new(6, 3);
        put_str(&mut grid, "abcdef\nghijkl\nmnopqr");
        grid.move_cursor(1, 2);
        grid.erase_line(0);
        assert_eq!(grid.text(), "abcdef\ngh\nmnopqr");
        grid.erase_display(0);
        assert_eq!(grid.text(), "abcdef\ngh\n");
        grid.erase_display(2);
        assert_eq!(grid.text(), "\n\n");
    }

    #[test]
    fn insert_and_delete_chars_shift_the_line() {
        let mut grid = Grid::new(6, 1);
        put_str(&mut grid, "abcdef");
        grid.move_cursor(0, 1);
        grid.delete_chars(2);
        assert_eq!(grid.text(), "adef");
        grid.insert_chars(1);
        assert_eq!(grid.text(), "a def");
        grid.erase_chars(2);
        assert_eq!(grid.text(), "a  ef");
    }

    #[test]
    fn resize_changes_the_columns_and_rows() {
        let mut grid = Grid::new(10, 3);
        put_str(&mut grid, "hello");
        grid.resize(20, 5);
        assert_eq!(grid.cols(), 20);
        assert_eq!(grid.rows(), 5);
        assert_eq!(grid.text(), "hello\n\n\n\n");
        grid.resize(4, 2);
        assert_eq!((grid.cols(), grid.rows()), (4, 2));
        // The cursor stays on screen.
        let (row, col) = grid.cursor();
        assert!(row < 2 && col < 4);
    }

    #[test]
    fn narrowing_rewraps_a_long_line_and_widening_joins_it_again() {
        let mut grid = Grid::new(10, 3);
        put_str(&mut grid, "abcdefghij");
        assert_eq!(grid.cursor(), (0, 9));
        grid.resize(4, 3);
        // All three rows are soft-wrapped, so the text has no newlines.
        assert_eq!(grid.text(), "abcdefghij");
        // The cursor stays on the last character written ('j').
        assert_eq!(grid.cursor(), (2, 1));
        grid.resize(10, 3);
        assert_eq!(grid.text(), "abcdefghij\n\n");
        assert_eq!(grid.cursor(), (0, 9));
    }

    #[test]
    fn reflow_keeps_the_history_and_the_cursor_row_visible() {
        let mut grid = Grid::new(8, 2);
        put_str(&mut grid, "first\nsecond\nthird");
        assert_eq!(grid.scrollback_len(), 1);
        grid.resize(8, 1);
        assert_eq!(grid.rows(), 1);
        assert_eq!(grid.text(), "third");
        assert_eq!(grid.scrollback_len(), 2);
    }

    #[test]
    fn selection_copies_across_rows_and_joins_soft_wraps() {
        let mut grid = Grid::new(4, 3);
        put_str(&mut grid, "abcdefg\nxy");
        assert_eq!(grid.text(), "abcdefg\nxy");
        grid.begin_selection(0, 2);
        grid.extend_selection(1, 1);
        // Row 0 wraps into row 1, so no newline between them.
        assert_eq!(grid.selected_text().as_deref(), Some("cdef"));
        assert!(grid.is_selected(0, 3));
        assert!(!grid.is_selected(2, 0));
        grid.begin_selection(1, 0);
        grid.extend_selection(2, 1);
        assert_eq!(grid.selected_text().as_deref(), Some("efg\nxy"));
        grid.clear_selection();
        assert_eq!(grid.selected_text(), None);
    }

    #[test]
    fn a_wide_character_takes_two_columns() {
        let mut grid = Grid::new(6, 1);
        put_str(&mut grid, "中a");
        assert_eq!(grid.viewport_cell(0, 0).ch, '中');
        assert_eq!(grid.viewport_cell(0, 0).width, Width::Wide);
        assert_eq!(grid.viewport_cell(0, 1).width, Width::Spacer);
        assert_eq!(grid.viewport_cell(0, 2).ch, 'a');
        assert_eq!(grid.cursor(), (0, 3));
        // The spacer contributes no text of its own.
        assert_eq!(grid.text(), "中a");
    }

    #[test]
    fn emoji_are_wide_and_a_variation_selector_attaches_to_them() {
        let mut grid = Grid::new(6, 1);
        grid.put('😀');
        grid.put('\u{fe0f}');
        assert_eq!(grid.viewport_cell(0, 0).width, Width::Wide);
        assert_eq!(grid.viewport_cell(0, 0).marks[0], '\u{fe0f}');
        assert_eq!(grid.cursor(), (0, 2));
        assert_eq!(grid.text(), "😀\u{fe0f}");
    }

    #[test]
    fn combining_marks_attach_to_the_previous_cell() {
        let mut grid = Grid::new(4, 1);
        put_str(&mut grid, "e\u{301}x");
        // The mark takes no column of its own.
        assert_eq!(grid.cursor(), (0, 2));
        assert_eq!(grid.viewport_cell(0, 0).ch, 'e');
        assert_eq!(grid.viewport_cell(0, 0).marks[0], '\u{301}');
        assert_eq!(grid.text(), "e\u{301}x");
    }

    #[test]
    fn a_mark_after_a_full_row_attaches_to_the_last_cell_and_extra_marks_are_dropped() {
        let mut grid = Grid::new(2, 1);
        put_str(&mut grid, "ab");
        grid.put('\u{301}');
        assert_eq!(grid.viewport_cell(0, 1).marks[0], '\u{301}');
        // A third mark would exceed MARKS_PER_CELL and is dropped.
        grid.put('\u{302}');
        grid.put('\u{303}');
        assert_eq!(grid.viewport_cell(0, 1).marks, ['\u{301}', '\u{302}']);
        assert_eq!(grid.cursor(), (0, 1));
    }

    #[test]
    fn a_mark_after_a_wide_character_attaches_to_its_first_half() {
        let mut grid = Grid::new(4, 1);
        put_str(&mut grid, "中");
        grid.put('\u{301}');
        assert_eq!(grid.viewport_cell(0, 0).marks[0], '\u{301}');
        assert_eq!(grid.viewport_cell(0, 1).marks, ['\0'; MARKS_PER_CELL]);
    }

    #[test]
    fn a_wide_character_that_does_not_fit_starts_the_next_row() {
        let mut grid = Grid::new(3, 2);
        put_str(&mut grid, "ab中");
        // The last column is left blank and the character starts below.
        assert_eq!(grid.viewport_cell(0, 2).ch, ' ');
        assert_eq!(grid.viewport_cell(1, 0).ch, '中');
        assert_eq!(grid.cursor(), (1, 2));
        assert_eq!(grid.text(), "ab中");
    }

    #[test]
    fn overwriting_half_of_a_wide_character_blanks_the_other_half() {
        let mut grid = Grid::new(4, 1);
        put_str(&mut grid, "中中");
        grid.move_cursor(0, 1);
        grid.put('x');
        assert_eq!(grid.viewport_cell(0, 0).width, Width::Single);
        assert_eq!(grid.viewport_cell(0, 0).ch, ' ');
        assert_eq!(grid.viewport_cell(0, 1).ch, 'x');
        assert_eq!(grid.viewport_cell(0, 2).width, Width::Wide);
        assert_eq!(grid.text(), " x中");
    }

    #[test]
    fn erasing_one_column_of_a_wide_character_blanks_the_rest_of_it() {
        let mut grid = Grid::new(6, 1);
        put_str(&mut grid, "a中b");
        grid.move_cursor(0, 2);
        grid.erase_chars(1);
        assert_eq!(grid.viewport_cell(0, 1).width, Width::Single);
        assert_eq!(grid.viewport_cell(0, 1).ch, ' ');
        assert_eq!(grid.text(), "a  b");
    }

    #[test]
    fn deleting_the_first_half_of_a_wide_character_blanks_its_spacer() {
        let mut grid = Grid::new(4, 1);
        put_str(&mut grid, "中b");
        grid.move_cursor(0, 0);
        grid.delete_chars(1);
        assert_eq!(grid.viewport_cell(0, 0).width, Width::Single);
        assert_eq!(grid.text(), " b");
    }

    #[test]
    fn selecting_part_of_a_wide_character_selects_all_of_it() {
        let mut grid = Grid::new(6, 1);
        put_str(&mut grid, "a中b");
        grid.begin_selection(0, 2);
        grid.extend_selection(0, 3);
        assert_eq!(grid.selected_text().as_deref(), Some("中b"));
        grid.begin_selection(0, 0);
        grid.extend_selection(0, 1);
        assert_eq!(grid.selected_text().as_deref(), Some("a中"));
    }

    #[test]
    fn narrowing_keeps_wide_characters_whole() {
        let mut grid = Grid::new(6, 2);
        put_str(&mut grid, "a中中");
        assert_eq!(grid.cursor(), (0, 5));
        grid.resize(4, 2);
        // "a中" fills the first row (the next character needs two columns
        // and only one is left); the second character starts the next row.
        assert_eq!(grid.viewport_cell(0, 1).width, Width::Wide);
        assert_eq!(grid.viewport_cell(0, 3).ch, ' ');
        assert_eq!(grid.viewport_cell(1, 0).ch, '中');
        assert_eq!(grid.viewport_cell(1, 1).width, Width::Spacer);
        assert_eq!(grid.cursor(), (1, 2));
        assert_eq!(grid.text(), "a中中");
    }

    #[test]
    fn a_single_click_selects_nothing() {
        let mut grid = Grid::new(4, 1);
        put_str(&mut grid, "abcd");
        grid.begin_selection(0, 1);
        assert!(!grid.has_selection());
        assert_eq!(grid.selected_text(), None);
    }
}
