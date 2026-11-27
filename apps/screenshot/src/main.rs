//! SLOPOS Screenshot: captures the whole screen, a window or a dragged region
//! and saves it as PNG. Recording is not available in this build.

mod capture;
mod geometry;
mod model;
mod paths;

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use image::RgbaImage;
use slopos_ui::accessibility::text_label;
use slopos_ui::controls::button::{Button, ButtonOutcome};
use slopos_ui::event::{AccessibilityRequest, Event, Key, PointerButton};
use slopos_ui::foundation::color::Color;
use slopos_ui::foundation::geometry::Rect;
use slopos_ui::image::Image;
use slopos_ui::platform::{self, App, Control, WindowOptions};
use slopos_ui::primitives::button_plate::ButtonPlateVariant;
use slopos_ui::render::RasterSurface;
use slopos_ui::text::Font;

use capture::{Backend, CaptureError, X11Backend};
use geometry::{fit_within, Preview};
use model::{Delay, Mode};

const WINDOW_BACKGROUND: Color = Color {
    red: 0xdd,
    green: 0xdd,
    blue: 0xdd,
    alpha: 0xff,
};

const MARGIN: i32 = 12;
const GAP: i32 = 8;
const WIDTH: u32 = 420;
const HEIGHT: u32 = 290;
/// Largest preview of a captured screen in the region picker.
const PICKER_MAX: (u32, u32) = (1024, 720);
/// Height of the picker's button row under the preview.
const TOOLBAR: u32 = 44;

const COUNTDOWN_TIMER: u64 = 1;
const SETTLE_TIMER: u64 = 2;
const SECOND: Duration = Duration::from_secs(1);
/// Lets the window manager unmap this app's windows before the screen is read.
const SETTLE: Duration = Duration::from_millis(250);

const ROOT_NODE: u64 = 0;
const STATUS_NODE: u64 = 1;
const NOTICE_NODE: u64 = 2;
const PREVIEW_NODE: u64 = 3;
const FIRST_CONTROL_ID: u64 = 10;

const NOTICE: &str = "Recording is not available in this build";
const REGION_HINT: &str = "Drag over the screen to select a region";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Mode(Mode),
    Delay(Delay),
    Take,
    SaveSelection,
    CancelRegion,
}

struct Item {
    button: Button,
    action: Action,
    accessible_name: &'static str,
}

impl Item {
    fn new(
        id: u64,
        label: &str,
        accessible_name: &'static str,
        action: Action,
        bounds: Rect,
    ) -> Self {
        Self {
            button: Button::new(id, label, ButtonPlateVariant::Regular).with_bounds(bounds),
            action,
            accessible_name,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Idle,
    /// Seconds left before the capture; the countdown timer is running.
    Countdown {
        remaining: u32,
    },
    /// App windows are hidden and the settle timer is running.
    Settling {
        mode: Mode,
    },
    /// The region picker is showing a captured screen.
    Picking,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Drag {
    anchor: (i32, i32),
    cursor: (i32, i32),
}

/// The region picker: a captured screen scaled to fit, with a drag selection.
struct Picker {
    screen: RgbaImage,
    preview: Preview,
    preview_image: Image,
    items: Vec<Item>,
    drag: Option<Drag>,
    dragging: bool,
}

impl Picker {
    /// The selected rectangle in screen pixels, if the drag covers any area.
    fn selection(&self) -> Option<Rect> {
        let drag = self.drag?;
        self.preview.selection(drag.anchor, drag.cursor)
    }

    /// The selected rectangle in preview pixels, for drawing.
    fn outline(&self) -> Option<Rect> {
        let drag = self.drag?;
        Some(self.preview.outline(drag.anchor, drag.cursor))
    }
}

pub struct ScreenshotApp {
    backend: Box<dyn Backend>,
    pictures: Option<PathBuf>,
    main_items: Vec<Item>,
    mode: Mode,
    delay: Delay,
    phase: Phase,
    picker: Option<Picker>,
    hidden: Vec<u32>,
    status: String,
    focused: Option<usize>,
    font: Font,
}

impl ScreenshotApp {
    pub fn new(backend: Box<dyn Backend>, pictures: Option<PathBuf>) -> Self {
        let mut main_items = Vec::new();
        let third = (WIDTH as i32 - 2 * MARGIN - 2 * GAP) / 3;
        for (index, mode) in Mode::ALL.into_iter().enumerate() {
            let bounds = Rect {
                x: MARGIN + index as i32 * (third + GAP),
                y: 30,
                width: third as u32,
                height: 26,
            };
            let id = FIRST_CONTROL_ID + main_items.len() as u64;
            main_items.push(Item::new(
                id,
                mode.label(),
                mode.accessible_name(),
                Action::Mode(mode),
                bounds,
            ));
        }
        for (index, delay) in Delay::ALL.into_iter().enumerate() {
            let bounds = Rect {
                x: MARGIN + index as i32 * (third + GAP),
                y: 86,
                width: third as u32,
                height: 26,
            };
            let id = FIRST_CONTROL_ID + main_items.len() as u64;
            main_items.push(Item::new(
                id,
                delay.label(),
                delay.accessible_name(),
                Action::Delay(delay),
                bounds,
            ));
        }
        let id = FIRST_CONTROL_ID + main_items.len() as u64;
        main_items.push(Item::new(
            id,
            "Take Screenshot",
            "Take screenshot",
            Action::Take,
            Rect {
                x: (WIDTH as i32 - 200) / 2,
                y: 126,
                width: 200,
                height: 28,
            },
        ));
        let mut app = Self {
            backend,
            pictures,
            main_items,
            mode: Mode::Screen,
            delay: Delay::Immediate,
            phase: Phase::Idle,
            picker: None,
            hidden: Vec::new(),
            status: String::new(),
            focused: None,
            font: Font::system(13.0),
        };
        app.sync();
        app
    }

    fn items(&self) -> &[Item] {
        match &self.picker {
            Some(picker) => &picker.items,
            None => &self.main_items,
        }
    }

    fn items_mut(&mut self) -> &mut Vec<Item> {
        match &mut self.picker {
            Some(picker) => &mut picker.items,
            None => &mut self.main_items,
        }
    }

    fn is_idle(&self) -> bool {
        self.phase == Phase::Idle
    }

    /// Shows the selected mode and delay, and enables what the state allows.
    fn sync(&mut self) {
        let (mode, delay) = (self.mode, self.delay);
        let selection = self.picker.as_ref().and_then(Picker::selection).is_some();
        for item in self.items_mut() {
            match item.action {
                Action::Mode(choice) => {
                    item.button.variant = if choice == mode {
                        ButtonPlateVariant::Default
                    } else {
                        ButtonPlateVariant::Regular
                    };
                }
                Action::Delay(choice) => {
                    item.button.variant = if choice == delay {
                        ButtonPlateVariant::Default
                    } else {
                        ButtonPlateVariant::Regular
                    };
                }
                Action::SaveSelection => item.button.enabled = selection,
                Action::Take | Action::CancelRegion => {}
            }
        }
    }

    fn move_focus(&mut self, forward: bool) {
        let count = self.items().len();
        if count == 0 {
            return;
        }
        let next = match self.focused {
            Some(current) if forward => (current + 1) % count,
            Some(current) => (current + count - 1) % count,
            None if forward => 0,
            None => count - 1,
        };
        self.set_focus(Some(next));
    }

    /// Moves keyboard focus. Reports whether the visible focus changed.
    fn set_focus(&mut self, index: Option<usize>) -> bool {
        let mut changed = false;
        for (position, item) in self.items_mut().iter_mut().enumerate() {
            let focused = index == Some(position);
            if item.button.focused != focused {
                item.button.focused = focused;
                changed = true;
            }
        }
        self.focused = index;
        changed
    }

    fn action_for(&self, id: u64) -> Option<Action> {
        self.items()
            .iter()
            .find(|item| item.button.id == id && item.button.enabled)
            .map(|item| item.action)
    }

    fn run(&mut self, action: Action, control: &mut Control) {
        match action {
            Action::Mode(mode) => {
                if self.is_idle() {
                    self.mode = mode;
                }
            }
            Action::Delay(delay) => {
                if self.is_idle() {
                    self.delay = delay;
                }
            }
            Action::Take => self.take(control),
            Action::SaveSelection => self.save_selection(control),
            Action::CancelRegion => self.cancel_region(control),
        }
        self.sync();
        control.request_redraw();
    }

    fn take(&mut self, control: &mut Control) {
        if !self.is_idle() {
            return;
        }
        if self.pictures.is_none() {
            self.status = "No Pictures folder is available".into();
            return;
        }
        let seconds = self.delay.seconds();
        if seconds == 0 {
            self.begin_capture(control);
        } else {
            self.phase = Phase::Countdown { remaining: seconds };
            self.status = countdown_text(seconds);
            control.start_timer(SECOND, COUNTDOWN_TIMER);
        }
    }

    fn tick(&mut self, control: &mut Control) {
        let Phase::Countdown { remaining } = self.phase else {
            return;
        };
        if remaining > 1 {
            self.phase = Phase::Countdown {
                remaining: remaining - 1,
            };
            self.status = countdown_text(remaining - 1);
            control.start_timer(SECOND, COUNTDOWN_TIMER);
        } else {
            self.begin_capture(control);
        }
        control.request_redraw();
    }

    fn begin_capture(&mut self, control: &mut Control) {
        self.phase = Phase::Idle;
        match self.mode {
            Mode::Window => {
                // The app keeps its window: the user chose another one.
                match self.backend.grab_active_window() {
                    Ok(image) => self.save(&image),
                    Err(error) => self.status = failure(&error),
                }
            }
            Mode::Screen | Mode::Region => {
                self.hidden = self.backend.hide_own_windows();
                self.phase = Phase::Settling { mode: self.mode };
                self.status = "Taking screenshot...".into();
                control.start_timer(SETTLE, SETTLE_TIMER);
            }
        }
        control.request_redraw();
    }

    fn settled(&mut self, control: &mut Control) {
        let Phase::Settling { mode } = self.phase else {
            return;
        };
        let grab = self.backend.grab_screen();
        self.restore_windows();
        self.phase = Phase::Idle;
        match grab {
            Err(error) => self.status = failure(&error),
            Ok(screen) if mode == Mode::Region => {
                if let Err(error) = self.enter_picker(screen, control) {
                    self.status = error;
                }
            }
            Ok(screen) => self.save(&screen),
        }
        self.sync();
        control.request_redraw();
    }

    fn restore_windows(&mut self) {
        if !self.hidden.is_empty() {
            let windows = std::mem::take(&mut self.hidden);
            self.backend.show_windows(&windows);
        }
    }

    fn save(&mut self, image: &RgbaImage) {
        let Some(dir) = self.pictures.clone() else {
            self.status = "No Pictures folder is available".into();
            return;
        };
        self.status = match paths::save_png(image, &dir, unix_now()) {
            Ok(path) => format!(
                "Saved {}",
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default()
            ),
            Err(error) => format!("Could not save the screenshot: {error}"),
        };
    }

    fn enter_picker(&mut self, screen: RgbaImage, control: &mut Control) -> Result<(), String> {
        let (width, height) = screen.dimensions();
        let shown = fit_within((width, height), PICKER_MAX);
        let preview = Preview::new((width, height), shown).ok_or("The screen has no size")?;
        let preview_image = Image::from_rgba(width, height, screen.as_raw().clone())
            .and_then(|image| image.scaled(shown.0, shown.1))
            .map_err(|error| format!("Could not show the screen: {error}"))?;
        let toolbar_y = shown.1 as i32 + 9;
        let items = vec![
            Item::new(
                FIRST_CONTROL_ID,
                "Save selection",
                "Save selection",
                Action::SaveSelection,
                Rect {
                    x: MARGIN,
                    y: toolbar_y,
                    width: 150,
                    height: 26,
                },
            ),
            Item::new(
                FIRST_CONTROL_ID + 1,
                "Cancel",
                "Cancel region selection",
                Action::CancelRegion,
                Rect {
                    x: MARGIN + 150 + GAP,
                    y: toolbar_y,
                    width: 100,
                    height: 26,
                },
            ),
        ];
        control.set_geometry(Rect {
            x: 0,
            y: 0,
            width: shown.0.max(WIDTH),
            height: shown.1 + TOOLBAR,
        });
        self.picker = Some(Picker {
            screen,
            preview,
            preview_image,
            items,
            drag: None,
            dragging: false,
        });
        self.phase = Phase::Picking;
        self.focused = None;
        self.status = REGION_HINT.into();
        Ok(())
    }

    fn leave_picker(&mut self, control: &mut Control) {
        self.picker = None;
        self.phase = Phase::Idle;
        self.focused = None;
        control.set_geometry(Rect {
            x: 0,
            y: 0,
            width: WIDTH,
            height: HEIGHT,
        });
    }

    fn save_selection(&mut self, control: &mut Control) {
        let Some(picker) = self.picker.as_ref() else {
            return;
        };
        let Some(rect) = picker.selection() else {
            self.status = REGION_HINT.into();
            return;
        };
        let cropped = image::imageops::crop_imm(
            &picker.screen,
            rect.x as u32,
            rect.y as u32,
            rect.width,
            rect.height,
        )
        .to_image();
        self.leave_picker(control);
        self.save(&cropped);
        self.sync();
        control.request_redraw();
    }

    fn cancel_region(&mut self, control: &mut Control) {
        if self.picker.is_none() {
            return;
        }
        self.leave_picker(control);
        self.status = "Region selection cancelled".into();
        self.sync();
        control.request_redraw();
    }

    /// Drag handling for the region picker. Reports whether the selection changed.
    fn drag(&mut self, event: &Event) -> bool {
        let Some(picker) = self.picker.as_mut() else {
            return false;
        };
        match *event {
            Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
            } if picker.preview.contains(x, y) => {
                picker.drag = Some(Drag {
                    anchor: (x, y),
                    cursor: (x, y),
                });
                picker.dragging = true;
                true
            }
            Event::PointerMoved { x, y } if picker.dragging => {
                if let Some(drag) = picker.drag.as_mut() {
                    drag.cursor = (x, y);
                }
                true
            }
            Event::PointerReleased {
                x,
                y,
                button: PointerButton::Primary,
            } if picker.dragging => {
                picker.dragging = false;
                if let Some(drag) = picker.drag.as_mut() {
                    drag.cursor = (x, y);
                }
                true
            }
            _ => false,
        }
    }

    fn paint_main(&self, surface: &RasterSurface) {
        let _ = surface.clear(WINDOW_BACKGROUND);
        self.text(surface, "Mode", MARGIN, 10);
        self.text(surface, "Delay", MARGIN, 66);
        for item in &self.main_items {
            let _ = item.button.paint(surface, &self.font);
        }
        self.text(surface, &self.status, MARGIN, 170);
        self.text(surface, NOTICE, MARGIN, 214);
        let folder = match &self.pictures {
            Some(path) => format!(
                "Saves to {}",
                paths::shorten(&path.display().to_string(), 52)
            ),
            None => "No Pictures folder is available".into(),
        };
        self.text(surface, &folder, MARGIN, 236);
    }

    fn paint_picker(&self, surface: &RasterSurface, picker: &Picker) {
        let _ = surface.clear(WINDOW_BACKGROUND);
        let _ = surface.draw_image(&picker.preview_image, 0, 0);
        if let Some(outline) = picker.outline() {
            let _ = surface.stroke_rect(outline, Color::BLACK);
            if outline.width > 2 && outline.height > 2 {
                let inner = Rect {
                    x: outline.x + 1,
                    y: outline.y + 1,
                    width: outline.width - 2,
                    height: outline.height - 2,
                };
                let _ = surface.stroke_rect(inner, Color::WHITE);
            }
        }
        for item in &picker.items {
            let _ = item.button.paint(surface, &self.font);
        }
        let size = match picker.selection() {
            Some(rect) => format!("{} x {}", rect.width, rect.height),
            None => "Drag to select".into(),
        };
        self.text(surface, &size, 282, picker.preview.shown.1 as i32 + 14);
    }

    fn text(&self, surface: &RasterSurface, text: &str, x: i32, y: i32) {
        let _ = surface.draw_text(text, x, y, &self.font, Color::BLACK);
    }
}

fn countdown_text(seconds: u32) -> String {
    if seconds == 1 {
        "Capturing in 1 second...".into()
    } else {
        format!("Capturing in {seconds} seconds...")
    }
}

/// What the user sees when a capture fails.
fn failure(error: &CaptureError) -> String {
    match error {
        CaptureError::OwnWindowActive => {
            "Screenshot is the active window. Click the window to capture first".into()
        }
        CaptureError::NoActiveWindow => "No window is active".into(),
        other => format!("Could not take the screenshot: {other}"),
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

impl App for ScreenshotApp {
    fn event(&mut self, event: &Event, control: &mut Control) {
        match event {
            Event::CloseRequested => {
                self.restore_windows();
                control.quit();
            }
            Event::Timer(COUNTDOWN_TIMER) => self.tick(control),
            Event::Timer(SETTLE_TIMER) => self.settled(control),
            Event::KeyPressed {
                key: Key::Escape, ..
            } if self.picker.is_some() => self.cancel_region(control),
            Event::KeyPressed {
                key: Key::Tab,
                modifiers,
            } => {
                self.move_focus(!modifiers.shift);
                control.request_redraw();
            }
            Event::KeyPressed { key, .. } if matches!(key, Key::Enter | Key::Space) => {
                match (self.focused, key) {
                    (Some(index), _) => {
                        let item = &self.items()[index];
                        if item.button.enabled {
                            let action = item.action;
                            self.run(action, control);
                        }
                    }
                    (None, Key::Enter) if self.picker.is_some() => self.save_selection(control),
                    _ => {}
                }
            }
            Event::Accessibility(AccessibilityRequest::Click(id)) => {
                if let Some(action) = self.action_for(*id) {
                    self.run(action, control);
                }
            }
            Event::Accessibility(AccessibilityRequest::Focus(id)) => {
                let index = self.items().iter().position(|item| item.button.id == *id);
                if self.set_focus(index) {
                    control.request_redraw();
                }
            }
            _ => {
                let mut changed = self.drag(event);
                if changed {
                    self.sync();
                }
                let mut activated = None;
                for (index, item) in self.items_mut().iter_mut().enumerate() {
                    match item.button.handle(event) {
                        ButtonOutcome::Activated => activated = Some(index),
                        ButtonOutcome::Changed => changed = true,
                        ButtonOutcome::Ignored => {}
                    }
                }
                if let Some(index) = activated {
                    let action = self.items()[index].action;
                    self.run(action, control);
                }
                if changed {
                    control.request_redraw();
                }
            }
        }
    }

    fn paint(&mut self, surface: &RasterSurface) {
        match &self.picker {
            Some(picker) => self.paint_picker(surface, picker),
            None => self.paint_main(surface),
        }
    }

    fn accessibility(&self) -> Option<accesskit::TreeUpdate> {
        use accesskit::{Node, NodeId, Role, Toggled, TreeId, TreeInfo, TreeUpdate};
        let mut root = Node::new(Role::Window);
        root.set_label("Screenshot");
        let mut nodes = vec![(NodeId(STATUS_NODE), text_label(self.status.clone()))];
        let mut children = vec![NodeId(STATUS_NODE)];
        match &self.picker {
            Some(_) => {
                let mut preview = Node::new(Role::Image);
                preview.set_label("Captured screen. Drag over it to select a region");
                nodes.push((NodeId(PREVIEW_NODE), preview));
                children.push(NodeId(PREVIEW_NODE));
            }
            None => {
                nodes.push((NodeId(NOTICE_NODE), text_label(NOTICE)));
                children.push(NodeId(NOTICE_NODE));
            }
        }
        for item in self.items() {
            let mut node = item.button.accessibility_node();
            node.set_label(item.accessible_name);
            match item.action {
                Action::Mode(mode) => node.set_toggled(if mode == self.mode {
                    Toggled::True
                } else {
                    Toggled::False
                }),
                Action::Delay(delay) => node.set_toggled(if delay == self.delay {
                    Toggled::True
                } else {
                    Toggled::False
                }),
                _ => {}
            }
            nodes.push((NodeId(item.button.id), node));
            children.push(NodeId(item.button.id));
        }
        root.set_children(children);
        nodes.insert(0, (NodeId(ROOT_NODE), root));
        let focus = self
            .focused
            .map(|index| NodeId(self.items()[index].button.id))
            .unwrap_or(NodeId(ROOT_NODE));
        Some(TreeUpdate {
            nodes,
            tree: Some(TreeInfo::new(NodeId(ROOT_NODE))),
            tree_id: TreeId::ROOT,
            focus,
        })
    }
}

/// The Pictures folder for this user, from the environment and `user-dirs.dirs`.
fn pictures_folder() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|home| home.join(".config")));
    let user_dirs = config.and_then(|dir| std::fs::read_to_string(dir.join("user-dirs.dirs")).ok());
    paths::pictures_dir(
        std::env::var("XDG_PICTURES_DIR").ok().as_deref(),
        user_dirs.as_deref(),
        home.as_deref(),
    )
}

/// `--capture-screen FILE`: write the whole screen to FILE as PNG and exit.
/// For scripts and QA; it does not show a window.
fn capture_screen_to(path: &Path) -> Result<(), String> {
    let image = X11Backend
        .grab_screen()
        .map_err(|error| error.to_string())?;
    image
        .save_with_format(path, image::ImageFormat::Png)
        .map_err(|error| error.to_string())
}

fn main() {
    env_logger_init();
    if let Some(path) = argument("--capture-screen") {
        if let Err(error) = capture_screen_to(Path::new(&path)) {
            eprintln!("slopos-screenshot: {error}");
            std::process::exit(1);
        }
        return;
    }
    let mut options = WindowOptions::new("Screenshot", capture::APP_ID, WIDTH, HEIGHT);
    options.resizable = false;
    let app = ScreenshotApp::new(Box::new(X11Backend), pictures_folder());
    if let Err(error) = platform::run(options, app, |_| {}) {
        eprintln!("slopos-screenshot: {error}");
        std::process::exit(1);
    }
}

fn argument(name: &str) -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == name {
            return args.next();
        }
    }
    None
}

fn env_logger_init() {
    if std::env::var_os("RUST_LOG").is_some() {
        let _ = env_logger::try_init();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::GenericImageView;
    use slopos_ui::event::{Modifiers, PointerButton};
    use std::cell::RefCell;
    use std::rc::Rc;

    type ShownLog = Rc<RefCell<Vec<u32>>>;

    /// A backend with a fixed screen and window. The app owns it, so the
    /// windows it showed are logged in a shared cell that tests keep.
    struct FakeBackend {
        screen: RgbaImage,
        window: Result<RgbaImage, CaptureError>,
        screen_error: Option<CaptureError>,
        shown: ShownLog,
    }

    impl FakeBackend {
        fn new(width: u32, height: u32) -> Self {
            Self {
                screen: RgbaImage::from_pixel(width, height, image::Rgba([200, 30, 30, 255])),
                window: Ok(RgbaImage::from_pixel(40, 30, image::Rgba([1, 2, 3, 255]))),
                screen_error: None,
                shown: ShownLog::default(),
            }
        }
    }

    impl Backend for FakeBackend {
        fn grab_screen(&mut self) -> Result<RgbaImage, CaptureError> {
            match self.screen_error.take() {
                Some(error) => Err(error),
                None => Ok(self.screen.clone()),
            }
        }

        fn grab_active_window(&mut self) -> Result<RgbaImage, CaptureError> {
            self.window.clone()
        }

        fn hide_own_windows(&mut self) -> Vec<u32> {
            vec![7]
        }

        fn show_windows(&mut self, windows: &[u32]) {
            self.shown.borrow_mut().extend_from_slice(windows);
        }
    }

    /// Creates a temporary Pictures folder and an app that saves into it.
    fn app_with(backend: FakeBackend) -> (ScreenshotApp, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let app = ScreenshotApp::new(Box::new(backend), Some(dir.path().to_path_buf()));
        (app, dir)
    }

    fn press(app: &mut ScreenshotApp, control: &mut Control, label: &str) {
        let bounds = app
            .items()
            .iter()
            .find(|item| item.button.label == label)
            .map(|item| item.button.bounds)
            .unwrap_or_else(|| panic!("no control labelled {label}"));
        let (x, y) = (bounds.x + 5, bounds.y + 5);
        app.event(&Event::PointerMoved { x, y }, control);
        app.event(
            &Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
            },
            control,
        );
        app.event(
            &Event::PointerReleased {
                x,
                y,
                button: PointerButton::Primary,
            },
            control,
        );
    }

    fn saved_files(dir: &tempfile::TempDir) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        files.sort();
        files
    }

    fn timer(app: &mut ScreenshotApp, id: u64) {
        app.event(&Event::Timer(id), &mut Control::default());
    }

    #[test]
    fn mode_and_delay_buttons_choose_the_capture() {
        let (mut app, _dir) = app_with(FakeBackend::new(200, 100));
        let mut control = Control::default();
        press(&mut app, &mut control, "Window");
        press(&mut app, &mut control, "10 seconds");
        assert_eq!(app.mode, Mode::Window);
        assert_eq!(app.delay, Delay::Ten);
        press(&mut app, &mut control, "Region");
        assert_eq!(app.mode, Mode::Region);
    }

    #[test]
    fn window_mode_saves_the_active_window_as_png() {
        let mut backend = FakeBackend::new(200, 100);
        backend.window = Ok(RgbaImage::from_pixel(40, 30, image::Rgba([9, 9, 9, 255])));
        let (mut app, dir) = app_with(backend);
        let mut control = Control::default();
        press(&mut app, &mut control, "Window");
        press(&mut app, &mut control, "Take Screenshot");
        let files = saved_files(&dir);
        assert_eq!(files.len(), 1);
        assert_eq!(image::open(&files[0]).unwrap().dimensions(), (40, 30));
        assert!(app.status.starts_with("Saved Screenshot-"));
        assert!(app.hidden.is_empty(), "window mode must not hide the app");
    }

    #[test]
    fn the_screenshot_window_itself_is_not_captured_in_window_mode() {
        let mut backend = FakeBackend::new(200, 100);
        backend.window = Err(CaptureError::OwnWindowActive);
        let (mut app, dir) = app_with(backend);
        let mut control = Control::default();
        press(&mut app, &mut control, "Window");
        press(&mut app, &mut control, "Take Screenshot");
        assert!(saved_files(&dir).is_empty());
        assert!(app.status.contains("Click the window to capture first"));
    }

    #[test]
    fn delayed_screen_capture_counts_down_then_hides_and_saves() {
        let (mut app, dir) = app_with(FakeBackend::new(200, 100));
        let mut control = Control::default();
        press(&mut app, &mut control, "Screen");
        press(&mut app, &mut control, "3 seconds");
        press(&mut app, &mut control, "Take Screenshot");
        assert_eq!(app.phase, Phase::Countdown { remaining: 3 });
        timer(&mut app, COUNTDOWN_TIMER);
        assert_eq!(app.phase, Phase::Countdown { remaining: 2 });
        timer(&mut app, COUNTDOWN_TIMER);
        assert_eq!(app.phase, Phase::Countdown { remaining: 1 });
        timer(&mut app, COUNTDOWN_TIMER);
        assert_eq!(app.phase, Phase::Settling { mode: Mode::Screen });
        assert_eq!(
            app.hidden,
            vec![7],
            "the app windows are hidden while settling"
        );
        timer(&mut app, SETTLE_TIMER);
        assert_eq!(app.phase, Phase::Idle);
        assert_eq!(saved_files(&dir).len(), 1);
        assert!(app.hidden.is_empty());
    }

    #[test]
    fn a_failed_screen_grab_restores_the_app_windows() {
        let shown = ShownLog::default();
        let mut backend = FakeBackend::new(200, 100);
        backend.screen_error = Some(CaptureError::Display("no server".into()));
        backend.shown = shown.clone();
        let (mut app, dir) = app_with(backend);
        let mut control = Control::default();
        press(&mut app, &mut control, "Take Screenshot");
        timer(&mut app, SETTLE_TIMER);
        assert!(saved_files(&dir).is_empty());
        assert_eq!(app.phase, Phase::Idle);
        assert!(app.status.contains("no server"));
        assert_eq!(*shown.borrow(), vec![7]);
    }

    #[test]
    fn taking_a_screenshot_without_a_pictures_folder_is_refused() {
        let mut app = ScreenshotApp::new(Box::new(FakeBackend::new(200, 100)), None);
        press(&mut app, &mut Control::default(), "Take Screenshot");
        assert_eq!(app.phase, Phase::Idle);
        assert_eq!(app.status, "No Pictures folder is available");
    }

    #[test]
    fn region_drag_crops_the_selected_pixels() {
        let (mut app, dir) = app_with(FakeBackend::new(200, 100));
        let mut control = Control::default();
        press(&mut app, &mut control, "Region");
        press(&mut app, &mut control, "Take Screenshot");
        timer(&mut app, SETTLE_TIMER);
        assert_eq!(app.phase, Phase::Picking);
        // The picker shows the whole 200x100 screen unscaled, so preview points map 1:1.
        app.event(
            &Event::PointerPressed {
                x: 10,
                y: 10,
                button: PointerButton::Primary,
            },
            &mut control,
        );
        app.event(&Event::PointerMoved { x: 60, y: 40 }, &mut control);
        app.event(
            &Event::PointerReleased {
                x: 60,
                y: 40,
                button: PointerButton::Primary,
            },
            &mut control,
        );
        let save = app.picker.as_ref().unwrap().items[0].button.enabled;
        assert!(save, "save should be enabled once a region is selected");
        press(&mut app, &mut control, "Save selection");
        let files = saved_files(&dir);
        assert_eq!(files.len(), 1);
        assert_eq!(image::open(&files[0]).unwrap().dimensions(), (50, 30));
        assert_eq!(app.phase, Phase::Idle);
        assert!(app.picker.is_none());
    }

    #[test]
    fn escape_cancels_the_region_picker_without_saving() {
        let (mut app, dir) = app_with(FakeBackend::new(200, 100));
        let mut control = Control::default();
        press(&mut app, &mut control, "Region");
        press(&mut app, &mut control, "Take Screenshot");
        timer(&mut app, SETTLE_TIMER);
        app.event(
            &Event::KeyPressed {
                key: Key::Escape,
                modifiers: Modifiers::default(),
            },
            &mut control,
        );
        assert!(app.picker.is_none());
        assert_eq!(app.status, "Region selection cancelled");
        assert!(saved_files(&dir).is_empty());
    }

    #[test]
    fn a_click_without_a_drag_does_not_enable_saving() {
        let (mut app, _dir) = app_with(FakeBackend::new(200, 100));
        let mut control = Control::default();
        press(&mut app, &mut control, "Region");
        press(&mut app, &mut control, "Take Screenshot");
        timer(&mut app, SETTLE_TIMER);
        let (x, y) = (30, 30);
        app.event(
            &Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
            },
            &mut control,
        );
        app.event(
            &Event::PointerReleased {
                x,
                y,
                button: PointerButton::Primary,
            },
            &mut control,
        );
        assert!(!app.picker.as_ref().unwrap().items[0].button.enabled);
    }

    #[test]
    fn keyboard_moves_focus_and_space_activates_it() {
        let (mut app, _dir) = app_with(FakeBackend::new(200, 100));
        let mut control = Control::default();
        let tab = Event::KeyPressed {
            key: Key::Tab,
            modifiers: Modifiers::default(),
        };
        app.event(&tab, &mut control);
        assert_eq!(app.focused, Some(0));
        app.event(&tab, &mut control);
        assert_eq!(app.focused, Some(1));
        app.event(
            &Event::KeyPressed {
                key: Key::Space,
                modifiers: Modifiers::default(),
            },
            &mut control,
        );
        assert_eq!(app.mode, Mode::Window);
    }

    #[test]
    fn accessibility_names_every_control_and_reports_the_choices() {
        let (mut app, _dir) = app_with(FakeBackend::new(200, 100));
        press(&mut app, &mut Control::default(), "Window");
        let tree = app.accessibility().expect("tree");
        let nodes: Vec<&accesskit::Node> = tree.nodes.iter().map(|(_, node)| node).collect();
        for node in &nodes {
            if node.role() == accesskit::Role::Button {
                assert!(node.label().is_some(), "every button needs a label");
            }
        }
        let window = tree
            .nodes
            .iter()
            .find(|(id, _)| *id == accesskit::NodeId(FIRST_CONTROL_ID + 1))
            .map(|(_, node)| node)
            .expect("window mode node");
        assert_eq!(window.toggled(), Some(accesskit::Toggled::True));
        assert!(nodes.iter().any(|node| node.value() == Some(NOTICE)));
    }

    #[test]
    fn recording_is_stated_not_offered() {
        let (app, _dir) = app_with(FakeBackend::new(200, 100));
        let tree = app.accessibility().expect("tree");
        assert!(tree
            .nodes
            .iter()
            .all(|(_, node)| node.label().is_none_or(|label| !label.contains("ecord"))));
        assert!(tree
            .nodes
            .iter()
            .any(|(_, node)| node.value() == Some(NOTICE)));
    }

    #[test]
    fn closing_while_windows_are_hidden_shows_them_again() {
        let shown = ShownLog::default();
        let mut backend = FakeBackend::new(200, 100);
        backend.shown = shown.clone();
        let (mut app, _dir) = app_with(backend);
        let mut control = Control::default();
        press(&mut app, &mut control, "Take Screenshot");
        assert_eq!(app.phase, Phase::Settling { mode: Mode::Screen });
        app.event(&Event::CloseRequested, &mut control);
        assert_eq!(*shown.borrow(), vec![7]);
    }
}
