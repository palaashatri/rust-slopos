//! Session idle / lock timeout pure policy (logind-adjacent).
//!
//! Pure timers and inhibit rules — no logind D-Bus calls here. Shell/compositor
//! feed timestamps and query whether the session should dim / lock / sleep.

use serde::{Deserialize, Serialize};

/// Idle phase the session is in.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdlePhase {
    Active,
    /// Dim / DPMS warning window.
    IdleWarn,
    /// Should show lock screen.
    ShouldLock,
    /// Beyond lock; optional suspend request.
    ShouldSuspend,
}

/// Idle configuration (seconds).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct IdleConfig {
    /// Seconds of inactivity before idle warning / dim.
    pub warn_after_secs: u64,
    /// Seconds of inactivity before auto-lock (0 = never).
    pub lock_after_secs: u64,
    /// Seconds of inactivity before suspend (0 = never). Requires lock first if lock_on_suspend.
    pub suspend_after_secs: u64,
    pub lock_on_suspend: bool,
    /// When true, idle progression is frozen (video / presentation).
    pub inhibited: bool,
}

impl Default for IdleConfig {
    fn default() -> Self {
        Self {
            warn_after_secs: 5 * 60,
            lock_after_secs: 10 * 60,
            suspend_after_secs: 30 * 60,
            lock_on_suspend: true,
            inhibited: false,
        }
    }
}

impl IdleConfig {
    /// Parse from settings.conf flat keys.
    pub fn parse_from_conf(text: &str) -> Self {
        let mut cfg = Self::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let k = k.trim();
            let v = v.trim();
            match k {
                "idle_warn_secs" | "idle.warn_after" => {
                    if let Ok(n) = v.parse() {
                        cfg.warn_after_secs = n;
                    }
                }
                "idle_lock_secs" | "idle.lock_after" | "lock_after_secs" => {
                    if let Ok(n) = v.parse() {
                        cfg.lock_after_secs = n;
                    }
                }
                "idle_suspend_secs" | "idle.suspend_after" => {
                    if let Ok(n) = v.parse() {
                        cfg.suspend_after_secs = n;
                    }
                }
                "lock_on_suspend" => {
                    cfg.lock_on_suspend =
                        matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on");
                }
                _ => {}
            }
        }
        cfg
    }
}

/// Why idle is inhibited.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum InhibitReason {
    /// Media playback / screencast.
    Media,
    /// User presentation mode.
    Presentation,
    /// Active full-screen app requested inhibit.
    FullscreenApp,
    /// Manual / debug.
    Manual,
}

/// Track multiple inhibit tokens.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct IdleInhibitState {
    reasons: Vec<InhibitReason>,
}

impl IdleInhibitState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, reason: InhibitReason) {
        if !self.reasons.contains(&reason) {
            self.reasons.push(reason);
        }
    }

    pub fn remove(&mut self, reason: InhibitReason) {
        self.reasons.retain(|r| *r != reason);
    }

    pub fn is_inhibited(&self) -> bool {
        !self.reasons.is_empty()
    }

    pub fn reasons(&self) -> &[InhibitReason] {
        &self.reasons
    }
}

/// Pure: compute idle phase from config + seconds since last user activity.
///
/// `already_locked` prevents re-requesting lock while locked.
pub fn idle_phase(
    cfg: &IdleConfig,
    idle_secs: u64,
    already_locked: bool,
    inhibit: &IdleInhibitState,
) -> IdlePhase {
    if cfg.inhibited || inhibit.is_inhibited() {
        return IdlePhase::Active;
    }
    if cfg.suspend_after_secs > 0 && idle_secs >= cfg.suspend_after_secs {
        return IdlePhase::ShouldSuspend;
    }
    if !already_locked && cfg.lock_after_secs > 0 && idle_secs >= cfg.lock_after_secs {
        return IdlePhase::ShouldLock;
    }
    if cfg.warn_after_secs > 0 && idle_secs >= cfg.warn_after_secs {
        return IdlePhase::IdleWarn;
    }
    IdlePhase::Active
}

/// Recommended session action for a phase (maps into `session_actions`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdleRecommendedAction {
    None,
    DimDisplay,
    Lock,
    Suspend,
}

pub fn recommended_action(phase: IdlePhase, already_locked: bool) -> IdleRecommendedAction {
    match phase {
        IdlePhase::Active => IdleRecommendedAction::None,
        IdlePhase::IdleWarn => IdleRecommendedAction::DimDisplay,
        IdlePhase::ShouldLock if !already_locked => IdleRecommendedAction::Lock,
        IdlePhase::ShouldLock => IdleRecommendedAction::None,
        IdlePhase::ShouldSuspend => IdleRecommendedAction::Suspend,
    }
}

/// Seconds remaining until next phase transition (for UI countdown). `None` if none.
pub fn secs_until_next_phase(
    cfg: &IdleConfig,
    idle_secs: u64,
    already_locked: bool,
) -> Option<u64> {
    if cfg.inhibited {
        return None;
    }
    let mut candidates = Vec::new();
    if cfg.warn_after_secs > idle_secs {
        candidates.push(cfg.warn_after_secs - idle_secs);
    }
    if !already_locked && cfg.lock_after_secs > idle_secs && cfg.lock_after_secs > 0 {
        candidates.push(cfg.lock_after_secs - idle_secs);
    }
    if cfg.suspend_after_secs > idle_secs && cfg.suspend_after_secs > 0 {
        candidates.push(cfg.suspend_after_secs - idle_secs);
    }
    candidates.into_iter().min()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phases_progress() {
        let cfg = IdleConfig {
            warn_after_secs: 60,
            lock_after_secs: 120,
            suspend_after_secs: 300,
            lock_on_suspend: true,
            inhibited: false,
        };
        let inh = IdleInhibitState::new();
        assert_eq!(idle_phase(&cfg, 0, false, &inh), IdlePhase::Active);
        assert_eq!(idle_phase(&cfg, 60, false, &inh), IdlePhase::IdleWarn);
        assert_eq!(idle_phase(&cfg, 120, false, &inh), IdlePhase::ShouldLock);
        assert_eq!(idle_phase(&cfg, 300, true, &inh), IdlePhase::ShouldSuspend);
    }

    #[test]
    fn inhibit_blocks() {
        let cfg = IdleConfig::default();
        let mut inh = IdleInhibitState::new();
        inh.add(InhibitReason::Media);
        assert_eq!(idle_phase(&cfg, 99999, false, &inh), IdlePhase::Active);
        inh.remove(InhibitReason::Media);
        assert_eq!(
            idle_phase(&cfg, cfg.lock_after_secs, false, &inh),
            IdlePhase::ShouldLock
        );
    }

    #[test]
    fn conf_parse() {
        let cfg = IdleConfig::parse_from_conf("idle_lock_secs=30\nlock_on_suspend=false\n");
        assert_eq!(cfg.lock_after_secs, 30);
        assert!(!cfg.lock_on_suspend);
    }

    #[test]
    fn recommended_and_countdown() {
        let cfg = IdleConfig {
            warn_after_secs: 10,
            lock_after_secs: 20,
            suspend_after_secs: 0,
            lock_on_suspend: true,
            inhibited: false,
        };
        assert_eq!(
            recommended_action(IdlePhase::ShouldLock, false),
            IdleRecommendedAction::Lock
        );
        assert_eq!(secs_until_next_phase(&cfg, 5, false), Some(5));
    }
}
