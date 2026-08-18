//! Service adapters for system state and hardware integration.

pub mod audio;
pub mod bluetooth;
pub mod clock;
pub mod monitor;
pub mod network;
pub mod power;
pub mod session;

pub use monitor::{SystemMonitor, SystemStatus};
