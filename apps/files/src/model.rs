//! Files: bounded, read-only filesystem enumeration and navigation state.
//! No deletes, copies, moves or directory symlink traversal in this P1 slice.
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const MAX_ENTRIES: usize = 8192;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Directory,
    File,
    Symlink,
    Other,
}

/// Device and inode of an item, used to tell whether the item at a path is
/// still the one that was listed.
pub type Identity = (u64, u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: OsString,
    pub path: PathBuf,
    pub kind: Kind,
    pub bytes: u64,
    pub modified: Option<std::time::SystemTime>,
    pub identity: Identity,
}

impl Entry {
    pub fn display_name(&self) -> String {
        self.name.to_string_lossy().into_owned()
    }

    pub fn is_hidden(&self) -> bool {
        use std::os::unix::ffi::OsStrExt;
        self.name.as_bytes().first() == Some(&b'.')
    }

    pub fn kind_label(&self) -> &'static str {
        match self.kind {
            Kind::Directory => "Folder",
            Kind::File => "File",
            Kind::Symlink => "Link (not followed)",
            Kind::Other => "Other",
        }
    }
}

/// The identity of `path` itself. Symlinks are not followed.
pub fn identity_of(path: &Path) -> io::Result<Identity> {
    use std::os::unix::fs::MetadataExt;
    let metadata = fs::symlink_metadata(path)?;
    Ok((metadata.dev(), metadata.ino()))
}

/// The folder the app starts in, with symlinks and `..` resolved, so that
/// Up walks real parents. An unresolvable path is kept as given; the first
/// read then reports the error.
pub fn normalize_startup(path: PathBuf) -> PathBuf {
    fs::canonicalize(&path).unwrap_or(path)
}

/// Read a *single* directory without recursively traversing children or
/// following symlinks. Every folder on the path must be a real directory,
/// not only the last one. Exactly-once directory entry validation prevents a
/// partial listing from pretending to be a complete directory.
pub fn read_directory(path: &Path) -> io::Result<Vec<Entry>> {
    use std::os::unix::fs::MetadataExt;
    // Check each prefix built from components. A trailing slash makes
    // symlink_metadata follow a link, so the raw path's ancestors would skip it.
    let mut prefix = PathBuf::new();
    for component in path.components() {
        prefix.push(component);
        if matches!(component, std::path::Component::Normal(_))
            && fs::symlink_metadata(&prefix)?.file_type().is_symlink()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a folder on the path is a symlink (directory symlinks are deliberately not followed)",
            ));
        }
    }
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a directory (directory symlinks are deliberately not followed)",
        ));
    }
    let mut entries = Vec::new();
    for item in fs::read_dir(path)? {
        let item = item?;
        if entries.len() >= MAX_ENTRIES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "directory exceeds 8192 entries; pagination is required",
            ));
        }
        let meta = fs::symlink_metadata(item.path())?;
        let kind = if meta.file_type().is_symlink() {
            Kind::Symlink
        } else if meta.is_dir() {
            Kind::Directory
        } else if meta.is_file() {
            Kind::File
        } else {
            Kind::Other
        };
        entries.push(Entry {
            name: item.file_name(),
            path: item.path(),
            kind,
            bytes: meta.len(),
            modified: meta.modified().ok(),
            identity: (meta.dev(), meta.ino()),
        });
    }
    entries.sort_by(|a, b| {
        (a.kind != Kind::Directory)
            .cmp(&(b.kind != Kind::Directory))
            .then_with(|| {
                a.display_name()
                    .to_lowercase()
                    .cmp(&b.display_name().to_lowercase())
            })
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(entries)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Navigation {
    New,
    Back,
    Forward,
    Refresh,
}

#[derive(Debug, Clone)]
pub struct DirectoryState {
    pub current: PathBuf,
    pub entries: Vec<Entry>,
    back: Vec<PathBuf>,
    forward: Vec<PathBuf>,
}

impl DirectoryState {
    pub fn new(initial: PathBuf) -> Self {
        Self {
            current: initial,
            entries: Vec::new(),
            back: Vec::new(),
            forward: Vec::new(),
        }
    }

    pub fn back_target(&self) -> Option<&Path> {
        self.back.last().map(PathBuf::as_path)
    }

    pub fn forward_target(&self) -> Option<&Path> {
        self.forward.last().map(PathBuf::as_path)
    }

    /// Apply a successful, fully scanned listing. Never change the directory
    /// or history for failed I/O. Stale worker results are filtered by UI.
    pub fn apply(&mut self, target: PathBuf, mode: Navigation, entries: Vec<Entry>) -> bool {
        if target != self.current {
            match mode {
                Navigation::New => {
                    self.back.push(self.current.clone());
                    if self.back.len() > 128 {
                        self.back.remove(0);
                    }
                    self.forward.clear();
                }
                Navigation::Back if self.back.last() == Some(&target) => {
                    self.back.pop();
                    self.forward.push(self.current.clone());
                }
                Navigation::Forward if self.forward.last() == Some(&target) => {
                    self.forward.pop();
                    self.back.push(self.current.clone());
                }
                _ => return false,
            }
        }
        self.current = target;
        self.entries = entries;
        true
    }

    pub fn parent(&self) -> Option<PathBuf> {
        self.current
            .parent()
            .filter(|p| *p != self.current)
            .map(Path::to_path_buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn fixture() -> PathBuf {
        static ID: AtomicU64 = AtomicU64::new(0);
        let base = std::env::temp_dir().join(format!(
            "slopos-files-model-{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&base).unwrap();
        base
    }

    #[test]
    fn reads_only_immediate_children_and_sorts_directories_first() {
        let root = fixture();
        fs::create_dir(root.join("a-folder")).unwrap();
        fs::write(root.join("b-file"), b"abc").unwrap();
        let listing = read_directory(&root).unwrap();
        assert_eq!(listing.len(), 2);
        assert_eq!(listing[0].kind, Kind::Directory);
        assert_eq!(listing[0].display_name(), "a-folder");
        assert_eq!(listing[1].bytes, 3);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn symlink_directory_is_not_traversed() {
        use std::os::unix::fs::symlink;
        let root = fixture();
        let external = fixture();
        fs::write(external.join("private"), b"secret").unwrap();
        symlink(&external, root.join("shortcut")).unwrap();
        let listing = read_directory(&root).unwrap();
        assert_eq!(listing[0].kind, Kind::Symlink);
        assert_eq!(listing[0].kind_label(), "Link (not followed)");
        assert!(read_directory(&root.join("shortcut")).is_err());
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(external).unwrap();
    }

    #[test]
    fn failed_navigation_preserves_history_and_listing() {
        let root = fixture();
        let mut state = DirectoryState::new(root.clone());
        assert!(state.apply(root.clone(), Navigation::Refresh, vec![]));
        let next = root.join("child");
        assert!(state.apply(next.clone(), Navigation::New, vec![]));
        assert_eq!(state.back_target(), Some(root.as_path()));
        assert_eq!(state.parent(), Some(root.clone()));
        assert!(!state.apply(root.clone(), Navigation::Forward, vec![]));
        assert_eq!(state.current, next);
        assert!(state.apply(root.clone(), Navigation::Back, vec![]));
        assert_eq!(state.forward_target(), Some(next.as_path()));
        assert!(state.apply(next, Navigation::Forward, vec![]));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn filenames_preserve_non_utf8_bytes() {
        use std::os::unix::ffi::OsStringExt;
        let root = fixture();
        let name = OsString::from_vec(vec![0x66, 0xff, 0x6f]);
        fs::write(root.join(&name), b"x").unwrap();
        let rows = read_directory(&root).unwrap();
        assert_eq!(rows[0].name, name);
        assert!(rows[0].display_name().contains('\u{fffd}'));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn no_partial_data_is_returned_on_unreadable_target() {
        let root = fixture();
        assert!(read_directory(&root.join("missing")).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn intermediate_symlink_is_not_traversed() {
        use std::os::unix::fs::symlink;
        let root = fixture();
        fs::create_dir_all(root.join("real").join("sub")).unwrap();
        symlink(root.join("real"), root.join("link")).unwrap();
        assert!(read_directory(&root.join("real").join("sub")).is_ok());
        assert!(read_directory(&root.join("link").join("sub")).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_trailing_slash_does_not_hide_a_symlink_component() {
        use std::os::unix::fs::symlink;
        let root = fixture();
        fs::create_dir_all(root.join("real").join("sub")).unwrap();
        symlink(root.join("real"), root.join("link")).unwrap();
        let suffixed = PathBuf::from(format!("{}/", root.join("link").join("sub").display()));
        assert!(read_directory(&suffixed).is_err());
        assert!(read_directory(&root.join("real").join("sub")).is_ok());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn startup_folder_is_normalized_so_up_reaches_the_real_parent() {
        use std::os::unix::fs::symlink;
        let root = fixture();
        fs::create_dir_all(root.join("real").join("sub")).unwrap();
        symlink(root.join("real"), root.join("link")).unwrap();
        let start = normalize_startup(root.join("link").join("sub").join(".."));
        assert_eq!(start, fs::canonicalize(root.join("real")).unwrap());
        let state = DirectoryState::new(start);
        assert_eq!(state.parent(), Some(fs::canonicalize(&root).unwrap()));
        // A missing folder is kept as given, so the read reports the error.
        let missing = root.join("missing");
        assert_eq!(normalize_startup(missing.clone()), missing);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn identity_changes_when_the_item_is_replaced() {
        let root = fixture();
        fs::write(root.join("a"), b"old").unwrap();
        fs::write(root.join("c"), b"new").unwrap();
        let listed = read_directory(&root).unwrap();
        let before = listed.iter().find(|e| e.display_name() == "a").unwrap();
        assert_eq!(identity_of(&before.path).unwrap(), before.identity);
        // Replacing `a` with `c` keeps the name but not the item.
        fs::rename(root.join("c"), root.join("a")).unwrap();
        assert_ne!(identity_of(&before.path).unwrap(), before.identity);
        fs::remove_dir_all(root).unwrap();
    }
}
