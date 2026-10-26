//! Restart policy for supervised session processes.

use std::process;
use std::time::{Duration, Instant};

/// A process that ran at least this long counts as healthy, which resets its
/// failure count.
const HEALTHY_RUNTIME: Duration = Duration::from_secs(30);
/// Consecutive quick failures after which supervision gives up.
const MAX_FAST_FAILURES: u32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    /// The session cannot continue without it (the window manager).
    Required,
    /// Restart on failure; if it keeps failing, give up but keep the session.
    Important,
    /// Nice to have: if it is not installed or fails to start, stop trying.
    Optional,
}

pub struct Child {
    name: &'static str,
    policy: Policy,
    spawn: fn() -> Option<process::Child>,
    process: Option<process::Child>,
    launched_at: Instant,
    fast_failures: u32,
    restart_after: Instant,
    given_up: bool,
}

impl Child {
    pub fn new(name: &'static str, policy: Policy, spawn: fn() -> Option<process::Child>) -> Self {
        let mut child = Self {
            name,
            policy,
            spawn,
            process: None,
            launched_at: Instant::now(),
            fast_failures: 0,
            restart_after: Instant::now(),
            given_up: false,
        };
        child.process = (child.spawn)();
        if child.process.is_none() && policy == Policy::Optional {
            child.given_up = true;
        }
        child
    }

    /// Restart the process if it exited. Returns an error once when the child
    /// is given up on; callers decide what that means for the session.
    pub fn supervise(&mut self) -> Result<(), String> {
        if self.given_up {
            return Ok(());
        }
        if let Some(process) = self.process.as_mut() {
            match process.try_wait() {
                Ok(None) => return Ok(()),
                Ok(Some(status)) => {
                    self.process = None;
                    if self.policy == Policy::Required && status.success() {
                        self.given_up = true;
                        return Err(format!("{} exited normally; ending the session", self.name));
                    }
                    self.record_failure(&format!("exited with {status}"))?;
                }
                Err(error) => {
                    self.process = None;
                    self.record_failure(&format!("could not be waited on: {error}"))?;
                }
            }
        }
        if Instant::now() < self.restart_after {
            return Ok(());
        }
        self.launched_at = Instant::now();
        self.process = (self.spawn)();
        if self.process.is_none() {
            if self.policy == Policy::Optional {
                self.given_up = true;
                return Err(format!("{} could not be started", self.name));
            }
            self.record_failure("could not be started")?;
        }
        Ok(())
    }

    fn record_failure(&mut self, reason: &str) -> Result<(), String> {
        if self.launched_at.elapsed() >= HEALTHY_RUNTIME {
            self.fast_failures = 0;
        }
        self.fast_failures += 1;
        if self.fast_failures >= MAX_FAST_FAILURES {
            self.given_up = true;
            return Err(format!(
                "{} {reason}; it failed {} times in quick succession, so it will not be restarted",
                self.name, self.fast_failures
            ));
        }
        let delay = Duration::from_millis(250 << self.fast_failures.min(4));
        self.restart_after = Instant::now() + delay;
        log::warn!(
            "{} {reason}; restarting in {} ms (attempt {} of {})",
            self.name,
            delay.as_millis(),
            self.fast_failures,
            MAX_FAST_FAILURES - 1
        );
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(mut process) = self.process.take() {
            // SAFETY: SIGTERM to our own child lets it clean up; fall back to
            // SIGKILL if it is still running after a grace period.
            unsafe {
                libc::kill(process.id() as i32, libc::SIGTERM);
            }
            let deadline = Instant::now() + Duration::from_secs(2);
            while Instant::now() < deadline {
                if let Ok(Some(_)) = process.try_wait() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            let _ = process.kill();
            let _ = process.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn never_starts() -> Option<process::Child> {
        None
    }

    fn exits_at_once() -> Option<process::Child> {
        process::Command::new("/bin/sh")
            .args(["-c", "exit 3"])
            .spawn()
            .ok()
    }

    fn exits_cleanly() -> Option<process::Child> {
        process::Command::new("/bin/sh")
            .args(["-c", "exit 0"])
            .spawn()
            .ok()
    }

    fn runs() -> Option<process::Child> {
        process::Command::new("/bin/sh")
            .args(["-c", "sleep 5"])
            .spawn()
            .ok()
    }

    #[test]
    fn a_missing_optional_program_is_dropped_without_errors() {
        let mut child = Child::new("compositor", Policy::Optional, never_starts);
        assert!(child.given_up);
        for _ in 0..10 {
            assert!(child.supervise().is_ok());
        }
    }

    #[test]
    fn crash_loops_back_off_and_then_give_up_once() {
        let mut child = Child::new("shell", Policy::Important, exits_at_once);
        let mut errors = 0;
        for _ in 0..400 {
            child.restart_after = Instant::now();
            std::thread::sleep(Duration::from_millis(5));
            if child.supervise().is_err() {
                errors += 1;
            }
        }
        assert!(child.given_up);
        assert_eq!(errors, 1, "giving up must be reported exactly once");
    }

    #[test]
    fn a_required_child_that_exits_cleanly_ends_the_session() {
        let mut child = Child::new("Openbox", Policy::Required, exits_cleanly);
        let mut error = None;
        for _ in 0..200 {
            std::thread::sleep(Duration::from_millis(5));
            if let Err(reason) = child.supervise() {
                error = Some(reason);
                break;
            }
        }
        let error = error.expect("a clean exit must end the session");
        assert!(error.contains("exited normally"), "{error}");
        assert!(child.given_up);
        assert!(child.process.is_none());
        // Ending the session is reported once, not on every later poll.
        assert!(child.supervise().is_ok());
    }

    #[test]
    fn a_required_child_that_fails_is_still_restarted() {
        let mut child = Child::new("Openbox", Policy::Required, exits_at_once);
        let mut restarted = false;
        for _ in 0..200 {
            child.restart_after = Instant::now();
            std::thread::sleep(Duration::from_millis(5));
            assert!(child.supervise().is_ok());
            if child.fast_failures > 0 && child.process.is_some() {
                restarted = true;
                break;
            }
        }
        assert!(restarted, "the failed child was not restarted");
        assert!(!child.given_up);
        child.stop();
    }

    #[test]
    fn a_running_process_is_left_alone_and_stopped_cleanly() {
        let mut child = Child::new("wm", Policy::Required, runs);
        assert!(child.supervise().is_ok());
        assert_eq!(child.fast_failures, 0);
        child.stop();
        assert!(child.process.is_none());
    }
}
