//! Startup phase budgets — pure timing policy (no wall clock).
//!
//! Callers measure elapsed ms and pass numbers into these helpers so unit
//! tests stay deterministic.

/// Ordered phases of session startup.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StartupPhase {
    SessionEnv,
    Compositor,
    Shell,
    Portals,
    Ready,
}

impl StartupPhase {
    /// All phases in startup order.
    pub fn all() -> &'static [StartupPhase] {
        &[
            StartupPhase::SessionEnv,
            StartupPhase::Compositor,
            StartupPhase::Shell,
            StartupPhase::Portals,
            StartupPhase::Ready,
        ]
    }

    pub fn as_str(self) -> &'static str {
        match self {
            StartupPhase::SessionEnv => "session_env",
            StartupPhase::Compositor => "compositor",
            StartupPhase::Shell => "shell",
            StartupPhase::Portals => "portals",
            StartupPhase::Ready => "ready",
        }
    }
}

/// Maximum allowed duration (ms) per startup phase.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartupBudget {
    pub session_env_ms: u64,
    pub compositor_ms: u64,
    pub shell_ms: u64,
    pub portals_ms: u64,
    pub ready_ms: u64,
}

impl StartupBudget {
    /// Max ms allowed for `phase`.
    pub fn max_ms(&self, phase: StartupPhase) -> u64 {
        match phase {
            StartupPhase::SessionEnv => self.session_env_ms,
            StartupPhase::Compositor => self.compositor_ms,
            StartupPhase::Shell => self.shell_ms,
            StartupPhase::Portals => self.portals_ms,
            StartupPhase::Ready => self.ready_ms,
        }
    }

    /// Sum of all phase budgets.
    pub fn total_ms(&self) -> u64 {
        self.session_env_ms + self.compositor_ms + self.shell_ms + self.portals_ms + self.ready_ms
    }
}

/// Default desktop-session budgets (milliseconds).
///
/// Tuned for a cold start on modest hardware; tests use these exact numbers.
pub fn default_desktop_budget() -> StartupBudget {
    StartupBudget {
        session_env_ms: 500,
        compositor_ms: 3_000,
        shell_ms: 2_000,
        portals_ms: 1_500,
        ready_ms: 500,
    }
}

/// Outcome of recording a single phase duration against a budget.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhaseResult {
    pub phase: StartupPhase,
    pub elapsed_ms: u64,
    pub budget_ms: u64,
    /// `true` when `elapsed_ms <= budget_ms`.
    pub ok: bool,
    /// `true` when `elapsed_ms > budget_ms`.
    pub over_budget: bool,
}

/// Compare `elapsed_ms` for `phase` against `budget`.
pub fn record_phase(budget: &StartupBudget, phase: StartupPhase, elapsed_ms: u64) -> PhaseResult {
    let budget_ms = budget.max_ms(phase);
    let over_budget = elapsed_ms > budget_ms;
    PhaseResult {
        phase,
        elapsed_ms,
        budget_ms,
        ok: !over_budget,
        over_budget,
    }
}

/// Overall startup is OK only if every recorded phase is OK.
pub fn overall_ok(results: &[PhaseResult]) -> bool {
    !results.is_empty() && results.iter().all(|r| r.ok)
}

/// Total elapsed ms across recorded results.
pub fn total_elapsed_ms(results: &[PhaseResult]) -> u64 {
    results.iter().map(|r| r.elapsed_ms).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_budget_values() {
        let b = default_desktop_budget();
        assert_eq!(b.session_env_ms, 500);
        assert_eq!(b.compositor_ms, 3_000);
        assert_eq!(b.shell_ms, 2_000);
        assert_eq!(b.portals_ms, 1_500);
        assert_eq!(b.ready_ms, 500);
        assert_eq!(b.total_ms(), 7_500);
        assert_eq!(b.max_ms(StartupPhase::Compositor), 3_000);
    }

    #[test]
    fn record_phase_within_budget() {
        let b = default_desktop_budget();
        let r = record_phase(&b, StartupPhase::SessionEnv, 200);
        assert!(r.ok);
        assert!(!r.over_budget);
        assert_eq!(r.elapsed_ms, 200);
        assert_eq!(r.budget_ms, 500);
        assert_eq!(r.phase, StartupPhase::SessionEnv);
    }

    #[test]
    fn record_phase_exact_boundary_is_ok() {
        let b = default_desktop_budget();
        let r = record_phase(&b, StartupPhase::Shell, 2_000);
        assert!(r.ok);
        assert!(!r.over_budget);
    }

    #[test]
    fn record_phase_over_budget() {
        let b = default_desktop_budget();
        let r = record_phase(&b, StartupPhase::Compositor, 3_001);
        assert!(!r.ok);
        assert!(r.over_budget);
        assert_eq!(r.budget_ms, 3_000);
    }

    #[test]
    fn overall_ok_requires_all_phases() {
        let b = default_desktop_budget();
        let results = vec![
            record_phase(&b, StartupPhase::SessionEnv, 100),
            record_phase(&b, StartupPhase::Compositor, 1_000),
            record_phase(&b, StartupPhase::Shell, 500),
            record_phase(&b, StartupPhase::Portals, 200),
            record_phase(&b, StartupPhase::Ready, 50),
        ];
        assert!(overall_ok(&results));
        assert_eq!(total_elapsed_ms(&results), 1_850);
    }

    #[test]
    fn overall_ok_false_on_any_overrun() {
        let b = default_desktop_budget();
        let results = vec![
            record_phase(&b, StartupPhase::SessionEnv, 100),
            record_phase(&b, StartupPhase::Compositor, 9_999), // over
            record_phase(&b, StartupPhase::Shell, 100),
        ];
        assert!(!overall_ok(&results));
    }

    #[test]
    fn overall_ok_empty_is_false() {
        assert!(!overall_ok(&[]));
    }

    #[test]
    fn phase_all_covers_five() {
        assert_eq!(StartupPhase::all().len(), 5);
        assert_eq!(StartupPhase::Portals.as_str(), "portals");
    }
}
