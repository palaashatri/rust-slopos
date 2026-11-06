//! The Dock's preferences: `$XDG_CONFIG_HOME/slopos-i/dock.conf`, one
//! `key = value` per line. Only `autohide` exists. A missing or unreadable
//! file means the defaults (auto-hide off), and unknown keys are ignored.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Each save gets its own temporary name, even two saves in one process.
static SAVES: AtomicU64 = AtomicU64::new(0);

/// `$XDG_CONFIG_HOME/slopos-i/dock.conf`, or `~/.config/...` when that is unset
/// or relative. Only the config directory is needed, so the other XDG
/// directories are not required.
pub fn path() -> Option<PathBuf> {
    config_file(
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
        std::env::var_os("HOME").map(PathBuf::from),
    )
}

/// The preferences file under `xdg_config_home` (when absolute), else under
/// `home/.config`.
fn config_file(xdg_config_home: Option<PathBuf>, home: Option<PathBuf>) -> Option<PathBuf> {
    let config = xdg_config_home
        .filter(|path| path.is_absolute())
        .or_else(|| home.map(|home| home.join(".config")))?;
    Some(config.join("slopos-i/dock.conf"))
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
    save_autohide_at(&path, on)
}

/// Write the preference to `path`: a temporary file under a unique name, then a
/// rename, so a crash never leaves a half-written file and overlapping saves
/// never share a temporary file.
pub fn save_autohide_at(path: &Path, on: bool) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let contents = format!(
        "# Dock preferences.\nautohide = {}\n",
        if on { "true" } else { "false" }
    );
    let serial = SAVES.fetch_add(1, Ordering::Relaxed);
    let temporary = path.with_extension(format!("{}.{serial}.tmp", std::process::id()));
    fs::write(&temporary, contents)?;
    fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_config_file_needs_only_the_config_directory() {
        let absolute = config_file(Some("/cfg".into()), Some("/home/u".into()));
        assert_eq!(absolute, Some(PathBuf::from("/cfg/slopos-i/dock.conf")));
        // A relative XDG_CONFIG_HOME is ignored, as the XDG spec says.
        let relative = config_file(Some("cfg".into()), Some("/home/u".into()));
        assert_eq!(
            relative,
            Some(PathBuf::from("/home/u/.config/slopos-i/dock.conf"))
        );
        assert_eq!(
            config_file(None, Some("/home/u".into())),
            Some(PathBuf::from("/home/u/.config/slopos-i/dock.conf"))
        );
        assert_eq!(config_file(None, None), None);
        // An absolute XDG_CONFIG_HOME needs no HOME, so the preference still
        // loads and saves when HOME is unset.
        assert_eq!(
            config_file(Some("/cfg".into()), None),
            Some(PathBuf::from("/cfg/slopos-i/dock.conf"))
        );
    }

    #[test]
    fn overlapping_saves_each_leave_one_complete_file() {
        let dir = std::env::temp_dir().join(format!("slopos-dock-prefs-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("a test directory");
        let path = dir.join("dock.conf");
        let writers: Vec<_> = (0..8)
            .map(|index| {
                let path = path.clone();
                std::thread::spawn(move || save_autohide_at(&path, index % 2 == 0))
            })
            .collect();
        for writer in writers {
            writer
                .join()
                .expect("a writer thread")
                .expect("every overlapping save succeeds");
        }
        let saved = fs::read_to_string(&path).expect("the preference file");
        assert!(saved.starts_with("# Dock preferences.\n"), "{saved:?}");
        assert_eq!(saved.matches("autohide = ").count(), 1, "{saved:?}");
        assert!(saved.contains("autohide = true") || saved.contains("autohide = false"));
        let leftover = fs::read_dir(&dir)
            .expect("the test directory")
            .filter(|entry| {
                entry
                    .as_ref()
                    .is_ok_and(|entry| entry.file_name() != "dock.conf")
            })
            .count();
        assert_eq!(leftover, 0, "no temporary file is left behind");
        fs::remove_dir_all(&dir).ok();
    }

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
