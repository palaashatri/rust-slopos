//! Performance budget gates (pure) — Phase F1 scaffold.
//!
//! Used to assert frame/input latency targets without requiring live hardware.

/// Named performance budgets for session components.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PerfMetric {
    /// Steady-state frame time budget.
    FrameTimeMs,
    /// Input-to-redraw latency budget.
    InputLatencyMs,
    /// Cold compositor bind time budget.
    CompositorReadyMs,
}

impl PerfMetric {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FrameTimeMs => "frame_time_ms",
            Self::InputLatencyMs => "input_latency_ms",
            Self::CompositorReadyMs => "compositor_ready_ms",
        }
    }

    /// Default budgets for a "daily driver" laptop-class target.
    pub fn default_budget_ms(self) -> u64 {
        match self {
            Self::FrameTimeMs => 16, // ~60 Hz
            Self::InputLatencyMs => 50,
            Self::CompositorReadyMs => 3000,
        }
    }
}

/// Result of checking a measured sample against budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PerfSample {
    pub metric: PerfMetric,
    pub measured_ms: u64,
    pub budget_ms: u64,
}

impl PerfSample {
    pub fn new(metric: PerfMetric, measured_ms: u64) -> Self {
        Self {
            metric,
            measured_ms,
            budget_ms: metric.default_budget_ms(),
        }
    }

    pub fn with_budget(metric: PerfMetric, measured_ms: u64, budget_ms: u64) -> Self {
        Self {
            metric,
            measured_ms,
            budget_ms,
        }
    }

    pub fn ok(self) -> bool {
        self.measured_ms <= self.budget_ms
    }

    pub fn overage_ms(self) -> u64 {
        self.measured_ms.saturating_sub(self.budget_ms)
    }
}

/// Aggregate a set of samples; overall ok only if all ok.
pub fn perf_budget_all_ok(samples: &[PerfSample]) -> bool {
    samples.iter().all(|s| s.ok())
}

/// Score 0–100 for a sample (100 if on budget, linear drop to 0 at 2× budget).
pub fn perf_sample_score(sample: PerfSample) -> u8 {
    if sample.budget_ms == 0 {
        return if sample.measured_ms == 0 { 100 } else { 0 };
    }
    if sample.measured_ms <= sample.budget_ms {
        return 100;
    }
    let twice = sample.budget_ms.saturating_mul(2);
    if sample.measured_ms >= twice {
        return 0;
    }
    let span = (twice - sample.budget_ms) as f64;
    let over = (sample.measured_ms - sample.budget_ms) as f64;
    (100.0 * (1.0 - over / span)).round().clamp(0.0, 100.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_budget_16ms_ok() {
        let s = PerfSample::new(PerfMetric::FrameTimeMs, 14);
        assert!(s.ok());
        assert_eq!(s.overage_ms(), 0);
        assert_eq!(perf_sample_score(s), 100);
    }

    #[test]
    fn over_budget_scores_down() {
        let s = PerfSample::new(PerfMetric::FrameTimeMs, 24);
        assert!(!s.ok());
        assert_eq!(s.overage_ms(), 8);
        let score = perf_sample_score(s);
        assert!(score < 100 && score > 0, "score={score}");
    }

    #[test]
    fn twice_budget_is_zero() {
        let s = PerfSample::new(PerfMetric::FrameTimeMs, 32);
        assert_eq!(perf_sample_score(s), 0);
    }

    #[test]
    fn all_ok_aggregate() {
        let samples = [
            PerfSample::new(PerfMetric::FrameTimeMs, 10),
            PerfSample::new(PerfMetric::InputLatencyMs, 40),
        ];
        assert!(perf_budget_all_ok(&samples));
        let bad = [
            PerfSample::new(PerfMetric::FrameTimeMs, 10),
            PerfSample::new(PerfMetric::InputLatencyMs, 80),
        ];
        assert!(!perf_budget_all_ok(&bad));
    }
}
