//! The Software window (STORE-02): one tab per source available on this
//! machine. The SLOPOS tab lists the verified catalogue and installed
//! AppImages on the left, the chosen app's details and actions on the right.
//! The native, Flatpak, Snap and AUR tabs list search results only;
//! installing from them is not built yet.
//!
//! Installs, updates and removals run one at a time on a worker thread
//! through the STORE-01 transaction code; the window shows real byte
//! progress, can cancel a download, and reports the transaction's own
//! result. Removal asks first. Nothing is offered from a catalogue that did
//! not verify. Searches also run on a worker thread and wake the window
//! when they finish.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;

use slopos_ui::accessibility::window_tree;
use slopos_ui::controls::button::{Button, ButtonOutcome};
use slopos_ui::controls::dialog::{wrap, Dialog, DialogOutcome};
use slopos_ui::controls::focus::FocusChain;
use slopos_ui::controls::list::{ListOutcome, ListView};
use slopos_ui::controls::menu::{MenuBar, MenuBarOutcome, MenuItem};
use slopos_ui::controls::range::{Checkbox, ProgressBar, ToggleOutcome};
use slopos_ui::controls::text_field::{TextField, TextFieldOutcome};
use slopos_ui::event::{AccessibilityRequest, Event, Key};
use slopos_ui::foundation::geometry::Rect;
use slopos_ui::icons::IconLookup;
use slopos_ui::image::Image;
use slopos_ui::platform::{App, Control, Proxy};
use slopos_ui::primitives::button_plate::ButtonPlateVariant;
use slopos_ui::render::{RasterSurface, RenderError};
use slopos_ui::text::{measure, Font};
use slopos_ui::theme::palette;

use crate::backends::{self, Format, Package, Runner};
use crate::install::{self, Layout, Progress};
use crate::sources::{self, Availability, Environment, Source, Tab};
use crate::store::{self, Action, CatalogueState, Row, Sources};
use crate::trust::CatalogueEntry;
use slopos_core::software_config::SoftwareConfig;

pub const WIDTH: u32 = 760;
pub const HEIGHT: u32 = 500;
const LIST_WIDTH: u32 = 300;
const HEADER_HEIGHT: i32 = 50;
const TABS_ROW: i32 = 30;
const TOGGLES_ROW: i32 = 24;
const STATUS_HEIGHT: u32 = 24;
const ROW_HEIGHT: u32 = 44;
const SEARCH_WIDTH: u32 = 260;
/// The text shown where an install button would be on a search-only tab.
pub const NOT_BUILT: &str = "Installing from this source is not built yet";

const LIST_ID: u64 = 10;
const PRIMARY_ID: u64 = 20;
const REMOVE_ID: u64 = 21;
const CANCEL_ID: u64 = 22;
const OPEN_ID: u64 = 23;
const PROGRESS_ID: u64 = 24;
const DETAILS_ID: u64 = 25;
const STATE_ID: u64 = 26;
const STATUS_ID: u64 = 27;
const SEARCH_ID: u64 = 28;
const NOTE_ID: u64 = 29;
/// Tab buttons take TAB_BASE + their position in the tab row.
const TAB_BASE: u64 = 40;
const FLATPAK_ID: u64 = 60;
const SNAP_ID: u64 = 61;
const AUR_ID: u64 = 62;
const MENU_ID: u64 = 100;
const DIALOG_ID: u64 = 300;

// Menu positions: File, App, View.
const FILE_OPEN: usize = 0;
const FILE_CLOSE: usize = 2;
const APP_INSTALL: usize = 0;
const APP_REMOVE: usize = 1;
const APP_CANCEL: usize = 3;

pub type WakeSlot = Arc<Mutex<Option<Proxy>>>;

fn wake(slot: &WakeSlot) {
    if let Ok(guard) = slot.lock() {
        if let Some(proxy) = guard.as_ref() {
            proxy.wake();
        }
    }
}

/// A finished transaction.
#[derive(Debug)]
enum Finished {
    Installed {
        name: String,
        result: Result<(), String>,
    },
    Removed {
        name: String,
        result: Result<(), String>,
    },
}

struct Running {
    id: String,
    verb: &'static str,
    progress: Arc<Progress>,
    done: Receiver<Finished>,
}

/// How a transaction reaches the outside world; tests swap in local files.
pub type Installer =
    Arc<dyn Fn(&CatalogueEntry, &Layout, &Progress) -> Result<(), String> + Send + Sync>;

pub fn network_installer() -> Installer {
    Arc::new(|entry, layout, progress| {
        install::install(entry, layout, progress)
            .map(|_| ())
            .map_err(|error| error.to_string())
    })
}

/// Where the search-only tabs stand.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Listing {
    /// Nothing searched on this tab yet.
    Idle,
    Loading(String),
    /// `total` is the number of matches before the display cap.
    Found {
        query: String,
        total: usize,
    },
    Failed(String),
}

/// A search on a worker thread. Dropping it abandons the result.
struct SearchJob {
    source: Source,
    query: String,
    done: Receiver<Result<(Vec<Package>, usize), String>>,
}

/// A row for a package a search-only tab lists.
fn package_row(package: &Package) -> Row {
    Row {
        id: package.id.clone(),
        name: package.name.clone(),
        summary: package.summary.clone(),
        description: String::new(),
        category: String::new(),
        icon: String::new(),
        publisher: package.origin.clone(),
        license: String::new(),
        entry: None,
        installed_version: None,
        version: package.version.clone(),
    }
}

/// The search format a backend tab uses on this machine.
fn format_for(source: Source, machine: &sources::Machine) -> Option<Format> {
    match source {
        Source::Native => machine.family.map(|family| match family {
            sources::Family::Apt => Format::Apt,
            sources::Family::Dnf => Format::Dnf,
            sources::Family::Pacman => Format::Pacman,
        }),
        Source::Flatpak => Some(Format::Flatpak),
        Source::Snap => Some(Format::Snap),
        Source::Aur => Some(Format::Pacman),
        Source::Slopos => None,
    }
}

pub struct SoftwareApp {
    window: (u32, u32),
    sources: Sources,
    layout: Layout,
    architecture: String,
    installer: Installer,
    wake: WakeSlot,
    environment: Environment,
    runner: Runner,
    config: SoftwareConfig,
    tabs: Vec<Tab>,
    active: Source,
    tab_buttons: Vec<Button>,
    toggles: Vec<(Source, Checkbox)>,
    search: TextField,
    listing: Listing,
    search_job: Option<SearchJob>,
    state: CatalogueState,
    /// The rows the active tab shows.
    rows: Vec<Row>,
    catalogue_rows: Vec<Row>,
    backend_rows: Vec<Row>,
    list: ListView,
    primary: Button,
    remove: Button,
    cancel: Button,
    open: Button,
    progress: ProgressBar,
    running: Option<Running>,
    dialog: Option<(Dialog, String)>,
    menu: MenuBar,
    focus: FocusChain,
    icons: IconLookup,
    icon_cache: HashMap<(String, u32), Option<Image>>,
    status: String,
    status_error: bool,
    font: Font,
}

fn menu_bar() -> MenuBar {
    MenuBar::new(
        MENU_ID,
        vec![
            (
                "File",
                vec![
                    MenuItem::new("Open App").shortcut("Ctrl+O"),
                    MenuItem::separator(),
                    MenuItem::new("Close Window").shortcut("Ctrl+W"),
                ],
            ),
            (
                "App",
                vec![
                    MenuItem::new("Install").shortcut("Ctrl+I"),
                    MenuItem::new("Remove…").shortcut("Delete"),
                    MenuItem::separator(),
                    MenuItem::new("Cancel Download").shortcut("Escape"),
                ],
            ),
            ("View", vec![MenuItem::new("Refresh").shortcut("F5")]),
        ],
    )
}

impl SoftwareApp {
    /// `runner` performs the search-only tabs' searches; tests supply a fake.
    pub fn new(
        sources: Sources,
        layout: Layout,
        architecture: &str,
        installer: Installer,
        wake: WakeSlot,
        environment: Environment,
        runner: Runner,
    ) -> Self {
        let repaired = install::recover(&layout);
        let mut app = Self {
            window: (WIDTH, HEIGHT),
            sources,
            layout,
            architecture: architecture.to_owned(),
            installer,
            wake,
            environment,
            runner,
            config: SoftwareConfig::default(),
            tabs: Vec::new(),
            active: Source::Slopos,
            tab_buttons: Vec::new(),
            toggles: Vec::new(),
            search: TextField::new(SEARCH_ID, "Search packages"),
            listing: Listing::Idle,
            search_job: None,
            state: CatalogueState::Missing,
            rows: Vec::new(),
            catalogue_rows: Vec::new(),
            backend_rows: Vec::new(),
            list: ListView::new(LIST_ID, "Applications", ROW_HEIGHT),
            primary: Button::new(PRIMARY_ID, "Install", ButtonPlateVariant::Default),
            remove: Button::new(REMOVE_ID, "Remove…", ButtonPlateVariant::Regular),
            cancel: Button::new(CANCEL_ID, "Cancel", ButtonPlateVariant::Regular),
            open: Button::new(OPEN_ID, "Open", ButtonPlateVariant::Regular),
            progress: ProgressBar::new(PROGRESS_ID, "Download progress"),
            running: None,
            dialog: None,
            menu: menu_bar(),
            focus: FocusChain::default(),
            icons: IconLookup::from_environment(),
            icon_cache: HashMap::new(),
            status: String::new(),
            status_error: false,
            font: Font::system(12.0),
        };
        app.search.placeholder = "Type a search, then Enter".into();
        app.list.focused = true;
        app.config = match SoftwareConfig::load(&app.environment.config_path) {
            Ok(config) => config,
            Err(error) => {
                app.fail(format!("Can't read software settings: {error}"));
                SoftwareConfig::default()
            }
        };
        app.reload();
        // The first tab is the native one when there is one, else the catalogue.
        app.active = app.tabs.first().map_or(Source::Slopos, |tab| tab.source);
        app.show_active();
        if repaired > 0 {
            app.notice(format!(
                "Finished cleaning up {repaired} interrupted install(s)."
            ));
        }
        app.layout();
        app.sync();
        app
    }

    fn notice(&mut self, text: impl Into<String>) {
        self.status = text.into();
        self.status_error = false;
    }

    fn fail(&mut self, text: impl Into<String>) {
        self.status = text.into();
        self.status_error = true;
    }

    /// Re-read the catalogue, the tools and the settings, keeping the
    /// selected app.
    fn reload(&mut self) {
        let (state, rows) = store::load(&self.sources, &self.layout, &self.architecture);
        self.state = state;
        self.catalogue_rows = rows;
        self.retune_tabs();
    }

    /// Recompute the tabs from the machine, the tools and the settings, and
    /// rebuild their controls. A tab that disappears falls back to the
    /// catalogue.
    fn retune_tabs(&mut self) {
        let tool_present: &dyn Fn(&str) -> bool = &*self.environment.tool_present;
        self.tabs = sources::tabs(&self.environment.machine, &self.config, tool_present);
        if !self.tabs.iter().any(|tab| tab.source == self.active) {
            self.select_tab_state(Source::Slopos);
        }
        self.tab_buttons = self
            .tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                Button::new(
                    TAB_BASE + index as u64,
                    &tab.caption(),
                    ButtonPlateVariant::Regular,
                )
            })
            .collect();
        let machine = self.environment.machine;
        let config = self.config;
        self.toggles = [
            (Source::Flatpak, FLATPAK_ID, "Flatpak", config.flatpak, true),
            (Source::Snap, SNAP_ID, "Snap", config.snap, true),
            (
                Source::Aur,
                AUR_ID,
                "AUR (Arch)",
                config.aur,
                machine.arch_linux,
            ),
        ]
        .into_iter()
        .filter(|(_, _, _, _, shown)| *shown)
        .map(|(source, id, label, checked, _)| (source, Checkbox::new(id, label, checked)))
        .collect();
        self.show_active();
    }

    /// Make `source` the active tab, dropping any search that belonged to the
    /// previous one.
    fn select_tab_state(&mut self, source: Source) {
        if source == self.active {
            return;
        }
        self.active = source;
        self.search_job = None;
        self.listing = Listing::Idle;
        self.backend_rows.clear();
    }

    /// Switch tabs from a click or the keyboard.
    fn select_tab(&mut self, source: Source) {
        if source == self.active {
            return;
        }
        self.select_tab_state(source);
        self.show_active();
        let hint = self.tab_hint();
        self.notice(hint);
    }

    /// Show the active tab's rows, keeping the selected row when it is still
    /// listed.
    fn show_active(&mut self) {
        let keep = self.selected().map(|row| row.id.clone());
        self.rows = if self.active.is_backend() {
            self.backend_rows.clone()
        } else {
            self.catalogue_rows.clone()
        };
        self.list.reset(self.rows.len());
        let index = keep
            .and_then(|id| self.rows.iter().position(|row| row.id == id))
            .or((!self.rows.is_empty()).then_some(0));
        if let Some(index) = index {
            self.list.select_only(index);
        }
    }

    fn tab(&self) -> Option<&Tab> {
        self.tabs.iter().find(|tab| tab.source == self.active)
    }

    /// The label of the active tab, for messages.
    fn active_label(&self) -> String {
        self.tab()
            .map_or_else(|| "SLOPOS".into(), |tab| tab.label.clone())
    }

    /// The reason the active search-only tab cannot search, when it is missing
    /// its tool.
    fn unavailable_reason(&self) -> Option<String> {
        match &self.tab()?.availability {
            Availability::Unavailable(reason) => Some(reason.clone()),
            Availability::Ready { .. } => None,
        }
    }

    /// What to say when the user has just switched to a tab.
    fn tab_hint(&self) -> String {
        if let Some(reason) = self.unavailable_reason() {
            return reason;
        }
        if self.active.is_backend() {
            format!(
                "Type a search term and press Enter to list packages from {}.",
                self.active_label()
            )
        } else {
            String::new()
        }
    }

    /// The line under the title and whether it reports a failure. Catalogue
    /// tabs show the catalogue's state; search-only tabs show their own.
    fn standing(&self) -> (String, bool) {
        if !self.active.is_backend() {
            let failed = matches!(self.state, CatalogueState::Rejected(_));
            return (self.state.describe(), failed);
        }
        if let Some(reason) = self.unavailable_reason() {
            return (reason, true);
        }
        let label = self.active_label();
        match &self.listing {
            Listing::Idle => (self.tab_hint(), false),
            Listing::Loading(query) => (format!("Searching {label} for {query}…"), false),
            Listing::Found { query, total } => {
                (format!("{total} matches for {query} in {label}."), false)
            }
            Listing::Failed(error) => (format!("{label} search failed: {error}"), true),
        }
    }

    /// What the empty list says on a search-only tab.
    fn empty_message(&self) -> String {
        if let Some(reason) = self.unavailable_reason() {
            return reason;
        }
        match &self.listing {
            Listing::Idle => self.tab_hint(),
            Listing::Loading(_) => "Searching…".into(),
            Listing::Found { query, .. } => {
                format!("No packages match {query} in {}.", self.active_label())
            }
            Listing::Failed(error) => error.clone(),
        }
    }

    fn selected(&self) -> Option<&Row> {
        self.list.cursor().and_then(|index| self.rows.get(index))
    }

    fn top(&self) -> i32 {
        self.menu.height() as i32
    }

    /// The y of the first row of the list and of the detail pane.
    fn body_top(&self) -> i32 {
        self.top() + HEADER_HEIGHT + TABS_ROW + TOGGLES_ROW
    }

    fn detail(&self) -> Rect {
        let top = self.body_top();
        Rect {
            x: LIST_WIDTH as i32 + 16,
            y: top + 12,
            width: self.window.0.saturating_sub(LIST_WIDTH + 32),
            height: self
                .window
                .1
                .saturating_sub(top as u32 + STATUS_HEIGHT + 24),
        }
    }

    fn layout(&mut self) {
        let (width, height) = self.window;
        let top = self.top();
        self.menu.bounds = Rect {
            x: 0,
            y: 0,
            width,
            height: self.menu.height(),
        };
        self.search.bounds = Rect {
            x: width.saturating_sub(SEARCH_WIDTH + 14) as i32,
            y: top + 6,
            width: SEARCH_WIDTH,
            height: 22,
        };
        // Tabs run left to right under the header.
        let tabs_y = top + HEADER_HEIGHT + 4;
        let mut x = 14;
        for button in &mut self.tab_buttons {
            let tab_width = (measure(&button.label, &self.font).width as u32 + 28).max(70);
            button.bounds = Rect {
                x,
                y: tabs_y,
                width: tab_width,
                height: 22,
            };
            x += tab_width as i32 + 6;
        }
        // Source toggles sit on their own row, left to right.
        let toggles_y = top + HEADER_HEIGHT + TABS_ROW + 4;
        let mut x = 14 + 62;
        for (_, checkbox) in &mut self.toggles {
            checkbox.bounds.x = x;
            checkbox.bounds.y = toggles_y;
            x += checkbox.bounds.width as i32 + 18;
        }
        let list_top = self.body_top();
        self.list.set_bounds(Rect {
            x: 0,
            y: list_top,
            width: LIST_WIDTH,
            height: height.saturating_sub(list_top as u32 + STATUS_HEIGHT),
        });
        let d = self.detail();
        let buttons_y = d.y + 70;
        let mut x = d.x + 60;
        for button in [&mut self.primary, &mut self.remove, &mut self.open] {
            let width = measure(&button.label, &self.font).width as u32 + 28;
            button.bounds = Rect {
                x,
                y: buttons_y,
                width: width.max(80),
                height: 20,
            };
            x += width.max(80) as i32 + 8;
        }
        self.progress.bounds = Rect {
            x: d.x + 60,
            y: buttons_y + 30,
            width: d.width.saturating_sub(150),
            height: 14,
        };
        self.cancel.bounds = Rect {
            x: d.x + d.width as i32 - 80,
            y: buttons_y + 27,
            width: 80,
            height: 20,
        };
        if let Some((dialog, _)) = &mut self.dialog {
            dialog.layout(self.window);
        }
    }

    /// Bring buttons, progress and menus in line with the selection and any
    /// running transaction.
    fn sync(&mut self) {
        let busy = self.running.is_some();
        let backend = self.active.is_backend();
        let selected = self.selected().cloned();
        let action = selected.as_ref().map(Row::action);
        self.primary.label = match action {
            Some(Action::Update) => "Update".into(),
            _ => "Install".into(),
        };
        // Search-only tabs never install; their detail shows NOT_BUILT instead.
        self.primary.enabled =
            !busy && !backend && matches!(action, Some(Action::Install | Action::Update));
        self.remove.enabled = !busy && !backend && selected.as_ref().is_some_and(Row::is_installed);
        self.open.enabled = !backend
            && selected.as_ref().is_some_and(Row::is_installed)
            && selected
                .as_ref()
                .is_some_and(|row| self.layout.is_installed(&row.id));
        self.cancel.enabled = busy;
        self.progress.fraction = self.running.as_ref().and_then(|r| r.progress.fraction());
        let mut ids: Vec<u64> = self.tab_buttons.iter().map(|tab| tab.id).collect();
        ids.extend(self.toggles.iter().map(|(_, checkbox)| checkbox.id));
        if backend {
            ids.push(SEARCH_ID);
        }
        ids.push(LIST_ID);
        for (id, enabled) in [
            (PRIMARY_ID, self.primary.enabled),
            (REMOVE_ID, self.remove.enabled),
            (OPEN_ID, self.open.enabled),
            (CANCEL_ID, self.cancel.enabled),
        ] {
            if enabled {
                ids.push(id);
            }
        }
        let focused = self.focus.focused();
        self.focus.set_ids(ids);
        if focused.is_none_or(|id| !self.focus.ids().contains(&id)) {
            self.focus.focus(LIST_ID);
        }
        let f = self.focus.focused();
        self.list.focused = f == Some(LIST_ID);
        for (tab, info) in self.tab_buttons.iter_mut().zip(&self.tabs) {
            tab.variant = if info.source == self.active {
                ButtonPlateVariant::Default
            } else {
                ButtonPlateVariant::Regular
            };
            tab.focused = f == Some(tab.id);
        }
        for (_, checkbox) in &mut self.toggles {
            checkbox.focused = f == Some(checkbox.id);
        }
        self.search.enabled = backend;
        self.search.focused = backend && f == Some(SEARCH_ID);
        self.primary.focused = f == Some(PRIMARY_ID);
        self.remove.focused = f == Some(REMOVE_ID);
        self.open.focused = f == Some(OPEN_ID);
        self.cancel.focused = f == Some(CANCEL_ID);
        self.layout();
        let install_label = self.primary.label.clone();
        let [file, app, _] = &mut self.menu.menus[..] else {
            return;
        };
        file.items[FILE_OPEN].enabled = self.open.enabled;
        app.items[APP_INSTALL].label = install_label;
        app.items[APP_INSTALL].enabled = self.primary.enabled;
        app.items[APP_REMOVE].enabled = self.remove.enabled;
        app.items[APP_CANCEL].enabled = busy;
    }

    /// Start a search on the active search-only tab.
    fn start_search(&mut self) {
        if !self.active.is_backend() {
            return;
        }
        if let Some(reason) = self.unavailable_reason() {
            return self.fail(reason);
        }
        let Some(Availability::Ready { program }) = self.tab().map(|tab| tab.availability.clone())
        else {
            return;
        };
        let Some(format) = format_for(self.active, &self.environment.machine) else {
            return;
        };
        let query = match backends::validate_query(self.search.text()) {
            Ok(query) => query.to_owned(),
            Err(error) => return self.fail(error),
        };
        let search = match backends::search(format, &program, &query) {
            Ok(search) => search,
            Err(error) => return self.fail(error),
        };
        let (sender, done) = mpsc::channel();
        let runner = self.runner.clone();
        let slot = self.wake.clone();
        let spawned = thread::Builder::new()
            .name("software-search".into())
            .spawn(move || {
                let result = runner(&search).map(|text| {
                    let mut packages = backends::parse(format, &text);
                    let total = packages.len();
                    packages.truncate(backends::MAX_RESULTS);
                    (packages, total)
                });
                let _ = sender.send(result);
                wake(&slot);
            });
        match spawned {
            Ok(_) => {
                let label = self.active_label();
                self.notice(format!("Searching {label} for {query}…"));
                self.search_job = Some(SearchJob {
                    source: self.active,
                    query: query.clone(),
                    done,
                });
                self.listing = Listing::Loading(query);
                self.backend_rows.clear();
                self.show_active();
            }
            Err(error) => self.fail(format!("Can't start the search: {error}")),
        }
    }

    /// Collect a finished search. Returns whether anything changed.
    fn poll_search(&mut self) -> bool {
        let Some(job) = &self.search_job else {
            return false;
        };
        let result = match job.done.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return false,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("the search stopped unexpectedly".to_owned())
            }
        };
        let job = self.search_job.take().expect("a search is running");
        // A result for a tab the user has since left is dropped with its job.
        if job.source != self.active {
            return true;
        }
        let label = self.active_label();
        match result {
            Ok((packages, total)) => {
                self.backend_rows = packages.iter().map(package_row).collect();
                let shown = self.backend_rows.len();
                self.notice(if total == 0 {
                    format!("No packages match {} in {label}.", job.query)
                } else if total > shown {
                    format!("{total} matches in {label}; showing the first {shown}.")
                } else {
                    format!("{total} matches in {label}.")
                });
                self.listing = Listing::Found {
                    query: job.query,
                    total,
                };
            }
            Err(error) => {
                self.fail(format!("{label} search failed: {error}"));
                self.listing = Listing::Failed(error);
                self.backend_rows.clear();
            }
        }
        self.show_active();
        true
    }

    /// Turn a source on or off from its toggle, save the settings and retune
    /// the tabs.
    fn set_option(&mut self, source: Source, value: bool) {
        let label = match source {
            Source::Flatpak => {
                self.config.flatpak = value;
                "Flatpak"
            }
            Source::Snap => {
                self.config.snap = value;
                "Snap"
            }
            Source::Aur => {
                self.config.aur = value;
                "AUR"
            }
            Source::Native | Source::Slopos => return,
        };
        let saved = self.config.save(&self.environment.config_path);
        self.retune_tabs();
        match saved {
            Ok(()) if value => {
                self.notice(format!("{label} is on. Search its tab to list packages."))
            }
            Ok(()) => self.notice(format!("{label} is off.")),
            Err(error) => self.fail(format!("Can't save software settings: {error}")),
        }
    }

    fn start_install(&mut self) {
        if self.active.is_backend() {
            return self.notice(NOT_BUILT);
        }
        let Some(row) = self.selected().cloned() else {
            return;
        };
        let (Some(entry), Action::Install | Action::Update) = (row.entry.clone(), row.action())
        else {
            return;
        };
        if self.running.is_some() {
            return;
        }
        let verb = if row.action() == Action::Update {
            "Updating"
        } else {
            "Installing"
        };
        let progress = Arc::new(Progress::default());
        let slot = self.wake.clone();
        progress.on_change(move || wake(&slot));
        let (sender, done) = mpsc::channel();
        let installer = self.installer.clone();
        let layout = self.layout.clone();
        let worker_progress = progress.clone();
        let slot = self.wake.clone();
        let spawned = thread::Builder::new()
            .name("software-install".into())
            .spawn(move || {
                let result = installer(&entry, &layout, &worker_progress);
                let _ = sender.send(Finished::Installed {
                    name: entry.name.clone(),
                    result,
                });
                wake(&slot);
            });
        match spawned {
            Ok(_) => {
                self.notice(format!("{verb} {}…", row.name));
                self.running = Some(Running {
                    id: row.id,
                    verb,
                    progress,
                    done,
                });
            }
            Err(error) => self.fail(format!("Can't start the installer: {error}")),
        }
    }

    fn confirm_remove(&mut self) {
        let Some(row) = self.selected().cloned() else {
            return;
        };
        if !row.is_installed() || self.running.is_some() {
            return;
        }
        let mut dialog = Dialog::new(
            DIALOG_ID,
            &format!("Remove {}?", row.name),
            &format!(
                "{} and its menu entry will be deleted from this computer. Your own documents are not touched.",
                row.name
            ),
            &["Cancel", "Remove"],
        );
        dialog.set_default(0);
        dialog.layout(self.window);
        self.dialog = Some((dialog, row.id));
    }

    fn start_remove(&mut self, id: String) {
        let Some(row) = self.rows.iter().find(|row| row.id == id).cloned() else {
            return;
        };
        let (sender, done) = mpsc::channel();
        let layout = self.layout.clone();
        let slot = self.wake.clone();
        let name = row.name.clone();
        let spawned = thread::Builder::new()
            .name("software-remove".into())
            .spawn(move || {
                let result = install::uninstall(&id, &layout).map_err(|error| error.to_string());
                let _ = sender.send(Finished::Removed { name, result });
                wake(&slot);
            });
        match spawned {
            Ok(_) => {
                self.notice(format!("Removing {}…", row.name));
                self.running = Some(Running {
                    id: row.id,
                    verb: "Removing",
                    progress: Arc::new(Progress::default()),
                    done,
                });
            }
            Err(error) => self.fail(format!("Can't start the removal: {error}")),
        }
    }

    fn open_selected(&mut self) {
        let Some(row) = self.selected() else {
            return;
        };
        let path = self.layout.appimage(&row.id);
        let name = row.name.clone();
        if let Err(error) = slopos_core::process::spawn_detached(&path, &[] as &[&str]) {
            self.fail(format!("Can't open {name}: {error}"));
        }
    }

    /// Collect whatever finished on a worker thread. Returns whether anything
    /// changed.
    fn poll(&mut self) -> bool {
        let transaction = self.poll_transaction();
        let search = self.poll_search();
        transaction || search
    }

    /// Collect a finished transaction. Returns whether anything changed.
    fn poll_transaction(&mut self) -> bool {
        let Some(running) = &self.running else {
            return false;
        };
        let finished = match running.done.try_recv() {
            Ok(finished) => finished,
            Err(mpsc::TryRecvError::Empty) => return true, // progress moved
            Err(mpsc::TryRecvError::Disconnected) => Finished::Installed {
                name: running.id.clone(),
                result: Err("the installer stopped unexpectedly".into()),
            },
        };
        let verb = running.verb;
        self.running = None;
        match finished {
            Finished::Installed {
                name,
                result: Ok(()),
            } => self.notice(if verb == "Updating" {
                format!("{name} is up to date.")
            } else {
                format!("{name} is installed. Find it with Search or in the Applications menu.")
            }),
            Finished::Installed {
                name,
                result: Err(error),
            } => self.fail(format!("{name} was not installed: {error}")),
            Finished::Removed {
                name,
                result: Ok(()),
            } => self.notice(format!("{name} was removed.")),
            Finished::Removed {
                name,
                result: Err(error),
            } => self.fail(format!("{name} was not removed: {error}")),
        }
        self.reload();
        true
    }

    fn cancel_running(&mut self) {
        if let Some(running) = &self.running {
            running.progress.cancel();
            self.notice("Cancelling…");
        }
    }

    fn handle(&mut self, event: &Event, control: &mut Control) {
        if let Some((dialog, _)) = &mut self.dialog {
            match dialog.handle(event) {
                DialogOutcome::Chosen { index, .. } => {
                    let (_, id) = self.dialog.take().expect("dialog is open");
                    if index == 1 {
                        self.start_remove(id);
                    }
                }
                DialogOutcome::Copy(text) => control.set_clipboard(&text),
                _ => {}
            }
            return;
        }
        match self.menu.handle(event, self.window) {
            MenuBarOutcome::Chosen { menu, item } => {
                match (menu, item) {
                    (0, FILE_OPEN) => self.open_selected(),
                    (0, FILE_CLOSE) => control.quit(),
                    (1, APP_INSTALL) => self.start_install(),
                    (1, APP_REMOVE) => self.confirm_remove(),
                    (1, APP_CANCEL) => self.cancel_running(),
                    (2, _) => self.reload(),
                    _ => {}
                }
                return;
            }
            MenuBarOutcome::Changed => return,
            MenuBarOutcome::Ignored => {}
        }
        if let Event::KeyPressed { key, modifiers } = event {
            match key {
                Key::Character('w' | 'W' | 'q' | 'Q') if modifiers.control => {
                    return control.quit()
                }
                Key::Character('i' | 'I') if modifiers.control => return self.start_install(),
                Key::Character('o' | 'O') if modifiers.control => return self.open_selected(),
                Key::Function(5) => return self.reload(),
                Key::Delete if self.focus.focused() == Some(LIST_ID) => {
                    return self.confirm_remove()
                }
                Key::Escape if self.running.is_some() => return self.cancel_running(),
                _ => {}
            }
        }
        if self.focus.handle(event).is_some() {
            return;
        }
        let clicked = |id: u64| matches!(event, Event::Accessibility(AccessibilityRequest::Click(target)) if *target == id);
        if let Event::Accessibility(AccessibilityRequest::Focus(id)) = event {
            if let Some(index) = self.list.row_for_node(*id) {
                self.list.select_only(index);
            }
            self.focus.focus(*id);
            return;
        }
        // Source tabs.
        for index in 0..self.tab_buttons.len() {
            let id = self.tab_buttons[index].id;
            let clicked = matches!(
                event,
                Event::Accessibility(AccessibilityRequest::Click(target)) if *target == id
            );
            if self.tab_buttons[index].handle(event) == ButtonOutcome::Activated || clicked {
                let source = self.tabs[index].source;
                return self.select_tab(source);
            }
        }
        // Source toggles.
        for index in 0..self.toggles.len() {
            match self.toggles[index].1.handle(event) {
                ToggleOutcome::Toggled(value) => {
                    let source = self.toggles[index].0;
                    return self.set_option(source, value);
                }
                ToggleOutcome::Changed => return,
                ToggleOutcome::FocusRequested => {
                    let id = self.toggles[index].1.id;
                    self.focus.focus(id);
                    return;
                }
                ToggleOutcome::Ignored => {}
            }
        }
        // The search box, on search-only tabs.
        if self.active.is_backend() {
            match self.search.handle(event) {
                TextFieldOutcome::Ignored => {}
                TextFieldOutcome::Submitted => return self.start_search(),
                TextFieldOutcome::FocusRequested => {
                    self.focus.focus(SEARCH_ID);
                    return;
                }
                TextFieldOutcome::Copy(text) => return control.set_clipboard(&text),
                _ => return,
            }
        }
        let names: Vec<String> = self.rows.iter().map(|row| row.name.clone()).collect();
        let label = |index: usize| names.get(index).cloned().unwrap_or_default();
        match self.list.handle(event, control.modifiers(), &label) {
            ListOutcome::FocusRequested => {
                self.focus.focus(LIST_ID);
            }
            ListOutcome::Activated(_) if self.active.is_backend() => self.start_install(),
            ListOutcome::Activated(_) => {
                let action = self.selected().map(Row::action);
                match action {
                    Some(Action::Install | Action::Update) => self.start_install(),
                    Some(Action::UpToDate) => self.open_selected(),
                    None => {}
                }
            }
            _ => {}
        }
        if self.primary.handle(event) == ButtonOutcome::Activated
            || (clicked(PRIMARY_ID) && self.primary.enabled)
        {
            self.start_install();
        }
        if self.remove.handle(event) == ButtonOutcome::Activated
            || (clicked(REMOVE_ID) && self.remove.enabled)
        {
            self.confirm_remove();
        }
        if self.open.handle(event) == ButtonOutcome::Activated
            || (clicked(OPEN_ID) && self.open.enabled)
        {
            self.open_selected();
        }
        if self.cancel.handle(event) == ButtonOutcome::Activated
            || (clicked(CANCEL_ID) && self.cancel.enabled)
        {
            self.cancel_running();
        }
    }

    fn icon(&mut self, name: &str, size: u32, scale: u32) -> Option<Image> {
        let key = (name.to_owned(), size * scale);
        if let Some(image) = self.icon_cache.get(&key) {
            return image.clone();
        }
        let image = [name, "application-x-executable", "application-x-generic"]
            .into_iter()
            .filter(|candidate| !candidate.is_empty())
            .find_map(|candidate| self.icons.load(candidate, size, scale));
        self.icon_cache.insert(key, image.clone());
        image
    }

    fn paint_all(&mut self, surface: &RasterSurface) -> Result<(), RenderError> {
        let colors = palette();
        let (width, height) = self.window;
        let scale = surface.scale().factor();
        surface.clear(colors.surface)?;
        let font = self.font.clone();
        let bold = font.clone().bold();
        let title_font = Font::system(16.0).bold();
        let line = measure("Ag", &font).height as i32;

        // Header: title, the active source's standing and the search box.
        let top = self.top();
        surface.draw_text("Software", 14, top + 8, &title_font, colors.ink)?;
        let (standing, failed) = self.standing();
        let state_ink = if failed {
            colors.danger
        } else {
            colors.ink_secondary
        };
        let state_line = wrap(&standing, &font, width.saturating_sub(28))
            .into_iter()
            .next()
            .unwrap_or_default();
        surface.draw_text(&state_line, 14, top + 30, &font, state_ink)?;
        surface.fill_rect(
            Rect {
                x: 0,
                y: top + HEADER_HEIGHT - 1,
                width,
                height: 1,
            },
            colors.border,
        )?;
        // Tabs, then the source toggles on their own row.
        for button in &self.tab_buttons {
            button.paint(surface, &font)?;
        }
        let toggles_y = top + HEADER_HEIGHT + TABS_ROW + 4;
        surface.draw_text("Sources", 14, toggles_y + 3, &font, colors.ink_secondary)?;
        for (_, checkbox) in &self.toggles {
            checkbox.paint(surface)?;
        }
        if self.active.is_backend() {
            self.search.paint(surface)?;
        }
        surface.fill_rect(
            Rect {
                x: 0,
                y: self.body_top() - 1,
                width,
                height: 1,
            },
            colors.border,
        )?;

        // The list: icon, name, summary.
        let mut icons = Vec::with_capacity(self.rows.len());
        for index in self.list.visible_rows() {
            let name = self.rows[index].icon.clone();
            icons.push((index, self.icon(&name, 32, scale)));
        }
        let rows = &self.rows;
        self.list.paint(surface, |surface, index, rect, selected| {
            let row = &rows[index];
            let tile = Rect {
                x: rect.x + 8,
                y: rect.y + 6,
                width: 32,
                height: 32,
            };
            match icons.iter().find(|(i, _)| *i == index) {
                Some((_, Some(icon))) => surface.draw_image_scaled(icon, tile.x, tile.y, 32, 32)?,
                _ => paint_placeholder(surface, tile, &row.name, &title_font)?,
            }
            let ink = if selected {
                colors.selection_ink
            } else {
                colors.ink
            };
            let secondary = if selected {
                colors.selection_ink
            } else {
                colors.ink_secondary
            };
            surface.draw_text(&row.name, rect.x + 48, rect.y + 6, &bold, ink)?;
            let summary = wrap(&row.summary, &font, rect.width.saturating_sub(60))
                .into_iter()
                .next()
                .unwrap_or_default();
            surface.draw_text(&summary, rect.x + 48, rect.y + 8 + line, &font, secondary)?;
            // Installed apps say so; listed packages show their version.
            let mark = if row.is_installed() {
                Some(match row.action() {
                    Action::Update => "Update".to_owned(),
                    _ => "Installed".to_owned(),
                })
            } else if !row.version.is_empty() {
                Some(row.version.clone())
            } else {
                None
            };
            if let Some(mark) = mark {
                let mark_width = measure(&mark, &font).width as i32;
                surface.draw_text(
                    &mark,
                    rect.x + rect.width as i32 - mark_width - 22,
                    rect.y + 6,
                    &font,
                    secondary,
                )?;
            }
            Ok(())
        })?;
        surface.fill_rect(
            Rect {
                x: LIST_WIDTH as i32,
                y: self.body_top(),
                width: 1,
                height: height.saturating_sub(self.body_top() as u32 + STATUS_HEIGHT),
            },
            colors.border,
        )?;

        // Details of the selected app.
        let d = self.detail();
        if let Some(row) = self.selected().cloned() {
            match self.icon(&row.icon, 48, scale) {
                Some(icon) => surface.draw_image_scaled(&icon, d.x, d.y, 48, 48)?,
                None => paint_placeholder(
                    surface,
                    Rect {
                        x: d.x,
                        y: d.y,
                        width: 48,
                        height: 48,
                    },
                    &row.name,
                    &Font::system(22.0).bold(),
                )?,
            }
            surface.draw_text(&row.name, d.x + 60, d.y + 2, &title_font, colors.ink)?;
            let mut facts = vec![row.version_text()];
            if self.active.is_backend() {
                facts.push(self.active_label());
            }
            if !row.category.is_empty() {
                facts.push(row.category.clone());
            }
            if let Some(entry) = &row.entry {
                facts.push(store::size_text(entry.size));
            }
            surface.draw_text(
                &facts.join("  ·  "),
                d.x + 60,
                d.y + 26,
                &font,
                colors.ink_secondary,
            )?;
            let publisher = if self.active.is_backend() {
                if row.publisher.is_empty() {
                    format!("Listed by {}", self.active_label())
                } else {
                    format!("Listed by {} from {}", self.active_label(), row.publisher)
                }
            } else {
                match (&row.entry, row.publisher.is_empty()) {
                    (Some(entry), _) => {
                        format!("Signed by {} (key {})", entry.publisher, entry.key_id)
                    }
                    (None, false) => {
                        format!("From {} · not in the current catalogue", row.publisher)
                    }
                    (None, true) => "Not in the current catalogue".into(),
                }
            };
            surface.draw_text(&publisher, d.x + 60, d.y + 44, &font, colors.ink_secondary)?;
            if self.active.is_backend() {
                // Where an install button would be, say why there is none.
                surface.draw_text(NOT_BUILT, d.x + 60, d.y + 74, &font, colors.ink_secondary)?;
            } else {
                self.primary.paint(surface, &font)?;
                self.remove.paint(surface, &font)?;
                self.open.paint(surface, &font)?;
            }
            let mut y = d.y + 140;
            if let Some(running) = &self.running {
                let text = format!(
                    "{} — {}",
                    running.verb,
                    self.progress
                        .fraction
                        .map_or_else(|| "starting…".into(), |f| format!("{:.0}%", f * 100.0))
                );
                surface.draw_text(
                    &text,
                    d.x + 60,
                    self.progress.bounds.y - 2 - line,
                    &font,
                    colors.ink,
                )?;
                self.progress.paint(surface)?;
                self.cancel.paint(surface, &font)?;
                y += 10;
            }
            let mut text = if row.description.is_empty() {
                row.summary.clone()
            } else {
                row.description.clone()
            };
            if !row.license.is_empty() {
                text.push_str(&format!("\n\nLicense: {}", row.license));
            }
            for paragraph in text.split('\n') {
                for wrapped in wrap(paragraph, &font, d.width.saturating_sub(60)) {
                    if y + line > d.y + d.height as i32 {
                        break;
                    }
                    surface.draw_text(&wrapped, d.x + 60, y, &font, colors.ink)?;
                    y += line + 3;
                }
                y += 4;
            }
        } else {
            let message = if self.active.is_backend() {
                self.empty_message()
            } else {
                match self.state {
                    CatalogueState::Verified { .. } => {
                        "No apps are offered for this computer.".to_owned()
                    }
                    _ => "No apps to show.".to_owned(),
                }
            };
            surface.draw_text(&message, d.x, d.y + 4, &font, colors.ink_secondary)?;
        }

        // Status line.
        let status_y = height as i32 - STATUS_HEIGHT as i32;
        surface.fill_rect(
            Rect {
                x: 0,
                y: status_y,
                width,
                height: 1,
            },
            colors.border,
        )?;
        let ink = if self.status_error {
            colors.danger
        } else {
            colors.ink_secondary
        };
        surface.draw_text(&self.status, 10, status_y + 5, &font, ink)?;
        self.menu.paint_menus(surface)?;
        if let Some((dialog, _)) = &mut self.dialog {
            dialog.paint(surface)?;
        }
        self.menu.paint(surface)
    }
}

/// A stand-in for a missing app icon: a framed tile with the app's initial.
fn paint_placeholder(
    surface: &RasterSurface,
    rect: Rect,
    name: &str,
    font: &Font,
) -> Result<(), RenderError> {
    let colors = palette();
    surface.fill_rect(rect, colors.content)?;
    surface.stroke_rect(rect, colors.border)?;
    surface.fill_rect(
        Rect {
            x: rect.x + 1,
            y: rect.y + rect.height as i32 - 3,
            width: rect.width.saturating_sub(2),
            height: 2,
        },
        colors.bevel_shadow,
    )?;
    let initial: String = name.chars().take(1).flat_map(char::to_uppercase).collect();
    let size = measure(&initial, font);
    surface.draw_text(
        &initial,
        rect.x + (rect.width as i32 - size.width as i32) / 2,
        rect.y + (rect.height as i32 - size.height as i32) / 2,
        font,
        colors.ink_secondary,
    )
}

impl App for SoftwareApp {
    fn event(&mut self, event: &Event, control: &mut Control) {
        match event {
            Event::CloseRequested => {
                self.cancel_running();
                return control.quit();
            }
            Event::Resized { width, height } => {
                self.window = (*width, *height);
                self.layout();
            }
            Event::GlobalMenu(_) => {
                self.menu.handle(event, self.window);
                self.layout();
            }
            Event::Wake => {
                if !self.poll() {
                    return;
                }
            }
            _ => self.handle(event, control),
        }
        self.sync();
        control.request_redraw();
    }

    fn paint(&mut self, surface: &RasterSurface) {
        if let Err(error) = self.paint_all(surface) {
            eprintln!("slopos-software: paint failed: {error}");
        }
    }

    fn menu_bar(&self) -> Option<&MenuBar> {
        Some(&self.menu)
    }

    fn accessibility(&self) -> Option<accesskit::TreeUpdate> {
        let mut nodes = self.menu.accessibility_nodes();
        let label = |index: usize| {
            self.rows
                .get(index)
                .map(|row| {
                    if row.is_installed() {
                        format!("{}, {}, installed", row.name, row.summary)
                    } else {
                        format!("{}, {}", row.name, row.summary)
                    }
                })
                .unwrap_or_default()
        };
        nodes.extend(self.list.accessibility_nodes(&label));
        let (standing, _) = self.standing();
        nodes.push((STATE_ID, slopos_ui::accessibility::text_label(standing)));
        for (tab, info) in self.tab_buttons.iter().zip(&self.tabs) {
            let mut node = tab.accessibility_node();
            if let Availability::Unavailable(reason) = &info.availability {
                node.set_label(format!("{}, {reason}", info.label));
            }
            nodes.push((tab.id, node));
        }
        for (_, checkbox) in &self.toggles {
            nodes.push((checkbox.id, checkbox.accessibility_node()));
        }
        if self.active.is_backend() {
            nodes.push((SEARCH_ID, self.search.accessibility_node()));
        }
        if let Some(row) = self.selected() {
            let details = slopos_ui::accessibility::text_label(format!(
                "{}. {}. {}",
                row.name,
                row.version_text(),
                if row.description.is_empty() {
                    &row.summary
                } else {
                    &row.description
                }
            ));
            nodes.push((DETAILS_ID, details));
            if self.active.is_backend() {
                nodes.push((NOTE_ID, slopos_ui::accessibility::text_label(NOT_BUILT)));
            } else {
                nodes.push((PRIMARY_ID, self.primary.accessibility_node()));
                nodes.push((REMOVE_ID, self.remove.accessibility_node()));
                nodes.push((OPEN_ID, self.open.accessibility_node()));
            }
        }
        if self.running.is_some() {
            nodes.push((PROGRESS_ID, self.progress.accessibility_node()));
            nodes.push((CANCEL_ID, self.cancel.accessibility_node()));
        }
        let status = slopos_ui::accessibility::text_label(self.status.as_str());
        nodes.push((STATUS_ID, status));
        let focus = if let Some((dialog, _)) = &self.dialog {
            nodes.extend(dialog.accessibility_nodes());
            Some(dialog.focus_node())
        } else if let Some(node) = self.menu.focus_node() {
            Some(node)
        } else {
            match self.focus.focused() {
                Some(LIST_ID) => Some(self.list.focus_node()),
                other => other,
            }
        };
        Some(window_tree("Software", nodes, focus))
    }
}

/// The installed file for tests and QA.
#[allow(dead_code)]
pub fn installed_path(layout: &Layout, id: &str) -> PathBuf {
    layout.appimage(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::Search;
    use crate::store::tests::write_sources;
    use crate::trust::signing::sign_catalogue;
    use crate::trust::tests::{key, signed};
    use sha2::{Digest, Sha256};
    use slopos_ui::event::Modifiers;
    use slopos_ui::foundation::geometry::Scale;
    use std::io::Cursor;
    use std::time::{Duration, Instant};

    /// A machine with no native manager and no optional sources, so only the
    /// catalogue tab exists. Its settings live in `dir`.
    fn catalogue_only(dir: &std::path::Path) -> (Environment, Runner) {
        let environment = Environment {
            machine: sources::Machine {
                family: None,
                arch_linux: false,
            },
            config_path: dir.join("software.conf"),
            tool_present: Arc::new(|_: &str| false),
        };
        let runner: Runner = Arc::new(|_: &Search| Err("no searches in this test".to_owned()));
        (environment, runner)
    }

    fn key_event(key: Key) -> Event {
        Event::KeyPressed {
            key,
            modifiers: Modifiers::default(),
        }
    }

    /// An installer that "downloads" `payload` from memory.
    fn local_installer(payload: Vec<u8>) -> Installer {
        Arc::new(move |entry, layout, progress| {
            install::install_from(entry, &mut Cursor::new(payload.clone()), layout, progress)
                .map(|_| ())
                .map_err(|error| error.to_string())
        })
    }

    fn wait_for(app: &mut SoftwareApp, control: &mut Control) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.running.is_some() {
            assert!(Instant::now() < deadline, "transaction never finished");
            thread::sleep(Duration::from_millis(10));
            app.event(&Event::Wake, control);
        }
    }

    #[test]
    fn install_update_and_remove_through_the_window() {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::for_home(&dir.path().join("home"));
        let mut payload = b"\x7fELF".to_vec();
        payload.resize(50_000, 9);
        let (mut catalogue, trust) = signed(5);
        for entry in &mut catalogue.entries {
            entry.sha256 = hex::encode(Sha256::digest(&payload));
            entry.size = payload.len() as u64;
        }
        catalogue.entries[0].name = "Alpha".into();
        catalogue.entries[1].name = "Beta".into();
        sign_catalogue(&mut catalogue, &key(5));
        let sources = write_sources(dir.path(), &catalogue, &trust);
        let (environment, runner) = catalogue_only(dir.path());
        let mut app = SoftwareApp::new(
            sources.clone(),
            layout.clone(),
            "x86_64",
            local_installer(payload.clone()),
            Arc::new(Mutex::new(None)),
            environment.clone(),
            runner.clone(),
        );
        let mut control = Control::default();
        assert!(matches!(app.state, CatalogueState::Verified { .. }));
        assert_eq!(app.rows.len(), 2);
        assert_eq!(app.selected().unwrap().name, "Alpha");
        assert!(app.primary.enabled && !app.remove.enabled && !app.open.enabled);
        assert_eq!(app.menu.menus[1].items[APP_INSTALL].label, "Install");

        // Enter on the list installs the selected app.
        app.event(&key_event(Key::Enter), &mut control);
        assert!(app.running.is_some() && !app.primary.enabled);
        wait_for(&mut app, &mut control);
        assert!(!app.status_error, "{}", app.status);
        assert!(app.status.starts_with("Alpha is installed"));
        assert!(layout.is_installed("example"));
        assert!(app.remove.enabled && app.open.enabled && !app.primary.enabled);

        // A newer catalogue version turns the button into Update.
        catalogue.entries[0].version = "1.1".into();
        sign_catalogue(&mut catalogue, &key(5));
        write_sources(dir.path(), &catalogue, &trust);
        app.event(&key_event(Key::Function(5)), &mut control);
        assert_eq!(app.primary.label, "Update");
        assert_eq!(app.menu.menus[1].items[APP_INSTALL].label, "Update");
        app.event(
            &Event::MenuChosen {
                menu: 1,
                item: APP_INSTALL,
            },
            &mut control,
        );
        wait_for(&mut app, &mut control);
        assert_eq!(app.status, "Alpha is up to date.");
        assert_eq!(app.selected().unwrap().action(), Action::UpToDate);

        // Removal asks first; Cancel keeps the app, Remove deletes it.
        app.event(&key_event(Key::Delete), &mut control);
        assert!(app.dialog.is_some());
        app.event(&key_event(Key::Enter), &mut control); // default: Cancel
        assert!(app.dialog.is_none() && layout.is_installed("example"));
        app.event(
            &Event::MenuChosen {
                menu: 1,
                item: APP_REMOVE,
            },
            &mut control,
        );
        app.event(&key_event(Key::Tab), &mut control);
        app.event(&key_event(Key::Enter), &mut control);
        wait_for(&mut app, &mut control);
        assert_eq!(app.status, "Alpha was removed.");
        assert!(!layout.is_installed("example"));

        // A corrupt download is refused and leaves nothing installed.
        let mut bad = SoftwareApp::new(
            sources,
            layout.clone(),
            "x86_64",
            local_installer(b"\x7fELF-but-different".to_vec()),
            Arc::new(Mutex::new(None)),
            environment,
            runner,
        );
        bad.event(
            &Event::MenuChosen {
                menu: 1,
                item: APP_INSTALL,
            },
            &mut control,
        );
        wait_for(&mut bad, &mut control);
        assert!(bad.status_error && bad.status.starts_with("Alpha was not installed"));
        assert!(!layout.is_installed("example"));

        let tree = app.accessibility().unwrap();
        assert!(tree.nodes.iter().any(|(_, n)| n.label() == Some("Install")));
        for factor in [1, 2] {
            let surface = RasterSurface::new(WIDTH, HEIGHT, Scale::new(factor).unwrap()).unwrap();
            app.paint(&surface);
        }
    }

    #[test]
    fn search_tabs_list_results_and_never_install() {
        const SAMPLE: &str = "zstd - fast lossless compression\nzstd-dev - development files\n";
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::for_home(&dir.path().join("home"));
        let sources = Sources {
            trust: dir.path().join("trust.json"),
            catalogue: None,
        };
        let environment = Environment {
            machine: sources::Machine {
                family: Some(sources::Family::Apt),
                arch_linux: false,
            },
            config_path: dir.path().join("software.conf"),
            tool_present: Arc::new(|name: &str| name == "apt-cache"),
        };
        let runner: Runner = Arc::new(|search: &Search| match search.format {
            Format::Apt => Ok(SAMPLE.to_owned()),
            _ => Err("unexpected tool".to_owned()),
        });
        let mut app = SoftwareApp::new(
            sources,
            layout,
            "x86_64",
            network_installer(),
            Arc::new(Mutex::new(None)),
            environment,
            runner,
        );
        let mut control = Control::default();
        assert_eq!(app.active, Source::Native, "the native tab comes first");
        assert!(app.rows.is_empty(), "nothing is listed before a search");

        app.search.set_text("zstd");
        app.start_search();
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.search_job.is_some() {
            assert!(Instant::now() < deadline, "search never finished");
            thread::sleep(Duration::from_millis(10));
            app.poll();
        }
        assert_eq!(app.rows.len(), 2);
        assert_eq!(app.selected().unwrap().name, "zstd");
        assert!(!app.primary.enabled, "search-only tabs never install");

        // Activating a listed package says the install is not built.
        app.event(&key_event(Key::Enter), &mut control);
        assert_eq!(app.status, NOT_BUILT);
        assert!(!app.status_error);

        // A tab whose tool is missing says so and lists nothing.
        app.set_option(Source::Flatpak, true);
        app.select_tab(Source::Flatpak);
        assert_eq!(
            app.standing(),
            ("unavailable: flatpak not found".to_owned(), true)
        );
        assert!(app.rows.is_empty());

        // Turning the source off returns to the catalogue, and the setting is saved.
        app.set_option(Source::Flatpak, false);
        assert_eq!(app.active, Source::Slopos);
        let saved = SoftwareConfig::load(&dir.path().join("software.conf")).unwrap();
        assert!(!saved.flatpak);
    }

    #[test]
    fn rejected_catalogue_offers_nothing_and_says_why() {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::for_home(&dir.path().join("home"));
        let (catalogue, _) = signed(6);
        let (_, other_trust) = signed(7); // a different publisher key
        let sources = write_sources(dir.path(), &catalogue, &other_trust);
        let (environment, runner) = catalogue_only(dir.path());
        let app = SoftwareApp::new(
            sources,
            layout,
            "x86_64",
            network_installer(),
            Arc::new(Mutex::new(None)),
            environment,
            runner,
        );
        assert!(matches!(app.state, CatalogueState::Rejected(_)));
        assert!(app.rows.is_empty() && !app.primary.enabled);
        let surface = RasterSurface::new(WIDTH, HEIGHT, Scale::new(1).unwrap()).unwrap();
        let mut app = app;
        app.paint(&surface);
    }
}
