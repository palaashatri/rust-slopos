use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_TEMP_ATTEMPTS: u32 = 128;
const RECOVERY_HEADER: &str = "SLOPOS-I TextEdit recovery v1\n";

pub(crate) struct OpenedDocument {
    pub(crate) text: String,
    pub(crate) saved_text: String,
    pub(crate) recovered: bool,
}

/// Save a document through a durable sibling snapshot and an atomic rename.
///
/// The recovery snapshot is written before the destination is replaced. It is
/// removed after a successful replacement, but intentionally remains when the
/// replacement fails so the next open can recover the user's text.
pub(crate) fn save_document(path: &Path, contents: &str) -> io::Result<()> {
    write_recovery_snapshot(path, contents)?;
    atomic_write(path, contents.as_bytes())?;

    // The document is already safely replaced if cleanup fails. A later open
    // compares the snapshot with the document and removes an equal stale copy.
    let _ = remove_recovery_snapshot(path);
    Ok(())
}

/// Open a document, preferring a different valid recovery snapshot over the
/// on-disk contents. The snapshot is retained until a successful save.
pub(crate) fn open_document(path: &Path) -> io::Result<OpenedDocument> {
    let on_disk = fs::read_to_string(path);
    let recovery = read_recovery_snapshot(path)?;

    match on_disk {
        Ok(saved_text) => match recovery {
            Some(recovered_text) if recovered_text != saved_text => Ok(OpenedDocument {
                text: recovered_text,
                saved_text,
                recovered: true,
            }),
            Some(_) => {
                let _ = remove_recovery_snapshot(path);
                Ok(OpenedDocument {
                    text: saved_text.clone(),
                    saved_text,
                    recovered: false,
                })
            }
            None => Ok(OpenedDocument {
                text: saved_text.clone(),
                saved_text,
                recovered: false,
            }),
        },
        Err(error) => match recovery {
            Some(recovered_text) => Ok(OpenedDocument {
                text: recovered_text,
                saved_text: String::new(),
                recovered: true,
            }),
            None => Err(error),
        },
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    atomic_write_with_nonce(path, bytes, &unique_nonce())
}

fn atomic_write_with_nonce(path: &Path, bytes: &[u8], nonce: &str) -> io::Result<()> {
    let parent = parent_directory(path);
    let (temporary_path, mut temporary_file) = create_temporary_file(path, nonce)?;

    let write_result = temporary_file
        .write_all(bytes)
        .and_then(|_| temporary_file.sync_all());
    drop(temporary_file);

    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }

    if let Err(error) = fs::rename(&temporary_path, path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }

    // A synced file plus an atomic rename protects the document contents. A
    // best-effort directory sync also makes the rename itself durable where
    // the platform permits opening a directory as a file.
    if let Ok(directory) = File::open(parent) {
        let _ = directory.sync_all();
    }
    Ok(())
}

fn create_temporary_file(path: &Path, nonce: &str) -> io::Result<(PathBuf, File)> {
    for attempt in 0..MAX_TEMP_ATTEMPTS {
        let candidate = temporary_path(path, nonce, attempt)?;
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((candidate, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a unique TextEdit temporary file",
    ))
}

fn temporary_path(path: &Path, nonce: &str, attempt: u32) -> io::Result<PathBuf> {
    let file_name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "TextEdit save path must name a file",
        )
    })?;
    let mut temporary_name = OsString::from(".");
    temporary_name.push(file_name);
    temporary_name.push(format!(".slopos-textedit-{nonce}-{attempt}.tmp"));
    Ok(parent_directory(path).join(temporary_name))
}

fn recovery_path(path: &Path) -> io::Result<PathBuf> {
    let file_name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "TextEdit recovery path must name a file",
        )
    })?;
    let mut recovery_name = OsString::from(".");
    recovery_name.push(file_name);
    recovery_name.push(".slopos-recovery");
    Ok(parent_directory(path).join(recovery_name))
}

fn recovery_prefix(path: &Path) -> String {
    format!("{RECOVERY_HEADER}target={}\n\n", path.display())
}

fn write_recovery_snapshot(path: &Path, contents: &str) -> io::Result<()> {
    let mut payload = recovery_prefix(path).into_bytes();
    payload.extend_from_slice(contents.as_bytes());
    let recovery = recovery_path(path)?;
    atomic_write(&recovery, &payload)
}

fn read_recovery_snapshot(path: &Path) -> io::Result<Option<String>> {
    let recovery = recovery_path(path)?;
    let bytes = match fs::read(recovery) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };

    let prefix = recovery_prefix(path);
    let Some(payload) = bytes.strip_prefix(prefix.as_bytes()) else {
        return Ok(None);
    };
    Ok(String::from_utf8(payload.to_vec()).ok())
}

fn remove_recovery_snapshot(path: &Path) -> io::Result<()> {
    match fs::remove_file(recovery_path(path)?) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn parent_directory(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn unique_nonce() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{}-{timestamp}", std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn unique_directory(name: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "slopos-i_textedit_save_{name}_{timestamp}_{sequence}"
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn atomic_write_retries_after_temp_file_collision() {
        let directory = unique_directory("collision");
        let target = directory.join("note.txt");
        let nonce = "collision-test";
        let colliding_temp = temporary_path(&target, nonce, 0).unwrap();
        fs::write(&colliding_temp, b"keep this file").unwrap();

        atomic_write_with_nonce(&target, b"new contents", nonce).unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"new contents");
        assert_eq!(fs::read(&colliding_temp).unwrap(), b"keep this file");
        assert!(
            !temporary_path(&target, nonce, 1).unwrap().exists(),
            "the successful temporary file must be renamed away"
        );

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn atomic_write_removes_temp_file_when_rename_fails() {
        let directory = unique_directory("rename-failure");
        let target = directory.join("blocked");
        fs::create_dir(&target).unwrap();
        let nonce = "rename-failure-test";

        let result = atomic_write_with_nonce(&target, b"must not replace directory", nonce);

        assert!(result.is_err());
        assert!(target.is_dir(), "the existing target must remain untouched");
        assert!(
            !temporary_path(&target, nonce, 0).unwrap().exists(),
            "failed replacement must not leave a temporary file behind"
        );

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn failed_save_keeps_recovery_snapshot_for_next_open() {
        let directory = unique_directory("recovery");
        let target = directory.join("blocked");
        fs::create_dir(&target).unwrap();

        let result = save_document(&target, "unsaved changes");

        assert!(result.is_err());
        let recovery = recovery_path(&target).unwrap();
        assert!(recovery.is_file());

        let opened = open_document(&target).unwrap();
        assert_eq!(opened.text, "unsaved changes");
        assert!(opened.saved_text.is_empty());
        assert!(opened.recovered);

        fs::remove_dir_all(directory).unwrap();
    }
}
