//! Modal file chooser: open one or more files, open a folder, or save.
//!
//! The chooser shows a folder list, a path field and Up / Filter / Cancel /
//! accept buttons. Save mode adds a file-name field and an overwrite
//! confirmation. Directories sort first, then files, by case-insensitive
//! name. Hidden entries (leading dot) are left out. Filters limit which
//! files are shown; directories are always shown (folder mode lists
//! directories only). Large folders are capped at [`MAX_ENTRIES`] and say
//! so; unreadable folders show the error instead of an empty list.
//!
//! The pure parts (listing, file-name validation, save-target classification)
//! are plain functions so they can be tested without a window.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::button::{Button, ButtonOutcome};
use super::focus::FocusChain;
use super::list::{ListOutcome, ListView};
use super::text_field::{TextField, TextFieldOutcome};
use crate::event::{AccessibilityRequest, Event, Key, Modifiers};
use crate::foundation::geometry::Rect;
use crate::primitives::button_plate::ButtonPlateVariant;
use crate::render::{RasterSurface, RenderError};
use crate::text::{measure, Font};
use crate::theme::palette;

/// The most entries one folder listing keeps.
pub const MAX_ENTRIES: usize = 20_000;
/// The longest file name, in bytes, that most file systems accept.
pub const MAX_NAME_BYTES: usize = 255;
const ROW_HEIGHT: u32 = 18;
const FILTER_WIDTH: u32 = 120;

/// What the dialog is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChooserKind {
    /// Choose one file (or several when `multiple` is set).
    Open,
    /// Choose a folder: the folder being shown is the answer.
    OpenFolder,
    /// Choose a name for a new or existing file in the shown folder.
    Save,
}

/// A named file filter. Only files are filtered; folders always show.
#[derive(Clone)]
pub struct ChooserFilter {
    pub label: String,
    accepts: Arc<dyn Fn(&str) -> bool + Send + Sync>,
}

impl ChooserFilter {
    /// A filter that accepts the file names `accepts` returns true for.
    pub fn new(label: &str, accepts: impl Fn(&str) -> bool + Send + Sync + 'static) -> Self {
        Self {
            label: label.to_owned(),
            accepts: Arc::new(accepts),
        }
    }

    /// A filter for the given lower-case extensions (without dots).
    pub fn extensions(label: &str, extensions: &[String]) -> Self {
        let extensions = extensions.to_vec();
        Self::new(label, move |name| extension_in(name, &extensions))
    }

    pub fn accepts(&self, name: &str) -> bool {
        (self.accepts)(name)
    }
}

impl fmt::Debug for ChooserFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChooserFilter")
            .field("label", &self.label)
            .finish_non_exhaustive()
    }
}

fn extension_in(name: &str, extensions: &[String]) -> bool {
    Path::new(name)
        .extension()
        .map(|ext| ext.to_string_lossy().to_lowercase())
        .is_some_and(|ext| extensions.contains(&ext))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChooserEntry {
    pub name: String,
    pub is_dir: bool,
}

/// List `dir` for the chooser: sorted, hidden entries skipped. Files must be
/// accepted by `accepts`; with `dirs_only` files are left out entirely.
/// Returns the entries and whether the listing was truncated at
/// [`MAX_ENTRIES`].
pub fn list_entries(
    dir: &Path,
    accepts: &dyn Fn(&str) -> bool,
    dirs_only: bool,
) -> io::Result<(Vec<ChooserEntry>, bool)> {
    let mut entries = Vec::new();
    let mut truncated = false;
    for entry in fs::read_dir(dir)? {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        // Follow symlinks for the kind, but a dangling link is just a file.
        let is_dir = fs::metadata(entry.path()).is_ok_and(|meta| meta.is_dir());
        if !is_dir && (dirs_only || !accepts(&name)) {
            continue;
        }
        if entries.len() == MAX_ENTRIES {
            truncated = true;
            break;
        }
        entries.push(ChooserEntry { name, is_dir });
    }
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok((entries, truncated))
}

/// List `dir` with an optional extension filter (lower-case, without dots).
pub fn list_directory(
    dir: &Path,
    extensions: Option<&[String]>,
) -> io::Result<(Vec<ChooserEntry>, bool)> {
    list_entries(
        dir,
        &|name| extensions.is_none_or(|known| extension_in(name, known)),
        false,
    )
}

/// Why a typed file name cannot be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameError {
    Empty,
    /// Names are one path component; `/` would name a different folder.
    ContainsSlash,
    /// `.` and `..` name folders, never files.
    Reserved,
    ContainsNul,
    TooLong,
}

impl NameError {
    pub fn message(self) -> &'static str {
        match self {
            Self::Empty => "Enter a file name",
            Self::ContainsSlash => "A file name can't contain “/”",
            Self::Reserved => "That name is reserved",
            Self::ContainsNul => "A file name can't contain a NUL character",
            Self::TooLong => "That file name is too long",
        }
    }
}

/// Check that `name` is a single, usable file-name component.
pub fn validate_file_name(name: &str) -> Result<(), NameError> {
    if name.trim().is_empty() {
        Err(NameError::Empty)
    } else if name == "." || name == ".." {
        Err(NameError::Reserved)
    } else if name.contains('/') {
        Err(NameError::ContainsSlash)
    } else if name.contains('\0') {
        Err(NameError::ContainsNul)
    } else if name.len() > MAX_NAME_BYTES {
        Err(NameError::TooLong)
    } else {
        Ok(())
    }
}

/// What saving to `path` would replace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveTarget {
    /// Nothing is there: save without asking.
    New,
    /// A file (or a dangling link) is there: ask before replacing it.
    Replace,
    /// A folder is there: a save can't replace it.
    FolderInTheWay,
}

/// Classify the save target at `path` without writing anything. A dangling
/// symlink counts as something a save would replace.
pub fn classify_save_target(path: &Path) -> SaveTarget {
    match fs::metadata(path) {
        Ok(meta) if meta.is_dir() => SaveTarget::FolderInTheWay,
        Ok(_) => SaveTarget::Replace,
        Err(_) if fs::symlink_metadata(path).is_ok() => SaveTarget::Replace,
        Err(_) => SaveTarget::New,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChooserOutcome {
    Ignored,
    Changed,
    Chosen(PathBuf),
    /// Several files chosen with `multiple`.
    ChosenMany(Vec<PathBuf>),
    Cancelled,
    Copy(String),
    PasteRequested,
}

const ID_LIST: u64 = 1;
const ID_PATH: u64 = 2;
const ID_UP: u64 = 3;
const ID_CANCEL: u64 = 4;
const ID_ACCEPT: u64 = 5;
const ID_NAME: u64 = 6;
const ID_FILTER: u64 = 7;

#[derive(Debug)]
pub struct FileChooser {
    pub id: u64,
    pub title: String,
    pub font: Font,
    kind: ChooserKind,
    multiple: bool,
    filters: Vec<ChooserFilter>,
    active_filter: usize,
    dir: PathBuf,
    entries: Vec<ChooserEntry>,
    status: Option<String>,
    /// The file a save would replace, while the overwrite question is open.
    confirm: Option<PathBuf>,
    list: ListView,
    path: TextField,
    name: TextField,
    filter_button: Button,
    up: Button,
    cancel: Button,
    accept: Button,
    focus: FocusChain,
    bounds: Rect,
    accept_label: String,
}

impl FileChooser {
    /// An open dialog. `extensions` (lower-case, without dots) limits the
    /// files shown; `None` shows every file.
    pub fn new(id: u64, title: &str, dir: &Path, extensions: Option<Vec<String>>) -> Self {
        let filters = extensions
            .map(|exts| vec![ChooserFilter::extensions("Supported files", &exts)])
            .unwrap_or_default();
        Self::build(id, title, dir, ChooserKind::Open, false, filters, None)
    }

    /// An open dialog with named filters (the first is active) and optional
    /// multiple selection.
    pub fn open_with_filters(
        id: u64,
        title: &str,
        dir: &Path,
        filters: Vec<ChooserFilter>,
        multiple: bool,
    ) -> Self {
        Self::build(id, title, dir, ChooserKind::Open, multiple, filters, None)
    }

    /// A dialog that chooses a folder.
    pub fn select_folder(id: u64, title: &str, dir: &Path) -> Self {
        Self::build(
            id,
            title,
            dir,
            ChooserKind::OpenFolder,
            false,
            Vec::new(),
            None,
        )
    }

    /// A save dialog with `suggested_name` in the name field.
    pub fn save(
        id: u64,
        title: &str,
        dir: &Path,
        filters: Vec<ChooserFilter>,
        suggested_name: &str,
    ) -> Self {
        Self::build(
            id,
            title,
            dir,
            ChooserKind::Save,
            false,
            filters,
            Some(suggested_name),
        )
    }

    /// Start with the filter at `index` active; out-of-range indexes are ignored.
    pub fn with_active_filter(mut self, index: usize) -> Self {
        if index < self.filters.len() {
            self.active_filter = index;
            let dir = self.dir.clone();
            self.navigate(&dir);
        }
        self
    }

    /// Replace the accept button's label (for example a portal's "Save As").
    pub fn with_accept_label(mut self, label: &str) -> Self {
        self.accept_label = label.to_owned();
        self.accept.label = label.to_owned();
        self
    }

    fn build(
        id: u64,
        title: &str,
        dir: &Path,
        kind: ChooserKind,
        multiple: bool,
        filters: Vec<ChooserFilter>,
        suggested_name: Option<&str>,
    ) -> Self {
        let accept_label = match kind {
            ChooserKind::Open => "Open",
            ChooserKind::OpenFolder => "Select",
            ChooserKind::Save => "Save",
        };
        let mut chooser = Self {
            id,
            title: title.to_owned(),
            font: Font::system(12.0),
            kind,
            multiple,
            filters,
            active_filter: 0,
            dir: PathBuf::new(),
            entries: Vec::new(),
            status: None,
            confirm: None,
            list: ListView::new(id + ID_LIST, "Folder contents", ROW_HEIGHT),
            path: TextField::new(id + ID_PATH, "Path"),
            name: TextField::new(id + ID_NAME, "File name"),
            filter_button: Button::new(id + ID_FILTER, "Filter", ButtonPlateVariant::Regular),
            up: Button::new(id + ID_UP, "Up", ButtonPlateVariant::Regular),
            cancel: Button::new(id + ID_CANCEL, "Cancel", ButtonPlateVariant::Regular),
            accept: Button::new(id + ID_ACCEPT, accept_label, ButtonPlateVariant::Default),
            focus: FocusChain::new(Vec::new()),
            bounds: Rect::default(),
            accept_label: accept_label.to_owned(),
        };
        if let Some(name) = suggested_name {
            chooser.name.set_text(name);
        }
        chooser.navigate(dir);
        let first = if kind == ChooserKind::Save {
            id + ID_NAME
        } else {
            id + ID_LIST
        };
        chooser.rebuild_focus(first);
        chooser
    }

    pub fn kind(&self) -> ChooserKind {
        self.kind
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn entries(&self) -> &[ChooserEntry] {
        &self.entries
    }

    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    /// The name typed in the save dialog's name field.
    pub fn file_name(&self) -> &str {
        self.name.text()
    }

    /// The active filter's label, if there are filters.
    pub fn filter_label(&self) -> Option<&str> {
        self.filters
            .get(self.active_filter)
            .map(|f| f.label.as_str())
    }

    /// True while the "replace existing file?" question is open.
    pub fn is_confirming_overwrite(&self) -> bool {
        self.confirm.is_some()
    }

    fn has_filter_button(&self) -> bool {
        self.filters.len() > 1
    }

    /// Show `dir`; on failure keep the path and show the error.
    pub fn navigate(&mut self, dir: &Path) {
        self.dir = dir.to_path_buf();
        self.path.set_text(&dir.to_string_lossy());
        self.relist();
    }

    fn relist(&mut self) {
        let filter = self.filters.get(self.active_filter).cloned();
        let accepts = |name: &str| filter.as_ref().is_none_or(|f| f.accepts(name));
        let dirs_only = self.kind == ChooserKind::OpenFolder;
        match list_entries(&self.dir, &accepts, dirs_only) {
            Ok((entries, truncated)) => {
                self.status = truncated.then(|| format!("Showing the first {MAX_ENTRIES} items"));
                self.entries = entries;
            }
            Err(error) => {
                self.status = Some(format!("Can't open this folder: {error}"));
                self.entries.clear();
            }
        }
        self.list.reset(self.entries.len());
        if !self.entries.is_empty() {
            self.list.select_only(0);
        }
    }

    /// Cycle to the next filter and show the folder again.
    fn cycle_filter(&mut self) -> ChooserOutcome {
        if self.has_filter_button() {
            self.active_filter = (self.active_filter + 1) % self.filters.len();
            let dir = self.dir.clone();
            self.navigate(&dir);
        }
        ChooserOutcome::Changed
    }

    /// Open the entry at `index`: enter folders, choose files.
    fn open_entry(&mut self, index: usize) -> ChooserOutcome {
        let Some(entry) = self.entries.get(index).cloned() else {
            return ChooserOutcome::Changed;
        };
        let path = self.dir.join(&entry.name);
        if entry.is_dir {
            self.navigate(&path);
            ChooserOutcome::Changed
        } else {
            ChooserOutcome::Chosen(path)
        }
    }

    fn open_typed_path(&mut self) -> ChooserOutcome {
        let typed = PathBuf::from(self.path.text().trim());
        let path = if typed.is_absolute() {
            typed
        } else {
            self.dir.join(typed)
        };
        match fs::metadata(&path) {
            Ok(meta) if meta.is_dir() => {
                self.navigate(&path);
                ChooserOutcome::Changed
            }
            Ok(_) if self.kind == ChooserKind::OpenFolder => {
                self.status = Some(format!("{} is not a folder", path.display()));
                ChooserOutcome::Changed
            }
            Ok(_) => ChooserOutcome::Chosen(path),
            Err(error) => {
                self.status = Some(format!("{}: {error}", path.display()));
                ChooserOutcome::Changed
            }
        }
    }

    fn go_up(&mut self) {
        if let Some(parent) = self.dir.parent().map(Path::to_path_buf) {
            let child = self
                .dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned());
            self.navigate(&parent);
            if let Some(index) =
                child.and_then(|name| self.entries.iter().position(|e| e.name == name))
            {
                self.list.select_only(index);
            }
        }
    }

    /// The accept button in browsing mode.
    fn accept(&mut self) -> ChooserOutcome {
        match self.kind {
            ChooserKind::OpenFolder => ChooserOutcome::Chosen(self.dir.clone()),
            ChooserKind::Save => self.request_save(),
            ChooserKind::Open if self.multiple => self.accept_selection(),
            ChooserKind::Open => match self.list.cursor() {
                Some(index) => self.open_entry(index),
                None => self.open_typed_path(),
            },
        }
    }

    /// Choose the selected files (or the cursor file) of a multiple-choice
    /// dialog. With nothing selected the typed path is used.
    fn accept_selection(&mut self) -> ChooserOutcome {
        let mut indices = self.list.selection();
        if indices.is_empty() {
            indices.extend(self.list.cursor());
        }
        let files: Vec<PathBuf> = indices
            .into_iter()
            .filter_map(|index| self.entries.get(index))
            .filter(|entry| !entry.is_dir)
            .map(|entry| self.dir.join(&entry.name))
            .collect();
        match files.len() {
            0 => match self.list.cursor() {
                Some(index) => self.open_entry(index),
                None => self.open_typed_path(),
            },
            1 => ChooserOutcome::Chosen(files.into_iter().next().unwrap_or_default()),
            _ => ChooserOutcome::ChosenMany(files),
        }
    }

    fn request_save(&mut self) -> ChooserOutcome {
        let name = self.name.text().to_owned();
        if let Err(error) = validate_file_name(&name) {
            self.status = Some(error.message().to_owned());
            return ChooserOutcome::Changed;
        }
        let target = self.dir.join(&name);
        match classify_save_target(&target) {
            SaveTarget::New => ChooserOutcome::Chosen(target),
            SaveTarget::Replace => {
                self.status = None;
                self.confirm = Some(target);
                self.enter_confirm();
                ChooserOutcome::Changed
            }
            SaveTarget::FolderInTheWay => {
                self.status = Some(format!("“{name}” is a folder"));
                ChooserOutcome::Changed
            }
        }
    }

    /// Switch the bottom buttons to Cancel / Replace and move focus to
    /// Cancel, so Enter does not replace by accident.
    fn enter_confirm(&mut self) {
        self.accept.label = "Replace".into();
        self.rebuild_focus(self.id + ID_CANCEL);
    }

    fn leave_confirm(&mut self) {
        self.confirm = None;
        self.accept.label = self.accept_label.clone();
        self.rebuild_focus(self.id + ID_NAME);
    }

    /// The focus order for the current state, with `focus` focused.
    fn rebuild_focus(&mut self, focus: u64) {
        let mut ids = Vec::new();
        if self.confirm.is_some() {
            ids.extend([self.id + ID_CANCEL, self.id + ID_ACCEPT]);
        } else {
            ids.push(self.id + ID_LIST);
            ids.push(self.id + ID_PATH);
            if self.kind == ChooserKind::Save {
                ids.push(self.id + ID_NAME);
            }
            if self.has_filter_button() {
                ids.push(self.id + ID_FILTER);
            }
            ids.extend([self.id + ID_UP, self.id + ID_CANCEL, self.id + ID_ACCEPT]);
        }
        self.focus.set_ids(ids);
        if !self.focus.focus(focus) {
            if let Some(first) = self.focus.ids().first().copied() {
                self.focus.focus(first);
            }
        }
        self.sync_focus();
    }

    fn sync_focus(&mut self) {
        let focused = self.focus.focused();
        self.list.focused = focused == Some(self.list.id);
        self.path.focused = focused == Some(self.path.id);
        self.name.focused = focused == Some(self.name.id);
        self.filter_button.focused = focused == Some(self.filter_button.id);
        self.up.focused = focused == Some(self.up.id);
        self.cancel.focused = focused == Some(self.cancel.id);
        self.accept.focused = focused == Some(self.accept.id);
    }

    /// Lay out to fill most of a window of logical size `window`.
    pub fn layout(&mut self, window: (u32, u32)) {
        let width = window.0.saturating_sub(40).clamp(240, 560);
        let height = window.1.saturating_sub(40).clamp(200, 440);
        let x = (window.0 as i32 - width as i32) / 2;
        let y = (window.1 as i32 - height as i32) / 2;
        self.bounds = Rect {
            x,
            y,
            width,
            height,
        };
        let inner_x = x + 12;
        let inner_width = width - 24;
        self.up.bounds = Rect {
            x: inner_x + inner_width as i32 - 50,
            y: y + 30,
            width: 50,
            height: 20,
        };
        self.path.bounds = Rect {
            x: inner_x,
            y: y + 29,
            width: inner_width - 60,
            height: 22,
        };
        let list_top = y + 60;
        let buttons_y = y + height as i32 - 32;
        let status_space = 18;
        // Save mode puts the name field between the list and the buttons.
        let name_y = buttons_y - 28;
        self.name.bounds = Rect {
            x: inner_x,
            y: name_y,
            width: inner_width,
            height: 22,
        };
        let list_bottom = if self.kind == ChooserKind::Save {
            name_y
        } else {
            buttons_y
        } - status_space;
        self.list.set_bounds(Rect {
            x: inner_x + 1,
            y: list_top,
            width: inner_width - 2,
            height: (list_bottom - list_top).max(ROW_HEIGHT as i32) as u32,
        });
        self.filter_button.bounds = Rect {
            x: inner_x,
            y: buttons_y,
            width: FILTER_WIDTH,
            height: 20,
        };
        self.accept.bounds = Rect {
            x: x + width as i32 - 12 - 80,
            y: buttons_y,
            width: 80,
            height: 20,
        };
        self.cancel.bounds = Rect {
            x: self.accept.bounds.x - 90,
            y: buttons_y,
            width: 80,
            height: 20,
        };
    }

    pub fn handle(&mut self, event: &Event, modifiers: Modifiers) -> ChooserOutcome {
        if self.confirm.is_some() {
            return self.handle_confirm(event);
        }
        if self.focus.handle(event).is_some() {
            self.sync_focus();
            return ChooserOutcome::Changed;
        }
        match event {
            Event::KeyPressed {
                key: Key::Escape, ..
            } => return ChooserOutcome::Cancelled,
            Event::KeyPressed {
                key: Key::Up,
                modifiers,
            } if modifiers.alt => {
                self.go_up();
                return ChooserOutcome::Changed;
            }
            Event::KeyPressed {
                key: Key::Backspace,
                ..
            } if self.list.focused => {
                self.go_up();
                return ChooserOutcome::Changed;
            }
            Event::Accessibility(AccessibilityRequest::Focus(id)) => {
                if self.focus.focus(*id) {
                    self.sync_focus();
                }
                return ChooserOutcome::Changed;
            }
            Event::Accessibility(AccessibilityRequest::Click(id)) if *id == self.up.id => {
                self.go_up();
                return ChooserOutcome::Changed;
            }
            Event::Accessibility(AccessibilityRequest::Click(id)) if *id == self.cancel.id => {
                return ChooserOutcome::Cancelled
            }
            Event::Accessibility(AccessibilityRequest::Click(id)) if *id == self.accept.id => {
                return self.accept();
            }
            Event::Accessibility(AccessibilityRequest::Click(id))
                if *id == self.filter_button.id =>
            {
                return self.cycle_filter();
            }
            Event::Accessibility(AccessibilityRequest::Click(id)) => {
                if let Some(index) = self.list.row_for_node(*id) {
                    return self.open_entry(index);
                }
            }
            _ => {}
        }
        let entries = &self.entries;
        let label = |index: usize| {
            entries
                .get(index)
                .map(|e| e.name.clone())
                .unwrap_or_default()
        };
        match self.list.handle(event, modifiers, &label) {
            ListOutcome::Activated(index) => return self.open_entry(index),
            ListOutcome::FocusRequested => {
                self.focus.focus(self.list.id);
                self.sync_focus();
                return ChooserOutcome::Changed;
            }
            ListOutcome::Ignored => {}
            _ => return ChooserOutcome::Changed,
        }
        match self.path.handle(event) {
            TextFieldOutcome::Ignored => {}
            TextFieldOutcome::Submitted => return self.open_typed_path(),
            TextFieldOutcome::Cancelled => return ChooserOutcome::Cancelled,
            TextFieldOutcome::Copy(text) => return ChooserOutcome::Copy(text),
            TextFieldOutcome::PasteRequested => return ChooserOutcome::PasteRequested,
            TextFieldOutcome::FocusRequested => {
                self.focus.focus(self.path.id);
                self.sync_focus();
                return ChooserOutcome::Changed;
            }
            _ => return ChooserOutcome::Changed,
        }
        if self.kind == ChooserKind::Save {
            match self.name.handle(event) {
                TextFieldOutcome::Ignored => {}
                TextFieldOutcome::Submitted => return self.request_save(),
                TextFieldOutcome::Cancelled => return ChooserOutcome::Cancelled,
                TextFieldOutcome::Copy(text) => return ChooserOutcome::Copy(text),
                TextFieldOutcome::PasteRequested => return ChooserOutcome::PasteRequested,
                TextFieldOutcome::FocusRequested => {
                    self.focus.focus(self.name.id);
                    self.sync_focus();
                    return ChooserOutcome::Changed;
                }
                _ => return ChooserOutcome::Changed,
            }
        }
        if self.has_filter_button() && self.filter_button.handle(event) == ButtonOutcome::Activated
        {
            return self.cycle_filter();
        }
        if self.up.handle(event) == ButtonOutcome::Activated {
            self.go_up();
            return ChooserOutcome::Changed;
        }
        if self.cancel.handle(event) == ButtonOutcome::Activated {
            return ChooserOutcome::Cancelled;
        }
        if self.accept.handle(event) == ButtonOutcome::Activated {
            return self.accept();
        }
        // Modal: swallow everything else.
        ChooserOutcome::Changed
    }

    /// The overwrite question: Cancel goes back to the save dialog, Escape
    /// does too, and Replace chooses the file.
    fn handle_confirm(&mut self, event: &Event) -> ChooserOutcome {
        if self.focus.handle(event).is_some() {
            self.sync_focus();
            return ChooserOutcome::Changed;
        }
        let target = self.confirm.clone();
        match event {
            Event::KeyPressed {
                key: Key::Escape, ..
            } => {
                self.leave_confirm();
                return ChooserOutcome::Changed;
            }
            Event::Accessibility(AccessibilityRequest::Focus(id)) => {
                if self.focus.focus(*id) {
                    self.sync_focus();
                }
                return ChooserOutcome::Changed;
            }
            Event::Accessibility(AccessibilityRequest::Click(id)) if *id == self.cancel.id => {
                self.leave_confirm();
                return ChooserOutcome::Changed;
            }
            Event::Accessibility(AccessibilityRequest::Click(id)) if *id == self.accept.id => {
                return target.map_or(ChooserOutcome::Changed, ChooserOutcome::Chosen);
            }
            _ => {}
        }
        if self.cancel.handle(event) == ButtonOutcome::Activated {
            self.leave_confirm();
            return ChooserOutcome::Changed;
        }
        if self.accept.handle(event) == ButtonOutcome::Activated {
            return target.map_or(ChooserOutcome::Changed, ChooserOutcome::Chosen);
        }
        ChooserOutcome::Changed
    }

    pub fn paint(&mut self, surface: &RasterSurface) -> Result<(), RenderError> {
        let colors = palette();
        surface.fill_rect(
            Rect {
                x: 0,
                y: 0,
                width: surface.logical_width(),
                height: surface.logical_height(),
            },
            colors.scrim,
        )?;
        let bounds = self.bounds;
        surface.fill_rect(
            Rect {
                x: bounds.x + 3,
                y: bounds.y + 3,
                ..bounds
            },
            colors.bevel_shadow,
        )?;
        surface.fill_rect(bounds, colors.surface)?;
        surface.stroke_rect(bounds, colors.border)?;
        surface.draw_text(
            &self.title,
            bounds.x + 12,
            bounds.y + 8,
            &self.font.clone().bold(),
            colors.ink,
        )?;
        self.path.paint(surface)?;
        self.up.paint(surface, &self.font)?;
        let font = &self.font;
        let entries = &self.entries;
        let line_height = measure("Ag", font).height as i32;
        self.list.paint(surface, |surface, index, rect, _| {
            let entry = &entries[index];
            let y = rect.y + (rect.height as i32 - line_height) / 2;
            // A small folder or document glyph drawn with rectangles.
            let glyph = Rect {
                x: rect.x + 6,
                y: rect.y + 4,
                width: 12,
                height: 10,
            };
            if entry.is_dir {
                surface.fill_rect(glyph, colors.thumb)?;
                surface.fill_rect(
                    Rect {
                        width: 5,
                        height: 2,
                        y: glyph.y - 2,
                        ..glyph
                    },
                    colors.thumb,
                )?;
            } else {
                surface.fill_rect(Rect { width: 9, ..glyph }, colors.content)?;
            }
            surface.stroke_rect(
                if entry.is_dir {
                    glyph
                } else {
                    Rect { width: 9, ..glyph }
                },
                colors.ink,
            )?;
            surface.draw_text(&entry.name, rect.x + 24, y, font, colors.ink)
        })?;
        if self.kind == ChooserKind::Save {
            self.name.paint(surface)?;
        }
        if self.has_filter_button() {
            self.filter_button.paint(surface, &self.font)?;
        }
        let list = self.list.bounds();
        if let Some(target) = &self.confirm {
            let name = target
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            surface.draw_text(
                &format!("“{name}” already exists. Replace it?"),
                list.x,
                list.y + list.height as i32 + 2,
                &self.font,
                colors.danger,
            )?;
        } else if let Some(status) = &self.status {
            surface.draw_text(
                status,
                list.x,
                list.y + list.height as i32 + 2,
                &self.font,
                colors.danger,
            )?;
        } else if self.entries.is_empty() {
            surface.draw_text(
                "This folder is empty",
                list.x + 8,
                list.y + 6,
                &self.font,
                colors.ink_secondary,
            )?;
        }
        self.cancel.paint(surface, &self.font)?;
        self.accept.paint(surface, &self.font)?;
        Ok(())
    }

    pub fn focus_node(&self) -> u64 {
        if self.list.focused {
            self.list.focus_node()
        } else {
            self.focus.focused().unwrap_or(self.id)
        }
    }

    pub fn accessibility_nodes(&self) -> Vec<(u64, accesskit::Node)> {
        let mut dialog = accesskit::Node::new(accesskit::Role::Dialog);
        dialog.set_label(self.title.as_str());
        dialog.set_modal();
        dialog.set_bounds(super::bounds_of(self.bounds));
        if self.confirm.is_some() {
            let nodes = vec![
                (self.cancel.id, self.cancel.accessibility_node()),
                (self.accept.id, self.accept.accessibility_node()),
            ];
            dialog.set_children(vec![
                accesskit::NodeId(self.cancel.id),
                accesskit::NodeId(self.accept.id),
            ]);
            let mut nodes = nodes;
            nodes.insert(0, (self.id, dialog));
            return nodes;
        }
        let entries = &self.entries;
        let label = |index: usize| {
            entries
                .get(index)
                .map(|e| e.name.clone())
                .unwrap_or_default()
        };
        let mut nodes = vec![
            (self.path.id, self.path.accessibility_node()),
            (self.up.id, self.up.accessibility_node()),
        ];
        let mut children = vec![
            accesskit::NodeId(self.path.id),
            accesskit::NodeId(self.up.id),
            accesskit::NodeId(self.list.id),
        ];
        nodes.extend(self.list.accessibility_nodes(&label));
        if self.kind == ChooserKind::Save {
            nodes.push((self.name.id, self.name.accessibility_node()));
            children.push(accesskit::NodeId(self.name.id));
        }
        if self.has_filter_button() {
            nodes.push((
                self.filter_button.id,
                self.filter_button.accessibility_node(),
            ));
            children.push(accesskit::NodeId(self.filter_button.id));
        }
        nodes.push((self.cancel.id, self.cancel.accessibility_node()));
        nodes.push((self.accept.id, self.accept.accessibility_node()));
        children.push(accesskit::NodeId(self.cancel.id));
        children.push(accesskit::NodeId(self.accept.id));
        dialog.set_children(children);
        nodes.insert(0, (self.id, dialog));
        nodes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::geometry::Scale;

    fn key(key: Key) -> Event {
        Event::KeyPressed {
            key,
            modifiers: Modifiers::default(),
        }
    }

    fn click(id: u64) -> Event {
        Event::Accessibility(AccessibilityRequest::Click(id))
    }

    fn type_text(chooser: &mut FileChooser, text: &str) {
        for ch in text.chars() {
            chooser.handle(&key(Key::Character(ch)), Modifiers::default());
        }
    }

    fn fixture() -> tempfile_dir::Dir {
        let dir = tempfile_dir::Dir::new();
        std::fs::create_dir(dir.path().join("Photos")).unwrap();
        std::fs::write(dir.path().join("Photos/cat.PNG"), b"x").unwrap();
        std::fs::write(dir.path().join("b.png"), b"x").unwrap();
        std::fs::write(dir.path().join("A.jpg"), b"x").unwrap();
        std::fs::write(dir.path().join("notes.txt"), b"x").unwrap();
        std::fs::write(dir.path().join(".hidden.png"), b"x").unwrap();
        dir
    }

    /// A tiny self-cleaning temporary directory (the toolkit has no
    /// tempfile dependency).
    mod tempfile_dir {
        pub struct Dir(std::path::PathBuf);
        impl Dir {
            pub fn new() -> Self {
                use std::sync::atomic::{AtomicU32, Ordering};
                static NEXT: AtomicU32 = AtomicU32::new(0);
                let path = std::env::temp_dir().join(format!(
                    "slopos-chooser-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                std::fs::create_dir_all(&path).unwrap();
                Self(path)
            }
            pub fn path(&self) -> &std::path::Path {
                &self.0
            }
        }
        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    #[test]
    fn listing_sorts_filters_and_hides() {
        let dir = fixture();
        let filter = vec!["png".to_owned(), "jpg".to_owned()];
        let (entries, truncated) = list_directory(dir.path(), Some(&filter)).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Photos", "A.jpg", "b.png"]);
        assert!(!truncated);
        let (all, _) = list_directory(dir.path(), None).unwrap();
        assert_eq!(all.len(), 4);
        assert!(list_directory(&dir.path().join("missing"), None).is_err());
    }

    #[test]
    fn folder_listing_drops_files() {
        let dir = fixture();
        let (folders, _) = list_entries(dir.path(), &|_| true, true).unwrap();
        let names: Vec<&str> = folders.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Photos"]);
    }

    #[test]
    fn file_names_are_single_components() {
        assert_eq!(validate_file_name("report.txt"), Ok(()));
        assert_eq!(validate_file_name(""), Err(NameError::Empty));
        assert_eq!(validate_file_name("   "), Err(NameError::Empty));
        assert_eq!(validate_file_name("."), Err(NameError::Reserved));
        assert_eq!(validate_file_name(".."), Err(NameError::Reserved));
        assert_eq!(validate_file_name("a/b"), Err(NameError::ContainsSlash));
        assert_eq!(validate_file_name("a\0b"), Err(NameError::ContainsNul));
        let long = "x".repeat(MAX_NAME_BYTES + 1);
        assert_eq!(validate_file_name(&long), Err(NameError::TooLong));
        assert_eq!(validate_file_name(&"x".repeat(MAX_NAME_BYTES)), Ok(()));
    }

    #[test]
    fn save_targets_are_classified_without_writing() {
        let dir = fixture();
        assert_eq!(
            classify_save_target(&dir.path().join("new.txt")),
            SaveTarget::New
        );
        assert_eq!(
            classify_save_target(&dir.path().join("notes.txt")),
            SaveTarget::Replace
        );
        assert_eq!(
            classify_save_target(&dir.path().join("Photos")),
            SaveTarget::FolderInTheWay
        );
        #[cfg(unix)]
        {
            let link = dir.path().join("dangling");
            std::os::unix::fs::symlink(dir.path().join("nowhere"), &link).unwrap();
            assert_eq!(classify_save_target(&link), SaveTarget::Replace);
        }
    }

    #[test]
    fn keyboard_navigation_enters_folders_goes_up_and_chooses() {
        let dir = fixture();
        let mut chooser = FileChooser::new(
            200,
            "Open Image",
            dir.path(),
            Some(vec!["png".into(), "jpg".into()]),
        );
        chooser.layout((640, 480));
        assert_eq!(
            chooser.handle(&key(Key::Enter), Modifiers::default()),
            ChooserOutcome::Changed
        );
        assert_eq!(chooser.dir(), dir.path().join("Photos"));
        assert_eq!(
            chooser.handle(&key(Key::Enter), Modifiers::default()),
            ChooserOutcome::Chosen(dir.path().join("Photos/cat.PNG"))
        );
        chooser.handle(&key(Key::Backspace), Modifiers::default());
        assert_eq!(chooser.dir(), dir.path());
        assert_eq!(chooser.list.cursor(), Some(0));
        chooser.handle(&key(Key::Down), Modifiers::default());
        assert_eq!(
            chooser.handle(&key(Key::Enter), Modifiers::default()),
            ChooserOutcome::Chosen(dir.path().join("A.jpg"))
        );
        assert_eq!(
            chooser.handle(&key(Key::Escape), Modifiers::default()),
            ChooserOutcome::Cancelled
        );
    }

    #[test]
    fn typed_paths_and_unreadable_folders_report_errors() {
        let dir = fixture();
        let mut chooser = FileChooser::new(200, "Open", &dir.path().join("missing"), None);
        assert!(chooser
            .status()
            .unwrap()
            .starts_with("Can't open this folder"));
        assert!(chooser.entries().is_empty());
        chooser.layout((640, 480));
        chooser.handle(&key(Key::Tab), Modifiers::default());
        assert!(chooser.path.focused);
        chooser
            .path
            .set_text(&dir.path().join("notes.txt").to_string_lossy());
        assert_eq!(
            chooser.handle(&key(Key::Enter), Modifiers::default()),
            ChooserOutcome::Chosen(dir.path().join("notes.txt"))
        );
        chooser.path.set_text("nope.txt");
        assert_eq!(
            chooser.handle(&key(Key::Enter), Modifiers::default()),
            ChooserOutcome::Changed
        );
        assert!(chooser.status().unwrap().contains("nope.txt"));
        for factor in [1, 2] {
            let surface = RasterSurface::new(640, 480, Scale::new(factor).unwrap()).unwrap();
            chooser.paint(&surface).unwrap();
        }
        assert_eq!(
            chooser.accessibility_nodes()[0].1.role(),
            accesskit::Role::Dialog
        );
    }

    #[test]
    fn save_types_a_new_name_and_chooses_it() {
        let dir = fixture();
        let mut chooser = FileChooser::save(300, "Save As", dir.path(), Vec::new(), "draft.txt");
        chooser.layout((640, 480));
        assert_eq!(chooser.kind(), ChooserKind::Save);
        assert_eq!(chooser.file_name(), "draft.txt");
        assert_eq!(chooser.focus_node(), 300 + ID_NAME);
        chooser.name.set_text("");
        type_text(&mut chooser, "memo.md");
        assert_eq!(
            chooser.handle(&key(Key::Enter), Modifiers::default()),
            ChooserOutcome::Chosen(dir.path().join("memo.md"))
        );
    }

    #[test]
    fn save_rejects_invalid_names_and_folders() {
        let dir = fixture();
        let mut chooser = FileChooser::save(300, "Save As", dir.path(), Vec::new(), "");
        chooser.layout((640, 480));
        assert_eq!(
            chooser.handle(&key(Key::Enter), Modifiers::default()),
            ChooserOutcome::Changed
        );
        assert_eq!(chooser.status(), Some("Enter a file name"));
        chooser.name.set_text("Photos");
        assert_eq!(
            chooser.handle(&key(Key::Enter), Modifiers::default()),
            ChooserOutcome::Changed
        );
        assert_eq!(chooser.status(), Some("“Photos” is a folder"));
        chooser.name.set_text("sub/x.txt");
        chooser.handle(&key(Key::Enter), Modifiers::default());
        assert_eq!(chooser.status(), Some("A file name can't contain “/”"));
    }

    #[test]
    fn overwriting_asks_first_and_escape_returns_to_the_dialog() {
        let dir = fixture();
        let mut chooser = FileChooser::save(300, "Save As", dir.path(), Vec::new(), "notes.txt");
        chooser.layout((640, 480));
        assert_eq!(
            chooser.handle(&key(Key::Enter), Modifiers::default()),
            ChooserOutcome::Changed
        );
        assert!(chooser.is_confirming_overwrite());
        // Focus starts on Cancel, so a second Enter does not replace.
        assert_eq!(
            chooser.handle(&key(Key::Enter), Modifiers::default()),
            ChooserOutcome::Changed
        );
        assert!(!chooser.is_confirming_overwrite());
        chooser.handle(&key(Key::Enter), Modifiers::default());
        assert_eq!(
            chooser.handle(&key(Key::Escape), Modifiers::default()),
            ChooserOutcome::Changed
        );
        assert!(!chooser.is_confirming_overwrite());
        // Escape from the dialog itself cancels.
        assert_eq!(
            chooser.handle(&key(Key::Escape), Modifiers::default()),
            ChooserOutcome::Cancelled
        );
    }

    #[test]
    fn overwrite_replace_chooses_the_existing_file() {
        let dir = fixture();
        let mut chooser = FileChooser::save(300, "Save As", dir.path(), Vec::new(), "notes.txt");
        chooser.layout((640, 480));
        chooser.handle(&key(Key::Enter), Modifiers::default());
        assert_eq!(
            chooser.handle(&click(300 + ID_ACCEPT), Modifiers::default()),
            ChooserOutcome::Chosen(dir.path().join("notes.txt"))
        );
    }

    #[test]
    fn filters_cycle_and_limit_the_listing() {
        let dir = fixture();
        let filters = vec![
            ChooserFilter::extensions("Images", &["png".into(), "jpg".into()]),
            ChooserFilter::new("Text", |name| name.ends_with(".txt")),
        ];
        let mut chooser = FileChooser::open_with_filters(400, "Open", dir.path(), filters, false);
        chooser.layout((640, 480));
        let names = |c: &FileChooser| -> Vec<String> {
            c.entries().iter().map(|e| e.name.clone()).collect()
        };
        assert_eq!(chooser.filter_label(), Some("Images"));
        assert_eq!(names(&chooser), ["Photos", "A.jpg", "b.png"]);
        chooser.handle(&click(400 + ID_FILTER), Modifiers::default());
        assert_eq!(chooser.filter_label(), Some("Text"));
        assert_eq!(names(&chooser), ["Photos", "notes.txt"]);
    }

    #[test]
    fn select_folder_answers_with_the_shown_folder() {
        let dir = fixture();
        let mut chooser = FileChooser::select_folder(500, "Choose Folder", dir.path());
        chooser.layout((640, 480));
        assert_eq!(
            chooser.entries().len(),
            1,
            "files are hidden in folder mode"
        );
        assert_eq!(
            chooser.handle(&key(Key::Enter), Modifiers::default()),
            ChooserOutcome::Changed
        );
        assert_eq!(chooser.dir(), dir.path().join("Photos"));
        assert_eq!(
            chooser.handle(&click(500 + ID_ACCEPT), Modifiers::default()),
            ChooserOutcome::Chosen(dir.path().join("Photos"))
        );
    }

    #[test]
    fn multiple_selection_chooses_every_selected_file() {
        let dir = fixture();
        let mut chooser = FileChooser::open_with_filters(600, "Open", dir.path(), Vec::new(), true);
        chooser.layout((640, 480));
        chooser.list.set_selection(&[1, 2]);
        assert_eq!(
            chooser.handle(&click(600 + ID_ACCEPT), Modifiers::default()),
            ChooserOutcome::ChosenMany(vec![dir.path().join("A.jpg"), dir.path().join("b.png")])
        );
    }
}
