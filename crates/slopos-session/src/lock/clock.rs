//! A monotonic clock the lock uses for delays and idle time. Tests pass a
//! [`ManualClock`] so no test waits for real time.

use std::sync::Mutex;
use std::time::{Duration, Instant};

pub trait Clock {
    /// Time since an arbitrary fixed origin; never goes backwards.
    fn now(&self) -> Duration;
}

impl<C: Clock + ?Sized> Clock for std::sync::Arc<C> {
    fn now(&self) -> Duration {
        (**self).now()
    }
}

/// The process's monotonic clock.
#[derive(Debug, Clone, Copy)]
pub struct SystemClock {
    origin: Instant,
}

impl SystemClock {
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        self.origin.elapsed()
    }
}

/// A clock that moves only when told to.
#[derive(Debug, Default)]
pub struct ManualClock {
    now: Mutex<Duration>,
}

impl ManualClock {
    pub fn set(&self, now: Duration) {
        *self
            .now
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = now;
    }

    pub fn advance(&self, by: Duration) {
        let mut now = self
            .now
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *now += by;
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Duration {
        *self
            .now
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_clock_moves_only_when_told() {
        let clock = ManualClock::default();
        assert_eq!(clock.now(), Duration::ZERO);
        clock.advance(Duration::from_secs(3));
        clock.advance(Duration::from_millis(500));
        assert_eq!(clock.now(), Duration::from_millis(3500));
        clock.set(Duration::from_secs(1));
        assert_eq!(clock.now(), Duration::from_secs(1));
    }

    #[test]
    fn system_clock_never_goes_backwards() {
        let clock = SystemClock::new();
        let first = clock.now();
        assert!(clock.now() >= first);
    }
}
