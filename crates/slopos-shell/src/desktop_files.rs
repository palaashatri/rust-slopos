//! Desktop file operations offered by the menu bar.

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

fn home() -> Option<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

/// The user's desktop directory from `user-dirs.dirs`, else `~/Desktop`.
pub fn desktop_dir() -> Option<PathBuf> {
    let home = home()?;
    let config = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".config"));
    let configured = fs::read_to_string(config.join("user-dirs.dirs"))
        .ok()
        .and_then(|text| parse_user_dir(&text, "XDG_DESKTOP_DIR", &home));
    Some(configured.unwrap_or_else(|| home.join("Desktop")))
}

/// Parse one entry of `user-dirs.dirs` (`XDG_DESKTOP_DIR="$HOME/Desktop"`).
fn parse_user_dir(text: &str, key: &str, home: &Path) -> Option<PathBuf> {
    text.lines().find_map(|line| {
        let value = line
            .trim()
            .strip_prefix(key)?
            .trim_start()
            .strip_prefix('=')?;
        let value = value.trim().trim_matches('"');
        if let Some(rest) = value.strip_prefix("$HOME") {
            Some(home.join(rest.trim_start_matches('/')))
        } else if value.starts_with('/') {
            Some(PathBuf::from(value))
        } else {
            None
        }
    })
}

/// Create "untitled folder" on the desktop, adding a number if that name is
/// taken. Never reuses or touches an existing folder.
pub fn create_untitled_folder() -> Result<PathBuf, String> {
    let desktop = desktop_dir().ok_or("HOME is not set")?;
    fs::create_dir_all(&desktop)
        .map_err(|error| format!("{} is not available: {error}", desktop.display()))?;
    create_unique_dir(&desktop, "untitled folder")
}

fn create_unique_dir(parent: &Path, base: &str) -> Result<PathBuf, String> {
    for index in 1..1000 {
        let name = if index == 1 {
            base.to_owned()
        } else {
            format!("{base} {index}")
        };
        let candidate = parent.join(name);
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("{}: {error}", candidate.display())),
        }
    }
    Err(format!("too many untitled folders in {}", parent.display()))
}

/// Empty the Trash. `gio trash --empty` also clears the trash folders of
/// other mounted volumes; without gio, the home trash is emptied directly.
pub fn empty_trash() -> Result<(), String> {
    if let Some(gio) = slopos_core::process::resolve_program("gio") {
        let status = Command::new(gio)
            .args(["trash", "--empty"])
            .status()
            .map_err(|error| format!("could not run gio: {error}"))?;
        return if status.success() {
            Ok(())
        } else {
            Err(format!("gio trash --empty failed ({status})"))
        };
    }
    let data = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| home().map(|home| home.join(".local/share")))
        .ok_or("HOME is not set")?;
    empty_trash_dir(&data.join("Trash"))
}

fn empty_trash_dir(trash: &Path) -> Result<(), String> {
    let mut failures = Vec::new();
    for sub in ["files", "info", "expunged"] {
        let dir = trash.join(sub);
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            // file_type() does not follow symlinks, so a link to a folder
            // outside the Trash is removed as a link, never traversed.
            let result = match entry.file_type() {
                Ok(kind) if kind.is_dir() => fs::remove_dir_all(&path),
                Ok(_) => fs::remove_file(&path),
                Err(error) => Err(error),
            };
            if let Err(error) = result {
                failures.push(format!("{}: {error}", path.display()));
            }
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} item(s) could not be deleted:\n{}",
            failures.len(),
            failures.join("\n")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("slopos-shell-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn user_dirs_entries_expand_home_and_absolute_paths() {
        let home = Path::new("/home/ada");
        let text = "# comment\nXDG_DOWNLOAD_DIR=\"$HOME/Downloads\"\nXDG_DESKTOP_DIR=\"$HOME/Schreibtisch\"\n";
        assert_eq!(
            parse_user_dir(text, "XDG_DESKTOP_DIR", home),
            Some(PathBuf::from("/home/ada/Schreibtisch"))
        );
        assert_eq!(
            parse_user_dir("XDG_DESKTOP_DIR=\"/data/desk\"", "XDG_DESKTOP_DIR", home),
            Some(PathBuf::from("/data/desk"))
        );
        assert_eq!(
            parse_user_dir("XDG_DESKTOP_DIR=\"relative\"", "XDG_DESKTOP_DIR", home),
            None
        );
    }

    #[test]
    fn untitled_folders_get_unique_names_and_never_reuse_existing_ones() {
        let dir = temp("folders");
        fs::create_dir(dir.join("untitled folder")).unwrap();
        fs::write(dir.join("untitled folder/keep.txt"), "user data").unwrap();
        let created = create_unique_dir(&dir, "untitled folder").unwrap();
        assert_eq!(created, dir.join("untitled folder 2"));
        assert!(dir.join("untitled folder/keep.txt").exists());
    }

    #[test]
    fn emptying_the_trash_removes_links_without_following_them() {
        let root = temp("trash");
        let outside = root.join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("precious.txt"), "keep me").unwrap();

        let trash = root.join("Trash");
        fs::create_dir_all(trash.join("files/old dir")).unwrap();
        fs::write(trash.join("files/old dir/a.txt"), "x").unwrap();
        fs::write(trash.join("files/b.txt"), "y").unwrap();
        fs::create_dir_all(trash.join("info")).unwrap();
        fs::write(trash.join("info/b.txt.trashinfo"), "[Trash Info]").unwrap();
        std::os::unix::fs::symlink(&outside, trash.join("files/link")).unwrap();

        empty_trash_dir(&trash).unwrap();
        assert_eq!(fs::read_dir(trash.join("files")).unwrap().count(), 0);
        assert_eq!(fs::read_dir(trash.join("info")).unwrap().count(), 0);
        assert!(outside.join("precious.txt").exists());
    }
}
