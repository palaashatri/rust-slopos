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
use slopos_ui::controls::menu::{MenuBar, MenuBarOutcome, MenuItem, MENU_BAR_HEIGHT};
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

/// One decoding thread fed by an unbounded queue of small requests; it
/// skips requests that are already stale when it reaches them.
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
                    // Skip anything older than the newest request (rapid
                    // next/next/next), except prefetches marked with seq 0.
                    let newest = worker_latest.load(std::sync::atomic::Ordering::Relaxed);
                    if seq != 0 && seq < newest {
                        continue;
                    }
                    let result = decode::decode_path(&path);
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
    path: Option<PathBuf>,
    folder: Vec<PathBuf>,
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
            path: None,
            folder: Vec::new(),
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
        let top = MENU_BAR_HEIGHT as i32 + TOOLBAR_HEIGHT;
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
            height: MENU_BAR_HEIGHT,
        };
        let mut x = 8;
        for (button, _) in &mut self.toolbar {
            let width = measure(&button.label, &self.font).width as u32 + 22;
            let width = width.max(36);
            button.bounds = Rect {
                x,
                y: MENU_BAR_HEIGHT as i32 + 7,
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

    /// Open `path`, listing its folder for next/previous.
    fn open(&mut self, path: &Path) {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let (folder, index) = decode::siblings(&path);
        self.folder = folder;
        self.index = index;
        self.show(path);
    }

    fn show(&mut self, path: PathBuf) {
        self.path = Some(path.clone());
        if let Some(decoded) = self.cache.get(&path) {
            self.pending = None;
            self.set_image(decoded);
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

    fn set_image(&mut self, decoded: Arc<Decoded>) {
        self.viewport.set_image(decoded.width, decoded.height);
        self.image = Some(decoded);
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
        let position = if self.folder.len() > 1 {
            format!(" — {} of {}", self.index + 1, self.folder.len())
        } else {
            String::new()
        };
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
                        self.set_image(decoded);
                        self.prefetch();
                        changed = true;
                    }
                }
                Err(error) if current => {
                    self.pending = None;
                    self.image = None;
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
        let mut status = accesskit::Node::new(accesskit::Role::Label);
        status.set_label(self.status.as_str());
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
        // Wait for the first decode so the snapshot shows the image.
        if let Ok(result) = app
            .decoder
            .results
            .recv_timeout(std::time::Duration::from_secs(10))
        {
            let (seq, path, decoded) = result;
            let _ = seq;
            match decoded {
                Ok(decoded) => {
                    app.pending = None;
                    app.set_image(Arc::new(decoded));
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
}
