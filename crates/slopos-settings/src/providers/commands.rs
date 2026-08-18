//! Command execution provider for settings helpers and delegated utilities.

use super::availability::resolve_program_path;
use std::process::Command;

pub fn execute_command(program: &str, args: &[&str]) -> bool {
    let Some(path) = resolve_program_path(program) else {
        log::warn!("Command not found: {program}");
        return false;
    };

    match Command::new(&path).args(args).spawn() {
        Ok(_) => true,
        Err(error) => {
            log::warn!("Failed to execute {}: {error}", path.display());
            false
        }
    }
}

pub fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let path = resolve_program_path(program)?;
    let output = Command::new(path).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}
