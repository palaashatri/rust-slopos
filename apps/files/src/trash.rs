//! freedesktop.org Trash specification 1.0: move to Trash, list, restore,
//! and empty.
//!
//! Items on the home filesystem go to `$XDG_DATA_HOME/Trash`. Items on
//! another filesystem go to that volume's `$topdir/.Trash-$uid` (created
//! 0700, refused if it is a symlink or not ours), because moving them home
//! would be a slow copy. The `.trashinfo` file is created exclusively first
//! and removed again if the move fails, so no item is ever lost or
//! described twice.

use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::{Path, PathBuf};

use crate::ops::{delete_permanently, rename_no_replace, OpError, OpResult};

/// Tests that change `XDG_DATA_HOME` (process-wide) hold this lock.
#[cfg(test)]
pub(crate) static TEST_ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashItem {
    /// Name inside `files/` and `info/`.
    pub id: String,
    /// The trash directory that holds it.
    pub trash: PathBuf,
    /// Where it came from.
    pub original: PathBuf,
    /// `DeletionDate` as written (local time, `YYYY-MM-DDThh:mm:ss`).
    pub deleted: String,
}

impl TrashItem {
    pub fn file(&self) -> PathBuf {
        self.trash.join("files").join(&self.id)
    }

    fn info(&self) -> PathBuf {
        self.trash
            .join("info")
            .join(format!("{}.trashinfo", self.id))
    }
}

fn io_error(path: &Path) -> impl FnOnce(io::Error) -> OpError + '_ {
    move |source| OpError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// The home trash directory.
pub fn home_trash() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("Trash")
}

fn ensure_trash(trash: &Path) -> io::Result<()> {
    for sub in ["files", "info"] {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(trash.join(sub))?;
    }
    Ok(())
}

/// Percent-encode a path for `Path=` (RFC 2396 escaping, `/` kept).
fn encode(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut out = String::new();
    for &byte in path.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || b"/-_.~!*'()".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn decode(text: &str) -> PathBuf {
    use std::os::unix::ffi::OsStringExt;
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    PathBuf::from(std::ffi::OsString::from_vec(out))
}

/// Local time as `YYYY-MM-DDThh:mm:ss`.
fn local_timestamp() -> String {
    // SAFETY: time() and localtime_r() with valid pointers.
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&now, &mut tm);
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min,
            tm.tm_sec
        )
    }
}

/// The mount point of the filesystem holding `path` (walk up while the
/// device stays the same).
fn mount_root(path: &Path) -> io::Result<PathBuf> {
    let device = fs::symlink_metadata(path)?.dev();
    let mut root = path.to_path_buf();
    while let Some(parent) = root.parent() {
        if fs::metadata(parent)?.dev() != device {
            break;
        }
        root = parent.to_path_buf();
    }
    Ok(root)
}

/// The trash directory for `path`, and the `Path=` value to record.
fn trash_for(path: &Path) -> OpResult<(PathBuf, String)> {
    let home = home_trash();
    let home_base = home.parent().unwrap_or(Path::new("/"));
    fs::create_dir_all(home_base).map_err(io_error(home_base))?;
    let item_device = fs::symlink_metadata(path).map_err(io_error(path))?.dev();
    let home_device = fs::metadata(home_base).map_err(io_error(home_base))?.dev();
    if item_device == home_device {
        return Ok((home, encode(path)));
    }
    let top = mount_root(path).map_err(io_error(path))?;
    // SAFETY: getuid() has no preconditions.
    let uid = unsafe { libc::getuid() };
    let trash = top.join(format!(".Trash-{uid}"));
    match fs::symlink_metadata(&trash) {
        Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() || meta.uid() != uid => {
            return Err(OpError::Io {
                path: trash,
                source: io::Error::new(io::ErrorKind::PermissionDenied, "unsafe trash directory"),
            });
        }
        _ => {}
    }
    let relative = path.strip_prefix(&top).unwrap_or(path);
    Ok((trash, encode(relative)))
}

/// Move `path` to the Trash. Returns the new trash item.
pub fn trash(path: &Path) -> OpResult<TrashItem> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map_err(io_error(path))?.join(path)
    };
    let (trash_dir, recorded) = trash_for(&path)?;
    ensure_trash(&trash_dir).map_err(io_error(&trash_dir))?;
    let base = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "item".into());
    let deleted = local_timestamp();
    for n in 1..10_000 {
        let id = if n == 1 {
            base.clone()
        } else {
            format!("{base}.{n}")
        };
        let info_path = trash_dir.join("info").join(format!("{id}.trashinfo"));
        let mut info = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&info_path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(io_error(&info_path)(error)),
        };
        let content = format!("[Trash Info]\nPath={recorded}\nDeletionDate={deleted}\n");
        if let Err(error) = info
            .write_all(content.as_bytes())
            .and_then(|()| info.sync_all())
        {
            let _ = fs::remove_file(&info_path);
            return Err(io_error(&info_path)(error));
        }
        let target = trash_dir.join("files").join(&id);
        match rename_no_replace(&path, &target) {
            Ok(()) => {
                return Ok(TrashItem {
                    id,
                    trash: trash_dir,
                    original: path,
                    deleted,
                })
            }
            Err(error) => {
                let _ = fs::remove_file(&info_path);
                if error.raw_os_error() == Some(libc::EEXIST) {
                    // A stray file without info: try the next name.
                    continue;
                }
                return Err(io_error(&path)(error));
            }
        }
    }
    Err(OpError::Exists(path))
}

/// Items in the home trash, newest first. Entries whose file is missing are
/// skipped; malformed info files are ignored.
pub fn list() -> Vec<TrashItem> {
    list_in(&home_trash())
}

fn list_in(trash_dir: &Path) -> Vec<TrashItem> {
    let Ok(entries) = fs::read_dir(trash_dir.join("info")) else {
        return Vec::new();
    };
    let mut items: Vec<TrashItem> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let id = name.strip_suffix(".trashinfo")?.to_owned();
            let content = fs::read_to_string(entry.path()).ok()?;
            if !content.lines().any(|line| line.trim() == "[Trash Info]") {
                return None;
            }
            let field = |key: &str| {
                content
                    .lines()
                    .find_map(|line| line.strip_prefix(key).map(str::to_owned))
            };
            let recorded = decode(&field("Path=")?);
            let original = if recorded.is_absolute() {
                recorded
            } else {
                trash_dir.parent()?.join(recorded)
            };
            let item = TrashItem {
                id,
                trash: trash_dir.to_path_buf(),
                original,
                deleted: field("DeletionDate=").unwrap_or_default(),
            };
            fs::symlink_metadata(item.file()).is_ok().then_some(item)
        })
        .collect();
    items.sort_by(|a, b| b.deleted.cmp(&a.deleted).then_with(|| a.id.cmp(&b.id)));
    items
}

/// Put an item back where it came from. Fails, changing nothing, when
/// something now exists there.
pub fn restore(item: &TrashItem) -> OpResult<PathBuf> {
    if let Some(parent) = item.original.parent() {
        fs::create_dir_all(parent).map_err(io_error(parent))?;
    }
    match rename_no_replace(&item.file(), &item.original) {
        Ok(()) => {
            let _ = fs::remove_file(item.info());
            Ok(item.original.clone())
        }
        Err(error) if error.raw_os_error() == Some(libc::EEXIST) => {
            Err(OpError::Exists(item.original.clone()))
        }
        Err(error) => Err(io_error(&item.original)(error)),
    }
}

/// Permanently delete one trashed item.
pub fn erase(item: &TrashItem) -> OpResult<()> {
    delete_permanently(&item.file())?;
    let _ = fs::remove_file(item.info());
    Ok(())
}

/// Permanently delete everything in the home trash. Returns how many items.
pub fn empty() -> OpResult<usize> {
    let items = list();
    for item in &items {
        erase(item)?;
    }
    Ok(items.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    static ENV: &std::sync::Mutex<()> = &TEST_ENV;

    #[test]
    fn trash_list_restore_and_empty_round_trip() {
        let _guard = ENV.lock().unwrap_or_else(|p| p.into_inner());
        let home = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_DATA_HOME", home.path().join("data"));
        let docs = home.path().join("docs");
        fs::create_dir(&docs).unwrap();
        let file = docs.join("report 100%.txt");
        fs::write(&file, b"r").unwrap();
        let item = trash(&file).unwrap();
        assert!(!file.exists());
        assert_eq!(item.id, "report 100%.txt");
        let info = fs::read_to_string(item.info()).unwrap();
        assert!(info.contains("Path=") && info.contains("report%20100%25.txt"));
        // Same name again gets a distinct id.
        fs::write(&file, b"r2").unwrap();
        let second = trash(&file).unwrap();
        assert_eq!(second.id, "report 100%.txt.2");
        let listed = list();
        assert_eq!(listed.len(), 2);
        assert!(listed.iter().all(|i| i.original == file));
        // Restore refuses to overwrite, then succeeds once the name is free.
        fs::write(&file, b"occupied").unwrap();
        assert!(matches!(restore(&item), Err(OpError::Exists(_))));
        assert_eq!(fs::read(&file).unwrap(), b"occupied");
        fs::remove_file(&file).unwrap();
        assert_eq!(restore(&item).unwrap(), file);
        assert_eq!(fs::read(&file).unwrap(), b"r");
        assert_eq!(empty().unwrap(), 1);
        assert!(list().is_empty());
        assert!(fs::read_dir(home_trash().join("files"))
            .unwrap()
            .next()
            .is_none());
        std::env::remove_var("XDG_DATA_HOME");
    }

    #[test]
    fn folders_and_symlinks_are_trashed_without_following() {
        let _guard = ENV.lock().unwrap_or_else(|p| p.into_inner());
        let home = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_DATA_HOME", home.path().join("data"));
        let target = home.path().join("precious");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("keep"), b"k").unwrap();
        let link = home.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let item = trash(&link).unwrap();
        assert!(fs::symlink_metadata(item.file())
            .unwrap()
            .file_type()
            .is_symlink());
        erase(&item).unwrap();
        assert_eq!(fs::read(target.join("keep")).unwrap(), b"k");
        let folder = trash(&target).unwrap();
        assert!(folder.file().join("keep").exists());
        std::env::remove_var("XDG_DATA_HOME");
    }

    #[test]
    fn missing_items_and_malformed_info_are_handled() {
        let _guard = ENV.lock().unwrap_or_else(|p| p.into_inner());
        let home = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_DATA_HOME", home.path().join("data"));
        assert!(matches!(
            trash(&home.path().join("nope")),
            Err(OpError::Io { .. })
        ));
        ensure_trash(&home_trash()).unwrap();
        fs::write(home_trash().join("info/bad.trashinfo"), "garbage").unwrap();
        fs::write(
            home_trash().join("info/orphan.trashinfo"),
            "[Trash Info]\nPath=/x\n",
        )
        .unwrap();
        assert!(list().is_empty());
        std::env::remove_var("XDG_DATA_HOME");
    }

    #[test]
    fn percent_encoding_round_trips_arbitrary_bytes() {
        use std::os::unix::ffi::OsStrExt;
        let raw = std::ffi::OsStr::from_bytes(b"/tmp/a b/\xff%z\n");
        let path = Path::new(raw);
        assert_eq!(decode(&encode(path)), path);
        assert_eq!(decode("%zz%4"), Path::new("%zz%4"));
    }
}
