//! Counting clicks in a run: a double click selects a word, a triple click a
//! line. This is plain logic over the time and cell of each press, so it can
//! be tested without a window. Times are the server's press timestamps in
//! milliseconds, so a busy window does not merge two separate presses.
//! Nothing here waits or polls.

/// Presses this close together, on the same cell, continue a run.
pub const MULTI_CLICK_INTERVAL_MS: u32 = 500;

/// The last primary-button press on a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Click {
    /// The server's timestamp of the press, in milliseconds (it wraps).
    pub at: u32,
    /// `(row, column)` in the viewport.
    pub cell: (usize, usize),
    /// Presses in the run so far, 1 to 3.
    pub count: u8,
}

/// The number of the press at `cell` at time `now`, given the previous press.
/// The count rises 1, 2, 3 and then stays at 3, so a fourth quick click
/// selects the line again rather than starting over at one.
pub fn next_count(previous: Option<Click>, now: u32, cell: (usize, usize)) -> u8 {
    match previous {
        // The server's clock wraps; the difference is taken modulo 2^32.
        Some(last) if last.cell == cell && now.wrapping_sub(last.at) <= MULTI_CLICK_INTERVAL_MS => {
            last.count.min(2) + 1
        }
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(previous: Option<Click>, now: u32, cell: (usize, usize)) -> Click {
        Click {
            at: now,
            cell,
            count: next_count(previous, now, cell),
        }
    }

    #[test]
    fn quick_presses_on_one_cell_count_up_to_three() {
        let first = press(None, 1_000, (2, 5));
        assert_eq!(first.count, 1);
        let second = press(Some(first), 1_100, (2, 5));
        assert_eq!(second.count, 2);
        let third = press(Some(second), 1_200, (2, 5));
        assert_eq!(third.count, 3);
        let fourth = press(Some(third), 1_300, (2, 5));
        assert_eq!(fourth.count, 3);
    }

    #[test]
    fn a_press_on_another_cell_starts_a_new_run() {
        let first = press(None, 0, (0, 0));
        assert_eq!(next_count(Some(first), 0, (0, 1)), 1);
    }

    #[test]
    fn a_press_after_the_interval_starts_a_new_run() {
        let first = press(None, 10_000, (1, 1));
        assert_eq!(next_count(Some(first), 10_501, (1, 1)), 1);
        // Exactly at the interval it still counts.
        assert_eq!(next_count(Some(first), 10_500, (1, 1)), 2);
    }

    #[test]
    fn the_server_clock_may_wrap_inside_a_run() {
        let first = press(None, u32::MAX - 100, (3, 3));
        assert_eq!(next_count(Some(first), 300, (3, 3)), 2);
    }

    #[test]
    fn presses_that_reached_the_app_late_still_count_by_their_own_time() {
        // Two presses 600 ms apart on the server stay two runs, even if the app
        // handled both at the same moment.
        let first = press(None, 0, (0, 0));
        assert_eq!(next_count(Some(first), 600, (0, 0)), 1);
    }
}
