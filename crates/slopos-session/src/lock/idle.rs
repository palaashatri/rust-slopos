//! Idle timing for the automatic lock. The X server reports the time since
//! the last user input (the screensaver extension); this module decides what
//! to do with that reading. Nothing here sleeps: the caller waits for the
//! returned duration and asks again, so a timer fires once per timeout and is
//! armed again by the next input.

use std::time::Duration;

/// Idle time before the screen locks when no setting exists.
pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdleAction {
    /// The user has been idle for the whole timeout: lock now.
    Lock,
    /// Ask again after this much time, when the timeout would be reached if
    /// there is no input.
    Wait(Duration),
}

/// What to do after reading `idle_for` (the time since the last input).
pub fn next_idle_action(idle_for: Duration, timeout: Duration) -> IdleAction {
    if idle_for >= timeout {
        IdleAction::Lock
    } else {
        IdleAction::Wait(timeout - idle_for)
    }
}

/// Parse the idle setting: whole seconds, where `0` turns the automatic lock
/// off. Returns `None` for text that is not a number of seconds, so the
/// caller can keep the default and say so.
///
/// `Some(None)` means "disabled"; `Some(Some(d))` is the timeout `d`.
pub fn parse_idle_seconds(text: &str) -> Option<Option<Duration>> {
    let seconds: u64 = text.trim().parse().ok()?;
    Some((seconds > 0).then(|| Duration::from_secs(seconds)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    /// Run the idle policy against a user who gives input at `inputs` (in
    /// seconds, sorted) and report the first lock time, if any, before
    /// `horizon`. Time moves only to the moments the policy asks for, the way
    /// the watcher's timer works.
    fn first_lock(timeout: Duration, inputs: &[u64], horizon: u64) -> Option<u64> {
        let mut now = 0u64;
        let mut last_input = 0u64;
        let mut inputs = inputs.iter().copied().peekable();
        while now < horizon {
            // Input that happened while we were waiting resets the idle time.
            while let Some(&at) = inputs.peek() {
                if at <= now {
                    last_input = at;
                    inputs.next();
                } else {
                    break;
                }
            }
            let idle = secs(now - last_input);
            match next_idle_action(idle, timeout) {
                IdleAction::Lock => return Some(now),
                IdleAction::Wait(wait) => {
                    let deadline = now + wait.as_secs();
                    // Wake at the deadline or at the next input, whichever is
                    // first; the next loop turn re-reads the idle time.
                    let next_input = inputs.peek().copied().unwrap_or(u64::MAX);
                    now = deadline.min(next_input).min(horizon);
                    if now == horizon {
                        break;
                    }
                }
            }
        }
        None
    }

    #[test]
    fn idle_for_at_or_past_the_timeout_locks() {
        assert_eq!(next_idle_action(secs(300), secs(300)), IdleAction::Lock);
        assert_eq!(next_idle_action(secs(301), secs(300)), IdleAction::Lock);
    }

    #[test]
    fn idle_below_the_timeout_waits_for_the_remainder() {
        assert_eq!(
            next_idle_action(secs(0), secs(300)),
            IdleAction::Wait(secs(300))
        );
        assert_eq!(
            next_idle_action(secs(299), secs(300)),
            IdleAction::Wait(secs(1))
        );
    }

    #[test]
    fn a_user_who_never_touches_the_machine_is_locked_at_the_timeout() {
        assert_eq!(first_lock(secs(60), &[], 1000), Some(60));
    }

    #[test]
    fn input_re_arms_the_timer() {
        // Input at 30 s pushes the lock from 60 s to 90 s. Input after the
        // first deadline would not have changed it.
        assert_eq!(first_lock(secs(60), &[30], 1000), Some(90));
        assert_eq!(first_lock(secs(60), &[100], 1000), Some(60));
    }

    #[test]
    fn steady_input_never_locks() {
        let inputs: Vec<u64> = (1..50).map(|i| i * 10).collect();
        assert_eq!(first_lock(secs(60), &inputs, 490), None);
    }

    #[test]
    fn input_just_before_the_deadline_keeps_the_screen_open() {
        assert_eq!(first_lock(secs(60), &[59], 1000), Some(119));
    }

    #[test]
    fn parse_accepts_whole_seconds_and_zero_disables() {
        assert_eq!(parse_idle_seconds("300\n"), Some(Some(secs(300))));
        assert_eq!(parse_idle_seconds(" 45 "), Some(Some(secs(45))));
        assert_eq!(parse_idle_seconds("0"), Some(None));
    }

    #[test]
    fn parse_rejects_anything_else() {
        assert_eq!(parse_idle_seconds(""), None);
        assert_eq!(parse_idle_seconds("five minutes"), None);
        assert_eq!(parse_idle_seconds("-5"), None);
        assert_eq!(parse_idle_seconds("1.5"), None);
    }
}
