#![allow(unused_imports, clippy::collapsible_match, clippy::unnecessary_cast)]

use crate::terminal::Terminal;
use vte::{Params, Perform};

pub struct VtHandler<'a> {
    pub term: &'a mut Terminal,
}

impl<'a> Perform for VtHandler<'a> {
    fn print(&mut self, c: char) {
        self.term.print_char(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x0a => {
                // LF: within the DECSTBM region [scroll_top, scroll_bottom],
                // reaching the bottom margin scrolls only that region. Previously
                // this checked against the full screen height, so a region never
                // scrolled at its own bottom edge (DECSTBM was parsed but dead).
                // Outside the region, LF just moves the cursor down, stopping at
                // the last row of the screen without scrolling.
                let top = self.term.scroll_top;
                let bottom = self
                    .term
                    .scroll_bottom
                    .min(self.term.rows.saturating_sub(1));
                if self.term.cursor_y == bottom && self.term.cursor_y >= top {
                    self.term.scroll_up();
                } else if self.term.cursor_y + 1 < self.term.rows {
                    self.term.cursor_y += 1;
                }
            }
            0x0d => {
                // CR
                self.term.cursor_x = 0;
            }
            0x08 if self.term.cursor_x > 0 => {
                // BS
                self.term.cursor_x -= 1;
            }
            0x09 => {
                // HT: advance to the next 8-column tab stop, clamped to the last column.
                if self.term.cols > 0 {
                    let next_stop = (self.term.cursor_x / 8 + 1) * 8;
                    self.term.cursor_x = next_stop.min(self.term.cols - 1);
                }
            }
            _ => {}
        }
    }

    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {}
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}
    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        // OSC 0 ; title BEL  — set icon name and window title
        // OSC 2 ; title BEL  — set window title only
        if params.len() >= 2 {
            let ps = params[0];
            if ps == b"0" || ps == b"2" {
                if let Ok(title) = std::str::from_utf8(params[1]) {
                    self.term.set_window_title(title.to_string());
                }
            }
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        // Handle DEC private modes (CSI ? Ps h/l)
        if intermediates.contains(&b'?') {
            let mode: u16 = params
                .iter()
                .next()
                .and_then(|p| p.first())
                .copied()
                .unwrap_or(0);
            match action {
                'h' => match mode {
                    1000 => {
                        // Enable basic mouse button reporting
                        self.term.mouse_reporting = true;
                    }
                    1049 => {
                        // Enter alternate screen
                        self.term.enter_alt_screen();
                    }
                    _ => {}
                },
                'l' => match mode {
                    1000 => {
                        // Disable mouse reporting
                        self.term.mouse_reporting = false;
                    }
                    1049 => {
                        // Leave alternate screen
                        self.term.leave_alt_screen();
                    }
                    _ => {}
                },
                _ => {}
            }
            return;
        }

        match action {
            'm' => {
                let flat: Vec<u16> = params.iter().flat_map(|p| p.iter()).copied().collect();
                let mut i = 0;
                while i < flat.len() {
                    match flat[i] {
                        0 => {
                            self.term.current_fg = slopos_kit::Color::WHITE;
                            self.term.current_bg = slopos_kit::Color::BLACK;
                            self.term.current_bold = false;
                            self.term.current_italic = false;
                            self.term.current_underline = false;
                        }
                        1 => {
                            self.term.current_bold = true;
                        }
                        3 => {
                            self.term.current_italic = true;
                        }
                        4 => {
                            self.term.current_underline = true;
                        }
                        30..=37 => {
                            self.term.current_fg = map_ansi_color(flat[i] - 30);
                        }
                        38 => {
                            if i + 2 < flat.len() && flat[i + 1] == 5 {
                                self.term.current_fg = map_256_color(flat[i + 2]);
                                i += 2;
                            } else if i + 4 < flat.len() && flat[i + 1] == 2 {
                                let r = flat[i + 2] as f32 / 255.0;
                                let g = flat[i + 3] as f32 / 255.0;
                                let b = flat[i + 4] as f32 / 255.0;
                                self.term.current_fg = slopos_kit::Color::new(r, g, b, 1.0);
                                i += 4;
                            }
                        }
                        40..=47 => {
                            self.term.current_bg = map_ansi_color(flat[i] - 40);
                        }
                        48 => {
                            if i + 2 < flat.len() && flat[i + 1] == 5 {
                                self.term.current_bg = map_256_color(flat[i + 2]);
                                i += 2;
                            } else if i + 4 < flat.len() && flat[i + 1] == 2 {
                                let r = flat[i + 2] as f32 / 255.0;
                                let g = flat[i + 3] as f32 / 255.0;
                                let b = flat[i + 4] as f32 / 255.0;
                                self.term.current_bg = slopos_kit::Color::new(r, g, b, 1.0);
                                i += 4;
                            }
                        }
                        _ => {}
                    }
                    i += 1;
                }
            }
            'A' => {
                // CUU — cursor up Ps lines (default 1); stops at the top of the screen.
                let n = first_param_or(params, 1) as usize;
                self.term.cursor_y = self.term.cursor_y.saturating_sub(n);
            }
            'B' => {
                // CUD — cursor down Ps lines (default 1); stops at the bottom of the screen.
                let n = first_param_or(params, 1) as usize;
                let max_row = self.term.rows.saturating_sub(1);
                self.term.cursor_y = self.term.cursor_y.saturating_add(n).min(max_row);
            }
            'C' => {
                // CUF — cursor forward Ps columns (default 1); stops at the right edge.
                let n = first_param_or(params, 1) as usize;
                let max_col = self.term.cols.saturating_sub(1);
                self.term.cursor_x = self.term.cursor_x.saturating_add(n).min(max_col);
            }
            'D' => {
                // CUB — cursor back Ps columns (default 1); stops at the left edge.
                let n = first_param_or(params, 1) as usize;
                self.term.cursor_x = self.term.cursor_x.saturating_sub(n);
            }
            'G' => {
                // CHA — cursor horizontal absolute: move to column Ps (default 1), 1-based.
                let col = first_param_or(params, 1) as usize;
                let max_col = self.term.cols.saturating_sub(1);
                self.term.cursor_x = col.saturating_sub(1).min(max_col);
            }
            'd' => {
                // VPA — vertical line position absolute: move to row Ps (default 1), 1-based.
                let row = first_param_or(params, 1) as usize;
                let max_row = self.term.rows.saturating_sub(1);
                self.term.cursor_y = row.saturating_sub(1).min(max_row);
            }
            'H' | 'f' => {
                let mut row = 1;
                let mut col = 1;
                let mut iter = params.iter();
                if let Some(r_param) = iter.next() {
                    if let Some(r) = r_param.first() {
                        row = *r;
                    }
                }
                if let Some(c_param) = iter.next() {
                    if let Some(c) = c_param.first() {
                        col = *c;
                    }
                }
                self.term.cursor_y = (row as usize)
                    .saturating_sub(1)
                    .min(self.term.rows.saturating_sub(1));
                self.term.cursor_x = (col as usize)
                    .saturating_sub(1)
                    .min(self.term.cols.saturating_sub(1));
            }
            'J' => {
                // ED — erase in display. 0 = cursor to end (default), 1 = start to
                // cursor (inclusive), 2 = whole screen (existing behaviour, which
                // also homes the cursor and is relied on by callers/tests).
                let mut val = 0;
                if let Some(p) = params.iter().next().and_then(|p| p.first()) {
                    val = *p;
                }
                let cols = self.term.cols;
                let cursor_idx = self.term.cursor_y.saturating_mul(cols) + self.term.cursor_x;
                match val {
                    0 => {
                        let start = cursor_idx.min(self.term.grid.len());
                        for cell in self.term.grid[start..].iter_mut() {
                            *cell = crate::terminal::Cell::default();
                        }
                    }
                    1 => {
                        let end = cursor_idx.saturating_add(1).min(self.term.grid.len());
                        for cell in self.term.grid[..end].iter_mut() {
                            *cell = crate::terminal::Cell::default();
                        }
                    }
                    2 => {
                        self.term.grid.fill(crate::terminal::Cell::default());
                        self.term.cursor_x = 0;
                        self.term.cursor_y = 0;
                    }
                    _ => {}
                }
            }
            'K' => {
                let mut mode = 0;
                if let Some(p) = params.iter().next().and_then(|p| p.first()) {
                    mode = *p;
                }
                let row = self.term.cursor_y;
                let cols = self.term.cols;
                match mode {
                    0 => {
                        for col in self.term.cursor_x..cols {
                            let idx = row * cols + col;
                            if idx < self.term.grid.len() {
                                self.term.grid[idx] = crate::terminal::Cell::default();
                            }
                        }
                    }
                    1 => {
                        let last = self.term.cursor_x.min(cols.saturating_sub(1));
                        for col in 0..=last {
                            let idx = row * cols + col;
                            if idx < self.term.grid.len() {
                                self.term.grid[idx] = crate::terminal::Cell::default();
                            }
                        }
                    }
                    2 => {
                        for col in 0..cols {
                            let idx = row * cols + col;
                            if idx < self.term.grid.len() {
                                self.term.grid[idx] = crate::terminal::Cell::default();
                            }
                        }
                    }
                    _ => {}
                }
            }
            'r' => {
                let mut top: usize = 1;
                let mut bottom: usize = self.term.rows;
                let mut iter = params.iter();
                if let Some(t_param) = iter.next() {
                    if let Some(t) = t_param.first() {
                        top = *t as usize;
                    }
                }
                if let Some(b_param) = iter.next() {
                    if let Some(b) = b_param.first() {
                        bottom = *b as usize;
                    }
                }
                let max_row = self.term.rows.saturating_sub(1);
                self.term.scroll_top = (top as usize).saturating_sub(1).min(max_row);
                self.term.scroll_bottom = (bottom as usize).saturating_sub(1).min(max_row);
            }
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
}

/// First CSI parameter, defaulting to `default` when absent *or* given
/// explicitly as 0 (ECMA-48 5.4.2: an explicit 0 means "use the default").
fn first_param_or(params: &Params, default: u16) -> u16 {
    match params.iter().next().and_then(|p| p.first()).copied() {
        None | Some(0) => default,
        Some(n) => n,
    }
}

fn map_ansi_color(code: u16) -> slopos_kit::Color {
    match code {
        0 => slopos_kit::Color::BLACK,
        1 => slopos_kit::Color::new(0.8, 0.0, 0.0, 1.0),
        2 => slopos_kit::Color::new(0.0, 0.8, 0.0, 1.0),
        3 => slopos_kit::Color::new(0.8, 0.8, 0.0, 1.0),
        4 => slopos_kit::Color::new(0.0, 0.0, 0.8, 1.0),
        5 => slopos_kit::Color::new(0.8, 0.0, 0.8, 1.0),
        6 => slopos_kit::Color::new(0.0, 0.8, 0.8, 1.0),
        _ => slopos_kit::Color::WHITE,
    }
}

fn map_256_color(idx: u16) -> slopos_kit::Color {
    if idx < 8 {
        map_ansi_color(idx)
    } else if idx < 16 {
        match idx {
            8 => slopos_kit::Color::new(0.3, 0.3, 0.3, 1.0),
            9 => slopos_kit::Color::new(1.0, 0.3, 0.3, 1.0),
            10 => slopos_kit::Color::new(0.3, 1.0, 0.3, 1.0),
            11 => slopos_kit::Color::new(1.0, 1.0, 0.3, 1.0),
            12 => slopos_kit::Color::new(0.3, 0.3, 1.0, 1.0),
            13 => slopos_kit::Color::new(1.0, 0.3, 1.0, 1.0),
            14 => slopos_kit::Color::new(0.3, 1.0, 1.0, 1.0),
            _ => slopos_kit::Color::WHITE,
        }
    } else if idx < 232 {
        let cube_idx = idx - 16;
        let r = (cube_idx / 36) % 6;
        let g = (cube_idx / 6) % 6;
        let b = cube_idx % 6;
        slopos_kit::Color::new(r as f32 / 5.0, g as f32 / 5.0, b as f32 / 5.0, 1.0)
    } else {
        let gray = (idx - 232) as f32 / 23.0;
        slopos_kit::Color::new(gray, gray, gray, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::Terminal;

    /// Feed `ESC [ seq` byte-by-byte, where `seq` includes any parameters
    /// and the final action byte (e.g. "5A", "3;9H", "?1049h").
    fn write_csi(term: &mut Terminal, seq: &str) {
        term.write_byte(0x1b);
        term.write_byte(b'[');
        for b in seq.bytes() {
            term.write_byte(b);
        }
    }

    #[test]
    fn cuu_cud_cuf_cub_move_and_clamp() {
        let mut term = Terminal::new(10, 10);
        term.cursor_x = 5;
        term.cursor_y = 5;

        write_csi(&mut term, "2A"); // CUU 2
        assert_eq!(term.cursor_y, 3);

        write_csi(&mut term, "B"); // CUD, default Ps = 1
        assert_eq!(term.cursor_y, 4);

        write_csi(&mut term, "3C"); // CUF 3
        assert_eq!(term.cursor_x, 8);

        write_csi(&mut term, "D"); // CUB, default Ps = 1
        assert_eq!(term.cursor_x, 7);

        // Clamp at the top edge instead of underflowing.
        term.cursor_y = 1;
        write_csi(&mut term, "5A");
        assert_eq!(term.cursor_y, 0);

        // Clamp at the bottom edge instead of running off the grid.
        term.cursor_y = 8;
        write_csi(&mut term, "20B");
        assert_eq!(term.cursor_y, 9);

        // An explicit 0 count means "1", not "don't move" (ECMA-48 5.4.2).
        term.cursor_x = 5;
        write_csi(&mut term, "0C");
        assert_eq!(term.cursor_x, 6);
    }

    #[test]
    fn cha_and_vpa_move_to_absolute_position_and_clamp() {
        let mut term = Terminal::new(10, 10);

        write_csi(&mut term, "5G"); // CHA column 5 (1-based) -> index 4
        assert_eq!(term.cursor_x, 4);

        write_csi(&mut term, "8d"); // VPA row 8 (1-based) -> index 7
        assert_eq!(term.cursor_y, 7);

        // Out-of-range column/row clamps to the last cell instead of panicking.
        write_csi(&mut term, "999G");
        assert_eq!(term.cursor_x, 9);
        write_csi(&mut term, "999d");
        assert_eq!(term.cursor_y, 9);
    }

    #[test]
    fn cup_moves_cursor_to_row_and_column_and_clamps() {
        let mut term = Terminal::new(10, 10);
        write_csi(&mut term, "3;5H");
        assert_eq!(term.cursor_y, 2);
        assert_eq!(term.cursor_x, 4);

        write_csi(&mut term, "H"); // no params -> home (1, 1)
        assert_eq!(term.cursor_y, 0);
        assert_eq!(term.cursor_x, 0);

        write_csi(&mut term, "999;999f");
        assert_eq!(term.cursor_y, 9);
        assert_eq!(term.cursor_x, 9);
    }

    #[test]
    fn ht_advances_to_next_eight_column_tab_stop() {
        let mut term = Terminal::new(40, 5);
        assert_eq!(term.cursor_x, 0);

        term.write_byte(0x09);
        assert_eq!(term.cursor_x, 8);

        term.write_byte(0x09);
        assert_eq!(term.cursor_x, 16);

        term.cursor_x = 17;
        term.write_byte(0x09);
        assert_eq!(term.cursor_x, 24);

        // Clamp at the last column instead of overflowing past it.
        term.cursor_x = 39;
        term.write_byte(0x09);
        assert_eq!(term.cursor_x, 39);
    }

    #[test]
    fn ed_0_clears_from_cursor_to_end_of_screen_and_leaves_cursor() {
        let mut term = Terminal::new(4, 2);
        for c in "abcdefgh".chars() {
            term.print_char(c);
        }
        term.cursor_x = 2;
        term.cursor_y = 0;

        write_csi(&mut term, "0J");
        assert_eq!(term.grid[0].c, 'a');
        assert_eq!(term.grid[1].c, 'b');
        assert_eq!(term.grid[2].c, ' '); // cleared (cursor cell)
        assert_eq!(term.grid[3].c, ' '); // cleared
        assert_eq!(term.grid[4].c, ' '); // cleared (whole next row erased too)
        assert_eq!(term.grid[7].c, ' '); // cleared
                                         // ED does not move the cursor.
        assert_eq!(term.cursor_x, 2);
        assert_eq!(term.cursor_y, 0);
    }

    #[test]
    fn ed_1_clears_from_start_of_screen_to_cursor_inclusive() {
        let mut term = Terminal::new(4, 1);
        for c in "abcd".chars() {
            term.print_char(c);
        }
        term.cursor_x = 2;
        term.cursor_y = 0;

        write_csi(&mut term, "1J");
        assert_eq!(term.grid[0].c, ' ');
        assert_eq!(term.grid[1].c, ' ');
        assert_eq!(term.grid[2].c, ' '); // inclusive of the cursor cell
        assert_eq!(term.grid[3].c, 'd'); // untouched
    }

    #[test]
    fn decstbm_scroll_region_scrolls_at_its_own_bottom_margin() {
        // 5-row screen, region set to rows 2..=4 (1-based) i.e. index 1..=3.
        let mut term = Terminal::new(4, 5);
        write_csi(&mut term, "2;4r");
        assert_eq!(term.scroll_top, 1);
        assert_eq!(term.scroll_bottom, 3);

        // Mark each row so a scroll within the region is observable.
        for row in 0..5 {
            term.grid[row * term.cols].c = (b'0' + row as u8) as char;
        }

        // Cursor at the bottom of the region: LF must scroll only [1, 3].
        term.cursor_y = 3;
        term.write_byte(0x0a);

        assert_eq!(term.grid[0].c, '0'); // above region: untouched
        assert_eq!(term.grid[term.cols].c, '2'); // region shifted up
        assert_eq!(term.grid[2 * term.cols].c, '3');
        assert_eq!(term.grid[3 * term.cols].c, ' '); // new blank line at the margin
        assert_eq!(term.grid[4 * term.cols].c, '4'); // below region: untouched
                                                     // Cursor stays pinned to the bottom margin, not the screen bottom.
        assert_eq!(term.cursor_y, 3);
    }

    #[test]
    fn lf_below_scroll_region_moves_cursor_without_scrolling() {
        let mut term = Terminal::new(4, 5);
        write_csi(&mut term, "2;4r"); // region index 1..=3

        for row in 0..5 {
            term.grid[row * term.cols].c = (b'0' + row as u8) as char;
        }

        // Cursor sits below the region, already at the last row of the screen.
        term.cursor_y = 4;
        term.write_byte(0x0a);

        assert_eq!(term.cursor_y, 4); // no scroll happened, so nothing to clamp
        assert_eq!(term.grid[0].c, '0');
        assert_eq!(term.grid[term.cols].c, '1');
        assert_eq!(term.grid[4 * term.cols].c, '4');
    }

    #[test]
    fn decstbm_with_inverted_or_out_of_range_margins_is_safe() {
        let mut term = Terminal::new(4, 5);
        // top (9) is past the screen and greater than bottom (2): must clamp
        // into range without panicking, and must not let a later scroll
        // corrupt the grid.
        write_csi(&mut term, "9;2r");
        assert_eq!(term.scroll_top, 4);
        assert_eq!(term.scroll_bottom, 1);

        for row in 0..5 {
            term.grid[row * term.cols].c = (b'0' + row as u8) as char;
        }

        term.cursor_y = 1;
        term.write_byte(0x0a);
        // scroll_up() only scrolls when scroll_top < scroll_bottom, so an
        // inverted region degrades to a plain cursor-down with no corruption.
        assert_eq!(term.cursor_y, 2);
        for row in 0..5 {
            assert_eq!(term.grid[row * term.cols].c, (b'0' + row as u8) as char);
        }
    }
}
