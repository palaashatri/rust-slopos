//! The VT/xterm side of the terminal: `vte` splits the byte stream into
//! control characters, escape sequences and text, and this module applies
//! them to the [`Grid`].
//!
//! Supported: printing, CR, LF, BS, tab, cursor movement (CUU/CUD/CUF/CUB,
//! CNL/CPL, CHA, VPA, CUP/HVP), erase (EL, ED), insert/delete of characters
//! and lines (ICH, DCH, ECH, IL, DL), SGR text attributes with the 16 basic
//! and bright colours and the 256-colour table, device status reports
//! (CSI 5 n and CSI 6 n), DECTCEM cursor visibility (`?25`), the alternate
//! screen (`?47`, `?1047`, `?1049`), and OSC 0/2 window titles. Anything else
//! is ignored.
//!
//! Not supported yet: scroll regions (DECSTBM), true colour (`38;2`, parsed
//! and skipped), character sets, mouse reporting, and save/restore cursor
//! (DECSC/DECRC; only mode 1049 saves the cursor).

use crate::grid::{Attr, Color, Grid};

/// A terminal: the grid plus the parser state that persists between reads.
pub struct Terminal {
    grid: Grid,
    parser: vte::Parser,
    replies: Vec<u8>,
    title: Option<String>,
}

impl Terminal {
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            grid: Grid::new(cols, rows),
            parser: vte::Parser::new(),
            replies: Vec::new(),
            title: None,
        }
    }

    /// Feed bytes from the PTY. Sequences may be split across calls.
    pub fn feed(&mut self, bytes: &[u8]) {
        let mut performer = Performer {
            grid: &mut self.grid,
            replies: &mut self.replies,
            title: &mut self.title,
        };
        self.parser.advance(&mut performer, bytes);
    }

    pub fn grid(&self) -> &Grid {
        &self.grid
    }

    pub fn grid_mut(&mut self) -> &mut Grid {
        &mut self.grid
    }

    /// Bytes the terminal answers with (device status reports). They go back
    /// to the program on the PTY.
    pub fn take_replies(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.replies)
    }

    /// The title the program set with OSC 0 or 2, if any.
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }
}

struct Performer<'a> {
    grid: &'a mut Grid,
    replies: &'a mut Vec<u8>,
    title: &'a mut Option<String>,
}

/// The sequence's numeric parameters, subparameters flattened. An empty
/// parameter is 0, as vte reports it.
fn flatten(params: &vte::Params) -> Vec<u16> {
    params
        .iter()
        .flat_map(|group| group.iter().copied())
        .collect()
}

impl vte::Perform for Performer<'_> {
    fn print(&mut self, ch: char) {
        self.grid.put(ch);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x08 => self.grid.backspace(),
            0x09 => self.grid.tab(),
            // LF, VT and FF all move down a row.
            0x0A..=0x0C => self.grid.line_feed(),
            0x0D => self.grid.carriage_return(),
            // BEL and the shift codes have no effect here.
            _ => {}
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        // OSC 0 and OSC 2 set the window title.
        if let [code, text, ..] = params {
            if matches!(*code, b"0" | b"2") {
                *self.title = Some(String::from_utf8_lossy(text).into_owned());
            }
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        ignore: bool,
        action: char,
    ) {
        if ignore {
            return;
        }
        if intermediates.first() == Some(&b'?') {
            self.private_mode(params, action);
            return;
        }
        if !intermediates.is_empty() {
            return;
        }
        let values = flatten(params);
        // A missing or zero parameter takes its default.
        let arg = |index: usize, default: usize| -> usize {
            values
                .get(index)
                .copied()
                .filter(|value| *value != 0)
                .map_or(default, usize::from)
        };
        let first = values.first().copied().unwrap_or(0);
        match action {
            'A' => self.grid.move_cursor_by(-(arg(0, 1) as isize), 0),
            'B' | 'e' => self.grid.move_cursor_by(arg(0, 1) as isize, 0),
            'C' | 'a' => self.grid.move_cursor_by(0, arg(0, 1) as isize),
            'D' => self.grid.move_cursor_by(0, -(arg(0, 1) as isize)),
            'E' => {
                self.grid.move_cursor_by(arg(0, 1) as isize, 0);
                self.grid.carriage_return();
            }
            'F' => {
                self.grid.move_cursor_by(-(arg(0, 1) as isize), 0);
                self.grid.carriage_return();
            }
            'G' | '`' => {
                let (row, _) = self.grid.cursor();
                self.grid.move_cursor(row, arg(0, 1) - 1);
            }
            'd' => {
                let (_, col) = self.grid.cursor();
                self.grid.move_cursor(arg(0, 1) - 1, col);
            }
            'H' | 'f' => self.grid.move_cursor(arg(0, 1) - 1, arg(1, 1) - 1),
            'J' => self.grid.erase_display(first),
            'K' => self.grid.erase_line(first),
            '@' => self.grid.insert_chars(arg(0, 1)),
            'P' => self.grid.delete_chars(arg(0, 1)),
            'X' => self.grid.erase_chars(arg(0, 1)),
            'L' => self.grid.insert_lines(arg(0, 1)),
            'M' => self.grid.delete_lines(arg(0, 1)),
            'm' => self.sgr(&values),
            'n' => match first {
                5 => self.replies.extend_from_slice(b"\x1b[0n"),
                6 => {
                    let (row, col) = self.grid.cursor();
                    let report = format!("\x1b[{};{}R", row + 1, col + 1);
                    self.replies.extend_from_slice(report.as_bytes());
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
}

impl Performer<'_> {
    /// DEC private modes (`CSI ? Pm h` to set, `l` to reset): the cursor's
    /// visibility and the alternate screen.
    fn private_mode(&mut self, params: &vte::Params, action: char) {
        let set = match action {
            'h' => true,
            'l' => false,
            _ => return,
        };
        for mode in flatten(params) {
            match (mode, set) {
                (25, _) => self.grid.set_cursor_visible(set),
                // 1049 also saves the cursor on entry and restores it on exit.
                (1049, true) => self.grid.enter_alternate_screen(true),
                (1049, false) => self.grid.leave_alternate_screen(),
                (47 | 1047, true) => self.grid.enter_alternate_screen(false),
                (47 | 1047, false) => self.grid.leave_alternate_screen(),
                _ => {}
            }
        }
    }

    /// Select Graphic Rendition: text attributes and colours.
    fn sgr(&mut self, values: &[u16]) {
        let default = [0u16];
        let values = if values.is_empty() {
            &default[..]
        } else {
            values
        };
        let mut attr: Attr = self.grid.attr();
        let mut index = 0;
        while index < values.len() {
            let code = values[index];
            match code {
                0 => attr = Attr::default(),
                1 => attr.bold = true,
                22 => attr.bold = false,
                7 => attr.inverse = true,
                27 => attr.inverse = false,
                30..=37 => attr.fg = Color::Indexed((code - 30) as u8),
                39 => attr.fg = Color::Default,
                40..=47 => attr.bg = Color::Indexed((code - 40) as u8),
                49 => attr.bg = Color::Default,
                90..=97 => attr.fg = Color::Indexed((code - 90 + 8) as u8),
                100..=107 => attr.bg = Color::Indexed((code - 100 + 8) as u8),
                38 | 48 => {
                    let is_foreground = code == 38;
                    match values.get(index + 1).copied() {
                        // 38;5;n and 48;5;n pick a colour from the 256 table.
                        Some(5) => {
                            if let Some(&entry) = values.get(index + 2) {
                                let color = Color::Indexed(entry.min(255) as u8);
                                if is_foreground {
                                    attr.fg = color;
                                } else {
                                    attr.bg = color;
                                }
                            }
                            index += 2;
                        }
                        // True colour is not drawn yet; skip its r, g and b.
                        Some(2) => index += 4,
                        _ => {}
                    }
                }
                _ => {}
            }
            index += 1;
        }
        self.grid.set_attr(attr);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::{Cell, Width};

    fn terminal_text(terminal: &Terminal) -> String {
        terminal.grid().text().trim_end().to_owned()
    }

    fn cell(terminal: &Terminal, row: usize, col: usize) -> Cell {
        terminal.grid().viewport_cell(row, col)
    }

    #[test]
    fn text_and_line_breaks_are_applied() {
        let mut terminal = Terminal::new(10, 3);
        terminal.feed(b"hello\r\nworld");
        let lines: Vec<String> = terminal_text(&terminal)
            .lines()
            .map(str::to_owned)
            .collect();
        assert_eq!(lines, ["hello", "world"]);
    }

    #[test]
    fn a_sequence_split_across_reads_is_applied() {
        let mut terminal = Terminal::new(10, 2);
        terminal.feed(b"\x1b[3");
        terminal.feed(b"1mX");
        assert_eq!(cell(&terminal, 0, 0).ch, 'X');
        assert_eq!(cell(&terminal, 0, 0).attr.fg, Color::Indexed(1));
    }

    #[test]
    fn cursor_position_and_relative_moves() {
        let mut terminal = Terminal::new(10, 5);
        terminal.feed(b"\x1b[2;3Hx");
        assert_eq!(cell(&terminal, 1, 2).ch, 'x');
        terminal.feed(b"\x1b[H\x1b[2B\x1b[3Cy");
        assert_eq!(cell(&terminal, 2, 3).ch, 'y');
        // A move past the edge stops at the edge.
        terminal.feed(b"\x1b[99A\x1b[99Dz");
        assert_eq!(cell(&terminal, 0, 0).ch, 'z');
    }

    #[test]
    fn sgr_sets_colours_and_bold_and_resets() {
        let mut terminal = Terminal::new(10, 1);
        terminal.feed(b"\x1b[31;1mR\x1b[0mN");
        let red = cell(&terminal, 0, 0);
        assert_eq!(red.attr.fg, Color::Indexed(1));
        assert!(red.attr.bold);
        let plain = cell(&terminal, 0, 1);
        assert_eq!(plain.attr, Attr::default());
    }

    #[test]
    fn bright_and_256_colours_and_background() {
        let mut terminal = Terminal::new(10, 1);
        terminal.feed(b"\x1b[92mG\x1b[38;5;200mA\x1b[48;5;4mB\x1b[0m");
        assert_eq!(cell(&terminal, 0, 0).attr.fg, Color::Indexed(10));
        assert_eq!(cell(&terminal, 0, 1).attr.fg, Color::Indexed(200));
        assert_eq!(cell(&terminal, 0, 2).attr.bg, Color::Indexed(4));
        assert_eq!(cell(&terminal, 0, 2).attr.fg, Color::Indexed(200));
    }

    #[test]
    fn true_colour_operands_are_skipped_not_printed() {
        let mut terminal = Terminal::new(10, 1);
        terminal.feed(b"\x1b[38;2;1;2;3mT");
        assert_eq!(terminal_text(&terminal), "T");
        assert_eq!(cell(&terminal, 0, 0).attr.fg, Color::Default);
    }

    #[test]
    fn erase_and_backspace() {
        let mut terminal = Terminal::new(5, 1);
        terminal.feed(b"abc\x08\x1b[K");
        assert_eq!(terminal_text(&terminal), "ab");
        terminal.feed(b"\x1b[2K");
        assert_eq!(terminal_text(&terminal), "");
    }

    #[test]
    fn tab_moves_to_the_next_stop() {
        let mut terminal = Terminal::new(20, 1);
        terminal.feed(b"a\tb");
        assert_eq!(cell(&terminal, 0, 8).ch, 'b');
    }

    #[test]
    fn character_insert_and_delete() {
        let mut terminal = Terminal::new(5, 1);
        terminal.feed(b"abcd\x1b[1G\x1b[P");
        assert_eq!(terminal_text(&terminal), "bcd");
        terminal.feed(b"\x1b[1G\x1b[@a");
        assert_eq!(terminal_text(&terminal), "abcd");
    }

    #[test]
    fn device_status_reports_are_queued_for_the_program() {
        let mut terminal = Terminal::new(10, 5);
        terminal.feed(b"abc\x1b[6n");
        assert_eq!(terminal.take_replies(), b"\x1b[1;4R");
        assert!(terminal.take_replies().is_empty());
        terminal.feed(b"\x1b[5n");
        assert_eq!(terminal.take_replies(), b"\x1b[0n");
    }

    #[test]
    fn osc_sets_the_title_and_private_modes_hide_the_cursor() {
        let mut terminal = Terminal::new(10, 1);
        terminal.feed(b"\x1b]0;Build log\x07ok\x1b[?25l");
        assert_eq!(terminal.title(), Some("Build log"));
        assert_eq!(terminal_text(&terminal), "ok");
        assert!(!terminal.grid().cursor_visible());
        terminal.feed(b"\x1b[?25h");
        assert!(terminal.grid().cursor_visible());
    }

    #[test]
    fn wide_and_combining_text_from_utf8_bytes_is_placed_by_column() {
        let mut terminal = Terminal::new(10, 1);
        terminal.feed("中文e\u{301}😀".as_bytes());
        assert_eq!(cell(&terminal, 0, 0).ch, '中');
        assert_eq!(cell(&terminal, 0, 0).width, Width::Wide);
        assert_eq!(cell(&terminal, 0, 2).ch, '文');
        assert_eq!(cell(&terminal, 0, 4).marks[0], '\u{301}');
        assert_eq!(cell(&terminal, 0, 5).ch, '😀');
        assert_eq!(terminal.grid().cursor(), (0, 7));
        assert_eq!(terminal_text(&terminal), "中文e\u{301}😀");
    }

    #[test]
    fn a_utf8_character_split_across_reads_is_decoded_once() {
        let mut terminal = Terminal::new(10, 1);
        let bytes = "中".as_bytes();
        terminal.feed(&bytes[..1]);
        terminal.feed(&bytes[1..]);
        assert_eq!(cell(&terminal, 0, 0).ch, '中');
        assert_eq!(terminal.grid().cursor(), (0, 2));
    }

    #[test]
    fn unsupported_sequences_are_ignored_without_damage() {
        let mut terminal = Terminal::new(10, 2);
        // Mouse reporting, scroll region, character set and a bogus CSI.
        terminal.feed(b"\x1b[?1000h\x1b[1;2r\x1b(B\x1b[zok");
        assert_eq!(terminal_text(&terminal), "ok");
    }

    /// Writes `count` numbered lines the way a program's output does: each
    /// line ends with CR LF, so the screen scrolls as it fills.
    fn write_lines(terminal: &mut Terminal, prefix: &str, count: usize) {
        for index in 0..count {
            terminal.feed(format!("{prefix}{index}\r\n").as_bytes());
        }
    }

    #[test]
    fn full_screen_programs_leave_no_scrollback_behind() {
        let mut terminal = Terminal::new(10, 3);
        write_lines(&mut terminal, "main", 4);
        let history = terminal.grid().scrollback_len();
        assert!(history > 0);

        // What `less` or `vim` does: enter, clear, draw and scroll a lot.
        terminal.feed(b"\x1b[?1049h\x1b[H\x1b[2J");
        assert!(terminal.grid().in_alternate_screen());
        write_lines(&mut terminal, "alt", 50);
        assert_eq!(terminal.grid().scrollback_len(), history);
        // The view cannot scroll back into the main screen's history either.
        terminal.grid_mut().scroll_view(isize::MAX);
        assert_eq!(terminal.grid().display_offset(), 0);
        assert!(!terminal_text(&terminal).contains("main"));

        terminal.feed(b"\x1b[?1049l");
        assert!(!terminal.grid().in_alternate_screen());
        assert_eq!(terminal.grid().scrollback_len(), history);
        assert!(terminal_text(&terminal).contains("main3"));
        assert!(!terminal_text(&terminal).contains("alt"));
    }

    #[test]
    fn mode_1049_restores_the_cursor_and_attributes_on_exit() {
        let mut terminal = Terminal::new(10, 3);
        terminal.feed(b"\x1b[2;4H\x1b[32m");
        terminal.feed(b"\x1b[?1049h\x1b[H\x1b[0mdraw\x1b[31m");
        terminal.feed(b"\x1b[?1049l");
        assert_eq!(terminal.grid().cursor(), (1, 3));
        assert_eq!(terminal.grid().attr().fg, Color::Indexed(2));
        terminal.feed(b"x");
        assert_eq!(cell(&terminal, 1, 3).ch, 'x');
        assert_eq!(cell(&terminal, 1, 3).attr.fg, Color::Indexed(2));
    }

    #[test]
    fn modes_47_and_1047_switch_screens_without_moving_the_cursor() {
        for mode in ["47", "1047"] {
            let mut terminal = Terminal::new(10, 2);
            terminal.feed(b"main");
            terminal.feed(format!("\x1b[?{mode}h\x1b[2;2Halt").as_bytes());
            assert!(terminal.grid().in_alternate_screen(), "mode {mode}");
            assert_eq!(terminal.grid().cursor(), (1, 4), "mode {mode}");
            assert!(!terminal_text(&terminal).contains("main"), "mode {mode}");
            terminal.feed(format!("\x1b[?{mode}l").as_bytes());
            assert!(!terminal.grid().in_alternate_screen(), "mode {mode}");
            assert_eq!(terminal_text(&terminal), "main", "mode {mode}");
            // Without mode 1049 the cursor is not restored; it stays where
            // the alternate screen left it.
            assert_eq!(terminal.grid().cursor(), (1, 4), "mode {mode}");
        }
    }

    #[test]
    fn leaving_without_entering_does_nothing() {
        let mut terminal = Terminal::new(10, 1);
        terminal.feed(b"keep\x1b[?1049l");
        assert_eq!(terminal_text(&terminal), "keep");
        assert_eq!(terminal.grid().cursor(), (0, 4));
    }

    #[test]
    fn resizing_while_on_the_alternate_screen_reflows_the_main_screen() {
        let mut terminal = Terminal::new(10, 2);
        terminal.feed(b"abcdefgh\x1b[?1049h\x1b[Hfull");
        terminal.grid_mut().resize(4, 3);
        assert!(terminal.grid().in_alternate_screen());
        // The alternate screen is blank at the new size.
        assert_eq!(terminal_text(&terminal), "");
        terminal.feed(b"\x1b[?1049l");
        // The main text was narrowed to four columns and is still there.
        assert_eq!(terminal.grid().cols(), 4);
        assert_eq!(terminal_text(&terminal), "abcdefgh");
    }
}
