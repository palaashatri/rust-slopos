//! Provider availability detection for delegated and native settings panels.

use std::path::PathBuf;

pub fn command_exists(program: &str) -> bool {
    resolve_program_path(program).is_some()
}

pub fn resolve_program_path(program: &str) -> Option<PathBuf> {
    slopos_core::process::resolve_program(program)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_standard_utilities() {
        assert!(command_exists("sh") || command_exists("ls"));
        assert!(!command_exists("nonexistent_fake_utility_12345"));
    }
}
