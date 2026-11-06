//! The Dock's preferences: `$XDG_CONFIG_HOME/slopos-i/dock.conf`, one
//! `key = value` per line. Only `autohide` exists. A missing or unreadable
//! file means the defaults (auto-hide off), and unknown keys are ignored.

use std::fs;
use std::io;
use std::path::PathBuf;

use slopos_core::xdg::XdgPaths;

pub fn path() -> Option<PathBuf> {
    XdgPaths::from_environment()
        .ok()
        .map(|paths| paths.config.join("slopos-i/dock.conf"))
}

/// Whether auto-hide is on in `contents`.
pub fn parse_autohide(contents: &str) -> bool {
    contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .find(|(key, _)| key.trim() == "autohide")
        .is_some_and(|(_, value)| matches!(value.trim(), "1" | "true" | "yes" | "on"))
}

/// Whether the user turned auto-hide on.
pub fn autohide() -> bool {
    path()
        .and_then(|path| fs::read_to_string(path).ok())
        .is_some_and(|contents| parse_autohide(&contents))
}

pub fn save_autohide(on: bool) -> io::Result<()> {
    let path = path().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no config dir"))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let contents = format!(
        "# Dock preferences.\nautohide = {}\n",
        if on { "true" } else { "false" }
    );
    // Write then rename, so a crash never leaves a half-written file.
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, contents)?;
    fs::rename(temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autohide_is_read_from_the_preferences_text() {
        assert!(parse_autohide("autohide = true\n"));
        assert!(parse_autohide("# comment\nautohide=on\n"));
        assert!(parse_autohide("  autohide = 1  "));
        assert!(!parse_autohide("autohide = false\n"));
        assert!(!parse_autohide("autohide = no\n"));
        assert!(!parse_autohide("# autohide = true\n"));
        assert!(!parse_autohide("other = true\n"));
        assert!(!parse_autohide(""));
    }
}
