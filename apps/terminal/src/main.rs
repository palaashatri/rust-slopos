//! SLOPOS Terminal: a native terminal that runs the user's shell on a
//! pseudo-terminal. This is the CPU-only first slice: the escape-sequence
//! parser, the cell grid and the painting are all software, drawn into the
//! toolkit's raster surface.
//!
//! The shell's output arrives on a reader thread that wakes the window; the
//! window never polls. Window size changes reflow the grid and tell the shell
//! the new size (TIOCSWINSZ).
//!
//! Not in this slice: mouse reporting, and the GPU path.

mod clicks;
mod colors;
mod grid;
mod input;
mod parser;
mod pty;

use std::sync::mpsc::Receiver;
use std::sync::{Arc, OnceLock};

use accesskit::{Node, Role, TreeUpdate};
use slopos_ui::accessibility::window_tree;
use slopos_ui::controls::menu::{MenuBar, MenuBarOutcome, MenuItem};
use slopos_ui::event::{Event, Key, Modifiers, PointerButton};
use slopos_ui::foundation::color::Color as Rgb;
use slopos_ui::foundation::geometry::Rect;
use slopos_ui::platform::{self, App, Control, Proxy, WindowOptions};
use slopos_ui::render::{RasterSurface, RenderError};
use slopos_ui::text::{measure, Font};
use slopos_ui::theme::palette;

use grid::{Grid, Width};
use parser::Terminal;
use pty::Output;

const APP_ID: &str = "slopos-terminal";
const FONT_SIZE: f32 = 13.0;
/// Space between the window edge and the text.
const PADDING: i32 = 4;
const DEFAULT_COLS: usize = 80;
const DEFAULT_ROWS: usize = 24;
/// Lines moved per wheel step.
const WHEEL_LINES: isize = 3;
const MENU_ID: u64 = 100;
const TERMINAL_NODE: u64 = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    NewWindow,
    Quit,
    Copy,
    Paste,
}

/// Menu titles, their items as (label, shortcut, command), and the index of
/// each command's item within its menu. An item without a command is a
/// separator.
type MenuSpec = &'static [(&'static str, Option<&'static str>, Option<Command>)];
const MENUS: &[(&str, MenuSpec)] = &[
    (
        "File",
        &[
            ("New Window", Some("Ctrl+Shift+N"), Some(Command::NewWindow)),
            ("", None, None),
            ("Quit", Some("Ctrl+Shift+Q"), Some(Command::Quit)),
        ],
    ),
    (
        "Edit",
        &[
            ("Copy", Some("Ctrl+Shift+C"), Some(Command::Copy)),
            ("Paste", Some("Ctrl+Shift+V"), Some(Command::Paste)),
        ],
    ),
];
const EDIT_MENU: usize = 1;
const COPY_ITEM: usize = 0;

/// The key chords that belong to the window rather than the shell. Ctrl+Shift
/// keeps them clear of the Ctrl codes programs use.
fn shortcut(key: &Key, modifiers: Modifiers) -> Option<Command> {
    if !(modifiers.control && modifiers.shift) {
        return None;
    }
    let Key::Character(ch) = key else {
        return None;
    };
    match ch.to_ascii_lowercase() {
        'c' => Some(Command::Copy),
        'v' => Some(Command::Paste),
        'n' => Some(Command::NewWindow),
        'q' => Some(Command::Quit),
        _ => None,
    }
}

fn build_menu() -> MenuBar {
    MenuBar::new(
        MENU_ID,
        MENUS
            .iter()
            .map(|(title, items)| {
                let items = items
                    .iter()
                    .map(|(label, shortcut, command)| match (command, shortcut) {
                        (None, _) => MenuItem::separator(),
                        (Some(_), Some(text)) => MenuItem::new(label).shortcut(text),
                        (Some(_), None) => MenuItem::new(label),
                    })
                    .collect();
                (*title, items)
            })
            .collect(),
    )
}

/// The width and height of one character cell, in logical pixels.
fn cell_size(font: &Font) -> (u32, u32) {
    let width = measure("M", font).width.ceil().max(1.0) as u32;
    let height = measure("Mg", font).height.ceil().max(1.0) as u32;
    (width, height)
}

/// What a cell looks like; equal styles are drawn as one run of text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Style {
    fg: Rgb,
    bg: Rgb,
    bold: bool,
}

struct TerminalApp {
    terminal: Terminal,
    /// The shell, its output channel, or `None` when it could not start.
    shell: Option<(pty::Pty, Receiver<Output>)>,
    /// Shown in place of the screen when no shell runs.
    status: String,
    menu: MenuBar,
    font: Font,
    bold_font: Font,
    cell: (u32, u32),
    window: (u32, u32),
    /// A primary-button drag is extending the selection.
    selecting: bool,
    /// The last primary-button press, for double and triple clicks.
    last_click: Option<clicks::Click>,
    shown_title: String,
    /// The window's proxy, filled in once it exists. Used to ask for another
    /// output turn when a turn stopped at its budget.
    wake_slot: Arc<OnceLock<Proxy>>,
}

/// Output bytes the window feeds to the terminal in one turn. The rest waits
/// for the next turn, so painting and Close keep up with a fast program.
const OUTPUT_BUDGET: usize = 64 * 1024;

impl TerminalApp {
    /// A terminal of the default size, with no shell running yet.
    fn new() -> Self {
        let font = Font::monospace(FONT_SIZE);
        let bold_font = font.clone().bold();
        let cell = cell_size(&font);
        let mut app = Self {
            terminal: Terminal::new(DEFAULT_COLS, DEFAULT_ROWS),
            shell: None,
            status: String::new(),
            menu: build_menu(),
            font,
            bold_font,
            cell,
            window: (0, 0),
            selecting: false,
            last_click: None,
            shown_title: "Terminal".into(),
            wake_slot: Arc::new(OnceLock::new()),
        };
        app.window = app.window_for(DEFAULT_COLS, DEFAULT_ROWS);
        app.layout();
        app
    }

    /// Run `program` on a pseudo-terminal sized to the grid. `wake` is called
    /// from the reader thread when output or the exit arrives.
    fn start_shell(&mut self, program: &str, args: &[&str], wake: impl Fn() + Send + 'static) {
        let grid = self.terminal.grid();
        let cols = u16::try_from(grid.cols()).unwrap_or(u16::MAX);
        let rows = u16::try_from(grid.rows()).unwrap_or(u16::MAX);
        match pty::spawn(program, args, cols, rows, wake) {
            Ok(shell) => self.shell = Some(shell),
            Err(error) => self.status = format!("Could not start {program}: {error}"),
        }
    }

    /// The window size that fits `cols` by `rows` characters.
    fn window_for(&self, cols: usize, rows: usize) -> (u32, u32) {
        let (cell_w, cell_h) = self.cell;
        let width = cols as u32 * cell_w + 2 * PADDING as u32;
        let height = self.menu.height() + rows as u32 * cell_h + 2 * PADDING as u32;
        (width, height)
    }

    /// The characters that fit in the window below the menu bar.
    fn grid_size(&self) -> (usize, usize) {
        let (cell_w, cell_h) = self.cell;
        let width = (self.window.0 as i32 - 2 * PADDING).max(0) as u32;
        let height = (self.window.1 as i32 - self.menu.height() as i32 - 2 * PADDING).max(0) as u32;
        (
            (width / cell_w).max(1) as usize,
            (height / cell_h).max(1) as usize,
        )
    }

    /// Fit the menu bar and the grid to the window. The shell is told only
    /// when the size really changes.
    fn layout(&mut self) {
        self.menu.bounds = Rect {
            x: 0,
            y: 0,
            width: self.window.0,
            height: self.menu.height(),
        };
        let (cols, rows) = self.grid_size();
        let before = (self.terminal.grid().cols(), self.terminal.grid().rows());
        self.terminal.grid_mut().resize(cols, rows);
        if before != (cols, rows) {
            if let Some((shell, _)) = &self.shell {
                let cols = u16::try_from(cols).unwrap_or(u16::MAX);
                let rows = u16::try_from(rows).unwrap_or(u16::MAX);
                if let Err(error) = shell.resize(cols, rows) {
                    eprintln!("slopos-terminal: cannot resize the shell: {error}");
                }
            }
        }
    }

    /// Top-left corner of the first cell, in logical pixels.
    fn origin(&self) -> (i32, i32) {
        (PADDING, self.menu.height() as i32 + PADDING)
    }

    /// The cell under a point, or `None` when it lies outside the grid.
    fn cell_at(&self, x: i32, y: i32) -> Option<(usize, usize)> {
        let (left, top) = self.origin();
        if x < left || y < top {
            return None;
        }
        let (cell_w, cell_h) = (self.cell.0 as i32, self.cell.1 as i32);
        let col = ((x - left) / cell_w) as usize;
        let row = ((y - top) / cell_h) as usize;
        let grid = self.terminal.grid();
        (col < grid.cols() && row < grid.rows()).then_some((row, col))
    }

    /// The cell nearest a point, clamped to the grid. A drag keeps extending
    /// the selection while the pointer leaves the text.
    fn cell_clamped(&self, x: i32, y: i32) -> (usize, usize) {
        let (left, top) = self.origin();
        let (cell_w, cell_h) = (self.cell.0 as i32, self.cell.1 as i32);
        let grid = self.terminal.grid();
        let col = ((x - left).max(0) / cell_w) as usize;
        let row = ((y - top).max(0) / cell_h) as usize;
        (row.min(grid.rows() - 1), col.min(grid.cols() - 1))
    }

    fn write(&self, bytes: &[u8]) {
        if let Some((shell, _)) = &self.shell {
            if let Err(error) = shell.write(bytes) {
                // The program is not taking its input; the keystroke is refused.
                eprintln!("slopos-terminal: {error}");
            }
        }
    }

    /// Ask the window for another output turn. The reader's wake-ups coalesce
    /// into one turn, so a turn that stopped at its budget asks for the next.
    fn request_output_turn(&self) {
        if let Some(proxy) = self.wake_slot.get() {
            proxy.wake();
        }
    }

    /// Feed the shell's pending output to the terminal and send back any
    /// device-status replies. The window closes when the shell exits.
    fn drain_output(&mut self, control: &mut Control) {
        let mut exited = false;
        let mut more = false;
        let mut fed = 0;
        if let Some((shell, output)) = &self.shell {
            while let Ok(message) = output.try_recv() {
                match message {
                    Output::Data(bytes) => {
                        fed += bytes.len();
                        self.terminal.feed(&bytes);
                        let replies = self.terminal.take_replies();
                        if !replies.is_empty() {
                            if let Err(error) = shell.write(&replies) {
                                eprintln!("slopos-terminal: {error}");
                            }
                        }
                        if fed >= OUTPUT_BUDGET {
                            more = true;
                            break;
                        }
                    }
                    Output::Exited => exited = true,
                }
            }
        }
        if exited {
            control.quit();
        }
        if more {
            self.request_output_turn();
        }
        control.request_redraw();
    }

    /// A primary-button press on a cell: a single press starts a drag
    /// selection, a double press selects the word under it and a triple press
    /// the line.
    fn press(&mut self, (row, col): (usize, usize), now: u32) {
        let count = clicks::next_count(self.last_click, now, (row, col));
        self.last_click = Some(clicks::Click {
            at: now,
            cell: (row, col),
            count,
        });
        let grid = self.terminal.grid_mut();
        match count {
            2 => grid.select_word(row, col),
            3 => grid.select_line(row),
            _ => {
                grid.begin_selection(row, col);
                self.selecting = true;
            }
        }
    }

    fn key(&mut self, key: &Key, modifiers: Modifiers, control: &mut Control) {
        if let Some(command) = shortcut(key, modifiers) {
            self.run(command, control);
            return;
        }
        // A key that sends nothing leaves the scrollback view and the selection
        // as they are.
        if let Some(bytes) = input::key_bytes(key, modifiers, self.terminal.application_cursor()) {
            self.terminal.grid_mut().scroll_to_bottom();
            self.write(&bytes);
        }
        control.request_redraw();
    }

    fn paste(&mut self, text: &str) {
        match input::paste_bytes(text, self.terminal.bracketed_paste()) {
            Some(bytes) => {
                self.terminal.grid_mut().scroll_to_bottom();
                self.write(&bytes);
            }
            None => eprintln!(
                "slopos-terminal: a multi-line paste is refused until the program enables bracketed paste"
            ),
        }
    }

    fn run(&mut self, command: Command, control: &mut Control) {
        match command {
            Command::Copy => {
                if let Some(text) = self.terminal.grid().selected_text() {
                    control.set_clipboard(&text);
                }
            }
            Command::Paste => control.request_paste(),
            Command::NewWindow => {
                if let Err(error) = slopos_core::process::launch(APP_ID, &[]) {
                    eprintln!("slopos-terminal: cannot open a new window: {error}");
                }
            }
            Command::Quit => control.quit(),
        }
    }

    /// Keep the Copy item in step with whether anything is selected.
    fn sync_menu(&mut self) {
        let has_selection = self.terminal.grid().has_selection();
        self.menu.menus[EDIT_MENU].items[COPY_ITEM].enabled = has_selection;
    }

    fn sync_title(&mut self, control: &mut Control) {
        let title = self.terminal.title().unwrap_or("Terminal");
        if title != self.shown_title {
            control.set_title(title);
            self.shown_title = title.to_owned();
        }
    }

    fn style_at(&self, row: usize, col: usize, cursor: Option<(usize, usize)>) -> Style {
        let colors = palette();
        let grid = self.terminal.grid();
        // The second column of a wide character takes its first column's style,
        // and the cursor covers either column of it.
        let lead = if grid.viewport_cell(row, col).width == Width::Spacer && col > 0 {
            col - 1
        } else {
            col
        };
        let cell = grid.viewport_cell(row, lead);
        let mut fg = colors::resolve(cell.attr.fg, colors.ink);
        let mut bg = colors::resolve(cell.attr.bg, colors.content);
        let at_cursor = cursor.is_some_and(|(r, c)| r == row && (c == col || c == lead));
        if cell.attr.inverse ^ at_cursor {
            std::mem::swap(&mut fg, &mut bg);
        }
        if grid.is_selected(row, col) {
            fg = colors.selection_ink;
            bg = colors.selection;
        }
        Style {
            fg,
            bg,
            bold: cell.attr.bold,
        }
    }

    fn paint_grid(&self, surface: &RasterSurface) -> Result<(), RenderError> {
        let colors = palette();
        let (origin_x, origin_y) = self.origin();
        let (cell_w, cell_h) = (self.cell.0 as i32, self.cell.1 as i32);
        if self.shell.is_none() {
            let message = self.no_shell_message();
            return surface.draw_text(message, origin_x, origin_y, &self.font, colors.ink);
        }
        let grid: &Grid = self.terminal.grid();
        let cursor = (grid.display_offset() == 0 && grid.cursor_visible()).then(|| grid.cursor());
        for row in 0..grid.rows() {
            let y = origin_y + row as i32 * cell_h;
            let mut col = 0;
            while col < grid.cols() {
                let style = self.style_at(row, col, cursor);
                let mut end = col + 1;
                while end < grid.cols() && self.style_at(row, end, cursor) == style {
                    end += 1;
                }
                let width = (end - col) as u32 * self.cell.0;
                surface.fill_rect(
                    Rect {
                        x: origin_x + col as i32 * cell_w,
                        y,
                        width,
                        height: self.cell.1,
                    },
                    style.bg,
                )?;
                let font = if style.bold {
                    &self.bold_font
                } else {
                    &self.font
                };
                // Each character is drawn at its own column, so glyphs cannot
                // drift from the backgrounds and the hit-test cells. A wide
                // character is drawn across its two columns; a spacer draws nothing.
                for index in col..end {
                    let cell = grid.viewport_cell(row, index);
                    if cell.width == Width::Spacer || cell.ch == ' ' && cell.marks[0] == '\0' {
                        continue;
                    }
                    let mut text = String::new();
                    cell.push_text(&mut text);
                    let x = origin_x + index as i32 * cell_w;
                    surface.draw_text(&text, x, y, font, style.fg)?;
                }
                col = end;
            }
        }
        Ok(())
    }

    fn visible_text(&self) -> String {
        self.terminal.grid().text()
    }

    /// The line shown in place of the screen when no shell runs: the reason
    /// the shell did not start, or a plain notice.
    fn no_shell_message(&self) -> &str {
        if self.status.is_empty() {
            "No shell is running."
        } else {
            self.status.as_str()
        }
    }
}

impl App for TerminalApp {
    fn event(&mut self, event: &Event, control: &mut Control) {
        if let Event::GlobalMenu(_) = event {
            self.menu.handle(event, self.window);
            self.layout();
            control.request_redraw();
            return;
        }
        match self.menu.handle(event, self.window) {
            MenuBarOutcome::Chosen { menu, item } => {
                let command = MENUS
                    .get(menu)
                    .and_then(|(_, items)| items.get(item))
                    .and_then(|entry| entry.2);
                if let Some(command) = command {
                    self.run(command, control);
                }
                control.request_redraw();
            }
            MenuBarOutcome::Changed => control.request_redraw(),
            // While a menu is open it takes the keyboard and the pointer.
            MenuBarOutcome::Ignored if self.menu.is_open() && is_input(event) => {}
            MenuBarOutcome::Ignored => self.handle(event, control),
        }
        self.sync_menu();
        self.sync_title(control);
    }

    fn paint(&mut self, surface: &RasterSurface) {
        let colors = palette();
        let _ = surface.clear(colors.content);
        if let Err(error) = self.menu.paint(surface) {
            eprintln!("slopos-terminal: paint failed: {error}");
        }
        if let Err(error) = self.paint_grid(surface) {
            eprintln!("slopos-terminal: paint failed: {error}");
        }
        if let Err(error) = self.menu.paint_menus(surface) {
            eprintln!("slopos-terminal: paint failed: {error}");
        }
    }

    fn accessibility(&self) -> Option<TreeUpdate> {
        let mut nodes = self.menu.accessibility_nodes();
        let mut terminal = Node::new(Role::Terminal);
        terminal.set_label("Terminal");
        terminal.set_value(self.visible_text());
        nodes.push((TERMINAL_NODE, terminal));
        Some(window_tree(
            self.shown_title.as_str(),
            nodes,
            Some(TERMINAL_NODE),
        ))
    }

    fn menu_bar(&self) -> Option<&MenuBar> {
        Some(&self.menu)
    }
}

impl TerminalApp {
    fn handle(&mut self, event: &Event, control: &mut Control) {
        match event {
            Event::Wake => self.drain_output(control),
            Event::Resized { width, height } => {
                self.window = (*width, *height);
                self.layout();
                control.request_redraw();
            }
            Event::CloseRequested => control.quit(),
            Event::KeyPressed { key, modifiers } => self.key(key, *modifiers, control),
            Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
            } => {
                self.selecting = false;
                self.terminal.grid_mut().clear_selection();
                match self.cell_at(*x, *y) {
                    // The server's press time; 0 when the host reports none.
                    Some(cell) => self.press(cell, control.press_time().unwrap_or(0)),
                    None => self.last_click = None,
                }
                control.request_redraw();
            }
            Event::PointerMoved { x, y } if self.selecting => {
                let (row, col) = self.cell_clamped(*x, *y);
                self.terminal.grid_mut().extend_selection(row, col);
                control.request_redraw();
            }
            Event::PointerReleased {
                button: PointerButton::Primary,
                ..
            } => self.selecting = false,
            Event::Scroll { dy, .. } => {
                // Positive dy scrolls down, towards the newest output.
                let lines = -(*dy as isize) * WHEEL_LINES;
                self.terminal.grid_mut().scroll_view(lines);
                control.request_redraw();
            }
            Event::Paste(text) => {
                self.paste(text);
                control.request_redraw();
            }
            _ => {}
        }
    }
}

fn is_input(event: &Event) -> bool {
    matches!(
        event,
        Event::KeyPressed { .. }
            | Event::PointerPressed { .. }
            | Event::PointerReleased { .. }
            | Event::PointerMoved { .. }
            | Event::Scroll { .. }
            | Event::Paste(_)
    )
}

fn main() {
    if std::env::var_os("RUST_LOG").is_some() {
        let _ = env_logger::try_init();
    }
    // The shell starts before the window exists, so its wake-ups go through
    // a slot that the window fills in once it has its proxy.
    let proxy_slot: Arc<OnceLock<Proxy>> = Arc::new(OnceLock::new());
    let wake_slot = Arc::clone(&proxy_slot);
    let wake = move || {
        if let Some(proxy) = wake_slot.get() {
            proxy.wake();
        }
    };
    let mut app = TerminalApp::new();
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|shell| !shell.is_empty())
        .unwrap_or_else(|| "/bin/sh".into());
    app.wake_slot = Arc::clone(&proxy_slot);
    app.start_shell(&shell, &[], wake);
    let (width, height) = app.window;
    let options = WindowOptions::new("Terminal", APP_ID, width, height);
    let result = platform::run(options, app, move |proxy| {
        let _ = proxy_slot.set(proxy);
        // Anything the shell printed before the slot was filled.
        if let Some(proxy) = proxy_slot.get() {
            proxy.wake();
        }
    });
    if let Err(error) = result {
        eprintln!("slopos-terminal: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    /// Resize the window so that it fits `cols` by `rows` characters.
    fn fit(app: &mut TerminalApp, cols: usize, rows: usize) {
        let (width, height) = app.window_for(cols, rows);
        let mut control = Control::default();
        app.event(&Event::Resized { width, height }, &mut control);
    }

    #[test]
    fn copy_puts_the_selection_on_the_clipboard() {
        let mut app = TerminalApp::new();
        app.terminal.feed(b"hello world");
        app.terminal.grid_mut().begin_selection(0, 0);
        app.terminal.grid_mut().extend_selection(0, 4);
        let mut control = Control::default();
        app.event(
            &Event::MenuChosen {
                menu: EDIT_MENU,
                item: COPY_ITEM,
            },
            &mut control,
        );
        assert_eq!(control.pending_clipboard(), Some("hello"));
    }

    #[test]
    fn copy_is_disabled_until_something_is_selected() {
        let mut app = TerminalApp::new();
        app.sync_menu();
        assert!(!app.menu.menus[EDIT_MENU].items[COPY_ITEM].enabled);
        app.terminal.feed(b"abc");
        app.terminal.grid_mut().begin_selection(0, 0);
        app.terminal.grid_mut().extend_selection(0, 1);
        app.sync_menu();
        assert!(app.menu.menus[EDIT_MENU].items[COPY_ITEM].enabled);
    }

    #[test]
    fn paste_is_requested_from_the_edit_menu() {
        let mut app = TerminalApp::new();
        let mut control = Control::default();
        app.event(
            &Event::MenuChosen {
                menu: EDIT_MENU,
                item: 1,
            },
            &mut control,
        );
        assert!(control.paste_requested());
    }

    #[test]
    fn ctrl_shift_chords_are_window_shortcuts_and_ctrl_alone_goes_to_the_shell() {
        let ctrl_shift = Modifiers {
            control: true,
            shift: true,
            ..Modifiers::default()
        };
        let ctrl = Modifiers {
            control: true,
            ..Modifiers::default()
        };
        assert_eq!(
            shortcut(&Key::Character('C'), ctrl_shift),
            Some(Command::Copy)
        );
        assert_eq!(
            shortcut(&Key::Character('q'), ctrl_shift),
            Some(Command::Quit)
        );
        assert_eq!(shortcut(&Key::Character('c'), ctrl), None);
    }

    #[test]
    fn resizing_the_window_changes_the_grid_columns_and_rows() {
        let mut app = TerminalApp::new();
        let (cols, rows) = (app.terminal.grid().cols(), app.terminal.grid().rows());
        fit(&mut app, 120, 40);
        assert_eq!(app.terminal.grid().cols(), 120);
        assert_eq!(app.terminal.grid().rows(), 40);
        assert_ne!((cols, rows), (120, 40));
        fit(&mut app, 30, 10);
        assert_eq!(app.terminal.grid().cols(), 30);
        assert_eq!(app.terminal.grid().rows(), 10);
    }

    #[test]
    fn a_shell_that_exits_closes_the_window() {
        let mut app = TerminalApp::new();
        app.start_shell("sh", &["-c", "echo bye"], || {});
        assert!(app.shell.is_some(), "/bin/sh starts on a pseudo-terminal");
        let mut control = Control::default();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !control.quit_requested() && Instant::now() < deadline {
            app.event(&Event::Wake, &mut control);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(control.quit_requested());
        assert!(app.terminal.grid().text().contains("bye"));
    }

    /// The pixel position of the middle of a cell.
    fn point(app: &TerminalApp, row: usize, col: usize) -> (i32, i32) {
        let (left, top) = app.origin();
        (
            left + col as i32 * app.cell.0 as i32 + 1,
            top + row as i32 * app.cell.1 as i32 + 1,
        )
    }

    /// A primary-button press and release at a pixel position.
    fn click(app: &mut TerminalApp, (x, y): (i32, i32), control: &mut Control) {
        app.event(
            &Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
            },
            control,
        );
        app.event(
            &Event::PointerReleased {
                x,
                y,
                button: PointerButton::Primary,
            },
            control,
        );
    }

    #[test]
    fn a_double_click_selects_the_word_and_a_triple_click_the_line() {
        let mut app = TerminalApp::new();
        app.terminal.feed(b"hello world\r\nsecond line");
        let mut control = Control::default();
        let on_hello = point(&app, 0, 1);
        click(&mut app, on_hello, &mut control);
        click(&mut app, on_hello, &mut control);
        assert_eq!(
            app.terminal.grid().selected_text().as_deref(),
            Some("hello")
        );

        let on_second = point(&app, 1, 2);
        for _ in 0..3 {
            click(&mut app, on_second, &mut control);
        }
        assert_eq!(
            app.terminal.grid().selected_text().as_deref(),
            Some("second line")
        );
    }

    #[test]
    fn a_single_click_and_drag_still_select_a_range() {
        let mut app = TerminalApp::new();
        app.terminal.feed(b"hello world");
        let mut control = Control::default();
        let start = point(&app, 0, 0);
        let end = point(&app, 0, 4);
        app.event(
            &Event::PointerPressed {
                x: start.0,
                y: start.1,
                button: PointerButton::Primary,
            },
            &mut control,
        );
        app.event(&Event::PointerMoved { x: end.0, y: end.1 }, &mut control);
        app.event(
            &Event::PointerReleased {
                x: end.0,
                y: end.1,
                button: PointerButton::Primary,
            },
            &mut control,
        );
        assert_eq!(
            app.terminal.grid().selected_text().as_deref(),
            Some("hello")
        );
    }

    #[test]
    fn without_a_shell_the_window_says_so() {
        let mut app = TerminalApp::new();
        assert!(app.shell.is_none());
        assert!(!app.terminal.grid().has_selection());
        // With no reason recorded, the window shows the plain notice.
        assert_eq!(app.no_shell_message(), "No shell is running.");
        // A start failure is what the window shows instead.
        app.status = "Could not start /bin/zsh: No such file or directory".into();
        assert_eq!(
            app.no_shell_message(),
            "Could not start /bin/zsh: No such file or directory"
        );
    }
}
