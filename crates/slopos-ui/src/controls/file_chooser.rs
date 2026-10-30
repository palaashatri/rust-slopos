//! Modal "Open" chooser: a folder list, a path field and Up/Cancel/Open.
//!
//! Directories sort first, then files, by case-insensitive name. Hidden
//! entries (leading dot) are left out. An optional extension filter limits
//! which files are shown. Large folders are capped at [`MAX_ENTRIES`] and
//! say so; unreadable folders show the error instead of an empty list.

use std::path::{Path, PathBuf};

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
const ROW_HEIGHT: u32 = 18;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChooserEntry {
    pub name: String,
    pub is_dir: bool,
}

/// List `dir` for the chooser: sorted, hidden entries skipped, files limited
/// to `extensions` (lower-case, without dots) when given. Returns the
/// entries and whether the listing was truncated at [`MAX_ENTRIES`].
pub fn list_directory(
    dir: &Path,
    extensions: Option<&[String]>,
) -> std::io::Result<(Vec<ChooserEntry>, bool)> {
    let mut entries = Vec::new();
    let mut truncated = false;
    for entry in std::fs::read_dir(dir)? {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        // Follow symlinks for the kind, but a dangling link is just a file.
        let is_dir = std::fs::metadata(entry.path()).is_ok_and(|meta| meta.is_dir());
        if !is_dir {
            if let Some(extensions) = extensions {
                let extension = Path::new(&name)
                    .extension()
                    .map(|ext| ext.to_string_lossy().to_lowercase());
                if !extension.is_some_and(|ext| extensions.contains(&ext)) {
                    continue;
                }
            }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChooserOutcome {
    Ignored,
    Changed,
    Chosen(PathBuf),
    Cancelled,
    Copy(String),
    PasteRequested,
}

#[derive(Debug)]
pub struct FileChooser {
    pub id: u64,
    pub title: String,
    pub extensions: Option<Vec<String>>,
    pub font: Font,
    dir: PathBuf,
    entries: Vec<ChooserEntry>,
    status: Option<String>,
    list: ListView,
    path: TextField,
    up: Button,
    cancel: Button,
    open: Button,
    focus: FocusChain,
    bounds: Rect,
}

impl FileChooser {
    pub fn new(id: u64, title: &str, dir: &Path, extensions: Option<Vec<String>>) -> Self {
        let mut chooser = Self {
            id,
            title: title.to_owned(),
            extensions,
            font: Font::system(12.0),
            dir: PathBuf::new(),
            entries: Vec::new(),
            status: None,
            list: ListView::new(id + 1, "Folder contents", ROW_HEIGHT),
            path: TextField::new(id + 2, "Path"),
            up: Button::new(id + 3, "Up", ButtonPlateVariant::Regular),
            cancel: Button::new(id + 4, "Cancel", ButtonPlateVariant::Regular),
            open: Button::new(id + 5, "Open", ButtonPlateVariant::Default),
            focus: FocusChain::new(vec![id + 1, id + 2, id + 3, id + 4, id + 5]),
            bounds: Rect::default(),
        };
        chooser.navigate(dir);
        chooser.focus.focus(id + 1);
        chooser.sync_focus();
        chooser
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

    /// Show `dir`; on failure keep the path and show the error.
    pub fn navigate(&mut self, dir: &Path) {
        self.dir = dir.to_path_buf();
        self.path.set_text(&dir.to_string_lossy());
        match list_directory(dir, self.extensions.as_deref()) {
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
        match std::fs::metadata(&path) {
            Ok(meta) if meta.is_dir() => {
                self.navigate(&path);
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

    fn sync_focus(&mut self) {
        let focused = self.focus.focused();
        self.list.focused = focused == Some(self.list.id);
        self.path.focused = focused == Some(self.path.id);
        self.up.focused = focused == Some(self.up.id);
        self.cancel.focused = focused == Some(self.cancel.id);
        self.open.focused = focused == Some(self.open.id);
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
        self.list.set_bounds(Rect {
            x: inner_x + 1,
            y: list_top,
            width: inner_width - 2,
            height: (buttons_y - status_space - list_top).max(ROW_HEIGHT as i32) as u32,
        });
        self.open.bounds = Rect {
            x: x + width as i32 - 12 - 80,
            y: buttons_y,
            width: 80,
            height: 20,
        };
        self.cancel.bounds = Rect {
            x: self.open.bounds.x - 90,
            y: buttons_y,
            width: 80,
            height: 20,
        };
    }

    pub fn handle(&mut self, event: &Event, modifiers: Modifiers) -> ChooserOutcome {
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
            Event::Accessibility(AccessibilityRequest::Click(id)) if *id == self.open.id => {
                return match self.list.cursor() {
                    Some(index) => self.open_entry(index),
                    None => ChooserOutcome::Changed,
                };
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
        if self.up.handle(event) == ButtonOutcome::Activated {
            self.go_up();
            return ChooserOutcome::Changed;
        }
        if self.cancel.handle(event) == ButtonOutcome::Activated {
            return ChooserOutcome::Cancelled;
        }
        if self.open.handle(event) == ButtonOutcome::Activated {
            return match self.list.cursor() {
                Some(index) => self.open_entry(index),
                None => self.open_typed_path(),
            };
        }
        // Modal: swallow everything else.
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
        if let Some(status) = &self.status {
            let list = self.list.bounds();
            surface.draw_text(
                status,
                list.x,
                list.y + list.height as i32 + 2,
                &self.font,
                colors.danger,
            )?;
        } else if self.entries.is_empty() {
            let list = self.list.bounds();
            surface.draw_text(
                "This folder is empty",
                list.x + 8,
                list.y + 6,
                &self.font,
                colors.ink_secondary,
            )?;
        }
        self.cancel.paint(surface, &self.font)?;
        self.open.paint(surface, &self.font)?;
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
        nodes.extend(self.list.accessibility_nodes(&label));
        nodes.push((self.cancel.id, self.cancel.accessibility_node()));
        nodes.push((self.open.id, self.open.accessibility_node()));
        dialog.set_children(vec![
            accesskit::NodeId(self.path.id),
            accesskit::NodeId(self.up.id),
            accesskit::NodeId(self.list.id),
            accesskit::NodeId(self.cancel.id),
            accesskit::NodeId(self.open.id),
        ]);
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
}
