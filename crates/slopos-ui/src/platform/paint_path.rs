//! Which renderer paints a window's frames. The CPU path is the default.
//!
//! With the `gpu` feature built in and `SLOPOS_GPU=1` in the environment, a
//! window paints each frame into a recording surface and replays the recorded
//! [`DrawList`](crate::render::list::DrawList) on the wgpu executor. A window
//! falls back to the CPU for good when no GPU adapter exists or a GPU frame
//! fails; the reason is logged once. The policy here has no X11 types, so it
//! is tested without a display.

use std::fmt::Display;

use crate::render::gpu::WgpuExecutor;

/// The environment variable that opts a process into GPU painting.
pub const GPU_VARIABLE: &str = "SLOPOS_GPU";

/// The most commands one frame's recording may hold. A frame of an app's paint
/// is a few thousand commands; this bounds the list's memory.
pub const RECORD_COMMAND_LIMIT: usize = 1 << 16;

/// Whether the value of [`GPU_VARIABLE`] asks for GPU painting. Only `1` and
/// `true` (any case, surrounding spaces ignored) opt in.
pub fn gpu_requested(value: Option<&str>) -> bool {
    matches!(
        value
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref(),
        Some("1" | "true")
    )
}

/// The GPU state of one window.
pub enum GpuPaint {
    /// Not requested: every frame is painted by the CPU.
    Off,
    /// Requested. The executor is created when the first frame needs it, so
    /// windows that never paint pay nothing.
    Requested,
    /// Ready to paint frames.
    Ready(Box<WgpuExecutor>),
    /// Unusable for this window. The reason has been logged; the CPU paints
    /// every later frame.
    Fallback,
}

impl GpuPaint {
    /// `Requested` when `SLOPOS_GPU` opts in, otherwise `Off`.
    pub fn from_environment() -> Self {
        let value = std::env::var(GPU_VARIABLE).ok();
        if gpu_requested(value.as_deref()) {
            Self::Requested
        } else {
            Self::Off
        }
    }

    /// The executor to paint with, or `None` when this frame must be painted
    /// on the CPU. The first call on a requested window creates the executor.
    pub fn executor(&mut self) -> Option<&mut WgpuExecutor> {
        if matches!(self, Self::Requested) {
            *self = match WgpuExecutor::new_headless() {
                Ok(executor) => {
                    log::info!(
                        "{GPU_VARIABLE}=1: this window paints on the GPU ({})",
                        executor.adapter_info().name
                    );
                    Self::Ready(Box::new(executor))
                }
                Err(error) => {
                    log::warn!(
                        "{GPU_VARIABLE}=1 but no usable GPU adapter was found; \
                         this window paints on the CPU: {error}"
                    );
                    Self::Fallback
                }
            };
        }
        match self {
            Self::Ready(executor) => Some(executor),
            _ => None,
        }
    }

    /// Stop using the GPU for this window after `error`, and log why.
    pub fn fail(&mut self, error: &dyn Display) {
        if matches!(self, Self::Ready(_) | Self::Requested) {
            log::warn!("GPU painting failed; this window paints on the CPU from now on: {error}");
        }
        *self = Self::Fallback;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_one_or_true_opt_into_gpu_painting() {
        assert!(gpu_requested(Some("1")));
        assert!(gpu_requested(Some(" TRUE ")));
        for value in [
            None,
            Some(""),
            Some("0"),
            Some("yes"),
            Some("on"),
            Some("2"),
        ] {
            assert!(!gpu_requested(value), "{value:?} must not opt in");
        }
    }

    #[test]
    fn a_failed_window_stays_on_the_cpu() {
        let mut state = GpuPaint::Requested;
        state.fail(&"no adapter in this test");
        assert!(matches!(state, GpuPaint::Fallback));
        // A fallback never tries the adapter again.
        assert!(state.executor().is_none());
        assert!(matches!(state, GpuPaint::Fallback));
    }

    #[test]
    fn a_window_that_did_not_ask_for_gpu_never_has_an_executor() {
        let mut state = GpuPaint::Off;
        assert!(state.executor().is_none());
        assert!(matches!(state, GpuPaint::Off));
    }
}
