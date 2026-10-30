//! SLOPOS desktop: the full-screen window under every other window that
//! shows the desktop picture and icons for Home, mounted volumes, the
//! items in `~/Desktop` and the Trash.
//!
//! Click selects (Ctrl adds), dragging moves an icon to another cell and
//! remembers it, double-click or Return opens, arrows move the selection,
//! typing a letter jumps to the next name starting with it, and a
//! secondary click opens a menu. The picture is a copy of the root
//! window's background, which the session keeps painting, so the desktop
//! looks the same with or without this window.

mod model;
mod watch;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};

use model::{Direction, Grid, Item, Kind};
use slopos_core::process::launch;
use slopos_ui::accessibility::window_tree;
use slopos_ui::controls::menu::{Menu, MenuItem, MenuOutcome};
use slopos_ui::event::{AccessibilityRequest, Event, Key, PointerButton};
use slopos_ui::foundation::color::Color;
use slopos_ui::foundation::geometry::Rect;
use slopos_ui::image::Image;
use slopos_ui::platform::{self, App, Control, Proxy, WindowKind, WindowOptions};
use slopos_ui::render::{RasterSurface, RenderError};
use slopos_ui::text::{measure, Font};
use slopos_ui::theme::palette;
use watch::Change;

const LIST_ID: u64 = 2;
const MENU_ID: u64 = 3;
/// Item nodes are `ITEM_BASE + index`.
const ITEM_BASE: u64 = 1000;
const ICON: u32 = 48;
/// Shown until the session has painted the root picture.
const FALLBACK_BACKDROP: Color = Color {
    red: 0x5a,
    green: 0x6e,
    blue: 0x9a,
    alpha: 0xff,
};
/// A press must move this far (logical pixels) to become a drag.
const DRAG_SLOP: i32 = 4;

/// Menu entries, by what they do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Open,
    OpenDesktopFolder,
    NewFolder,
    ChangePicture,
}

struct Drag {
    index: usize,
    from: (i32, i32),
    at: (i32, i32),
    moving: bool,
}

pub struct Desktop {
    /// Logical size of the screen.
    size: (u32, u32),
    background: Option<Image>,
    home: PathBuf,
    folder: PathBuf,
    trash: PathBuf,
    positions_file: Option<PathBuf>,
    items: Vec<Item>,
    cells: Vec<(u32, u32)>,
    positions: HashMap<String, (u32, u32)>,
    selected: HashSet<usize>,
    focused: Option<usize>,
    menu: Menu,
    menu_actions: Vec<Action>,
    drag: Option<Drag>,
    icons: HashMap<(String, u32), Option<Image>>,
    lookup: Option<slopos_ui::icons::IconLookup>,
    changes: Option<Receiver<Change>>,
    font: Font,
}

impl Desktop {
    fn new(home: PathBuf, config: Option<PathBuf>, changes: Option<Receiver<Change>>) -> Self {
        let folder = desktop_folder(&home);
        let trash = data_home(&home).join("Trash/files");
        let positions_file = config.map(|dir| dir.join("desktop-positions"));
        let positions = positions_file
            .as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|text| model::load_positions(&text))
            .unwrap_or_default();
        let mut desktop = Self {
            size: (1024, 768),
            background: None,
            home,
            folder,
            trash,
            positions_file,
            items: Vec::new(),
            cells: Vec::new(),
            positions,
            selected: HashSet::new(),
            focused: None,
            menu: Menu::new(MENU_ID, Vec::new()),
            menu_actions: Vec::new(),
            drag: None,
            icons: HashMap::new(),
            lookup: None,
            changes,
            font: Font::system(12.0),
        };
        desktop.reload();
        desktop
    }

    fn grid(&self) -> Grid {
        Grid::new(self.size)
    }

    /// Read the items again, keeping the selection by identity.
    fn reload(&mut self) {
        let selected: HashSet<String> = self
            .selected
            .iter()
            .filter_map(|&i| self.items.get(i).map(|item| item.key.clone()))
            .collect();
        let focused = self
            .focused
            .and_then(|i| self.items.get(i))
            .map(|item| item.key.clone());
        let entries: Vec<(String, bool)> = std::fs::read_dir(&self.folder)
            .map(|dir| {
                dir.flatten()
                    .take(model::MAX_ITEMS * 2)
                    .map(|entry| {
                        let is_dir = entry.path().is_dir();
                        (entry.file_name().to_string_lossy().into_owned(), is_dir)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mounts = std::fs::read_to_string("/proc/self/mountinfo")
            .map(|text| model::parse_mounts(&text))
            .unwrap_or_default();
        let trash_full = std::fs::read_dir(&self.trash)
            .map(|mut dir| dir.next().is_some())
            .unwrap_or(false);
        self.items = model::items(&self.home, &self.folder, &mounts, &entries, trash_full);
        self.cells = model::arrange(&self.items, &self.positions, self.grid());
        self.selected = (0..self.items.len())
            .filter(|&i| selected.contains(&self.items[i].key))
            .collect();
        self.focused = focused.and_then(|key| self.items.iter().position(|i| i.key == key));
    }

    /// The item drawn under a point.
    fn item_at(&self, x: i32, y: i32) -> Option<usize> {
        let cell = self.grid().cell_at(x, y)?;
        // The last one drawn is on top.
        self.cells.iter().rposition(|&c| c == cell)
    }

    fn select_only(&mut self, index: usize) {
        self.selected.clear();
        self.selected.insert(index);
        self.focused = Some(index);
    }

    fn open(&mut self, index: usize) {
        let Some(item) = self.items.get(index) else {
            return;
        };
        let path = item.path.to_string_lossy().into_owned();
        let result = match item.kind {
            Kind::Home | Kind::Folder | Kind::Volume => {
                launch("slopos-files", &[&path]).map(|_| ())
            }
            Kind::Trash => launch("slopos-files", &["--trash"]).map(|_| ()),
            Kind::File => slopos_core::mime::open(&item.path)
                .map(|_| ())
                .ok_or_else(|| format!("no application opens {}", item.label)),
        };
        if let Err(error) = result {
            log::warn!("desktop: can't open {}: {error}", item.label);
        }
    }

    fn open_selection(&mut self) {
        let mut selected: Vec<usize> = self.selected.iter().copied().collect();
        selected.sort_unstable();
        for index in selected.into_iter().take(16) {
            self.open(index);
        }
    }

    fn new_folder(&mut self) {
        let _ = std::fs::create_dir_all(&self.folder);
        let name = model::new_folder_name(|name| self.folder.join(name).exists());
        // create_dir never replaces: a race with another program just fails.
        match std::fs::create_dir(self.folder.join(&name)) {
            Ok(()) => {
                self.reload();
                if let Some(index) = self.items.iter().position(|i| i.label == name) {
                    self.select_only(index);
                }
            }
            Err(error) => log::warn!("desktop: can't create {name}: {error}"),
        }
    }

    fn save_positions(&self) {
        if let Some(path) = &self.positions_file {
            let text = model::save_positions(&self.positions);
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let partial = path.with_extension("partial");
            if std::fs::write(&partial, text)
                .and_then(|()| std::fs::rename(&partial, path))
                .is_err()
            {
                log::warn!("desktop: can't remember icon positions");
            }
        }
    }

    /// Move an item to the cell under a point, if it is free.
    fn drop_at(&mut self, index: usize, x: i32, y: i32) {
        let Some(cell) = self.grid().cell_at(x, y) else {
            return;
        };
        if self
            .cells
            .iter()
            .enumerate()
            .any(|(i, &c)| c == cell && i != index)
        {
            return;
        }
        self.cells[index] = cell;
        // Remember every placed item so the others stay where they are.
        for (item, &cell) in self.items.iter().zip(&self.cells) {
            self.positions.insert(item.key.clone(), cell);
        }
        self.save_positions();
    }

    fn open_menu(&mut self, x: i32, y: i32, on: Option<usize>) {
        let (labels, actions): (Vec<&str>, Vec<Action>) = match on {
            Some(_) => (vec!["Open"], vec![Action::Open]),
            None => (
                vec![
                    "New Folder",
                    "Change Desktop Picture…",
                    "Open Desktop in Files",
                ],
                vec![
                    Action::NewFolder,
                    Action::ChangePicture,
                    Action::OpenDesktopFolder,
                ],
            ),
        };
        self.menu.items = labels.into_iter().map(MenuItem::new).collect();
        self.menu_actions = actions;
        self.menu.open_at(x, y, self.size);
    }

    fn act(&mut self, action: Action) {
        match action {
            Action::Open => self.open_selection(),
            Action::NewFolder => self.new_folder(),
            Action::ChangePicture => {
                if let Err(error) = launch("slopos-control-panels", &["--desktop"]) {
                    log::warn!("desktop: {error}");
                }
            }
            Action::OpenDesktopFolder => {
                let _ = std::fs::create_dir_all(&self.folder);
                let folder = self.folder.to_string_lossy().into_owned();
                if let Err(error) = launch("slopos-files", &[&folder]) {
                    log::warn!("desktop: {error}");
                }
            }
        }
    }

    fn apply(&mut self, change: Change, control: &mut Control) {
        match change {
            Change::Background { size, picture } => {
                let factor = control.scale().map_or(1, |scale| scale.factor());
                let logical = ((size.0 / factor).max(1), (size.1 / factor).max(1));
                if logical != self.size {
                    self.size = logical;
                    control.set_geometry(Rect {
                        x: 0,
                        y: 0,
                        width: logical.0,
                        height: logical.1,
                    });
                    self.cells = model::arrange(&self.items, &self.positions, self.grid());
                }
                self.background = picture;
            }
            Change::Items => self.reload(),
        }
    }

    fn icon(&mut self, names: &[String], factor: u32) -> Option<Image> {
        let key = (names.join("|"), factor);
        if let Some(image) = self.icons.get(&key) {
            return image.clone();
        }
        let lookup = self
            .lookup
            .get_or_insert_with(slopos_ui::icons::IconLookup::from_environment);
        let image = names
            .iter()
            .find_map(|name| lookup.load(name, ICON, factor));
        // Icons are few and small; a bounded cache keeps memory flat.
        if self.icons.len() > 64 {
            self.icons.clear();
        }
        self.icons.insert(key, image.clone());
        image
    }

    fn handle(&mut self, event: &Event) {
        if self.menu.is_open() {
            if let MenuOutcome::Chosen(index) = self.menu.handle(event) {
                if let Some(&action) = self.menu_actions.get(index) {
                    self.act(action);
                }
            }
            return;
        }
        let ctrl = |modifiers: &slopos_ui::event::Modifiers| modifiers.control;
        match event {
            Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
            } => match self.item_at(*x, *y) {
                Some(index) => {
                    self.drag = Some(Drag {
                        index,
                        from: (*x, *y),
                        at: (*x, *y),
                        moving: false,
                    });
                    if !self.selected.contains(&index) {
                        self.select_only(index);
                    }
                    self.focused = Some(index);
                }
                None => {
                    self.selected.clear();
                    self.focused = None;
                }
            },
            Event::PointerMoved { x, y } => {
                if let Some(drag) = &mut self.drag {
                    drag.at = (*x, *y);
                    if (x - drag.from.0).abs() > DRAG_SLOP || (y - drag.from.1).abs() > DRAG_SLOP {
                        drag.moving = true;
                    }
                }
            }
            Event::PointerReleased {
                x,
                y,
                button: PointerButton::Primary,
            } => {
                if let Some(drag) = self.drag.take() {
                    if drag.moving {
                        self.drop_at(drag.index, *x, *y);
                    }
                }
            }
            Event::PointerPressed {
                x,
                y,
                button: PointerButton::Secondary,
            } => {
                let on = self.item_at(*x, *y);
                if let Some(index) = on {
                    if !self.selected.contains(&index) {
                        self.select_only(index);
                    }
                }
                self.open_menu(*x, *y, on);
            }
            Event::DoubleClicked { x, y } => {
                self.drag = None;
                if let Some(index) = self.item_at(*x, *y) {
                    self.select_only(index);
                    self.open(index);
                }
            }
            Event::KeyPressed { key, modifiers } => match key {
                Key::Enter => self.open_selection(),
                Key::Escape => self.selected.clear(),
                Key::Up | Key::Down | Key::Left | Key::Right => {
                    let direction = match key {
                        Key::Up => Direction::Up,
                        Key::Down => Direction::Down,
                        Key::Left => Direction::Left,
                        _ => Direction::Right,
                    };
                    let next = match self.focused {
                        Some(from) => model::neighbour(&self.cells, from, direction),
                        None => (!self.items.is_empty()).then_some(0),
                    };
                    if let Some(index) = next {
                        self.select_only(index);
                    }
                }
                Key::Character('a' | 'A') if ctrl(modifiers) => {
                    self.selected = (0..self.items.len()).collect();
                }
                Key::Character('n' | 'N') if ctrl(modifiers) && modifiers.shift => {
                    self.new_folder();
                }
                Key::Character('o' | 'O') if ctrl(modifiers) => self.open_selection(),
                Key::Character(c) if !ctrl(modifiers) && !c.is_control() => {
                    let found = model::next_with_prefix(&self.items, self.focused, &c.to_string());
                    if let Some(index) = found {
                        self.select_only(index);
                    }
                }
                _ => {}
            },
            Event::Accessibility(AccessibilityRequest::Click(node)) => {
                if let Some(index) = node.checked_sub(ITEM_BASE).map(|i| i as usize) {
                    if index < self.items.len() {
                        self.select_only(index);
                        self.open(index);
                    }
                }
            }
            Event::Accessibility(AccessibilityRequest::Focus(node)) => {
                if let Some(index) = node.checked_sub(ITEM_BASE).map(|i| i as usize) {
                    if index < self.items.len() {
                        self.select_only(index);
                    }
                }
            }
            _ => {}
        }
    }

    fn paint_all(&mut self, surface: &RasterSurface) -> Result<(), RenderError> {
        let colors = palette();
        let (width, height) = (surface.logical_width(), surface.logical_height());
        match &self.background {
            Some(picture) => surface.draw_image_scaled(
                picture,
                0,
                0,
                picture.width() / surface.scale().factor(),
                picture.height() / surface.scale().factor(),
            )?,
            None => surface.fill_rect(
                Rect {
                    x: 0,
                    y: 0,
                    width,
                    height,
                },
                FALLBACK_BACKDROP,
            )?,
        }
        let grid = self.grid();
        let factor = surface.scale().factor();
        let line = measure("Ag", &self.font).height as i32;
        let dragged = self
            .drag
            .as_ref()
            .filter(|d| d.moving)
            .map(|d| (d.index, (d.at.0 - d.from.0, d.at.1 - d.from.1)));
        for index in 0..self.items.len() {
            let (cx, cy, cw, _) = grid.rect(self.cells[index]);
            let (dx, dy) = match dragged {
                Some((i, offset)) if i == index => offset,
                _ => (0, 0),
            };
            let (x, y) = (cx + dx, cy + dy);
            let selected = self.selected.contains(&index);
            let names = self.items[index].icons.clone();
            let icon_x = x + (cw as i32 - ICON as i32) / 2;
            if selected {
                // A soft plate behind a selected icon.
                let mut plate = Color::BLACK;
                plate.alpha = 0x40;
                surface.fill_rect(
                    Rect {
                        x: icon_x - 4,
                        y,
                        width: ICON + 8,
                        height: ICON + 8,
                    },
                    plate,
                )?;
            }
            match self.icon(&names, factor) {
                Some(icon) => surface.draw_image_scaled(&icon, icon_x, y + 4, ICON, ICON)?,
                None => surface.fill_rect(
                    Rect {
                        x: icon_x + 8,
                        y: y + 8,
                        width: ICON - 16,
                        height: ICON - 12,
                    },
                    colors.bevel_shadow,
                )?,
            }
            let font = &self.font;
            let lines = model::label_lines(&self.items[index].label, |text| {
                measure(text, font).width <= (cw - 8) as f32
            });
            for (row, text) in lines.iter().enumerate() {
                let text_width = measure(text, font).width.ceil() as i32;
                let tx = x + (cw as i32 - text_width) / 2;
                let ty = y + ICON as i32 + 8 + row as i32 * (line + 1);
                let (back, ink) = if selected {
                    (colors.menu_highlight, colors.menu_highlight_ink)
                } else {
                    let mut back = colors.content;
                    back.alpha = 0xd8;
                    (back, colors.ink)
                };
                surface.fill_rect(
                    Rect {
                        x: tx - 3,
                        y: ty - 1,
                        width: text_width as u32 + 6,
                        height: line as u32 + 2,
                    },
                    back,
                )?;
                surface.draw_text(text, tx, ty, font, ink)?;
            }
            if self.focused == Some(index) && !selected {
                slopos_ui::controls::button::paint_focus_ring(
                    surface,
                    Rect {
                        x: icon_x - 2,
                        y: y + 2,
                        width: ICON + 4,
                        height: ICON + 4,
                    },
                )?;
            }
        }
        self.menu.paint(surface)
    }
}

impl App for Desktop {
    fn event(&mut self, event: &Event, control: &mut Control) {
        match event {
            // The desktop is never closed from the window manager.
            Event::CloseRequested => return,
            Event::Resized { width, height } => {
                self.size = (*width, *height);
                self.cells = model::arrange(&self.items, &self.positions, self.grid());
            }
            Event::Wake => {
                let changes: Vec<Change> = self
                    .changes
                    .as_ref()
                    .map(|receiver| receiver.try_iter().collect())
                    .unwrap_or_default();
                for change in changes {
                    self.apply(change, control);
                }
            }
            _ => self.handle(event),
        }
        control.request_redraw();
    }

    fn paint(&mut self, surface: &RasterSurface) {
        if let Err(error) = self.paint_all(surface) {
            log::warn!("desktop: paint failed: {error}");
        }
    }

    fn accessibility(&self) -> Option<accesskit::TreeUpdate> {
        let grid = self.grid();
        let mut nodes = Vec::new();
        let mut list = accesskit::Node::new(accesskit::Role::List);
        list.set_label("Desktop");
        list.set_children(
            (0..self.items.len())
                .map(|i| accesskit::NodeId(ITEM_BASE + i as u64))
                .collect::<Vec<_>>(),
        );
        nodes.push((LIST_ID, list));
        for (index, item) in self.items.iter().enumerate() {
            let (x, y, w, h) = grid.rect(self.cells[index]);
            let mut node = accesskit::Node::new(accesskit::Role::ListItem);
            node.set_label(item.label.as_str());
            node.set_selected(self.selected.contains(&index));
            node.set_bounds(accesskit::Rect {
                x0: f64::from(x),
                y0: f64::from(y),
                x1: f64::from(x) + f64::from(w),
                y1: f64::from(y) + f64::from(h),
            });
            node.add_action(accesskit::Action::Click);
            node.add_action(accesskit::Action::Focus);
            nodes.push((ITEM_BASE + index as u64, node));
        }
        nodes.extend(self.menu.accessibility_nodes());
        let focus = if self.menu.is_open() {
            Some(self.menu.focus_node())
        } else {
            self.focused.map(|i| ITEM_BASE + i as u64).or(Some(LIST_ID))
        };
        Some(window_tree("Desktop", nodes, focus))
    }
}

/// `XDG_DESKTOP_DIR` from user-dirs.dirs, else `~/Desktop`.
fn desktop_folder(home: &Path) -> PathBuf {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".config"));
    std::fs::read_to_string(config.join("user-dirs.dirs"))
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                let value = line.trim().strip_prefix("XDG_DESKTOP_DIR=")?;
                let value = value.trim_matches('"');
                let path = match value.strip_prefix("$HOME") {
                    Some(rest) => home.join(rest.trim_start_matches('/')),
                    None => PathBuf::from(value),
                };
                (path.is_absolute() && path != home).then_some(path)
            })
        })
        .unwrap_or_else(|| home.join("Desktop"))
}

fn data_home(home: &Path) -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".local/share"))
}

type WakeSlot = Arc<Mutex<Option<Proxy>>>;

fn main() {
    if std::env::var_os("RUST_LOG").is_some() {
        let _ = env_logger::try_init();
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"));
    let config = slopos_core::wallpaper::config_dir();
    let slot: WakeSlot = Arc::new(Mutex::new(None));
    let wake_slot = slot.clone();
    let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        if let Ok(guard) = wake_slot.lock() {
            if let Some(proxy) = guard.as_ref() {
                proxy.wake();
            }
        }
    });
    let (sender, receiver) = mpsc::channel();
    let app = Desktop::new(home, config, Some(receiver));
    watch::watch_items(
        app.folder.clone(),
        app.trash.clone(),
        sender.clone(),
        wake.clone(),
    );
    if watch::watch_root(sender, wake).is_none() {
        log::warn!("desktop: can't follow the screen size or desktop picture");
    }
    let (width, height) = x11rb::connect(None)
        .ok()
        .map(|(conn, screen)| {
            let root = &x11rb::connection::Connection::setup(&conn).roots[screen];
            (
                u32::from(root.width_in_pixels),
                u32::from(root.height_in_pixels),
            )
        })
        .unwrap_or((1024, 768));
    let scale = platform::scale_from_environment();
    let mut options = WindowOptions::new(
        "Desktop",
        "slopos-desktop",
        (width / scale.factor()).max(1),
        (height / scale.factor()).max(1),
    );
    options.kind = WindowKind::Desktop;
    options.resizable = false;
    options.position = Some((0, 0));
    if let Err(error) = platform::run(options, app, move |proxy| {
        if let Ok(mut guard) = slot.lock() {
            *guard = Some(proxy.clone());
        }
        proxy.wake();
    }) {
        eprintln!("slopos-desktop: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slopos_ui::event::Modifiers;
    use slopos_ui::foundation::geometry::Scale;

    fn key(key: Key) -> Event {
        Event::KeyPressed {
            key,
            modifiers: Modifiers::default(),
        }
    }

    fn desktop(dir: &Path) -> Desktop {
        std::fs::create_dir_all(dir.join("Desktop")).unwrap();
        std::fs::write(dir.join("Desktop/notes.txt"), "x").unwrap();
        std::fs::create_dir(dir.join("Desktop/Projects")).unwrap();
        std::env::set_var("XDG_DATA_HOME", dir.join("data"));
        std::env::set_var("XDG_CONFIG_HOME", dir.join("config"));
        let mut desktop = Desktop::new(dir.to_owned(), Some(dir.join("config/slopos-i")), None);
        desktop.size = (800, 600);
        desktop.cells = model::arrange(&desktop.items, &desktop.positions, desktop.grid());
        desktop
    }

    #[test]
    fn icons_are_selected_moved_and_remembered() {
        let dir = tempfile::tempdir().unwrap();
        let mut desktop = desktop(dir.path());
        let labels: Vec<&str> = desktop.items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            labels.starts_with(&["Home"]) && labels.ends_with(&["notes.txt", "Projects", "Trash"])
        );
        let mut control = Control::default();
        // Arrows from nothing select the first item, then walk the column.
        desktop.event(&key(Key::Down), &mut control);
        assert_eq!(desktop.focused, Some(0));
        desktop.event(&key(Key::Down), &mut control);
        assert_eq!(desktop.items[desktop.focused.unwrap()].label, "notes.txt");
        // Typing jumps by name.
        desktop.event(&key(Key::Character('p')), &mut control);
        let projects = desktop.focused.unwrap();
        assert_eq!(desktop.items[projects].label, "Projects");
        assert_eq!(desktop.selected, HashSet::from([projects]));
        // Drag Projects one column left; it stays there after a reload.
        let (x, y, w, h) = desktop.grid().rect(desktop.cells[projects]);
        let (cx, cy) = (x + w as i32 / 2, y + h as i32 / 2);
        let press = Event::PointerPressed {
            x: cx,
            y: cy,
            button: PointerButton::Primary,
        };
        desktop.event(&press, &mut control);
        desktop.event(&Event::PointerMoved { x: cx - 50, y: cy }, &mut control);
        desktop.event(
            &Event::PointerReleased {
                x: cx - 96,
                y: cy,
                button: PointerButton::Primary,
            },
            &mut control,
        );
        let moved = desktop.cells[projects];
        assert_eq!(moved.0, 1);
        let saved =
            std::fs::read_to_string(dir.path().join("config/slopos-i/desktop-positions")).unwrap();
        assert!(saved.contains("file:Projects"));
        desktop.reload();
        let projects = desktop
            .items
            .iter()
            .position(|i| i.label == "Projects")
            .unwrap();
        assert_eq!(desktop.cells[projects], moved);
        // Dropping onto another icon is refused.
        let home = desktop.cells[0];
        desktop.drop_at(
            projects,
            desktop.grid().rect(home).0 + 5,
            desktop.grid().rect(home).1 + 5,
        );
        assert_eq!(desktop.cells[projects], moved);
        // Clicking empty space clears the selection.
        desktop.event(
            &Event::PointerPressed {
                x: 5,
                y: 300,
                button: PointerButton::Primary,
            },
            &mut control,
        );
        assert!(desktop.selected.is_empty());
        let tree = desktop.accessibility().unwrap();
        assert!(tree
            .nodes
            .iter()
            .any(|(_, n)| n.role() == accesskit::Role::ListItem && n.label() == Some("Trash")));
        for factor in [1, 2] {
            let surface = RasterSurface::new(800, 600, Scale::new(factor).unwrap()).unwrap();
            desktop.paint(&surface);
        }
    }

    #[test]
    fn new_folders_never_replace_and_the_menu_offers_them() {
        let dir = tempfile::tempdir().unwrap();
        let mut desktop = desktop(dir.path());
        std::fs::create_dir(dir.path().join("Desktop/untitled folder")).unwrap();
        let mut control = Control::default();
        desktop.event(
            &Event::PointerPressed {
                x: 5,
                y: 300,
                button: PointerButton::Secondary,
            },
            &mut control,
        );
        assert!(desktop.menu.is_open());
        assert_eq!(desktop.menu.items[0].label, "New Folder");
        desktop.event(&key(Key::Down), &mut control);
        desktop.event(&key(Key::Enter), &mut control);
        assert!(dir.path().join("Desktop/untitled folder 2").is_dir());
        let focused = desktop.focused.unwrap();
        assert_eq!(desktop.items[focused].label, "untitled folder 2");
        // A file appearing on disk shows up after the watcher's change.
        std::fs::write(dir.path().join("Desktop/new.txt"), "x").unwrap();
        desktop.apply(Change::Items, &mut control);
        assert!(desktop.items.iter().any(|i| i.label == "new.txt"));
        // The picture and size follow the root window.
        desktop.apply(
            Change::Background {
                size: (1280, 800),
                picture: Image::from_rgba(1, 1, vec![1, 2, 3, 255]).ok(),
            },
            &mut control,
        );
        assert_eq!(desktop.size, (1280, 800));
        assert!(desktop.background.is_some());
    }
}
