//! The Disks window: drives and volumes from the UDisks2 storage provider,
//! in a list with the selected row's facts beside it.
//!
//! Reading the provider blocks, so it runs on a worker thread that wakes the
//! window when it finishes. Refresh starts another read. Nothing is mounted,
//! unmounted, ejected or formatted by this window; those actions are not
//! built yet and the window says so.

use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use accesskit::TreeUpdate;
use slopos_services::storage::{self, StorageError, StorageSnapshot};
use slopos_ui::accessibility::{text_label, window_tree};
use slopos_ui::controls::button::{Button, ButtonOutcome};
use slopos_ui::controls::dialog::wrap;
use slopos_ui::controls::list::{ListOutcome, ListView};
use slopos_ui::event::{AccessibilityRequest, Event, Key};
use slopos_ui::foundation::geometry::Rect;
use slopos_ui::platform::{App, Control};
use slopos_ui::primitives::button_plate::ButtonPlateVariant;
use slopos_ui::render::{RasterSurface, RenderError};
use slopos_ui::text::Font;
use slopos_ui::theme::palette;

use crate::model::{self, Row};

pub const WIDTH: u32 = 640;
pub const HEIGHT: u32 = 400;

/// Wakes the window's event loop from another thread.
pub type Waker = Arc<dyn Fn() + Send + Sync>;
/// Reads the storage provider. The window uses [`storage::snapshot_blocking`];
/// tests pass their own.
pub type Loader = fn() -> Result<StorageSnapshot, StorageError>;

const MARGIN: i32 = 12;
const LIST_WIDTH: u32 = 300;
const BUTTON_HEIGHT: u32 = 26;
const ROW_HEIGHT: u32 = 18;
const LINE_HEIGHT: i32 = 16;
const PADDING: i32 = 10;
const NOTE: &str = "Read-only. Mount, unmount, eject and format are not built in this window yet.";

const LIST_NODE: u64 = 2;
const REFRESH_NODE: u64 = 3;
const DETAILS_NODE: u64 = 4;
const STATUS_NODE: u64 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    List,
    Refresh,
}

type Slot = Arc<Mutex<Option<Result<StorageSnapshot, StorageError>>>>;

pub struct DisksApp {
    rows: Vec<Row>,
    list: ListView,
    refresh: Button,
    focus: Focus,
    /// Why the list is empty, or what the last read said.
    status: String,
    status_is_error: bool,
    loading: bool,
    slot: Slot,
    worker: Option<JoinHandle<()>>,
    waker: Waker,
    loader: Loader,
    body: Font,
    heading: Font,
}

impl DisksApp {
    /// A window that starts reading the provider at once.
    pub fn new(waker: Waker) -> Self {
        Self::with_loader(waker, storage::snapshot_blocking)
    }

    pub fn with_loader(waker: Waker, loader: Loader) -> Self {
        let mut list = ListView::new(LIST_NODE, "Drives and volumes", ROW_HEIGHT);
        list.set_bounds(Rect {
            x: MARGIN,
            y: MARGIN + BUTTON_HEIGHT as i32 + 8,
            width: LIST_WIDTH,
            height: HEIGHT - (2 * MARGIN as u32 + BUTTON_HEIGHT + 8),
        });
        let mut refresh = Button::new(REFRESH_NODE, "Refresh", ButtonPlateVariant::Regular)
            .with_bounds(Rect {
                x: MARGIN,
                y: MARGIN,
                width: 96,
                height: BUTTON_HEIGHT,
            });
        refresh.focused = false;
        let mut app = Self {
            rows: Vec::new(),
            list,
            refresh,
            focus: Focus::List,
            status: String::new(),
            status_is_error: false,
            loading: false,
            slot: Arc::new(Mutex::new(None)),
            worker: None,
            waker,
            loader,
            body: Font::system(12.0),
            heading: Font::system(13.0).bold(),
        };
        app.list.focused = true;
        app.start_read();
        app
    }

    /// Starts a read unless one is already running. The read happens on a
    /// worker thread; the window repaints when it wakes.
    fn start_read(&mut self) {
        if self.loading {
            return;
        }
        self.loading = true;
        self.status = "Reading disks…".to_owned();
        self.status_is_error = false;
        let slot = Arc::clone(&self.slot);
        let waker = Arc::clone(&self.waker);
        let loader = self.loader;
        self.worker = Some(std::thread::spawn(move || {
            let result = loader();
            if let Ok(mut slot) = slot.lock() {
                *slot = Some(result);
            }
            waker();
        }));
    }

    /// Takes the worker's result once it is there.
    fn finish_if_done(&mut self) {
        let outcome = self.slot.lock().ok().and_then(|mut slot| slot.take());
        let Some(outcome) = outcome else {
            return;
        };
        if let Some(handle) = self.worker.take() {
            let _ = handle.join();
        }
        self.loading = false;
        match outcome {
            Ok(snapshot) => match model::availability_text(snapshot.availability) {
                None => {
                    self.rows = model::rows(&snapshot);
                    self.status = if self.rows.is_empty() {
                        "UDisks2 reported no drives or volumes.".to_owned()
                    } else {
                        format!("{} drives and volumes. {NOTE}", self.rows.len())
                    };
                    self.status_is_error = false;
                }
                Some(text) => {
                    self.rows = Vec::new();
                    self.status = text;
                    self.status_is_error = true;
                }
            },
            Err(error) => {
                self.rows = Vec::new();
                self.status = model::error_text(&error);
                self.status_is_error = true;
            }
        }
        self.list.reset(self.rows.len());
        if !self.rows.is_empty() {
            self.list.select_only(0);
        }
    }

    fn selected(&self) -> Option<&Row> {
        self.rows.get(self.list.cursor()?)
    }

    /// Lines for the details panel: the selected row, or the status when
    /// there is nothing to select.
    fn details(&self) -> Vec<String> {
        match self.selected() {
            Some(row) => vec![
                format!("Name: {}", row.name),
                format!("Size: {}", model::human_size(row.size)),
                format!(
                    "Kind: {}",
                    if row.kind == model::Kind::Drive {
                        "drive"
                    } else {
                        "volume"
                    }
                ),
                row.detail.clone(),
            ],
            None => vec![self.status.clone()],
        }
    }

    fn set_focus(&mut self, focus: Focus) -> bool {
        if self.focus == focus {
            return false;
        }
        self.focus = focus;
        self.list.focused = focus == Focus::List;
        self.refresh.focused = focus == Focus::Refresh;
        true
    }

    fn details_rect(&self) -> Rect {
        Rect {
            x: MARGIN + LIST_WIDTH as i32 + 12,
            y: MARGIN + BUTTON_HEIGHT as i32 + 8,
            width: WIDTH - (2 * MARGIN as u32 + LIST_WIDTH + 12),
            height: HEIGHT - (2 * MARGIN as u32 + BUTTON_HEIGHT + 8),
        }
    }

    fn paint_details(&self, surface: &RasterSurface, area: Rect) -> Result<(), RenderError> {
        let colors = palette();
        let width = area.width.saturating_sub(2 * PADDING as u32).max(1);
        let mut y = area.y + PADDING;
        for (index, line) in self.details().iter().enumerate() {
            let font = if index == 0 {
                &self.heading
            } else {
                &self.body
            };
            for wrapped in wrap(line, font, width) {
                surface.draw_text(&wrapped, area.x + PADDING, y, font, colors.ink)?;
                y += LINE_HEIGHT;
            }
            y += 4;
        }
        Ok(())
    }
}

impl App for DisksApp {
    fn event(&mut self, event: &Event, control: &mut Control) {
        match event {
            Event::CloseRequested => control.quit(),
            Event::Wake => self.finish_if_done(),
            Event::KeyPressed { key: Key::Tab, .. } => {
                let next = if self.focus == Focus::List {
                    Focus::Refresh
                } else {
                    Focus::List
                };
                self.set_focus(next);
            }
            Event::Accessibility(AccessibilityRequest::Click(id)) if *id == REFRESH_NODE => {
                self.start_read();
            }
            Event::Accessibility(AccessibilityRequest::Focus(id)) => {
                let focus = match *id {
                    LIST_NODE => Focus::List,
                    REFRESH_NODE => Focus::Refresh,
                    _ => return,
                };
                self.set_focus(focus);
            }
            _ => {
                let modifiers = control.modifiers();
                let rows = &self.rows;
                match self.list.handle(event, modifiers, &|row| {
                    rows.get(row).map(Row::label).unwrap_or_default()
                }) {
                    ListOutcome::FocusRequested => {
                        self.set_focus(Focus::List);
                    }
                    ListOutcome::SelectionChanged | ListOutcome::Activated(_) => {}
                    _ => {}
                }
                if self.refresh.handle(event) == ButtonOutcome::Activated {
                    self.set_focus(Focus::Refresh);
                    self.start_read();
                }
            }
        }
        control.request_redraw();
    }

    fn paint(&mut self, surface: &RasterSurface) {
        let colors = palette();
        let _ = surface.clear(colors.surface);
        let _ = self.refresh.paint(surface, &self.body);
        let window = &*self;
        let _ = window.list.paint(surface, |surface, row, rect, selected| {
            let ink = if selected {
                palette().selection_ink
            } else {
                palette().ink
            };
            let text = window.rows.get(row).map(Row::label).unwrap_or_default();
            surface.draw_text(&text, rect.x + 6, rect.y + 1, &window.body, ink)
        });
        let panel = self.details_rect();
        let _ = surface.fill_rect(panel, colors.content);
        let _ = surface.stroke_rect(panel, colors.border);
        let _ = surface.with_clip(panel, || self.paint_details(surface, panel));

        let status_ink = if self.status_is_error {
            colors.danger
        } else {
            colors.ink
        };
        let status_area = Rect {
            x: MARGIN,
            y: HEIGHT as i32 - MARGIN - LINE_HEIGHT,
            width: WIDTH - 2 * MARGIN as u32,
            height: LINE_HEIGHT as u32,
        };
        let _ = surface.with_clip(status_area, || {
            surface.draw_text(
                &self.status,
                status_area.x,
                status_area.y,
                &self.body,
                status_ink,
            )
        });
    }

    fn accessibility(&self) -> Option<TreeUpdate> {
        let rows = &self.rows;
        let mut nodes = vec![(REFRESH_NODE, self.refresh.accessibility_node())];
        nodes.extend(
            self.list
                .accessibility_nodes(&|row| rows.get(row).map(Row::label).unwrap_or_default()),
        );
        nodes.push((DETAILS_NODE, text_label(self.details().join("\n"))));
        nodes.push((STATUS_NODE, text_label(self.status.clone())));
        let focus = match self.focus {
            Focus::List => self.list.focus_node(),
            Focus::Refresh => REFRESH_NODE,
        };
        Some(window_tree("Disks", nodes, Some(focus)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slopos_core::capability::Availability;

    fn noop() -> Waker {
        Arc::new(|| {})
    }

    fn empty_available() -> Result<StorageSnapshot, StorageError> {
        Ok(StorageSnapshot {
            revision: 1,
            availability: Availability::Available,
            blocks: Vec::new(),
            drives: Vec::new(),
        })
    }

    fn missing_service() -> Result<StorageSnapshot, StorageError> {
        Ok(StorageSnapshot {
            revision: 1,
            availability: Availability::MissingService,
            blocks: Vec::new(),
            drives: Vec::new(),
        })
    }

    fn denied() -> Result<StorageSnapshot, StorageError> {
        Err(StorageError::PermissionDenied(zbus::Error::Failure(
            "not authorised".to_owned(),
        )))
    }

    /// Joins the worker and delivers its wake, as the event loop would.
    fn settle(app: &mut DisksApp) {
        if let Some(handle) = app.worker.take() {
            let _ = handle.join();
        }
        app.event(&Event::Wake, &mut Control::default());
    }

    fn texts(app: &DisksApp) -> Vec<String> {
        app.accessibility()
            .expect("a tree")
            .nodes
            .iter()
            .filter_map(|(_, node)| node.value().or(node.label()).map(str::to_owned))
            .collect()
    }

    #[test]
    fn a_read_starts_at_once_and_says_so_until_it_finishes() {
        let mut app = DisksApp::with_loader(noop(), empty_available);
        assert!(app.loading);
        assert!(texts(&app).iter().any(|t| t == "Reading disks…"));
        settle(&mut app);
        assert!(!app.loading);
    }

    #[test]
    fn a_healthy_read_with_no_devices_says_none_were_reported() {
        let mut app = DisksApp::with_loader(noop(), empty_available);
        settle(&mut app);
        assert!(app.rows.is_empty());
        assert_eq!(app.status, "UDisks2 reported no drives or volumes.");
        assert!(!app.status_is_error);
    }

    #[test]
    fn a_missing_service_is_text_on_the_screen() {
        let mut app = DisksApp::with_loader(noop(), missing_service);
        settle(&mut app);
        assert!(app.status_is_error);
        assert!(
            app.status.contains("UDisks2 is not running"),
            "{}",
            app.status
        );
        assert!(texts(&app)
            .iter()
            .any(|t| t.contains("UDisks2 is not running")));
    }

    #[test]
    fn a_permission_error_is_text_on_the_screen() {
        let mut app = DisksApp::with_loader(noop(), denied);
        settle(&mut app);
        assert!(
            app.status.starts_with("Permission denied"),
            "{}",
            app.status
        );
    }

    #[test]
    fn refresh_reads_again_only_when_no_read_is_running() {
        let mut app = DisksApp::with_loader(noop(), empty_available);
        app.start_read();
        assert!(app.worker.is_some());
        settle(&mut app);
        app.event(
            &Event::Accessibility(AccessibilityRequest::Click(REFRESH_NODE)),
            &mut Control::default(),
        );
        assert!(app.loading);
        settle(&mut app);
        assert!(!app.loading);
    }

    #[test]
    fn tab_moves_between_the_list_and_refresh() {
        let mut app = DisksApp::with_loader(noop(), empty_available);
        settle(&mut app);
        assert_eq!(app.focus, Focus::List);
        app.event(
            &Event::KeyPressed {
                key: Key::Tab,
                modifiers: Default::default(),
            },
            &mut Control::default(),
        );
        assert_eq!(app.focus, Focus::Refresh);
        assert!(app.refresh.focused);
    }

    #[test]
    fn paints_at_1x_and_2x() {
        use slopos_ui::foundation::geometry::Scale;
        for factor in [1, 2] {
            let scale = Scale::new(factor).unwrap();
            let surface = RasterSurface::new(WIDTH, HEIGHT, scale).unwrap();
            let mut app = DisksApp::with_loader(noop(), missing_service);
            settle(&mut app);
            app.paint(&surface);
            assert_eq!(surface.device_width(), WIDTH * factor);
            assert_eq!(surface.device_height(), HEIGHT * factor);
        }
    }
}
