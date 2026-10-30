//! The Software window (STORE-02): the verified catalogue and installed
//! AppImages on the left, the chosen app's details and actions on the right.
//!
//! Installs, updates and removals run one at a time on a worker thread
//! through the STORE-01 transaction code; the window shows real byte
//! progress, can cancel a download, and reports the transaction's own
//! result. Removal asks first. Nothing is offered from a catalogue that did
//! not verify.

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
use slopos_ui::event::{AccessibilityRequest, Event, Key};
use slopos_ui::foundation::geometry::Rect;
use slopos_ui::icons::IconLookup;
use slopos_ui::image::Image;
use slopos_ui::platform::{App, Control, Proxy};
use slopos_ui::primitives::button_plate::ButtonPlateVariant;
use slopos_ui::render::{RasterSurface, RenderError};
use slopos_ui::text::{measure, Font};
use slopos_ui::theme::palette;

use crate::install::{self, Layout, Progress};
use crate::store::{self, Action, CatalogueState, Row, Sources};
use crate::trust::CatalogueEntry;

pub const WIDTH: u32 = 760;
pub const HEIGHT: u32 = 500;
const LIST_WIDTH: u32 = 300;
const HEADER_HEIGHT: i32 = 50;
const STATUS_HEIGHT: u32 = 24;
const ROW_HEIGHT: u32 = 44;

const LIST_ID: u64 = 10;
const PRIMARY_ID: u64 = 20;
const REMOVE_ID: u64 = 21;
const CANCEL_ID: u64 = 22;
const OPEN_ID: u64 = 23;
const PROGRESS_ID: u64 = 24;
const DETAILS_ID: u64 = 25;
const STATE_ID: u64 = 26;
const STATUS_ID: u64 = 27;
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

pub struct SoftwareApp {
    window: (u32, u32),
    sources: Sources,
    layout: Layout,
    architecture: String,
    installer: Installer,
    wake: WakeSlot,
    state: CatalogueState,
    rows: Vec<Row>,
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
    pub fn new(
        sources: Sources,
        layout: Layout,
        architecture: &str,
        installer: Installer,
        wake: WakeSlot,
    ) -> Self {
        let repaired = install::recover(&layout);
        let mut app = Self {
            window: (WIDTH, HEIGHT),
            sources,
            layout,
            architecture: architecture.to_owned(),
            installer,
            wake,
            state: CatalogueState::Missing,
            rows: Vec::new(),
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
        app.list.focused = true;
        app.reload();
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

    /// Re-read the catalogue and installs, keeping the selected app.
    fn reload(&mut self) {
        let keep = self.selected().map(|row| row.id.clone());
        let (state, rows) = store::load(&self.sources, &self.layout, &self.architecture);
        self.state = state;
        self.rows = rows;
        self.list.reset(self.rows.len());
        let index = keep
            .and_then(|id| self.rows.iter().position(|row| row.id == id))
            .or((!self.rows.is_empty()).then_some(0));
        if let Some(index) = index {
            self.list.select_only(index);
        }
    }

    fn selected(&self) -> Option<&Row> {
        self.list.cursor().and_then(|index| self.rows.get(index))
    }

    fn top(&self) -> i32 {
        self.menu.height() as i32
    }

    fn detail(&self) -> Rect {
        let top = self.top() + HEADER_HEIGHT;
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
        let list_top = top + HEADER_HEIGHT;
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
        let selected = self.selected().cloned();
        let action = selected.as_ref().map(Row::action);
        self.primary.label = match action {
            Some(Action::Update) => "Update".into(),
            _ => "Install".into(),
        };
        self.primary.enabled = !busy && matches!(action, Some(Action::Install | Action::Update));
        self.remove.enabled = !busy && selected.as_ref().is_some_and(Row::is_installed);
        self.open.enabled = selected.as_ref().is_some_and(Row::is_installed)
            && selected
                .as_ref()
                .is_some_and(|row| self.layout.is_installed(&row.id));
        self.cancel.enabled = busy;
        self.progress.fraction = self.running.as_ref().and_then(|r| r.progress.fraction());
        let mut ids = vec![LIST_ID];
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

    fn start_install(&mut self) {
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

    /// Collect a finished transaction. Returns whether anything changed.
    fn poll(&mut self) -> bool {
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
        let names: Vec<String> = self.rows.iter().map(|row| row.name.clone()).collect();
        let label = |index: usize| names.get(index).cloned().unwrap_or_default();
        match self.list.handle(event, control.modifiers(), &label) {
            ListOutcome::FocusRequested => {
                self.focus.focus(LIST_ID);
            }
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

        // Header: title and the catalogue's standing.
        let top = self.top();
        surface.draw_text("Software", 14, top + 8, &title_font, colors.ink)?;
        let state_ink = match self.state {
            CatalogueState::Rejected(_) => colors.danger,
            _ => colors.ink_secondary,
        };
        let state = self.state.describe();
        let state_line = wrap(&state, &font, width.saturating_sub(28))
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
            if row.is_installed() {
                let mark = match row.action() {
                    Action::Update => "Update",
                    _ => "Installed",
                };
                let mark_width = measure(mark, &font).width as i32;
                surface.draw_text(
                    mark,
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
                y: top + HEADER_HEIGHT,
                width: 1,
                height: height.saturating_sub(top as u32 + HEADER_HEIGHT as u32 + STATUS_HEIGHT),
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
            let publisher = match (&row.entry, row.publisher.is_empty()) {
                (Some(entry), _) => format!("Signed by {} (key {})", entry.publisher, entry.key_id),
                (None, false) => format!("From {} · not in the current catalogue", row.publisher),
                (None, true) => "Not in the current catalogue".into(),
            };
            surface.draw_text(&publisher, d.x + 60, d.y + 44, &font, colors.ink_secondary)?;
            self.primary.paint(surface, &font)?;
            self.remove.paint(surface, &font)?;
            self.open.paint(surface, &font)?;
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
            let message = match self.state {
                CatalogueState::Verified { .. } => "No apps are offered for this computer.",
                _ => "No apps to show.",
            };
            surface.draw_text(message, d.x, d.y + 4, &font, colors.ink_secondary)?;
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
        let mut state = accesskit::Node::new(accesskit::Role::Label);
        state.set_label(self.state.describe());
        nodes.push((STATE_ID, state));
        if let Some(row) = self.selected() {
            let mut details = accesskit::Node::new(accesskit::Role::Label);
            details.set_label(format!(
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
            nodes.push((PRIMARY_ID, self.primary.accessibility_node()));
            nodes.push((REMOVE_ID, self.remove.accessibility_node()));
            nodes.push((OPEN_ID, self.open.accessibility_node()));
        }
        if self.running.is_some() {
            nodes.push((PROGRESS_ID, self.progress.accessibility_node()));
            nodes.push((CANCEL_ID, self.cancel.accessibility_node()));
        }
        let mut status = accesskit::Node::new(accesskit::Role::Label);
        status.set_label(self.status.as_str());
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
    use crate::store::tests::write_sources;
    use crate::trust::signing::sign_catalogue;
    use crate::trust::tests::{key, signed};
    use sha2::{Digest, Sha256};
    use slopos_ui::event::Modifiers;
    use slopos_ui::foundation::geometry::Scale;
    use std::io::Cursor;
    use std::time::{Duration, Instant};

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
        let mut app = SoftwareApp::new(
            sources.clone(),
            layout.clone(),
            "x86_64",
            local_installer(payload.clone()),
            Arc::new(Mutex::new(None)),
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
    fn rejected_catalogue_offers_nothing_and_says_why() {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::for_home(&dir.path().join("home"));
        let (catalogue, _) = signed(6);
        let (_, other_trust) = signed(7); // a different publisher key
        let sources = write_sources(dir.path(), &catalogue, &other_trust);
        let app = SoftwareApp::new(
            sources,
            layout,
            "x86_64",
            network_installer(),
            Arc::new(Mutex::new(None)),
        );
        assert!(matches!(app.state, CatalogueState::Rejected(_)));
        assert!(app.rows.is_empty() && !app.primary.enabled);
        let surface = RasterSurface::new(WIDTH, HEIGHT, Scale::new(1).unwrap()).unwrap();
        let mut app = app;
        app.paint(&surface);
    }
}
