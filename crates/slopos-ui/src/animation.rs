//! Deterministic, event-driven animation sampling.
//!
//! The caller provides elapsed time; no timers, threads, GPU or busy loops
//! are started here. Hosts schedule frames only while animation is active.

use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Easing {
    Linear,
    EaseInOutCubic,
    EaseOutCubic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionPreference {
    Full,
    Reduced,
}

/// A finite progress fraction in [0, 1]. Zero-duration and reduced-motion
/// transitions complete immediately.
pub fn progress(
    elapsed: Duration,
    duration: Duration,
    easing: Easing,
    motion: MotionPreference,
) -> f32 {
    if duration.is_zero() || motion == MotionPreference::Reduced {
        return 1.0;
    }
    let t = (elapsed.as_secs_f64() / duration.as_secs_f64()).min(1.0) as f32;
    match easing {
        Easing::Linear => t,
        Easing::EaseInOutCubic if t < 0.5 => 4.0 * t * t * t,
        Easing::EaseInOutCubic => 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0,
        Easing::EaseOutCubic => 1.0 - (1.0 - t).powi(3),
    }
}

pub fn lerp(from: f32, to: f32, progress: f32) -> f32 {
    from + (to - from) * progress.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easing_hits_exact_endpoints_and_does_not_overshoot() {
        for easing in [Easing::Linear, Easing::EaseInOutCubic, Easing::EaseOutCubic] {
            let duration = Duration::from_millis(100);
            assert_eq!(
                progress(Duration::ZERO, duration, easing, MotionPreference::Full),
                0.0
            );
            assert_eq!(
                progress(duration, duration, easing, MotionPreference::Full),
                1.0
            );
            assert_eq!(
                progress(
                    Duration::from_secs(5),
                    duration,
                    easing,
                    MotionPreference::Full
                ),
                1.0
            );
            for ms in 0..=100 {
                let value = progress(
                    Duration::from_millis(ms),
                    duration,
                    easing,
                    MotionPreference::Full,
                );
                assert!((0.0..=1.0).contains(&value));
            }
        }
    }

    #[test]
    fn reduced_motion_and_zero_duration_complete_immediately() {
        assert_eq!(
            progress(
                Duration::ZERO,
                Duration::from_secs(10),
                Easing::Linear,
                MotionPreference::Reduced
            ),
            1.0
        );
        assert_eq!(
            progress(
                Duration::ZERO,
                Duration::ZERO,
                Easing::Linear,
                MotionPreference::Full
            ),
            1.0
        );
    }

    #[test]
    fn lerp_clamps_out_of_range_progress() {
        assert_eq!(lerp(10.0, 30.0, -1.0), 10.0);
        assert_eq!(lerp(10.0, 30.0, 1.5), 30.0);
        assert_eq!(lerp(10.0, 30.0, 0.5), 20.0);
    }
}
