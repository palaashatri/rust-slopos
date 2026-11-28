//! The notification history: the notices the bar has received, newest first, for
//! the history popover. It is kept in memory and bounded, and nothing is written
//! to disk. It is a record, not the toast queue: a notice leaves the toasts when
//! it times out or is dismissed, and stays here until it is cleared or pushed out
//! by newer notices. Plain logic, tested without X.

use std::collections::VecDeque;

use crate::notifications::Toast;

/// The most notices kept. The oldest is dropped when a newer one arrives.
pub const LIMIT: usize = 50;
/// The longest text kept per field, in characters. A longer one is cut with an
/// ellipsis, so one notice cannot hold much memory.
const FIELD_CHARS: usize = 512;

#[derive(Debug)]
pub struct History {
    entries: VecDeque<Toast>,
    limit: usize,
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

impl History {
    pub fn new() -> Self {
        Self::with_limit(LIMIT)
    }

    /// A history that keeps at most `limit` notices (at least one).
    pub fn with_limit(limit: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            limit: limit.max(1),
        }
    }

    /// Records a notice as the newest. A notice with the id of one already kept
    /// replaces it and moves to the front, as a replacing notification does.
    pub fn record(&mut self, mut toast: Toast) {
        toast.app = clip(&toast.app);
        toast.summary = clip(&toast.summary);
        toast.body = clip(&toast.body);
        // The history shows what was said; the actions were for the toast.
        toast.actions.clear();
        self.entries.retain(|entry| entry.id != toast.id);
        self.entries.push_front(toast);
        while self.entries.len() > self.limit {
            self.entries.pop_back();
        }
    }

    /// Removes every notice from the history.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// The notices, newest first.
    pub fn iter(&self) -> impl Iterator<Item = &Toast> {
        self.entries.iter()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// `text` cut to `FIELD_CHARS` characters, with an ellipsis when it was cut.
fn clip(text: &str) -> String {
    if text.chars().count() <= FIELD_CHARS {
        return text.to_owned();
    }
    let head: String = text.chars().take(FIELD_CHARS - 1).collect();
    format!("{head}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toast(id: u32, summary: &str) -> Toast {
        Toast {
            id,
            app: "Mail".into(),
            summary: summary.into(),
            body: String::new(),
            actions: vec![("default".into(), "Open".into())],
            timeout_ms: Some(1000),
        }
    }

    fn ids(history: &History) -> Vec<u32> {
        history.iter().map(|toast| toast.id).collect()
    }

    #[test]
    fn the_newest_notice_comes_first_and_the_oldest_is_dropped_at_the_limit() {
        let mut history = History::with_limit(3);
        for id in 1..=5 {
            history.record(toast(id, "hello"));
        }
        assert_eq!(history.len(), 3);
        assert_eq!(ids(&history), [5, 4, 3]);
    }

    #[test]
    fn the_default_limit_is_fifty() {
        let mut history = History::new();
        for id in 1..=(LIMIT as u32 + 10) {
            history.record(toast(id, "x"));
        }
        assert_eq!(history.len(), LIMIT);
        assert_eq!(history.iter().next().map(|t| t.id), Some(LIMIT as u32 + 10));
        assert_eq!(history.iter().last().map(|t| t.id), Some(11));
    }

    #[test]
    fn a_replacing_notice_takes_the_place_of_the_one_it_replaces() {
        let mut history = History::with_limit(5);
        history.record(toast(1, "first"));
        history.record(toast(2, "second"));
        history.record(toast(3, "third"));
        history.record(toast(1, "first, updated"));
        assert_eq!(ids(&history), [1, 3, 2]);
        assert_eq!(
            history.iter().next().map(|t| t.summary.as_str()),
            Some("first, updated")
        );
    }

    #[test]
    fn clear_all_empties_the_history() {
        let mut history = History::with_limit(5);
        history.record(toast(1, "a"));
        history.record(toast(2, "b"));
        history.clear();
        assert!(history.is_empty());
        assert_eq!(history.len(), 0);
        history.record(toast(3, "c"));
        assert_eq!(ids(&history), [3]);
    }

    #[test]
    fn a_huge_notice_is_cut_and_its_actions_are_not_kept() {
        let mut history = History::with_limit(2);
        let mut big = toast(1, "");
        big.body = "a".repeat(100_000);
        history.record(big);
        let kept = history.iter().next().expect("the notice is kept");
        assert_eq!(kept.body.chars().count(), FIELD_CHARS);
        assert!(kept.body.ends_with('…'));
        assert!(kept.actions.is_empty());
        assert_eq!(kept.app, "Mail", "a short field is kept whole");
    }

    #[test]
    fn a_zero_limit_still_keeps_the_newest_notice() {
        let mut history = History::with_limit(0);
        history.record(toast(1, "a"));
        history.record(toast(2, "b"));
        assert_eq!(ids(&history), [2]);
    }
}
