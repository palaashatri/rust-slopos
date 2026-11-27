//! SLOPOS Text Editor: a plain-text editor on the SLOPOS toolkit.
//!
//! The text lives in [`document::Document`], which handles undo, the modified
//! flag, file limits and atomic saves. The window shows it through the
//! toolkit's multi-line [`TextArea`] and keeps the menus, dialogs and status
//! line.

mod document;

use std::path::{Path, PathBuf};

use document::{find, Document};
use slopos_ui::accessibility::{text_label, window_tree};
use slopos_ui::controls::dialog::{Dialog, DialogOutcome};
use slopos_ui::controls::file_chooser::{ChooserOutcome, FileChooser};
use slopos_ui::controls::menu::{MenuBar, MenuBarOutcome, MenuItem, MENU_BAR_HEIGHT};
use slopos_ui::controls::text_area::{TextArea, TextAreaOutcome, TextEdit};
use slopos_ui::event::{Event, Key, Modifiers};
use slopos_ui::foundation::geometry::{Rect, Scale};
use slopos_ui::platform::{self, App, Control, WindowOptions};
use slopos_ui::render::{RasterSurface, RenderError};
use slopos_ui::text::{measure, Font};
use slopos_ui::theme::palette;

use document::MAX_FILE_BYTES;

const WIDTH: u32 = 640;
const HEIGHT: u32 = 480;
const STATUS_HEIGHT: u32 = 20;

const MENU_ID: u64 = 10;
const TEXT_ID: u64 = 100;
const STATUS_ID: u64 = 101;
const DIALOG_ID: u64 = 200;
const CHOOSER_ID: u64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    New,
    Open,
    Save,
    SaveAs,
    Close,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Find,
    FindNext,
}

/// (label, shortcut, command). An empty label is a separator.
type Entry = (&'static str, &'static str, Option<Command>);

const FILE_MENU: &[Entry] = &[
    ("New", "Ctrl+N", Some(Command::New)),
    ("Open…", "Ctrl+O", Some(Command::Open)),
    ("", "", None),
    ("Save", "Ctrl+S", Some(Command::Save)),
    ("Save As…", "Ctrl+Shift+S", Some(Command::SaveAs)),
    ("", "", None),
    ("Close", "Ctrl+W", Some(Command::Close)),
];

const EDIT_MENU: &[Entry] = &[
    ("Undo", "Ctrl+Z", Some(Command::Undo)),
    ("Redo", "Ctrl+Shift+Z", Some(Command::Redo)),
    ("", "", None),
    ("Cut", "Ctrl+X", Some(Command::Cut)),
    ("Copy", "Ctrl+C", Some(Command::Copy)),
    ("Paste", "Ctrl+V", Some(Command::Paste)),
    ("", "", None),
    ("Select All", "Ctrl+A", Some(Command::SelectAll)),
    ("Find…", "Ctrl+F", Some(Command::Find)),
    ("Find Next", "F3", Some(Command::FindNext)),
];

const MENUS: [&[Entry]; 2] = [FILE_MENU, EDIT_MENU];

/// What to do once a question about unsaved changes is answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Next {
    New,
    Open,
    Close,
}

/// What an open dialog is asking, which decides how its answer is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ask {
    /// Buttons: Cancel, Discard, Save.
    Unsaved(Next),
    /// Buttons: Cancel, Save. The field holds the file name.
    SaveAs(Option<Next>),
    /// Buttons: Cancel, Find Next. The field holds the search text.
    Find,
    /// Button: OK.
    Message,
}

pub struct TextEditorApp {
    window: (u32, u32),
    menu: MenuBar,
    document: Document,
    area: TextArea,
    chooser: Option<FileChooser>,
    dialog: Option<(Dialog, Ask)>,
    find: String,
    status: String,
    font: Font,
}

impl TextEditorApp {
    pub fn new(path: Option<&Path>) -> Self {
        let mut area = TextArea::new(TEXT_ID, "Document");
        area.max_len = MAX_FILE_BYTES as usize;
        area.focused = true;
        let mut app = Self {
            window: (WIDTH, HEIGHT),
            menu: MenuBar::new(
                MENU_ID,
                vec![
                    ("File", menu_items(FILE_MENU)),
                    ("Edit", menu_items(EDIT_MENU)),
                ],
            ),
            document: Document::new(),
            area,
            chooser: None,
            dialog: None,
            find: String::new(),
            status: "Ready".into(),
            font: Font::system(12.0),
        };
        app.layout();
        if let Some(path) = path {
            app.open_path(path);
        }
        app.refresh_menus();
        app
    }

    fn layout(&mut self) {
        let (width, height) = self.window;
        self.menu.bounds = Rect {
            x: 0,
            y: 0,
            width,
            height: MENU_BAR_HEIGHT,
        };
        self.area.bounds = Rect {
            x: 0,
            y: MENU_BAR_HEIGHT as i32,
            width,
            height: height.saturating_sub(MENU_BAR_HEIGHT + STATUS_HEIGHT),
        };
        if let Some((dialog, _)) = &mut self.dialog {
            dialog.layout(self.window);
        }
        if let Some(chooser) = &mut self.chooser {
            chooser.layout(self.window);
        }
    }

    fn status_y(&self) -> i32 {
        self.window.1.saturating_sub(STATUS_HEIGHT) as i32
    }

    fn status_right(&self) -> String {
        let caret = self.area.caret();
        let (line, column) = self.document.line_col(caret);
        format!("Ln {line}, Col {column}")
    }

    fn enabled(&self, command: Command) -> bool {
        match command {
            Command::Undo => self.document.can_undo(),
            Command::Redo => self.document.can_redo(),
            _ => true,
        }
    }

    fn refresh_menus(&mut self) {
        for (menu, entries) in MENUS.iter().enumerate() {
            for (item, (_, _, command)) in entries.iter().enumerate() {
                if let Some(command) = command {
                    let enabled = self.enabled(*command);
                    self.menu.menus[menu].items[item].enabled = enabled;
                }
            }
        }
    }

    fn open_dialog(&mut self, dialog: Dialog, ask: Ask) {
        let mut dialog = dialog;
        dialog.layout(self.window);
        self.dialog = Some((dialog, ask));
    }

    fn message(&mut self, title: &str, text: &str) {
        self.status = title.to_owned();
        let dialog = Dialog::new(DIALOG_ID, title, text, &["OK"]);
        self.open_dialog(dialog, Ask::Message);
    }

    /// The folder for a new file: the document's folder, else the home folder.
    fn save_dir(&self) -> PathBuf {
        self.document
            .path()
            .and_then(Path::parent)
            .filter(|dir| !dir.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("/"))
    }

    fn open_path(&mut self, path: &Path) {
        match Document::open(path) {
            Ok(document) => {
                self.document = document;
                let text = self.document.text();
                self.area.place_caret(text, 0);
                self.status = format!("Opened {}", self.document.title());
            }
            Err(error) => {
                self.message(
                    "Could not open the file",
                    &format!("{}: {error}.", path.display()),
                );
            }
        }
    }

    fn open_chooser(&mut self) {
        let dir = self.save_dir();
        let mut chooser = FileChooser::new(CHOOSER_ID, "Open Text File", &dir, None);
        chooser.layout(self.window);
        self.chooser = Some(chooser);
    }

    /// Run `next` now, or ask first when the document has unsaved changes.
    fn request(&mut self, next: Next, control: &mut Control) {
        if self.document.is_modified() {
            let title = self.document.title();
            let dialog = Dialog::new(
                DIALOG_ID,
                "Unsaved changes",
                &format!("Save the changes to “{title}” first?"),
                &["Cancel", "Discard", "Save"],
            );
            self.open_dialog(dialog, Ask::Unsaved(next));
        } else {
            self.perform(next, control);
        }
    }

    fn perform(&mut self, next: Next, control: &mut Control) {
        match next {
            Next::New => {
                self.document = Document::new();
                let text = self.document.text();
                self.area.place_caret(text, 0);
                self.status = "New document".into();
            }
            Next::Open => self.open_chooser(),
            Next::Close => control.quit(),
        }
    }

    /// Save the document, or ask for a name first. Runs `next` after a
    /// successful save.
    fn save(&mut self, next: Option<Next>, control: &mut Control) {
        if self.document.path().is_none() {
            self.save_as_dialog(next);
            return;
        }
        match self.document.save() {
            Ok(()) => {
                self.status = format!("Saved {}", self.document.title());
                if let Some(next) = next {
                    self.perform(next, control);
                }
            }
            Err(error) => self.message("Could not save", &error.to_string()),
        }
    }

    fn save_as_dialog(&mut self, next: Option<Next>) {
        let name = self.document.title();
        let name = if self.document.path().is_some() {
            name
        } else {
            "Untitled.txt".into()
        };
        let dir = self.save_dir();
        let dialog = Dialog::new(
            DIALOG_ID,
            "Save As",
            &format!("Enter a file name. It will be saved in {}.", dir.display()),
            &["Cancel", "Save"],
        )
        .with_field("File name", &name);
        self.open_dialog(dialog, Ask::SaveAs(next));
    }

    fn save_as_name(&mut self, name: &str, next: Option<Next>, control: &mut Control) {
        if name.is_empty() || name.contains('/') || name == "." || name == ".." {
            self.message("Not saved", "Enter a file name without slashes.");
            return;
        }
        let path = self.save_dir().join(name);
        let same_file = self.document.path() == Some(path.as_path());
        if path.exists() && !same_file {
            self.message(
                "Not saved",
                &format!("{} already exists. Choose another name.", path.display()),
            );
            return;
        }
        match self.document.save_as(path) {
            Ok(()) => {
                self.status = format!("Saved {}", self.document.title());
                if let Some(next) = next {
                    self.perform(next, control);
                }
            }
            Err(error) => self.message("Could not save", &error.to_string()),
        }
    }

    fn find_dialog(&mut self) {
        let dialog = Dialog::new(
            DIALOG_ID,
            "Find",
            "Find the text:",
            &["Cancel", "Find Next"],
        )
        .with_field("Find", &self.find);
        self.open_dialog(dialog, Ask::Find);
    }

    fn find_next(&mut self) {
        if self.find.is_empty() {
            self.find_dialog();
            return;
        }
        let from = self.area.caret();
        let text = self.document.text();
        match find(text, &self.find, from) {
            Some((start, end)) => {
                self.area.select(text, start, end);
                self.status = format!("Found “{}”", self.find);
            }
            None => self.status = format!("“{}” was not found", self.find),
        }
    }

    fn answer(&mut self, ask: Ask, index: usize, text: Option<String>, control: &mut Control) {
        match ask {
            Ask::Message => {}
            Ask::Unsaved(next) => match index {
                // Cancel: nothing is discarded and the action does not run.
                0 => {}
                1 => self.perform(next, control),
                _ => self.save(Some(next), control),
            },
            Ask::SaveAs(next) => {
                if index == 1 {
                    let name = text.unwrap_or_default();
                    self.save_as_name(name.trim(), next, control);
                }
            }
            Ask::Find => {
                if index == 1 {
                    self.find = text.unwrap_or_default();
                    self.find_next();
                }
            }
        }
    }

    fn run(&mut self, command: Command, control: &mut Control) {
        if !self.enabled(command) {
            return;
        }
        match command {
            Command::New => self.request(Next::New, control),
            Command::Open => self.request(Next::Open, control),
            Command::Save => self.save(None, control),
            Command::SaveAs => self.save_as_dialog(None),
            Command::Close => self.request(Next::Close, control),
            Command::Undo => {
                if let Some(caret) = self.document.undo() {
                    let text = self.document.text();
                    self.area.place_caret(text, caret);
                }
            }
            Command::Redo => {
                if let Some(caret) = self.document.redo() {
                    let text = self.document.text();
                    self.area.place_caret(text, caret);
                }
            }
            Command::Cut => self.edit_key('x', control),
            Command::Copy => self.edit_key('c', control),
            Command::Paste => control.request_paste(),
            Command::SelectAll => {
                let text = self.document.text();
                self.area.select_all(text);
            }
            Command::Find => self.find_dialog(),
            Command::FindNext => self.find_next(),
        }
    }

    /// Send Ctrl+`letter` to the text area, as a keyboard shortcut would.
    fn edit_key(&mut self, letter: char, control: &mut Control) {
        let event = Event::KeyPressed {
            key: Key::Character(letter),
            modifiers: Modifiers {
                control: true,
                ..Modifiers::default()
            },
        };
        self.text_event(&event, control);
    }

    fn shortcut(key: &Key, modifiers: Modifiers) -> Option<Command> {
        if matches!(key, Key::Function(3)) {
            return Some(Command::FindNext);
        }
        let Key::Character(c) = key else {
            return None;
        };
        if !modifiers.control || modifiers.alt {
            return None;
        }
        Some(match (c.to_ascii_lowercase(), modifiers.shift) {
            ('n', false) => Command::New,
            ('o', false) => Command::Open,
            ('s', false) => Command::Save,
            ('s', true) => Command::SaveAs,
            ('w', false) => Command::Close,
            ('z', false) => Command::Undo,
            ('z', true) | ('y', false) => Command::Redo,
            ('f', false) => Command::Find,
            ('g', false) => Command::FindNext,
            _ => return None,
        })
    }

    /// Give an event to the text area and apply what it asks for.
    fn text_event(&mut self, event: &Event, control: &mut Control) {
        let caret_before = self.area.caret();
        match self.area.handle(self.document.text(), event) {
            TextAreaOutcome::Edit(edit) => self.apply(edit, caret_before),
            TextAreaOutcome::Cut { text, edit } => {
                control.set_clipboard(&text);
                self.apply(edit, caret_before);
            }
            TextAreaOutcome::Copy(text) => control.set_clipboard(&text),
            TextAreaOutcome::PasteRequested => control.request_paste(),
            TextAreaOutcome::Changed
            | TextAreaOutcome::FocusRequested
            | TextAreaOutcome::Ignored => {}
        }
    }

    fn apply(&mut self, edit: TextEdit, caret_before: usize) {
        if let Err(error) = self.document.apply(&edit, caret_before) {
            // The area's caret moved with its edit: put it back.
            self.status = format!("Edit refused: {error}");
            let text = self.document.text();
            self.area.place_caret(text, caret_before);
        }
    }

    fn handle(&mut self, event: &Event, control: &mut Control) {
        if let Some((dialog, _)) = &mut self.dialog {
            match dialog.handle(event) {
                DialogOutcome::Chosen { index, text } => {
                    let (_, ask) = self.dialog.take().expect("dialog is open");
                    self.answer(ask, index, text, control);
                }
                DialogOutcome::Copy(text) => control.set_clipboard(&text),
                DialogOutcome::PasteRequested => control.request_paste(),
                DialogOutcome::Changed | DialogOutcome::Ignored => {}
            }
            return;
        }
        if let Some(chooser) = &mut self.chooser {
            match chooser.handle(event, control.modifiers()) {
                ChooserOutcome::Chosen(path) => {
                    self.chooser = None;
                    self.open_path(&path);
                }
                ChooserOutcome::Cancelled => self.chooser = None,
                ChooserOutcome::Copy(text) => control.set_clipboard(&text),
                ChooserOutcome::PasteRequested => control.request_paste(),
                ChooserOutcome::Changed | ChooserOutcome::Ignored => {}
            }
            return;
        }
        match self.menu.handle(event, self.window) {
            MenuBarOutcome::Chosen { menu, item } => {
                if let Some(command) = MENUS[menu].get(item).and_then(|entry| entry.2) {
                    self.run(command, control);
                }
                return;
            }
            MenuBarOutcome::Changed => return,
            MenuBarOutcome::Ignored => {}
        }
        if let Event::KeyPressed { key, modifiers } = event {
            if let Some(command) = Self::shortcut(key, *modifiers) {
                self.run(command, control);
                return;
            }
        }
        self.text_event(event, control);
    }

    fn paint_all(&mut self, surface: &RasterSurface) -> Result<(), RenderError> {
        let colors = palette();
        surface.clear(colors.surface)?;
        self.area.paint(surface, self.document.text())?;
        let status = Rect {
            x: 0,
            y: self.status_y(),
            width: self.window.0,
            height: STATUS_HEIGHT,
        };
        surface.fill_rect(status, colors.surface)?;
        let text_y =
            status.y + (STATUS_HEIGHT as i32 - measure("Ag", &self.font).height as i32) / 2;
        surface.draw_text(&self.status, 8, text_y, &self.font, colors.ink)?;
        let right = self.status_right();
        let extent = measure(&right, &self.font);
        let x = self.window.0 as i32 - 8 - extent.width as i32;
        surface.draw_text(&right, x, text_y, &self.font, colors.ink)?;
        self.menu.paint(surface)?;
        if let Some((dialog, _)) = &mut self.dialog {
            dialog.paint(surface)?;
        }
        if let Some(chooser) = &mut self.chooser {
            chooser.paint(surface)?;
        }
        self.menu.paint_menus(surface)
    }

    fn accessibility_tree(&self) -> accesskit::TreeUpdate {
        let mut nodes = self.menu.accessibility_nodes();
        nodes.push((TEXT_ID, self.area.accessibility_node(self.document.text())));
        let status = format!("{}. {}", self.status, self.status_right());
        nodes.push((STATUS_ID, text_label(status)));
        let focus = if let Some((dialog, _)) = &self.dialog {
            nodes.extend(dialog.accessibility_nodes());
            Some(dialog.focus_node())
        } else if let Some(chooser) = &self.chooser {
            nodes.extend(chooser.accessibility_nodes());
            Some(chooser.focus_node())
        } else {
            self.menu.focus_node().or(Some(self.area.focus_node()))
        };
        window_tree(&self.window_title(), nodes, focus)
    }

    fn window_title(&self) -> String {
        let marker = if self.document.is_modified() { "*" } else { "" };
        format!("{marker}{} - Text Editor", self.document.title())
    }
}

fn menu_items(entries: &[Entry]) -> Vec<MenuItem> {
    entries
        .iter()
        .map(|(label, shortcut, _)| {
            if label.is_empty() {
                MenuItem::separator()
            } else {
                let item = MenuItem::new(label);
                if shortcut.is_empty() {
                    item
                } else {
                    item.shortcut(shortcut)
                }
            }
        })
        .collect()
}

impl App for TextEditorApp {
    fn event(&mut self, event: &Event, control: &mut Control) {
        match event {
            Event::CloseRequested => {
                // A question left open must not stop the window from closing.
                self.dialog = None;
                self.chooser = None;
                self.request(Next::Close, control);
            }
            Event::Resized { width, height } => {
                self.window = (*width, *height);
                self.layout();
            }
            // Applied even under a dialog: the bar's height changes the layout.
            Event::GlobalMenu(_) => {
                self.menu.handle(event, self.window);
                self.layout();
            }
            _ => self.handle(event, control),
        }
        self.refresh_menus();
        control.set_title(&self.window_title());
        control.request_redraw();
    }

    fn paint(&mut self, surface: &RasterSurface) {
        if let Err(error) = self.paint_all(surface) {
            eprintln!("slopos-text-editor: paint failed: {error}");
        }
    }

    fn menu_bar(&self) -> Option<&MenuBar> {
        Some(&self.menu)
    }

    fn accessibility(&self) -> Option<accesskit::TreeUpdate> {
        Some(self.accessibility_tree())
    }
}

fn main() {
    if std::env::var_os("RUST_LOG").is_some() {
        let _ = env_logger::try_init();
    }
    let mut args = std::env::args_os().skip(1);
    let first = args.next();
    if first.as_deref() == Some(std::ffi::OsStr::new("--snapshot")) {
        // `--snapshot OUT [FILE]`: render one frame headless (for QA and docs).
        let out: Option<PathBuf> = args.next().map(Into::into);
        let file: Option<PathBuf> = args.next().map(Into::into);
        let Some(out) = out else {
            eprintln!("usage: slopos-text-editor --snapshot OUT [FILE]");
            std::process::exit(2);
        };
        let mut app = TextEditorApp::new(file.as_deref());
        let surface =
            RasterSurface::new(WIDTH, HEIGHT, Scale::new(1).expect("scale")).expect("surface");
        app.paint(&surface);
        if let Err(error) = surface.save_png(&out) {
            eprintln!("slopos-text-editor: snapshot failed: {error}");
            std::process::exit(1);
        }
        return;
    }
    let path = first.map(PathBuf::from);
    let options = WindowOptions::new("Text Editor", "slopos-text-editor", WIDTH, HEIGHT);
    let app = TextEditorApp::new(path.as_deref());
    if let Err(error) = platform::run(options, app, |_| {}) {
        eprintln!("slopos-text-editor: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slopos_ui::event::PointerButton;

    fn press(key: Key, modifiers: Modifiers) -> Event {
        Event::KeyPressed { key, modifiers }
    }

    fn ctrl() -> Modifiers {
        Modifiers {
            control: true,
            ..Modifiers::default()
        }
    }

    fn type_text(app: &mut TextEditorApp, control: &mut Control, text: &str) {
        for c in text.chars() {
            let key = if c == '\n' {
                Key::Enter
            } else {
                Key::Character(c)
            };
            app.event(&press(key, Modifiers::default()), control);
        }
    }

    fn temp_file(dir: &tempfile::TempDir, name: &str, text: &str) -> PathBuf {
        let path = dir.path().join(name);
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn typing_edits_the_document_and_tracks_the_modified_flag() {
        let mut app = TextEditorApp::new(None);
        let mut control = Control::default();
        type_text(&mut app, &mut control, "hi\nthere");
        assert_eq!(app.document.text(), "hi\nthere");
        assert!(app.document.is_modified());
        assert_eq!(app.status_right(), "Ln 2, Col 6");
        assert_eq!(app.window_title(), "*Untitled - Text Editor");
    }

    #[test]
    fn undo_and_redo_shortcuts_restore_text() {
        let mut app = TextEditorApp::new(None);
        let mut control = Control::default();
        type_text(&mut app, &mut control, "abc");
        app.event(&press(Key::Character('z'), ctrl()), &mut control);
        assert_eq!(app.document.text(), "");
        let redo = Modifiers {
            shift: true,
            ..ctrl()
        };
        app.event(&press(Key::Character('Z'), redo), &mut control);
        assert_eq!(app.document.text(), "abc");
        assert_eq!(app.area.caret(), 3);
    }

    #[test]
    fn save_writes_the_document_file_and_clears_modified() {
        // Only temporary folders are touched: names typed in the Save As
        // dialog resolve against $HOME, so that path is not exercised here.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.txt");
        let mut app = TextEditorApp::new(None);
        let mut control = Control::default();
        type_text(&mut app, &mut control, "draft");
        app.document.save_as(path.clone()).unwrap();
        type_text(&mut app, &mut control, "!");
        app.event(&press(Key::Character('s'), ctrl()), &mut control);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "draft!");
        assert!(!app.document.is_modified());
    }

    #[test]
    fn closing_with_unsaved_changes_asks_first() {
        let mut app = TextEditorApp::new(None);
        let mut control = Control::default();
        type_text(&mut app, &mut control, "x");
        app.event(&Event::CloseRequested, &mut control);
        assert!(!control.quit_requested(), "the close waits for an answer");
        let (dialog, ask) = app.dialog.as_ref().expect("the unsaved dialog is open");
        assert_eq!(*ask, Ask::Unsaved(Next::Close));
        assert_eq!(dialog.buttons.len(), 3);

        // Discard closes the window.
        app.answer(Ask::Unsaved(Next::Close), 1, None, &mut control);
        assert!(control.quit_requested());
    }

    #[test]
    fn cancelling_the_unsaved_question_keeps_the_text() {
        let mut app = TextEditorApp::new(None);
        let mut control = Control::default();
        type_text(&mut app, &mut control, "keep");
        app.answer(Ask::Unsaved(Next::New), 0, None, &mut control);
        assert_eq!(app.document.text(), "keep");
        assert!(!control.quit_requested());
    }

    #[test]
    fn discarding_then_new_empties_the_document() {
        let mut app = TextEditorApp::new(None);
        let mut control = Control::default();
        type_text(&mut app, &mut control, "gone");
        app.request(Next::New, &mut control);
        assert!(app.dialog.is_some());
        app.answer(Ask::Unsaved(Next::New), 1, None, &mut control);
        assert_eq!(app.document.text(), "");
        assert!(!app.document.is_modified());
    }

    #[test]
    fn opening_a_large_file_is_refused_with_a_message() {
        let dir = tempfile::tempdir().unwrap();
        let big = dir.path().join("big.txt");
        std::fs::write(&big, vec![b'a'; MAX_FILE_BYTES as usize + 1]).unwrap();
        let app = TextEditorApp::new(Some(&big));
        assert_eq!(app.document.text(), "");
        let (_, ask) = app.dialog.as_ref().expect("a message dialog");
        assert_eq!(*ask, Ask::Message);
        assert_eq!(app.status, "Could not open the file");
    }

    #[test]
    fn opening_a_file_loads_its_text_with_the_caret_at_the_start() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_file(&dir, "hello.txt", "first\nsecond");
        let app = TextEditorApp::new(Some(&path));
        assert_eq!(app.document.text(), "first\nsecond");
        assert_eq!(app.area.caret(), 0);
        assert!(!app.document.is_modified());
    }

    #[test]
    fn find_selects_the_next_match_and_reports_misses() {
        let mut app = TextEditorApp::new(None);
        let mut control = Control::default();
        type_text(&mut app, &mut control, "cat dog cat");
        app.area.place_caret(app.document.text(), 0);
        app.find = "cat".into();
        app.find_next();
        assert_eq!(app.area.selection(), Some((0, 3)));
        app.find_next();
        assert_eq!(app.area.selection(), Some((8, 11)));
        app.find = "bird".into();
        app.find_next();
        assert!(app.status.contains("not found"));
    }

    #[test]
    fn menu_commands_respect_their_enabled_state() {
        let mut app = TextEditorApp::new(None);
        let mut control = Control::default();
        app.refresh_menus();
        let undo = &app.menu.menus[1].items[0];
        assert!(!undo.enabled, "nothing to undo yet");
        type_text(&mut app, &mut control, "a");
        app.refresh_menus();
        assert!(app.menu.menus[1].items[0].enabled);
    }

    #[test]
    fn select_all_then_cut_and_paste_use_the_clipboard() {
        let mut app = TextEditorApp::new(None);
        let mut control = Control::default();
        type_text(&mut app, &mut control, "copy me");
        app.run(Command::SelectAll, &mut control);
        app.event(&press(Key::Character('x'), ctrl()), &mut control);
        assert_eq!(control.pending_clipboard(), Some("copy me"));
        assert_eq!(app.document.text(), "");
        app.event(&press(Key::Character('v'), ctrl()), &mut control);
        assert!(control.paste_requested());
        app.event(&Event::Paste("copy me".into()), &mut control);
        assert_eq!(app.document.text(), "copy me");
    }

    #[test]
    fn clicking_in_the_text_places_the_caret() {
        let mut app = TextEditorApp::new(None);
        let mut control = Control::default();
        type_text(&mut app, &mut control, "one\ntwo");
        let rect = app.area.bounds;
        let y = rect.y + 6;
        let x = rect.x + 4;
        app.event(
            &Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
            },
            &mut control,
        );
        assert_eq!(app.area.caret(), 0);
    }

    #[test]
    fn accessibility_exposes_the_text_and_the_menus() {
        let mut app = TextEditorApp::new(None);
        let mut control = Control::default();
        type_text(&mut app, &mut control, "spoken");
        let tree = app.accessibility().expect("a tree");
        let text = tree
            .nodes
            .iter()
            .find(|(id, _)| *id == accesskit::NodeId(TEXT_ID))
            .map(|(_, node)| node)
            .expect("text node");
        assert_eq!(text.role(), accesskit::Role::MultilineTextInput);
        assert_eq!(text.value(), Some("spoken"));
        let labels: Vec<_> = tree.nodes.iter().filter_map(|(_, n)| n.label()).collect();
        assert!(labels.contains(&"File"));
        assert!(labels.contains(&"Edit"));
    }

    #[test]
    fn the_window_is_painted_headless() {
        let mut app = TextEditorApp::new(None);
        let mut control = Control::default();
        type_text(&mut app, &mut control, "hello");
        let surface = RasterSurface::new(WIDTH, HEIGHT, Scale::new(1).unwrap()).unwrap();
        app.paint(&surface);
        let bytes = surface.encode_png().unwrap();
        assert!(bytes.len() > 100);
    }

    #[test]
    fn shortcuts_map_to_commands() {
        assert_eq!(
            TextEditorApp::shortcut(&Key::Character('s'), ctrl()),
            Some(Command::Save)
        );
        assert_eq!(
            TextEditorApp::shortcut(
                &Key::Character('S'),
                Modifiers {
                    shift: true,
                    ..ctrl()
                }
            ),
            Some(Command::SaveAs)
        );
        assert_eq!(
            TextEditorApp::shortcut(&Key::Function(3), Modifiers::default()),
            Some(Command::FindNext)
        );
        assert_eq!(
            TextEditorApp::shortcut(&Key::Character('s'), Modifiers::default()),
            None
        );
    }
}
