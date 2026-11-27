//! The System Monitor window: five tabs (Processes, CPU, Memory, Disk, GPU).
//!
//! The window samples `/proc` once a second, and only while it is mapped:
//! [`Event::Visibility`] starts and stops the timer, so a minimised or
//! hidden monitor costs nothing. Every value shown comes from the last
//! [`Monitor`] view, and a value that could not be read says so.
//!
//! Ending a process is the one action. The request is checked twice (see
//! [`crate::policy`]), confirmed in a modal [`Dialog`] whose default button is
//! "No", sent as SIGTERM, and read back a second later.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use accesskit::{Node, NodeId, Role, TreeId, TreeInfo, TreeUpdate};
use slopos_ui::controls::button::{Button, ButtonOutcome};
use slopos_ui::controls::dialog::{self, Dialog, DialogOutcome};
use slopos_ui::controls::focus::FocusChain;
use slopos_ui::controls::list::{ListOutcome, ListView};
use slopos_ui::controls::tabs::{TabOutcome, TabStrip};
use slopos_ui::controls::text_field::{TextField, TextFieldOutcome};
use slopos_ui::event::{AccessibilityRequest, Event};
use slopos_ui::foundation::geometry::Rect;
use slopos_ui::platform::{App, Control};
use slopos_ui::primitives::button_plate::ButtonPlateVariant;
use slopos_ui::render::{RasterSurface, RenderError};
use slopos_ui::text::{measure, Font};
use slopos_ui::theme::palette;

use crate::format;
use crate::gpu::GpuInfo;
use crate::model::{Monitor, ProcessRow, View};
use crate::policy::{self, Readback, Target};
use crate::procfs;
use crate::source::Source;

pub const TABS: [&str; 5] = ["Processes", "CPU", "Memory", "Disk", "GPU"];
const PROCESSES: usize = 0;
const CPU: usize = 1;
const MEMORY: usize = 2;
const DISK: usize = 3;
const GPU: usize = 4;

const MARGIN: i32 = 8;
const TAB_HEIGHT: u32 = 24;
const CONTROL_HEIGHT: u32 = 24;
const FILTER_LABEL_WIDTH: i32 = 48;
const INSPECT_WIDTH: u32 = 84;
const END_WIDTH: u32 = 104;
const ROW_HEIGHT: u32 = 20;
const HEADER_HEIGHT: i32 = 16;
const STATUS_HEIGHT: i32 = 16;
const LINE_HEIGHT: i32 = 16;
const PANEL_HEIGHT: i32 = 6 * LINE_HEIGHT + 2 * 3;
const ENTRY_HEIGHT: i32 = 40;
const BAR_HEIGHT: i32 = 10;
/// Width of a CPU entry column, so a wide window shows the cores side by side.
const CPU_COLUMN_WIDTH: u32 = 260;

const SAMPLE_TIMER: u64 = 1;
const READBACK_TIMER: u64 = 2;
const SAMPLE_PERIOD: Duration = Duration::from_secs(1);
const READBACK_DELAY: Duration = Duration::from_secs(1);

const ROOT_NODE: u64 = 0;
const TABS_ID: u64 = 1;
const FILTER_ID: u64 = 2;
const LIST_ID: u64 = 3;
const INSPECT_ID: u64 = 4;
const END_ID: u64 = 5;
/// The confirmation dialog's ids are `DIALOG_ID` and up (field and buttons).
const DIALOG_ID: u64 = 50;
/// Text lines are label nodes numbered from here.
const LINE_NODE: u64 = 100;
const MAX_LINE_NODES: usize = 128;

/// The focusable controls, in the order Tab visits them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Tabs,
    Filter,
    List,
    Inspect,
    End,
}

/// A rectangle with a text label, or a bar, drawn in a view.
#[derive(Debug, Clone, PartialEq)]
struct Label {
    text: String,
    rect: Rect,
    bold: bool,
    /// A bar under the text, filled to this fraction (0 to 1).
    fraction: Option<f64>,
    /// Drawn in the secondary ink (the status line).
    secondary: bool,
}

/// One line of a Memory, CPU, Disk or GPU view.
#[derive(Debug, Clone, PartialEq)]
struct Entry {
    text: String,
    fraction: Option<f64>,
}

impl Entry {
    fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            fraction: None,
        }
    }
}

/// Where the Processes view puts its parts, in logical pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Layout {
    /// The area under the tabs.
    content: Rect,
    /// Column titles above the process list.
    header: Rect,
    /// The inspection panel under the list.
    panel: Rect,
    /// The status line at the bottom.
    status: Rect,
}

pub struct SystemMonitor<S: Source> {
    source: S,
    monitor: Monitor,
    tabs: TabStrip,
    filter: TextField,
    list: ListView,
    inspect: Button,
    end: Button,
    focus: FocusChain,
    dialog: Option<Dialog>,
    pending_end: Option<Target>,
    pending_readback: Option<Target>,
    /// Indices into `monitor.view.processes` that pass the filter.
    visible: Vec<usize>,
    visible_names: Vec<String>,
    selected_pid: Option<u32>,
    inspected_pid: Option<u32>,
    visible_window: bool,
    timer_armed: bool,
    status: String,
    gpu: Option<GpuInfo>,
    width: u32,
    height: u32,
    layout: Layout,
    font: Font,
    bold: Font,
}

impl<S: Source> SystemMonitor<S> {
    pub fn new(source: S, width: u32, height: u32) -> Self {
        let users = source.user_names();
        let mut app = Self {
            source,
            monitor: Monitor::new(users),
            tabs: TabStrip::new(TABS_ID, &TABS),
            filter: TextField::new(FILTER_ID, "Filter processes"),
            list: ListView::new(LIST_ID, "Processes", ROW_HEIGHT),
            inspect: Button::new(INSPECT_ID, "Inspect", ButtonPlateVariant::Regular),
            end: Button::new(END_ID, "End process", ButtonPlateVariant::Regular),
            focus: FocusChain::default(),
            dialog: None,
            pending_end: None,
            pending_readback: None,
            visible: Vec::new(),
            visible_names: Vec::new(),
            selected_pid: None,
            inspected_pid: None,
            visible_window: false,
            timer_armed: false,
            status: String::new(),
            gpu: None,
            width,
            height,
            layout: Layout::default(),
            font: Font::system(12.0),
            bold: Font::system(12.0).bold(),
        };
        app.relayout(width, height);
        app
    }

    /// The selected tab's index.
    pub fn view_index(&self) -> usize {
        self.tabs.selected
    }

    /// The text of the status line, as the window shows it.
    pub fn status(&self) -> &str {
        &self.status
    }

    fn view(&self) -> &View {
        &self.monitor.view
    }

    fn relayout(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        let w = width as i32;
        let inner = width.saturating_sub(2 * MARGIN as u32).max(1);
        self.tabs.set_bounds(Rect {
            x: MARGIN,
            y: MARGIN,
            width: inner,
            height: TAB_HEIGHT,
        });
        let content_top = MARGIN + TAB_HEIGHT as i32 + MARGIN;
        let status = Rect {
            x: MARGIN,
            y: height as i32 - MARGIN - STATUS_HEIGHT,
            width: inner,
            height: STATUS_HEIGHT as u32,
        };
        let content = Rect {
            x: MARGIN,
            y: content_top,
            width: inner,
            height: (status.y - MARGIN - content_top).max(1) as u32,
        };
        let row_y = content_top;
        self.end.bounds = Rect {
            x: w - MARGIN - END_WIDTH as i32,
            y: row_y,
            width: END_WIDTH,
            height: CONTROL_HEIGHT,
        };
        self.inspect.bounds = Rect {
            x: self.end.bounds.x - MARGIN - INSPECT_WIDTH as i32,
            y: row_y,
            width: INSPECT_WIDTH,
            height: CONTROL_HEIGHT,
        };
        let filter_x = MARGIN + FILTER_LABEL_WIDTH;
        self.filter.bounds = Rect {
            x: filter_x,
            y: row_y,
            width: (self.inspect.bounds.x - MARGIN - filter_x).max(40) as u32,
            height: CONTROL_HEIGHT,
        };
        let header_y = row_y + CONTROL_HEIGHT as i32 + MARGIN;
        let panel_y = status.y - MARGIN - PANEL_HEIGHT;
        let list_y = header_y + HEADER_HEIGHT + 2;
        self.list.set_bounds(Rect {
            x: MARGIN,
            y: list_y,
            width: inner,
            height: (panel_y - MARGIN - list_y).max(ROW_HEIGHT as i32) as u32,
        });
        self.layout = Layout {
            content,
            header: Rect {
                x: MARGIN,
                y: header_y,
                width: inner,
                height: HEADER_HEIGHT as u32,
            },
            panel: Rect {
                x: MARGIN,
                y: panel_y,
                width: inner,
                height: PANEL_HEIGHT as u32,
            },
            status,
        };
        if let Some(dialog) = &mut self.dialog {
            dialog.layout((width, height));
        }
        self.refresh_rows();
    }

    /// Recompute the visible rows from the filter and the latest view, and
    /// keep the selection on the same process while it is still listed.
    fn refresh_rows(&mut self) {
        let query = self.filter.text().trim().to_lowercase();
        let rows = &self.monitor.view.processes;
        self.visible = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| query.is_empty() || matches_query(row, &query))
            .map(|(index, _)| index)
            .collect();
        self.visible_names = self
            .visible
            .iter()
            .map(|&index| rows[index].name.clone())
            .collect();
        self.list.set_len(self.visible.len());
        let position = self.selected_pid.and_then(|pid| {
            self.visible
                .iter()
                .position(|&index| rows[index].pid == pid)
        });
        match position {
            Some(row) => self.list.select_without_scrolling(row),
            None => {
                self.list.clear_selection();
                self.selected_pid = None;
            }
        }
        self.sync_controls();
    }

    /// Enable the buttons that apply and set which control holds focus.
    fn sync_controls(&mut self) {
        let processes = self.tabs.selected == PROCESSES;
        let has_selection = processes && self.selected_pid.is_some();
        self.inspect.enabled = has_selection;
        self.end.enabled = has_selection;
        let mut order = vec![TABS_ID];
        if processes {
            order.push(FILTER_ID);
            order.push(LIST_ID);
            if self.inspect.enabled {
                order.push(INSPECT_ID);
            }
            if self.end.enabled {
                order.push(END_ID);
            }
        }
        self.focus.set_ids(order);
        let focused = self.focused();
        self.tabs.focused = focused == Some(Focus::Tabs);
        self.filter.focused = focused == Some(Focus::Filter);
        self.list.focused = focused == Some(Focus::List);
        self.inspect.focused = focused == Some(Focus::Inspect);
        self.end.focused = focused == Some(Focus::End);
    }

    fn focused(&self) -> Option<Focus> {
        match self.focus.focused()? {
            TABS_ID => Some(Focus::Tabs),
            FILTER_ID => Some(Focus::Filter),
            LIST_ID => Some(Focus::List),
            INSPECT_ID => Some(Focus::Inspect),
            END_ID => Some(Focus::End),
            _ => None,
        }
    }

    fn focus_on(&mut self, control: Focus) {
        let id = match control {
            Focus::Tabs => TABS_ID,
            Focus::Filter => FILTER_ID,
            Focus::List => LIST_ID,
            Focus::Inspect => INSPECT_ID,
            Focus::End => END_ID,
        };
        self.focus.focus(id);
        self.sync_controls();
    }

    fn sample(&mut self, control: &mut Control) {
        let snapshot = self.source.sample(Instant::now());
        self.monitor.ingest(snapshot);
        self.refresh_rows();
        control.request_redraw();
    }

    /// Start the one-second timer unless it is running or the window is hidden.
    fn arm_sampling(&mut self, control: &mut Control) {
        if self.visible_window && !self.timer_armed {
            control.start_timer(SAMPLE_PERIOD, SAMPLE_TIMER);
            self.timer_armed = true;
        }
    }

    /// Show tab `index`. The strip may already have selected it (pointer and
    /// keyboard events do that), so this only makes the change take effect.
    fn change_tab(&mut self, index: usize) {
        self.tabs.select(index);
        // The GPU is probed the first time its tab is shown, then kept.
        if index == GPU && self.gpu.is_none() {
            self.gpu = Some(self.source.gpu());
        }
    }

    fn inspect_selected(&mut self) {
        match self.selected_pid {
            Some(pid) => {
                self.inspected_pid = Some(pid);
                self.status = format!("Inspecting PID {pid}.");
            }
            None => self.status = String::from("Select a process first."),
        }
    }

    fn selected_row(&self) -> Option<&ProcessRow> {
        let pid = self.selected_pid?;
        self.view().processes.iter().find(|row| row.pid == pid)
    }

    fn target_of(row: &ProcessRow) -> Target {
        Target {
            pid: row.pid,
            start_ticks: row.start_ticks,
            uid: row.uid,
            name: row.name.clone(),
        }
    }

    fn log_end(&self, target: &Target, outcome: &str) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|time| time.as_secs())
            .unwrap_or(0);
        eprintln!("{}", policy::action_log_line(now, target, outcome));
    }

    /// The first check, when "End process" is chosen: opens the confirmation.
    fn request_end(&mut self) {
        let Some(row) = self.selected_row() else {
            self.status = String::from("Select a process first.");
            return;
        };
        let target = Self::target_of(row);
        let user = self.source.current_user();
        if let Err(refusal) = policy::may_offer_end(target.pid, target.uid, user) {
            self.status = format!("Refused: {refusal}.");
            self.log_end(&target, &format!("refused: {refusal}"));
            return;
        }
        let message = format!(
            "End \"{}\" (PID {})? It receives SIGTERM, which asks it to quit. Unsaved work in it may be lost.",
            target.name, target.pid
        );
        let mut dialog = Dialog::new(DIALOG_ID, "End process?", &message, &["No", "Yes"]);
        dialog.set_default(0);
        dialog.layout((self.width, self.height));
        self.dialog = Some(dialog);
        self.pending_end = Some(target);
    }

    fn cancel_end(&mut self) {
        if let Some(target) = self.pending_end.take() {
            self.log_end(&target, "cancelled");
        }
        self.status = String::from("End process cancelled.");
    }

    /// The second check, after "Yes", against `/proc` as it is now.
    fn confirm_end(&mut self, control: &mut Control) {
        let Some(target) = self.pending_end.take() else {
            return;
        };
        let identity = self.source.identity(target.pid);
        let user = self.source.current_user();
        if let Err(refusal) = policy::may_send(&target, identity.as_ref(), user) {
            self.status = format!("Not ended: {refusal}.");
            self.log_end(&target, &format!("refused: {refusal}"));
            return;
        }
        match self.source.send_sigterm(target.pid) {
            Ok(()) => {
                self.status = format!(
                    "Sent SIGTERM to \"{}\" (PID {}). Checking...",
                    target.name, target.pid
                );
                self.log_end(&target, "sent SIGTERM");
                control.start_timer(READBACK_DELAY, READBACK_TIMER);
                self.pending_readback = Some(target);
            }
            Err(error) => {
                self.status = format!("Could not send SIGTERM to PID {}: {error}.", target.pid);
                self.log_end(&target, &format!("failed: {error}"));
            }
        }
    }

    fn read_back(&mut self, control: &mut Control) {
        let Some(target) = self.pending_readback.take() else {
            return;
        };
        let identity = self.source.identity(target.pid);
        match policy::read_back(&target, identity.as_ref()) {
            Readback::Ended => {
                self.status = format!("PID {} (\"{}\") has ended.", target.pid, target.name);
                self.log_end(&target, "read-back: ended");
            }
            Readback::StillRunning => {
                self.status = format!("PID {} is still running after SIGTERM.", target.pid);
                self.log_end(&target, "read-back: still running");
            }
        }
        self.sample(control);
    }

    /// Route a pointer event to the tab strip, the filter, the list and the
    /// buttons. Returns whether anything changed.
    fn pointer(&mut self, event: &Event, control: &mut Control) -> bool {
        match self.tabs.handle(event) {
            TabOutcome::Selected(index) => {
                self.change_tab(index);
                self.focus_on(Focus::Tabs);
                return true;
            }
            TabOutcome::FocusRequested => {
                self.focus_on(Focus::Tabs);
                return true;
            }
            TabOutcome::Ignored => {}
        }
        if self.tabs.selected != PROCESSES {
            return false;
        }
        let mut changed = false;
        match self.filter.handle(event) {
            TextFieldOutcome::Ignored => {}
            TextFieldOutcome::FocusRequested => {
                self.focus_on(Focus::Filter);
                changed = true;
            }
            _ => {
                self.refresh_rows();
                changed = true;
            }
        }
        changed |= self.list_event(event, control);
        match self.inspect.handle(event) {
            ButtonOutcome::Activated => {
                self.focus_on(Focus::Inspect);
                self.inspect_selected();
                return true;
            }
            ButtonOutcome::Changed => changed = true,
            ButtonOutcome::Ignored => {}
        }
        match self.end.handle(event) {
            ButtonOutcome::Activated => {
                self.focus_on(Focus::End);
                self.request_end();
                return true;
            }
            ButtonOutcome::Changed => changed = true,
            ButtonOutcome::Ignored => {}
        }
        changed
    }

    /// Give the list an event; selection changes are mirrored into `selected_pid`.
    fn list_event(&mut self, event: &Event, control: &Control) -> bool {
        let names = &self.visible_names;
        let outcome = self.list.handle(event, control.modifiers(), &|index| {
            names.get(index).cloned().unwrap_or_default()
        });
        match outcome {
            ListOutcome::Ignored => false,
            ListOutcome::Changed => true,
            ListOutcome::SelectionChanged => {
                self.sync_selection();
                true
            }
            ListOutcome::Activated(_) => {
                self.focus_on(Focus::List);
                self.sync_selection();
                self.inspect_selected();
                true
            }
            ListOutcome::ContextMenu { .. } => {
                self.sync_selection();
                false
            }
            // A press moves the selection and asks for focus at once.
            ListOutcome::FocusRequested => {
                self.focus_on(Focus::List);
                self.sync_selection();
                true
            }
        }
    }

    fn sync_selection(&mut self) {
        self.selected_pid = self
            .list
            .selection()
            .first()
            .and_then(|&row| self.visible.get(row))
            .map(|&index| self.view().processes[index].pid);
        self.sync_controls();
    }

    fn keyboard(&mut self, event: &Event, control: &mut Control) -> bool {
        if self.focus.handle(event).is_some() {
            self.sync_controls();
            return true;
        }
        match self.focused() {
            Some(Focus::Tabs) => match self.tabs.handle(event) {
                TabOutcome::Selected(index) => {
                    self.change_tab(index);
                    true
                }
                TabOutcome::FocusRequested => true,
                TabOutcome::Ignored => false,
            },
            Some(Focus::Filter) => {
                let outcome = self.filter.handle(event);
                if outcome != TextFieldOutcome::Ignored {
                    self.refresh_rows();
                    return true;
                }
                false
            }
            Some(Focus::List) => self.list_event(event, control),
            Some(Focus::Inspect) => {
                if self.inspect.handle(event) == ButtonOutcome::Activated {
                    self.inspect_selected();
                    return true;
                }
                false
            }
            Some(Focus::End) => {
                if self.end.handle(event) == ButtonOutcome::Activated {
                    self.request_end();
                    return true;
                }
                false
            }
            None => false,
        }
    }

    fn accessibility_request(
        &mut self,
        request: &AccessibilityRequest,
        control: &mut Control,
    ) -> bool {
        match request {
            AccessibilityRequest::Click(id) if *id == INSPECT_ID => {
                if self.inspect.enabled {
                    self.focus_on(Focus::Inspect);
                    self.inspect_selected();
                }
                true
            }
            AccessibilityRequest::Click(id) if *id == END_ID => {
                if self.end.enabled {
                    self.focus_on(Focus::End);
                    self.request_end();
                }
                true
            }
            AccessibilityRequest::Click(id) => {
                if let Some(index) = self.list_row_for_node(*id) {
                    self.list.select_without_scrolling(index);
                    self.focus_on(Focus::List);
                    self.sync_selection();
                    return true;
                }
                match self.tabs.handle(&Event::Accessibility(request.clone())) {
                    TabOutcome::Selected(index) => {
                        self.change_tab(index);
                        self.focus_on(Focus::Tabs);
                        control.request_redraw();
                        true
                    }
                    _ => false,
                }
            }
            AccessibilityRequest::Focus(id) => {
                let target = if *id == TABS_ID || *id >> 32 == TABS_ID {
                    Focus::Tabs
                } else if *id == FILTER_ID {
                    Focus::Filter
                } else if *id == LIST_ID || *id >> 32 == LIST_ID {
                    Focus::List
                } else if *id == INSPECT_ID {
                    Focus::Inspect
                } else if *id == END_ID {
                    Focus::End
                } else {
                    return false;
                };
                let reachable = match target {
                    Focus::Tabs | Focus::Filter | Focus::List => true,
                    Focus::Inspect => self.inspect.enabled,
                    Focus::End => self.end.enabled,
                };
                if reachable {
                    self.focus_on(target);
                }
                true
            }
            AccessibilityRequest::SetValue(id, value) if *id == FILTER_ID => {
                self.filter.set_text(value);
                self.refresh_rows();
                true
            }
            _ => false,
        }
    }

    fn list_row_for_node(&self, node: u64) -> Option<usize> {
        self.list
            .row_for_node(node)
            .filter(|&index| index < self.visible.len())
    }

    /// Non-pointer, non-keyboard window input: accessibility, paste, scroll.
    fn route(&mut self, event: &Event, control: &mut Control) -> bool {
        match event {
            Event::PointerMoved { .. }
            | Event::PointerPressed { .. }
            | Event::PointerReleased { .. }
            | Event::PointerLeft
            | Event::DoubleClicked { .. } => self.pointer(event, control),
            Event::KeyPressed { .. } => self.keyboard(event, control),
            Event::Accessibility(request) => self.accessibility_request(request, control),
            Event::Paste(text) if self.focused() == Some(Focus::Filter) => {
                self.filter.insert(text);
                self.refresh_rows();
                true
            }
            Event::Scroll { .. } if self.tabs.selected == PROCESSES => {
                self.list_event(event, control)
            }
            _ => false,
        }
    }

    /// The labels of the current view: the CPU, Memory, Disk or GPU lines, or
    /// the Processes column titles and inspection lines; always the status line.
    fn labels(&self) -> Vec<Label> {
        let mut labels = Vec::new();
        let status = self.status_text();
        if self.tabs.selected == PROCESSES {
            let columns = process_columns(self.list.bounds().width);
            for (index, title) in PROCESS_TITLES.iter().enumerate() {
                let (offset, width) = columns[index];
                labels.push(Label {
                    text: (*title).to_owned(),
                    rect: Rect {
                        x: self.list.bounds().x + offset,
                        y: self.layout.header.y,
                        width,
                        height: HEADER_HEIGHT as u32,
                    },
                    bold: true,
                    fraction: None,
                    secondary: false,
                });
            }
            for (line, text) in self.inspection_lines().into_iter().enumerate() {
                labels.push(Label {
                    text,
                    rect: Rect {
                        x: self.layout.panel.x + 8,
                        y: self.layout.panel.y + 3 + line as i32 * LINE_HEIGHT,
                        width: self.layout.panel.width.saturating_sub(16),
                        height: LINE_HEIGHT as u32,
                    },
                    bold: false,
                    fraction: None,
                    secondary: false,
                });
            }
        } else {
            let entries = self.entries();
            let columns = if self.tabs.selected == CPU {
                (self.layout.content.width / CPU_COLUMN_WIDTH).max(1) as usize
            } else {
                1
            };
            let column_width = self.layout.content.width as usize / columns;
            for (index, entry) in entries.into_iter().enumerate() {
                let column = index % columns;
                let row = index / columns;
                let x = self.layout.content.x + (column * column_width) as i32;
                let y = self.layout.content.y + row as i32 * ENTRY_HEIGHT;
                labels.push(Label {
                    text: entry.text,
                    rect: Rect {
                        x,
                        y,
                        width: column_width.saturating_sub(12).max(1) as u32,
                        height: LINE_HEIGHT as u32,
                    },
                    bold: false,
                    fraction: entry.fraction,
                    secondary: false,
                });
            }
        }
        labels.push(Label {
            text: status,
            rect: self.layout.status,
            bold: false,
            fraction: None,
            secondary: true,
        });
        labels
    }

    fn status_text(&self) -> String {
        if !self.status.is_empty() {
            return self.status.clone();
        }
        match self.tabs.selected {
            PROCESSES => format!(
                "Showing {} of {} processes. Refreshes every second while this window is visible.",
                self.visible.len(),
                self.view().processes.len()
            ),
            _ => String::from("Refreshes every second while this window is visible."),
        }
    }

    fn inspection_lines(&self) -> Vec<String> {
        let Some(pid) = self.inspected_pid else {
            return vec![String::from(
                "Select a process and press Inspect to see its details.",
            )];
        };
        let Some(row) = self.view().processes.iter().find(|row| row.pid == pid) else {
            return vec![format!("PID {pid} is no longer running.")];
        };
        let threads = row
            .threads
            .map_or_else(|| String::from("unavailable"), |count| count.to_string());
        let memory = row
            .rss_bytes
            .map_or_else(|| String::from("unavailable"), format::bytes);
        let command = if row.cmdline.is_empty() {
            String::from("unavailable (a kernel thread, or its arguments are not readable)")
        } else {
            row.cmdline.clone()
        };
        let mut lines = vec![
            format!(
                "Name: {}    PID: {}    Parent PID: {}",
                row.name, row.pid, row.ppid
            ),
            format!(
                "User: {}    State: {} ({})    Threads: {threads}",
                row.user,
                row.state,
                procfs::state_name(row.state)
            ),
            format!(
                "CPU: {}    Memory: {memory}",
                row.cpu_percent
                    .map_or_else(|| String::from("measuring"), format::percent)
            ),
        ];
        let width = self.layout.panel.width.saturating_sub(16);
        let command_line = format!("Command: {command}");
        lines.extend(
            dialog::wrap(&command_line, &self.font, width.max(1))
                .into_iter()
                .take(3),
        );
        lines
    }

    fn entries(&self) -> Vec<Entry> {
        let view = self.view();
        match self.tabs.selected {
            CPU => match &view.cpu {
                None => vec![Entry::text(
                    "CPU usage: unavailable (/proc/stat could not be read)",
                )],
                Some(cpu) => {
                    let mut entries =
                        vec![Entry::text(format!("Logical CPUs: {}", cpu.logical_cpus))];
                    entries.push(percent_entry("Total usage", cpu.total_percent));
                    for (index, percent) in cpu.per_cpu_percent.iter().enumerate() {
                        entries.push(percent_entry(&format!("CPU {index}"), *percent));
                    }
                    entries
                }
            },
            MEMORY => match view.memory {
                None => vec![Entry::text(
                    "Memory: unavailable (/proc/meminfo could not be read)",
                )],
                Some(info) => {
                    let used = match (info.total, info.available) {
                        (Some(total), Some(available)) => Some(total.saturating_sub(available)),
                        _ => None,
                    };
                    vec![
                        size_entry("Total", info.total, None),
                        size_entry("Used", used, info.total),
                        size_entry("Cached", info.cached, info.total),
                        size_entry("Available", info.available, info.total),
                        size_entry("Free", info.free, info.total),
                    ]
                }
            },
            DISK => {
                let disk = &view.disk;
                let mut entries = Vec::new();
                match disk.filesystem {
                    Some(space) if space.total > 0 => entries.push(Entry {
                        text: format!(
                            "Root file system (/): {} available of {}",
                            format::bytes(space.free),
                            format::bytes(space.total)
                        ),
                        fraction: Some(
                            1.0 - space.free.min(space.total) as f64 / space.total as f64,
                        ),
                    }),
                    _ => entries.push(Entry::text(
                        "Root file system (/): unavailable (statvfs failed)",
                    )),
                }
                entries.push(Entry::text(format!(
                    "Whole disks counted: {}",
                    disk.whole_disks
                )));
                entries.push(rate_entry(
                    "Read",
                    disk.read_bytes_per_second,
                    disk.whole_disks,
                ));
                entries.push(rate_entry(
                    "Write",
                    disk.written_bytes_per_second,
                    disk.whole_disks,
                ));
                entries
            }
            _ => match &self.gpu {
                None => vec![Entry::text("GPU: not probed yet")],
                Some(GpuInfo::Adapter { name, backend }) => vec![
                    Entry::text(format!("Adapter: {name}")),
                    Entry::text(format!("Backend: {backend}")),
                ],
                Some(GpuInfo::Unavailable(reason)) => vec![
                    Entry::text("Adapter: unavailable"),
                    Entry::text(format!("Reason: {reason}")),
                ],
            },
        }
    }

    fn paint_labels(&self, surface: &RasterSurface, labels: &[Label]) -> Result<(), RenderError> {
        let colors = palette();
        for label in labels {
            let font = if label.bold { &self.bold } else { &self.font };
            let color = if label.secondary {
                colors.ink_secondary
            } else {
                colors.ink
            };
            let text = fit(&label.text, font, label.rect.width);
            surface.draw_text(&text, label.rect.x, label.rect.y + 1, font, color)?;
            if let Some(fraction) = label.fraction {
                let bar = Rect {
                    x: label.rect.x,
                    y: label.rect.y + LINE_HEIGHT,
                    width: label.rect.width,
                    height: BAR_HEIGHT as u32,
                };
                paint_bar(surface, bar, fraction)?;
            }
        }
        Ok(())
    }

    fn paint_processes(&mut self, surface: &RasterSurface) -> Result<(), RenderError> {
        let colors = palette();
        let filter_label = Rect {
            x: MARGIN,
            y: self.filter.bounds.y + 5,
            width: FILTER_LABEL_WIDTH as u32,
            height: LINE_HEIGHT as u32,
        };
        surface.draw_text(
            "Filter:",
            filter_label.x,
            filter_label.y,
            &self.font,
            colors.ink,
        )?;
        self.filter.paint(surface)?;
        self.inspect.paint(surface, &self.font)?;
        self.end.paint(surface, &self.font)?;
        let columns = process_columns(self.list.bounds().width);
        let list_x = self.list.bounds().x;
        let names = &self.visible_names;
        let rows = &self.view().processes;
        let visible = &self.visible;
        let cpu_known = self.view().cpu.is_some();
        self.list.paint(surface, |surface, index, rect, selected| {
            let Some(&row_index) = visible.get(index) else {
                return Ok(());
            };
            let row = &rows[row_index];
            let ink = if selected {
                colors.selection_ink
            } else {
                colors.ink
            };
            let cells = [
                names[index].clone(),
                row.pid.to_string(),
                match row.cpu_percent {
                    Some(percent) => format::percent(percent),
                    None if cpu_known => String::from("measuring"),
                    None => String::from("unavailable"),
                },
                row.rss_bytes
                    .map_or_else(|| String::from("unavailable"), format::bytes),
                format!("{} {}", row.state, procfs::state_name(row.state)),
                row.user.clone(),
            ];
            for (column, text) in cells.iter().enumerate() {
                let (offset, width) = columns[column];
                let text = fit(text, &self.font, width.saturating_sub(6));
                surface.draw_text(&text, list_x + offset + 3, rect.y + 3, &self.font, ink)?;
            }
            Ok(())
        })?;
        let panel = self.layout.panel;
        surface.fill_rect(panel, colors.content)?;
        surface.stroke_rect(panel, colors.bevel_shadow)?;
        Ok(())
    }
}

const PROCESS_TITLES: [&str; 6] = ["Name", "PID", "CPU", "Memory", "State", "User"];

fn matches_query(row: &ProcessRow, query: &str) -> bool {
    row.name.to_lowercase().contains(query)
        || row.pid.to_string().contains(query)
        || row.user.to_lowercase().contains(query)
        || row.cmdline.to_lowercase().contains(query)
}

/// Column offsets and widths for the process table, from the list's width.
/// The name column takes what the fixed columns leave.
fn process_columns(width: u32) -> [(i32, u32); 6] {
    const PID: u32 = 56;
    const CPU: u32 = 64;
    const MEMORY: u32 = 88;
    const STATE: u32 = 128;
    const USER: u32 = 104;
    let fixed = PID + CPU + MEMORY + STATE + USER;
    let name = width.saturating_sub(fixed).max(80);
    let mut columns = [(0, 0); 6];
    let mut x = 0i32;
    for (index, col_width) in [name, PID, CPU, MEMORY, STATE, USER]
        .into_iter()
        .enumerate()
    {
        columns[index] = (x, col_width);
        x += col_width as i32;
    }
    columns
}

fn percent_entry(label: &str, value: Option<f64>) -> Entry {
    match value {
        Some(percent) => Entry {
            text: format!("{label}: {}", format::percent(percent)),
            fraction: Some(percent / 100.0),
        },
        None => Entry::text(format!("{label}: measuring")),
    }
}

fn size_entry(label: &str, value: Option<u64>, total: Option<u64>) -> Entry {
    match value {
        None => Entry::text(format!("{label}: unavailable")),
        Some(bytes) => match total.filter(|total| *total > 0) {
            Some(total) => Entry {
                text: format!(
                    "{label}: {} ({})",
                    format::bytes(bytes),
                    format::percent(bytes as f64 / total as f64 * 100.0)
                ),
                fraction: Some((bytes as f64 / total as f64).min(1.0)),
            },
            None => Entry::text(format!("{label}: {}", format::bytes(bytes))),
        },
    }
}

fn rate_entry(label: &str, rate: Option<f64>, whole_disks: usize) -> Entry {
    if whole_disks == 0 {
        return Entry::text(format!(
            "{label}: unavailable (no whole disks in /proc/diskstats)"
        ));
    }
    match rate {
        Some(bytes_per_second) => {
            Entry::text(format!("{label}: {}", format::rate(bytes_per_second)))
        }
        None => Entry::text(format!("{label}: measuring")),
    }
}

/// Shorten `text` with an ellipsis so it fits `max_width` logical pixels.
fn fit(text: &str, font: &Font, max_width: u32) -> String {
    let fits = |candidate: &str| measure(candidate, font).width <= max_width as f32;
    if fits(text) {
        return text.to_owned();
    }
    let mut kept: Vec<char> = text.chars().collect();
    while !kept.is_empty() {
        kept.pop();
        let candidate: String = kept.iter().collect::<String>() + "…";
        if fits(&candidate) {
            return candidate;
        }
    }
    String::new()
}

/// AccessKit bounds for a logical rectangle.
fn bounds(rect: Rect) -> accesskit::Rect {
    accesskit::Rect {
        x0: f64::from(rect.x),
        y0: f64::from(rect.y),
        x1: f64::from(rect.x) + f64::from(rect.width),
        y1: f64::from(rect.y) + f64::from(rect.height),
    }
}

fn paint_bar(surface: &RasterSurface, bar: Rect, fraction: f64) -> Result<(), RenderError> {
    let colors = palette();
    surface.fill_rect(bar, colors.content)?;
    let filled = (f64::from(bar.width) * fraction.clamp(0.0, 1.0)).round() as u32;
    if filled > 0 {
        surface.fill_rect(
            Rect {
                x: bar.x,
                y: bar.y,
                width: filled,
                height: bar.height,
            },
            colors.selection,
        )?;
    }
    surface.stroke_rect(bar, colors.bevel_shadow)
}

impl<S: Source> App for SystemMonitor<S> {
    fn event(&mut self, event: &Event, control: &mut Control) {
        match event {
            Event::CloseRequested => {
                control.quit();
                return;
            }
            Event::Visibility(visible) => {
                self.visible_window = *visible;
                if *visible {
                    self.sample(control);
                }
                self.arm_sampling(control);
                return;
            }
            Event::Timer(SAMPLE_TIMER) => {
                self.timer_armed = false;
                if self.visible_window {
                    self.sample(control);
                    self.arm_sampling(control);
                }
                return;
            }
            Event::Timer(READBACK_TIMER) => {
                self.read_back(control);
                return;
            }
            Event::Resized { width, height } => {
                self.relayout(*width, *height);
                control.request_redraw();
                return;
            }
            _ => {}
        }
        if let Some(dialog) = &mut self.dialog {
            let outcome = dialog.handle(event);
            match outcome {
                DialogOutcome::Ignored | DialogOutcome::Changed => {}
                DialogOutcome::Chosen { index, .. } => {
                    self.dialog = None;
                    if index == 1 {
                        self.confirm_end(control);
                    } else {
                        self.cancel_end();
                    }
                    self.sync_controls();
                }
                DialogOutcome::Copy(text) => control.set_clipboard(&text),
                DialogOutcome::PasteRequested => control.request_paste(),
            }
            control.request_redraw();
            return;
        }
        if self.route(event, control) {
            control.request_redraw();
        }
    }

    fn paint(&mut self, surface: &RasterSurface) {
        let colors = palette();
        if let Err(error) = surface.clear(colors.surface) {
            log::warn!("system monitor: cannot clear the window: {error}");
        }
        if let Err(error) = self.paint_window(surface) {
            log::warn!("system monitor: cannot paint: {error}");
        }
    }

    fn accessibility(&self) -> Option<TreeUpdate> {
        let mut nodes = Vec::new();
        let mut children = vec![NodeId(TABS_ID)];
        nodes.extend(
            self.tabs
                .accessibility_nodes()
                .into_iter()
                .map(|(id, node)| (NodeId(id), node)),
        );
        if self.tabs.selected == PROCESSES {
            children.extend([FILTER_ID, LIST_ID, INSPECT_ID, END_ID].map(NodeId));
            nodes.push((NodeId(FILTER_ID), self.filter.accessibility_node()));
            nodes.push((NodeId(INSPECT_ID), self.inspect.accessibility_node()));
            nodes.push((NodeId(END_ID), self.end.accessibility_node()));
            let names = &self.visible_names;
            nodes.extend(
                self.list
                    .accessibility_nodes(&|index| {
                        names
                            .get(index)
                            .map_or_else(String::new, |name| format!("{name}, row {}", index + 1))
                    })
                    .into_iter()
                    .map(|(id, node)| (NodeId(id), node)),
            );
        }
        for (index, label) in self.labels().into_iter().take(MAX_LINE_NODES).enumerate() {
            let id = LINE_NODE + index as u64;
            let mut node = Node::new(Role::Label);
            node.set_label(label.text.as_str());
            node.set_bounds(bounds(label.rect));
            children.push(NodeId(id));
            nodes.push((NodeId(id), node));
        }
        if let Some(dialog) = &self.dialog {
            children.push(NodeId(DIALOG_ID));
            nodes.extend(
                dialog
                    .accessibility_nodes()
                    .into_iter()
                    .map(|(id, node)| (NodeId(id), node)),
            );
        }
        let mut root = Node::new(Role::Window);
        root.set_label("System Monitor");
        root.set_children(children);
        nodes.insert(0, (NodeId(ROOT_NODE), root));
        Some(TreeUpdate {
            nodes,
            tree: Some(TreeInfo::new(NodeId(ROOT_NODE))),
            tree_id: TreeId::ROOT,
            focus: NodeId(self.focus_node()),
        })
    }
}

impl<S: Source> SystemMonitor<S> {
    fn focus_node(&self) -> u64 {
        if let Some(dialog) = &self.dialog {
            return dialog.focus_node();
        }
        match self.focused() {
            Some(Focus::Tabs) => self.tabs.focus_node(),
            Some(Focus::Filter) => FILTER_ID,
            Some(Focus::List) => self.list.focus_node(),
            Some(Focus::Inspect) => INSPECT_ID,
            Some(Focus::End) => END_ID,
            None => ROOT_NODE,
        }
    }

    fn paint_window(&mut self, surface: &RasterSurface) -> Result<(), RenderError> {
        self.tabs.paint(surface, &self.font)?;
        if self.tabs.selected == PROCESSES {
            self.paint_processes(surface)?;
        }
        let labels = self.labels();
        self.paint_labels(surface, &labels)?;
        if let Some(dialog) = &mut self.dialog {
            dialog.paint(surface)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
