//! Counting clicks in a run: a double click selects a word, a triple click a
//! line. This is plain logic over the time and cell of each press, so it can
//! be tested without a window. It reads the clock only when a press arrives;
//! nothing here waits or polls.

use std::time::{Duration, Instant};

/// Presses this close together, on the same cell, continue a run.
pub const MULTI_CLICK_INTERVAL: Duration = Duration::from_millis(500);

/// The last primary-button press on a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Click {
    pub at: Instant,
    /// `(row, column)` in the viewport.
    pub cell: (usize, usize),
    /// Presses in the run so far, 1 to 3.
    pub count: u8,
}

/// The number of the press at `cell` at time `now`, given the previous press.
/// The count rises 1, 2, 3 and then stays at 3, so a fourth quick click
/// selects the line again rather than starting over at one.
pub fn next_count(previous: Option<Click>, now: Instant, cell: (usize, usize)) -> u8 {
    match previous {
        Some(last)
            if last.cell == cell
                && now.saturating_duration_since(last.at) <= MULTI_CLICK_INTERVAL =>
        {
            last.count.min(2) + 1
        }
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(previous: Option<Click>, now: Instant, cell: (usize, usize)) -> Click {
        Click {
            at: now,
            cell,
            count: next_count(previous, now, cell),
        }
    }

    #[test]
    fn quick_presses_on_one_cell_count_up_to_three() {
        let t0 = Instant::now();
        let first = press(None, t0, (2, 5));
        assert_eq!(first.count, 1);
        let second = press(Some(first), t0 + Duration::from_millis(100), (2, 5));
        assert_eq!(second.count, 2);
        let third = press(Some(second), t0 + Duration::from_millis(200), (2, 5));
        assert_eq!(third.count, 3);
        let fourth = press(Some(third), t0 + Duration::from_millis(300), (2, 5));
        assert_eq!(fourth.count, 3);
    }

    #[test]
    fn a_press_on_another_cell_starts_a_new_run() {
        let t0 = Instant::now();
        let first = press(None, t0, (0, 0));
        assert_eq!(next_count(Some(first), t0, (0, 1)), 1);
    }

    #[test]
    fn a_press_after_the_interval_starts_a_new_run() {
        let t0 = Instant::now();
        let first = press(None, t0, (1, 1));
        let late = t0 + MULTI_CLICK_INTERVAL + Duration::from_millis(1);
        assert_eq!(next_count(Some(first), late, (1, 1)), 1);
        // Exactly at the interval it still counts.
        let on_time = t0 + MULTI_CLICK_INTERVAL;
        assert_eq!(next_count(Some(first), on_time, (1, 1)), 2);
    }
}
