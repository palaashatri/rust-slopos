//! SLOPOS Files: first native toolkit-backed file browser.
//!
//! This milestone is intentionally read-only. Destructive operations require
//! the separately audited transactional filesystem engine. Directory I/O
//! occurs on a bounded background worker, never in an X11 event callback.
mod model;

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use accesskit::{Node, NodeId, Role, TreeId, TreeInfo, TreeUpdate};
use model::{DirectoryState, Entry, Kind, Navigation};
use slopos_core::process::{resolve_program, spawn_detached};
use slopos_ui::controls::button::{Button, ButtonOutcome};
use slopos_ui::controls::list_view::{ListItem, ListOutcome, ListView};
use slopos_ui::event::{AccessibilityRequest, Event, Key};
use slopos_ui::foundation::{color::Color, geometry::Rect};
use slopos_ui::platform::{self, App, Control, Proxy, WindowOptions};
use slopos_ui::primitives::button_plate::ButtonPlateVariant;
use slopos_ui::render::RasterSurface;
use slopos_ui::text::{measure, Font};

const WIDTH: u32 = 760;
const HEIGHT: u32 = 540;
const TOOLBAR_HEIGHT: i32 = 48;
const FOOTER_HEIGHT: u32 = 34;
const LIST_ID: u64 = 50;
const FIRST_ROW_ID: u64 = 1000;
const BACKGROUND: Color = Color {
    red: 0xdd,
    green: 0xdd,
    blue: 0xdd,
    alpha: 255,
};
const BAR: Color = Color {
    red: 0xee,
    green: 0xee,
    blue: 0xee,
    alpha: 255,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Back,
    Forward,
    Up,
    Home,
    Refresh,
    Open,
}

impl Action {
    fn label(self) -> &'static str {
        match self {
            Self::Back => "Back",
            Self::Forward => "Forward",
            Self::Up => "Up",
            Self::Home => "Home",
            Self::Refresh => "Refresh",
            Self::Open => "Open",
        }
    }
}

struct Toolbar {
    button: Button,
    action: Action,
}

type ScanResult = (u64, PathBuf, io::Result<Vec<Entry>>);

struct ScanWorker {
    requests: mpsc::SyncSender<(u64, PathBuf)>,
    results: mpsc::Receiver<ScanResult>,
    wake: Arc<Mutex<Option<Proxy>>>,
}

impl ScanWorker {
    fn new() -> io::Result<Self> {
        let (requests, rx) = mpsc::sync_channel::<(u64, PathBuf)>(2);
        let (tx, results) = mpsc::channel::<ScanResult>();
        let wake: Arc<Mutex<Option<Proxy>>> = Arc::new(Mutex::new(None));
        let worker_wake = wake.clone();
        thread::Builder::new()
            .name("files-read-dir".into())
            .spawn(move || {
                while let Ok((seq, path)) = rx.recv() {
                    let result = model::read_directory(&path);
                    if tx.send((seq, path, result)).is_err() {
                        break;
                    }
                    if let Ok(guard) = worker_wake.lock() {
                        if let Some(proxy) = guard.as_ref() {
                            proxy.wake();
                        }
                    }
                }
            })?;
        Ok(Self {
            requests,
            results,
            wake,
        })
    }
}

pub struct FilesApp {
    state: DirectoryState,
    list: ListView,
    toolbar: Vec<Toolbar>,
    focused: Option<usize>, // toolbar indices, then list
    worker: ScanWorker,
    pending: Option<(u64, PathBuf, Navigation)>,
    serial: u64,
    status: String,
    width: u32,
    height: u32,
}

impl FilesApp {
    fn new(path: PathBuf) -> io::Result<Self> {
        let mut toolbar = Vec::new();
        for (idx, action) in [
            Action::Back,
            Action::Forward,
            Action::Up,
            Action::Home,
            Action::Refresh,
            Action::Open,
        ]
        .into_iter()
        .enumerate()
        {
            toolbar.push(Toolbar {
                action,
                button: Button::new(10 + idx as u64, action.label(), ButtonPlateVariant::Regular)
                    .with_bounds(Rect {
                        x: 10 + idx as i32 * 94,
                        y: 10,
                        width: 86,
                        height: 26,
                    }),
            });
        }
        let worker = ScanWorker::new()?;
        let mut app = Self {
            state: DirectoryState::new(path.clone()),
            list: ListView::new(LIST_ID, list_area(WIDTH, HEIGHT)),
            toolbar,
            focused: None,
            worker,
            pending: None,
            serial: 0,
            status: String::new(),
            width: WIDTH,
            height: HEIGHT,
        };
        app.request(path, Navigation::Refresh);
        Ok(app)
    }

    fn proxy_slot(&self) -> Arc<Mutex<Option<Proxy>>> {
        self.worker.wake.clone()
    }

    fn request(&mut self, path: PathBuf, mode: Navigation) {
        let id = self.serial.wrapping_add(1);
        match self.worker.requests.try_send((id, path.clone())) {
            Ok(()) => {
                self.serial = id;
                self.pending = Some((id, path, mode));
                self.status = "Reading folder…".into();
            }
            Err(mpsc::TrySendError::Full(_)) => self.status = "Folder worker busy. Retry.".into(),
            Err(mpsc::TrySendError::Disconnected(_)) => {
                self.status = "Folder worker unavailable.".into()
            }
        }
    }

    fn apply_result(&mut self, result: ScanResult) {
        let (id, path, listing) = result;
        let Some((pending_id, pending_path, mode)) = &self.pending else {
            return;
        };
        if id != *pending_id || path != *pending_path {
            return; // Stale asynchronous scan must not rewrite navigation.
        }
        let mode = *mode;
        self.pending = None;
        match listing {
            Ok(entries) => {
                let rows = entries
                    .iter()
                    .enumerate()
                    .map(|(index, entry)| ListItem {
                        id: FIRST_ROW_ID + index as u64,
                        title: format!(
                            "{}  {}",
                            if entry.kind == Kind::Directory {
                                "▸"
                            } else {
                                " "
                            },
                            entry.display_name()
                        ),
                        detail: entry.kind_label().into(),
                    })
                    .collect();
                if let Err(error) = self.list.set_items(rows) {
                    self.status = format!("Cannot list folder: {error}");
                    return;
                }
                if self.state.apply(path, mode, entries) {
                    self.status = format!("{} items", self.state.entries.len());
                } else {
                    self.status = "Navigation state changed; retry.".into();
                }
            }
            Err(error) => self.status = format!("Cannot read folder: {error}"),
        }
        self.refresh_actions();
    }

    fn receive(&mut self) -> bool {
        let mut changed = false;
        while let Ok(result) = self.worker.results.try_recv() {
            self.apply_result(result);
            changed = true;
        }
        changed
    }

    fn initial_snapshot(&mut self) {
        if let Ok(result) = self.worker.results.recv_timeout(Duration::from_secs(3)) {
            self.apply_result(result);
        }
    }

    fn refresh_actions(&mut self) {
        for item in &mut self.toolbar {
            item.button.enabled = match item.action {
                Action::Back => self.state.back_target().is_some(),
                Action::Forward => self.state.forward_target().is_some(),
                Action::Up => self.state.parent().is_some(),
                Action::Open => self.list.selected().is_some(),
                _ => true,
            };
        }
    }

    fn focus(&mut self, index: usize) {
        self.focused = Some(index);
        for (i, item) in self.toolbar.iter_mut().enumerate() {
            item.button.focused = i == index;
        }
        self.list.focused = index == self.toolbar.len();
    }

    fn focus_next(&mut self, reverse: bool) {
        let len = self.toolbar.len() + 1;
        let next = self
            .focused
            .map_or(if reverse { len - 1 } else { 0 }, |index| {
                if reverse {
                    (index + len - 1) % len
                } else {
                    (index + 1) % len
                }
            });
        self.focus(next);
    }

    fn open_selected(&mut self) {
        let Some(index) = self.list.selected() else {
            self.status = "Select a file or folder first.".into();
            return;
        };
        let Some(row) = self.state.row(index) else {
            self.status = "Selection changed. Refresh the folder.".into();
            return;
        };
        let target = row.path.clone();
        match row.kind {
            Kind::Directory => self.request(target, Navigation::New),
            Kind::File => match resolve_program("xdg-open") {
                Some(opener) => match spawn_detached(&opener, [target.as_os_str()]) {
                    Ok(_) => self.status = format!("Opening {}", target.display()),
                    Err(error) => self.status = format!("Open failed: {error}"),
                },
                None => self.status = "xdg-open is unavailable; file has not been opened.".into(),
            },
            Kind::Symlink => {
                self.status = "Symbolic links are not followed by this read-only viewer.".into()
            }
            Kind::Other => self.status = "This file type cannot be opened.".into(),
        }
    }

    fn apply_action(&mut self, action: Action) {
        match action {
            Action::Back => {
                if let Some(path) = self.state.back_target().map(Path::to_path_buf) {
                    self.request(path, Navigation::Back);
                }
            }
            Action::Forward => {
                if let Some(path) = self.state.forward_target().map(Path::to_path_buf) {
                    self.request(path, Navigation::Forward);
                }
            }
            Action::Up => {
                if let Some(path) = self.state.parent() {
                    self.request(path, Navigation::New);
                }
            }
            Action::Home => {
                if let Some(path) = std::env::var_os("HOME").map(PathBuf::from) {
                    self.request(path, Navigation::New);
                } else {
                    self.status = "HOME is unavailable.".into();
                }
            }
            Action::Refresh => self.request(self.state.current.clone(), Navigation::Refresh),
            Action::Open => self.open_selected(),
        }
    }
}

fn list_area(width: u32, height: u32) -> Rect {
    Rect {
        x: 12,
        y: TOOLBAR_HEIGHT + 32,
        width: width.saturating_sub(24),
        height: height.saturating_sub(TOOLBAR_HEIGHT as u32 + FOOTER_HEIGHT + 42),
    }
}

fn clipped(text: &str, font: &Font, max: u32) -> String {
    if measure(text, font).width <= max as f32 {
        return text.to_owned();
    }
    let mut end = String::new();
    for c in text.chars().take(192) {
        let mut candidate = end.clone();
        candidate.push(c);
        candidate.push('…');
        if measure(&candidate, font).width > max as f32 {
            break;
        }
        end.push(c);
    }
    end.push('…');
    end
}

impl App for FilesApp {
    fn event(&mut self, event: &Event, control: &mut Control) {
        match event {
            Event::CloseRequested => control.quit(),
            Event::Resized { width, height } => {
                self.width = *width;
                self.height = *height;
                self.list.set_bounds(list_area(*width, *height));
                control.request_redraw();
            }
            Event::Wake => {
                if self.receive() {
                    control.request_redraw();
                    control.set_title(&format!("Files — {}", self.state.current.display()));
                }
            }
            Event::KeyPressed {
                key: Key::Tab,
                modifiers,
            } => {
                self.focus_next(modifiers.shift);
                control.request_redraw();
            }
            Event::KeyPressed {
                key: Key::Backspace,
                modifiers,
            } if !modifiers.alt && !modifiers.control => {
                self.apply_action(Action::Up);
                control.request_redraw();
            }
            Event::Accessibility(AccessibilityRequest::Click(id)) => {
                if let Some(index) = self.toolbar.iter().position(|b| b.button.id == *id) {
                    self.apply_action(self.toolbar[index].action);
                    control.request_redraw();
                } else if let Some(index) = self.list.index_for_id(*id) {
                    self.list.select(Some(index));
                    self.focus(self.toolbar.len());
                    self.refresh_actions();
                    control.request_redraw();
                }
            }
            Event::Accessibility(AccessibilityRequest::Focus(id)) => {
                if let Some(index) = self.toolbar.iter().position(|b| b.button.id == *id) {
                    self.focus(index);
                    control.request_redraw();
                } else if let Some(index) = self.list.index_for_id(*id) {
                    self.list.select(Some(index));
                    self.focus(self.toolbar.len());
                    self.refresh_actions();
                    control.request_redraw();
                }
            }
            _ => {
                let list_result = self.list.handle(event);
                match list_result {
                    ListOutcome::Activated(_) => {
                        self.open_selected();
                        control.request_redraw();
                    }
                    ListOutcome::Changed => {
                        if matches!(event, Event::PointerPressed { .. }) {
                            self.focus(self.toolbar.len());
                        }
                        self.refresh_actions();
                        control.request_redraw();
                    }
                    ListOutcome::Ignored => {}
                }
                let mut action = None;
                for item in &mut self.toolbar {
                    match item.button.handle(event) {
                        ButtonOutcome::Activated => action = Some(item.action),
                        ButtonOutcome::Changed => control.request_redraw(),
                        ButtonOutcome::Ignored => {}
                    }
                }
                if let Some(action) = action {
                    self.apply_action(action);
                    control.request_redraw();
                }
            }
        }
    }

    fn paint(&mut self, surface: &RasterSurface) {
        let _ = surface.clear(BACKGROUND);
        let font = Font::system(13.0);
        let small = Font::system(12.0);
        let _ = surface.fill_rect(
            Rect {
                x: 0,
                y: 0,
                width: self.width,
                height: TOOLBAR_HEIGHT as u32,
            },
            BACKGROUND,
        );
        for item in &self.toolbar {
            let _ = item.button.paint(surface, &font);
        }
        let _ = surface.fill_rect(
            Rect {
                x: 0,
                y: TOOLBAR_HEIGHT,
                width: self.width,
                height: 31,
            },
            BAR,
        );
        let path = clipped(
            &self.state.current.display().to_string(),
            &small,
            self.width.saturating_sub(24),
        );
        let _ = surface.draw_text(&path, 12, TOOLBAR_HEIGHT + 7, &small, Color::BLACK);
        let _ = self.list.paint(surface, &font);
        let y = self.height.saturating_sub(FOOTER_HEIGHT) as i32;
        let _ = surface.fill_rect(
            Rect {
                x: 0,
                y,
                width: self.width,
                height: FOOTER_HEIGHT,
            },
            BAR,
        );
        let text = clipped(&self.status, &small, self.width.saturating_sub(24));
        let _ = surface.draw_text(&text, 12, y + 6, &small, Color::BLACK);
    }

    fn accessibility(&self) -> Option<TreeUpdate> {
        let mut window = Node::new(Role::Window);
        window.set_label("SLOPOS Files");
        let mut path = Node::new(Role::Label);
        path.set_label(self.state.current.display().to_string());
        let mut status = Node::new(Role::Label);
        status.set_label(self.status.as_str());
        let mut nodes = vec![(NodeId(1), path), (NodeId(2), status)];
        let mut children = vec![NodeId(1), NodeId(2)];
        for item in &self.toolbar {
            nodes.push((NodeId(item.button.id), item.button.accessibility_node()));
            children.push(NodeId(item.button.id));
        }
        let (mut list, rows) = self.list.accessibility_nodes();
        list.set_label("Folder contents");
        nodes.push((NodeId(LIST_ID), list));
        children.push(NodeId(LIST_ID));
        nodes.extend(rows);
        window.set_children(children);
        nodes.insert(0, (NodeId(0), window));
        let focus = if let Some(focused) = self.focused {
            if focused == self.toolbar.len() {
                self.list
                    .selected()
                    .and_then(|index| self.state.row(index))
                    .map(|_| NodeId(FIRST_ROW_ID + self.list.selected().unwrap_or(0) as u64))
                    .unwrap_or(NodeId(LIST_ID))
            } else {
                NodeId(self.toolbar[focused].button.id)
            }
        } else {
            NodeId(0)
        };
        Some(TreeUpdate {
            nodes,
            tree: Some(TreeInfo::new(NodeId(0))),
            tree_id: TreeId::ROOT,
            focus,
        })
    }
}

fn main() {
    if std::env::var_os("RUST_LOG").is_some() {
        let _ = env_logger::try_init();
    }
    let mut args = std::env::args_os().skip(1);
    let first = args.next();
    let (snapshot, folder) = if first.as_deref() == Some(std::ffi::OsStr::new("--snapshot")) {
        (
            args.next().map(PathBuf::from),
            args.next().map(PathBuf::from),
        )
    } else {
        (None, first.map(PathBuf::from))
    };
    let folder = folder
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("/"));
    let mut app = match FilesApp::new(folder) {
        Ok(app) => app,
        Err(error) => {
            eprintln!("slopos-files: cannot start directory worker: {error}");
            std::process::exit(1);
        }
    };
    if let Some(output) = snapshot {
        app.initial_snapshot();
        let mut surface =
            RasterSurface::new(WIDTH, HEIGHT, platform::scale_from_environment()).expect("surface");
        app.paint(&surface);
        surface.save_png(&output).expect("snapshot");
        return;
    }
    let wake = app.proxy_slot();
    let opts = WindowOptions::new("Files", "slopos-files", WIDTH, HEIGHT);
    if let Err(error) = platform::run(opts, app, move |proxy| {
        if let Ok(mut guard) = wake.lock() {
            *guard = Some(proxy.clone());
        }
        proxy.wake();
    }) {
        eprintln!("slopos-files: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slopos_ui::event::Modifiers;
    use slopos_ui::foundation::geometry::Scale;

    #[test]
    fn initial_snapshot_renders_and_publishes_accessibility() {
        let mut app = FilesApp::new(std::env::temp_dir()).unwrap();
        app.initial_snapshot();
        let surface = RasterSurface::new(WIDTH, HEIGHT, Scale::new(1).unwrap()).unwrap();
        app.paint(&surface);
        assert!(surface.pixel(0, 0).is_some());
        let tree = app.accessibility().unwrap();
        assert!(tree.nodes.len() >= 8);
    }

    #[test]
    fn keyboard_tab_focus_and_missing_directory_do_not_crash() {
        let mut app = FilesApp::new(PathBuf::from("/this/does/not/exist/slopos-files")).unwrap();
        app.initial_snapshot();
        assert!(app.status.contains("Cannot read folder"));
        let mut ctrl = Control::default();
        app.event(
            &Event::KeyPressed {
                key: Key::Tab,
                modifiers: Modifiers::default(),
            },
            &mut ctrl,
        );
        assert_eq!(app.focused, Some(0));
    }

    #[test]
    fn back_is_disabled_until_navigation_succeeds() {
        let mut app = FilesApp::new(std::env::temp_dir()).unwrap();
        app.initial_snapshot();
        app.refresh_actions();
        assert!(!app.toolbar[0].button.enabled);
        assert!(app.toolbar[4].button.enabled);
    }
}
