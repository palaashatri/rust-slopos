//! The lock state machine. It has no X, D-Bus or PAM in it: callers feed
//! [`Input`]s with the current time and carry out the returned [`Effect`]s.
//!
//! ```text
//!            Lock                 Submit            Checked(Accepted)
//! Unlocked ---------> Locked ----------> Checking ------------------> Unlocked
//!                       ^  ^  Checked(Rejected) |  Unlock (logind)      ^
//!                       |  |  ----------------->|  from any locked state |
//!                       |  |  (3rd failure)     v                       |
//!                       |  +----------------- Delayed { until } ---------+
//!                       |      Tick(now >= until)
//!                       +-- Checked(Unavailable) (no failure counted)
//! ```

use std::time::Duration;

/// Failed attempts before the screen waits between attempts.
pub const FAILURES_BEFORE_DELAY: u32 = 3;
/// The longest wait between attempts.
pub const MAX_DELAY: Duration = Duration::from_secs(30);

/// Where the lock is in its life cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Unlocked,
    /// Shown, waiting for a password.
    Locked,
    /// A password check is running.
    Checking,
    /// Too many failures: typing is accepted but not checked until `until`
    /// (a monotonic time, as passed to [`LockMachine::handle`]).
    Delayed {
        until: Duration,
    },
}

/// The result of a password check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Accepted,
    Rejected,
    /// The authentication service could not decide. The screen stays locked
    /// and the attempt is not counted as a wrong password.
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    /// Someone asked for the lock: the explicit action, the idle timeout or
    /// logind (PrepareForSleep, Session.Lock).
    Lock,
    /// Enter was pressed with a password typed. The caller does not send
    /// this for an empty password.
    Submit,
    /// A password check finished.
    Checked(Verdict),
    /// The clock reached (or may have reached) a time the machine asked for.
    Tick,
    /// logind unlocked the session (Session.Unlock).
    Unlock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notice {
    /// Nothing to say.
    None,
    Checking,
    Incorrect,
    /// Incorrect, and attempts are paused for this long.
    IncorrectWait(Duration),
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// The screen is now shown.
    Show,
    /// The screen goes away: the session is unlocked.
    Dismiss,
    /// Check the typed password; the caller takes the password from its field.
    Authenticate,
    /// Forget the typed password.
    ClearPassword,
    /// Deliver [`Input::Tick`] at this monotonic time.
    WakeAt(Duration),
    /// Change the status line.
    ShowNotice(Notice),
}

/// The delay after the `failures`-th consecutive wrong password.
///
/// Nothing is delayed for the first [`FAILURES_BEFORE_DELAY`]−1 failures. Then
/// the delay starts at one second and doubles for every further failure, up
/// to [`MAX_DELAY`]. It never locks the account: the wait only paces attempts.
pub fn failure_delay(failures: u32) -> Option<Duration> {
    if failures < FAILURES_BEFORE_DELAY {
        return None;
    }
    let doublings = (failures - FAILURES_BEFORE_DELAY).min(16);
    let seconds = 1u64 << doublings;
    Some(Duration::from_secs(seconds).min(MAX_DELAY))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockMachine {
    phase: Phase,
    failures: u32,
}

impl Default for LockMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl LockMachine {
    pub fn new() -> Self {
        Self {
            phase: Phase::Unlocked,
            failures: 0,
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// Consecutive wrong passwords since the last accepted one.
    pub fn failures(&self) -> u32 {
        self.failures
    }

    /// Apply one input at monotonic time `now` and return what to do.
    /// Inputs that do not apply to the current phase change nothing.
    pub fn handle(&mut self, input: Input, now: Duration) -> Vec<Effect> {
        use Effect::*;
        use Phase::*;
        match (self.phase, input) {
            (Unlocked, Input::Lock) => {
                self.phase = Locked;
                vec![Show, ShowNotice(Notice::None)]
            }
            (Locked, Input::Submit) => {
                self.phase = Checking;
                vec![Authenticate, ShowNotice(Notice::Checking)]
            }
            (Checking, Input::Checked(Verdict::Accepted)) => {
                self.phase = Unlocked;
                self.failures = 0;
                vec![ClearPassword, Dismiss]
            }
            (Checking, Input::Checked(Verdict::Rejected)) => {
                self.failures = self.failures.saturating_add(1);
                match failure_delay(self.failures) {
                    Some(wait) => {
                        let until = now + wait;
                        self.phase = Delayed { until };
                        vec![
                            ClearPassword,
                            ShowNotice(Notice::IncorrectWait(wait)),
                            WakeAt(until),
                        ]
                    }
                    None => {
                        self.phase = Locked;
                        vec![ClearPassword, ShowNotice(Notice::Incorrect)]
                    }
                }
            }
            (Checking, Input::Checked(Verdict::Unavailable)) => {
                self.phase = Locked;
                vec![ClearPassword, ShowNotice(Notice::Unavailable)]
            }
            (Delayed { until }, Input::Tick) if now >= until => {
                self.phase = Locked;
                vec![ShowNotice(Notice::None)]
            }
            (Delayed { until }, Input::Tick) => vec![WakeAt(until)],
            // A logind unlock dismisses the lock whatever it was doing. A
            // check still running reports later and is then ignored.
            (Locked | Checking | Delayed { .. }, Input::Unlock) => {
                self.phase = Unlocked;
                self.failures = 0;
                vec![ClearPassword, Dismiss]
            }
            // Everything else is a no-op: a second lock request, input while
            // a check or delay runs, or a result that no longer matters.
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    fn machine_in(phase: Phase, failures: u32) -> LockMachine {
        LockMachine { phase, failures }
    }

    #[test]
    fn lock_from_unlocked_shows_the_screen() {
        let mut machine = LockMachine::new();
        let effects = machine.handle(Input::Lock, secs(0));
        assert_eq!(machine.phase(), Phase::Locked);
        assert_eq!(
            effects,
            vec![Effect::Show, Effect::ShowNotice(Notice::None)]
        );
    }

    #[test]
    fn a_second_lock_request_changes_nothing() {
        let mut machine = machine_in(Phase::Locked, 1);
        assert!(machine.handle(Input::Lock, secs(5)).is_empty());
        assert_eq!(machine.phase(), Phase::Locked);
        assert_eq!(machine.failures(), 1);
    }

    #[test]
    fn lock_while_checking_or_delayed_changes_nothing() {
        let mut checking = machine_in(Phase::Checking, 0);
        assert!(checking.handle(Input::Lock, secs(0)).is_empty());
        assert_eq!(checking.phase(), Phase::Checking);
        let until = secs(40);
        let mut delayed = machine_in(Phase::Delayed { until }, 3);
        assert!(delayed.handle(Input::Lock, secs(0)).is_empty());
        assert_eq!(delayed.phase(), Phase::Delayed { until });
    }

    #[test]
    fn submit_starts_one_check_and_says_so() {
        let mut machine = machine_in(Phase::Locked, 0);
        let effects = machine.handle(Input::Submit, secs(1));
        assert_eq!(machine.phase(), Phase::Checking);
        assert_eq!(
            effects,
            vec![Effect::Authenticate, Effect::ShowNotice(Notice::Checking)]
        );
        // A second Enter while the check runs does not start another check.
        assert!(machine.handle(Input::Submit, secs(1)).is_empty());
    }

    #[test]
    fn submit_while_unlocked_or_delayed_does_nothing() {
        let mut unlocked = LockMachine::new();
        assert!(unlocked.handle(Input::Submit, secs(0)).is_empty());
        assert_eq!(unlocked.phase(), Phase::Unlocked);
        let mut delayed = machine_in(Phase::Delayed { until: secs(9) }, 4);
        assert!(delayed.handle(Input::Submit, secs(1)).is_empty());
        assert_eq!(delayed.phase(), Phase::Delayed { until: secs(9) });
    }

    #[test]
    fn accepted_password_dismisses_and_resets_failures() {
        let mut machine = machine_in(Phase::Checking, 2);
        let effects = machine.handle(Input::Checked(Verdict::Accepted), secs(3));
        assert_eq!(machine.phase(), Phase::Unlocked);
        assert_eq!(machine.failures(), 0);
        assert_eq!(effects, vec![Effect::ClearPassword, Effect::Dismiss]);
    }

    #[test]
    fn first_wrong_password_stays_locked_and_clears_the_field() {
        let mut machine = machine_in(Phase::Checking, 0);
        let effects = machine.handle(Input::Checked(Verdict::Rejected), secs(3));
        assert_eq!(machine.phase(), Phase::Locked);
        assert_eq!(machine.failures(), 1);
        assert_eq!(
            effects,
            vec![Effect::ClearPassword, Effect::ShowNotice(Notice::Incorrect)]
        );
    }

    #[test]
    fn second_wrong_password_still_has_no_delay() {
        let mut machine = machine_in(Phase::Checking, 1);
        let effects = machine.handle(Input::Checked(Verdict::Rejected), secs(3));
        assert_eq!(machine.phase(), Phase::Locked);
        assert_eq!(machine.failures(), 2);
        assert_eq!(
            effects,
            vec![Effect::ClearPassword, Effect::ShowNotice(Notice::Incorrect)]
        );
    }

    #[test]
    fn third_wrong_password_waits_one_second_from_now() {
        let mut machine = machine_in(Phase::Checking, 2);
        let effects = machine.handle(Input::Checked(Verdict::Rejected), secs(10));
        assert_eq!(machine.phase(), Phase::Delayed { until: secs(11) });
        assert_eq!(machine.failures(), 3);
        assert_eq!(
            effects,
            vec![
                Effect::ClearPassword,
                Effect::ShowNotice(Notice::IncorrectWait(secs(1))),
                Effect::WakeAt(secs(11)),
            ]
        );
    }

    #[test]
    fn tick_before_the_deadline_rearms_the_wake_up() {
        let mut machine = machine_in(Phase::Delayed { until: secs(11) }, 3);
        let effects = machine.handle(Input::Tick, secs(10));
        assert_eq!(machine.phase(), Phase::Delayed { until: secs(11) });
        assert_eq!(effects, vec![Effect::WakeAt(secs(11))]);
    }

    #[test]
    fn tick_at_the_deadline_returns_to_locked() {
        let mut machine = machine_in(Phase::Delayed { until: secs(11) }, 3);
        let effects = machine.handle(Input::Tick, secs(11));
        assert_eq!(machine.phase(), Phase::Locked);
        assert_eq!(effects, vec![Effect::ShowNotice(Notice::None)]);
        // The failures stay, so the next wrong password waits again.
        assert_eq!(machine.failures(), 3);
    }

    #[test]
    fn tick_after_the_deadline_also_returns_to_locked() {
        let mut machine = machine_in(Phase::Delayed { until: secs(11) }, 3);
        machine.handle(Input::Tick, secs(90));
        assert_eq!(machine.phase(), Phase::Locked);
    }

    #[test]
    fn tick_while_locked_or_unlocked_does_nothing() {
        let mut locked = machine_in(Phase::Locked, 0);
        assert!(locked.handle(Input::Tick, secs(1)).is_empty());
        let mut unlocked = LockMachine::new();
        assert!(unlocked.handle(Input::Tick, secs(1)).is_empty());
    }

    #[test]
    fn unavailable_keeps_the_screen_locked_without_counting_a_failure() {
        let mut machine = machine_in(Phase::Checking, 2);
        let effects = machine.handle(Input::Checked(Verdict::Unavailable), secs(3));
        assert_eq!(machine.phase(), Phase::Locked);
        assert_eq!(machine.failures(), 2);
        assert_eq!(
            effects,
            vec![
                Effect::ClearPassword,
                Effect::ShowNotice(Notice::Unavailable)
            ]
        );
    }

    #[test]
    fn logind_unlock_dismisses_from_every_locked_phase() {
        for phase in [
            Phase::Locked,
            Phase::Checking,
            Phase::Delayed { until: secs(50) },
        ] {
            let mut machine = machine_in(phase, 4);
            let effects = machine.handle(Input::Unlock, secs(1));
            assert_eq!(machine.phase(), Phase::Unlocked, "from {phase:?}");
            assert_eq!(machine.failures(), 0);
            assert_eq!(effects, vec![Effect::ClearPassword, Effect::Dismiss]);
        }
    }

    #[test]
    fn logind_unlock_while_unlocked_does_nothing() {
        let mut machine = LockMachine::new();
        assert!(machine.handle(Input::Unlock, secs(0)).is_empty());
        assert_eq!(machine.phase(), Phase::Unlocked);
    }

    #[test]
    fn a_stale_verdict_after_unlock_is_ignored() {
        let mut machine = machine_in(Phase::Checking, 0);
        machine.handle(Input::Unlock, secs(1));
        assert!(machine
            .handle(Input::Checked(Verdict::Rejected), secs(2))
            .is_empty());
        assert_eq!(machine.phase(), Phase::Unlocked);
        assert_eq!(machine.failures(), 0);
    }

    #[test]
    fn a_verdict_when_no_check_is_running_is_ignored() {
        let mut locked = machine_in(Phase::Locked, 1);
        assert!(locked
            .handle(Input::Checked(Verdict::Accepted), secs(1))
            .is_empty());
        assert_eq!(locked.phase(), Phase::Locked);
        let mut delayed = machine_in(Phase::Delayed { until: secs(9) }, 3);
        assert!(delayed
            .handle(Input::Checked(Verdict::Accepted), secs(1))
            .is_empty());
        assert_eq!(delayed.phase(), Phase::Delayed { until: secs(9) });
    }

    #[test]
    fn failure_delay_is_none_before_the_threshold() {
        assert_eq!(failure_delay(0), None);
        assert_eq!(failure_delay(1), None);
        assert_eq!(failure_delay(2), None);
    }

    #[test]
    fn failure_delay_doubles_from_one_second_and_is_capped() {
        assert_eq!(failure_delay(3), Some(secs(1)));
        assert_eq!(failure_delay(4), Some(secs(2)));
        assert_eq!(failure_delay(5), Some(secs(4)));
        assert_eq!(failure_delay(6), Some(secs(8)));
        assert_eq!(failure_delay(7), Some(secs(16)));
        assert_eq!(failure_delay(8), Some(MAX_DELAY));
        assert_eq!(failure_delay(9), Some(MAX_DELAY));
        // Huge counts neither overflow nor exceed the cap.
        assert_eq!(failure_delay(u32::MAX), Some(MAX_DELAY));
    }

    #[test]
    fn a_full_wrong_password_sequence_never_locks_the_account() {
        let mut machine = LockMachine::new();
        machine.handle(Input::Lock, secs(0));
        let mut now = secs(0);
        for attempt in 1..=12u32 {
            machine.handle(Input::Submit, now);
            machine.handle(Input::Checked(Verdict::Rejected), now);
            if let Phase::Delayed { until } = machine.phase() {
                now = until;
                machine.handle(Input::Tick, now);
            }
            assert_eq!(machine.phase(), Phase::Locked, "after attempt {attempt}");
            assert_eq!(machine.failures(), attempt);
        }
        // The right password still works after twelve wrong ones.
        machine.handle(Input::Submit, now);
        machine.handle(Input::Checked(Verdict::Accepted), now);
        assert_eq!(machine.phase(), Phase::Unlocked);
    }
}
