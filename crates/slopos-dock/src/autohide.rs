//! Auto-hide ("dodge"): the Dock slides out of view when the pointer leaves
//! it and comes back when the pointer reaches the bottom edge. While hidden
//! it reserves no screen space, so maximised windows use the whole screen.
//!
//! This is a pure state machine. The Dock applies each [`Effect`] to its
//! window (geometry, reserved strut and one timer). Nothing polls: the hide
//! is a single timer armed when the pointer leaves, and a reveal happens only
//! when the pointer arrives.

use std::time::Duration;

/// How long the pointer must stay away before the Dock hides.
pub const HIDE_DELAY: Duration = Duration::from_millis(600);

/// The part of the Dock's edge that stays on screen, and catches the pointer,
/// while the Dock is hidden (logical pixels).
pub const REVEAL_STRIP: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Nothing to change.
    None,
    /// Show the whole Dock and reserve its space again.
    Reveal,
    /// Shrink to the reveal strip and release the space.
    Hide,
    /// Deliver `Event::Timer(id)` after [`HIDE_DELAY`]; then call
    /// [`AutoHide::timer`] with that id.
    ScheduleHide(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoHide {
    enabled: bool,
    hidden: bool,
    pointer_inside: bool,
    /// A menu is open on the Dock, so it stays up.
    held: bool,
    /// Bumped whenever a scheduled hide stops being valid. A timer whose id
    /// is not the current generation does nothing.
    generation: u64,
}

impl AutoHide {
    /// With auto-hide on, the Dock starts hidden until the pointer reaches it.
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            hidden: enabled,
            pointer_inside: false,
            held: false,
            generation: 0,
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn hidden(&self) -> bool {
        self.hidden
    }

    /// The pointer is over the Dock or its reveal strip. Repeated motion
    /// inside does nothing.
    pub fn pointer_moved(&mut self) -> Effect {
        if self.pointer_inside {
            return Effect::None;
        }
        self.pointer_inside = true;
        // Whatever hide was pending is no longer wanted.
        self.generation += 1;
        if std::mem::replace(&mut self.hidden, false) {
            Effect::Reveal
        } else {
            Effect::None
        }
    }

    /// The pointer left the Dock.
    pub fn pointer_left(&mut self) -> Effect {
        if !self.pointer_inside {
            return Effect::None;
        }
        self.pointer_inside = false;
        self.schedule()
    }

    /// The timer `id` fired. It hides the Dock only if it is still the
    /// current hide and nothing has since kept the Dock up.
    pub fn timer(&mut self, id: u64) -> Effect {
        if id != self.generation || !self.wants_to_hide() {
            return Effect::None;
        }
        self.hidden = true;
        Effect::Hide
    }

    /// A menu on the Dock opened (`true`) or closed (`false`).
    pub fn hold(&mut self, held: bool) -> Effect {
        self.held = held;
        if held {
            self.generation += 1;
            Effect::None
        } else {
            self.schedule()
        }
    }

    /// Auto-hide was switched on or off.
    pub fn set_enabled(&mut self, enabled: bool) -> Effect {
        self.enabled = enabled;
        self.generation += 1;
        if enabled {
            return self.schedule();
        }
        if std::mem::replace(&mut self.hidden, false) {
            Effect::Reveal
        } else {
            Effect::None
        }
    }

    fn wants_to_hide(&self) -> bool {
        self.enabled && !self.hidden && !self.pointer_inside && !self.held
    }

    fn schedule(&mut self) -> Effect {
        if !self.wants_to_hide() {
            return Effect::None;
        }
        self.generation += 1;
        Effect::ScheduleHide(self.generation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scheduled(effect: Effect) -> u64 {
        match effect {
            Effect::ScheduleHide(id) => id,
            other => panic!("expected a scheduled hide, got {other:?}"),
        }
    }

    #[test]
    fn with_auto_hide_on_the_dock_starts_hidden_and_the_pointer_reveals_it() {
        let mut dock = AutoHide::new(true);
        assert!(dock.hidden());
        assert_eq!(dock.pointer_moved(), Effect::Reveal);
        assert!(!dock.hidden());
        // More motion inside is not a second reveal.
        assert_eq!(dock.pointer_moved(), Effect::None);
    }

    #[test]
    fn leaving_arms_one_timer_and_its_firing_hides_the_dock() {
        let mut dock = AutoHide::new(true);
        dock.pointer_moved();
        let id = scheduled(dock.pointer_left());
        assert_eq!(dock.timer(id), Effect::Hide);
        assert!(dock.hidden());
        // A second firing of the same timer does nothing more.
        assert_eq!(dock.timer(id), Effect::None);
    }

    #[test]
    fn coming_back_cancels_a_pending_hide() {
        let mut dock = AutoHide::new(true);
        dock.pointer_moved();
        let id = scheduled(dock.pointer_left());
        assert_eq!(dock.pointer_moved(), Effect::None);
        assert_eq!(dock.timer(id), Effect::None);
        assert!(!dock.hidden());
    }

    #[test]
    fn a_timer_from_an_earlier_leave_cannot_hide_after_a_later_one() {
        let mut dock = AutoHide::new(true);
        dock.pointer_moved();
        let first = scheduled(dock.pointer_left());
        dock.pointer_moved();
        let second = scheduled(dock.pointer_left());
        assert_ne!(first, second);
        assert_eq!(dock.timer(first), Effect::None);
        assert!(!dock.hidden());
        assert_eq!(dock.timer(second), Effect::Hide);
    }

    #[test]
    fn an_open_menu_keeps_the_dock_up_until_it_closes() {
        let mut dock = AutoHide::new(true);
        dock.pointer_moved();
        assert_eq!(dock.hold(true), Effect::None);
        assert_eq!(dock.pointer_left(), Effect::None);
        assert!(!dock.hidden());
        let id = scheduled(dock.hold(false));
        assert_eq!(dock.timer(id), Effect::Hide);
    }

    #[test]
    fn a_dock_without_auto_hide_never_hides() {
        let mut dock = AutoHide::new(false);
        assert!(!dock.hidden());
        assert_eq!(dock.pointer_moved(), Effect::None);
        assert_eq!(dock.pointer_left(), Effect::None);
        assert!(!dock.hidden());
    }

    #[test]
    fn switching_auto_hide_off_reveals_and_switching_it_on_hides_when_the_pointer_is_away() {
        let mut dock = AutoHide::new(true);
        assert_eq!(dock.set_enabled(false), Effect::Reveal);
        assert!(!dock.hidden());
        let id = scheduled(dock.set_enabled(true));
        assert_eq!(dock.timer(id), Effect::Hide);
        assert!(dock.hidden());
    }

    #[test]
    fn arriving_and_leaving_again_schedules_the_next_hide() {
        let mut dock = AutoHide::new(true);
        assert_eq!(dock.pointer_moved(), Effect::Reveal);
        let id = scheduled(dock.pointer_left());
        assert_eq!(dock.timer(id), Effect::Hide);
        assert!(dock.hidden());
    }
}
