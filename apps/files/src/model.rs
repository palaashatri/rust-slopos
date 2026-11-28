//! Files: bounded, read-only filesystem enumeration and navigation state.
//! No deletes, copies, moves or directory symlink traversal in this module.
//!
//! Folders are read in pages of at most [`PAGE_SIZE`] entries, so a large
//! directory never produces one long blocking read. Each page is merged into
//! the sorted listing in [`DirectoryState`], and `more` stays set until the
//! last page has arrived.
use std::cmp::Ordering;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Entries per page read from the disk.
pub const PAGE_SIZE: usize = 1024;
/// Largest folder shown. Beyond this a listing fails with an explicit error
/// rather than showing a silently truncated set.
pub const MAX_ENTRIES: usize = 100_000;
/// While a listing is still loading, rebuild the visible rows only after this
/// many new entries, so each page costs a bounded amount of UI work.
pub const REFRESH_STEP: usize = 8192;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Directory,
    File,
    Symlink,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: OsString,
    pub path: PathBuf,
    pub kind: Kind,
    pub bytes: u64,
    pub modified: Option<std::time::SystemTime>,
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

/// The listing order: folders first, then case-insensitive name, then the
/// exact name so that the order is total.
///
/// Called once per comparison while pages are merged, so it must not
/// allocate: names are compared character by character, lowercased on the fly.
pub fn entry_order(a: &Entry, b: &Entry) -> Ordering {
    (a.kind != Kind::Directory)
        .cmp(&(b.kind != Kind::Directory))
        .then_with(|| cmp_names_ignoring_case(&a.name, &b.name))
        .then_with(|| a.name.cmp(&b.name))
}

/// Case-insensitive comparison of two names, lossy UTF-8 as the display shows
/// them. Borrows the names when they are valid UTF-8, so it does not allocate.
fn cmp_names_ignoring_case(a: &OsStr, b: &OsStr) -> Ordering {
    let a = a.to_string_lossy();
    let b = b.to_string_lossy();
    a.chars()
        .flat_map(char::to_lowercase)
        .cmp(b.chars().flat_map(char::to_lowercase))
}

/// Merge a sorted `page` into the sorted listing `into`, in linear time.
pub fn merge_sorted(into: &mut Vec<Entry>, page: Vec<Entry>) {
    if into.is_empty() {
        *into = page;
        return;
    }
    let old = std::mem::take(into);
    let mut merged = Vec::with_capacity(old.len() + page.len());
    let mut left = old.into_iter().peekable();
    let mut right = page.into_iter().peekable();
    loop {
        let take_left = match (left.peek(), right.peek()) {
            (Some(a), Some(b)) => entry_order(a, b) != Ordering::Greater,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => break,
        };
        let next = if take_left { left.next() } else { right.next() };
        merged.extend(next);
    }
    *into = merged;
}

/// One page of a folder listing, sorted by [`entry_order`]. `more` is true
/// when the folder may hold further entries.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Page {
    pub entries: Vec<Entry>,
    pub more: bool,
}

/// Streams a single directory in pages without recursing into children or
/// following symlinks.
pub struct Lister {
    reader: fs::ReadDir,
    read: usize,
    page_size: usize,
    limit: usize,
    done: bool,
}

impl Lister {
    pub fn open(path: &Path) -> io::Result<Self> {
        Self::with_limits(path, PAGE_SIZE, MAX_ENTRIES)
    }

    /// Like [`Lister::open`] with explicit page size and entry limit.
    pub fn with_limits(path: &Path, page_size: usize, limit: usize) -> io::Result<Self> {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.file_type().is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "not a directory (directory symlinks are deliberately not followed)",
            ));
        }
        Ok(Self {
            reader: fs::read_dir(path)?,
            read: 0,
            page_size: page_size.max(1),
            limit,
            done: false,
        })
    }

    /// Read the next page. Once `more` is false the listing is complete and
    /// further calls return empty pages.
    pub fn next_page(&mut self) -> io::Result<Page> {
        let mut entries = Vec::new();
        while !self.done && entries.len() < self.page_size {
            let Some(item) = self.reader.next() else {
                self.done = true;
                break;
            };
            let item = item?;
            self.read += 1;
            if self.read > self.limit {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "folder has more than {} items; it is not shown in full",
                        self.limit
                    ),
                ));
            }
            let path = item.path();
            let meta = fs::symlink_metadata(&path)?;
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
                path,
                kind,
                bytes: meta.len(),
                modified: meta.modified().ok(),
            });
        }
        entries.sort_by(entry_order);
        Ok(Page {
            entries,
            more: !self.done,
        })
    }
}

/// Read a *single* directory completely, page by page. Exactly-once entry
/// validation prevents a partial listing from pretending to be complete.
/// Test helper: the app streams pages rather than reading a whole folder.
#[cfg(test)]
pub fn read_directory(path: &Path) -> io::Result<Vec<Entry>> {
    let mut lister = Lister::open(path)?;
    let mut entries = Vec::new();
    loop {
        let page = lister.next_page()?;
        merge_sorted(&mut entries, page.entries);
        if !page.more {
            return Ok(entries);
        }
    }
}

/// Whether the visible rows should be rebuilt now, given the number of
/// entries loaded so far, the number the rows were last built from, and
/// whether more pages are coming.
pub fn should_refresh_rows(loaded: usize, shown: usize, more: bool) -> bool {
    !more || shown == 0 || loaded.saturating_sub(shown) >= REFRESH_STEP
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
    /// Pages are still arriving for `current`.
    pub more: bool,
    back: Vec<PathBuf>,
    forward: Vec<PathBuf>,
}

impl DirectoryState {
    pub fn new(initial: PathBuf) -> Self {
        Self {
            current: initial,
            entries: Vec::new(),
            more: false,
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
    #[cfg(test)]
    pub fn apply(&mut self, target: PathBuf, mode: Navigation, entries: Vec<Entry>) -> bool {
        if !self.begin(target, mode) {
            return false;
        }
        self.entries = entries;
        self.more = false;
        true
    }

    /// Start a paged listing of `target`: move the history and clear the
    /// entries. Returns false, changing nothing, for an invalid transition.
    pub fn begin(&mut self, target: PathBuf, mode: Navigation) -> bool {
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
        self.entries.clear();
        self.more = true;
        true
    }

    /// Merge the next page of the listing started by [`DirectoryState::begin`].
    pub fn extend(&mut self, page: Vec<Entry>, more: bool) {
        merge_sorted(&mut self.entries, page);
        self.more = more;
    }

    /// Drop a listing that failed part-way: no partial folder is presented
    /// as complete.
    pub fn abandon(&mut self) {
        self.entries.clear();
        self.more = false;
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
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

    fn fixture() -> PathBuf {
        static ID: AtomicU64 = AtomicU64::new(0);
        let base = std::env::temp_dir().join(format!(
            "slopos-files-model-{}-{}",
            std::process::id(),
            ID.fetch_add(1, AtomicOrdering::Relaxed)
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

    fn names(entries: &[Entry]) -> Vec<String> {
        entries.iter().map(Entry::display_name).collect()
    }

    #[test]
    fn large_folder_arrives_in_bounded_pages_that_merge_sorted() {
        let root = fixture();
        // 2500 files, page size 1000: pages of 1000, 1000 and 500.
        for index in 0..2500 {
            fs::write(root.join(format!("f{index:05}")), b"").unwrap();
        }
        fs::create_dir(root.join("zz-folder")).unwrap();
        let mut lister = Lister::with_limits(&root, 1000, MAX_ENTRIES).unwrap();
        let mut sizes = Vec::new();
        let mut merged = Vec::new();
        loop {
            let page = lister.next_page().unwrap();
            assert!(page.entries.len() <= 1000, "a page exceeded the page size");
            sizes.push(page.entries.len());
            // Each page is itself sorted.
            assert!(page
                .entries
                .windows(2)
                .all(|w| entry_order(&w[0], &w[1]) != Ordering::Greater));
            merge_sorted(&mut merged, page.entries);
            if !page.more {
                break;
            }
        }
        assert_eq!(sizes.len(), 3);
        assert_eq!(merged.len(), 2501);
        // The folder sorts first; the merged order is the full sort.
        assert_eq!(merged[0].display_name(), "zz-folder");
        let mut expected = read_directory(&root).unwrap();
        expected.sort_by(entry_order);
        assert_eq!(names(&merged), names(&expected));
        // After the end, further calls are empty and complete.
        let tail = lister.next_page().unwrap();
        assert!(tail.entries.is_empty() && !tail.more);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn name_order_ignores_case_then_breaks_ties_exactly() {
        let mut items = vec![entry("b"), entry("C"), entry("B"), entry("a")];
        items.sort_by(entry_order);
        assert_eq!(names(&items), ["a", "B", "b", "C"]);
        // Accented and non-ASCII letters fold too, and the order is total.
        let mut accented = vec![entry("É"), entry("e"), entry("é")];
        accented.sort_by(entry_order);
        assert_eq!(names(&accented), ["e", "É", "é"]);
        assert_eq!(entry_order(&entry("x"), &entry("x")), Ordering::Equal);
    }

    #[test]
    fn exceeding_the_entry_limit_is_an_explicit_error() {
        let root = fixture();
        for index in 0..5 {
            fs::write(root.join(format!("n{index}")), b"").unwrap();
        }
        let mut lister = Lister::with_limits(&root, 2, 4).unwrap();
        let mut failure = None;
        for _ in 0..4 {
            match lister.next_page() {
                Ok(_) => {}
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        }
        let error = failure.expect("the fifth entry must be refused");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("more than 4 items"));
        assert!(
            read_directory(&root).is_ok(),
            "the real limit is far larger"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn paged_navigation_matches_a_complete_apply() {
        let root = fixture();
        let mut paged = DirectoryState::new(root.clone());
        assert!(paged.begin(root.clone(), Navigation::Refresh));
        assert!(paged.more, "a listing in progress is marked as more");
        paged.extend(vec![entry("b"), entry("d")], true);
        paged.extend(vec![entry("a"), entry("c")], false);
        assert!(!paged.more);
        let mut whole = DirectoryState::new(root.clone());
        whole.apply(
            root.clone(),
            Navigation::Refresh,
            vec![entry("a"), entry("b"), entry("c"), entry("d")],
        );
        assert_eq!(names(&paged.entries), names(&whole.entries));
        // A rejected transition changes nothing, including the loading flag.
        assert!(!paged.begin(root.join("nowhere"), Navigation::Forward));
        assert_eq!(paged.entries.len(), 4);
        // Abandoning a failed listing shows nothing as if complete.
        paged.abandon();
        assert!(paged.entries.is_empty() && !paged.more);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rows_refresh_once_per_step_while_loading() {
        assert!(
            should_refresh_rows(10, 0, true),
            "the first page shows at once"
        );
        assert!(
            should_refresh_rows(10, 10, false),
            "the last page always shows"
        );
        assert!(!should_refresh_rows(2000, 1000, true));
        assert!(should_refresh_rows(1000 + REFRESH_STEP, 1000, true));
    }

    fn entry(name: &str) -> Entry {
        Entry {
            name: OsString::from(name),
            path: PathBuf::from(name),
            kind: Kind::File,
            bytes: 0,
            modified: None,
        }
    }
}
