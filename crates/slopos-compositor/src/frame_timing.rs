//! VRR (Variable Refresh Rate) and frame timing support.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Supported refresh rates (Hz).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RefreshRate {
    Hz60 = 60,
    Hz120 = 120,
    Hz144 = 144,
    Hz165 = 165,
    Adaptive = 0, // VRR (variable)
}

impl RefreshRate {
    pub fn as_hz(&self) -> u32 {
        *self as u32
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Hz60 => "60hz",
            Self::Hz120 => "120hz",
            Self::Hz144 => "144hz",
            Self::Hz165 => "165hz",
            Self::Adaptive => "adaptive",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        Self::parse_flexible(s)
    }

    /// Parse refresh rate from settings/env (`60`, `60hz`, `120Hz`, `adaptive`).
    pub fn parse_flexible(s: &str) -> Option<Self> {
        let s = s.trim().to_ascii_lowercase();
        match s.as_str() {
            "60" | "60hz" | "60h" => Some(Self::Hz60),
            "120" | "120hz" => Some(Self::Hz120),
            "144" | "144hz" => Some(Self::Hz144),
            "165" | "165hz" => Some(Self::Hz165),
            "adaptive" | "vrr" | "variable" | "0" => Some(Self::Adaptive),
            _ => None,
        }
    }

    pub fn frame_duration(&self) -> Duration {
        match self {
            Self::Hz60 => Duration::from_nanos(1_000_000_000 / 60),
            Self::Hz120 => Duration::from_nanos(1_000_000_000 / 120),
            Self::Hz144 => Duration::from_nanos(1_000_000_000 / 144),
            Self::Hz165 => Duration::from_nanos(1_000_000_000 / 165),
            // Adaptive scheduling is damage/presentation driven. The fixed-rate
            // scheduler must never introduce a periodic wake-up for VRR.
            Self::Adaptive => Duration::ZERO,
        }
    }

    /// Whether this rate means "pace with FrameScheduler" (false for Adaptive/VRR).
    pub fn is_fixed(&self) -> bool {
        !matches!(self, Self::Adaptive)
    }
}

/// Frame timing and VRR scheduler.
#[derive(Debug)]
pub struct FrameScheduler {
    target_refresh_rate: RefreshRate,
    last_frame_time: Option<Instant>,
    frame_times: VecDeque<Duration>,
    max_frame_history: usize,
}

impl Default for FrameScheduler {
    fn default() -> Self {
        Self::new(RefreshRate::Hz60)
    }
}

impl FrameScheduler {
    const DEFAULT_HISTORY: usize = 120;

    pub fn new(target_refresh_rate: RefreshRate) -> Self {
        Self::with_history_limit(target_refresh_rate, Self::DEFAULT_HISTORY)
    }

    pub fn with_history_limit(target_refresh_rate: RefreshRate, max_frame_history: usize) -> Self {
        Self {
            target_refresh_rate,
            last_frame_time: None,
            frame_times: VecDeque::new(),
            max_frame_history: max_frame_history.max(1),
        }
    }

    /// Set the target refresh rate.
    ///
    /// A rate change starts a new pacing epoch. Keeping the old timestamp would
    /// make the first frame at the new rate inherit a deadline from the old
    /// mode, and keeping old samples would make diagnostics report a blended
    /// FPS that never existed at either rate.
    pub fn set_refresh_rate(&mut self, rate: RefreshRate) {
        if self.target_refresh_rate != rate {
            self.target_refresh_rate = rate;
            self.reset_timing();
        }
    }

    /// Get the target refresh rate.
    pub fn refresh_rate(&self) -> RefreshRate {
        self.target_refresh_rate
    }

    /// Clear deadline and diagnostic history without changing refresh policy.
    pub fn reset_timing(&mut self) {
        self.last_frame_time = None;
        self.frame_times.clear();
    }

    /// Record a frame at the current monotonic instant.
    ///
    /// Returns true only when a fixed-rate backend should consult
    /// [`Self::time_until_next_frame`]. Adaptive/VRR backends should wait for
    /// damage, presentation feedback, or another compositor event instead.
    pub fn record_frame(&mut self) -> bool {
        self.record_frame_at(Instant::now())
    }

    /// Deterministic form of [`Self::record_frame`] used by tests and replayable
    /// compositor timing diagnostics.
    pub fn record_frame_at(&mut self, now: Instant) -> bool {
        if let Some(last_time) = self.last_frame_time {
            if let Some(elapsed) = now.checked_duration_since(last_time) {
                self.frame_times.push_back(elapsed);
                while self.frame_times.len() > self.max_frame_history {
                    self.frame_times.pop_front();
                }
            }
        }

        self.last_frame_time = Some(now);
        self.target_refresh_rate.is_fixed()
    }

    /// Calculate the time to wait before presenting the next frame.
    /// This implements fixed-rate VSync pacing. Adaptive mode deliberately
    /// returns zero because its wake-up source is external to this scheduler.
    pub fn time_until_next_frame(&self) -> Duration {
        self.time_until_next_frame_at(Instant::now())
    }

    /// Deterministic form of [`Self::time_until_next_frame`].
    pub fn time_until_next_frame_at(&self, now: Instant) -> Duration {
        if !self.target_refresh_rate.is_fixed() {
            return Duration::ZERO;
        }

        let Some(last_time) = self.last_frame_time else {
            return Duration::ZERO;
        };
        let elapsed = now.checked_duration_since(last_time).unwrap_or_default();
        self.target_refresh_rate
            .frame_duration()
            .saturating_sub(elapsed)
    }

    /// Get the average frame time over recent history.
    pub fn average_frame_time(&self) -> Duration {
        if self.frame_times.is_empty() {
            return Duration::ZERO;
        }

        let total: Duration = self.frame_times.iter().copied().sum();
        total / self.frame_times.len() as u32
    }

    /// Get the current FPS based on recent frame times.
    pub fn current_fps(&self) -> f32 {
        let avg = self.average_frame_time();
        if avg.is_zero() {
            0.0
        } else {
            1.0 / avg.as_secs_f32()
        }
    }

    /// Number of inter-frame samples currently retained for diagnostics.
    pub fn sample_count(&self) -> usize {
        self.frame_times.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_rate_duration_is_exactly_policy_driven() {
        assert_eq!(
            RefreshRate::Hz60.frame_duration(),
            Duration::from_nanos(16_666_666)
        );
        assert_eq!(
            RefreshRate::Hz120.frame_duration(),
            Duration::from_nanos(8_333_333)
        );
        assert_eq!(
            RefreshRate::Hz144.frame_duration(),
            Duration::from_nanos(6_944_444)
        );
        assert_eq!(RefreshRate::Adaptive.frame_duration(), Duration::ZERO);
        assert!(!RefreshRate::Adaptive.is_fixed());
        assert!(RefreshRate::Hz60.is_fixed());
    }

    #[test]
    fn refresh_rate_serialization_and_flexible_parsing_match() {
        assert_eq!(RefreshRate::Hz60.as_str(), "60hz");
        assert_eq!(RefreshRate::Hz120.as_str(), "120hz");
        assert_eq!(RefreshRate::Adaptive.as_str(), "adaptive");
        assert_eq!(RefreshRate::from_str("60hz"), Some(RefreshRate::Hz60));
        assert_eq!(RefreshRate::from_str("120Hz"), Some(RefreshRate::Hz120));
        assert_eq!(
            RefreshRate::from_str("adaptive"),
            Some(RefreshRate::Adaptive)
        );
        assert_eq!(
            RefreshRate::parse_flexible("VRR"),
            Some(RefreshRate::Adaptive)
        );
        assert_eq!(RefreshRate::from_str("invalid"), None);
    }

    #[test]
    fn fixed_rate_deadline_is_deterministic_without_sleeping() {
        let mut scheduler = FrameScheduler::new(RefreshRate::Hz60);
        let start = Instant::now();
        assert!(scheduler.record_frame_at(start));

        let halfway = start + Duration::from_millis(8);
        let remaining = scheduler.time_until_next_frame_at(halfway);
        assert_eq!(remaining, Duration::from_nanos(8_666_666));

        let late = start + Duration::from_millis(20);
        assert_eq!(scheduler.time_until_next_frame_at(late), Duration::ZERO);
    }

    #[test]
    fn adaptive_mode_never_requests_fixed_rate_waiting() {
        let mut scheduler = FrameScheduler::new(RefreshRate::Adaptive);
        let start = Instant::now();
        assert!(!scheduler.record_frame_at(start));
        assert_eq!(
            scheduler.time_until_next_frame_at(start + Duration::from_millis(1)),
            Duration::ZERO
        );
    }

    #[test]
    fn rate_change_resets_deadline_and_diagnostic_history() {
        let mut scheduler = FrameScheduler::new(RefreshRate::Hz60);
        let start = Instant::now();
        scheduler.record_frame_at(start);
        scheduler.record_frame_at(start + Duration::from_millis(16));
        assert_eq!(scheduler.sample_count(), 1);

        scheduler.set_refresh_rate(RefreshRate::Hz120);
        assert_eq!(scheduler.sample_count(), 0);
        assert_eq!(scheduler.current_fps(), 0.0);
        assert_eq!(scheduler.time_until_next_frame_at(start), Duration::ZERO);
    }

    #[test]
    fn history_is_bounded_and_fps_uses_only_retained_samples() {
        let mut scheduler = FrameScheduler::with_history_limit(RefreshRate::Hz60, 3);
        let start = Instant::now();
        scheduler.record_frame_at(start);
        for frame in 1..=5 {
            scheduler.record_frame_at(start + Duration::from_millis(10 * frame));
        }

        assert_eq!(scheduler.sample_count(), 3);
        assert_eq!(scheduler.average_frame_time(), Duration::from_millis(10));
        assert!((scheduler.current_fps() - 100.0).abs() < 0.01);
    }

    #[test]
    fn non_monotonic_sample_does_not_poison_history() {
        let mut scheduler = FrameScheduler::new(RefreshRate::Hz60);
        let start = Instant::now();
        scheduler.record_frame_at(start + Duration::from_millis(10));
        scheduler.record_frame_at(start);
        assert_eq!(scheduler.sample_count(), 0);

        scheduler.record_frame_at(start + Duration::from_millis(20));
        assert_eq!(scheduler.sample_count(), 1);
        assert_eq!(scheduler.average_frame_time(), Duration::from_millis(20));
    }
}
