//! Durable, bounded Recent Items storage for the shell.
//!
//! Recent history is user data, not a cache: writes are atomic and a malformed
//! or unsafe file is ignored rather than replaced with fabricated entries.

use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const FORMAT_VERSION: u32 = 1;
pub const MAX_ITEMS: usize = 20;
const MAX_ITEM_BYTES: usize = 4096;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct RecentItemsFile {
    version: u32,
    items: Vec<String>,
}

/// Return the user-scoped Recent Items path.
pub fn default_path() -> PathBuf {
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
        if !data_home.is_empty() {
            return PathBuf::from(data_home)
                .join("slopos-i")
                .join("recent-items.json");
        }
    }

    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".local/share/slopos-i/recent-items.json")
}

/// Normalize entries read from disk or produced by the shell.
///
/// Ordering is preserved, duplicate values are removed, and malformed/empty
/// entries are discarded. This keeps an untrusted or hand-edited file bounded.
pub fn normalize_items<I, S>(items: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut normalized = Vec::with_capacity(MAX_ITEMS);
    for value in items {
        let value = value.as_ref().trim();
        if value.is_empty() || value.len() > MAX_ITEM_BYTES {
            continue;
        }
        if normalized.iter().any(|existing| existing == value) {
            continue;
        }
        normalized.push(value.to_string());
        if normalized.len() == MAX_ITEMS {
            break;
        }
    }
    normalized
}

/// Read Recent Items. Missing, malformed, old-version, directory, or symlink
/// paths are treated as an empty history; the caller never receives invented
/// values from a failed read.
pub fn load(path: &Path) -> Vec<String> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Vec::new();
    };
    if !metadata.file_type().is_file() {
        return Vec::new();
    }

    let Ok(bytes) = fs::read(path) else {
        return Vec::new();
    };
    let Ok(file) = serde_json::from_slice::<RecentItemsFile>(&bytes) else {
        return Vec::new();
    };
    if file.version != FORMAT_VERSION {
        return Vec::new();
    }
    normalize_items(file.items)
}

/// Persist Recent Items with a same-directory create-new temporary file and a
/// final rename. Existing symlinks/directories are rejected to avoid writing
/// through a path controlled by another object.
pub fn save(path: &Path, items: &[String]) -> io::Result<()> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if !metadata.file_type().is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "recent items path is not a regular file",
            ));
        }
    }

    let items = normalize_items(items.iter().map(String::as_str));
    let payload = serde_json::to_vec_pretty(&RecentItemsFile {
        version: FORMAT_VERSION,
        items,
    })
    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;

    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "recent items path has no parent directory",
        )
    })?;
    fs::create_dir_all(parent)?;

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "recent items path has no valid filename",
            )
        })?;
    let temporary = parent.join(format!(".{file_name}.tmp-{}-{unique}", std::process::id()));

    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(&payload)?;
        file.sync_all()?;
        drop(file);

        // Re-check after writing: a symlink may have appeared while the
        // temporary file was being prepared.
        if let Ok(metadata) = fs::symlink_metadata(path) {
            if !metadata.file_type().is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "recent items path changed to a non-regular file",
                ));
            }
        }
        fs::rename(&temporary, path)?;

        // Best-effort directory durability on Unix. The data file is already
        // durable; syncing the directory closes the rename durability window.
        #[cfg(unix)]
        {
            if let Ok(directory) = File::open(parent) {
                let _ = directory.sync_all();
            }
        }
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("slopos-i_recent_{name}_{unique}.json"))
    }

    #[test]
    fn normalize_is_bounded_and_deduplicated() {
        let mut values = vec![" Home ".to_string(), "Home".to_string(), String::new()];
        values.extend((0..30).map(|index| format!("item-{index}")));
        let normalized = normalize_items(values);
        assert_eq!(normalized.first().map(String::as_str), Some("Home"));
        assert_eq!(normalized.len(), MAX_ITEMS);
        assert!(!normalized.iter().any(|value| value.is_empty()));
    }

    #[test]
    fn save_and_load_round_trip_atomically() {
        let path = temporary_path("roundtrip");
        let _ = fs::remove_file(&path);
        save(&path, &["Home".to_string(), "Applications".to_string()]).unwrap();
        assert_eq!(load(&path), ["Home", "Applications"]);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"version\": 1"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn malformed_or_wrong_version_is_empty() {
        let path = temporary_path("invalid");
        fs::write(&path, b"not json").unwrap();
        assert!(load(&path).is_empty());
        fs::write(&path, br#"{"version": 99, "items": ["fake"]}"#).unwrap();
        assert!(load(&path).is_empty());
        let _ = fs::remove_file(path);
    }
}
