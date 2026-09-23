//! X11 Integration subsystem for SLOPOS-I.
//!
//! Provides long-lived event-driven state tracking using `x11rb`, eliminating
//! process-spawning polling on timers.

pub mod atoms;
pub mod connection;
pub mod events;
pub mod ewmh;
pub mod monitors;
pub mod pointer;
pub mod windows;

pub use atoms::Atoms;
pub use connection::X11Connection;
pub use events::{X11Event, X11EventBus};
pub use monitors::{Monitor, MonitorModel};
pub use windows::WindowInfo;
