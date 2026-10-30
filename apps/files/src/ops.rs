//! Transactional file operations: rename, new folder, copy, move and
//! permanent delete.
//!
//! Safety rules (docs/p1-x11-release.md, "Native-app implementation rules"):
//! - Copies are written under a private `.slopos-partial-*` name in the
//!   destination folder and committed with an atomic no-replace rename
//!   (`renameat2(RENAME_NOREPLACE)`). Cancelling or failing removes only that
//!   partial item; an existing target is never touched by a rollback.
//! - Replacing moves the old target aside first and removes it only after the
//!   new item is in place; if the swap fails the old target is put back.
//! - A cross-device move copies, commits, and only then removes the source.
//! - Symbolic links are copied as links and never followed; sockets, FIFOs
//!   and devices are refused.
//! - Copying or moving a folder into itself is refused.

use std::ffi::CString;
use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// How to resolve a name that already exists in the destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conflict {
    /// Stop with [`OpError::Exists`] so the UI can ask.
    Ask,
    Replace,
    KeepBoth,
    Skip,
}

#[derive(Debug, thiserror::Error)]
pub enum OpError {
    #[error("cancelled")]
    Cancelled,
    #[error("“{}” already exists", .0.display())]
    Exists(PathBuf),
    #[error("can't use the name “{0}”: {1}")]
    InvalidName(String, &'static str),
    #[error("can't put “{}” inside itself", .0.display())]
    IntoItself(PathBuf),
    #[error("“{}” is a special file and can't be copied", .0.display())]
    Unsupported(PathBuf),
    #[error("{}: {source}", .path.display())]
    Io { path: PathBuf, source: io::Error },
}

pub type OpResult<T> = Result<T, OpError>;

fn io_error(path: &Path) -> impl FnOnce(io::Error) -> OpError + '_ {
    move |source| OpError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Cancellation and progress shared between the UI and a worker.
#[derive(Default)]
pub struct Job {
    cancelled: AtomicBool,
    done_bytes: AtomicU64,
    total_bytes: AtomicU64,
    /// Called as bytes complete, at most every [`NOTIFY_INTERVAL`], so a UI
    /// repaints on real progress instead of polling.
    notify: std::sync::OnceLock<Box<dyn Fn() + Send + Sync>>,
    last_notify: std::sync::Mutex<Option<std::time::Instant>>,
}

const NOTIFY_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

impl std::fmt::Debug for Job {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Job")
            .field("cancelled", &self.is_cancelled())
            .field("fraction", &self.fraction())
            .finish()
    }
}

impl Job {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    fn check(&self) -> OpResult<()> {
        if self.is_cancelled() {
            Err(OpError::Cancelled)
        } else {
            Ok(())
        }
    }

    /// Completed fraction, when the total is known.
    pub fn fraction(&self) -> Option<f64> {
        let total = self.total_bytes.load(Ordering::Relaxed);
        (total > 0)
            .then(|| (self.done_bytes.load(Ordering::Relaxed) as f64 / total as f64).min(1.0))
    }

    fn add_done(&self, bytes: u64) {
        self.done_bytes.fetch_add(bytes, Ordering::Relaxed);
        let Some(notify) = self.notify.get() else {
            return;
        };
        let now = std::time::Instant::now();
        let mut last = self.last_notify.lock().unwrap_or_else(|p| p.into_inner());
        if last.is_none_or(|then| now - then >= NOTIFY_INTERVAL) {
            *last = Some(now);
            drop(last);
            notify();
        }
    }

    /// Install the progress callback (once).
    pub fn on_progress(&self, notify: impl Fn() + Send + Sync + 'static) {
        let _ = self.notify.set(Box::new(notify));
    }
}

/// Check a single file name typed by the user.
pub fn validate_name(name: &str) -> OpResult<()> {
    let reason = if name.is_empty() || name.trim().is_empty() {
        Some("the name is empty")
    } else if name == "." || name == ".." {
        Some("that name is reserved")
    } else if name.contains('/') {
        Some("names can't contain “/”")
    } else if name.contains('\0') {
        Some("names can't contain NUL")
    } else if name.len() > 255 {
        Some("the name is longer than 255 bytes")
    } else {
        None
    };
    match reason {
        Some(reason) => Err(OpError::InvalidName(name.to_owned(), reason)),
        None => Ok(()),
    }
}

/// `name` if free in `dir`, else "stem 2.ext", "stem 3.ext", …
pub fn unique_name(dir: &Path, name: &str) -> String {
    if fs::symlink_metadata(dir.join(name)).is_err() {
        return name.to_owned();
    }
    let (stem, extension) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (name, String::new()),
    };
    (2..)
        .map(|n| format!("{stem} {n}{extension}"))
        .find(|candidate| fs::symlink_metadata(dir.join(candidate)).is_err())
        .expect("an unused name exists")
}

/// Atomically rename `from` to `to`, failing with `EEXIST` instead of
/// replacing an existing `to`.
pub fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    let from_c = CString::new(from.as_os_str().as_bytes())?;
    let to_c = CString::new(to.as_os_str().as_bytes())?;
    // SAFETY: both strings are valid NUL-terminated paths for the call.
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            from_c.as_ptr(),
            libc::AT_FDCWD,
            to_c.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    // Some filesystems lack RENAME_NOREPLACE; fall back to a hard link,
    // which also fails when the target exists, for regular files only.
    if error.raw_os_error() == Some(libc::EINVAL) {
        let meta = fs::symlink_metadata(from)?;
        if meta.is_file() {
            fs::hard_link(from, to)?;
            return fs::remove_file(from);
        }
        if fs::symlink_metadata(to).is_ok() {
            return Err(io::Error::from_raw_os_error(libc::EEXIST));
        }
        return fs::rename(from, to);
    }
    Err(error)
}

/// Rename `path` within its folder. Returns the new path.
pub fn rename(path: &Path, new_name: &str) -> OpResult<PathBuf> {
    validate_name(new_name)?;
    let parent = path
        .parent()
        .ok_or_else(|| OpError::InvalidName(new_name.into(), "no parent"))?;
    let target = parent.join(new_name);
    if target == path {
        return Ok(target);
    }
    match rename_no_replace(path, &target) {
        Ok(()) => Ok(target),
        Err(error) if error.raw_os_error() == Some(libc::EEXIST) => Err(OpError::Exists(target)),
        Err(error) => Err(OpError::Io {
            path: path.to_path_buf(),
            source: error,
        }),
    }
}

/// Create "untitled folder" (or "untitled folder 2", …) in `dir`.
pub fn new_folder(dir: &Path) -> OpResult<PathBuf> {
    for _ in 0..8 {
        let path = dir.join(unique_name(dir, "untitled folder"));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            // Raced with another creator: pick the next free name.
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(io_error(dir)(error)),
        }
    }
    Err(OpError::Exists(dir.join("untitled folder")))
}

fn partial_path(dir: &Path, name: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    dir.join(format!(".slopos-partial-{}-{n}-{name}", std::process::id()))
}

/// Regular-file bytes under `path`, without following links (for progress).
pub fn tree_size(path: &Path, job: &Job) -> OpResult<u64> {
    job.check()?;
    let meta = fs::symlink_metadata(path).map_err(io_error(path))?;
    if !meta.is_dir() {
        // Only regular-file bytes are copied by [`copy_file`].
        return Ok(if meta.is_file() { meta.len() } else { 0 });
    }
    let mut total = 0;
    for entry in fs::read_dir(path).map_err(io_error(path))? {
        let entry = entry.map_err(io_error(path))?;
        total += tree_size(&entry.path(), job)?;
    }
    Ok(total)
}

/// Remove a tree without following symbolic links.
pub fn delete_permanently(path: &Path) -> OpResult<()> {
    let meta = fs::symlink_metadata(path).map_err(io_error(path))?;
    if meta.is_dir() {
        fs::remove_dir_all(path).map_err(io_error(path))
    } else {
        fs::remove_file(path).map_err(io_error(path))
    }
}

fn copy_file(source: &Path, target: &Path, mode: u32, job: &Job) -> OpResult<()> {
    let mut input = fs::File::open(source).map_err(io_error(source))?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(io_error(target))?;
    let mut buffer = vec![0u8; 256 * 1024];
    loop {
        job.check()?;
        let read = input.read(&mut buffer).map_err(io_error(source))?;
        if read == 0 {
            break;
        }
        output
            .write_all(&buffer[..read])
            .map_err(io_error(target))?;
        job.add_done(read as u64);
    }
    output.sync_all().map_err(io_error(target))?;
    fs::set_permissions(target, fs::Permissions::from_mode(mode & 0o7777)).map_err(io_error(target))
}

/// Copy `source` to the not-yet-existing `target`, never following links.
fn copy_tree(source: &Path, target: &Path, job: &Job) -> OpResult<()> {
    job.check()?;
    let meta = fs::symlink_metadata(source).map_err(io_error(source))?;
    let kind = meta.file_type();
    if kind.is_symlink() {
        let link = fs::read_link(source).map_err(io_error(source))?;
        symlink(&link, target).map_err(io_error(target))
    } else if kind.is_dir() {
        fs::create_dir(target).map_err(io_error(target))?;
        for entry in fs::read_dir(source).map_err(io_error(source))? {
            let entry = entry.map_err(io_error(source))?;
            copy_tree(&entry.path(), &target.join(entry.file_name()), job)?;
        }
        fs::set_permissions(target, fs::Permissions::from_mode(meta.mode() & 0o7777))
            .map_err(io_error(target))
    } else if kind.is_file() {
        copy_file(source, target, meta.mode(), job)
    } else {
        Err(OpError::Unsupported(source.to_path_buf()))
    }
}

/// Where `source` should land in `dest_dir`, per `conflict`. `None` skips.
fn resolve_target(
    source: &Path,
    dest_dir: &Path,
    conflict: Conflict,
) -> OpResult<Option<(PathBuf, bool)>> {
    let name = source
        .file_name()
        .ok_or_else(|| OpError::InvalidName(source.display().to_string(), "no file name"))?;
    let target = dest_dir.join(name);
    if fs::symlink_metadata(&target).is_err() {
        return Ok(Some((target, false)));
    }
    if target == source {
        // Duplicating in place always keeps both.
        let unique = unique_name(dest_dir, &name.to_string_lossy());
        return Ok(Some((dest_dir.join(unique), false)));
    }
    match conflict {
        Conflict::Ask => Err(OpError::Exists(target)),
        Conflict::Skip => Ok(None),
        Conflict::KeepBoth => Ok(Some((
            dest_dir.join(unique_name(dest_dir, &name.to_string_lossy())),
            false,
        ))),
        Conflict::Replace => Ok(Some((target, true))),
    }
}

fn refuse_into_itself(source: &Path, dest_dir: &Path) -> OpResult<()> {
    let is_dir = fs::symlink_metadata(source).is_ok_and(|meta| meta.is_dir());
    if !is_dir {
        return Ok(());
    }
    let source = fs::canonicalize(source).map_err(io_error(source))?;
    let dest = fs::canonicalize(dest_dir).map_err(io_error(dest_dir))?;
    if dest.starts_with(&source) {
        Err(OpError::IntoItself(source))
    } else {
        Ok(())
    }
}

/// Move the committed `partial` to `target`, replacing it if asked.
fn commit(partial: &Path, target: &Path, replace: bool) -> OpResult<()> {
    if !replace {
        return match rename_no_replace(partial, target) {
            Ok(()) => Ok(()),
            Err(error) => {
                let _ = delete_permanently(partial);
                if error.raw_os_error() == Some(libc::EEXIST) {
                    Err(OpError::Exists(target.to_path_buf()))
                } else {
                    Err(io_error(target)(error))
                }
            }
        };
    }
    let dir = target.parent().unwrap_or(Path::new("/"));
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let aside = partial_path(dir, &format!("replaced-{name}"));
    fs::rename(target, &aside).map_err(|error| {
        let _ = delete_permanently(partial);
        io_error(target)(error)
    })?;
    if let Err(error) = rename_no_replace(partial, target) {
        // Put the original back before reporting; never lose it.
        let _ = fs::rename(&aside, target);
        let _ = delete_permanently(partial);
        return Err(io_error(target)(error));
    }
    let _ = delete_permanently(&aside);
    Ok(())
}

/// Copy `sources` into `dest_dir`. Returns the paths created.
pub fn copy_into(
    sources: &[PathBuf],
    dest_dir: &Path,
    conflict: Conflict,
    job: &Job,
) -> OpResult<Vec<PathBuf>> {
    let mut total = 0;
    for source in sources {
        refuse_into_itself(source, dest_dir)?;
        total += tree_size(source, job)?;
    }
    job.total_bytes.store(total, Ordering::Relaxed);
    let mut created = Vec::new();
    for source in sources {
        let Some((target, replace)) = resolve_target(source, dest_dir, conflict)? else {
            continue;
        };
        let name = target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let partial = partial_path(dest_dir, &name);
        if let Err(error) = copy_tree(source, &partial, job) {
            let _ = delete_permanently(&partial);
            return Err(error);
        }
        commit(&partial, &target, replace)?;
        created.push(target);
    }
    Ok(created)
}

/// Move `sources` into `dest_dir`: an atomic rename on the same device, or
/// copy-commit-then-delete across devices. Returns the new paths.
pub fn move_into(
    sources: &[PathBuf],
    dest_dir: &Path,
    conflict: Conflict,
    job: &Job,
) -> OpResult<Vec<PathBuf>> {
    let mut moved = Vec::new();
    for source in sources {
        job.check()?;
        refuse_into_itself(source, dest_dir)?;
        if source.parent() == Some(dest_dir) {
            moved.push(source.clone());
            continue;
        }
        let Some((target, replace)) = resolve_target(source, dest_dir, conflict)? else {
            continue;
        };
        let same_device = match (fs::symlink_metadata(source), fs::metadata(dest_dir)) {
            (Ok(a), Ok(b)) => a.dev() == b.dev(),
            (Err(error), _) => return Err(io_error(source)(error)),
            (_, Err(error)) => return Err(io_error(dest_dir)(error)),
        };
        if same_device && !replace {
            match rename_no_replace(source, &target) {
                Ok(()) => {
                    moved.push(target);
                    continue;
                }
                Err(error) if error.raw_os_error() == Some(libc::EXDEV) => {}
                Err(error) if error.raw_os_error() == Some(libc::EEXIST) => {
                    return Err(OpError::Exists(target))
                }
                Err(error) => return Err(io_error(source)(error)),
            }
        }
        // Cross-device, or replacing: copy, commit, then remove the source.
        let name = target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let partial = partial_path(dest_dir, &name);
        if let Err(error) = copy_tree(source, &partial, job) {
            let _ = delete_permanently(&partial);
            return Err(error);
        }
        commit(&partial, &target, replace)?;
        delete_permanently(source)?;
        moved.push(target);
    }
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn names_are_validated_and_made_unique() {
        assert!(validate_name("ok.txt").is_ok());
        for bad in ["", "  ", ".", "..", "a/b", "x\0y"] {
            assert!(validate_name(bad).is_err(), "{bad:?}");
        }
        assert!(validate_name(&"x".repeat(256)).is_err());
        let tmp = dir();
        fs::write(tmp.path().join("a.txt"), b"").unwrap();
        fs::write(tmp.path().join("a 2.txt"), b"").unwrap();
        assert_eq!(unique_name(tmp.path(), "a.txt"), "a 3.txt");
        assert_eq!(unique_name(tmp.path(), "b"), "b");
        fs::write(tmp.path().join(".hidden"), b"").unwrap();
        assert_eq!(unique_name(tmp.path(), ".hidden"), ".hidden 2");
    }

    #[test]
    fn rename_never_overwrites() {
        let tmp = dir();
        let a = tmp.path().join("a");
        fs::write(&a, b"A").unwrap();
        fs::write(tmp.path().join("b"), b"B").unwrap();
        assert!(matches!(rename(&a, "b"), Err(OpError::Exists(_))));
        assert_eq!(fs::read(tmp.path().join("b")).unwrap(), b"B");
        assert_eq!(rename(&a, "c").unwrap(), tmp.path().join("c"));
        assert!(matches!(
            rename(&tmp.path().join("c"), "x/y"),
            Err(OpError::InvalidName(..))
        ));
        assert_eq!(
            new_folder(tmp.path()).unwrap(),
            tmp.path().join("untitled folder")
        );
        assert_eq!(
            new_folder(tmp.path()).unwrap(),
            tmp.path().join("untitled folder 2")
        );
    }

    #[test]
    fn copy_preserves_trees_links_and_modes() {
        let src = dir();
        let dst = dir();
        let tree = src.path().join("tree");
        fs::create_dir_all(tree.join("sub")).unwrap();
        fs::write(tree.join("sub/data.bin"), vec![7u8; 600_000]).unwrap();
        fs::set_permissions(tree.join("sub/data.bin"), fs::Permissions::from_mode(0o640)).unwrap();
        symlink("/etc/passwd", tree.join("link")).unwrap();
        let job = Job::default();
        let created =
            copy_into(std::slice::from_ref(&tree), dst.path(), Conflict::Ask, &job).unwrap();
        assert_eq!(created, [dst.path().join("tree")]);
        let copied = dst.path().join("tree");
        assert_eq!(
            fs::read(copied.join("sub/data.bin")).unwrap().len(),
            600_000
        );
        assert_eq!(
            fs::metadata(copied.join("sub/data.bin")).unwrap().mode() & 0o777,
            0o640
        );
        assert_eq!(
            fs::read_link(copied.join("link")).unwrap(),
            Path::new("/etc/passwd")
        );
        assert_eq!(job.fraction(), Some(1.0));
        // No partial leftovers.
        assert_eq!(names(dst.path()), ["tree"]);
    }

    #[test]
    fn conflicts_ask_skip_keep_both_and_replace() {
        let src = dir();
        let dst = dir();
        let file = src.path().join("f.txt");
        fs::write(&file, b"new").unwrap();
        fs::write(dst.path().join("f.txt"), b"old").unwrap();
        let job = Job::default();
        let ask = copy_into(std::slice::from_ref(&file), dst.path(), Conflict::Ask, &job);
        assert!(matches!(ask, Err(OpError::Exists(_))));
        assert_eq!(fs::read(dst.path().join("f.txt")).unwrap(), b"old");
        assert!(copy_into(
            std::slice::from_ref(&file),
            dst.path(),
            Conflict::Skip,
            &job
        )
        .unwrap()
        .is_empty());
        copy_into(
            std::slice::from_ref(&file),
            dst.path(),
            Conflict::KeepBoth,
            &job,
        )
        .unwrap();
        assert_eq!(fs::read(dst.path().join("f 2.txt")).unwrap(), b"new");
        copy_into(
            std::slice::from_ref(&file),
            dst.path(),
            Conflict::Replace,
            &job,
        )
        .unwrap();
        assert_eq!(fs::read(dst.path().join("f.txt")).unwrap(), b"new");
        assert_eq!(names(dst.path()), ["f 2.txt", "f.txt"]);
        // Duplicating in place keeps both.
        copy_into(std::slice::from_ref(&file), src.path(), Conflict::Ask, &job).unwrap();
        assert_eq!(names(src.path()), ["f 2.txt", "f.txt"]);
    }

    #[test]
    fn cancelled_copy_leaves_no_partial_and_keeps_existing_targets() {
        let src = dir();
        let dst = dir();
        let big = src.path().join("big");
        fs::write(&big, vec![1u8; 2_000_000]).unwrap();
        fs::write(dst.path().join("big"), b"keep me").unwrap();
        let job = Arc::new(Job::default());
        job.cancel();
        let result = copy_into(&[big], dst.path(), Conflict::Replace, &job);
        assert!(matches!(result, Err(OpError::Cancelled)));
        assert_eq!(names(dst.path()), ["big"]);
        assert_eq!(fs::read(dst.path().join("big")).unwrap(), b"keep me");
    }

    #[test]
    fn into_itself_and_special_files_are_refused() {
        let tmp = dir();
        let folder = tmp.path().join("folder");
        fs::create_dir_all(folder.join("inner")).unwrap();
        let job = Job::default();
        assert!(matches!(
            copy_into(
                std::slice::from_ref(&folder),
                &folder.join("inner"),
                Conflict::Ask,
                &job
            ),
            Err(OpError::IntoItself(_))
        ));
        assert!(matches!(
            move_into(std::slice::from_ref(&folder), &folder, Conflict::Ask, &job),
            Err(OpError::IntoItself(_))
        ));
        let fifo = tmp.path().join("pipe");
        let c = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        // SAFETY: valid path string.
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        let out = dir();
        assert!(matches!(
            copy_into(&[fifo], out.path(), Conflict::Ask, &job),
            Err(OpError::Unsupported(_))
        ));
        assert!(names(out.path()).is_empty());
    }

    #[test]
    fn move_renames_atomically_and_refuses_overwrite() {
        let src = dir();
        let dst = dir();
        let a = src.path().join("a");
        fs::write(&a, b"A").unwrap();
        fs::write(dst.path().join("a"), b"other").unwrap();
        let job = Job::default();
        assert!(matches!(
            move_into(std::slice::from_ref(&a), dst.path(), Conflict::Ask, &job),
            Err(OpError::Exists(_))
        ));
        assert!(a.exists());
        let moved = move_into(
            std::slice::from_ref(&a),
            dst.path(),
            Conflict::KeepBoth,
            &job,
        )
        .unwrap();
        assert_eq!(moved, [dst.path().join("a 2")]);
        assert!(!a.exists());
        fs::write(&a, b"A2").unwrap();
        move_into(
            std::slice::from_ref(&a),
            dst.path(),
            Conflict::Replace,
            &job,
        )
        .unwrap();
        assert_eq!(fs::read(dst.path().join("a")).unwrap(), b"A2");
        assert!(!a.exists());
        // Moving into the same folder is a no-op.
        let b = dst.path().join("a");
        assert_eq!(
            move_into(std::slice::from_ref(&b), dst.path(), Conflict::Ask, &job).unwrap(),
            [b]
        );
    }

    #[test]
    fn cross_device_move_copies_then_deletes_when_tmpfs_is_available() {
        // /dev/shm is usually a different filesystem from the temp dir.
        let shm = Path::new("/dev/shm");
        let src = dir();
        let Ok(meta) = fs::metadata(shm) else { return };
        if meta.dev() == fs::metadata(src.path()).unwrap().dev() {
            return;
        }
        let dst = tempfile::tempdir_in(shm).unwrap();
        let tree = src.path().join("t");
        fs::create_dir(&tree).unwrap();
        fs::write(tree.join("x"), b"x").unwrap();
        let job = Job::default();
        let moved =
            move_into(std::slice::from_ref(&tree), dst.path(), Conflict::Ask, &job).unwrap();
        assert_eq!(fs::read(moved[0].join("x")).unwrap(), b"x");
        assert!(!tree.exists());
    }
}
