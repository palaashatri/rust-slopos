//! The text document: the buffer, undo and redo, the modified flag, loading,
//! atomic saving and search. Nothing here touches a window, so it is tested
//! directly.

use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use slopos_ui::controls::text_area::TextEdit;

/// The largest file the editor opens or saves: 1 MiB. Every edit is checked
/// against it, so the text can never grow past what the editor can handle.
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;

#[derive(Debug)]
pub enum DocumentError {
    /// The text or file is larger than [`MAX_FILE_BYTES`].
    TooLarge {
        size: u64,
    },
    /// The file is not UTF-8 text, or contains NUL bytes.
    NotText,
    /// The edit's range does not lie on char boundaries of the text.
    Invalid,
    Io(io::Error),
}

impl fmt::Display for DocumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge { size } => write!(
                f,
                "the file is {size} bytes; the editor opens files up to {MAX_FILE_BYTES} bytes (1 MiB)"
            ),
            Self::NotText => write!(f, "the file is not UTF-8 text"),
            Self::Invalid => write!(f, "the edit does not match the text"),
            Self::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for DocumentError {}

impl From<io::Error> for DocumentError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// One applied edit, kept so it can be undone and redone. Undo puts `removed`
/// back where `inserted` now is.
#[derive(Debug, Clone)]
struct Record {
    start: usize,
    removed: String,
    inserted: String,
    caret_before: usize,
}

#[derive(Debug, Clone)]
pub struct Document {
    path: Option<PathBuf>,
    text: String,
    history: Vec<Record>,
    /// How many records are applied; records past it are redo steps.
    applied: usize,
    /// The history position of the last load or save. `None` once that point
    /// can no longer be reached, so the document counts as modified.
    saved_at: Option<usize>,
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

impl Document {
    /// An empty, untitled document.
    pub fn new() -> Self {
        Self {
            path: None,
            text: String::new(),
            history: Vec::new(),
            applied: 0,
            saved_at: Some(0),
        }
    }

    /// Read a UTF-8 text file. Larger files and binary files are refused.
    pub fn open(path: &Path) -> Result<Self, DocumentError> {
        let size = fs::metadata(path)?.len();
        if size > MAX_FILE_BYTES {
            return Err(DocumentError::TooLarge { size });
        }
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(DocumentError::TooLarge {
                size: bytes.len() as u64,
            });
        }
        if bytes.contains(&0) {
            return Err(DocumentError::NotText);
        }
        let text = String::from_utf8(bytes).map_err(|_| DocumentError::NotText)?;
        Ok(Self {
            path: Some(path.to_owned()),
            text,
            history: Vec::new(),
            applied: 0,
            saved_at: Some(0),
        })
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// The file name, or "Untitled".
    pub fn title(&self) -> String {
        self.path.as_deref().and_then(Path::file_name).map_or_else(
            || "Untitled".into(),
            |name| name.to_string_lossy().into_owned(),
        )
    }

    /// True when the text differs from the last loaded or saved state.
    pub fn is_modified(&self) -> bool {
        self.saved_at != Some(self.applied)
    }

    pub fn can_undo(&self) -> bool {
        self.applied > 0
    }

    pub fn can_redo(&self) -> bool {
        self.applied < self.history.len()
    }

    /// Apply an edit from the text area. `caret_before` is where the caret
    /// was before the edit, and undo puts it back there. Returns the caret
    /// after the edit.
    ///
    /// Consecutive typing (single insertions with no newline, each starting
    /// where the last one ended) is merged into one undo step.
    pub fn apply(&mut self, edit: &TextEdit, caret_before: usize) -> Result<usize, DocumentError> {
        if edit.start > edit.end
            || edit.end > self.text.len()
            || !self.text.is_char_boundary(edit.start)
            || !self.text.is_char_boundary(edit.end)
        {
            return Err(DocumentError::Invalid);
        }
        let size = self.text.len() - (edit.end - edit.start) + edit.inserted.len();
        if size as u64 > MAX_FILE_BYTES {
            return Err(DocumentError::TooLarge { size: size as u64 });
        }

        // Redo steps are discarded by a new edit, and so is the save point
        // if it was among them.
        self.history.truncate(self.applied);
        if self.saved_at.is_some_and(|saved| saved > self.applied) {
            self.saved_at = None;
        }

        let removed = self.text[edit.start..edit.end].to_owned();
        self.text
            .replace_range(edit.start..edit.end, &edit.inserted);
        let caret_after = edit.caret_after();

        let typing =
            removed.is_empty() && !edit.inserted.is_empty() && !edit.inserted.contains('\n');
        if let Some(last) = self.history.last_mut() {
            if typing
                && last.removed.is_empty()
                && !last.inserted.is_empty()
                && !last.inserted.contains('\n')
                && last.start + last.inserted.len() == edit.start
            {
                last.inserted.push_str(&edit.inserted);
                // The merged step no longer matches the save point.
                if self.saved_at == Some(self.applied) {
                    self.saved_at = None;
                }
                return Ok(caret_after);
            }
        }
        self.history.push(Record {
            start: edit.start,
            removed,
            inserted: edit.inserted.clone(),
            caret_before,
        });
        self.applied += 1;
        Ok(caret_after)
    }

    /// Undo the last step. Returns the caret position to restore.
    pub fn undo(&mut self) -> Option<usize> {
        if !self.can_undo() {
            return None;
        }
        self.applied -= 1;
        let record = &self.history[self.applied];
        let end = record.start + record.inserted.len();
        self.text.replace_range(record.start..end, &record.removed);
        Some(record.caret_before)
    }

    /// Redo the step undone last. Returns the caret position after it.
    pub fn redo(&mut self) -> Option<usize> {
        if !self.can_redo() {
            return None;
        }
        let record = &self.history[self.applied];
        let end = record.start + record.removed.len();
        self.text.replace_range(record.start..end, &record.inserted);
        self.applied += 1;
        Some(record.start + record.inserted.len())
    }

    /// Save to the document's file. Fails with `Io` (kind `InvalidInput`) if
    /// the document has no file yet; use [`Self::save_as`] then.
    pub fn save(&mut self) -> Result<(), DocumentError> {
        let path = match self.path.clone() {
            Some(path) => path,
            None => return Err(io::Error::from(io::ErrorKind::InvalidInput).into()),
        };
        write_atomic(&path, self.text.as_bytes())?;
        self.saved_at = Some(self.applied);
        Ok(())
    }

    /// Save to `path` and make it the document's file.
    pub fn save_as(&mut self, path: PathBuf) -> Result<(), DocumentError> {
        write_atomic(&path, self.text.as_bytes())?;
        self.path = Some(path);
        self.saved_at = Some(self.applied);
        Ok(())
    }

    /// The 1-based line and column of byte `caret`. The column counts chars.
    pub fn line_col(&self, caret: usize) -> (usize, usize) {
        let before = &self.text[..caret.min(self.text.len())];
        let line = before.matches('\n').count() + 1;
        let start = before.rfind('\n').map_or(0, |newline| newline + 1);
        (line, before[start..].chars().count() + 1)
    }
}

/// The byte range of the next match of `needle` at or after `from`, wrapping
/// to the start of the text. Matching is case-sensitive.
pub fn find(text: &str, needle: &str, from: usize) -> Option<(usize, usize)> {
    if needle.is_empty() {
        return None;
    }
    let from = from.min(text.len());
    let from = (from..=text.len()).find(|&index| text.is_char_boundary(index))?;
    let at = text[from..]
        .find(needle)
        .map(|index| from + index)
        .or_else(|| text.find(needle))?;
    Some((at, at + needle.len()))
}

/// Write `bytes` to a temporary file in the same folder, then rename it over
/// `path`. The file is either the old one or the complete new one, never a
/// partial write. Permissions of an existing file are kept.
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
    let folder = match path.parent() {
        Some(folder) if !folder.as_os_str().is_empty() => folder,
        _ => Path::new("."),
    };
    let temp = folder.join(format!(
        ".{}.{}.tmp",
        name.to_string_lossy(),
        std::process::id()
    ));
    let _ = fs::remove_file(&temp);
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        if let Ok(metadata) = fs::metadata(path) {
            fs::set_permissions(&temp, metadata.permissions())?;
        }
        fs::rename(&temp, path)?;
        // Make the rename itself durable. Best effort: not every filesystem
        // allows syncing a folder.
        if let Ok(dir) = fs::File::open(folder) {
            let _ = dir.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insertion(at: usize, inserted: &str) -> TextEdit {
        TextEdit {
            start: at,
            end: at,
            inserted: inserted.into(),
        }
    }

    fn typed(document: &mut Document, at: usize, s: &str) -> usize {
        document.apply(&insertion(at, s), at).expect("edit applies")
    }

    #[test]
    fn apply_then_undo_and_redo_restore_the_text_and_caret() {
        let mut document = Document::new();
        typed(&mut document, 0, "hello");
        let caret = document
            .apply(
                &TextEdit {
                    start: 1,
                    end: 4,
                    inserted: String::new(),
                },
                5,
            )
            .unwrap();
        assert_eq!((document.text(), caret), ("ho", 1));
        assert_eq!(document.undo(), Some(5));
        assert_eq!(document.text(), "hello");
        assert_eq!(document.redo(), Some(1));
        assert_eq!(document.text(), "ho");
    }

    #[test]
    fn consecutive_typing_is_one_undo_step_but_a_newline_breaks_it() {
        let mut document = Document::new();
        typed(&mut document, 0, "a");
        typed(&mut document, 1, "b");
        typed(&mut document, 2, "c");
        assert_eq!(document.text(), "abc");
        assert_eq!(document.undo(), Some(0));
        assert_eq!(document.text(), "");
        assert!(!document.can_undo());

        let mut document = Document::new();
        typed(&mut document, 0, "a");
        typed(&mut document, 1, "\n");
        typed(&mut document, 2, "b");
        assert_eq!(document.undo(), Some(2));
        assert_eq!(document.text(), "a\n");
        assert_eq!(document.undo(), Some(1));
        assert_eq!(document.text(), "a");
    }

    #[test]
    fn a_new_edit_discards_the_redo_steps() {
        let mut document = Document::new();
        typed(&mut document, 0, "one");
        typed(&mut document, 3, "\ntwo");
        document.undo();
        assert!(document.can_redo());
        typed(&mut document, 3, "!");
        assert!(!document.can_redo());
        assert_eq!(document.text(), "one!");
    }

    #[test]
    fn modified_tracks_the_save_point_through_undo_and_redo() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.txt");
        let mut document = Document::new();
        typed(&mut document, 0, "x\n");
        document.save_as(path).unwrap();
        assert!(!document.is_modified());

        typed(&mut document, 2, "y");
        assert!(document.is_modified());
        document.undo();
        assert!(
            !document.is_modified(),
            "undo back to the saved text is clean"
        );
        document.redo();
        assert!(document.is_modified());
    }

    #[test]
    fn typing_after_a_save_is_a_change_even_when_it_merges_into_an_undo_step() {
        let dir = tempfile::tempdir().unwrap();
        let mut document = Document::new();
        typed(&mut document, 0, "a");
        document.save_as(dir.path().join("m.txt")).unwrap();
        typed(&mut document, 1, "b");
        assert!(document.is_modified());
        document.undo();
        assert!(
            document.is_modified(),
            "the saved state is no longer reachable by undo"
        );
    }

    #[test]
    fn undoing_back_to_the_saved_state_is_not_modified() {
        let dir = tempfile::tempdir().unwrap();
        let mut document = Document::new();
        typed(&mut document, 0, "x");
        document.save_as(dir.path().join("a.txt")).unwrap();
        document
            .apply(
                &TextEdit {
                    start: 1,
                    end: 1,
                    inserted: "\n".into(),
                },
                1,
            )
            .unwrap();
        assert!(document.is_modified());
        document.undo();
        assert!(!document.is_modified());
    }

    #[test]
    fn saving_clears_the_modified_flag_and_writes_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.txt");
        fs::write(&path, "old").unwrap();
        let mut document = Document::open(&path).unwrap();
        typed(&mut document, 3, " new");
        assert!(document.is_modified());
        document.save().unwrap();
        assert!(!document.is_modified());
        assert_eq!(fs::read_to_string(&path).unwrap(), "old new");
        // The atomic write leaves no temporary file behind.
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(leftovers, vec![std::ffi::OsString::from("doc.txt")]);
    }

    #[test]
    fn a_failed_save_keeps_the_old_file_and_the_modified_flag() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keep.txt");
        fs::write(&path, "original").unwrap();
        let mut document = Document::open(&path).unwrap();
        typed(&mut document, 8, "!");
        let missing = dir.path().join("no-such-folder").join("out.txt");
        assert!(document.save_as(missing).is_err());
        assert!(document.is_modified());
        assert!(document.path().is_some_and(|p| p == path));
        assert_eq!(fs::read_to_string(&path).unwrap(), "original");
    }

    #[test]
    fn save_without_a_file_asks_for_save_as() {
        let mut document = Document::new();
        assert!(matches!(document.save(), Err(DocumentError::Io(_))));
    }

    #[test]
    fn open_refuses_files_over_the_limit_and_binary_files() {
        let dir = tempfile::tempdir().unwrap();
        let big = dir.path().join("big.txt");
        fs::write(&big, vec![b'a'; MAX_FILE_BYTES as usize + 1]).unwrap();
        assert!(matches!(
            Document::open(&big),
            Err(DocumentError::TooLarge { size }) if size == MAX_FILE_BYTES + 1
        ));

        let limit = dir.path().join("limit.txt");
        fs::write(&limit, vec![b'a'; MAX_FILE_BYTES as usize]).unwrap();
        assert!(Document::open(&limit).is_ok(), "exactly the limit opens");

        let binary = dir.path().join("bin.dat");
        fs::write(&binary, b"ab\0cd").unwrap();
        assert!(matches!(
            Document::open(&binary),
            Err(DocumentError::NotText)
        ));

        let latin1 = dir.path().join("latin1.txt");
        fs::write(&latin1, [0xe9, b'a']).unwrap();
        assert!(matches!(
            Document::open(&latin1),
            Err(DocumentError::NotText)
        ));
    }

    #[test]
    fn edits_that_would_pass_the_limit_are_refused_and_change_nothing() {
        let mut document = Document::new();
        let filler = "a".repeat(MAX_FILE_BYTES as usize);
        typed(&mut document, 0, &filler);
        let result = document.apply(&insertion(0, "b"), 0);
        assert!(matches!(result, Err(DocumentError::TooLarge { .. })));
        assert_eq!(document.text().len(), MAX_FILE_BYTES as usize);
    }

    #[test]
    fn edits_off_char_boundaries_are_refused() {
        let mut document = Document::new();
        typed(&mut document, 0, "é");
        let result = document.apply(
            &TextEdit {
                start: 1,
                end: 1,
                inserted: "x".into(),
            },
            0,
        );
        assert!(matches!(result, Err(DocumentError::Invalid)));
    }

    #[test]
    fn undo_and_redo_handle_multi_byte_text() {
        let mut document = Document::new();
        typed(&mut document, 0, "€");
        typed(&mut document, 3, "é\n");
        assert_eq!(document.undo(), Some(3));
        assert_eq!(document.text(), "€");
        assert_eq!(document.redo(), Some(6));
        assert_eq!(document.text(), "€é\n");
    }

    #[test]
    fn line_and_column_count_from_one_and_chars() {
        let mut document = Document::new();
        typed(&mut document, 0, "ab\nécd");
        assert_eq!(document.line_col(0), (1, 1));
        assert_eq!(document.line_col(2), (1, 3));
        assert_eq!(document.line_col(3), (2, 1));
        // "é" is two bytes but one column.
        assert_eq!(document.line_col(6), (2, 3));
        assert_eq!(document.line_col(999), (2, 4));
    }

    #[test]
    fn find_wraps_and_handles_empty_needles() {
        let text = "one two one";
        assert_eq!(find(text, "one", 0), Some((0, 3)));
        assert_eq!(find(text, "one", 1), Some((8, 11)));
        assert_eq!(find(text, "one", 9), Some((0, 3)), "wraps to the start");
        assert_eq!(find(text, "three", 0), None);
        assert_eq!(find(text, "", 0), None);
        assert_eq!(
            find("aé", "é", 2),
            Some((1, 3)),
            "a start inside a char moves forward"
        );
    }

    #[test]
    fn title_uses_the_file_name_or_untitled() {
        assert_eq!(Document::new().title(), "Untitled");
        let dir = tempfile::tempdir().unwrap();
        let mut document = Document::new();
        document.save_as(dir.path().join("notes.md")).unwrap();
        assert_eq!(document.title(), "notes.md");
    }
}
