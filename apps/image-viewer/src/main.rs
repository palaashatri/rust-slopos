//! Initial native SLOPOS Image Viewer. All visible controls are slopos-ui.
//! Supported inputs in this first proof: PNG and SVG. Other formats must
//! receive a reviewed, bounded decoder before being advertised as supported.

use std::path::{Path, PathBuf};

use slopos_ui::controls::button::{Button, ButtonOutcome};
use slopos_ui::event::{AccessibilityRequest, Event, Key, PointerButton};
use slopos_ui::foundation::{color::Color, geometry::Rect};
use slopos_ui::image::Image;
use slopos_ui::image_viewport::ImageViewport;
use slopos_ui::platform::{self, App, Control, WindowOptions};
use slopos_ui::primitives::button_plate::ButtonPlateVariant;
use slopos_ui::render::RasterSurface;
use slopos_ui::text::Font;

const WIDTH: u32 = 640;
const HEIGHT: u32 = 480;
const MAX_ENCODED_BYTES: u64 = 24 * 1024 * 1024;

const BODY: Color = Color { red: 0xdd, green: 0xdd, blue: 0xdd, alpha: 255 };
const CANVAS: Color = Color { red: 0x30, green: 0x32, blue: 0x37, alpha: 255 };
const STATUS: Color = Color { red: 0xf1, green: 0xf1, blue: 0xf1, alpha: 255 };

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Fit,
    Actual,
    ZoomIn,
    ZoomOut,
}

struct ToolbarButton {
    control: Button,
    action: Action,
}

pub struct ImageViewerApp {
    image: Option<Image>,
    title: String,
    status: String,
    viewport: ImageViewport,
    buttons: Vec<ToolbarButton>,
    focused: Option<usize>,
    width: u32,
    height: u32,
}

impl ImageViewerApp {
    pub fn new(path: Option<&Path>) -> Self {
        let (image, title, status) = match path {
            Some(path) => match load_bounded(path) {
                Ok(image) => {
                    let title = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                    let status = format!("{title}  —  {} × {} px", image.width(), image.height());
                    (Some(image), title, status)
                }
                Err(error) => (None, "Image Viewer".into(), format!("Cannot open image: {error}")),
            },
            None => (
                None,
                "Image Viewer".into(),
                "Open a PNG or SVG file using the command-line argument or file association.".into(),
            ),
        };
        let size = image.as_ref().map(|i| (i.width(), i.height())).unwrap_or((0, 0));
        let mut buttons = Vec::new();
        for (index, (label, action)) in [
            ("Fit", Action::Fit),
            ("1:1", Action::Actual),
            ("+", Action::ZoomIn),
            ("-", Action::ZoomOut),
        ]
        .into_iter()
        .enumerate()
        {
            let bounds = Rect {
                x: 12 + 60 * index as i32,
                y: 10,
                width: 54,
                height: 28,
            };
            buttons.push(ToolbarButton {
                control: Button::new(10 + index as u64, label, ButtonPlateVariant::Regular)
                    .with_bounds(bounds),
                action,
            });
        }
        Self {
            image,
            title,
            status,
            viewport: ImageViewport::new(size.0, size.1, canvas_area(WIDTH, HEIGHT)),
            buttons,
            focused: None,
            width: WIDTH,
            height: HEIGHT,
        }
    }

    fn apply(&mut self, action: Action, control: &mut Control) {
        if self.image.is_none() {
            return;
        }
        match action {
            Action::Fit => self.viewport.fit(),
            Action::Actual => self.viewport.actual_pixels(),
            Action::ZoomIn => { self.viewport.zoom_by(1.25); }
            Action::ZoomOut => { self.viewport.zoom_by(0.8); }
        }
        control.request_redraw();
    }

    fn set_focus(&mut self, focused: Option<usize>) {
        self.focused = focused;
        for (index, button) in self.buttons.iter_mut().enumerate() {
            button.control.focused = Some(index) == focused;
        }
    }
}

fn canvas_area(width: u32, height: u32) -> Rect {
    Rect {
        x: 12,
        y: 48,
        width: width.saturating_sub(24),
        height: height.saturating_sub(84),
    }
}

fn load_bounded(path: &Path) -> Result<Image, String> {
    let metadata = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("not a regular file".into());
    }
    if metadata.len() > MAX_ENCODED_BYTES {
        return Err("encoded image is larger than the 24 MiB proof-app limit".into());
    }
    let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    if !["png", "svg"].iter().any(|s| extension.eq_ignore_ascii_case(s)) {
        return Err("this initial viewer supports PNG and SVG only".into());
    }
    Image::load(path, 1024).map_err(|e| e.to_string())
}

impl App for ImageViewerApp {
    fn event(&mut self, event: &Event, control: &mut Control) {
        match event {
            Event::CloseRequested => control.quit(),
            Event::Resized { width, height } => {
                self.width = *width;
                self.height = *height;
                self.viewport.resize(canvas_area(*width, *height));
                control.request_redraw();
            }
            Event::KeyPressed { key: Key::Tab, modifiers } => {
                let len = self.buttons.len();
                let index = self.focused.map_or(0, |old| {
                    if modifiers.shift { (old + len - 1) % len } else { (old + 1) % len }
                });
                self.set_focus(Some(index));
                control.request_redraw();
            }
            Event::KeyPressed { key, modifiers } if !modifiers.control && !modifiers.alt => {
                let action = match key {
                    Key::Character('f' | 'F') => Some(Action::Fit),
                    Key::Character('0') => Some(Action::Actual),
                    Key::Character('+' | '=') => Some(Action::ZoomIn),
                    Key::Character('-') => Some(Action::ZoomOut),
                    Key::Enter | Key::Space => self.focused.map(|index| self.buttons[index].action),
                    _ => None,
                };
                if let Some(action) = action {
                    self.apply(action, control);
                }
            }
            Event::Accessibility(AccessibilityRequest::Click(id)) => {
                if let Some(action) = self.buttons.iter().find(|b| b.control.id == *id).map(|b| b.action) {
                    self.apply(action, control);
                }
            }
            Event::Accessibility(AccessibilityRequest::Focus(id)) => {
                if let Some(index) = self.buttons.iter().position(|b| b.control.id == *id) {
                    self.set_focus(Some(index));
                    control.request_redraw();
                }
            }
            Event::PointerPressed { button: PointerButton::Primary, .. }
            | Event::PointerReleased { button: PointerButton::Primary, .. }
            | Event::PointerMoved { .. } | Event::PointerLeft => {
                let mut action = None;
                for button in &mut self.buttons {
                    match button.control.handle(event) {
                        ButtonOutcome::Activated => action = Some(button.action),
                        ButtonOutcome::Changed => control.request_redraw(),
                        ButtonOutcome::Ignored => {}
                    }
                }
                if let Some(action) = action { self.apply(action, control); }
            }
            _ => {}
        }
    }

    fn paint(&mut self, surface: &RasterSurface) {
        let _ = surface.clear(BODY);
        let area = canvas_area(self.width, self.height);
        let _ = surface.fill_rect(area, CANVAS);
        if let (Some(image), Some(dest)) = (&self.image, self.viewport.image_bounds()) {
            let _ = surface.draw_image_scaled(image, dest.x, dest.y, dest.width, dest.height);
        } else {
            let _ = surface.draw_text("No image loaded", 22, 70, &Font::system(14.0), Color::WHITE);
        }
        let bar = Rect { x: 0, y: 0, width: self.width, height: 44 };
        let _ = surface.fill_rect(bar, BODY);
        let _ = surface.stroke_rect(Rect { x: 0, y: 42, width: self.width, height: 1 }, Color::BLACK);
        let footer = Rect { x: 0, y: self.height.saturating_sub(30) as i32, width: self.width, height: 30 };
        let _ = surface.fill_rect(footer, STATUS);
        let _ = surface.draw_text(&self.status, 12, self.height.saturating_sub(24) as i32, &Font::system(12.0), Color::BLACK);
        for button in &self.buttons {
            let _ = button.control.paint(surface, &Font::system(13.0));
        }
    }

    fn accessibility(&self) -> Option<accesskit::TreeUpdate> {
        use accesskit::{Node, NodeId, Role, TreeId, TreeInfo, TreeUpdate};
        let mut root = Node::new(Role::Window);
        root.set_label(format!("Image Viewer — {}", self.title));
        let mut status = Node::new(Role::Label);
        status.set_label(self.status.clone());
        let mut nodes = vec![(NodeId(1), status)];
        let mut children = vec![NodeId(1)];
        for button in &self.buttons {
            let node = button.control.accessibility_node();
            children.push(NodeId(button.control.id));
            nodes.push((NodeId(button.control.id), node));
        }
        root.set_children(children);
        nodes.insert(0, (NodeId(0), root));
        Some(TreeUpdate {
            nodes,
            tree: Some(TreeInfo::new(NodeId(0))),
            tree_id: TreeId::ROOT,
            focus: self.focused.map(|i| NodeId(self.buttons[i].control.id)).unwrap_or(NodeId(0)),
        })
    }
}

fn main() {
    if std::env::var_os("RUST_LOG").is_some() {
        let _ = env_logger::try_init();
    }
    let mut args = std::env::args_os().skip(1);
    let first = args.next();
    let (snapshot, path): (Option<PathBuf>, Option<PathBuf>) = if first.as_deref() == Some(std::ffi::OsStr::new("--snapshot")) {
        (args.next().map(Into::into), args.next().map(Into::into))
    } else {
        (None, first.map(Into::into))
    };
    let app = ImageViewerApp::new(path.as_deref());
    if let Some(output) = snapshot {
        let surface = RasterSurface::new(WIDTH, HEIGHT, platform::scale_from_environment()).expect("surface");
        let mut app = app;
        app.paint(&surface);
        surface.save_png(&output).expect("save PNG snapshot");
        return;
    }
    let options = WindowOptions::new("Image Viewer", "slopos-image-viewer", WIDTH, HEIGHT);
    if let Err(error) = platform::run(options, app, |_| {}) {
        eprintln!("slopos-image-viewer: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_move_focus_and_publish_accessibility() {
        let mut app = ImageViewerApp::new(None);
        let mut ctrl = Control::default();
        app.event(&Event::KeyPressed { key: Key::Tab, modifiers: Default::default() }, &mut ctrl);
        assert_eq!(app.focused, Some(0));
        let update = app.accessibility().unwrap();
        assert_eq!(update.nodes.len(), 6);
        assert_eq!(update.focus, accesskit::NodeId(app.buttons[0].control.id));
    }

    #[test]
    fn missing_file_reports_error_in_accessible_status() {
        let app = ImageViewerApp::new(Some(Path::new("/not/a/real/image.png")));
        assert!(app.image.is_none());
        assert!(app.status.contains("Cannot open image"));
    }

    #[test]
    fn paint_does_not_require_a_window_server() {
        let mut app = ImageViewerApp::new(None);
        let surface = RasterSurface::new(WIDTH, HEIGHT, slopos_ui::foundation::geometry::Scale::new(1).unwrap()).unwrap();
        app.paint(&surface);
        assert!(surface.pixel(0, 0).is_some());
    }

    #[test]
    fn invalid_extensions_fail_closed() {
        let dir = std::env::temp_dir();
        assert!(load_bounded(&dir).is_err());
    }
}
