use crate::vt_parser::VtHandler;
use slopos_kit::clipboard::Clipboard;
use slopos_kit::event::{KeyCode, MouseButton};
use slopos_kit::theme::ThemeContext;
use slopos_kit::Color;
use slopos_kit::{
    AccessibilityNode, AccessibilityRole, Event, EventResult, LayoutConstraint, MonospaceCell,
    MonospaceView, Rect, Size, Widget, WidgetState,
};
use std::any::Any;
use std::sync::{Arc, Mutex};

/// Events delivered by the background PTY reader to the SDK event-loop thread.
///
/// The reader never mutates terminal state directly: it queues one of these
/// events and then wakes the application event loop. That keeps parser/widget
/// state single-threaded while still allowing output and PTY lifecycle changes
/// to redraw an otherwise idle client.
#[derive(Debug, PartialEq, Eq)]
pub enum PtyEvent {
    Output(Vec<u8>),
    Exited,
    Error(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridPoint {
    pub col: usize,
    pub row: usize,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct Cell {
    pub c: char,
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Cell {
            c: ' ',
            fg: Color::WHITE,
            bg: Color::BLACK,
            bold: false,
            italic: false,
            underline: false,
        }
    }
}

pub struct Terminal {
    state: WidgetState,
    pub cols: usize,
    pub rows: usize,
    pub grid: Vec<Cell>,
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub scrollback: Vec<Vec<Cell>>,
    pub max_scrollback: usize,
    pub current_fg: Color,
    pub current_bg: Color,
    pub current_bold: bool,
    pub current_italic: bool,
    pub current_underline: bool,
    parser: vte::Parser,
    pub pty: Option<crate::pty::Pty>,
    pub rx: Option<Arc<Mutex<std::sync::mpsc::Receiver<PtyEvent>>>>,
    pub pty_exited: bool,
    pub pty_error: Option<String>,
    display: MonospaceView,
    pub scroll_offset: usize,
    pub selection_start: Option<GridPoint>,
    pub selection_end: Option<GridPoint>,
    selecting: bool,
    pub scroll_top: usize,
    pub scroll_bottom: usize,
    // Alternate screen buffer (CSI ? 1049 h / l)
    pub alt_grid: Option<Vec<Cell>>,
    pub alt_cursor_x: usize,
    pub alt_cursor_y: usize,
    pub using_alt_screen: bool,
    // Mouse reporting (CSI ? 1000 h / l)
    pub mouse_reporting: bool,
    // Window title set via OSC sequences
    pub window_title: Option<String>,
    pub title_changed: bool,
}

impl Terminal {
    pub fn new(cols: usize, rows: usize) -> Self {
        Terminal {
            state: WidgetState::new(),
            cols,
            rows,
            grid: vec![Cell::default(); cols * rows],
            cursor_x: 0,
            cursor_y: 0,
            scrollback: vec![],
            max_scrollback: 1000,
            current_fg: Color::WHITE,
            current_bg: Color::BLACK,
            current_bold: false,
            current_italic: false,
            current_underline: false,
            parser: vte::Parser::new(),
            pty: None,
            rx: None,
            pty_exited: false,
            pty_error: None,
            display: MonospaceView::new(cols, rows),
            scroll_offset: 0,
            selection_start: None,
            selection_end: None,
            selecting: false,
            scroll_top: 0,
            scroll_bottom: rows - 1,
            alt_grid: None,
            alt_cursor_x: 0,
            alt_cursor_y: 0,
            using_alt_screen: false,
            mouse_reporting: false,
            window_title: None,
            title_changed: false,
        }
    }

    /// Activate alternate screen buffer (CSI ? 1049 h)
    pub fn enter_alt_screen(&mut self) {
        if !self.using_alt_screen {
            // Save primary cursor position and grid
            self.alt_cursor_x = self.cursor_x;
            self.alt_cursor_y = self.cursor_y;
            self.alt_grid = Some(self.grid.clone());
            // Fill screen with blank cells
            self.grid.fill(Cell::default());
            self.cursor_x = 0;
            self.cursor_y = 0;
            self.using_alt_screen = true;
        }
    }

    /// Return to primary screen buffer (CSI ? 1049 l)
    pub fn leave_alt_screen(&mut self) {
        if self.using_alt_screen {
            if let Some(saved) = self.alt_grid.take() {
                self.grid = saved;
            }
            self.cursor_x = self.alt_cursor_x;
            self.cursor_y = self.alt_cursor_y;
            self.using_alt_screen = false;
        }
    }

    /// Set the window title from an OSC sequence
    pub fn set_window_title(&mut self, title: String) {
        self.window_title = Some(title);
        self.title_changed = true;
    }

    /// Send a mouse button event to the PTY (X10/X11 basic mouse reporting)
    pub fn send_mouse_event(&mut self, col: usize, row: usize, button: u8, pressed: bool) {
        if !self.mouse_reporting {
            return;
        }
        // CSI M Cb Cx Cy — 1-based, add 32 offset per X10 protocol
        let cb = button + 32 + if !pressed { 3 } else { 0 };
        let cx = (col + 1 + 32).min(255) as u8;
        let cy = (row + 1 + 32).min(255) as u8;
        let seq = [0x1b, b'[', b'M', cb, cx, cy];
        if let Some(ref mut pty) = self.pty {
            let _ = pty.write(&seq);
        }
    }

    pub fn resize_term(&mut self, cols: usize, rows: usize) {
        let mut new_grid = vec![Cell::default(); cols * rows];
        for r in 0..rows.min(self.rows) {
            for c in 0..cols.min(self.cols) {
                new_grid[r * cols + c] = self.grid[r * self.cols + c].clone();
            }
        }
        self.grid = new_grid;
        self.cols = cols;
        self.rows = rows;
        self.display.resize(cols, rows);
        self.cursor_x = self.cursor_x.min(cols.saturating_sub(1));
        self.cursor_y = self.cursor_y.min(rows.saturating_sub(1));
        self.scroll_offset = self.scroll_offset.min(self.scrollback.len());
        self.clear_selection();
        self.scroll_top = 0;
        self.scroll_bottom = rows - 1;
        if let Some(ref pty) = self.pty {
            let _ = pty.resize(cols as u16, rows as u16);
        }
    }

    pub fn print_char(&mut self, c: char) {
        if self.cursor_x >= self.cols {
            self.cursor_x = 0;
            self.cursor_y += 1;
        }
        if self.cursor_y >= self.rows {
            self.scroll_up();
            self.cursor_y = self.rows - 1;
        }
        let idx = self.cursor_y * self.cols + self.cursor_x;
        if idx < self.grid.len() {
            self.grid[idx] = Cell {
                c,
                fg: self.current_fg,
                bg: self.current_bg,
                bold: self.current_bold,
                italic: self.current_italic,
                underline: self.current_underline,
            };
        }
        self.cursor_x += 1;
    }

    pub fn scroll_up(&mut self) {
        let top = self.scroll_top;
        let bottom = self.scroll_bottom;
        if top == 0 && bottom == self.rows - 1 {
            let first_row = self.grid[0..self.cols].to_vec();
            if self.scrollback.len() >= self.max_scrollback {
                self.scrollback.remove(0);
            }
            self.scrollback.push(first_row);
            self.grid.drain(0..self.cols);
            self.grid.extend(vec![Cell::default(); self.cols]);
        } else if top < bottom && bottom < self.rows {
            for r in top..bottom {
                for c in 0..self.cols {
                    self.grid[r * self.cols + c] = self.grid[(r + 1) * self.cols + c].clone();
                }
            }
            for c in 0..self.cols {
                self.grid[bottom * self.cols + c] = Cell::default();
            }
        }
    }

    pub fn scroll_lines(&mut self, lines: isize) {
        if lines > 0 {
            self.scroll_offset = (self.scroll_offset + lines as usize).min(self.scrollback.len());
        } else if lines < 0 {
            self.scroll_offset = self.scroll_offset.saturating_sub(lines.unsigned_abs());
        }
        self.sync_display();
    }

    pub fn visible_text(&self) -> String {
        self.visible_rows()
            .into_iter()
            .map(Self::row_text)
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn selected_text(&self) -> Option<String> {
        let (start, end) = self.normalized_selection()?;
        let rows = self.visible_rows();
        let mut lines = Vec::new();

        for row in start.row..=end.row {
            let start_col = if row == start.row { start.col } else { 0 };
            let end_col = if row == end.row {
                end.col
            } else {
                self.cols.saturating_sub(1)
            };

            let Some(cells) = rows.get(row) else {
                continue;
            };

            let mut text = String::new();
            for col in start_col..=end_col.min(self.cols.saturating_sub(1)) {
                if let Some(cell) = cells.get(col) {
                    text.push(cell.c);
                }
            }
            while text.ends_with(' ') {
                text.pop();
            }
            lines.push(text);
        }

        Some(lines.join("\n"))
    }

    pub fn select_all_visible(&mut self) {
        self.selection_start = Some(GridPoint { col: 0, row: 0 });
        self.selection_end = Some(GridPoint {
            col: self.cols.saturating_sub(1),
            row: self.rows.saturating_sub(1),
        });
        self.selecting = false;
        self.sync_display();
    }

    pub fn clear_selection(&mut self) {
        self.selection_start = None;
        self.selection_end = None;
        self.selecting = false;
    }

    fn visible_rows(&self) -> Vec<Vec<Cell>> {
        if self.scroll_offset == 0 {
            return self
                .grid
                .chunks(self.cols)
                .map(|row| row.to_vec())
                .collect();
        }

        let history: Vec<Vec<Cell>> = self
            .scrollback
            .iter()
            .cloned()
            .chain(self.grid.chunks(self.cols).map(|row| row.to_vec()))
            .collect();
        let total = history.len();
        let end = total.saturating_sub(self.scroll_offset);
        let start = end.saturating_sub(self.rows);
        let mut rows = history[start..end].to_vec();
        while rows.len() < self.rows {
            rows.insert(0, vec![Cell::default(); self.cols]);
        }
        rows
    }

    fn row_text(row: Vec<Cell>) -> String {
        let mut text: String = row.into_iter().map(|cell| cell.c).collect();
        while text.ends_with(' ') {
            text.pop();
        }
        text
    }

    pub fn write_byte(&mut self, byte: u8) {
        let mut parser = std::mem::replace(&mut self.parser, vte::Parser::new());
        {
            let mut handler = VtHandler { term: self };
            parser.advance(&mut handler, byte);
        }
        self.parser = parser;
    }
}

impl Widget for Terminal {
    fn widget_state(&self) -> &WidgetState {
        &self.state
    }
    fn widget_state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        let size = constraint.clamp(Size::new(constraint.max_width, constraint.max_height));
        self.set_rect(Rect::new(
            self.rect().x,
            self.rect().y,
            size.width,
            size.height,
        ));
        self.display.set_rect(self.rect());
        self.display.layout(constraint);
        size
    }

    fn draw(&self, theme: &ThemeContext) {
        self.display.draw(theme);
    }

    fn handle_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::MouseDown {
                button: MouseButton::Left,
                point,
                ..
            } => {
                if let Some(cell) = self.point_to_cell(*point) {
                    if self.mouse_reporting {
                        self.send_mouse_event(cell.col, cell.row, 0, true);
                        return EventResult::Handled;
                    }
                    self.selection_start = Some(cell);
                    self.selection_end = Some(cell);
                    self.selecting = true;
                    self.sync_display();
                    return EventResult::Handled;
                }
                self.clear_selection();
                self.sync_display();
                EventResult::Ignored
            }
            Event::MouseMove { point, .. } | Event::Drag { point } => {
                if self.selecting {
                    if let Some(cell) = self.point_to_cell(*point) {
                        self.selection_end = Some(cell);
                        self.sync_display();
                    }
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseUp {
                button: MouseButton::Left,
                point,
                ..
            }
            | Event::DragEnd { point } => {
                if self.mouse_reporting {
                    if let Some(cell) = self.point_to_cell(*point) {
                        self.send_mouse_event(cell.col, cell.row, 0, false);
                        return EventResult::Handled;
                    }
                }
                if self.selecting {
                    if let Some(cell) = self.point_to_cell(*point) {
                        self.selection_end = Some(cell);
                    }
                    self.selecting = false;
                    self.sync_display();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::KeyDown { key, modifiers } => {
                if modifiers.meta {
                    match key {
                        KeyCode::C => {
                            let text = self.selected_text().unwrap_or_else(|| self.visible_text());
                            Clipboard::copy(&text);
                            return EventResult::Handled;
                        }
                        KeyCode::V => {
                            let pasted = Clipboard::paste();
                            if !pasted.is_empty() {
                                if let Some(ref mut pty) = self.pty {
                                    let _ = pty.write(pasted.as_bytes());
                                    return EventResult::Handled;
                                }
                            }
                            return EventResult::Ignored;
                        }
                        KeyCode::A => {
                            self.select_all_visible();
                            return EventResult::Handled;
                        }
                        _ => {}
                    }
                }

                if modifiers.shift {
                    match key {
                        KeyCode::PageUp => {
                            self.scroll_lines(self.rows as isize);
                            return EventResult::Handled;
                        }
                        KeyCode::PageDown => {
                            self.scroll_lines(-(self.rows as isize));
                            return EventResult::Handled;
                        }
                        KeyCode::Home => {
                            self.scroll_lines(self.scrollback.len() as isize);
                            return EventResult::Handled;
                        }
                        KeyCode::End => {
                            self.scroll_offset = 0;
                            self.sync_display();
                            return EventResult::Handled;
                        }
                        _ => {}
                    }
                }

                if let Some(ref mut pty) = self.pty {
                    let mut bytes = vec![];
                    if modifiers.control {
                        if let Some(byte) = match key {
                            KeyCode::A => Some(b'\x01'),
                            KeyCode::B => Some(b'\x02'),
                            KeyCode::C => Some(b'\x03'),
                            KeyCode::D => Some(b'\x04'),
                            KeyCode::E => Some(b'\x05'),
                            KeyCode::F => Some(b'\x06'),
                            KeyCode::G => Some(b'\x07'),
                            KeyCode::H => Some(b'\x08'),
                            KeyCode::I => Some(b'\x09'),
                            KeyCode::J => Some(b'\x0a'),
                            KeyCode::K => Some(b'\x0b'),
                            KeyCode::L => Some(b'\x0c'),
                            KeyCode::M => Some(b'\x0d'),
                            KeyCode::N => Some(b'\x0e'),
                            KeyCode::O => Some(b'\x0f'),
                            KeyCode::P => Some(b'\x10'),
                            KeyCode::Q => Some(b'\x11'),
                            KeyCode::R => Some(b'\x12'),
                            KeyCode::S => Some(b'\x13'),
                            KeyCode::T => Some(b'\x14'),
                            KeyCode::U => Some(b'\x15'),
                            KeyCode::V => Some(b'\x16'),
                            KeyCode::W => Some(b'\x17'),
                            KeyCode::X => Some(b'\x18'),
                            KeyCode::Y => Some(b'\x19'),
                            KeyCode::Z => Some(b'\x1a'),
                            _ => None,
                        } {
                            bytes.push(byte);
                        }
                    } else {
                        match key {
                            KeyCode::Backspace => {
                                bytes.push(0x7f);
                            }
                            KeyCode::Enter => {
                                bytes.push(b'\r');
                            }
                            KeyCode::Tab => {
                                bytes.push(b'\t');
                            }
                            KeyCode::Escape => {
                                bytes.push(0x1b);
                            }
                            KeyCode::ArrowUp => {
                                bytes.extend_from_slice(b"\x1b[A");
                            }
                            KeyCode::ArrowDown => {
                                bytes.extend_from_slice(b"\x1b[B");
                            }
                            KeyCode::ArrowRight => {
                                bytes.extend_from_slice(b"\x1b[C");
                            }
                            KeyCode::ArrowLeft => {
                                bytes.extend_from_slice(b"\x1b[D");
                            }
                            KeyCode::Home => {
                                bytes.extend_from_slice(b"\x1b[H");
                            }
                            KeyCode::End => {
                                bytes.extend_from_slice(b"\x1b[F");
                            }
                            KeyCode::Insert => {
                                bytes.extend_from_slice(b"\x1b[2~");
                            }
                            KeyCode::Delete => {
                                bytes.extend_from_slice(b"\x1b[3~");
                            }
                            KeyCode::PageUp => {
                                bytes.extend_from_slice(b"\x1b[5~");
                            }
                            KeyCode::PageDown => {
                                bytes.extend_from_slice(b"\x1b[6~");
                            }
                            _ => {}
                        }
                    }

                    if !bytes.is_empty() {
                        let _ = pty.write(&bytes);
                        self.clear_selection();
                        return EventResult::Handled;
                    }
                }
                EventResult::Ignored
            }
            Event::Char { character } => {
                if let Some(ref mut pty) = self.pty {
                    self.scroll_offset = 0;
                    let mut buf = [0u8; 4];
                    let s = character.encode_utf8(&mut buf);
                    let _ = pty.write(s.as_bytes());
                    self.clear_selection();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::Scroll { delta, .. } => {
                if delta.y > 0.0 {
                    self.scroll_lines(3);
                } else if delta.y < 0.0 {
                    self.scroll_lines(-3);
                }
                EventResult::Handled
            }
            _ => EventResult::Ignored,
        }
    }

    fn update(&mut self) {
        let rx = self.rx.clone();
        if let Some(rx) = rx {
            if let Ok(rx_lock) = rx.try_lock() {
                while let Ok(bytes) = rx_lock.try_recv() {
                    match bytes {
                        PtyEvent::Output(bytes) => {
                            for b in bytes {
                                self.write_byte(b);
                            }
                        }
                        PtyEvent::Exited => {
                            self.pty_exited = true;
                        }
                        PtyEvent::Error(error) => {
                            self.pty_error = Some(error);
                        }
                    }
                }
            }
        }
        self.sync_display();
    }

    fn children(&self) -> Vec<&dyn Widget> {
        vec![&self.display]
    }

    fn children_mut(&mut self) -> Vec<&mut dyn Widget> {
        vec![&mut self.display]
    }

    fn accessibility(&self) -> Option<AccessibilityNode> {
        Some(AccessibilityNode::new(
            AccessibilityRole::Window,
            "Terminal window",
        ))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Terminal {
    fn point_to_cell(&self, point: slopos_kit::Point) -> Option<GridPoint> {
        let rect = self.rect();
        if !rect.contains(point) {
            return None;
        }

        let col = ((point.x - rect.x) / self.display.cell_width).floor() as usize;
        let row = ((point.y - rect.y) / self.display.cell_height).floor() as usize;
        if col < self.cols && row < self.rows {
            Some(GridPoint { col, row })
        } else {
            None
        }
    }

    fn normalized_selection(&self) -> Option<(GridPoint, GridPoint)> {
        let start = self.selection_start?;
        let end = self.selection_end?;
        let start_index = start.row * self.cols + start.col;
        let end_index = end.row * self.cols + end.col;
        if start_index <= end_index {
            Some((start, end))
        } else {
            Some((end, start))
        }
    }

    fn is_selected(&self, row: usize, col: usize) -> bool {
        let Some((start, end)) = self.normalized_selection() else {
            return false;
        };
        let index = row * self.cols + col;
        let start_index = start.row * self.cols + start.col;
        let end_index = end.row * self.cols + end.col;
        index >= start_index && index <= end_index
    }

    pub fn sync_display(&mut self) {
        let rows = self.visible_rows();
        for row in 0..self.rows {
            for col in 0..self.cols {
                let index = row * self.cols + col;
                let selected = self.is_selected(row, col);
                if let Some(slot) = self.display.cells.get_mut(index) {
                    let cell = rows
                        .get(row)
                        .and_then(|row| row.get(col))
                        .cloned()
                        .unwrap_or_default();
                    let base_fg = if selected {
                        [1.0_f32, 1.0, 1.0, 1.0]
                    } else if cell.bold {
                        // Brighten foreground by 1.3x when bold, clamped to 1.0
                        [
                            (cell.fg.r * 1.3).min(1.0),
                            (cell.fg.g * 1.3).min(1.0),
                            (cell.fg.b * 1.3).min(1.0),
                            cell.fg.a,
                        ]
                    } else {
                        [cell.fg.r, cell.fg.g, cell.fg.b, cell.fg.a]
                    };
                    *slot = MonospaceCell {
                        ch: cell.c,
                        fg: base_fg,
                        bg: if selected {
                            [0.22, 0.43, 0.68, 1.0]
                        } else {
                            [cell.bg.r, cell.bg.g, cell.bg.b, cell.bg.a]
                        },
                    };
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pty::Pty;
    use crate::tabs::TabManager;
    use std::sync::Mutex;

    static CLIPBOARD_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_terminal_emulator() {
        let mut term = Terminal::new(80, 24);
        assert_eq!(term.cols, 80);
        assert_eq!(term.rows, 24);
        assert_eq!(term.cursor_x, 0);
        assert_eq!(term.cursor_y, 0);

        // Print character
        term.print_char('A');
        assert_eq!(term.cursor_x, 1);
        assert_eq!(term.cursor_y, 0);
        assert_eq!(term.grid[0].c, 'A');

        // Verify Cell fields
        let cell = &term.grid[0];
        let _ = cell.fg;
        let _ = cell.bg;
        let _ = cell.bold;
        let _ = cell.italic;
        let _ = cell.underline;

        // Write VT bytes
        term.write_byte(b'B');
        assert_eq!(term.cursor_x, 2);
        assert_eq!(term.grid[1].c, 'B');

        // Backspace CSI
        term.write_byte(0x08);
        assert_eq!(term.cursor_x, 1);

        // Carriage Return CSI
        term.write_byte(0x0d);
        assert_eq!(term.cursor_x, 0);

        // Line Feed CSI
        term.write_byte(0x0a);
        assert_eq!(term.cursor_y, 1);

        // Write ANSI styling CSI (Set red foreground)
        term.write_byte(0x1b);
        term.write_byte(b'[');
        term.write_byte(b'3');
        term.write_byte(b'1');
        term.write_byte(b'm');
        assert_eq!(term.current_fg.r, 0.8);

        // Clear screen (ESC [ 2 J)
        term.write_byte(0x1b);
        term.write_byte(b'[');
        term.write_byte(b'2');
        term.write_byte(b'J');
        assert_eq!(term.cursor_x, 0);
        assert_eq!(term.cursor_y, 0);
        assert_eq!(term.grid[0].c, ' ');

        // Test resize
        term.resize_term(40, 12);
        assert_eq!(term.cols, 40);
        assert_eq!(term.rows, 12);
        let _ = term.max_scrollback;
        let _ = term.scrollback;
    }

    #[test]
    fn test_tab_manager() {
        let mut tab_manager = TabManager::new();
        assert!(tab_manager.tabs.is_empty());

        let _tab_id = tab_manager.open_tab(80, 24).unwrap();
        assert_eq!(tab_manager.tabs.len(), 1);
        assert_eq!(tab_manager.active_tab_index, 0);

        // Open another tab
        let _tab_id_2 = tab_manager.open_tab(80, 24).unwrap();
        assert_eq!(tab_manager.tabs.len(), 2);
        assert_eq!(tab_manager.active_tab_index, 1);

        // Switch tab
        assert!(tab_manager.switch_tab(0));
        assert_eq!(tab_manager.active_tab_index, 0);

        // Get tab references and read fields to avoid dead_code warnings
        {
            let active = tab_manager.active_tab().unwrap();
            assert_eq!(active.id, 1);
            assert!(!active.title.is_empty());
            let _ = active.term.cols;
            let _ = active.pty.master_file.metadata();
        }
        assert!(tab_manager.active_tab_mut().is_some());

        // Close tabs and clean up processes
        let child_pid = tab_manager.tabs[1].child_pid;
        let _ = nix::sys::signal::kill(child_pid, nix::sys::signal::Signal::SIGKILL);

        let child_pid_active = tab_manager.tabs[0].child_pid;
        let _ = nix::sys::signal::kill(child_pid_active, nix::sys::signal::Signal::SIGKILL);

        assert!(tab_manager.close_tab(1));
        assert_eq!(tab_manager.tabs.len(), 1);
    }

    #[test]
    fn test_pty_creation_and_resize() {
        let res = Pty::new(80, 24);
        assert!(res.is_ok());
        let (pty, pid) = res.unwrap();

        // Test PTY resizing
        assert!(pty.resize(100, 30).is_ok());

        // Clean up process
        let _ = nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGKILL);
    }

    #[test]
    fn terminal_scrollback_changes_visible_text() {
        let mut term = Terminal::new(20, 2);
        for byte in b"first\r\nsecond\r\nthird" {
            term.write_byte(*byte);
        }
        term.sync_display();

        assert!(term.visible_text().contains("third"));

        term.scroll_lines(1);

        assert_eq!(term.scroll_offset, 1);
        assert!(term.visible_text().contains("second"));
        assert!(!term.visible_text().contains("third"));
    }

    #[test]
    fn terminal_meta_copy_copies_visible_text() {
        let _guard = CLIPBOARD_TEST_LOCK.lock().unwrap();
        Clipboard::clear();
        let mut term = Terminal::new(8, 2);
        for byte in b"copy me" {
            term.write_byte(*byte);
        }

        let result = term.handle_event(&Event::KeyDown {
            key: KeyCode::C,
            modifiers: slopos_kit::event::Modifiers {
                shift: false,
                control: false,
                alt: false,
                meta: true,
            },
        });

        assert!(matches!(result, EventResult::Handled));
        assert!(Clipboard::paste().contains("copy me"));
    }

    #[test]
    fn terminal_selected_text_copies_only_selection() {
        let _guard = CLIPBOARD_TEST_LOCK.lock().unwrap();
        Clipboard::clear();
        let mut term = Terminal::new(12, 2);
        for byte in b"alpha beta\r\nnext" {
            term.write_byte(*byte);
        }

        term.selection_start = Some(GridPoint { col: 6, row: 0 });
        term.selection_end = Some(GridPoint { col: 9, row: 0 });

        let result = term.handle_event(&Event::KeyDown {
            key: KeyCode::C,
            modifiers: slopos_kit::event::Modifiers {
                shift: false,
                control: false,
                alt: false,
                meta: true,
            },
        });

        assert!(matches!(result, EventResult::Handled));
        assert_eq!(Clipboard::paste(), "beta");
    }

    #[test]
    fn terminal_mouse_drag_highlights_selected_cells() {
        let mut term = Terminal::new(12, 2);
        term.layout(LayoutConstraint::tight(Size::new(96.0, 32.0)));
        for byte in b"select me" {
            term.write_byte(*byte);
        }

        let down = term.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point: slopos_kit::Point::new(0.0, 0.0),
            modifiers: slopos_kit::event::Modifiers::NONE,
        });
        let drag = term.handle_event(&Event::MouseMove {
            point: slopos_kit::Point::new(47.0, 0.0),
            modifiers: slopos_kit::event::Modifiers::NONE,
        });
        let up = term.handle_event(&Event::MouseUp {
            button: MouseButton::Left,
            point: slopos_kit::Point::new(47.0, 0.0),
            modifiers: slopos_kit::event::Modifiers::NONE,
        });

        assert!(matches!(down, EventResult::Handled));
        assert!(matches!(drag, EventResult::Handled));
        assert!(matches!(up, EventResult::Handled));
        assert_eq!(term.selected_text().as_deref(), Some("select"));
        assert_eq!(term.display.cells[0].bg, [0.22, 0.43, 0.68, 1.0]);
        assert_eq!(term.display.cells[5].bg, [0.22, 0.43, 0.68, 1.0]);
    }

    #[test]
    fn terminal_meta_a_selects_visible_buffer() {
        let mut term = Terminal::new(8, 2);
        for byte in b"one\r\ntwo" {
            term.write_byte(*byte);
        }

        let result = term.handle_event(&Event::KeyDown {
            key: KeyCode::A,
            modifiers: slopos_kit::event::Modifiers {
                shift: false,
                control: false,
                alt: false,
                meta: true,
            },
        });

        assert!(matches!(result, EventResult::Handled));
        assert_eq!(term.selected_text().as_deref(), Some("one\ntwo"));
    }

    #[test]
    fn terminal_update_consumes_pty_events_and_exposes_window_role() {
        let mut term = Terminal::new(8, 2);
        let (tx, rx) = std::sync::mpsc::channel();
        term.rx = Some(Arc::new(Mutex::new(rx)));
        tx.send(PtyEvent::Output(b"ok".to_vec())).unwrap();
        tx.send(PtyEvent::Exited).unwrap();

        Widget::update(&mut term);

        assert_eq!(term.visible_text().lines().next(), Some("ok"));
        assert!(term.pty_exited);
        assert!(term.pty_error.is_none());
        assert_eq!(
            term.accessibility().unwrap().role,
            AccessibilityRole::Window
        );
    }

    #[test]
    fn terminal_update_retains_pty_error() {
        let mut term = Terminal::new(8, 2);
        let (tx, rx) = std::sync::mpsc::channel();
        term.rx = Some(Arc::new(Mutex::new(rx)));
        tx.send(PtyEvent::Error("reader failed".to_string()))
            .unwrap();

        Widget::update(&mut term);

        assert_eq!(term.pty_error.as_deref(), Some("reader failed"));
        assert!(!term.pty_exited);
    }
}
