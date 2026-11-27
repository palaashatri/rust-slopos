//! Which processes the monitor may end, and how it reads back the result.
//!
//! Ending a process is the only action that changes the system, so the rules
//! live here as plain functions. The app asks twice: when the confirmation is
//! opened, and again after "Yes" (the process may have exited, or its PID may
//! have been reused by another program, in the meantime).

use std::fmt;

/// The process the user chose, as it was when the monitor listed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub pid: u32,
    pub start_ticks: u64,
    pub uid: Option<u32>,
    pub name: String,
}

/// What `/proc` says about a PID right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Identity {
    pub start_ticks: u64,
    pub uid: Option<u32>,
    pub state: char,
}

/// Why the monitor will not send a signal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// PID 1 (init or systemd) is never ended from here.
    InitProcess,
    /// The process belongs to another user (or its owner is unknown).
    NotOwned { owner: Option<u32> },
    /// No process has this PID any more.
    Gone,
    /// The PID now belongs to a different process than the one listed.
    Reused,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refusal::InitProcess => {
                write!(f, "PID 1 is the init process and is never ended here")
            }
            Refusal::NotOwned { owner: Some(uid) } => write!(
                f,
                "the process belongs to user {uid}; only your own processes can be ended"
            ),
            Refusal::NotOwned { owner: None } => write!(
                f,
                "the process owner is unavailable; only your own processes can be ended"
            ),
            Refusal::Gone => write!(f, "the process has already exited"),
            Refusal::Reused => write!(
                f,
                "the PID now belongs to another process; nothing was sent"
            ),
        }
    }
}

/// Checks made when the confirmation is opened (`current_user` is the
/// caller's effective user ID).
pub fn may_offer_end(pid: u32, uid: Option<u32>, current_user: u32) -> Result<(), Refusal> {
    if pid <= 1 {
        return Err(Refusal::InitProcess);
    }
    match uid {
        Some(owner) if owner == current_user => Ok(()),
        owner => Err(Refusal::NotOwned { owner }),
    }
}

/// Checks made after "Yes", against what `/proc` says now. `identity` is
/// `None` when the PID no longer exists.
pub fn may_send(
    target: &Target,
    identity: Option<&Identity>,
    current_user: u32,
) -> Result<(), Refusal> {
    let Some(now) = identity else {
        return Err(Refusal::Gone);
    };
    if now.start_ticks != target.start_ticks {
        return Err(Refusal::Reused);
    }
    if now.state == 'Z' {
        return Err(Refusal::Gone);
    }
    may_offer_end(target.pid, now.uid, current_user)
}

/// What the read-back after SIGTERM found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Readback {
    Ended,
    StillRunning,
}

/// Decide from the PID's current identity whether the listed process ended.
pub fn read_back(target: &Target, identity: Option<&Identity>) -> Readback {
    match identity {
        Some(now) if now.start_ticks == target.start_ticks && now.state != 'Z' => {
            Readback::StillRunning
        }
        _ => Readback::Ended,
    }
}

/// One line for the action log, written to standard error: the time in
/// seconds since the epoch, the action, the target and the outcome.
pub fn action_log_line(unix_seconds: u64, target: &Target, outcome: &str) -> String {
    format!(
        "slopos-system-monitor: {unix_seconds} end-process pid={} name={:?} start_ticks={} result={outcome}",
        target.pid, target.name, target.start_ticks
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> Target {
        Target {
            pid: 4000,
            start_ticks: 77,
            uid: Some(1000),
            name: "runaway".into(),
        }
    }

    fn identity(start: u64, uid: Option<u32>, state: char) -> Identity {
        Identity {
            start_ticks: start,
            uid,
            state,
        }
    }

    #[test]
    fn init_is_never_offered_even_when_it_belongs_to_the_user() {
        assert_eq!(
            may_offer_end(1, Some(1000), 1000),
            Err(Refusal::InitProcess)
        );
        assert_eq!(may_offer_end(0, Some(0), 0), Err(Refusal::InitProcess));
    }

    #[test]
    fn only_the_current_users_processes_are_offered() {
        assert_eq!(may_offer_end(4000, Some(1000), 1000), Ok(()));
        assert_eq!(
            may_offer_end(4000, Some(0), 1000),
            Err(Refusal::NotOwned { owner: Some(0) })
        );
        assert_eq!(
            may_offer_end(4000, None, 1000),
            Err(Refusal::NotOwned { owner: None })
        );
    }

    #[test]
    fn a_reused_pid_or_a_vanished_process_is_refused_at_send_time() {
        let listed = target();
        assert_eq!(may_send(&listed, None, 1000), Err(Refusal::Gone));
        assert_eq!(
            may_send(&listed, Some(&identity(78, Some(1000), 'S')), 1000),
            Err(Refusal::Reused)
        );
        assert_eq!(
            may_send(&listed, Some(&identity(77, Some(1000), 'Z')), 1000),
            Err(Refusal::Gone)
        );
        assert_eq!(
            may_send(&listed, Some(&identity(77, Some(1000), 'S')), 1000),
            Ok(())
        );
        assert_eq!(
            may_send(&listed, Some(&identity(77, Some(0), 'S')), 1000),
            Err(Refusal::NotOwned { owner: Some(0) })
        );
    }

    #[test]
    fn read_back_reports_exit_zombies_and_reuse_as_ended() {
        let listed = target();
        assert_eq!(
            read_back(&listed, Some(&identity(77, Some(1000), 'S'))),
            Readback::StillRunning
        );
        assert_eq!(
            read_back(&listed, Some(&identity(77, Some(1000), 'Z'))),
            Readback::Ended
        );
        assert_eq!(
            read_back(&listed, Some(&identity(900, Some(1000), 'S'))),
            Readback::Ended
        );
        assert_eq!(read_back(&listed, None), Readback::Ended);
    }

    #[test]
    fn refusals_read_as_sentences() {
        assert!(Refusal::InitProcess.to_string().starts_with("PID 1"));
        assert_eq!(
            Refusal::NotOwned { owner: Some(0) }.to_string(),
            "the process belongs to user 0; only your own processes can be ended"
        );
    }

    #[test]
    fn the_log_line_names_the_action_target_and_outcome() {
        let line = action_log_line(1_700_000_000, &target(), "sent SIGTERM");
        assert_eq!(
            line,
            "slopos-system-monitor: 1700000000 end-process pid=4000 name=\"runaway\" start_ticks=77 result=sent SIGTERM"
        );
    }
}
