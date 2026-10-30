//! SLOPOS Files: a native file manager on the SLOPOS toolkit.
//!
//! Folder reads, file operations and folder watching run on worker threads
//! (`worker.rs`) and wake the event loop when they finish; the UI thread
//! never touches the disk for listing or copying. Operations are the
//! transactional ones in `ops.rs` and `trash.rs`.

mod model;
mod ops;
mod trash;
mod worker;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use model::{DirectoryState, Entry, Kind, Navigation};
use ops::{Conflict, Job, OpError};
use slopos_core::mime;
use slopos_ui::accessibility::window_tree;
use slopos_ui::controls::button::{Button, ButtonOutcome};
use slopos_ui::controls::dialog::{Dialog, DialogOutcome};
use slopos_ui::controls::focus::FocusChain;
use slopos_ui::controls::list::{ListOutcome, ListView};
use slopos_ui::controls::menu::{Menu, MenuBar, MenuBarOutcome, MenuItem, MenuOutcome};
use slopos_ui::controls::range::ProgressBar;
use slopos_ui::controls::text_field::{TextField, TextFieldOutcome};
use slopos_ui::event::{AccessibilityRequest, Event, Key, Modifiers};
use slopos_ui::foundation::geometry::Rect;
use slopos_ui::icons::IconLookup;
use slopos_ui::image::Image;
use slopos_ui::platform::{self, App, Control, WindowOptions};
use slopos_ui::primitives::button_plate::ButtonPlateVariant;
use slopos_ui::render::{RasterSurface, RenderError};
use slopos_ui::text::{measure, Font};
use slopos_ui::theme::palette;
use trash::TrashItem;
use worker::{Finished, Operation, Scanner, WakeSlot, Watcher};

const WIDTH: u32 = 880;
const HEIGHT: u32 = 560;
const SIDEBAR_WIDTH: u32 = 160;
const TOOLBAR_HEIGHT: i32 = 34;
const HEADER_HEIGHT: i32 = 18;
const STATUS_HEIGHT: u32 = 24;
const ROW_HEIGHT: u32 = 20;

const STATUS_ID: u64 = 2;
const BACK_ID: u64 = 10;
const FORWARD_ID: u64 = 11;
const UP_ID: u64 = 12;
const HOME_ID: u64 = 13;
const LOCATION_ID: u64 = 20;
const CANCEL_ID: u64 = 30;
const PROGRESS_ID: u64 = 31;
const LIST_ID: u64 = 50;
const PLACES_ID: u64 = 60;
const MENU_ID: u64 = 100;
const CONTEXT_ID: u64 = 300;
const DIALOG_ID: u64 = 400;

/// Everything a menu item, shortcut or button can ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    NewFolder,
    Open,
    Rename,
    Trash,
    DeletePermanently,
    Close,
    Cut,
    Copy,
    Paste,
    SelectAll,
    ToggleHidden,
    Refresh,
    Back,
    Forward,
    Up,
    Home,
    GoTrash,
    Restore,
    EmptyTrash,
}

/// (label, shortcut text, command); no command is a separator.
type MenuSpec = &'static [(&'static str, Option<&'static str>, Option<Command>)];

const FILE_MENU: MenuSpec = &[
    ("New Folder", Some("Ctrl+Shift+N"), Some(Command::NewFolder)),
    ("Open", Some("Ctrl+O"), Some(Command::Open)),
    ("Rename…", Some("F2"), Some(Command::Rename)),
    ("", None, None),
    ("Move to Trash", Some("Delete"), Some(Command::Trash)),
    (
        "Delete Permanently…",
        Some("Shift+Delete"),
        Some(Command::DeletePermanently),
    ),
    ("", None, None),
    ("Close Window", Some("Ctrl+W"), Some(Command::Close)),
];
const EDIT_MENU: MenuSpec = &[
    ("Cut", Some("Ctrl+X"), Some(Command::Cut)),
    ("Copy", Some("Ctrl+C"), Some(Command::Copy)),
    ("Paste", Some("Ctrl+V"), Some(Command::Paste)),
    ("", None, None),
    ("Select All", Some("Ctrl+A"), Some(Command::SelectAll)),
];
const VIEW_MENU: MenuSpec = &[
    (
        "Show Hidden Files",
        Some("Ctrl+H"),
        Some(Command::ToggleHidden),
    ),
    ("Refresh", Some("F5"), Some(Command::Refresh)),
];
const GO_MENU: MenuSpec = &[
    ("Back", Some("Alt+Left"), Some(Command::Back)),
    ("Forward", Some("Alt+Right"), Some(Command::Forward)),
    ("Enclosing Folder", Some("Alt+Up"), Some(Command::Up)),
    ("Home", Some("Alt+Home"), Some(Command::Home)),
    ("", None, None),
    ("Trash", None, Some(Command::GoTrash)),
    ("Empty Trash…", None, Some(Command::EmptyTrash)),
];
const MENUS: [MenuSpec; 4] = [FILE_MENU, EDIT_MENU, VIEW_MENU, GO_MENU];

fn menu_items(spec: MenuSpec) -> Vec<MenuItem> {
    spec.iter()
        .map(|(label, shortcut, command)| match command {
            None => MenuItem::separator(),
            Some(_) => {
                let item = MenuItem::new(label);
                match shortcut {
                    Some(text) => item.shortcut(text),
                    None => item,
                }
            }
        })
        .collect()
}

/// One displayed row (folder entry or Trash item).
#[derive(Debug, Clone)]
struct Row {
    name: String,
    path: PathBuf,
    kind: Kind,
    size: String,
    modified: String,
    kind_text: String,
    /// Theme icon names, most specific first.
    icons: Vec<String>,
    trash: Option<TrashItem>,
}

/// A sidebar destination.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Place {
    name: String,
    icon: &'static str,
    /// `None` is the Trash.
    path: Option<PathBuf>,
}

/// Home, the standard folders that exist, the whole computer and the Trash.
fn places(home: Option<&Path>) -> Vec<Place> {
    let mut places = Vec::new();
    if let Some(home) = home {
        places.push(Place {
            name: "Home".into(),
            icon: "user-home",
            path: Some(home.to_path_buf()),
        });
        for (folder, icon) in [
            ("Desktop", "user-desktop"),
            ("Documents", "folder-documents"),
            ("Downloads", "folder-download"),
            ("Music", "folder-music"),
            ("Pictures", "folder-pictures"),
            ("Videos", "folder-videos"),
        ] {
            let path = home.join(folder);
            if path.is_dir() {
                places.push(Place {
                    name: folder.into(),
                    icon,
                    path: Some(path),
                });
            }
        }
    }
    places.push(Place {
        name: "Computer".into(),
        icon: "computer",
        path: Some(PathBuf::from("/")),
    });
    places.push(Place {
        name: "Trash".into(),
        icon: "user-trash",
        path: None,
    });
    places
}

/// Icon names for a folder entry: the standard folders get their own.
fn folder_icons(path: &Path, home: Option<&Path>) -> Vec<String> {
    let special = places(home)
        .into_iter()
        .find(|place| place.path.as_deref() == Some(path) && place.icon != "computer")
        .map(|place| place.icon.to_owned());
    special.into_iter().chain(["folder".to_owned()]).collect()
}

/// Icon names for a file of MIME type `mime`: "image-png", then
/// "image-x-generic", then the generic document.
fn file_icons(mime: &str) -> Vec<String> {
    let mut icons = vec![mime.replace('/', "-")];
    if let Some((media, _)) = mime.split_once('/') {
        let generic = match media {
            "image" | "audio" | "video" | "text" => format!("{media}-x-generic"),
            _ => "application-x-generic".into(),
        };
        icons.push(generic);
    }
    icons.push("text-x-generic".into());
    icons
}

/// What an open dialog is asking about.
#[derive(Debug, Clone)]
enum Pending {
    Rename(PathBuf),
    Conflict(Operation),
    ConfirmDelete(Vec<PathBuf>),
    ConfirmErase(Vec<TrashItem>),
    ConfirmEmpty,
    Message,
}

struct Running {
    job: Arc<Job>,
    results: mpsc::Receiver<Finished>,
    label: String,
}

pub struct FilesApp {
    window: (u32, u32),
    state: DirectoryState,
    in_trash: bool,
    rows: Vec<Row>,
    show_hidden: bool,
    menu: MenuBar,
    context: Menu,
    context_commands: Vec<Option<Command>>,
    back: Button,
    forward: Button,
    up: Button,
    home: Button,
    cancel: Button,
    progress: ProgressBar,
    location: TextField,
    list: ListView,
    places: Vec<Place>,
    sidebar: ListView,
    icon_lookup: IconLookup,
    icon_cache: HashMap<(String, u32), Option<Image>>,
    focus: FocusChain,
    dialog: Option<(Dialog, Pending)>,
    slot: WakeSlot,
    scanner: Scanner,
    watcher: Option<Arc<Watcher>>,
    pending_scan: Option<(u64, PathBuf, Navigation)>,
    serial: u64,
    running: Option<Running>,
    clipboard: Option<(Vec<PathBuf>, bool)>,
    select_after: Vec<PathBuf>,
    status: String,
    font: Font,
}

fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["bytes", "KB", "MB", "GB", "TB"];
    if bytes < 1000 {
        return if bytes == 1 {
            "1 byte".into()
        } else {
            format!("{bytes} bytes")
        };
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

fn format_time(time: Option<std::time::SystemTime>) -> String {
    let Some(seconds) = time
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|elapsed| libc::time_t::try_from(elapsed.as_secs()).ok())
    else {
        return "—".into();
    };
    // SAFETY: localtime_r with valid pointers.
    let tm = unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&seconds, &mut tm);
        tm
    };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min
    )
}

fn row_for(entry: &Entry) -> Row {
    let name = entry.display_name();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let (size, kind_text, icons) = match entry.kind {
        Kind::Directory => (
            "—".into(),
            entry.kind_label().into(),
            folder_icons(&entry.path, home.as_deref()),
        ),
        Kind::Symlink | Kind::Other => (
            "—".into(),
            entry.kind_label().into(),
            vec!["emblem-symbolic-link".into(), "text-x-generic".into()],
        ),
        Kind::File => {
            let mime = mime::guess_for_name(&name);
            (
                format_size(entry.bytes),
                mime::describe(&mime),
                file_icons(&mime),
            )
        }
    };
    Row {
        name,
        path: entry.path.clone(),
        kind: entry.kind,
        size,
        modified: format_time(entry.modified),
        kind_text,
        icons,
        trash: None,
    }
}

impl FilesApp {
    fn new(path: PathBuf) -> std::io::Result<Self> {
        let slot: WakeSlot = Arc::new(Mutex::new(None));
        let scanner = Scanner::new(slot.clone())?;
        let watcher = Watcher::new(slot.clone()).ok();
        let menu = MenuBar::new(
            MENU_ID,
            vec![
                ("File", menu_items(FILE_MENU)),
                ("Edit", menu_items(EDIT_MENU)),
                ("View", menu_items(VIEW_MENU)),
                ("Go", menu_items(GO_MENU)),
            ],
        );
        let button = |id, label| Button::new(id, label, ButtonPlateVariant::Regular);
        let mut list = ListView::new(LIST_ID, "Folder contents", ROW_HEIGHT);
        list.multiple = true;
        let places = places(std::env::var_os("HOME").map(PathBuf::from).as_deref());
        let mut sidebar = ListView::new(PLACES_ID, "Places", 22);
        sidebar.set_len(places.len());
        let mut icon_lookup = IconLookup::from_environment();
        let scale = platform::scale_from_environment().factor();
        let mut toolbar_button = |id, label, icon: &str| {
            let mut button = button(id, label);
            button.icon = icon_lookup.load(icon, 16, scale);
            button
        };
        let back = toolbar_button(BACK_ID, "Back", "go-previous");
        let forward = toolbar_button(FORWARD_ID, "Forward", "go-next");
        let up = toolbar_button(UP_ID, "Enclosing Folder", "go-up");
        let home = toolbar_button(HOME_ID, "Home", "go-home");
        let mut app = Self {
            window: (WIDTH, HEIGHT),
            state: DirectoryState::new(path.clone()),
            in_trash: false,
            rows: Vec::new(),
            show_hidden: false,
            menu,
            context: Menu::new(CONTEXT_ID, Vec::new()),
            context_commands: Vec::new(),
            back,
            forward,
            up,
            home,
            cancel: button(CANCEL_ID, "Cancel"),
            progress: ProgressBar::new(PROGRESS_ID, "Operation progress"),
            location: TextField::new(LOCATION_ID, "Location"),
            list,
            places,
            sidebar,
            icon_lookup,
            icon_cache: HashMap::new(),
            focus: FocusChain::new(vec![
                LIST_ID,
                PLACES_ID,
                BACK_ID,
                FORWARD_ID,
                UP_ID,
                HOME_ID,
                LOCATION_ID,
            ]),
            dialog: None,
            slot,
            scanner,
            watcher,
            pending_scan: None,
            serial: 0,
            running: None,
            clipboard: None,
            select_after: Vec::new(),
            status: String::new(),
            font: Font::system(12.0),
        };
        app.focus.focus(LIST_ID);
        app.sync_focus();
        app.layout();
        app.location.set_text(&path.to_string_lossy());
        app.request(path, Navigation::Refresh);
        Ok(app)
    }

    // ----- layout and state -------------------------------------------------

    /// The toolbar sits under the in-window menu bar, or at the top when the
    /// menus are in the desktop's global bar.
    fn toolbar_y(&self) -> i32 {
        self.menu.height() as i32
    }

    fn layout(&mut self) {
        let (width, height) = self.window;
        self.menu.bounds = Rect {
            x: 0,
            y: 0,
            width,
            height: self.menu.height(),
        };
        let toolbar_y = self.toolbar_y();
        let mut x = 8;
        for button in [
            &mut self.back,
            &mut self.forward,
            &mut self.up,
            &mut self.home,
        ] {
            // Icon buttons are compact; without an icon theme the words show.
            let width = if button.icon.is_some() {
                30
            } else {
                measure(&button.label, &self.font).width as u32 + 20
            };
            button.bounds = Rect {
                x,
                y: toolbar_y + 6,
                width,
                height: 22,
            };
            x += width as i32 + if button.id == FORWARD_ID { 10 } else { 4 };
        }
        self.location.bounds = Rect {
            x: x + 4,
            y: toolbar_y + 6,
            width: width.saturating_sub(x as u32 + 14),
            height: 22,
        };
        let pane_top = toolbar_y + TOOLBAR_HEIGHT;
        self.sidebar.set_bounds(Rect {
            x: 0,
            y: pane_top,
            width: SIDEBAR_WIDTH,
            height: height.saturating_sub(pane_top as u32 + STATUS_HEIGHT),
        });
        let list_top = pane_top + HEADER_HEIGHT;
        self.list.set_bounds(Rect {
            x: SIDEBAR_WIDTH as i32 + 1,
            y: list_top,
            width: width.saturating_sub(SIDEBAR_WIDTH + 2),
            height: height.saturating_sub(list_top as u32 + STATUS_HEIGHT + 1),
        });
        let status_y = height as i32 - STATUS_HEIGHT as i32;
        self.cancel.bounds = Rect {
            x: width as i32 - 78,
            y: status_y + 2,
            width: 70,
            height: 20,
        };
        self.progress.bounds = Rect {
            x: width as i32 - 250,
            y: status_y + 6,
            width: 160,
            height: 12,
        };
        if let Some((dialog, _)) = &mut self.dialog {
            dialog.layout(self.window);
        }
    }

    fn sync_focus(&mut self) {
        let focused = self.focus.focused();
        self.list.focused = focused == Some(LIST_ID);
        self.sidebar.focused = focused == Some(PLACES_ID);
        self.location.focused = focused == Some(LOCATION_ID);
        for button in [
            &mut self.back,
            &mut self.forward,
            &mut self.up,
            &mut self.home,
        ] {
            button.focused = focused == Some(button.id);
        }
    }

    fn selected_rows(&self) -> Vec<&Row> {
        self.list
            .selection()
            .into_iter()
            .filter_map(|index| self.rows.get(index))
            .collect()
    }

    fn selected_paths(&self) -> Vec<PathBuf> {
        self.selected_rows()
            .iter()
            .map(|row| row.path.clone())
            .collect()
    }

    fn enabled(&self, command: Command) -> bool {
        let selected = !self.list.selection().is_empty();
        let busy = self.running.is_some();
        match command {
            Command::NewFolder | Command::Paste => {
                !self.in_trash
                    && !busy
                    && (command == Command::NewFolder || self.clipboard.is_some())
            }
            Command::Open => selected && !self.in_trash,
            Command::Rename => self.list.selection().len() == 1 && !self.in_trash && !busy,
            Command::Trash | Command::Cut => selected && !self.in_trash && !busy,
            Command::Copy => selected && !self.in_trash,
            Command::DeletePermanently => selected && !busy,
            Command::Restore => selected && self.in_trash && !busy,
            Command::Back => self.in_trash || self.state.back_target().is_some(),
            Command::Forward => !self.in_trash && self.state.forward_target().is_some(),
            Command::Up => !self.in_trash && self.state.parent().is_some(),
            Command::EmptyTrash => !busy,
            Command::SelectAll => !self.rows.is_empty(),
            _ => true,
        }
    }

    fn refresh_controls(&mut self) {
        let flags: Vec<Vec<(bool, bool)>> = MENUS
            .iter()
            .map(|spec| {
                spec.iter()
                    .map(|(_, _, command)| match command {
                        Some(command) => (
                            self.enabled(*command),
                            *command == Command::ToggleHidden && self.show_hidden,
                        ),
                        None => (false, false),
                    })
                    .collect()
            })
            .collect();
        for (menu, flags) in self.menu.menus.iter_mut().zip(flags) {
            for (item, (enabled, checked)) in menu.items.iter_mut().zip(flags) {
                if !item.separator {
                    item.enabled = enabled;
                    item.checked = checked;
                }
            }
        }
        for (menu, spec) in self.menu.menus.iter_mut().zip(MENUS) {
            for (item, (_, _, command)) in menu.items.iter_mut().zip(spec.iter()) {
                item.checkable = *command == Some(Command::ToggleHidden);
            }
        }
        self.back.enabled = self.enabled(Command::Back);
        self.forward.enabled = self.enabled(Command::Forward);
        self.up.enabled = self.enabled(Command::Up);
        self.home.enabled = true;
        let here = (!self.in_trash).then(|| self.state.current.clone());
        match self.places.iter().position(|place| place.path == here) {
            Some(index)
                if self.sidebar.cursor() != Some(index) || self.sidebar.selection().len() != 1 =>
            {
                self.sidebar.select_only(index)
            }
            Some(_) => {}
            None => self.sidebar.clear_selection(),
        }
    }

    /// Rebuild rows from the model, keeping the selection by path.
    fn rebuild_rows(&mut self) {
        let mut keep: Vec<PathBuf> = std::mem::take(&mut self.select_after);
        if keep.is_empty() {
            keep = self.selected_paths();
        }
        self.rows = if self.in_trash {
            trash::list()
                .into_iter()
                .map(|item| Row {
                    name: item.original.file_name().map_or_else(
                        || item.id.clone(),
                        |name| name.to_string_lossy().into_owned(),
                    ),
                    path: item.file(),
                    kind: if item.file().is_dir() {
                        Kind::Directory
                    } else {
                        Kind::File
                    },
                    size: String::new(),
                    modified: item.deleted.replace('T', " "),
                    kind_text: format!(
                        "from {}",
                        item.original
                            .parent()
                            .map(|p| p.display().to_string())
                            .unwrap_or_default()
                    ),
                    icons: if item.file().is_dir() {
                        vec!["folder".into()]
                    } else {
                        file_icons(&mime::guess(&item.original))
                    },
                    trash: Some(item),
                })
                .collect()
        } else {
            self.state
                .entries
                .iter()
                .filter(|entry| self.show_hidden || !entry.is_hidden())
                .map(row_for)
                .collect()
        };
        self.list.set_len(self.rows.len());
        let indices: Vec<usize> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| keep.contains(&row.path))
            .map(|(index, _)| index)
            .collect();
        self.list.set_selection(&indices);
        self.refresh_controls();
    }

    fn set_status(&mut self, text: impl Into<String>) {
        self.status = text.into();
    }

    fn item_count_status(&mut self) {
        let count = self.rows.len();
        let selected = self.list.selection().len();
        let place = if self.in_trash { "in the Trash" } else { "" };
        self.status = match (count, selected) {
            (0, _) if self.in_trash => "The Trash is empty".into(),
            (0, _) => "This folder is empty".into(),
            (_, 0) => format!("{count} item{} {place}", if count == 1 { "" } else { "s" }),
            (_, n) => format!("{n} of {count} selected"),
        };
    }

    // ----- navigation ----------------------------------------------------

    fn request(&mut self, path: PathBuf, mode: Navigation) {
        let seq = self.serial.wrapping_add(1);
        match self.scanner.request(seq, path.clone()) {
            Ok(()) => {
                self.serial = seq;
                self.pending_scan = Some((seq, path, mode));
                self.set_status("Reading folder…");
            }
            Err(message) => self.set_status(message),
        }
    }

    fn apply_scan(&mut self, (seq, path, listing): worker::ScanResult) {
        let Some((pending_seq, pending_path, mode)) = self.pending_scan.clone() else {
            return;
        };
        if seq != pending_seq || path != pending_path {
            return; // A stale scan must not rewrite navigation.
        }
        self.pending_scan = None;
        match listing {
            Ok(entries) => {
                let changed_folder = path != self.state.current || self.in_trash;
                if !self.state.apply(path.clone(), mode, entries) {
                    self.set_status("Navigation changed while reading; try again.");
                    return;
                }
                if changed_folder {
                    self.in_trash = false;
                    self.list.reset(0);
                    if let Some(watcher) = &self.watcher {
                        watcher.watch(&path);
                    }
                }
                self.location.set_text(&path.to_string_lossy());
                self.rebuild_rows();
                if changed_folder && self.list.selection().is_empty() && !self.rows.is_empty() {
                    self.list.select_only(0);
                }
                self.item_count_status();
            }
            Err(error) => {
                self.set_status(format!("Cannot read folder: {error}"));
                self.location
                    .set_text(&self.state.current.to_string_lossy());
            }
        }
        self.refresh_controls();
    }

    /// Open sidebar place `index`.
    fn go_place(&mut self, index: usize) {
        match self.places.get(index).map(|place| place.path.clone()) {
            Some(Some(path)) if self.in_trash || path != self.state.current => {
                self.request(path, Navigation::New)
            }
            Some(None) if !self.in_trash => self.go_trash(),
            _ => {}
        }
    }

    fn go_trash(&mut self) {
        self.in_trash = true;
        self.list.reset(0);
        self.location.set_text("Trash");
        self.rebuild_rows();
        if !self.rows.is_empty() {
            self.list.select_only(0);
        }
        self.item_count_status();
    }

    fn open_rows(&mut self) {
        let rows: Vec<Row> = self.selected_rows().into_iter().cloned().collect();
        if rows.is_empty() {
            self.set_status("Select something to open first.");
            return;
        }
        if self.in_trash {
            self.set_status("Restore items from the Trash to open them.");
            return;
        }
        // One folder navigates; files open in their applications.
        if let Some(folder) = rows.iter().find(|row| row.kind == Kind::Directory) {
            self.request(folder.path.clone(), Navigation::New);
            return;
        }
        for row in rows.iter().take(16) {
            self.open_path(&row.path, row.kind);
        }
    }

    fn open_path(&mut self, path: &Path, kind: Kind) {
        if kind == Kind::Symlink {
            // An explicit open follows the link once, to its real location.
            match std::fs::canonicalize(path) {
                Ok(real) if real.is_dir() => return self.request(real, Navigation::New),
                Ok(real) => return self.open_path(&real, Kind::File),
                Err(error) => return self.set_status(format!("The link is broken: {error}")),
            }
        }
        if kind == Kind::Other {
            return self.set_status("Special files can't be opened.");
        }
        let content_type = mime::guess(path);
        match mime::open(path) {
            Some(name) => self.set_status(format!(
                "Opening “{}” with {name}",
                path.file_name().unwrap_or_default().to_string_lossy()
            )),
            None => self.set_status(format!(
                "No application is set to open {} files.",
                mime::describe(&content_type)
            )),
        }
    }

    // ----- operations ----------------------------------------------------

    fn start(&mut self, operation: Operation) {
        if self.running.is_some() {
            self.set_status("Wait for the current operation to finish.");
            return;
        }
        let label = operation.verb().to_owned();
        match worker::start(operation, self.slot.clone()) {
            Ok((job, results)) => {
                self.set_status(format!("{label}…"));
                self.running = Some(Running {
                    job,
                    results,
                    label,
                });
            }
            Err(error) => self.message("Can't start the operation", &error.to_string()),
        }
        self.refresh_controls();
    }

    fn finish(&mut self, done: Finished) {
        self.running = None;
        match done.result {
            Ok(paths) => {
                if let Operation::Move { .. } = done.operation {
                    // A completed cut empties the file clipboard.
                    if self.clipboard.as_ref().is_some_and(|(_, cut)| *cut) {
                        self.clipboard = None;
                    }
                }
                self.select_after = paths;
                self.set_status(format!("{} finished", done.operation.verb()));
            }
            Err(OpError::Exists(path))
                if matches!(
                    done.operation,
                    Operation::Copy { .. } | Operation::Move { .. }
                ) =>
            {
                let name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                let mut dialog = Dialog::new(
                    DIALOG_ID,
                    "An item with this name already exists",
                    &format!("“{name}” already exists in “{}”. Replace it, keep both, or skip the items that conflict?", path.parent().map(|p| p.display().to_string()).unwrap_or_default()),
                    &["Cancel", "Skip", "Keep Both", "Replace"],
                );
                dialog.set_default(2);
                dialog.layout(self.window);
                self.dialog = Some((dialog, Pending::Conflict(done.operation)));
            }
            Err(OpError::Cancelled) => {
                self.set_status(format!("{} cancelled", done.operation.verb()))
            }
            Err(error) => self.message(
                &format!("{} failed", done.operation.verb()),
                &error.to_string(),
            ),
        }
        self.refresh_listing();
    }

    fn refresh_listing(&mut self) {
        if self.in_trash {
            self.rebuild_rows();
            self.item_count_status();
        } else if self.pending_scan.is_none() {
            self.request(self.state.current.clone(), Navigation::Refresh);
        }
    }

    fn message(&mut self, title: &str, text: &str) {
        let mut dialog = Dialog::new(DIALOG_ID, title, text, &["OK"]);
        dialog.layout(self.window);
        self.dialog = Some((dialog, Pending::Message));
    }

    fn run(&mut self, command: Command, control: &mut Control) {
        if !self.enabled(command) {
            return;
        }
        match command {
            Command::NewFolder => match ops::new_folder(&self.state.current) {
                Ok(path) => {
                    self.select_after = vec![path.clone()];
                    self.refresh_listing();
                    self.ask_rename(path);
                }
                Err(error) => self.message("Can't create a folder", &error.to_string()),
            },
            Command::Open => self.open_rows(),
            Command::Rename => {
                if let Some(path) = self.selected_paths().into_iter().next() {
                    self.ask_rename(path);
                }
            }
            Command::Trash => {
                let paths = self.selected_paths();
                self.start(Operation::Trash(paths));
            }
            Command::DeletePermanently => {
                let rows: Vec<Row> = self.selected_rows().into_iter().cloned().collect();
                let what = if rows.len() == 1 {
                    format!("“{}”", rows[0].name)
                } else {
                    format!("these {} items", rows.len())
                };
                let mut dialog = Dialog::new(
                    DIALOG_ID,
                    "Delete permanently?",
                    &format!(
                        "Are you sure you want to permanently delete {what}? This can't be undone."
                    ),
                    &["Cancel", "Delete"],
                );
                // Destructive: Enter must not delete by accident.
                dialog.set_default(0);
                dialog.layout(self.window);
                let pending = if self.in_trash {
                    Pending::ConfirmErase(rows.into_iter().filter_map(|row| row.trash).collect())
                } else {
                    Pending::ConfirmDelete(rows.into_iter().map(|row| row.path).collect())
                };
                self.dialog = Some((dialog, pending));
            }
            Command::Close => control.quit(),
            Command::Cut | Command::Copy => {
                let paths = self.selected_paths();
                let text = paths
                    .iter()
                    .map(|p| p.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("\n");
                control.set_clipboard(&text);
                let cut = command == Command::Cut;
                self.set_status(format!(
                    "{} {} item{} — paste in another folder",
                    if cut { "Cut" } else { "Copied" },
                    paths.len(),
                    if paths.len() == 1 { "" } else { "s" }
                ));
                self.clipboard = Some((paths, cut));
            }
            Command::Paste => {
                if let Some((sources, cut)) = self.clipboard.clone() {
                    let dest = self.state.current.clone();
                    self.start(if cut {
                        Operation::Move {
                            sources,
                            dest,
                            conflict: Conflict::Ask,
                        }
                    } else {
                        Operation::Copy {
                            sources,
                            dest,
                            conflict: Conflict::Ask,
                        }
                    });
                }
            }
            Command::SelectAll => {
                self.list.select_all();
                self.item_count_status();
            }
            Command::ToggleHidden => {
                self.show_hidden = !self.show_hidden;
                self.rebuild_rows();
                self.item_count_status();
            }
            Command::Refresh => self.refresh_listing(),
            Command::Back => {
                if self.in_trash {
                    self.request(self.state.current.clone(), Navigation::Refresh);
                } else if let Some(path) = self.state.back_target().map(Path::to_path_buf) {
                    self.request(path, Navigation::Back);
                }
            }
            Command::Forward => {
                if let Some(path) = self.state.forward_target().map(Path::to_path_buf) {
                    self.request(path, Navigation::Forward);
                }
            }
            Command::Up => {
                if let Some(path) = self.state.parent() {
                    self.select_after = vec![self.state.current.clone()];
                    self.request(path, Navigation::New);
                }
            }
            Command::Home => match std::env::var_os("HOME") {
                Some(home) => self.request(PathBuf::from(home), Navigation::New),
                None => self.set_status("HOME is not set."),
            },
            Command::GoTrash => self.go_trash(),
            Command::Restore => {
                let items = self
                    .selected_rows()
                    .into_iter()
                    .filter_map(|row| row.trash.clone())
                    .collect();
                self.start(Operation::Restore(items));
            }
            Command::EmptyTrash => {
                let mut dialog = Dialog::new(
                    DIALOG_ID,
                    "Empty the Trash?",
                    "All items in the Trash will be permanently deleted. This can't be undone.",
                    &["Cancel", "Empty Trash"],
                );
                dialog.set_default(0);
                dialog.layout(self.window);
                self.dialog = Some((dialog, Pending::ConfirmEmpty));
            }
        }
        self.refresh_controls();
    }

    fn ask_rename(&mut self, path: PathBuf) {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let mut dialog = Dialog::new(
            DIALOG_ID,
            "Rename",
            &format!("Enter a new name for “{name}”."),
            &["Cancel", "Rename"],
        )
        .with_field("Name", &name);
        // Select the name without its extension, as people usually keep it.
        if let Some(field) = &mut dialog.field {
            if let Some(dot) = name.rfind('.').filter(|&dot| dot > 0) {
                field.select(0, dot);
            }
        }
        dialog.layout(self.window);
        self.dialog = Some((dialog, Pending::Rename(path)));
    }

    fn dialog_choice(&mut self, pending: Pending, index: usize, text: Option<String>) {
        match pending {
            Pending::Rename(path) if index == 1 => {
                let name = text.unwrap_or_default();
                match ops::rename(&path, name.trim()) {
                    Ok(new_path) => {
                        self.select_after = vec![new_path];
                        self.refresh_listing();
                    }
                    Err(error) => self.message("Can't rename", &error.to_string()),
                }
            }
            Pending::Conflict(operation) => {
                let policy = match index {
                    1 => Some(Conflict::Skip),
                    2 => Some(Conflict::KeepBoth),
                    3 => Some(Conflict::Replace),
                    _ => None,
                };
                match policy {
                    Some(policy) => self.start(operation.with_conflict(policy)),
                    None => self.set_status(format!("{} cancelled", operation.verb())),
                }
            }
            Pending::ConfirmDelete(paths) if index == 1 => self.start(Operation::Delete(paths)),
            Pending::ConfirmErase(items) if index == 1 => self.start(Operation::Erase(items)),
            Pending::ConfirmEmpty if index == 1 => self.start(Operation::EmptyTrash),
            _ => {}
        }
    }

    fn open_context_menu(&mut self, x: i32, y: i32) {
        let spec: Vec<(&str, Option<Command>)> = if self.in_trash {
            vec![
                ("Restore", Some(Command::Restore)),
                ("Delete Permanently…", Some(Command::DeletePermanently)),
                ("", None),
                ("Empty Trash…", Some(Command::EmptyTrash)),
            ]
        } else if self.list.selection().is_empty() {
            vec![
                ("New Folder", Some(Command::NewFolder)),
                ("Paste", Some(Command::Paste)),
                ("", None),
                ("Show Hidden Files", Some(Command::ToggleHidden)),
                ("Refresh", Some(Command::Refresh)),
            ]
        } else {
            vec![
                ("Open", Some(Command::Open)),
                ("Rename…", Some(Command::Rename)),
                ("", None),
                ("Cut", Some(Command::Cut)),
                ("Copy", Some(Command::Copy)),
                ("Paste", Some(Command::Paste)),
                ("", None),
                ("Move to Trash", Some(Command::Trash)),
            ]
        };
        self.context.items = spec
            .iter()
            .map(|(label, command)| match command {
                None => MenuItem::separator(),
                Some(command) => MenuItem::new(label)
                    .enabled(self.enabled(*command))
                    .checked(*command == Command::ToggleHidden && self.show_hidden),
            })
            .collect();
        self.context_commands = spec.iter().map(|(_, command)| *command).collect();
        self.context.open_at(x, y, self.window);
    }

    fn shortcut(&self, key: &Key, modifiers: Modifiers) -> Option<Command> {
        let (control, shift, alt) = (modifiers.control, modifiers.shift, modifiers.alt);
        Some(match key {
            Key::Character(c) if control => match c.to_ascii_lowercase() {
                'n' if shift => Command::NewFolder,
                'o' => Command::Open,
                'w' | 'q' => Command::Close,
                'x' => Command::Cut,
                'c' => Command::Copy,
                'v' => Command::Paste,
                'a' => Command::SelectAll,
                'h' => Command::ToggleHidden,
                'l' => return None,
                _ => return None,
            },
            Key::Function(2) => Command::Rename,
            Key::Function(5) => Command::Refresh,
            Key::Delete if shift => Command::DeletePermanently,
            Key::Delete => {
                if self.in_trash {
                    Command::DeletePermanently
                } else {
                    Command::Trash
                }
            }
            Key::Left if alt => Command::Back,
            Key::Right if alt => Command::Forward,
            Key::Up if alt => Command::Up,
            Key::Home if alt => Command::Home,
            Key::Backspace if !control && !alt => Command::Up,
            _ => return None,
        })
    }

    fn poll_workers(&mut self) -> bool {
        let mut changed = false;
        while let Ok(result) = self.scanner.results.try_recv() {
            self.apply_scan(result);
            changed = true;
        }
        if let Some(running) = &self.running {
            match running.results.try_recv() {
                Ok(done) => {
                    self.finish(done);
                    changed = true;
                }
                Err(_) => {
                    let fraction = running.job.fraction();
                    if self.progress.fraction != fraction {
                        self.progress.fraction = fraction;
                        changed = true;
                    }
                }
            }
        }
        if let Some(watcher) = &self.watcher {
            if watcher.changed.swap(false, Ordering::Relaxed)
                && !self.in_trash
                && self.running.is_none()
            {
                self.refresh_listing();
                changed = true;
            }
        }
        changed
    }

    /// Wait for the first folder read (snapshot mode and tests).
    fn initial_snapshot(&mut self) {
        if let Ok(result) = self.scanner.results.recv_timeout(Duration::from_secs(3)) {
            self.apply_scan(result);
        }
    }

    fn handle(&mut self, event: &Event, control: &mut Control) {
        if let Some((dialog, _)) = &mut self.dialog {
            match dialog.handle(event) {
                DialogOutcome::Chosen { index, text } => {
                    let (_, pending) = self.dialog.take().expect("dialog is open");
                    self.dialog_choice(pending, index, text);
                }
                DialogOutcome::Copy(text) => control.set_clipboard(&text),
                DialogOutcome::PasteRequested => control.request_paste(),
                _ => {}
            }
            return;
        }
        if self.context.is_open() {
            if let MenuOutcome::Chosen(index) = self.context.handle(event) {
                if let Some(Some(command)) = self.context_commands.get(index).copied() {
                    self.run(command, control);
                }
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
        if let Some(running) = &self.running {
            let cancel = match event {
                Event::Accessibility(AccessibilityRequest::Click(CANCEL_ID)) => true,
                Event::KeyPressed {
                    key: Key::Escape, ..
                } => true,
                _ => self.cancel.handle(event) == ButtonOutcome::Activated,
            };
            if cancel {
                running.job.cancel();
                self.set_status(format!("Cancelling {}…", running.label.to_lowercase()));
                return;
            }
        }
        if let Event::Accessibility(request) = event {
            return self.accessibility_request(request, control);
        }
        if self.focus.handle(event).is_some() {
            self.sync_focus();
            return;
        }
        // The location field gets keys first while it has focus.
        if self.location.focused {
            match self.location.handle(event) {
                TextFieldOutcome::Ignored => {}
                TextFieldOutcome::Submitted => {
                    let typed = PathBuf::from(self.location.text().trim());
                    self.focus.focus(LIST_ID);
                    self.sync_focus();
                    self.request(typed, Navigation::New);
                    return;
                }
                TextFieldOutcome::Cancelled => {
                    self.location
                        .set_text(&self.state.current.to_string_lossy());
                    self.focus.focus(LIST_ID);
                    self.sync_focus();
                    return;
                }
                TextFieldOutcome::Copy(text) => return control.set_clipboard(&text),
                TextFieldOutcome::PasteRequested => return control.request_paste(),
                _ => return,
            }
        } else if self.location.handle(event) == TextFieldOutcome::FocusRequested {
            self.focus.focus(LOCATION_ID);
            self.sync_focus();
            return;
        }
        if let Event::KeyPressed { key, modifiers } = event {
            if let Key::Character('l' | 'L') = key {
                if modifiers.control {
                    self.focus.focus(LOCATION_ID);
                    self.sync_focus();
                    self.location.select_all();
                    return;
                }
            }
            if let Some(command) = self.shortcut(key, *modifiers) {
                return self.run(command, control);
            }
        }
        for (id, command) in [
            (BACK_ID, Command::Back),
            (FORWARD_ID, Command::Forward),
            (UP_ID, Command::Up),
            (HOME_ID, Command::Home),
        ] {
            let button = match id {
                BACK_ID => &mut self.back,
                FORWARD_ID => &mut self.forward,
                UP_ID => &mut self.up,
                _ => &mut self.home,
            };
            if button.handle(event) == ButtonOutcome::Activated {
                return self.run(command, control);
            }
        }
        let names: Vec<String> = self.places.iter().map(|place| place.name.clone()).collect();
        let place_name = |index: usize| names.get(index).cloned().unwrap_or_default();
        match self.sidebar.handle(event, control.modifiers(), &place_name) {
            ListOutcome::FocusRequested => {
                self.focus.focus(PLACES_ID);
                self.sync_focus();
            }
            // One click (or Up/Down) on a place goes there.
            ListOutcome::SelectionChanged | ListOutcome::Activated(_) => {
                if let Some(index) = self.sidebar.cursor() {
                    self.go_place(index);
                }
                return;
            }
            _ => {}
        }
        let rows = &self.rows;
        let label = |index: usize| {
            rows.get(index)
                .map(|row| row.name.clone())
                .unwrap_or_default()
        };
        match self.list.handle(event, control.modifiers(), &label) {
            ListOutcome::Activated(_) => self.run(Command::Open, control),
            ListOutcome::ContextMenu { x, y, .. } => {
                self.focus.focus(LIST_ID);
                self.sync_focus();
                self.open_context_menu(x, y);
            }
            ListOutcome::FocusRequested => {
                self.focus.focus(LIST_ID);
                self.sync_focus();
                self.item_count_status();
            }
            ListOutcome::SelectionChanged => self.item_count_status(),
            _ => {}
        }
        self.refresh_controls();
    }

    fn accessibility_request(&mut self, request: &AccessibilityRequest, control: &mut Control) {
        match request {
            AccessibilityRequest::Click(id) => {
                let command = match *id {
                    BACK_ID => Some(Command::Back),
                    FORWARD_ID => Some(Command::Forward),
                    UP_ID => Some(Command::Up),
                    HOME_ID => Some(Command::Home),
                    _ => None,
                };
                if let Some(command) = command {
                    return self.run(command, control);
                }
                if let Some(index) = self.list.row_for_node(*id) {
                    self.list.select_only(index);
                    return self.run(Command::Open, control);
                }
                if let Some(menu) = self.menu.menu_for_title_node(*id) {
                    self.menu.open(menu, self.window, true);
                }
            }
            AccessibilityRequest::Focus(id) => {
                if let Some(index) = self.list.row_for_node(*id) {
                    self.list.select_only(index);
                    self.focus.focus(LIST_ID);
                } else {
                    self.focus.focus(*id);
                }
                self.sync_focus();
            }
            AccessibilityRequest::SetValue(LOCATION_ID, value) => {
                self.location.set_text(value);
                self.request(PathBuf::from(value), Navigation::New);
            }
            _ => {}
        }
    }

    // ----- painting ------------------------------------------------------

    fn paint_all(&mut self, surface: &RasterSurface) -> Result<(), RenderError> {
        let colors = palette();
        let (width, height) = self.window;
        surface.clear(colors.surface)?;
        self.menu.paint(surface)?;
        for button in [&self.back, &self.forward, &self.up, &self.home] {
            button.paint(surface, &self.font)?;
        }
        self.location.paint(surface)?;
        surface.fill_rect(
            Rect {
                x: 0,
                y: self.toolbar_y() + TOOLBAR_HEIGHT - 1,
                width,
                height: 1,
            },
            colors.bevel_shadow,
        )?;
        // Places.
        let scale = surface.scale().factor();
        let place_icons: Vec<Option<Image>> = self
            .places
            .clone()
            .iter()
            .map(|place| self.icon(&[place.icon.to_owned(), "folder".into()], scale))
            .collect();
        let places = &self.places;
        let font = &self.font;
        let line = measure("Ag", font).height as i32;
        self.sidebar
            .paint(surface, |surface, index, rect, selected| {
                if let Some(Some(icon)) = place_icons.get(index) {
                    surface.draw_image_scaled(icon, rect.x + 8, rect.y + 3, 16, 16)?;
                }
                let ink = if selected {
                    colors.selection_ink
                } else {
                    colors.ink
                };
                surface.draw_text(
                    &places[index].name,
                    rect.x + 30,
                    rect.y + (rect.height as i32 - line) / 2,
                    font,
                    ink,
                )
            })?;
        surface.fill_rect(
            Rect {
                x: SIDEBAR_WIDTH as i32,
                y: self.sidebar.bounds().y,
                width: 1,
                height: self.sidebar.bounds().height,
            },
            colors.border,
        )?;
        // Column header.
        let list_x = self.list.bounds().x;
        let header = Rect {
            x: list_x,
            y: self.toolbar_y() + TOOLBAR_HEIGHT,
            width: width.saturating_sub(list_x as u32),
            height: HEADER_HEIGHT as u32,
        };
        surface.fill_rect(header, colors.surface)?;
        surface.fill_rect(
            Rect {
                y: header.y + HEADER_HEIGHT - 1,
                height: 1,
                ..header
            },
            colors.bevel_shadow,
        )?;
        let columns = self.columns();
        let titles = if self.in_trash {
            ["Name", "", "Deleted", "Original location"]
        } else {
            ["Name", "Size", "Modified", "Kind"]
        };
        for (index, ((x, _), title)) in columns.iter().zip(titles).enumerate() {
            let left = list_x + x + if index == 0 { 26 } else { 4 };
            surface.draw_text(title, left, header.y + 2, &self.font, colors.ink_secondary)?;
            if index > 0 {
                surface.fill_rect(
                    Rect {
                        x: list_x + x,
                        y: header.y + 3,
                        width: 1,
                        height: HEADER_HEIGHT as u32 - 6,
                    },
                    colors.bevel_shadow,
                )?;
            }
        }
        let visible: Vec<usize> = self.list.visible_rows().collect();
        let row_icons: HashMap<usize, Option<Image>> = visible
            .into_iter()
            .map(|index| {
                let names = self.rows[index].icons.clone();
                (index, self.icon(&names, scale))
            })
            .collect();
        let rows = &self.rows;
        let font = &self.font;
        self.list.paint(surface, |surface, index, rect, selected| {
            let row = &rows[index];
            let y = rect.y + (rect.height as i32 - line) / 2;
            let ink = if selected {
                colors.selection_ink
            } else {
                colors.ink
            };
            let glyph = Rect {
                x: rect.x + 6,
                y: rect.y + 5,
                width: 12,
                height: 10,
            };
            match (row_icons.get(&index), row.kind) {
                (Some(Some(icon)), _) => {
                    surface.draw_image_scaled(icon, rect.x + 4, rect.y + 2, 16, 16)?;
                }
                (_, Kind::Directory) => {
                    surface.fill_rect(
                        Rect {
                            width: 5,
                            height: 2,
                            y: glyph.y - 2,
                            ..glyph
                        },
                        colors.thumb,
                    )?;
                    surface.fill_rect(glyph, colors.thumb)?;
                    surface.stroke_rect(glyph, colors.ink)?;
                }
                (_, _) => {
                    let page = Rect {
                        x: glyph.x + 1,
                        y: glyph.y - 2,
                        width: 9,
                        height: 13,
                    };
                    surface.fill_rect(page, colors.content)?;
                    surface.stroke_rect(page, colors.ink)?;
                    if row.kind == Kind::Symlink {
                        surface.draw_text("↗", page.x + 1, page.y, font, colors.ink)?;
                    }
                }
            }
            let cells = [&row.name, &row.size, &row.modified, &row.kind_text];
            for (column, ((x, w), text)) in columns.iter().zip(cells).enumerate() {
                let left = rect.x + x + if column == 0 { 26 } else { 4 };
                let cell = Rect {
                    x: left,
                    y: rect.y,
                    width: w.saturating_sub(if column == 0 { 30 } else { 8 }),
                    height: rect.height,
                };
                let ink = if column == 0 || selected {
                    ink
                } else {
                    colors.ink_secondary
                };
                surface.with_clip(cell, || surface.draw_text(text, left, y, font, ink))?;
            }
            Ok(())
        })?;
        if self.rows.is_empty() && self.pending_scan.is_none() {
            let bounds = self.list.bounds();
            let text = if self.in_trash {
                "The Trash is empty"
            } else {
                "This folder is empty"
            };
            let w = measure(text, &self.font).width as i32;
            surface.draw_text(
                text,
                bounds.x + (bounds.width as i32 - w) / 2,
                bounds.y + 40,
                &self.font,
                colors.ink_secondary,
            )?;
        }
        // Status bar.
        let status_y = height as i32 - STATUS_HEIGHT as i32;
        let bar = Rect {
            x: 0,
            y: status_y,
            width,
            height: STATUS_HEIGHT,
        };
        surface.fill_rect(bar, colors.surface)?;
        surface.fill_rect(Rect { height: 1, ..bar }, colors.bevel_shadow)?;
        let text_width = if self.running.is_some() {
            width.saturating_sub(270)
        } else {
            width.saturating_sub(16)
        };
        surface.with_clip(
            Rect {
                x: 8,
                y: status_y,
                width: text_width,
                height: STATUS_HEIGHT,
            },
            || surface.draw_text(&self.status, 8, status_y + 5, &self.font, colors.ink),
        )?;
        if self.running.is_some() {
            self.progress.paint(surface)?;
            self.cancel.paint(surface, &self.font)?;
        }
        if let Some((dialog, _)) = &mut self.dialog {
            dialog.paint(surface)?;
        }
        self.context.paint(surface)?;
        self.menu.paint_menus(surface)
    }

    /// The first icon in `names` the theme has, cached per size.
    fn icon(&mut self, names: &[String], scale: u32) -> Option<Image> {
        let key = (names.join("|"), scale);
        if let Some(image) = self.icon_cache.get(&key) {
            return image.clone();
        }
        let image = names
            .iter()
            .find_map(|name| self.icon_lookup.load(name, 16, scale));
        // Bounded: one entry per distinct icon list and scale.
        if self.icon_cache.len() > 256 {
            self.icon_cache.clear();
        }
        self.icon_cache.insert(key, image.clone());
        image
    }

    /// Column (x, width) for Name, Size, Modified, Kind.
    fn columns(&self) -> [(i32, u32); 4] {
        let width = self.list.scroll().content_area().width;
        let fixed = 80 + 130 + 170;
        let name = width.saturating_sub(fixed).max(120);
        let size_x = name as i32;
        [
            (0, name),
            (size_x, 80),
            (size_x + 80, 130),
            (size_x + 210, 170),
        ]
    }
}

impl App for FilesApp {
    fn event(&mut self, event: &Event, control: &mut Control) {
        match event {
            Event::CloseRequested => return control.quit(),
            Event::Resized { width, height } => {
                self.window = (*width, *height);
                self.layout();
            }
            // Applied even under a dialog: the bar's height changes the layout.
            Event::GlobalMenu(_) => {
                self.menu.handle(event, self.window);
                self.layout();
            }
            Event::Wake => {
                if !self.poll_workers() {
                    return;
                }
            }
            _ => self.handle(event, control),
        }
        let title = if self.in_trash {
            "Trash".to_owned()
        } else {
            self.state.current.file_name().map_or_else(
                || self.state.current.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            )
        };
        control.set_title(&title);
        control.request_redraw();
    }

    fn paint(&mut self, surface: &RasterSurface) {
        if let Err(error) = self.paint_all(surface) {
            log::warn!("paint failed: {error}");
        }
    }

    fn menu_bar(&self) -> Option<&MenuBar> {
        Some(&self.menu)
    }

    fn accessibility(&self) -> Option<accesskit::TreeUpdate> {
        let mut nodes = self.menu.accessibility_nodes();
        for button in [&self.back, &self.forward, &self.up, &self.home] {
            nodes.push((button.id, button.accessibility_node()));
        }
        nodes.push((LOCATION_ID, self.location.accessibility_node()));
        let places = &self.places;
        let place_label = |index: usize| {
            places
                .get(index)
                .map(|place| place.name.clone())
                .unwrap_or_default()
        };
        nodes.extend(self.sidebar.accessibility_nodes(&place_label));
        let rows = &self.rows;
        let label = |index: usize| {
            rows.get(index)
                .map(|row| format!("{}, {}, {}", row.name, row.kind_text, row.size))
                .unwrap_or_default()
        };
        nodes.extend(self.list.accessibility_nodes(&label));
        let mut status = accesskit::Node::new(accesskit::Role::Label);
        status.set_label(self.status.as_str());
        nodes.push((STATUS_ID, status));
        if self.running.is_some() {
            nodes.push((PROGRESS_ID, self.progress.accessibility_node()));
            nodes.push((CANCEL_ID, self.cancel.accessibility_node()));
        }
        nodes.extend(self.context.accessibility_nodes());
        let focus = if let Some((dialog, _)) = &self.dialog {
            nodes.extend(dialog.accessibility_nodes());
            dialog.focus_node()
        } else if self.context.is_open() {
            self.context.focus_node()
        } else if let Some(node) = self.menu.focus_node() {
            node
        } else if self.list.focused {
            self.list.focus_node()
        } else if self.sidebar.focused {
            self.sidebar.focus_node()
        } else {
            self.focus.focused().unwrap_or(LIST_ID)
        };
        let title = if self.in_trash {
            "Trash".to_owned()
        } else {
            format!("Files — {}", self.state.current.display())
        };
        Some(window_tree(&title, nodes, Some(focus)))
    }
}

fn main() {
    if std::env::var_os("RUST_LOG").is_some() {
        let _ = env_logger::try_init();
    }
    let mut args = std::env::args_os().skip(1);
    let first = args.next();
    // `--trash` opens the Trash (the desktop's Trash icon uses it).
    let trash = first.as_deref() == Some(std::ffi::OsStr::new("--trash"));
    let (snapshot, folder) = if first.as_deref() == Some(std::ffi::OsStr::new("--snapshot")) {
        (
            args.next().map(PathBuf::from),
            args.next().map(PathBuf::from),
        )
    } else if trash {
        (None, None)
    } else {
        (None, first.map(PathBuf::from))
    };
    let folder = folder
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("/"));
    let mut app = match FilesApp::new(folder) {
        Ok(app) => app,
        Err(error) => {
            eprintln!("slopos-files: cannot start the folder worker: {error}");
            std::process::exit(1);
        }
    };
    if trash {
        app.go_trash();
    }
    if let Some(output) = snapshot {
        app.initial_snapshot();
        let scale = platform::scale_from_environment();
        let surface = RasterSurface::new(WIDTH, HEIGHT, scale).expect("surface");
        app.paint(&surface);
        if let Err(error) = surface.save_png(&output) {
            eprintln!("slopos-files: {error}");
            std::process::exit(1);
        }
        return;
    }
    let slot = app.slot.clone();
    let options = WindowOptions::new("Files", "slopos-files", WIDTH, HEIGHT);
    if let Err(error) = platform::run(options, app, move |proxy| {
        if let Ok(mut guard) = slot.lock() {
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
    use slopos_ui::foundation::geometry::Scale;

    fn key(key: Key) -> Event {
        Event::KeyPressed {
            key,
            modifiers: Modifiers::default(),
        }
    }

    fn ctrl(c: char) -> Event {
        Event::KeyPressed {
            key: Key::Character(c),
            modifiers: Modifiers {
                control: true,
                ..Modifiers::default()
            },
        }
    }

    /// Feed events until background work settles.
    fn settle(app: &mut FilesApp) {
        for _ in 0..200 {
            let mut control = Control::default();
            app.event(&Event::Wake, &mut control);
            if app.pending_scan.is_none() && app.running.is_none() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("background work did not finish");
    }

    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("Folder")).unwrap();
        std::fs::write(dir.path().join("a.txt"), b"a").unwrap();
        std::fs::write(dir.path().join("b.png"), b"b").unwrap();
        std::fs::write(dir.path().join(".hidden"), b"h").unwrap();
        dir
    }

    fn select(app: &mut FilesApp, name: &str) {
        let index = app.rows.iter().position(|row| row.name == name).unwrap();
        app.list.select_only(index);
    }

    #[test]
    fn listing_hides_dotfiles_until_toggled_and_snapshot_paints() {
        let dir = fixture();
        let mut app = FilesApp::new(dir.path().to_path_buf()).unwrap();
        app.initial_snapshot();
        let names: Vec<&str> = app.rows.iter().map(|row| row.name.as_str()).collect();
        assert_eq!(names, ["Folder", "a.txt", "b.png"]);
        assert_eq!(app.rows[2].kind_text, "PNG image");
        let mut control = Control::default();
        app.event(&ctrl('h'), &mut control);
        assert_eq!(app.rows.len(), 4);
        for factor in [1, 2] {
            let surface = RasterSurface::new(WIDTH, HEIGHT, Scale::new(factor).unwrap()).unwrap();
            app.paint(&surface);
        }
        assert!(app.accessibility().unwrap().nodes.len() > 10);
    }

    #[test]
    fn missing_folder_reports_an_error_and_tab_moves_focus() {
        let mut app = FilesApp::new(PathBuf::from("/this/does/not/exist/slopos-files")).unwrap();
        app.initial_snapshot();
        assert!(app.status.contains("Cannot read folder"));
        let mut control = Control::default();
        // From the file list, Tab reaches the Places sidebar, then the toolbar.
        app.event(&key(Key::Tab), &mut control);
        assert_eq!(app.focus.focused(), Some(PLACES_ID));
        assert!(app.sidebar.focused && !app.list.focused);
        app.event(&key(Key::Tab), &mut control);
        assert_eq!(app.focus.focused(), Some(BACK_ID));
    }

    #[test]
    fn places_list_standard_folders_and_navigate() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir(home.path().join("Documents")).unwrap();
        std::fs::create_dir(home.path().join("Pictures")).unwrap();
        let names: Vec<String> = places(Some(home.path()))
            .into_iter()
            .map(|place| place.name)
            .collect();
        assert_eq!(
            names,
            ["Home", "Documents", "Pictures", "Computer", "Trash"]
        );
        assert_eq!(places(None).len(), 2);
        assert_eq!(
            folder_icons(&home.path().join("Pictures"), Some(home.path())),
            ["folder-pictures", "folder"]
        );
        assert_eq!(
            folder_icons(Path::new("/tmp"), Some(home.path())),
            ["folder"]
        );
        assert_eq!(
            file_icons("image/png"),
            ["image-png", "image-x-generic", "text-x-generic"]
        );
        assert_eq!(
            file_icons("application/pdf"),
            ["application-pdf", "application-x-generic", "text-x-generic"]
        );

        // Choosing a place goes there, and the sidebar follows navigation.
        let dir = fixture();
        let mut app = FilesApp::new(dir.path().to_path_buf()).unwrap();
        app.initial_snapshot();
        assert_eq!(app.sidebar.selection(), Vec::<usize>::new());
        let computer = app
            .places
            .iter()
            .position(|p| p.name == "Computer")
            .unwrap();
        let trash = app.places.iter().position(|p| p.name == "Trash").unwrap();
        app.sidebar.select_only(trash);
        app.go_place(trash);
        assert!(app.in_trash);
        app.go_place(computer);
        settle(&mut app);
        assert!(!app.in_trash);
        assert_eq!(app.state.current, PathBuf::from("/"));
        app.refresh_controls();
        assert_eq!(app.sidebar.selection(), vec![computer]);
    }

    #[test]
    fn navigation_open_up_back_forward() {
        let dir = fixture();
        let mut app = FilesApp::new(dir.path().to_path_buf()).unwrap();
        app.initial_snapshot();
        assert!(!app.back.enabled);
        select(&mut app, "Folder");
        let mut control = Control::default();
        app.event(&key(Key::Enter), &mut control);
        settle(&mut app);
        assert_eq!(app.state.current, dir.path().join("Folder"));
        assert!(app.back.enabled);
        app.event(&key(Key::Backspace), &mut control);
        settle(&mut app);
        assert_eq!(app.state.current, dir.path());
        // Up selects the folder we came from.
        assert_eq!(app.selected_rows()[0].name, "Folder");
    }

    #[test]
    fn copy_paste_conflict_keep_both_rename_and_trash_journey() {
        let dir = fixture();
        let _guard = trash::TEST_ENV.lock().unwrap_or_else(|p| p.into_inner());
        let data = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_DATA_HOME", data.path());
        let mut app = FilesApp::new(dir.path().to_path_buf()).unwrap();
        app.initial_snapshot();
        let mut control = Control::default();
        select(&mut app, "a.txt");
        app.event(&ctrl('c'), &mut control);
        assert!(control.pending_clipboard().unwrap().ends_with("a.txt"));
        app.event(&ctrl('v'), &mut control);
        settle(&mut app);
        // Pasting into the same folder duplicates.
        assert!(dir.path().join("a 2.txt").exists());
        // Rename via F2.
        select(&mut app, "a 2.txt");
        app.event(&key(Key::Function(2)), &mut control);
        assert!(app.dialog.is_some());
        app.event(&ctrl('a'), &mut control);
        for c in "renamed.txt".chars() {
            app.event(&key(Key::Character(c)), &mut control);
        }
        app.event(&key(Key::Enter), &mut control);
        settle(&mut app);
        assert!(dir.path().join("renamed.txt").exists());
        assert_eq!(app.selected_rows()[0].name, "renamed.txt");
        // Copy into Folder, then again: the conflict dialog offers Keep Both.
        app.clipboard = Some((vec![dir.path().join("a.txt")], false));
        app.state.current = dir.path().join("Folder");
        app.event(&ctrl('v'), &mut control);
        settle(&mut app);
        app.event(&ctrl('v'), &mut control);
        settle(&mut app);
        assert!(matches!(app.dialog, Some((_, Pending::Conflict(_)))));
        app.event(&key(Key::Enter), &mut control); // default: Keep Both
        settle(&mut app);
        assert!(dir.path().join("Folder/a 2.txt").exists());
        // Delete moves to the Trash; Trash view lists it; restore brings it back.
        app.state.current = dir.path().to_path_buf();
        app.request(dir.path().to_path_buf(), Navigation::Refresh);
        settle(&mut app);
        select(&mut app, "renamed.txt");
        app.event(&key(Key::Delete), &mut control);
        settle(&mut app);
        assert!(!dir.path().join("renamed.txt").exists());
        app.run(Command::GoTrash, &mut control);
        assert_eq!(app.rows.len(), 1);
        app.list.select_only(0);
        app.run(Command::Restore, &mut control);
        settle(&mut app);
        assert!(dir.path().join("renamed.txt").exists());
        std::env::remove_var("XDG_DATA_HOME");
    }

    #[test]
    fn permanent_delete_needs_confirmation_and_escape_cancels() {
        let dir = fixture();
        let mut app = FilesApp::new(dir.path().to_path_buf()).unwrap();
        app.initial_snapshot();
        let mut control = Control::default();
        select(&mut app, "a.txt");
        app.event(
            &Event::KeyPressed {
                key: Key::Delete,
                modifiers: Modifiers {
                    shift: true,
                    ..Modifiers::default()
                },
            },
            &mut control,
        );
        assert!(matches!(app.dialog, Some((_, Pending::ConfirmDelete(_)))));
        // Enter is the safe default (Cancel) for destructive dialogs.
        app.event(&key(Key::Enter), &mut control);
        settle(&mut app);
        assert!(dir.path().join("a.txt").exists());
        app.event(
            &Event::KeyPressed {
                key: Key::Delete,
                modifiers: Modifiers {
                    shift: true,
                    ..Modifiers::default()
                },
            },
            &mut control,
        );
        app.event(&key(Key::Tab), &mut control);
        app.event(&key(Key::Space), &mut control);
        settle(&mut app);
        assert!(!dir.path().join("a.txt").exists());
    }

    #[test]
    fn formats_are_readable() {
        assert_eq!(format_size(1), "1 byte");
        assert_eq!(format_size(999), "999 bytes");
        assert_eq!(format_size(1_500_000), "1.5 MB");
        assert_eq!(format_time(None), "—");
        assert_eq!(format_time(Some(std::time::UNIX_EPOCH)).len(), 16);
    }
}
