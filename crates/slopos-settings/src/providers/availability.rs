//! Provider availability detection for delegated and native settings panels.

use std::env;
use std::path::{Path, PathBuf};

pub fn command_exists(program: &str) -> bool {
    resolve_program_path(program).is_some()
}

pub fn resolve_program_path(program: &str) -> Option<PathBuf> {
    if program.starts_with("slopos-") {
        if let Ok(executable) = env::current_exe() {
            if let Some(parent) = executable.parent() {
                let sibling = parent.join(program);
                if sibling.is_file() {
                    return Some(sibling);
                }
            }
        }
        let local = PathBuf::from("scripts").join(program);
        if local.is_file() {
            return Some(local);
        }
    }

    let path = Path::new(program);
    if path.components().count() > 1 {
        return path.is_file().then(|| path.to_path_buf());
    }

    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths)
            .map(|directory| directory.join(program))
            .find(|candidate| candidate.is_file())
    })
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
