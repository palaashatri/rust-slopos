//! SLOPOS Dock: pinned and running applications along the bottom edge.
//!
//! Click a tile to start the application or bring its window forward; click
//! again to cycle through its windows. Middle-click starts another instance.
//! Pins live in `~/.config/slopos-i/dock-pins`.

mod model;
mod pins;
mod windows;

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver};

use model::{Action, Item};
use slopos_core::desktop_entry::{scan_desktop_apps, DesktopApp};
use slopos_ui::event::{AccessibilityRequest, Event, PointerButton};
use slopos_ui::foundation::color::Color;
use slopos_ui::foundation::geometry::Rect;
use slopos_ui::icons::IconLookup;
use slopos_ui::image::Image;
use slopos_ui::platform::{self, App, Control, Edge, WindowKind, WindowOptions};
use slopos_ui::render::RasterSurface;
use slopos_ui::text::{measure, Font};
use windows::Snapshot;

const APP_ID: &str = "slopos-dock";
const ICON: u32 = 40;
const TILE: u32 = 52;
const PADDING: u32 = 6;
const INDICATOR: u32 = 6;
const HEIGHT: u32 = PADDING + TILE + INDICATOR + 2;
/// Gap between pinned and unpinned (running-only) tiles.
const SEPARATOR: u32 = 9;

const BODY: Color = rgb(0xdd, 0xdd, 0xdd);
const LIGHT: Color = rgb(0xff, 0xff, 0xff);
const SHADOW: Color = rgb(0x88, 0x88, 0x88);
const ACTIVE_TILE: Color = rgb(0xbb, 0xbb, 0xbb);
const HOVER_TILE: Color = rgb(0xcc, 0xcc, 0xcc);
const ATTENTION: Color = rgb(0xd0, 0x70, 0x00);

const fn rgb(red: u8, green: u8, blue: u8) -> Color {
    Color {
        red,
        green,
        blue,
        alpha: 0xff,
    }
}

const ROOT_NODE: u64 = 1;
const FIRST_TILE_NODE: u64 = 100;

struct Dock {
    catalog: Vec<DesktopApp>,
    pins: Vec<DesktopApp>,
    snapshot: Snapshot,
    items: Vec<Item>,
    updates: Receiver<Snapshot>,
    icons: IconLookup,
    /// Icons by name, at the current scale.
    icon_cache: HashMap<String, Option<Image>>,
    scale: u32,
    hovered: Option<usize>,
    pressed: Option<(usize, PointerButton)>,
    screen: (u32, u32),
    font: Font,
}

impl Dock {
    fn new(updates: Receiver<Snapshot>, screen: (u32, u32)) -> Self {
        let catalog = scan_desktop_apps();
        let pins = pins::load(&catalog);
        let mut dock = Self {
            catalog,
            pins,
            snapshot: Snapshot::default(),
            items: Vec::new(),
            updates,
            icons: IconLookup::from_environment(),
            icon_cache: HashMap::new(),
            scale: 1,
            hovered: None,
            pressed: None,
            screen,
            font: Font::system(18.0).bold(),
        };
        dock.rebuild();
        dock
    }

    fn rebuild(&mut self) {
        self.items = model::build_items(&self.pins, &self.snapshot.windows, &self.catalog);
        if self.hovered.is_some_and(|index| index >= self.items.len()) {
            self.hovered = None;
        }
    }

    fn width(&self) -> u32 {
        let separator = if self.has_separator() { SEPARATOR } else { 0 };
        2 * PADDING + TILE * self.items.len().max(1) as u32 + separator
    }

    fn has_separator(&self) -> bool {
        self.items.iter().any(|item| item.pinned) && self.items.iter().any(|item| !item.pinned)
    }

    /// Centred on the bottom edge of the screen.
    fn geometry(&self) -> Rect {
        let width = self.width();
        Rect {
            x: (self.screen.0.saturating_sub(width) / 2) as i32,
            y: self.screen.1.saturating_sub(HEIGHT) as i32,
            width,
            height: HEIGHT,
        }
    }

    fn tile_bounds(&self, index: usize) -> Rect {
        let pinned = self.items.iter().take_while(|item| item.pinned).count();
        let separator = if self.has_separator() && index >= pinned {
            SEPARATOR
        } else {
            0
        };
        Rect {
            x: (PADDING + separator + TILE * index as u32) as i32,
            y: PADDING as i32,
            width: TILE,
            height: TILE,
        }
    }

    fn tile_at(&self, x: i32, y: i32) -> Option<usize> {
        (0..self.items.len()).find(|index| self.tile_bounds(*index).contains(x, y))
    }

    fn perform(&self, action: Action) {
        let result = match action {
            Action::Launch(argv) => {
                let args: Vec<&str> = argv[1..].iter().map(String::as_str).collect();
                slopos_core::process::launch(&argv[0], &args).map(|_| ())
            }
            Action::Activate(window) => windows::activate(window),
            Action::Nothing => Ok(()),
        };
        if let Err(error) = result {
            log::warn!("Dock action failed: {error}");
        }
    }

    fn icon(&mut self, name: &str) -> Option<Image> {
        if !self.icon_cache.contains_key(name) {
            let image = self.icons.load(name, ICON, self.scale).or_else(|| {
                self.icons
                    .load("application-x-executable", ICON, self.scale)
            });
            self.icon_cache.insert(name.to_owned(), image);
        }
        self.icon_cache.get(name).cloned().flatten()
    }

    fn apply_updates(&mut self, control: &mut Control) {
        let mut latest = None;
        while let Ok(snapshot) = self.updates.try_recv() {
            latest = Some(snapshot);
        }
        let Some(snapshot) = latest else { return };
        let had = self.items.len();
        let had_separator = self.has_separator();
        self.snapshot = snapshot;
        self.rebuild();
        if self.items.len() != had || self.has_separator() != had_separator {
            control.set_geometry(self.geometry());
        }
        control.request_redraw();
    }
}

impl App for Dock {
    fn event(&mut self, event: &Event, control: &mut Control) {
        if let Some(scale) = control.scale() {
            if scale.factor() != self.scale {
                self.scale = scale.factor();
                self.icon_cache.clear();
            }
        }
        match event {
            Event::Wake => self.apply_updates(control),
            Event::CloseRequested => control.quit(),
            Event::PointerMoved { x, y } => {
                let hovered = self.tile_at(*x, *y);
                if hovered != self.hovered {
                    self.hovered = hovered;
                    control.request_redraw();
                }
            }
            Event::PointerLeft => {
                if self.hovered.take().is_some() || self.pressed.take().is_some() {
                    control.request_redraw();
                }
            }
            Event::PointerPressed { x, y, button } => {
                if let Some(index) = self.tile_at(*x, *y) {
                    self.pressed = Some((index, *button));
                    control.request_redraw();
                }
            }
            Event::PointerReleased { x, y, button } => {
                let pressed = self.pressed.take();
                control.request_redraw();
                let Some((index, pressed_button)) = pressed else {
                    return;
                };
                if pressed_button != *button || self.tile_at(*x, *y) != Some(index) {
                    return;
                }
                let item = &self.items[index];
                let action = match button {
                    PointerButton::Primary => model::click(item, self.snapshot.active),
                    PointerButton::Middle => model::launch(item),
                    _ => Action::Nothing,
                };
                self.perform(action);
            }
            Event::Accessibility(AccessibilityRequest::Click(id)) => {
                let index = id.checked_sub(FIRST_TILE_NODE).map(|index| index as usize);
                if let Some(item) = index.and_then(|index| self.items.get(index)) {
                    self.perform(model::click(item, self.snapshot.active));
                }
            }
            _ => {}
        }
    }

    fn paint(&mut self, surface: &RasterSurface) {
        let width = surface.logical_width();
        let height = surface.logical_height();
        let _ = surface.clear(BODY);
        // Raised Platinum panel: black frame, white top-left, grey bottom-right.
        let line = |x: i32, y: i32, w: u32, h: u32, color: Color| {
            let _ = surface.fill_rect(
                Rect {
                    x,
                    y,
                    width: w,
                    height: h,
                },
                color,
            );
        };
        let _ = surface.stroke_rect(
            Rect {
                x: 0,
                y: 0,
                width,
                height,
            },
            Color::BLACK,
        );
        line(1, 1, width - 2, 1, LIGHT);
        line(1, 1, 1, height - 2, LIGHT);
        line(1, height as i32 - 2, width - 2, 1, SHADOW);
        line(width as i32 - 2, 1, 1, height - 2, SHADOW);

        if self.has_separator() {
            let pinned = self.items.iter().take_while(|item| item.pinned).count();
            let x = self.tile_bounds(pinned).x - SEPARATOR as i32 / 2 - 1;
            line(x, PADDING as i32 + 6, 1, TILE - 12, SHADOW);
            line(x + 1, PADDING as i32 + 6, 1, TILE - 12, LIGHT);
        }

        let active = self.snapshot.active;
        for index in 0..self.items.len() {
            let bounds = self.tile_bounds(index);
            let item = self.items[index].clone();
            let pressed = self.pressed.is_some_and(|(pressed, _)| pressed == index)
                && self.hovered == Some(index);
            if pressed || item.is_active(active) {
                line(
                    bounds.x + 2,
                    bounds.y,
                    bounds.width - 4,
                    bounds.height,
                    ACTIVE_TILE,
                );
                line(bounds.x + 2, bounds.y, bounds.width - 4, 1, SHADOW);
                line(bounds.x + 2, bounds.y, 1, bounds.height, SHADOW);
            } else if self.hovered == Some(index) {
                line(
                    bounds.x + 2,
                    bounds.y,
                    bounds.width - 4,
                    bounds.height,
                    HOVER_TILE,
                );
            }
            let offset = (TILE - ICON) as i32 / 2 + i32::from(pressed);
            let (icon_x, icon_y) = (bounds.x + offset, bounds.y + offset);
            match self.icon(item.icon()) {
                Some(icon) => {
                    let _ = surface.draw_image_scaled(&icon, icon_x, icon_y, ICON, ICON);
                }
                None => paint_placeholder_icon(surface, icon_x, icon_y, item.name(), &self.font),
            }
            // One dot per window, up to three; amber when asking for attention.
            let dots = item.windows.len().min(3) as i32;
            let color = if item.attention() {
                ATTENTION
            } else {
                Color::BLACK
            };
            let dots_width = dots * 4 + (dots - 1).max(0) * 3;
            let mut x = bounds.x + (TILE as i32 - dots_width) / 2;
            let y = bounds.y + TILE as i32 + 1;
            for _ in 0..dots {
                line(x, y, 4, 4, color);
                x += 7;
            }
        }
    }

    fn accessibility(&self) -> Option<accesskit::TreeUpdate> {
        use accesskit::{Action as A11yAction, Node, NodeId, Role, TreeId, TreeInfo, TreeUpdate};
        let mut root = Node::new(Role::Toolbar);
        root.set_label("Dock");
        let mut nodes = Vec::new();
        let mut children = Vec::new();
        for (index, item) in self.items.iter().enumerate() {
            let id = NodeId(FIRST_TILE_NODE + index as u64);
            let mut node = Node::new(Role::Button);
            let state = match item.windows.len() {
                0 => String::new(),
                1 => ", running".to_owned(),
                count => format!(", {count} windows"),
            };
            node.set_label(format!("{}{state}", item.name()));
            let bounds = self.tile_bounds(index);
            node.set_bounds(accesskit::Rect {
                x0: f64::from(bounds.x),
                y0: f64::from(bounds.y),
                x1: f64::from(bounds.x) + f64::from(bounds.width),
                y1: f64::from(bounds.y) + f64::from(bounds.height),
            });
            node.add_action(A11yAction::Click);
            nodes.push((id, node));
            children.push(id);
        }
        root.set_children(children);
        nodes.insert(0, (NodeId(ROOT_NODE), root));
        Some(TreeUpdate {
            nodes,
            tree: Some(TreeInfo::new(NodeId(ROOT_NODE))),
            tree_id: TreeId::ROOT,
            focus: NodeId(ROOT_NODE),
        })
    }
}

/// A framed tile with the application's initial, for apps whose icon is not
/// installed.
fn paint_placeholder_icon(surface: &RasterSurface, x: i32, y: i32, name: &str, font: &Font) {
    let frame = Rect {
        x: x + 4,
        y: y + 4,
        width: ICON - 8,
        height: ICON - 8,
    };
    let _ = surface.fill_rect(frame, LIGHT);
    let _ = surface.stroke_rect(frame, Color::BLACK);
    let initial: String = name.chars().take(1).flat_map(char::to_uppercase).collect();
    let extent = measure(&initial, font);
    let _ = surface.draw_text(
        &initial,
        frame.x + (frame.width as i32 - extent.width as i32) / 2,
        frame.y + (frame.height as i32 - extent.height as i32) / 2,
        font,
        Color::BLACK,
    );
}

fn main() {
    if std::env::var_os("RUST_LOG").is_some() {
        let _ = env_logger::try_init();
    }
    if std::env::args().any(|arg| arg == "--pin-defaults") {
        // Write the default pins so the user has a file to edit.
        let catalog = scan_desktop_apps();
        let defaults = pins::resolve(&pins::defaults(&catalog), &catalog);
        if let Err(error) = pins::save(&defaults) {
            eprintln!("slopos-dock: could not save pins: {error}");
            std::process::exit(1);
        }
        return;
    }
    let screen = match platform::x11::screen_size() {
        Ok(size) => size,
        Err(error) => {
            eprintln!("slopos-dock: {error}");
            std::process::exit(1);
        }
    };
    let scale = platform::scale_from_environment().factor();
    let logical_screen = (screen.0 / scale, screen.1 / scale);
    let (sender, receiver) = mpsc::channel();
    let dock = Dock::new(receiver, logical_screen);
    let geometry = dock.geometry();
    let mut options = WindowOptions::new("SLOPOS Dock", APP_ID, geometry.width, geometry.height);
    options.kind = WindowKind::Dock {
        edge: Edge::Bottom,
        strut: HEIGHT,
    };
    options.resizable = false;
    options.position = Some((geometry.x, geometry.y));
    let result = platform::run(options, dock, move |proxy| {
        let watch = windows::watch(APP_ID, move |snapshot| {
            if sender.send(snapshot).is_ok() {
                proxy.wake();
            }
        });
        if let Err(error) = watch {
            log::warn!("Dock cannot follow windows: {error}");
        }
    });
    if let Err(error) = result {
        eprintln!("slopos-dock: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slopos_ui::foundation::geometry::Scale;

    fn app(id: &str) -> DesktopApp {
        DesktopApp {
            id: id.into(),
            name: id.into(),
            argv: vec!["/bin/true".into()],
            icon: String::new(),
            comment: String::new(),
            terminal: false,
            startup_wm_class: String::new(),
        }
    }

    fn dock(pins: Vec<DesktopApp>, windows: Vec<model::Window>) -> Dock {
        let (_sender, receiver) = mpsc::channel();
        let mut dock = Dock {
            catalog: pins.clone(),
            pins,
            snapshot: Snapshot {
                windows,
                active: Some(2),
            },
            items: Vec::new(),
            updates: receiver,
            icons: IconLookup::new(Vec::new(), "none"),
            icon_cache: HashMap::new(),
            scale: 1,
            hovered: None,
            pressed: None,
            screen: (1280, 800),
            font: Font::system(18.0).bold(),
        };
        dock.rebuild();
        dock
    }

    fn window(id: u32, class: &str) -> model::Window {
        model::Window {
            id,
            instance: class.to_lowercase(),
            class: class.into(),
            title: String::new(),
            attention: false,
        }
    }

    #[test]
    fn the_dock_is_centred_on_the_bottom_edge_and_grows_with_its_items() {
        let small = dock(vec![app("a")], Vec::new());
        let geometry = small.geometry();
        assert_eq!(geometry.y as u32 + geometry.height, 800);
        assert_eq!(geometry.x * 2 + geometry.width as i32, 1280);
        let bigger = dock(vec![app("a")], vec![window(2, "Other")]);
        assert_eq!(bigger.width(), small.width() + TILE + SEPARATOR);
    }

    #[test]
    fn tiles_are_hit_tested_across_the_separator() {
        let dock = dock(vec![app("a"), app("b")], vec![window(2, "Other")]);
        let third = dock.tile_bounds(2);
        assert_eq!(third.x, (PADDING + SEPARATOR + 2 * TILE) as i32);
        assert_eq!(dock.tile_at(third.x + 1, third.y + 1), Some(2));
        assert_eq!(
            dock.tile_at(dock.tile_bounds(1).x + TILE as i32 + 2, 20),
            None
        );
    }

    #[test]
    fn painting_shows_running_dots_and_the_active_tile() {
        let mut dock = dock(vec![app("a")], vec![window(2, "A"), window(3, "A")]);
        let surface = RasterSurface::new(dock.width(), HEIGHT, Scale::new(1).unwrap()).unwrap();
        dock.paint(&surface);
        let tile = dock.tile_bounds(0);
        // Two dots under the tile, centred.
        let y = (tile.y + TILE as i32 + 2) as u32;
        let dark = (tile.x..tile.x + TILE as i32)
            .filter(|x| surface.pixel(*x as u32, y) == Some(0xff000000))
            .count();
        assert_eq!(dark, 8);
        // Window 2 is active, so the tile is drawn pressed in.
        assert_eq!(
            surface.pixel(tile.x as u32 + 10, tile.y as u32 + 1),
            Some(0xffbbbbbb)
        );
    }

    #[test]
    fn screen_readers_see_each_tile_with_its_window_count() {
        let dock = dock(
            vec![app("Files")],
            vec![window(2, "Files"), window(3, "Files")],
        );
        let tree = dock.accessibility().unwrap();
        let labels: Vec<_> = tree
            .nodes
            .iter()
            .filter_map(|(_, node)| node.label())
            .collect();
        assert_eq!(labels, ["Dock", "Files, 2 windows"]);
    }
}
