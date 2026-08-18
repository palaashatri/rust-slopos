//! Provider modules for system utilities and commands.

pub mod availability;
pub mod commands;

pub use availability::{command_exists, resolve_program_path};
pub use commands::{command_output, execute_command};
