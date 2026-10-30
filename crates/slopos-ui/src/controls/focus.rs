//! Keyboard focus order for one window.
//!
//! A [`FocusChain`] holds the accessibility ids of the focusable controls in
//! tab order. Tab and Shift+Tab move through them and wrap; the app asks
//! [`FocusChain::focused`] which control should draw its focus ring and
//! receive keys.

use crate::event::{Event, Key};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FocusChain {
    ids: Vec<u64>,
    current: Option<usize>,
}

impl FocusChain {
    pub fn new(ids: Vec<u64>) -> Self {
        Self { ids, current: None }
    }

    /// Replace the focusable ids, keeping focus on the same id if it is
    /// still present.
    pub fn set_ids(&mut self, ids: Vec<u64>) {
        let focused = self.focused();
        self.ids = ids;
        self.current = focused.and_then(|id| self.ids.iter().position(|&other| other == id));
    }

    pub fn ids(&self) -> &[u64] {
        &self.ids
    }

    pub fn focused(&self) -> Option<u64> {
        self.current.and_then(|index| self.ids.get(index).copied())
    }

    pub fn is_focused(&self, id: u64) -> bool {
        self.focused() == Some(id)
    }

    /// Focus `id`; returns false when it is not in the chain.
    pub fn focus(&mut self, id: u64) -> bool {
        match self.ids.iter().position(|&other| other == id) {
            Some(index) => {
                self.current = Some(index);
                true
            }
            None => false,
        }
    }

    pub fn clear(&mut self) {
        self.current = None;
    }

    pub fn focus_next(&mut self) -> Option<u64> {
        if self.ids.is_empty() {
            return None;
        }
        self.current = Some(match self.current {
            Some(index) => (index + 1) % self.ids.len(),
            None => 0,
        });
        self.focused()
    }

    pub fn focus_previous(&mut self) -> Option<u64> {
        if self.ids.is_empty() {
            return None;
        }
        self.current = Some(match self.current {
            Some(0) | None => self.ids.len() - 1,
            Some(index) => index - 1,
        });
        self.focused()
    }

    /// Handle Tab / Shift+Tab. Returns the newly focused id, or `None` when
    /// the event was not a focus-traversal key (or the chain is empty).
    pub fn handle(&mut self, event: &Event) -> Option<u64> {
        match event {
            Event::KeyPressed {
                key: Key::Tab,
                modifiers,
            } if !modifiers.control && !modifiers.alt => {
                if modifiers.shift {
                    self.focus_previous()
                } else {
                    self.focus_next()
                }
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Modifiers;

    fn tab(shift: bool) -> Event {
        Event::KeyPressed {
            key: Key::Tab,
            modifiers: Modifiers {
                shift,
                ..Modifiers::default()
            },
        }
    }

    #[test]
    fn tab_and_shift_tab_wrap_in_both_directions() {
        let mut chain = FocusChain::new(vec![10, 20, 30]);
        assert_eq!(chain.focused(), None);
        assert_eq!(chain.handle(&tab(false)), Some(10));
        assert_eq!(chain.handle(&tab(false)), Some(20));
        assert_eq!(chain.handle(&tab(false)), Some(30));
        assert_eq!(chain.handle(&tab(false)), Some(10));
        assert_eq!(chain.handle(&tab(true)), Some(30));
        let mut fresh = FocusChain::new(vec![10, 20]);
        assert_eq!(fresh.handle(&tab(true)), Some(20));
    }

    #[test]
    fn other_keys_and_empty_chains_do_not_move_focus() {
        let mut chain = FocusChain::new(vec![]);
        assert_eq!(chain.handle(&tab(false)), None);
        let mut chain = FocusChain::new(vec![1, 2]);
        chain.focus(2);
        let ctrl_tab = Event::KeyPressed {
            key: Key::Tab,
            modifiers: Modifiers {
                control: true,
                ..Modifiers::default()
            },
        };
        assert_eq!(chain.handle(&ctrl_tab), None);
        assert_eq!(
            chain.handle(&Event::KeyPressed {
                key: Key::Enter,
                modifiers: Modifiers::default()
            }),
            None
        );
        assert_eq!(chain.focused(), Some(2));
    }

    #[test]
    fn replacing_ids_keeps_or_drops_focus() {
        let mut chain = FocusChain::new(vec![1, 2, 3]);
        assert!(chain.focus(2));
        assert!(!chain.focus(9));
        chain.set_ids(vec![3, 2]);
        assert_eq!(chain.focused(), Some(2));
        chain.set_ids(vec![3]);
        assert_eq!(chain.focused(), None);
    }
}
