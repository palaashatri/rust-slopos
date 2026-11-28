//! The Archive Utility window: a path field to open an archive, its contents
//! in a list, and Extract / Cancel. Extraction runs on a worker thread; the
//! worker wakes the window when it finishes and every [`archive::NOTIFY_EVERY`]
//! bytes, so the window repaints without a timer.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use slopos_ui::accessibility::{text_label, window_tree};
use slopos_ui::controls::button::{Button, ButtonOutcome};
use slopos_ui::controls::list::{ListOutcome, ListView};
use slopos_ui::controls::text_field::{TextField, TextFieldOutcome};
use slopos_ui::event::{AccessibilityRequest, Event, Key};
use slopos_ui::foundation::geometry::Rect;
use slopos_ui::platform::{App, Control};
use slopos_ui::primitives::button_plate::ButtonPlateVariant;
use slopos_ui::render::{RasterSurface, RenderError};
use slopos_ui::text::Font;
use slopos_ui::theme::palette;

use crate::archive::{self, ArchiveError, Kind, Listing, Progress, Refusal, Summary};
use crate::{HEIGHT, WIDTH};

/// Wakes the window's event loop from another thread.
pub type Waker = Arc<dyn Fn() + Send + Sync>;

const TITLE: &str = "Archive Utility";
const MARGIN: i32 = 12;
const PATH_HEIGHT: u32 = 24;
const OPEN_WIDTH: u32 = 88;
const STATUS_Y: i32 = 44;
const LIST_Y: i32 = 66;
const LIST_BOTTOM: i32 = 378;
const ROW_HEIGHT: u32 = 18;
const BUTTON_Y: i32 = 396;
const BUTTON_HEIGHT: u32 = 26;
const BUTTON_WIDTH: u32 = 110;
const LINE_HEIGHT: i32 = 16;

const PATH_NODE: u64 = 2;
const OPEN_NODE: u64 = 3;
const LIST_NODE: u64 = 4;
const EXTRACT_NODE: u64 = 5;
const CANCEL_NODE: u64 = 6;
const STATUS_NODE: u64 = 7;
const DESTINATION_NODE: u64 = 8;

/// The controls Tab moves between, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Path,
    Open,
    List,
    Extract,
    Cancel,
}

const FOCUS_ORDER: [Focus; 5] = [
    Focus::Path,
    Focus::Open,
    Focus::List,
    Focus::Extract,
    Focus::Cancel,
];

/// An extraction on a worker thread.
struct Running {
    progress: Arc<Progress>,
    result: Arc<Mutex<Option<Result<Summary, ArchiveError>>>>,
    worker: Option<JoinHandle<()>>,
    destination: PathBuf,
}

pub struct ArchiveApp {
    path: TextField,
    open: Button,
    list: ListView,
    extract: Button,
    cancel: Button,
    focus: Focus,
    archive: Option<PathBuf>,
    listing: Option<Listing>,
    status: String,
    status_is_error: bool,
    running: Option<Running>,
    waker: Waker,
    body: Font,
}

impl ArchiveApp {
    /// A window with `initial` (a path, possibly empty) opened if given.
    pub fn new(initial: &str, waker: Waker) -> Self {
        let mut path = TextField::new(PATH_NODE, "Archive").with_bounds(Rect {
            x: MARGIN,
            y: MARGIN,
            width: WIDTH - 2 * MARGIN as u32 - OPEN_WIDTH - 8,
            height: PATH_HEIGHT,
        });
        path.focused = true;
        let mut list = ListView::new(LIST_NODE, "Archive contents", ROW_HEIGHT);
        list.set_bounds(Rect {
            x: MARGIN,
            y: LIST_Y,
            width: WIDTH - 2 * MARGIN as u32,
            height: (LIST_BOTTOM - LIST_Y) as u32,
        });
        let open_x = WIDTH as i32 - MARGIN - OPEN_WIDTH as i32;
        let mut app = Self {
            path,
            open: button(
                OPEN_NODE,
                "Open",
                Rect {
                    x: open_x,
                    y: MARGIN,
                    width: OPEN_WIDTH,
                    height: PATH_HEIGHT,
                },
            ),
            list,
            extract: button(
                EXTRACT_NODE,
                "Extract",
                Rect {
                    x: MARGIN,
                    y: BUTTON_Y,
                    width: BUTTON_WIDTH,
                    height: BUTTON_HEIGHT,
                },
            ),
            cancel: button(
                CANCEL_NODE,
                "Cancel",
                Rect {
                    x: MARGIN + BUTTON_WIDTH as i32 + 8,
                    y: BUTTON_Y,
                    width: BUTTON_WIDTH,
                    height: BUTTON_HEIGHT,
                },
            ),
            focus: Focus::Path,
            archive: None,
            listing: None,
            status: "Type the path of an archive, then press Open.".to_owned(),
            status_is_error: false,
            running: None,
            waker,
            body: Font::system(12.0),
        };
        app.sync_buttons();
        if !initial.trim().is_empty() {
            app.path.set_text(initial.trim());
            app.open_path();
        }
        app
    }

    /// The archive that is listed, if any.
    #[cfg(test)]
    pub fn listed(&self) -> Option<&PathBuf> {
        self.archive.as_ref()
    }

    /// The status line as the window shows it.
    #[cfg(test)]
    pub fn status(&self) -> &str {
        &self.status
    }

    fn set_focus(&mut self, focus: Focus) -> bool {
        if self.focus == focus {
            return false;
        }
        self.focus = focus;
        self.path.focused = focus == Focus::Path;
        self.open.focused = focus == Focus::Open;
        self.list.focused = focus == Focus::List;
        self.extract.focused = focus == Focus::Extract;
        self.cancel.focused = focus == Focus::Cancel;
        true
    }

    fn cycle_focus(&mut self, forward: bool) {
        let index = FOCUS_ORDER
            .iter()
            .position(|focus| *focus == self.focus)
            .unwrap_or(0);
        let count = FOCUS_ORDER.len();
        let next = if forward {
            (index + 1) % count
        } else {
            (index + count - 1) % count
        };
        self.set_focus(FOCUS_ORDER[next]);
    }

    /// Enable Extract only for a listed archive that is not being extracted,
    /// and Cancel only while an extraction runs.
    fn sync_buttons(&mut self) {
        self.extract.enabled = self.listing.is_some() && self.running.is_none();
        self.cancel.enabled = self.running.is_some();
    }

    fn set_status(&mut self, text: impl Into<String>, is_error: bool) {
        self.status = text.into();
        self.status_is_error = is_error;
    }

    fn open_path(&mut self) {
        let typed = self.path.text().trim().to_owned();
        if typed.is_empty() {
            self.archive = None;
            self.listing = None;
            self.list.reset(0);
            self.set_status("Type the path of an archive, then press Open.", true);
            self.sync_buttons();
            return;
        }
        let path = PathBuf::from(&typed);
        match archive::list(&path) {
            Ok(listing) => {
                let text = describe_listing(&listing);
                self.list.reset(listing.entries.len());
                self.archive = Some(path);
                self.listing = Some(listing);
                self.set_status(text, false);
            }
            Err(error) => {
                self.archive = None;
                self.listing = None;
                self.list.reset(0);
                self.set_status(error.to_string(), true);
            }
        }
        self.sync_buttons();
    }

    fn start_extract(&mut self) {
        if self.running.is_some() {
            return;
        }
        let (Some(archive), Some(_)) = (self.archive.clone(), self.listing.as_ref()) else {
            return;
        };
        let Some(destination) = archive::destination_for(&archive) else {
            return;
        };
        let progress = {
            let waker = Arc::clone(&self.waker);
            Arc::new(Progress::new(move || waker()))
        };
        let result = Arc::new(Mutex::new(None));
        let worker_progress = Arc::clone(&progress);
        let worker_result = Arc::clone(&result);
        let worker_waker = Arc::clone(&self.waker);
        let worker_destination = destination.clone();
        let worker = std::thread::spawn(move || {
            let outcome = archive::extract(&archive, &worker_destination, &worker_progress);
            if let Ok(mut slot) = worker_result.lock() {
                *slot = Some(outcome);
            }
            worker_waker();
        });
        self.set_status(format!("Extracting to {}…", destination.display()), false);
        self.running = Some(Running {
            progress,
            result,
            worker: Some(worker),
            destination,
        });
        self.sync_buttons();
    }

    fn cancel_extract(&mut self) {
        if let Some(running) = &self.running {
            running.progress.cancel();
            self.set_status("Cancelling…", false);
        }
    }

    /// Takes the worker's result once it is there; repaints otherwise.
    fn finish_if_done(&mut self) {
        let Some(running) = self.running.as_ref() else {
            return;
        };
        let outcome = running.result.lock().ok().and_then(|mut slot| slot.take());
        let Some(outcome) = outcome else {
            return;
        };
        if let Some(mut running) = self.running.take() {
            if let Some(handle) = running.worker.take() {
                let _ = handle.join();
            }
            match outcome {
                Ok(summary) => {
                    let text = format!(
                        "Extracted {} files and {} folders ({}) to {}.{}",
                        summary.files,
                        summary.directories,
                        human_size(summary.bytes),
                        running.destination.display(),
                        skipped_note(&summary),
                    );
                    self.set_status(text, false);
                }
                Err(error) => {
                    let is_error = !matches!(error, ArchiveError::Cancelled);
                    self.set_status(error.to_string(), is_error);
                }
            }
        }
        self.sync_buttons();
    }

    /// Joins a running worker and takes its result, as a test does.
    #[cfg(test)]
    fn wait_for_extraction(&mut self) {
        if let Some(handle) = self
            .running
            .as_mut()
            .and_then(|running| running.worker.take())
        {
            let _ = handle.join();
        }
    }

    fn progress_text(&self) -> Option<String> {
        self.running
            .as_ref()
            .map(|running| format!("{} written", human_size(running.progress.bytes())))
    }

    fn row_text(&self, index: usize) -> String {
        row_label(self.listing.as_ref(), index)
    }

    fn paint_rows(
        &self,
        surface: &RasterSurface,
        index: usize,
        rect: Rect,
        selected: bool,
    ) -> Result<(), RenderError> {
        let colors = palette();
        let ink = if selected {
            colors.selection_ink
        } else {
            colors.ink
        };
        let text = self.row_text(index);
        surface.draw_text(&text, rect.x + 6, rect.y + 1, &self.body, ink)
    }
}

fn button(id: u64, label: &str, bounds: Rect) -> Button {
    Button::new(id, label, ButtonPlateVariant::Regular).with_bounds(bounds)
}

/// "Name" in a list row, with its size, or why it is not extracted.
fn row_label(listing: Option<&Listing>, index: usize) -> String {
    let Some(entry) = listing.and_then(|listing| listing.entries.get(index)) else {
        return String::new();
    };
    let note = match (entry.refusal, entry.kind) {
        (Some(Refusal::Absolute | Refusal::ParentDirectory | Refusal::TooLong), _) => {
            "refused".to_owned()
        }
        (Some(Refusal::Empty), _) => String::new(),
        (None, Kind::Directory) => "folder".to_owned(),
        (None, Kind::Link) => "link, not extracted".to_owned(),
        (None, Kind::Other) => "not extracted".to_owned(),
        (None, Kind::File) => human_size(entry.size),
    };
    format!("{}    {}", entry.name, note)
}

/// "ZIP archive, 3 items, 4.0 KiB of data." and what is missing from it.
fn describe_listing(listing: &Listing) -> String {
    let count = listing.entries.len();
    let mut text = format!(
        "{} archive, {} {}, {} of data.",
        listing.format.label(),
        count,
        if count == 1 { "item" } else { "items" },
        human_size(listing.total_size()),
    );
    if listing.truncated {
        text.push_str(&format!(
            " Only the first {} items are listed.",
            archive::MAX_ENTRIES
        ));
    }
    if let Some(entry) = listing.first_refusal() {
        text.push_str(&format!(
            " \"{}\" is refused: extraction will not run.",
            entry.name
        ));
    }
    text
}

fn skipped_note(summary: &Summary) -> String {
    let skipped = summary.skipped_links + summary.skipped_other;
    if skipped == 0 {
        String::new()
    } else {
        format!(" {skipped} links or special files were not extracted.")
    }
}

/// Binary units with one decimal place: `1536` is `1.5 KiB`.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

impl App for ArchiveApp {
    fn event(&mut self, event: &Event, control: &mut Control) {
        match event {
            Event::CloseRequested => {
                if let Some(running) = &self.running {
                    running.progress.cancel();
                }
                control.quit();
            }
            Event::Wake => {
                self.finish_if_done();
                control.request_redraw();
            }
            Event::KeyPressed {
                key: Key::Tab,
                modifiers,
            } => {
                self.cycle_focus(!modifiers.shift);
                control.request_redraw();
            }
            Event::Paste(text) if self.focus == Focus::Path => {
                if self.path.insert(text) == TextFieldOutcome::Edited {
                    control.request_redraw();
                }
            }
            Event::Accessibility(AccessibilityRequest::Click(id)) => match *id {
                OPEN_NODE => self.open_path(),
                EXTRACT_NODE => self.start_extract(),
                CANCEL_NODE => self.cancel_extract(),
                _ => {}
            },
            Event::Accessibility(AccessibilityRequest::Focus(id)) => {
                let focus = match *id {
                    PATH_NODE => Some(Focus::Path),
                    OPEN_NODE => Some(Focus::Open),
                    EXTRACT_NODE => Some(Focus::Extract),
                    CANCEL_NODE => Some(Focus::Cancel),
                    _ => None,
                };
                if let Some(focus) = focus {
                    if self.set_focus(focus) {
                        control.request_redraw();
                    }
                }
            }
            _ => self.pointer_and_keys(event, control),
        }
    }

    fn paint(&mut self, surface: &RasterSurface) {
        let colors = palette();
        let _ = surface.clear(colors.surface);
        let _ = self.path.paint(surface);
        let _ = self.open.paint(surface, &self.body);
        let window = &*self;
        let _ = window
            .list
            .paint(surface, |surface, index, rect, selected| {
                window.paint_rows(surface, index, rect, selected)
            });
        let _ = self.extract.paint(surface, &self.body);
        let _ = self.cancel.paint(surface, &self.body);

        let status = self.progress_text().unwrap_or_else(|| self.status.clone());
        let status_ink = if self.status_is_error {
            colors.danger
        } else {
            colors.ink
        };
        let status_width = WIDTH - 2 * MARGIN as u32;
        let _ = surface.with_clip(
            Rect {
                x: MARGIN,
                y: STATUS_Y,
                width: status_width,
                height: LINE_HEIGHT as u32,
            },
            || surface.draw_text(&status, MARGIN, STATUS_Y, &self.body, status_ink),
        );

        if let Some(archive) = self.archive.as_ref() {
            let destination = archive::destination_for(archive)
                .map(|path| format!("Extracts to {}", path.display()))
                .unwrap_or_default();
            let x = MARGIN + 2 * (BUTTON_WIDTH as i32 + 8);
            let width = (WIDTH as i32 - x - MARGIN).max(1) as u32;
            let _ = surface.with_clip(
                Rect {
                    x,
                    y: BUTTON_Y,
                    width,
                    height: BUTTON_HEIGHT,
                },
                || {
                    surface.draw_text(
                        &destination,
                        x,
                        BUTTON_Y + 5,
                        &self.body,
                        colors.ink_secondary,
                    )
                },
            );
        }
    }

    fn accessibility(&self) -> Option<accesskit::TreeUpdate> {
        let mut nodes = vec![
            (PATH_NODE, self.path.accessibility_node()),
            (OPEN_NODE, self.open.accessibility_node()),
            (EXTRACT_NODE, self.extract.accessibility_node()),
            (CANCEL_NODE, self.cancel.accessibility_node()),
        ];
        let listing = self.listing.as_ref();
        nodes.extend(
            self.list
                .accessibility_nodes(&|index| row_label(listing, index)),
        );
        let status = self.progress_text().unwrap_or_else(|| self.status.clone());
        nodes.push((STATUS_NODE, text_label(status)));
        if let Some(archive) = self.archive.as_ref() {
            if let Some(destination) = archive::destination_for(archive) {
                nodes.push((
                    DESTINATION_NODE,
                    text_label(format!("Extracts to {}", destination.display())),
                ));
            }
        }
        let focus = match self.focus {
            Focus::Path => PATH_NODE,
            Focus::Open => OPEN_NODE,
            Focus::List => self.list.focus_node(),
            Focus::Extract => EXTRACT_NODE,
            Focus::Cancel => CANCEL_NODE,
        };
        Some(window_tree(TITLE, nodes, Some(focus)))
    }
}

impl ArchiveApp {
    fn pointer_and_keys(&mut self, event: &Event, control: &mut Control) {
        let modifiers = control.modifiers();
        match self.path.handle(event) {
            TextFieldOutcome::Submitted => {
                self.open_path();
                control.request_redraw();
            }
            TextFieldOutcome::FocusRequested => {
                self.set_focus(Focus::Path);
                control.request_redraw();
            }
            TextFieldOutcome::Copy(text) => control.set_clipboard(&text),
            TextFieldOutcome::PasteRequested => control.request_paste(),
            TextFieldOutcome::Changed | TextFieldOutcome::Edited => control.request_redraw(),
            TextFieldOutcome::Ignored | TextFieldOutcome::Cancelled => {}
        }
        let listing = self.listing.as_ref();
        match self
            .list
            .handle(event, modifiers, &|index| row_label(listing, index))
        {
            ListOutcome::FocusRequested => {
                self.set_focus(Focus::List);
                control.request_redraw();
            }
            ListOutcome::Changed | ListOutcome::SelectionChanged => control.request_redraw(),
            ListOutcome::Activated(_) | ListOutcome::ContextMenu { .. } | ListOutcome::Ignored => {}
        }
        match self.open.handle(event) {
            ButtonOutcome::Activated => {
                self.set_focus(Focus::Open);
                self.open_path();
                control.request_redraw();
            }
            ButtonOutcome::Changed => control.request_redraw(),
            ButtonOutcome::Ignored => {}
        }
        match self.extract.handle(event) {
            ButtonOutcome::Activated => {
                self.set_focus(Focus::Extract);
                self.start_extract();
                control.request_redraw();
            }
            ButtonOutcome::Changed => control.request_redraw(),
            ButtonOutcome::Ignored => {}
        }
        match self.cancel.handle(event) {
            ButtonOutcome::Activated => {
                self.set_focus(Focus::Cancel);
                self.cancel_extract();
                control.request_redraw();
            }
            ButtonOutcome::Changed => control.request_redraw(),
            ButtonOutcome::Ignored => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::{Entry, Format};
    use slopos_ui::foundation::geometry::Scale;
    use std::io::Write;
    use std::path::Path;

    fn noop() -> Waker {
        Arc::new(|| {})
    }

    fn make_zip(dir: &Path, name: &str, items: &[(&str, &str)]) -> PathBuf {
        let path = dir.join(name);
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(std::io::BufWriter::new(file));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (item, body) in items {
            if item.ends_with('/') {
                zip.add_directory(*item, options).unwrap();
            } else {
                zip.start_file(*item, options).unwrap();
                zip.write_all(body.as_bytes()).unwrap();
            }
        }
        zip.finish().unwrap();
        path
    }

    /// A tar whose entry name is stored as given, so an unsafe path can be tested.
    fn make_tar_with_name(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join("unsafe.tar");
        let mut builder = tar::Builder::new(std::fs::File::create(&path).unwrap());
        let mut header = tar::Header::new_ustar();
        let bytes = name.as_bytes();
        header.as_old_mut().name[..bytes.len()].copy_from_slice(bytes);
        header.set_size(body.len() as u64);
        header.set_mode(0o644);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_cksum();
        builder.append(&header, body.as_bytes()).unwrap();
        builder.into_inner().unwrap();
        path
    }

    fn click(app: &mut ArchiveApp, node: u64) -> Control {
        let mut control = Control::default();
        app.event(
            &Event::Accessibility(AccessibilityRequest::Click(node)),
            &mut control,
        );
        control
    }

    fn press(app: &mut ArchiveApp, key: Key, shift: bool) -> Control {
        let mut control = Control::default();
        let modifiers = slopos_ui::event::Modifiers {
            shift,
            ..Default::default()
        };
        app.event(&Event::KeyPressed { key, modifiers }, &mut control);
        control
    }

    /// Every label and value in the window's accessibility tree.
    fn texts(app: &ArchiveApp) -> Vec<String> {
        app.accessibility()
            .expect("a tree")
            .nodes
            .iter()
            .filter_map(|(_, node)| node.value().or(node.label()).map(str::to_owned))
            .collect()
    }

    #[test]
    fn opening_an_archive_lists_it_and_enables_extract() {
        let dir = tempfile::tempdir().unwrap();
        let archive = make_zip(
            dir.path(),
            "Photos.zip",
            &[("Photos/", ""), ("Photos/a.txt", "hello")],
        );
        let app = ArchiveApp::new(archive.to_str().unwrap(), noop());
        assert_eq!(app.listed(), Some(&archive));
        assert!(
            app.status().starts_with("ZIP archive, 2 items"),
            "{}",
            app.status()
        );
        assert!(app.extract.enabled);
        assert!(!app.cancel.enabled);
        assert!(texts(&app).contains(&"Photos/a.txt    5 B".to_owned()));
    }

    #[test]
    fn a_file_that_cannot_be_read_is_shown_as_an_error() {
        let mut app = ArchiveApp::new("/nonexistent-slopos/missing.zip", noop());
        assert!(app.status_is_error);
        assert!(
            app.status().contains("not a readable archive"),
            "{}",
            app.status()
        );
        assert!(!app.extract.enabled);
        assert!(app.listed().is_none());
        click(&mut app, EXTRACT_NODE);
        assert!(app.running.is_none());
    }

    #[test]
    fn an_empty_path_asks_for_one() {
        let app = ArchiveApp::new("", noop());
        assert!(app.status().contains("Type the path"));
        assert!(!app.extract.enabled);
    }

    #[test]
    fn extract_writes_the_folder_and_reports_it_after_the_wake() {
        let dir = tempfile::tempdir().unwrap();
        let archive = make_zip(
            dir.path(),
            "Photos.zip",
            &[("Photos/", ""), ("Photos/a.txt", "hello")],
        );
        let mut app = ArchiveApp::new(archive.to_str().unwrap(), noop());
        click(&mut app, EXTRACT_NODE);
        assert!(app.running.is_some());
        assert!(!app.extract.enabled, "Extract is off while a job runs");
        assert!(app.cancel.enabled);

        app.wait_for_extraction();
        let mut control = Control::default();
        app.event(&Event::Wake, &mut control);
        assert!(app.running.is_none());
        assert!(!app.cancel.enabled);
        assert!(app.extract.enabled);
        assert!(
            app.status().starts_with("Extracted 1 files"),
            "{}",
            app.status()
        );
        let written = dir.path().join("Photos").join("a.txt");
        assert_eq!(std::fs::read_to_string(written).unwrap(), "hello");
    }

    #[test]
    fn a_refused_archive_is_named_and_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let archive = make_tar_with_name(dir.path(), "../escape.txt", "no");
        let mut app = ArchiveApp::new(archive.to_str().unwrap(), noop());
        assert!(app.status().contains("is refused"), "{}", app.status());

        click(&mut app, EXTRACT_NODE);
        app.wait_for_extraction();
        app.event(&Event::Wake, &mut Control::default());
        assert!(app.status_is_error);
        assert!(app
            .status()
            .starts_with("Refused: the entry \"../escape.txt\""));
        assert!(!dir.path().join("unsafe").exists());
        assert!(!dir.path().join("escape.txt").exists());
    }

    #[test]
    fn tab_moves_focus_through_the_controls_and_back() {
        let dir = tempfile::tempdir().unwrap();
        let archive = make_zip(dir.path(), "A.zip", &[("a.txt", "x")]);
        let mut app = ArchiveApp::new(archive.to_str().unwrap(), noop());
        assert_eq!(app.focus, Focus::Path);
        press(&mut app, Key::Tab, false);
        assert_eq!(app.focus, Focus::Open);
        press(&mut app, Key::Tab, false);
        assert_eq!(app.focus, Focus::List);
        press(&mut app, Key::Tab, false);
        assert_eq!(app.focus, Focus::Extract);
        press(&mut app, Key::Tab, true);
        assert_eq!(app.focus, Focus::List);
        press(&mut app, Key::Tab, true);
        press(&mut app, Key::Tab, true);
        press(&mut app, Key::Tab, true);
        press(&mut app, Key::Tab, true);
        assert_eq!(app.focus, Focus::Path, "Shift+Tab wraps around");
        assert!(app.path.focused);
        assert!(!app.list.focused);
    }

    #[test]
    fn enter_on_the_focused_extract_button_starts_the_job() {
        let dir = tempfile::tempdir().unwrap();
        let archive = make_zip(dir.path(), "K.zip", &[("k.txt", "keys")]);
        let mut app = ArchiveApp::new(archive.to_str().unwrap(), noop());
        for _ in 0..3 {
            press(&mut app, Key::Tab, false);
        }
        assert_eq!(app.focus, Focus::Extract);
        press(&mut app, Key::Enter, false);
        assert!(app.running.is_some());
        app.wait_for_extraction();
        app.event(&Event::Wake, &mut Control::default());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("K").join("k.txt")).unwrap(),
            "keys"
        );
    }

    #[test]
    fn the_focus_node_follows_the_keyboard_focus() {
        let dir = tempfile::tempdir().unwrap();
        let archive = make_zip(dir.path(), "F.zip", &[("f.txt", "x")]);
        let mut app = ArchiveApp::new(archive.to_str().unwrap(), noop());
        assert_eq!(
            app.accessibility().unwrap().focus,
            accesskit::NodeId(PATH_NODE)
        );
        press(&mut app, Key::Tab, false);
        assert_eq!(
            app.accessibility().unwrap().focus,
            accesskit::NodeId(OPEN_NODE)
        );
    }

    #[test]
    fn rows_say_why_an_entry_is_not_extracted() {
        let listing = Listing {
            format: Format::Zip,
            entries: vec![
                Entry {
                    name: "../up.txt".to_owned(),
                    kind: Kind::File,
                    size: 3,
                    refusal: Some(Refusal::ParentDirectory),
                },
                Entry {
                    name: "link".to_owned(),
                    kind: Kind::Link,
                    size: 0,
                    refusal: None,
                },
                Entry {
                    name: "dir/".to_owned(),
                    kind: Kind::Directory,
                    size: 0,
                    refusal: None,
                },
                Entry {
                    name: "big.bin".to_owned(),
                    kind: Kind::File,
                    size: 3 * 1024 * 1024,
                    refusal: None,
                },
            ],
            truncated: false,
        };
        assert_eq!(row_label(Some(&listing), 0), "../up.txt    refused");
        assert_eq!(row_label(Some(&listing), 1), "link    link, not extracted");
        assert_eq!(row_label(Some(&listing), 2), "dir/    folder");
        assert_eq!(row_label(Some(&listing), 3), "big.bin    3.0 MiB");
        assert_eq!(row_label(Some(&listing), 4), "");
        assert!(describe_listing(&listing).contains("\"../up.txt\" is refused"));
    }

    #[test]
    fn sizes_use_binary_units() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1536), "1.5 KiB");
        assert_eq!(human_size(5 * 1024 * 1024 * 1024), "5.0 GiB");
    }

    #[test]
    fn paints_at_1x_and_2x_and_the_list_changes_the_picture() {
        let dir = tempfile::tempdir().unwrap();
        let archive = make_zip(dir.path(), "P.zip", &[("p.txt", "pixels")]);
        for factor in [1, 2] {
            let scale = Scale::new(factor).unwrap();
            let listed = RasterSurface::new(WIDTH, HEIGHT, scale).unwrap();
            let mut app = ArchiveApp::new(archive.to_str().unwrap(), noop());
            app.paint(&listed);
            assert_eq!(listed.device_width(), WIDTH * factor);
            assert_eq!(listed.device_height(), HEIGHT * factor);

            let empty = RasterSurface::new(WIDTH, HEIGHT, scale).unwrap();
            ArchiveApp::new("", noop()).paint(&empty);
            assert_ne!(listed.data().to_vec(), empty.data().to_vec(), "{factor}x");
        }
    }
}
