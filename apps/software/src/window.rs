//! The Software window (STORE-02): one tab per source available on this
//! machine. The SLOPOS tab lists the verified catalogue and installed
//! AppImages on the left, the chosen app's details and actions on the right.
//! The native, Flatpak, Snap and AUR tabs search their source and install
//! the chosen package through its own tool; a tab whose tool is missing says
//! so and offers no install. The Sources tab lists the system packages,
//! Flatpak, Snap and AUR and switches the optional sources on and off;
//! turning Flatpak on asks first.
//!
//! Installs, updates and removals run one at a time on a worker thread
//! through the STORE-01 transaction code; the window shows real byte
//! progress, can cancel a download, and reports the transaction's own
//! result. Removal asks first. Nothing is offered from a catalogue that did
//! not verify. Searches and package installs also run on worker threads and
//! wake the window as they go; either can be cancelled, which kills the
//! tool.

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
use slopos_ui::controls::range::ProgressBar;
use slopos_ui::controls::tabs::{Tabs, TabsOutcome};
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
use crate::packages;
use crate::run::{self, Cancel};
use crate::sources::{self, Availability, Environment, Section, Source, Status, Tab};
use crate::store::{self, Action, CatalogueState, Row, Sources};
use crate::trust::CatalogueEntry;
use slopos_core::software_config::SoftwareConfig;

pub const WIDTH: u32 = 760;
pub const HEIGHT: u32 = 500;
const LIST_WIDTH: u32 = 300;
const HEADER_HEIGHT: i32 = 50;
const TABS_ROW: i32 = 30;
const STATUS_HEIGHT: u32 = 24;
const ROW_HEIGHT: u32 = 44;
const SEARCH_WIDTH: u32 = 260;
/// The text shown where an install button would be on a source whose install
/// is unavailable: its tool is missing, or no install is built for it.
pub const NOT_BUILT: &str = "Installing from this source is not built yet";
/// Output lines kept from a package install, for showing why it failed.
const OUTPUT_LINES: usize = 8;

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
const SEARCH_CANCEL_ID: u64 = 30;
/// The tab strip is one focus stop; its tabs are TABS_ID + 1 + position.
const TABS_ID: u64 = 40;
/// Switch buttons of the Sources tab.
const FLATPAK_SWITCH_ID: u64 = 60;
const SNAP_SWITCH_ID: u64 = 61;
const AUR_SWITCH_ID: u64 = 62;
/// Text lines of the Sources tab, for accessibility.
const PANEL_TEXT_BASE: u64 = 70;
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
    /// A package installed through its source's own tool.
    Packaged {
        name: String,
        source: String,
        result: Result<(), String>,
    },
    Removed {
        name: String,
        result: Result<(), String>,
    },
    /// The Flatpak setup steps the Sources tab ran.
    FlatpakSetup { result: Result<(), String> },
}

/// What the open dialog is asking.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Prompt {
    /// Remove the app with this id.
    Remove(String),
    /// Install Flatpak and add Flathub, then turn Flatpak support on.
    EnableFlatpak,
}

struct Running {
    id: String,
    verb: &'static str,
    progress: Arc<Progress>,
    /// Stops a package install's tool. Set for every transaction; only
    /// package installs use it.
    stop: Arc<Cancel>,
    /// Output lines of a package install; `None` for an AppImage transaction.
    lines: Option<Receiver<String>>,
    done: Receiver<Finished>,
}

/// How a transaction reaches the outside world; tests swap in local files.
pub type Installer =
    Arc<dyn Fn(&CatalogueEntry, &Layout, &Progress) -> Result<(), String> + Send + Sync>;

/// Runs a package install's command, passing each output line on. Tests
/// supply a fake.
pub type PackageRunner =
    Arc<dyn Fn(&[String], &Cancel, &mut dyn FnMut(&str)) -> Result<(), String> + Send + Sync>;

/// What the window runs for searches and package installs.
#[derive(Clone)]
pub struct Commands {
    pub search: Runner,
    pub install: PackageRunner,
}

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

/// A search on a worker thread. Cancelling it kills the tool; dropping it
/// abandons the result.
struct SearchJob {
    source: Source,
    query: String,
    cancel: Arc<Cancel>,
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
        Source::Slopos | Source::Sources => None,
    }
}

/// The label of a source's switch: what pressing it does.
fn switch_label(source: Source, on: bool) -> String {
    let name = match source {
        Source::Flatpak => "Flatpak",
        Source::Snap => "Snap",
        Source::Aur => "AUR",
        _ => "",
    };
    match (on, source) {
        (true, _) => format!("Turn off {name}"),
        // Turning Flatpak on opens a question first.
        (false, Source::Flatpak) => format!("Turn on {name}…"),
        (false, _) => format!("Turn on {name}"),
    }
}

fn switch_id(source: Source) -> u64 {
    match source {
        Source::Flatpak => FLATPAK_SWITCH_ID,
        Source::Snap => SNAP_SWITCH_ID,
        _ => AUR_SWITCH_ID,
    }
}

/// Where the Sources tab draws one block: its heading, its text lines and
/// its switch button, as y coordinates in the window.
struct PanelBlock {
    heading: i32,
    lines: Vec<i32>,
    button: Option<i32>,
}

pub struct SoftwareApp {
    window: (u32, u32),
    sources: Sources,
    layout: Layout,
    architecture: String,
    installer: Installer,
    wake: WakeSlot,
    environment: Environment,
    commands: Commands,
    config: SoftwareConfig,
    tabs: Vec<Tab>,
    active: Source,
    tab_strip: Tabs,
    /// What is installed and configured, as of the last reload.
    probe: Status,
    /// The Sources tab's blocks.
    sections: Vec<Section>,
    /// The switch button of each Sources block that has one.
    switches: Vec<(Source, Button)>,
    search: TextField,
    search_cancel: Button,
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
    /// The last lines a package install printed, and which package they
    /// belong to, so the detail pane can show why an install failed.
    output: Vec<String>,
    output_key: Option<(Source, String)>,
    dialog: Option<(Dialog, Prompt)>,
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
        commands: Commands,
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
            commands,
            config: SoftwareConfig::default(),
            tabs: Vec::new(),
            active: Source::Slopos,
            tab_strip: Tabs::new(TABS_ID, "Software sources", &[], 0),
            probe: Status::default(),
            sections: Vec::new(),
            switches: Vec::new(),
            search: TextField::new(SEARCH_ID, "Search packages"),
            search_cancel: Button::new(
                SEARCH_CANCEL_ID,
                "Cancel search",
                ButtonPlateVariant::Regular,
            ),
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
            output: Vec::new(),
            output_key: None,
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
        self.probe = self.environment.probe();
        let tool_present: &dyn Fn(&str) -> bool = &*self.environment.tool_present;
        self.tabs = sources::tabs(&self.environment.machine, &self.config, tool_present);
        if !self.tabs.iter().any(|tab| tab.source == self.active) {
            self.select_tab_state(Source::Slopos);
        }
        let labels: Vec<String> = self.tabs.iter().map(Tab::caption).collect();
        self.tab_strip.set_labels(&labels);
        self.sections = sources::sections(&self.environment.machine, &self.config, &self.probe);
        self.switches = self
            .sections
            .iter()
            .filter_map(|section| {
                let on = section.switch?;
                let button = Button::new(
                    switch_id(section.source),
                    &switch_label(section.source, on),
                    ButtonPlateVariant::Regular,
                );
                Some((section.source, button))
            })
            .collect();
        self.show_active();
    }

    /// Make `source` the active tab, stopping any search that belonged to the
    /// previous one.
    fn select_tab_state(&mut self, source: Source) {
        if source == self.active {
            return;
        }
        self.active = source;
        self.stop_search();
        self.listing = Listing::Idle;
        self.backend_rows.clear();
    }

    /// Stop the running search, if any: its tool is killed and its result
    /// dropped. Returns the query that was running.
    fn stop_search(&mut self) -> Option<String> {
        let job = self.search_job.take()?;
        job.cancel.cancel();
        Some(job.query)
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
        self.rows = match self.active {
            Source::Sources => Vec::new(),
            source if source.is_backend() => self.backend_rows.clone(),
            _ => self.catalogue_rows.clone(),
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
        if self.active == Source::Sources {
            return (
                "Choose which package sources this computer uses.".into(),
                false,
            );
        }
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

    /// Whether the detail pane shows a package install's output for `row`
    /// rather than its description.
    fn shows_output(&self, row: &Row) -> bool {
        !self.output.is_empty()
            && self
                .output_key
                .as_ref()
                .is_some_and(|(source, id)| *source == self.active && *id == row.id)
    }

    fn top(&self) -> i32 {
        self.menu.height() as i32
    }

    /// The y of the first row of the list and of the detail pane.
    fn body_top(&self) -> i32 {
        self.top() + HEADER_HEIGHT + TABS_ROW
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
        // Left of the search box while a search runs.
        let cancel_width =
            (measure(&self.search_cancel.label, &self.font).width as u32 + 28).max(96);
        self.search_cancel.bounds = Rect {
            x: self.search.bounds.x - cancel_width as i32 - 8,
            y: self.search.bounds.y,
            width: cancel_width,
            height: 22,
        };
        // The tab strip sits under the header.
        let tabs_y = top + HEADER_HEIGHT + 4;
        self.tab_strip.layout(14, tabs_y, self.window.0.saturating_sub(28), &self.font);
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
        if self.active == Source::Sources {
            self.layout_panel();
        }
        if let Some((dialog, _)) = &mut self.dialog {
            dialog.layout(self.window);
        }
    }

    /// Where the Sources tab draws each block, from the top of the body down.
    fn panel_geometry(&self) -> Vec<PanelBlock> {
        let line = measure("Ag", &self.font).height as i32;
        let mut y = self.body_top() + 12 + if self.running.is_some() { 30 } else { 0 };
        let mut blocks = Vec::with_capacity(self.sections.len());
        for section in &self.sections {
            let heading = y;
            y += line + 8;
            let mut lines = Vec::with_capacity(section.lines.len());
            for _ in &section.lines {
                lines.push(y);
                y += line + 3;
            }
            let button = section.switch.map(|_| {
                let at = y + 2;
                y += 22 + 6;
                at
            });
            y += 14;
            blocks.push(PanelBlock {
                heading,
                lines,
                button,
            });
        }
        blocks
    }

    /// Place the Sources tab's switch buttons and its Cancel button.
    fn layout_panel(&mut self) {
        let blocks = self.panel_geometry();
        let x = 14;
        for (section, block) in self.sections.iter().zip(&blocks) {
            let Some(y) = block.button else { continue };
            let Some((_, button)) = self
                .switches
                .iter_mut()
                .find(|(source, _)| *source == section.source)
            else {
                continue;
            };
            let width = (measure(&button.label, &self.font).width as u32 + 28).max(120);
            button.bounds = Rect {
                x,
                y,
                width,
                height: 22,
            };
        }
        if self.running.is_some() {
            self.cancel.bounds = Rect {
                x: self.window.0 as i32 - 94,
                y: self.body_top() + 8,
                width: 80,
                height: 20,
            };
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
        // A package tab installs when its command can be built; otherwise the
        // detail says NOT_BUILT and why.
        self.primary.enabled = !busy
            && if backend {
                self.package_command().is_ok()
            } else {
                matches!(action, Some(Action::Install | Action::Update))
            };
        self.remove.enabled = !busy && !backend && selected.as_ref().is_some_and(Row::is_installed);
        self.open.enabled = !backend
            && selected.as_ref().is_some_and(Row::is_installed)
            && selected
                .as_ref()
                .is_some_and(|row| self.layout.is_installed(&row.id));
        self.cancel.enabled = busy;
        self.progress.fraction = self.running.as_ref().and_then(|r| r.progress.fraction());
        let searching = self.search_job.is_some();
        self.search_cancel.enabled = searching;
        let sources_tab = self.active == Source::Sources;
        for (_, button) in &mut self.switches {
            button.enabled = !busy;
        }
        let mut ids = vec![TABS_ID];
        if sources_tab {
            ids.extend(
                self.switches
                    .iter()
                    .filter(|(_, button)| button.enabled)
                    .map(|(_, button)| button.id),
            );
        }
        if backend {
            ids.push(SEARCH_ID);
        }
        if searching {
            ids.push(SEARCH_CANCEL_ID);
        }
        if !sources_tab {
            ids.push(LIST_ID);
        }
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
            self.focus
                .focus(if sources_tab { TABS_ID } else { LIST_ID });
        }
        let f = self.focus.focused();
        self.list.focused = f == Some(LIST_ID);
        self.tab_strip.focused = f == Some(TABS_ID);
        if let Some(index) = self.tabs.iter().position(|tab| tab.source == self.active) {
            self.tab_strip.select(index);
        }
        for (_, button) in &mut self.switches {
            button.focused = f == Some(button.id);
        }
        self.search.enabled = backend;
        self.search.focused = backend && f == Some(SEARCH_ID);
        self.search_cancel.focused = f == Some(SEARCH_CANCEL_ID);
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
        // A new search replaces one still running.
        self.stop_search();
        let (sender, done) = mpsc::channel();
        let runner = self.commands.search.clone();
        let cancel = Arc::new(Cancel::default());
        let worker_cancel = cancel.clone();
        let slot = self.wake.clone();
        let spawned = thread::Builder::new()
            .name("software-search".into())
            .spawn(move || {
                let result = runner(&search, &worker_cancel).map(|text| {
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
                    cancel,
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
            Source::Native | Source::Slopos | Source::Sources => return,
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

    /// The Sources tab's switch for `source` was pressed. Turning a source
    /// off is immediate; turning Flatpak on asks first.
    fn toggle_source(&mut self, source: Source) {
        if self.running.is_some() {
            return;
        }
        let on = match source {
            Source::Flatpak => self.config.flatpak,
            Source::Snap => self.config.snap,
            Source::Aur => self.config.aur,
            Source::Native | Source::Slopos | Source::Sources => return,
        };
        if on {
            return self.set_option(source, false);
        }
        if source == Source::Flatpak {
            return self.confirm_flatpak();
        }
        self.set_option(source, true);
    }

    /// Ask whether to set up Flatpak. No, the default, changes nothing.
    fn confirm_flatpak(&mut self) {
        let message = packages::flatpak_prompt(self.probe.manager, &self.probe);
        let mut dialog = Dialog::new(DIALOG_ID, "Turn on Flatpak?", &message, &["No", "Yes"]);
        dialog.set_default(0);
        dialog.layout(self.window);
        self.dialog = Some((dialog, Prompt::EnableFlatpak));
    }

    /// Run the Flatpak setup steps on a worker thread; when they all succeed,
    /// Flatpak support is turned on. Output lines reach the Sources tab as
    /// they arrive.
    fn start_flatpak_setup(&mut self) {
        if self.running.is_some() {
            return;
        }
        let steps = {
            let tool_present: &dyn Fn(&str) -> bool = &*self.environment.tool_present;
            match packages::flatpak_setup_steps(self.probe.manager, &self.probe, tool_present) {
                Ok(steps) => steps,
                Err(reason) => return self.fail(reason),
            }
        };
        if steps.is_empty() {
            return self.set_option(Source::Flatpak, true);
        }
        let stop = Arc::new(Cancel::default());
        let worker_stop = stop.clone();
        let (line_sender, lines) = mpsc::channel();
        let (sender, done) = mpsc::channel();
        let runner = self.commands.install.clone();
        let slot = self.wake.clone();
        let spawned = thread::Builder::new()
            .name("software-flatpak".into())
            .spawn(move || {
                let result = steps.iter().try_for_each(|argv| {
                    runner(argv.as_slice(), &worker_stop, &mut |line: &str| {
                        let _ = line_sender.send(line.to_owned());
                        wake(&slot);
                    })
                });
                let _ = sender.send(Finished::FlatpakSetup { result });
                wake(&slot);
            });
        match spawned {
            Ok(_) => {
                self.output.clear();
                self.output_key = None;
                self.notice("Setting up Flatpak…");
                self.running = Some(Running {
                    id: "flatpak".into(),
                    verb: "Setting up",
                    progress: Arc::new(Progress::default()),
                    stop,
                    lines: Some(lines),
                    done,
                });
            }
            Err(error) => self.fail(format!("Can't start the Flatpak setup: {error}")),
        }
    }

    /// The install command for the selected package on the active source, or
    /// why there is none: no selection, a missing tool, or an invalid name.
    fn package_command(&self) -> Result<Vec<String>, String> {
        let row = self
            .selected()
            .ok_or_else(|| "Select a package to install.".to_owned())?;
        let tab = self.tab().ok_or_else(|| NOT_BUILT.to_owned())?;
        let program = match &tab.availability {
            Availability::Ready { program } => program.as_str(),
            Availability::Unavailable(reason) => return Err(reason.clone()),
        };
        for tool in packages::extra_tools(self.active) {
            if !(*self.environment.tool_present)(tool) {
                return Err(format!("unavailable: {tool} not found"));
            }
        }
        packages::install_argv(
            self.active,
            self.environment.machine.family,
            program,
            &row.id,
        )
    }

    /// Install the selected package through its source's tool on a worker
    /// thread. The output lines reach the detail pane as they arrive.
    fn start_package_install(&mut self) {
        if self.running.is_some() {
            return;
        }
        let Some(row) = self.selected().cloned() else {
            return;
        };
        let argv = match self.package_command() {
            Ok(argv) => argv,
            Err(reason) => return self.fail(reason),
        };
        let label = self.active_label();
        let stop = Arc::new(Cancel::default());
        let worker_stop = stop.clone();
        let (line_sender, lines) = mpsc::channel();
        let (sender, done) = mpsc::channel();
        let runner = self.commands.install.clone();
        let slot = self.wake.clone();
        let name = row.name.clone();
        let source = label.clone();
        let spawned = thread::Builder::new()
            .name("software-package".into())
            .spawn(move || {
                let result = runner(&argv, &worker_stop, &mut |line: &str| {
                    let _ = line_sender.send(line.to_owned());
                    wake(&slot);
                });
                let _ = sender.send(Finished::Packaged {
                    name,
                    source,
                    result,
                });
                wake(&slot);
            });
        match spawned {
            Ok(_) => {
                self.output.clear();
                self.output_key = Some((self.active, row.id.clone()));
                self.notice(format!("Installing {} from {label}…", row.name));
                self.running = Some(Running {
                    id: row.id,
                    verb: "Installing",
                    progress: Arc::new(Progress::default()),
                    stop,
                    lines: Some(lines),
                    done,
                });
            }
            Err(error) => self.fail(format!("Can't start the installer: {error}")),
        }
    }

    /// Stop the running search from the Cancel button or Escape.
    fn cancel_search(&mut self) {
        if let Some(query) = self.stop_search() {
            self.listing = Listing::Idle;
            self.notice(format!("Search for {query} was cancelled."));
            self.show_active();
        }
    }

    fn start_install(&mut self) {
        if self.active.is_backend() {
            return self.start_package_install();
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
                    stop: Arc::new(Cancel::default()),
                    lines: None,
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
        self.dialog = Some((dialog, Prompt::Remove(row.id)));
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
                    stop: Arc::new(Cancel::default()),
                    lines: None,
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
        // Read the outcome before the lines: the worker sends every line
        // before its outcome, so draining the lines afterwards gets them all.
        let finished = match running.done.try_recv() {
            Ok(finished) => Some(finished),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Finished::Installed {
                name: running.id.clone(),
                result: Err("the installer stopped unexpectedly".into()),
            }),
        };
        if let Some(lines) = &running.lines {
            for line in lines.try_iter() {
                if self.output.len() == OUTPUT_LINES {
                    self.output.remove(0);
                }
                self.output.push(line);
            }
        }
        let Some(finished) = finished else {
            return true; // progress moved
        };
        let verb = running.verb;
        self.running = None;
        match finished {
            Finished::Packaged {
                name,
                source,
                result: Ok(()),
            } => self.notice(format!("{name} is installed from {source}.")),
            Finished::Packaged {
                name,
                result: Err(error),
                ..
            } if error == run::CANCELLED => {
                self.notice(format!("Installing {name} was cancelled."))
            }
            Finished::Packaged {
                name,
                result: Err(error),
                ..
            } => self.fail(format!("{name} was not installed: {error}")),
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
            Finished::FlatpakSetup { result: Ok(()) } => self.set_option(Source::Flatpak, true),
            Finished::FlatpakSetup { result: Err(error) } if error == run::CANCELLED => {
                self.notice("Flatpak setup was cancelled. Nothing was turned on.")
            }
            Finished::FlatpakSetup { result: Err(error) } => self.fail(format!(
                "Flatpak was not set up: {error}. Nothing was turned on."
            )),
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
            running.stop.cancel();
            self.notice("Cancelling…");
        }
    }

    fn handle(&mut self, event: &Event, control: &mut Control) {
        if let Some((dialog, _)) = &mut self.dialog {
            match dialog.handle(event) {
                DialogOutcome::Chosen { index, .. } => {
                    let (_, prompt) = self.dialog.take().expect("dialog is open");
                    if index == 1 {
                        match prompt {
                            Prompt::Remove(id) => self.start_remove(id),
                            Prompt::EnableFlatpak => self.start_flatpak_setup(),
                        }
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
                Key::Escape if self.search_job.is_some() => return self.cancel_search(),
                _ => {}
            }
        }
        if self.focus.handle(event).is_some() {
            return;
        }
        // The tab strip: clicks, Left/Right, Home/End and tab accessibility.
        match self.tab_strip.handle(event) {
            TabsOutcome::Selected(index) => {
                let source = self.tabs[index].source;
                self.focus.focus(TABS_ID);
                return self.select_tab(source);
            }
            TabsOutcome::Changed => {
                self.focus.focus(TABS_ID);
                return;
            }
            TabsOutcome::Ignored => {}
        }
        let clicked = |id: u64| matches!(event, Event::Accessibility(AccessibilityRequest::Click(target)) if *target == id);
        if let Event::Accessibility(AccessibilityRequest::Focus(id)) = event {
            if let Some(index) = self.list.row_for_node(*id) {
                self.list.select_only(index);
            }
            self.focus.focus(*id);
            return;
        }
        // The Sources tab's switches.
        if self.active == Source::Sources {
            for index in 0..self.switches.len() {
                let source = self.switches[index].0;
                let button = &mut self.switches[index].1;
                let id = button.id;
                let enabled = button.enabled;
                let pressed =
                    button.handle(event) == ButtonOutcome::Activated || (enabled && clicked(id));
                if pressed {
                    return self.toggle_source(source);
                }
            }
        }
        // The Cancel button beside the search box, while a search runs.
        if self.search_cancel.enabled
            && (self.search_cancel.handle(event) == ButtonOutcome::Activated
                || clicked(SEARCH_CANCEL_ID))
        {
            return self.cancel_search();
        }
        // The search box, on search-only tabs. Ctrl+V asks for the clipboard;
        // the text comes back as an Event::Paste that the field inserts.
        if self.active.is_backend() {
            match self.search.handle(event) {
                TextFieldOutcome::Ignored => {}
                TextFieldOutcome::Submitted => return self.start_search(),
                TextFieldOutcome::FocusRequested => {
                    self.focus.focus(SEARCH_ID);
                    return;
                }
                TextFieldOutcome::Copy(text) => return control.set_clipboard(&text),
                TextFieldOutcome::PasteRequested => return control.request_paste(),
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

    /// The Sources tab: each source's facts and its switch, block by block.
    fn paint_sources(
        &self,
        surface: &RasterSurface,
        font: &Font,
        bold: &Font,
    ) -> Result<(), RenderError> {
        let colors = palette();
        let x = 14;
        let width = self.window.0.saturating_sub(28);
        if let Some(running) = &self.running {
            let latest = self
                .output
                .last()
                .cloned()
                .unwrap_or_else(|| "starting…".into());
            let text = format!("{} Flatpak — {latest}", running.verb);
            let text = wrap(&text, font, width.saturating_sub(110))
                .into_iter()
                .next()
                .unwrap_or_default();
            surface.draw_text(&text, x, self.body_top() + 12, font, colors.ink)?;
            self.cancel.paint(surface, font)?;
        }
        for (section, block) in self.sections.iter().zip(self.panel_geometry()) {
            surface.draw_text(&section.title, x, block.heading, bold, colors.ink)?;
            for (text, y) in section.lines.iter().zip(&block.lines) {
                let text = wrap(text, font, width)
                    .into_iter()
                    .next()
                    .unwrap_or_default();
                surface.draw_text(&text, x, *y, font, colors.ink_secondary)?;
            }
            if block.button.is_some() {
                if let Some((_, button)) = self
                    .switches
                    .iter()
                    .find(|(source, _)| *source == section.source)
                {
                    button.paint(surface, font)?;
                }
            }
        }
        Ok(())
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
        // The tab strip.
        self.tab_strip.paint(surface, &font)?;
        if self.active.is_backend() {
            self.search.paint(surface)?;
        }
        if self.search_job.is_some() {
            self.search_cancel.paint(surface, &font)?;
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

        if self.active == Source::Sources {
            self.paint_sources(surface, &font, &bold)?;
        } else {
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
                    Some((_, Some(icon))) => {
                        surface.draw_image_scaled(icon, tile.x, tile.y, 32, 32)?
                    }
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
                    match self.package_command() {
                        Ok(_) => self.primary.paint(surface, &font)?,
                        Err(reason) => {
                            // Where an install button would be, say why there is none.
                            surface.draw_text(
                                NOT_BUILT,
                                d.x + 60,
                                d.y + 74,
                                &font,
                                colors.ink_secondary,
                            )?;
                            surface.draw_text(
                                &reason,
                                d.x + 60,
                                d.y + 92,
                                &font,
                                colors.ink_secondary,
                            )?;
                        }
                    }
                } else {
                    self.primary.paint(surface, &font)?;
                    self.remove.paint(surface, &font)?;
                    self.open.paint(surface, &font)?;
                }
                let mut y = d.y + 140;
                if let Some(running) = &self.running {
                    let text = if running.lines.is_some() {
                        // A package install shows the tool's latest line.
                        let latest = self
                            .output
                            .last()
                            .cloned()
                            .unwrap_or_else(|| "starting…".into());
                        format!("{} — {latest}", running.verb)
                    } else {
                        format!(
                            "{} — {}",
                            running.verb,
                            self.progress.fraction.map_or_else(
                                || "starting…".into(),
                                |f| format!("{:.0}%", f * 100.0)
                            )
                        )
                    };
                    let text = wrap(&text, &font, d.width.saturating_sub(60))
                        .into_iter()
                        .next()
                        .unwrap_or_default();
                    surface.draw_text(
                        &text,
                        d.x + 60,
                        self.progress.bounds.y - 2 - line,
                        &font,
                        colors.ink,
                    )?;
                    if running.lines.is_none() {
                        self.progress.paint(surface)?;
                    }
                    self.cancel.paint(surface, &font)?;
                    y += 10;
                }
                let mut text = if self.shows_output(&row) {
                    self.output.join("\n")
                } else if row.description.is_empty() {
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
        if self.active != Source::Sources {
            nodes.extend(self.list.accessibility_nodes(&label));
        }
        let (standing, _) = self.standing();
        nodes.push((STATE_ID, slopos_ui::accessibility::text_label(standing)));
        nodes.extend(self.tab_strip.accessibility_nodes());
        for (index, info) in self.tabs.iter().enumerate() {
            if let Availability::Unavailable(reason) = &info.availability {
                let tab_id = self.tab_strip.tab_id(index);
                if let Some((_, node)) = nodes.iter_mut().find(|(id, _)| *id == tab_id) {
                    node.set_label(format!("{}, {reason}", info.label));
                }
            }
        }
        if self.active == Source::Sources {
            for (_, button) in &self.switches {
                nodes.push((button.id, button.accessibility_node()));
            }
            let lines = self.sections.iter().flat_map(|section| {
                std::iter::once(section.title.clone()).chain(section.lines.iter().cloned())
            });
            for (index, line) in lines.enumerate() {
                nodes.push((
                    PANEL_TEXT_BASE + index as u64,
                    slopos_ui::accessibility::text_label(line),
                ));
            }
        }
        if self.active.is_backend() {
            nodes.push((SEARCH_ID, self.search.accessibility_node()));
        }
        if self.search_job.is_some() {
            nodes.push((SEARCH_CANCEL_ID, self.search_cancel.accessibility_node()));
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
                match self.package_command() {
                    Ok(_) => nodes.push((PRIMARY_ID, self.primary.accessibility_node())),
                    Err(reason) => nodes.push((
                        NOTE_ID,
                        slopos_ui::accessibility::text_label(format!("{NOT_BUILT}. {reason}")),
                    )),
                }
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
                Some(TABS_ID) => Some(self.tab_strip.focus_node()),
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

    /// What `apt-cache search zstd` prints.
    const ZSTD: &str = "zstd - fast lossless compression\nzstd-dev - development files\n";

    /// A machine with no native manager and no optional sources, so only the
    /// catalogue tab exists. Its settings live in `dir`.
    fn catalogue_only(dir: &std::path::Path) -> (Environment, Commands) {
        let environment = Environment {
            machine: sources::Machine {
                family: None,
                arch_linux: false,
            },
            config_path: dir.join("software.conf"),
            tool_present: Arc::new(|_: &str| false),
            flatpak_configs: Vec::new(),
        };
        (environment, unused_commands())
    }

    /// Commands that fail if a test reaches them by accident.
    fn unused_commands() -> Commands {
        Commands {
            search: Arc::new(|_: &Search, _: &Cancel| Err("no searches in this test".to_owned())),
            install: unused_install(),
        }
    }

    fn unused_install() -> PackageRunner {
        Arc::new(|_: &[String], _: &Cancel, _: &mut dyn FnMut(&str)| {
            Err("no installs in this test".to_owned())
        })
    }

    /// Commands whose search finds the two zstd packages.
    fn zstd_commands(install: PackageRunner) -> Commands {
        Commands {
            search: Arc::new(|_: &Search, _: &Cancel| Ok(ZSTD.to_owned())),
            install,
        }
    }

    /// The window for an apt machine whose present tools are `tools`.
    fn apt_app(
        dir: &std::path::Path,
        tools: &'static [&'static str],
        commands: Commands,
    ) -> SoftwareApp {
        let sources = Sources {
            trust: dir.join("trust.json"),
            catalogue: None,
        };
        let environment = Environment {
            machine: sources::Machine {
                family: Some(sources::Family::Apt),
                arch_linux: false,
            },
            config_path: dir.join("software.conf"),
            tool_present: Arc::new(move |name: &str| tools.contains(&name)),
            flatpak_configs: Vec::new(),
        };
        SoftwareApp::new(
            sources,
            Layout::for_home(&dir.join("home")),
            "x86_64",
            network_installer(),
            Arc::new(Mutex::new(None)),
            environment,
            commands,
        )
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

    /// Wait for a background search or package install to finish. Each wake
    /// is followed by a sync, as the event loop does.
    fn wait_for_work(app: &mut SoftwareApp) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.running.is_some() || app.search_job.is_some() {
            assert!(Instant::now() < deadline, "background work never finished");
            thread::sleep(Duration::from_millis(5));
            app.poll();
            app.sync();
        }
        app.sync();
    }

    /// Type `text` into the search box and start the search, as Enter would.
    fn search_for(app: &mut SoftwareApp, text: &str) {
        app.search.set_text(text);
        app.start_search();
        app.sync();
    }

    fn paint_everything(app: &mut SoftwareApp) {
        for factor in [1, 2] {
            let surface = RasterSurface::new(WIDTH, HEIGHT, Scale::new(factor).unwrap()).unwrap();
            app.paint(&surface);
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
        let (environment, commands) = catalogue_only(dir.path());
        let mut app = SoftwareApp::new(
            sources.clone(),
            layout.clone(),
            "x86_64",
            local_installer(payload.clone()),
            Arc::new(Mutex::new(None)),
            environment.clone(),
            commands.clone(),
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
            commands,
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
        paint_everything(&mut app);
    }

    #[test]
    fn search_tabs_list_results_and_install_only_when_the_tool_is_there() {
        let dir = tempfile::tempdir().unwrap();
        // pkexec is missing: the search works, the install is named as unavailable.
        let mut app = apt_app(dir.path(), &["apt-cache"], zstd_commands(unused_install()));
        let mut control = Control::default();
        assert_eq!(app.active, Source::Native, "the native tab comes first");
        assert!(app.rows.is_empty(), "nothing is listed before a search");

        search_for(&mut app, "zstd");
        wait_for_work(&mut app);
        assert_eq!(app.rows.len(), 2);
        assert_eq!(app.selected().unwrap().name, "zstd");
        assert!(!app.primary.enabled, "no install without pkexec");

        // Enter names the missing tool instead of installing.
        app.event(&key_event(Key::Enter), &mut control);
        assert_eq!(app.status, "unavailable: pkexec not found");
        assert!(app.status_error && app.running.is_none());
        let tree = app.accessibility().unwrap();
        assert!(tree.nodes.iter().any(|(_, n)| n
            .value()
            .is_some_and(|text| text.contains(NOT_BUILT) && text.contains("pkexec not found"))));
        paint_everything(&mut app);

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
    fn a_package_install_runs_the_tool_and_reports_its_output() {
        let dir = tempfile::tempdir().unwrap();
        let ran: Arc<Mutex<Vec<Vec<String>>>> = Arc::new(Mutex::new(Vec::new()));
        let record = ran.clone();
        let install: PackageRunner = Arc::new(
            move |argv: &[String], _: &Cancel, on_line: &mut dyn FnMut(&str)| {
                record.lock().unwrap().push(argv.to_vec());
                on_line("Reading package lists...");
                on_line("Setting up zstd");
                Ok(())
            },
        );
        let mut app = apt_app(dir.path(), &["apt-cache", "pkexec"], zstd_commands(install));
        let mut control = Control::default();
        search_for(&mut app, "zstd");
        wait_for_work(&mut app);
        assert!(
            app.primary.enabled,
            "pkexec is there, so install is offered"
        );

        app.event(&key_event(Key::Enter), &mut control);
        assert!(app.running.is_some());
        wait_for_work(&mut app);
        let expected: Vec<String> = [
            "pkexec",
            "env",
            "DEBIAN_FRONTEND=noninteractive",
            "apt-get",
            "install",
            "-y",
            "zstd",
        ]
        .map(str::to_owned)
        .to_vec();
        assert_eq!(ran.lock().unwrap().as_slice(), [expected]);
        assert!(!app.status_error);
        assert_eq!(app.status, "zstd is installed from Native (apt).");
        assert_eq!(app.output, ["Reading package lists...", "Setting up zstd"]);
        paint_everything(&mut app);
    }

    #[test]
    fn a_failed_package_install_shows_the_tools_own_error() {
        let dir = tempfile::tempdir().unwrap();
        let install: PackageRunner =
            Arc::new(|_: &[String], _: &Cancel, on_line: &mut dyn FnMut(&str)| {
                on_line("Authorization required");
                Err("pkexec exited with status 126: Not authorized".to_owned())
            });
        let mut app = apt_app(dir.path(), &["apt-cache", "pkexec"], zstd_commands(install));
        let mut control = Control::default();
        search_for(&mut app, "zstd");
        wait_for_work(&mut app);
        app.event(&key_event(Key::Enter), &mut control);
        wait_for_work(&mut app);
        assert!(app.status_error);
        assert_eq!(
            app.status,
            "zstd was not installed: pkexec exited with status 126: Not authorized"
        );
        assert_eq!(app.output, ["Authorization required"]);
        paint_everything(&mut app);
    }

    #[test]
    fn cancelling_a_package_install_kills_its_tool() {
        let dir = tempfile::tempdir().unwrap();
        // A real process stands in for a tool that runs for a long time.
        let install: PackageRunner = Arc::new(
            |_: &[String], cancel: &Cancel, on_line: &mut dyn FnMut(&str)| {
                let argv = ["sleep".to_owned(), "30".to_owned()];
                packages::run_install(&argv, cancel, on_line)
            },
        );
        let mut app = apt_app(dir.path(), &["apt-cache", "pkexec"], zstd_commands(install));
        let mut control = Control::default();
        search_for(&mut app, "zstd");
        wait_for_work(&mut app);

        let started = Instant::now();
        app.event(&key_event(Key::Enter), &mut control);
        assert!(app.running.is_some());
        paint_everything(&mut app);
        app.event(&key_event(Key::Escape), &mut control);
        wait_for_work(&mut app);
        assert_eq!(app.status, "Installing zstd was cancelled.");
        assert!(!app.status_error);
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the tool kept running"
        );
    }

    #[test]
    fn a_running_search_can_be_cancelled_by_escape_or_its_button() {
        let dir = tempfile::tempdir().unwrap();
        // The search runs `sleep` through the real runner, so cancelling must kill it.
        let (report, outcomes) = mpsc::channel::<Result<String, String>>();
        let commands = Commands {
            search: Arc::new(move |_: &Search, cancel: &Cancel| {
                let slow = Search {
                    format: Format::Apt,
                    program: "sleep".into(),
                    args: vec!["30".into()],
                };
                let result = backends::run_system(&slow, cancel);
                let _ = report.send(result.clone());
                result
            }),
            install: unused_install(),
        };
        let mut app = apt_app(dir.path(), &["apt-cache", "pkexec"], commands);
        let mut control = Control::default();

        // Escape stops it.
        search_for(&mut app, "zstd");
        assert!(app.search_job.is_some() && app.search_cancel.enabled);
        let tree = app.accessibility().unwrap();
        assert!(tree
            .nodes
            .iter()
            .any(|(_, n)| n.label() == Some("Cancel search")));
        app.event(&key_event(Key::Escape), &mut control);
        assert!(app.search_job.is_none());
        assert_eq!(app.status, "Search for zstd was cancelled.");
        let outcome = outcomes
            .recv_timeout(Duration::from_secs(10))
            .expect("the search stopped");
        assert_eq!(outcome, Err(run::CANCELLED.to_owned()));

        // The Cancel button stops it too.
        search_for(&mut app, "zstd");
        app.event(
            &Event::Accessibility(AccessibilityRequest::Click(SEARCH_CANCEL_ID)),
            &mut control,
        );
        assert!(app.search_job.is_none());
        let outcome = outcomes
            .recv_timeout(Duration::from_secs(10))
            .expect("the search stopped");
        assert_eq!(outcome, Err(run::CANCELLED.to_owned()));
        paint_everything(&mut app);
    }

    #[test]
    fn ctrl_v_in_the_search_box_pastes_the_clipboard() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = apt_app(
            dir.path(),
            &["apt-cache", "pkexec"],
            zstd_commands(unused_install()),
        );
        let mut control = Control::default();
        app.event(
            &Event::Accessibility(AccessibilityRequest::Focus(SEARCH_ID)),
            &mut control,
        );
        assert!(app.search.focused, "the search box has the keyboard");
        let ctrl_v = Event::KeyPressed {
            key: Key::Character('v'),
            modifiers: Modifiers {
                control: true,
                ..Modifiers::default()
            },
        };
        app.event(&ctrl_v, &mut control);
        assert!(control.paste_requested(), "Ctrl+V asks for the clipboard");

        let mut control = Control::default();
        app.event(&Event::Paste("zstd".into()), &mut control);
        assert_eq!(app.search.text(), "zstd");
    }

    #[test]
    fn the_catalogue_tab_is_unavailable_without_a_signed_catalogue() {
        let dir = tempfile::tempdir().unwrap();
        let (environment, commands) = catalogue_only(dir.path());
        let sources = Sources {
            trust: dir.path().join("none.json"),
            catalogue: None,
        };
        let app = SoftwareApp::new(
            sources,
            Layout::for_home(&dir.path().join("home")),
            "x86_64",
            network_installer(),
            Arc::new(Mutex::new(None)),
            environment,
            commands,
        );
        assert_eq!(app.active, Source::Slopos);
        let (standing, _) = app.standing();
        assert!(
            standing.starts_with("unavailable: no publisher key configured"),
            "{standing}"
        );
        assert!(!app.primary.enabled);

        // With a trust store but no catalogue, the narrower state is shown.
        let (catalogue, trust) = signed(9);
        let with_key = write_sources(dir.path(), &catalogue, &trust);
        let sources = Sources {
            trust: with_key.trust.clone(),
            catalogue: None,
        };
        let (environment, commands) = catalogue_only(dir.path());
        let app = SoftwareApp::new(
            sources,
            Layout::for_home(&dir.path().join("home")),
            "x86_64",
            network_installer(),
            Arc::new(Mutex::new(None)),
            environment,
            commands,
        );
        let (standing, _) = app.standing();
        assert!(
            standing.starts_with("unavailable: no signed catalogue"),
            "{standing}"
        );
    }

    #[test]
    fn rejected_catalogue_offers_nothing_and_says_why() {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::for_home(&dir.path().join("home"));
        let (catalogue, _) = signed(6);
        let (_, other_trust) = signed(7); // a different publisher key
        let sources = write_sources(dir.path(), &catalogue, &other_trust);
        let (environment, commands) = catalogue_only(dir.path());
        let app = SoftwareApp::new(
            sources,
            layout,
            "x86_64",
            network_installer(),
            Arc::new(Mutex::new(None)),
            environment,
            commands,
        );
        assert!(matches!(app.state, CatalogueState::Rejected(_)));
        assert!(app.rows.is_empty() && !app.primary.enabled);
        let surface = RasterSurface::new(WIDTH, HEIGHT, Scale::new(1).unwrap()).unwrap();
        let mut app = app;
        app.paint(&surface);
    }

    /// An install runner that notes each command. Its first step fails when
    /// `fail` is set.
    fn recording_install(ran: Arc<Mutex<Vec<Vec<String>>>>, fail: bool) -> PackageRunner {
        Arc::new(
            move |argv: &[String], _: &Cancel, on_line: &mut dyn FnMut(&str)| {
                ran.lock().unwrap().push(argv.to_vec());
                on_line("step ran");
                if fail {
                    Err("step failed".to_owned())
                } else {
                    Ok(())
                }
            },
        )
    }

    fn click(id: u64) -> Event {
        Event::Accessibility(AccessibilityRequest::Click(id))
    }

    /// Select the Sources tab by its accessibility node, as a screen reader would.
    fn open_sources(app: &mut SoftwareApp, control: &mut Control) {
        let index = app
            .tabs
            .iter()
            .position(|tab| tab.source == Source::Sources)
            .expect("the Sources tab is always there");
        app.event(&click(app.tab_strip.tab_id(index)), control);
        assert_eq!(app.active, Source::Sources);
    }

    /// Every label and value in the window's accessibility tree, joined.
    fn tree_text(app: &SoftwareApp) -> String {
        app.accessibility()
            .unwrap()
            .nodes
            .iter()
            .filter_map(|(_, node)| node.value().or_else(|| node.label()).map(str::to_owned))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn the_sources_tab_lists_the_machine_and_offers_no_aur_off_arch() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = apt_app(dir.path(), &["apt-cache"], unused_commands());
        let mut control = Control::default();
        assert_eq!(app.tabs.last().unwrap().source, Source::Sources);
        open_sources(&mut app, &mut control);
        assert!(app.rows.is_empty(), "the Sources tab has no package list");
        let text = tree_text(&app);
        assert!(text.contains("Package manager: apt"), "{text}");
        assert!(text.contains("Flatpak installed: no"), "{text}");
        assert!(text.contains("Flathub remote: not configured"), "{text}");
        assert!(text.contains("Turn on Flatpak…"), "{text}");
        assert!(!text.contains("AUR"), "AUR is only offered on Arch");
        paint_everything(&mut app);
    }

    #[test]
    fn arch_offers_aur_and_says_when_no_helper_is_there() {
        let dir = tempfile::tempdir().unwrap();
        let sources = Sources {
            trust: dir.path().join("trust.json"),
            catalogue: None,
        };
        let environment = Environment {
            machine: sources::Machine {
                family: Some(sources::Family::Pacman),
                arch_linux: true,
            },
            config_path: dir.path().join("software.conf"),
            tool_present: Arc::new(|name: &str| name == "pacman"),
            flatpak_configs: Vec::new(),
        };
        let mut app = SoftwareApp::new(
            sources,
            Layout::for_home(&dir.path().join("home")),
            "x86_64",
            network_installer(),
            Arc::new(Mutex::new(None)),
            environment,
            unused_commands(),
        );
        let mut control = Control::default();
        open_sources(&mut app, &mut control);
        let text = tree_text(&app);
        assert!(text.contains("AUR (Arch User Repository)"), "{text}");
        assert!(
            text.contains("AUR helper: unavailable (yay or paru not found)"),
            "{text}"
        );
        assert!(text.contains("Turn on AUR"), "{text}");
    }

    #[test]
    fn arrow_keys_move_between_the_tabs_from_the_strip() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = apt_app(dir.path(), &["apt-cache"], unused_commands());
        let mut control = Control::default();
        assert_eq!(app.active, Source::Native);
        app.focus.focus(TABS_ID);
        app.sync();
        app.event(&key_event(Key::Right), &mut control);
        assert_eq!(app.active, Source::Slopos);
        app.event(&key_event(Key::End), &mut control);
        assert_eq!(app.active, Source::Sources);
        assert_eq!(app.focus.focused(), Some(TABS_ID));
    }

    #[test]
    fn turning_flatpak_on_asks_and_no_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let ran = Arc::new(Mutex::new(Vec::new()));
        let commands = zstd_commands(recording_install(ran.clone(), false));
        let mut app = apt_app(dir.path(), &["apt-cache", "pkexec"], commands);
        let mut control = Control::default();
        open_sources(&mut app, &mut control);
        app.event(&click(FLATPAK_SWITCH_ID), &mut control);
        let (title, prompt) = {
            let (dialog, prompt) = app.dialog.as_ref().expect("a Yes/No question opens");
            (dialog.title.clone(), prompt.clone())
        };
        assert_eq!(title, "Turn on Flatpak?");
        assert_eq!(prompt, Prompt::EnableFlatpak);
        app.event(&key_event(Key::Enter), &mut control); // the default is No
        assert!(app.dialog.is_none());
        assert!(!app.config.flatpak);
        assert!(ran.lock().unwrap().is_empty(), "No runs nothing");
        let saved = SoftwareConfig::load(&dir.path().join("software.conf")).unwrap();
        assert!(!saved.flatpak);
    }

    #[test]
    fn yes_installs_flatpak_then_adds_flathub_and_turns_it_on() {
        let dir = tempfile::tempdir().unwrap();
        let ran = Arc::new(Mutex::new(Vec::new()));
        let commands = zstd_commands(recording_install(ran.clone(), false));
        let mut app = apt_app(dir.path(), &["apt-cache", "pkexec"], commands);
        let mut control = Control::default();
        open_sources(&mut app, &mut control);
        app.event(&click(FLATPAK_SWITCH_ID), &mut control);
        app.event(&key_event(Key::Tab), &mut control);
        app.event(&key_event(Key::Enter), &mut control); // Yes
        assert!(app.dialog.is_none());
        wait_for_work(&mut app);
        assert!(!app.status_error, "{}", app.status);
        let steps = ran.lock().unwrap().clone();
        assert_eq!(steps.len(), 2, "install flatpak, then add flathub");
        assert_eq!(steps[0][0], "pkexec");
        assert_eq!(steps[0].last().map(String::as_str), Some("flatpak"));
        assert_eq!(steps[1][..3], ["pkexec", "flatpak", "remote-add"]);
        assert!(steps[1].contains(&crate::packages::FLATHUB_URL.to_owned()));
        assert!(app.config.flatpak);
        let saved = SoftwareConfig::load(&dir.path().join("software.conf")).unwrap();
        assert!(saved.flatpak);
        assert!(app.tabs.iter().any(|tab| tab.source == Source::Flatpak));
        paint_everything(&mut app);
    }

    #[test]
    fn a_failed_flatpak_setup_turns_nothing_on_and_stops_at_the_failure() {
        let dir = tempfile::tempdir().unwrap();
        let ran = Arc::new(Mutex::new(Vec::new()));
        let commands = zstd_commands(recording_install(ran.clone(), true));
        let mut app = apt_app(dir.path(), &["apt-cache", "pkexec"], commands);
        let mut control = Control::default();
        open_sources(&mut app, &mut control);
        app.event(&click(FLATPAK_SWITCH_ID), &mut control);
        app.event(&key_event(Key::Tab), &mut control);
        app.event(&key_event(Key::Enter), &mut control);
        wait_for_work(&mut app);
        assert!(app.status_error && app.status.starts_with("Flatpak was not set up"));
        assert!(!app.config.flatpak);
        assert_eq!(
            ran.lock().unwrap().len(),
            1,
            "nothing runs after a failed step"
        );
    }

    #[test]
    fn without_a_manager_flatpak_says_why_and_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let ran = Arc::new(Mutex::new(Vec::new()));
        let commands = zstd_commands(recording_install(ran.clone(), false));
        // apt is the OS's manager, but apt-cache is not installed.
        let mut app = apt_app(dir.path(), &["pkexec"], commands);
        let mut control = Control::default();
        open_sources(&mut app, &mut control);
        app.event(&click(FLATPAK_SWITCH_ID), &mut control);
        assert!(app.dialog.is_some(), "the question is still asked");
        app.event(&key_event(Key::Tab), &mut control);
        app.event(&key_event(Key::Enter), &mut control);
        assert!(app.status_error);
        assert!(
            app.status.starts_with("unavailable: no apt, dnf or pacman"),
            "{}",
            app.status
        );
        assert!(!app.config.flatpak);
        assert!(ran.lock().unwrap().is_empty());
    }
}
