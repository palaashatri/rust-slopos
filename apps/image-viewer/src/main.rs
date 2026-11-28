//! SLOPOS Image Viewer: PNG, JPEG, GIF, BMP, WebP, TIFF and SVG on the
//! SLOPOS toolkit.
//!
//! Decoding runs on a worker thread with the limits in `decode.rs`; the
//! window stays responsive and wakes when an image is ready. Recently viewed
//! images stay in a byte-budgeted cache and the next image in the folder is
//! decoded ahead of time, so stepping through a folder is quick.

mod decode;

use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use decode::{DecodeCache, DecodeError, Decoded, CACHE_BUDGET_BYTES, EXTENSIONS};
use slopos_ui::accessibility::window_tree;
use slopos_ui::controls::button::{Button, ButtonOutcome};
use slopos_ui::controls::file_chooser::{ChooserOutcome, FileChooser};
use slopos_ui::controls::focus::FocusChain;
use slopos_ui::controls::menu::{MenuBar, MenuBarOutcome, MenuItem};
use slopos_ui::event::{AccessibilityRequest, Event, Key, PointerButton};
use slopos_ui::foundation::{color::Color, geometry::Rect};
use slopos_ui::image_viewport::{ImageViewport, ZoomMode};
use slopos_ui::platform::{self, App, Control, Proxy, WindowOptions};
use slopos_ui::primitives::button_plate::ButtonPlateVariant;
use slopos_ui::render::{RasterSurface, RenderError};
use slopos_ui::text::{measure, Font};
use slopos_ui::theme::palette;

const WIDTH: u32 = 720;
const HEIGHT: u32 = 520;
const TOOLBAR_HEIGHT: i32 = 34;
const STATUS_HEIGHT: u32 = 24;
const STATUS_ID: u64 = 2;
const IMAGE_ID: u64 = 3;
const MENU_ID: u64 = 100;
const CHOOSER_ID: u64 = 300;
/// The canvas behind the image (a neutral dark grey, like a light box).
/// Pixels moved per arrow-key press when the image is larger than the canvas.
const PAN_STEP: i32 = 48;
/// The canvas behind the image (a neutral dark grey, like a light box).
const CANVAS: Color = Color {
    red: 0x30,
    green: 0x32,
    blue: 0x37,
    alpha: 255,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    Open,
    Close,
    Previous,
    Next,
    First,
    Last,
    Fit,
    Actual,
    ZoomIn,
    ZoomOut,
}

type MenuSpec = &'static [(&'static str, Option<&'static str>, Option<Command>)];
const FILE_MENU: MenuSpec = &[
    ("Open…", Some("Ctrl+O"), Some(Command::Open)),
    ("", None, None),
    ("Close Window", Some("Ctrl+W"), Some(Command::Close)),
];
const VIEW_MENU: MenuSpec = &[
    ("Zoom to Fit", Some("0"), Some(Command::Fit)),
    ("Actual Size", Some("1"), Some(Command::Actual)),
    ("Zoom In", Some("+"), Some(Command::ZoomIn)),
    ("Zoom Out", Some("−"), Some(Command::ZoomOut)),
];
const GO_MENU: MenuSpec = &[
    ("Previous Image", Some("Left"), Some(Command::Previous)),
    ("Next Image", Some("Right"), Some(Command::Next)),
    ("", None, None),
    ("First Image", Some("Home"), Some(Command::First)),
    ("Last Image", Some("End"), Some(Command::Last)),
];
const MENUS: [MenuSpec; 3] = [FILE_MENU, VIEW_MENU, GO_MENU];

const TOOLBAR: &[(u64, &str, Command)] = &[
    (10, "Open…", Command::Open),
    (11, "◀", Command::Previous),
    (12, "▶", Command::Next),
    (13, "Fit", Command::Fit),
    (14, "1:1", Command::Actual),
    (15, "−", Command::ZoomOut),
    (16, "+", Command::ZoomIn),
];

fn menu_items(spec: MenuSpec) -> Vec<MenuItem> {
    spec.iter()
        .map(|(label, shortcut, command)| match (command, shortcut) {
            (None, _) => MenuItem::separator(),
            (Some(_), Some(text)) => MenuItem::new(label).shortcut(text),
            (Some(_), None) => MenuItem::new(label),
        })
        .collect()
}

type WakeSlot = Arc<Mutex<Option<Proxy>>>;
type Job = (u64, PathBuf);
type JobResult = (u64, PathBuf, Result<Decoded, DecodeError>);

/// A request is stale once a newer one (or a cancel) has been issued. Prefetches
/// carry sequence 0 and are never stale.
fn is_stale(seq: u64, latest: u64) -> bool {
    seq != 0 && seq < latest
}

/// One decoding thread fed by an unbounded queue of small requests. It skips
/// requests that are already stale when it reaches them, and stops a decode
/// that becomes stale while it runs.
struct Decoder {
    requests: mpsc::Sender<Job>,
    results: mpsc::Receiver<JobResult>,
    latest: Arc<std::sync::atomic::AtomicU64>,
}

impl Decoder {
    fn new(slot: WakeSlot) -> Self {
        let (requests, jobs) = mpsc::channel::<Job>();
        let (done, results) = mpsc::channel::<JobResult>();
        let latest = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let worker_latest = latest.clone();
        let spawned = thread::Builder::new()
            .name("image-decode".into())
            .spawn(move || {
                while let Ok((seq, path)) = jobs.recv() {
                    let load = || worker_latest.load(std::sync::atomic::Ordering::Relaxed);
                    if is_stale(seq, load()) {
                        continue;
                    }
                    let cancelled = || is_stale(seq, load());
                    let result = decode::decode_path_checked(&path, &cancelled);
                    if done.send((seq, path, result)).is_err() {
                        break;
                    }
                    if let Ok(guard) = slot.lock() {
                        if let Some(proxy) = guard.as_ref() {
                            proxy.wake();
                        }
                    }
                }
            });
        if let Err(error) = spawned {
            eprintln!("slopos-image-viewer: cannot start the decoder: {error}");
        }
        Self {
            requests,
            results,
            latest,
        }
    }
}

pub struct ImageViewerApp {
    window: (u32, u32),
    menu: MenuBar,
    toolbar: Vec<(Button, Command)>,
    focus: FocusChain,
    chooser: Option<FileChooser>,
    viewport: ImageViewport,
    image: Option<Arc<Decoded>>,
    /// The file `image` was decoded from. It is what is on screen, which can
    /// differ from `path` while an open is pending.
    shown: Option<PathBuf>,
    path: Option<PathBuf>,
    folder: Vec<PathBuf>,
    /// The folder listing stopped at a limit, so `folder` is only its start.
    folder_truncated: bool,
    index: usize,
    cache: DecodeCache,
    decoder: Decoder,
    slot: WakeSlot,
    pending: Option<(u64, PathBuf)>,
    serial: u64,
    drag: Option<(i32, i32)>,
    status: String,
    font: Font,
}

impl ImageViewerApp {
    pub fn new(path: Option<&Path>) -> Self {
        let slot: WakeSlot = Arc::new(Mutex::new(None));
        let toolbar: Vec<(Button, Command)> = TOOLBAR
            .iter()
            .map(|(id, label, command)| {
                (
                    Button::new(*id, label, ButtonPlateVariant::Regular),
                    *command,
                )
            })
            .collect();
        let mut app = Self {
            window: (WIDTH, HEIGHT),
            menu: MenuBar::new(
                MENU_ID,
                vec![
                    ("File", menu_items(FILE_MENU)),
                    ("View", menu_items(VIEW_MENU)),
                    ("Go", menu_items(GO_MENU)),
                ],
            ),
            focus: FocusChain::new(toolbar.iter().map(|(button, _)| button.id).collect()),
            toolbar,
            chooser: None,
            viewport: ImageViewport::new(0, 0, Rect::default()),
            image: None,
            shown: None,
            path: None,
            folder: Vec::new(),
            folder_truncated: false,
            index: 0,
            cache: DecodeCache::new(CACHE_BUDGET_BYTES),
            decoder: Decoder::new(slot.clone()),
            slot,
            pending: None,
            serial: 0,
            drag: None,
            status: "Choose File ▸ Open… to view an image.".into(),
            font: Font::system(12.0),
        };
        app.layout();
        if let Some(path) = path {
            app.open(path);
        }
        app.refresh_controls();
        app
    }

    fn canvas(&self) -> Rect {
        let top = self.menu.height() as i32 + TOOLBAR_HEIGHT;
        Rect {
            x: 0,
            y: top,
            width: self.window.0,
            height: self.window.1.saturating_sub(top as u32 + STATUS_HEIGHT),
        }
    }

    fn layout(&mut self) {
        self.menu.bounds = Rect {
            x: 0,
            y: 0,
            width: self.window.0,
            height: self.menu.height(),
        };
        let toolbar_y = self.menu.height() as i32;
        let mut x = 8;
        for (button, _) in &mut self.toolbar {
            let width = measure(&button.label, &self.font).width as u32 + 22;
            let width = width.max(36);
            button.bounds = Rect {
                x,
                y: toolbar_y + 7,
                width,
                height: 20,
            };
            x += width as i32
                + if button.label == "Open…" || button.label == "▶" || button.label == "1:1" {
                    16
                } else {
                    6
                };
        }
        let canvas = self.canvas();
        self.viewport.resize(canvas);
        if let Some(chooser) = &mut self.chooser {
            chooser.layout(self.window);
        }
    }

    fn enabled(&self, command: Command) -> bool {
        let has_image = self.image.is_some();
        let many = self.folder.len() > 1;
        match command {
            Command::Open | Command::Close => true,
            Command::Previous | Command::Next | Command::First | Command::Last => many,
            Command::Fit => has_image && self.viewport.mode() != ZoomMode::Fit,
            Command::Actual | Command::ZoomIn | Command::ZoomOut => has_image,
        }
    }

    fn refresh_controls(&mut self) {
        let flags: Vec<Vec<bool>> = MENUS
            .iter()
            .map(|spec| {
                spec.iter()
                    .map(|(_, _, c)| c.is_some_and(|c| self.enabled(c)))
                    .collect()
            })
            .collect();
        for (menu, flags) in self.menu.menus.iter_mut().zip(flags) {
            for (item, enabled) in menu.items.iter_mut().zip(flags) {
                if !item.separator {
                    item.enabled = enabled;
                }
            }
        }
        let enabled: Vec<bool> = self.toolbar.iter().map(|(_, c)| self.enabled(*c)).collect();
        for ((button, _), enabled) in self.toolbar.iter_mut().zip(enabled) {
            button.enabled = enabled;
        }
    }

    fn sync_focus(&mut self) {
        let focused = self.focus.focused();
        for (button, _) in &mut self.toolbar {
            button.focused = focused == Some(button.id);
        }
    }

    /// Open `path`, listing its folder for next/previous. A folder opens its
    /// first image, so a folder can be handed to the viewer.
    fn open(&mut self, path: &Path) {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if path.is_dir() {
            return self.open_folder(&path);
        }
        let (listing, index) = decode::siblings(&path);
        self.folder = listing.images;
        self.folder_truncated = listing.truncated;
        self.index = index;
        self.show(path);
    }

    fn open_folder(&mut self, dir: &Path) {
        let listing = decode::images_in(dir);
        let Some(first) = listing.images.first().cloned() else {
            self.abandon_pending();
            self.folder.clear();
            self.folder_truncated = listing.truncated;
            self.index = 0;
            self.path = None;
            self.image = None;
            self.shown = None;
            self.status = format!("No images in “{}”.", file_name(dir));
            self.refresh_controls();
            return;
        };
        self.folder = listing.images;
        self.folder_truncated = listing.truncated;
        self.index = 0;
        self.show(first);
    }

    fn show(&mut self, path: PathBuf) {
        self.path = Some(path.clone());
        // A new target supersedes any open still in flight.
        self.abandon_pending();
        if let Some(decoded) = self.cache.get(&path) {
            self.set_image(decoded, path);
            self.prefetch();
            return;
        }
        self.serial += 1;
        self.decoder
            .latest
            .store(self.serial, std::sync::atomic::Ordering::Relaxed);
        self.pending = Some((self.serial, path.clone()));
        self.status = format!("Opening “{}”…", file_name(&path));
        let _ = self.decoder.requests.send((self.serial, path));
    }

    /// Stop waiting for the pending open. A decode already running sees the newer
    /// serial and stops at its next checkpoint. Any result it still produces only
    /// fills the cache.
    fn abandon_pending(&mut self) {
        if self.pending.take().is_some() {
            self.serial += 1;
            self.decoder
                .latest
                .store(self.serial, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Escape: cancel the open in progress and go back to the image on screen.
    fn cancel_open(&mut self) {
        let Some((_, path)) = self.pending.clone() else {
            return;
        };
        self.abandon_pending();
        match self.shown.clone() {
            Some(shown) => {
                if let Some(index) = self.folder.iter().position(|p| *p == shown) {
                    self.index = index;
                }
                self.path = Some(shown);
            }
            None => self.path = None,
        }
        self.status = format!("Cancelled opening “{}”.", file_name(&path));
        self.refresh_controls();
    }

    fn set_image(&mut self, decoded: Arc<Decoded>, path: PathBuf) {
        self.viewport.set_image(decoded.width, decoded.height);
        self.image = Some(decoded);
        self.shown = Some(path);
        self.update_status();
        self.refresh_controls();
    }

    /// Decode the next image ahead of time if it is not cached yet.
    fn prefetch(&mut self) {
        if self.folder.len() < 2 {
            return;
        }
        let next = self.folder[(self.index + 1) % self.folder.len()].clone();
        if self.cache.get(&next).is_none() {
            let _ = self.decoder.requests.send((0, next));
        }
    }

    fn update_status(&mut self) {
        let Some(image) = &self.image else {
            return;
        };
        let name = self.path.as_deref().map(file_name).unwrap_or_default();
        let mut position = if self.folder.len() > 1 {
            format!(" — {} of {}", self.index + 1, self.folder.len())
        } else {
            String::new()
        };
        if self.folder_truncated {
            position.push_str(" — folder listing stopped early, only part of it is listed");
        }
        self.status = format!(
            "{name} — {} × {} {}{} — {:.0}%{position}",
            image.width,
            image.height,
            image.format,
            if image.reoriented {
                " (rotated by its camera)"
            } else {
                ""
            },
            self.viewport.zoom() * 100.0
        );
    }

    fn receive(&mut self) -> bool {
        let mut changed = false;
        while let Ok((seq, path, result)) = self.decoder.results.try_recv() {
            let current = self
                .pending
                .as_ref()
                .is_some_and(|(s, p)| *s == seq && *p == path);
            match result {
                Ok(decoded) => {
                    let decoded = Arc::new(decoded);
                    self.cache.insert(path.clone(), decoded.clone());
                    if current {
                        self.pending = None;
                        self.set_image(decoded, path);
                        self.prefetch();
                        changed = true;
                    }
                }
                Err(DecodeError::Cancelled) => {}
                Err(error) if current => {
                    self.pending = None;
                    self.image = None;
                    self.shown = None;
                    self.status = format!("Can't open “{}”: {error}", file_name(&path));
                    self.refresh_controls();
                    changed = true;
                }
                Err(_) => {}
            }
        }
        changed
    }

    fn step(&mut self, delta: isize) {
        if self.folder.len() < 2 {
            return;
        }
        let count = self.folder.len() as isize;
        self.index = (self.index as isize + delta).rem_euclid(count) as usize;
        self.show(self.folder[self.index].clone());
    }

    fn run(&mut self, command: Command, control: &mut Control) {
        if !self.enabled(command) {
            return;
        }
        match command {
            Command::Open => {
                let dir = self
                    .path
                    .as_deref()
                    .and_then(Path::parent)
                    .map(Path::to_path_buf)
                    .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
                    .unwrap_or_else(|| PathBuf::from("/"));
                let filter = EXTENSIONS.iter().map(|ext| (*ext).to_owned()).collect();
                let mut chooser = FileChooser::new(CHOOSER_ID, "Open Image", &dir, Some(filter));
                chooser.layout(self.window);
                self.chooser = Some(chooser);
            }
            Command::Close => control.quit(),
            Command::Previous => self.step(-1),
            Command::Next => self.step(1),
            Command::First => {
                self.index = 0;
                self.show(self.folder[0].clone());
            }
            Command::Last => {
                self.index = self.folder.len() - 1;
                self.show(self.folder[self.index].clone());
            }
            Command::Fit => self.viewport.fit(),
            Command::Actual => self.viewport.actual_pixels(),
            Command::ZoomIn => {
                self.viewport.zoom_by(1.25);
            }
            Command::ZoomOut => {
                self.viewport.zoom_by(0.8);
            }
        }
        self.update_status();
        self.refresh_controls();
    }

    fn shortcut(key: &Key, control: bool) -> Option<Command> {
        Some(match key {
            Key::Character('o' | 'O') if control => Command::Open,
            Key::Character('w' | 'W' | 'q' | 'Q') if control => Command::Close,
            Key::Character('0') => Command::Fit,
            Key::Character('1') => Command::Actual,
            Key::Character('+' | '=') => Command::ZoomIn,
            Key::Character('-' | '_') => Command::ZoomOut,
            Key::Left | Key::PageUp | Key::Backspace => Command::Previous,
            Key::Right | Key::PageDown | Key::Space => Command::Next,
            Key::Home => Command::First,
            Key::End => Command::Last,
            _ => return None,
        })
    }

    fn handle(&mut self, event: &Event, control: &mut Control) {
        if let Some(chooser) = &mut self.chooser {
            match chooser.handle(event, control.modifiers()) {
                ChooserOutcome::Chosen(path) => {
                    self.chooser = None;
                    self.open(&path);
                }
                ChooserOutcome::Cancelled => self.chooser = None,
                ChooserOutcome::Copy(text) => control.set_clipboard(&text),
                ChooserOutcome::PasteRequested => control.request_paste(),
                _ => {}
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
        if self.focus.handle(event).is_some() {
            self.sync_focus();
            return;
        }
        match event {
            Event::KeyPressed {
                key: Key::Escape, ..
            } if self.pending.is_some() => return self.cancel_open(),
            Event::KeyPressed { key, modifiers } => {
                // Space and Enter belong to a focused toolbar button.
                let button_focused = self.toolbar.iter().any(|(b, _)| b.focused);
                if !(button_focused && matches!(key, Key::Space | Key::Enter)) {
                    if let Some(command) = Self::shortcut(key, modifiers.control) {
                        return self.run(command, control);
                    }
                }
            }
            Event::Accessibility(AccessibilityRequest::Click(id)) => {
                if let Some(command) = self
                    .toolbar
                    .iter()
                    .find(|(b, _)| b.id == *id)
                    .map(|(_, c)| *c)
                {
                    return self.run(command, control);
                }
                if let Some(menu) = self.menu.menu_for_title_node(*id) {
                    self.menu.open(menu, self.window, true);
                }
                return;
            }
            Event::Accessibility(AccessibilityRequest::Focus(id)) => {
                self.focus.focus(*id);
                self.sync_focus();
                return;
            }
            Event::KeyPressed {
                key: key @ (Key::Up | Key::Down),
                ..
            } if self.image.is_some() && self.viewport.can_pan() => {
                // Up shows what lies above, so the picture moves down.
                let dy = if matches!(key, Key::Up) {
                    PAN_STEP
                } else {
                    -PAN_STEP
                };
                self.viewport.pan_by(0, dy);
                return;
            }
            Event::Scroll { dy, .. } if self.image.is_some() => {
                let factor = if *dy < 0 { 1.25 } else { 0.8 };
                self.viewport.zoom_by(factor);
                self.update_status();
                self.refresh_controls();
                return;
            }
            Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
            } if self.canvas().contains(*x, *y) && self.viewport.can_pan() => {
                self.drag = Some((*x, *y));
                return;
            }
            Event::PointerMoved { x, y } if self.drag.is_some() => {
                if let Some((last_x, last_y)) = self.drag {
                    self.viewport.pan_by(x - last_x, y - last_y);
                    self.drag = Some((*x, *y));
                }
                return;
            }
            Event::PointerReleased {
                button: PointerButton::Primary,
                ..
            } if self.drag.is_some() => {
                self.drag = None;
                return;
            }
            Event::DoubleClicked { x, y }
                if self.canvas().contains(*x, *y) && self.image.is_some() =>
            {
                let command = if self.viewport.mode() == ZoomMode::Fit {
                    Command::Actual
                } else {
                    Command::Fit
                };
                return self.run(command, control);
            }
            _ => {}
        }
        let mut chosen = None;
        for (button, command) in &mut self.toolbar {
            if button.handle(event) == ButtonOutcome::Activated {
                chosen = Some(*command);
            }
        }
        if let Some(command) = chosen {
            self.run(command, control);
        }
    }

    fn paint_all(&mut self, surface: &RasterSurface) -> Result<(), RenderError> {
        let colors = palette();
        let (width, height) = self.window;
        surface.clear(colors.surface)?;
        let canvas = self.canvas();
        surface.fill_rect(canvas, CANVAS)?;
        if let (Some(image), Some(dest)) = (&self.image, self.viewport.image_bounds()) {
            surface.with_clip(canvas, || {
                surface.draw_image_scaled(&image.image, dest.x, dest.y, dest.width, dest.height)
            })?;
        } else {
            let text = if self.pending.is_some() {
                "Opening…"
            } else {
                "No image"
            };
            let w = measure(text, &self.font).width as i32;
            surface.draw_text(
                text,
                canvas.x + (canvas.width as i32 - w) / 2,
                canvas.y + canvas.height as i32 / 2 - 8,
                &self.font,
                Color::WHITE,
            )?;
        }
        self.menu.paint(surface)?;
        for (button, _) in &self.toolbar {
            button.paint(surface, &self.font)?;
        }
        surface.fill_rect(
            Rect {
                x: 0,
                y: canvas.y - 1,
                width,
                height: 1,
            },
            colors.bevel_shadow,
        )?;
        let status_y = height as i32 - STATUS_HEIGHT as i32;
        surface.fill_rect(
            Rect {
                x: 0,
                y: status_y,
                width,
                height: STATUS_HEIGHT,
            },
            colors.surface,
        )?;
        surface.fill_rect(
            Rect {
                x: 0,
                y: status_y,
                width,
                height: 1,
            },
            colors.bevel_shadow,
        )?;
        surface.with_clip(
            Rect {
                x: 8,
                y: status_y,
                width: width.saturating_sub(16),
                height: STATUS_HEIGHT,
            },
            || surface.draw_text(&self.status, 8, status_y + 5, &self.font, colors.ink),
        )?;
        if let Some(chooser) = &mut self.chooser {
            chooser.paint(surface)?;
        }
        self.menu.paint_menus(surface)
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

impl App for ImageViewerApp {
    fn event(&mut self, event: &Event, control: &mut Control) {
        match event {
            Event::CloseRequested => return control.quit(),
            Event::Resized { width, height } => {
                self.window = (*width, *height);
                self.layout();
                self.update_status();
            }
            // Applied even under the chooser: the bar's height changes the layout.
            Event::GlobalMenu(_) => {
                self.menu.handle(event, self.window);
                self.layout();
                self.update_status();
            }
            Event::Wake => {
                if !self.receive() {
                    return;
                }
            }
            _ => self.handle(event, control),
        }
        let title = self
            .path
            .as_deref()
            .map(file_name)
            .unwrap_or_else(|| "Image Viewer".into());
        control.set_title(&title);
        control.request_redraw();
    }

    fn paint(&mut self, surface: &RasterSurface) {
        if let Err(error) = self.paint_all(surface) {
            eprintln!("slopos-image-viewer: paint failed: {error}");
        }
    }

    fn menu_bar(&self) -> Option<&MenuBar> {
        Some(&self.menu)
    }

    fn accessibility(&self) -> Option<accesskit::TreeUpdate> {
        let mut nodes = self.menu.accessibility_nodes();
        for (button, command) in &self.toolbar {
            let mut node = button.accessibility_node();
            let label = match command {
                Command::Previous => "Previous image",
                Command::Next => "Next image",
                Command::ZoomIn => "Zoom in",
                Command::ZoomOut => "Zoom out",
                Command::Actual => "Actual size",
                Command::Fit => "Zoom to fit",
                _ => button.label.as_str(),
            };
            node.set_label(label);
            nodes.push((button.id, node));
        }
        let mut image = accesskit::Node::new(accesskit::Role::Image);
        image.set_label(match (&self.image, &self.path) {
            (Some(image), Some(path)) => format!(
                "{}, {} by {} pixels",
                file_name(path),
                image.width,
                image.height
            ),
            _ => "No image".into(),
        });
        if let Some(bounds) = self.viewport.image_bounds() {
            image.set_bounds(accesskit::Rect {
                x0: f64::from(bounds.x),
                y0: f64::from(bounds.y),
                x1: f64::from(bounds.x) + f64::from(bounds.width),
                y1: f64::from(bounds.y) + f64::from(bounds.height),
            });
        }
        nodes.push((IMAGE_ID, image));
        let status = slopos_ui::accessibility::text_label(self.status.as_str());
        nodes.push((STATUS_ID, status));
        let focus = if let Some(chooser) = &self.chooser {
            nodes.extend(chooser.accessibility_nodes());
            Some(chooser.focus_node())
        } else {
            self.menu.focus_node().or(self.focus.focused())
        };
        let title = self
            .path
            .as_deref()
            .map(file_name)
            .unwrap_or_else(|| "Image Viewer".into());
        Some(window_tree(&title, nodes, focus))
    }
}

fn main() {
    if std::env::var_os("RUST_LOG").is_some() {
        let _ = env_logger::try_init();
    }
    let mut args = std::env::args_os().skip(1);
    let first = args.next();
    let (snapshot, path): (Option<PathBuf>, Option<PathBuf>) =
        if first.as_deref() == Some(std::ffi::OsStr::new("--snapshot")) {
            (args.next().map(Into::into), args.next().map(Into::into))
        } else {
            (None, first.map(Into::into))
        };
    let mut app = ImageViewerApp::new(path.as_deref());
    if let Some(output) = snapshot {
        // Wait for the first decode so the snapshot shows the image. A folder or
        // an empty start has nothing pending to wait for.
        let waited = app
            .pending
            .is_some()
            .then(|| {
                app.decoder
                    .results
                    .recv_timeout(std::time::Duration::from_secs(10))
            })
            .and_then(Result::ok);
        if let Some((_, path, decoded)) = waited {
            match decoded {
                Ok(decoded) => {
                    app.pending = None;
                    app.set_image(Arc::new(decoded), path);
                }
                Err(error) => app.status = format!("Can't open “{}”: {error}", file_name(&path)),
            }
        }
        let surface =
            RasterSurface::new(WIDTH, HEIGHT, platform::scale_from_environment()).expect("surface");
        app.paint(&surface);
        if let Err(error) = surface.save_png(&output) {
            eprintln!("slopos-image-viewer: {error}");
            std::process::exit(1);
        }
        return;
    }
    let slot = app.slot.clone();
    let options = WindowOptions::new("Image Viewer", "slopos-image-viewer", WIDTH, HEIGHT);
    if let Err(error) = platform::run(options, app, move |proxy| {
        if let Ok(mut guard) = slot.lock() {
            *guard = Some(proxy.clone());
        }
        proxy.wake();
    }) {
        eprintln!("slopos-image-viewer: {error}");
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

    fn settle(app: &mut ImageViewerApp) {
        for _ in 0..500 {
            app.event(&Event::Wake, &mut Control::default());
            if app.pending.is_none() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("decode did not finish");
    }

    fn folder() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (name, w, h) in [("a.png", 8, 6), ("b.png", 300, 200), ("c.png", 4, 4)] {
            std::fs::write(dir.path().join(name), decode::tests::png(w, h)).unwrap();
        }
        std::fs::write(dir.path().join("notes.txt"), b"x").unwrap();
        dir
    }

    #[test]
    fn opens_and_steps_through_the_folder() {
        let dir = folder();
        let mut app = ImageViewerApp::new(Some(&dir.path().join("b.png")));
        settle(&mut app);
        assert_eq!(app.folder.len(), 3);
        assert_eq!(app.image.as_ref().unwrap().width, 300);
        assert!(app.status.contains("2 of 3"));
        let mut control = Control::default();
        app.event(&key(Key::Right), &mut control);
        settle(&mut app);
        assert_eq!(app.image.as_ref().unwrap().width, 4);
        app.event(&key(Key::Right), &mut control);
        settle(&mut app);
        assert_eq!(app.index, 0);
        app.event(&key(Key::End), &mut control);
        settle(&mut app);
        assert_eq!(app.index, 2);
        assert!(app.cache.len() >= 2 && app.cache.used() > 0);
    }

    #[test]
    fn zoom_pan_and_fit() {
        let dir = folder();
        let mut app = ImageViewerApp::new(Some(&dir.path().join("b.png")));
        settle(&mut app);
        let mut control = Control::default();
        for _ in 0..8 {
            app.event(&key(Key::Character('+')), &mut control);
        }
        assert!(app.viewport.zoom() > 4.0);
        assert!(app.viewport.can_pan());
        let before = app.viewport.image_bounds().unwrap();
        let canvas = app.canvas();
        let (cx, cy) = (canvas.x + 100, canvas.y + 100);
        app.event(
            &Event::PointerPressed {
                x: cx,
                y: cy,
                button: PointerButton::Primary,
            },
            &mut control,
        );
        app.event(
            &Event::PointerMoved {
                x: cx + 40,
                y: cy + 30,
            },
            &mut control,
        );
        app.event(
            &Event::PointerReleased {
                x: cx + 40,
                y: cy + 30,
                button: PointerButton::Primary,
            },
            &mut control,
        );
        let after = app.viewport.image_bounds().unwrap();
        assert_eq!((after.x - before.x, after.y - before.y), (40, 30));
        app.event(&key(Key::Character('0')), &mut control);
        assert_eq!(app.viewport.mode(), ZoomMode::Fit);
        for factor in [1, 2] {
            let surface = RasterSurface::new(WIDTH, HEIGHT, Scale::new(factor).unwrap()).unwrap();
            app.paint(&surface);
        }
    }

    #[test]
    fn arrow_keys_pan_a_zoomed_image_by_a_fixed_step() {
        let dir = folder();
        let mut app = ImageViewerApp::new(Some(&dir.path().join("b.png")));
        settle(&mut app);
        let mut control = Control::default();
        for _ in 0..8 {
            app.event(&key(Key::Character('+')), &mut control);
        }
        let start = app.viewport.image_bounds().unwrap();
        app.event(&key(Key::Up), &mut control);
        let up = app.viewport.image_bounds().unwrap();
        assert_eq!(up.y - start.y, PAN_STEP);
        app.event(&key(Key::Down), &mut control);
        app.event(&key(Key::Down), &mut control);
        let down = app.viewport.image_bounds().unwrap();
        assert_eq!(down.y - up.y, -2 * PAN_STEP);
        // Arrow keys do not move a picture that fits, so they are not swallowed
        // by panning and still reach the rest of the handler.
        app.event(&key(Key::Character('0')), &mut control);
        let fitted = app.viewport.image_bounds().unwrap();
        app.event(&key(Key::Up), &mut control);
        assert_eq!(app.viewport.image_bounds().unwrap(), fitted);
    }

    /// Opaque device pixel as 0xAARRGGBB, the value `RasterSurface::pixel` gives.
    fn argb(color: Color) -> u32 {
        0xFF00_0000
            | (u32::from(color.red) << 16)
            | (u32::from(color.green) << 8)
            | u32::from(color.blue)
    }

    fn device_pixel(surface: &RasterSurface, x: i32, y: i32) -> u32 {
        surface
            .pixel(x as u32, y as u32)
            .expect("the pixel is inside the surface")
    }

    /// The colour `folder()` gives a.png at (x, y).
    fn a_png_source(x: u32, y: u32) -> u32 {
        argb(Color {
            red: (x * 40) as u8,
            green: (y * 40) as u8,
            blue: 200,
            alpha: 255,
        })
    }

    #[test]
    fn fit_paints_the_picture_pixel_for_pixel_on_the_canvas() {
        let dir = folder();
        let mut app = ImageViewerApp::new(Some(&dir.path().join("a.png")));
        settle(&mut app);
        let bounds = app.viewport.image_bounds().unwrap();
        // 8 by 6 is smaller than the canvas, so Fit shows it at 1:1.
        assert_eq!((bounds.width, bounds.height), (8, 6));
        let surface = RasterSurface::new(WIDTH, HEIGHT, Scale::new(1).unwrap()).unwrap();
        app.paint(&surface);
        for (x, y) in [(0, 0), (3, 2), (7, 5)] {
            assert_eq!(
                device_pixel(&surface, bounds.x + x, bounds.y + y),
                a_png_source(x as u32, y as u32),
                "picture pixel ({x}, {y})"
            );
        }
        let canvas = app.canvas();
        assert_eq!(
            device_pixel(&surface, canvas.x + 1, canvas.y + 1),
            argb(CANVAS)
        );
    }

    #[test]
    fn a_two_x_surface_draws_the_picture_at_double_size() {
        let dir = folder();
        let mut app = ImageViewerApp::new(Some(&dir.path().join("a.png")));
        settle(&mut app);
        let bounds = app.viewport.image_bounds().unwrap();
        let surface = RasterSurface::new(WIDTH, HEIGHT, Scale::new(2).unwrap()).unwrap();
        app.paint(&surface);
        // Inside the picture's top-left corner, in device pixels.
        assert_ne!(
            device_pixel(&surface, bounds.x * 2 + 1, bounds.y * 2 + 1),
            argb(CANVAS)
        );
        // Just past its right edge in device pixels, the canvas shows.
        let right = (bounds.x + bounds.width as i32) * 2 + 1;
        assert_eq!(
            device_pixel(&surface, right, bounds.y * 2 + 1),
            argb(CANVAS)
        );
    }

    #[test]
    fn a_zoomed_picture_covers_the_canvas_and_is_clipped_to_it() {
        let dir = folder();
        let mut app = ImageViewerApp::new(Some(&dir.path().join("b.png")));
        settle(&mut app);
        let mut control = Control::default();
        for _ in 0..8 {
            app.event(&key(Key::Character('+')), &mut control);
        }
        let surface = RasterSurface::new(WIDTH, HEIGHT, Scale::new(1).unwrap()).unwrap();
        app.paint(&surface);
        let canvas = app.canvas();
        // The picture is far larger than the canvas, so its colour reaches
        // the canvas corner, and nothing paints outside the canvas.
        assert_ne!(device_pixel(&surface, canvas.x, canvas.y), argb(CANVAS));
        let far_corner = (
            canvas.x + canvas.width as i32 - 1,
            canvas.y + canvas.height as i32 - 1,
        );
        assert_ne!(
            device_pixel(&surface, far_corner.0, far_corner.1),
            argb(CANVAS)
        );
    }

    #[test]
    fn accessibility_tree_names_the_picture_and_its_bounds() {
        use accesskit::{NodeId, Role};
        let dir = folder();
        let mut app = ImageViewerApp::new(Some(&dir.path().join("a.png")));
        settle(&mut app);
        let tree = app.accessibility().unwrap();
        let node = |id: u64| {
            tree.nodes
                .iter()
                .find(|(node_id, _)| *node_id == NodeId(id))
                .map(|(_, node)| node)
                .expect("node present")
        };
        let picture = node(IMAGE_ID);
        assert_eq!(picture.role(), Role::Image);
        assert_eq!(picture.label(), Some("a.png, 8 by 6 pixels"));
        let bounds = app.viewport.image_bounds().unwrap();
        let rect = picture.bounds().expect("picture has bounds");
        assert_eq!(
            (rect.x0, rect.y0),
            (f64::from(bounds.x), f64::from(bounds.y))
        );
        assert_eq!((rect.x1 - rect.x0, rect.y1 - rect.y0), (8.0, 6.0));
        let labels: Vec<&str> = tree.nodes.iter().filter_map(|(_, n)| n.label()).collect();
        let expected_labels = [
            "Previous image",
            "Next image",
            "Zoom in",
            "Zoom out",
            "Actual size",
            "Zoom to fit",
        ];
        for expected in expected_labels {
            assert!(
                labels.contains(&expected),
                "missing control label {expected}"
            );
        }
    }

    #[test]
    fn empty_window_says_no_image_and_disables_image_controls() {
        let app = ImageViewerApp::new(None);
        let tree = app.accessibility().unwrap();
        let picture = tree
            .nodes
            .iter()
            .find(|(node_id, _)| *node_id == accesskit::NodeId(IMAGE_ID))
            .map(|(_, node)| node)
            .unwrap();
        assert_eq!(picture.label(), Some("No image"));
        assert!(picture.bounds().is_none());
        let zoom_in = tree
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some("Zoom in"))
            .map(|(_, node)| node)
            .unwrap();
        assert!(zoom_in.is_disabled());
    }

    #[test]
    fn errors_are_reported_and_open_dialog_filters_images() {
        let dir = folder();
        std::fs::write(dir.path().join("broken.png"), b"\x89PNG\r\n\x1a\nnope").unwrap();
        let mut app = ImageViewerApp::new(Some(&dir.path().join("broken.png")));
        settle(&mut app);
        assert!(app.image.is_none());
        assert!(app.status.starts_with("Can't open “broken.png”"));
        let mut control = Control::default();
        app.event(
            &Event::KeyPressed {
                key: Key::Character('o'),
                modifiers: Modifiers {
                    control: true,
                    ..Modifiers::default()
                },
            },
            &mut control,
        );
        let chooser = app.chooser.as_ref().unwrap();
        let names: Vec<&str> = chooser.entries().iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"a.png") && !names.contains(&"notes.txt"));
        // Enter on the first entry opens it.
        app.event(&key(Key::Enter), &mut control);
        settle(&mut app);
        assert!(app.chooser.is_none());
        assert_eq!(app.image.as_ref().unwrap().width, 8);
    }

    #[test]
    fn empty_window_paints_and_publishes_accessibility() {
        let mut app = ImageViewerApp::new(None);
        let mut control = Control::default();
        app.event(&key(Key::Tab), &mut control);
        assert_eq!(app.focus.focused(), Some(10));
        let tree = app.accessibility().unwrap();
        assert_eq!(tree.focus, accesskit::NodeId(10));
        let surface = RasterSurface::new(WIDTH, HEIGHT, Scale::new(1).unwrap()).unwrap();
        app.paint(&surface);
        // Next is disabled with no folder.
        assert!(!app.toolbar[2].0.enabled);
    }

    #[test]
    fn opening_a_folder_opens_its_first_image() {
        let dir = folder();
        let mut app = ImageViewerApp::new(Some(dir.path()));
        settle(&mut app);
        assert_eq!(app.folder.len(), 3);
        assert_eq!(app.image.as_ref().unwrap().width, 8);
        assert!(app.status.contains("1 of 3"));
    }

    #[test]
    fn a_folder_without_images_says_so() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("notes.txt"), b"x").unwrap();
        let app = ImageViewerApp::new(Some(dir.path()));
        assert!(app.image.is_none());
        assert!(app.pending.is_none());
        assert!(app.status.starts_with("No images in"));
    }

    #[test]
    fn escape_cancels_a_pending_open_and_keeps_the_shown_image() {
        let dir = folder();
        let mut app = ImageViewerApp::new(Some(&dir.path().join("a.png")));
        settle(&mut app);
        let mut control = Control::default();
        // Escape with nothing pending changes nothing.
        app.event(&key(Key::Escape), &mut control);
        assert_eq!(app.image.as_ref().unwrap().width, 8);
        // End goes to c.png, which is not prefetched, so its decode is pending.
        app.event(&key(Key::End), &mut control);
        assert!(app.pending.is_some());
        app.event(&key(Key::Escape), &mut control);
        assert!(app.pending.is_none());
        assert_eq!(app.index, 0);
        assert_eq!(app.path.as_deref().map(file_name), Some("a.png".into()));
        assert!(app.status.starts_with("Cancelled opening “c.png”"));
        // Whatever the worker finishes now must not replace the image.
        for _ in 0..20 {
            app.event(&Event::Wake, &mut control);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(app.image.as_ref().unwrap().width, 8);
        assert_eq!(app.index, 0);
    }

    #[test]
    fn returning_to_a_cached_image_supersedes_the_older_request() {
        use std::sync::atomic::Ordering;
        let dir = folder();
        let mut app = ImageViewerApp::new(Some(&dir.path().join("a.png")));
        settle(&mut app);
        let mut control = Control::default();
        app.event(&key(Key::End), &mut control);
        assert!(app.pending.is_some());
        // a.png is cached, so Home shows it at once and drops the c.png request.
        app.event(&key(Key::Home), &mut control);
        assert!(app.pending.is_none());
        assert_eq!(app.decoder.latest.load(Ordering::Relaxed), app.serial);
        for _ in 0..20 {
            app.event(&Event::Wake, &mut control);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(app.image.as_ref().unwrap().width, 8);
        assert_eq!(app.index, 0);
    }

    #[test]
    fn truncated_files_report_a_clear_error() {
        let dir = tempfile::tempdir().unwrap();
        let jpeg = decode::tests::encode(image::ImageFormat::Jpeg);
        let cut = dir.path().join("cut.jpg");
        std::fs::write(&cut, &jpeg[..jpeg.len() - 10]).unwrap();
        let mut app = ImageViewerApp::new(Some(&cut));
        settle(&mut app);
        assert!(app.image.is_none());
        assert_eq!(
            app.status,
            "Can't open “cut.jpg”: the file ends before the image does, so it may be incomplete"
        );
    }
}
