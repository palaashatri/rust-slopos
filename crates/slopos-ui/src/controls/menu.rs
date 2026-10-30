//! Pull-down and context menus, and an in-window menu bar.
//!
//! A [`Menu`] is drawn inside the app's window on top of everything else.
//! While it is open it takes every pointer and key event: Up/Down move the
//! highlight (skipping separators and disabled items), Enter or a click
//! chooses, Escape or a click outside dismisses. Items show their keyboard
//! shortcut text on the right. A [`MenuBar`] holds titled menus; clicking a
//! title (or F10, then Left/Right) opens them.

use crate::event::{Event, Key, PointerButton};
use crate::foundation::geometry::Rect;
use crate::render::{RasterSurface, RenderError};
use crate::text::{measure, Font};
use crate::theme::palette;

pub(crate) const ITEM_HEIGHT: u32 = 20;
const SEPARATOR_HEIGHT: u32 = 9;
const PADDING_X: i32 = 18;
const SHADOW: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItem {
    pub label: String,
    /// Shortcut text shown on the right, e.g. "Ctrl+C".
    pub shortcut: Option<String>,
    pub enabled: bool,
    pub checked: bool,
    /// A toggle: shown with a check mark when [`Self::checked`]. The global
    /// menu bar shows check items as such even while unchecked.
    pub checkable: bool,
    pub separator: bool,
}

impl MenuItem {
    pub fn new(label: &str) -> Self {
        Self {
            label: label.to_owned(),
            shortcut: None,
            enabled: true,
            checked: false,
            checkable: false,
            separator: false,
        }
    }

    pub fn shortcut(mut self, text: &str) -> Self {
        self.shortcut = Some(text.to_owned());
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self.checkable = true;
        self
    }

    /// Mark the item as a toggle without checking it.
    pub fn checkable(mut self) -> Self {
        self.checkable = true;
        self
    }

    pub fn separator() -> Self {
        Self {
            separator: true,
            enabled: false,
            ..Self::new("")
        }
    }

    fn selectable(&self) -> bool {
        self.enabled && !self.separator
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuOutcome {
    /// The menu is closed and the event is not for it.
    Ignored,
    /// The menu used the event (highlight moved, or it swallowed it).
    Changed,
    /// The user chose the item at this index; the menu has closed.
    Chosen(usize),
    /// Closed without a choice.
    Dismissed,
    /// Left/Right while open: the menu bar should switch menus.
    Neighbour(i32),
}

#[derive(Debug, Clone)]
pub struct Menu {
    /// Accessibility id; item nodes use `(id << 32) | (index + 1)`.
    pub id: u64,
    pub items: Vec<MenuItem>,
    pub font: Font,
    /// The narrowest the menu gets (a pop-up's menu is as wide as it).
    pub min_width: u32,
    open: bool,
    bounds: Rect,
    highlighted: Option<usize>,
    pressed_inside: bool,
}

impl Menu {
    pub fn new(id: u64, items: Vec<MenuItem>) -> Self {
        Self {
            id,
            items,
            font: Font::system(12.0),
            min_width: 120,
            open: false,
            bounds: Rect::default(),
            highlighted: None,
            pressed_inside: false,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn bounds(&self) -> Rect {
        self.bounds
    }

    pub fn highlighted(&self) -> Option<usize> {
        self.highlighted
    }

    /// The menu's natural size.
    pub fn size(&self) -> (u32, u32) {
        let mut width = self.min_width;
        let mut height = 4;
        for item in &self.items {
            if item.separator {
                height += SEPARATOR_HEIGHT;
                continue;
            }
            let shortcut = item
                .shortcut
                .as_deref()
                .map_or(0.0, |text| measure(text, &self.font).width + 24.0);
            let label = measure(&item.label, &self.font).width;
            width = width.max((label + shortcut) as u32 + 2 * PADDING_X as u32);
            height += ITEM_HEIGHT;
        }
        (width, height)
    }

    /// Open with the top-left corner at `x, y`, moved so the whole menu fits
    /// inside `window` (logical size of the window).
    pub fn open_at(&mut self, x: i32, y: i32, window: (u32, u32)) {
        let (width, height) = self.size();
        let max_x = window.0 as i32 - width as i32 - SHADOW as i32;
        let max_y = window.1 as i32 - height as i32 - SHADOW as i32;
        self.bounds = Rect {
            x: x.min(max_x).max(0),
            y: y.min(max_y).max(0),
            width,
            height,
        };
        self.open = true;
        self.highlighted = None;
        self.pressed_inside = false;
    }

    /// Open from the keyboard: like [`Self::open_at`] with the first usable
    /// item highlighted.
    pub fn open_with_keyboard(&mut self, x: i32, y: i32, window: (u32, u32)) {
        self.open_at(x, y, window);
        self.highlighted = self.step(None, 1);
    }

    pub fn close(&mut self) {
        self.open = false;
        self.highlighted = None;
    }

    /// Highlight an item while open (ignored for separators and disabled
    /// items).
    pub fn highlight(&mut self, index: Option<usize>) {
        if self.open {
            self.highlighted =
                index.filter(|&i| self.items.get(i).is_some_and(MenuItem::selectable));
        }
    }

    fn item_rects(&self) -> Vec<Rect> {
        let mut y = self.bounds.y + 2;
        self.items
            .iter()
            .map(|item| {
                let height = if item.separator {
                    SEPARATOR_HEIGHT
                } else {
                    ITEM_HEIGHT
                };
                let rect = Rect {
                    x: self.bounds.x + 1,
                    y,
                    width: self.bounds.width.saturating_sub(2),
                    height,
                };
                y += height as i32;
                rect
            })
            .collect()
    }

    fn item_at(&self, x: i32, y: i32) -> Option<usize> {
        self.item_rects()
            .iter()
            .position(|rect| rect.contains(x, y))
            .filter(|&index| self.items[index].selectable())
    }

    fn step(&self, from: Option<usize>, direction: i32) -> Option<usize> {
        let count = self.items.len() as i32;
        if count == 0 {
            return None;
        }
        let mut index = from.map_or(if direction > 0 { -1 } else { count }, |i| i as i32);
        for _ in 0..count {
            index = (index + direction).rem_euclid(count);
            if self.items[index as usize].selectable() {
                return Some(index as usize);
            }
        }
        None
    }

    fn choose(&mut self, index: usize) -> MenuOutcome {
        self.close();
        MenuOutcome::Chosen(index)
    }

    pub fn handle(&mut self, event: &Event) -> MenuOutcome {
        if !self.open {
            return MenuOutcome::Ignored;
        }
        match event {
            Event::PointerMoved { x, y } => {
                let item = self.item_at(*x, *y);
                if item != self.highlighted && (item.is_some() || self.bounds.contains(*x, *y)) {
                    self.highlighted = item;
                }
                MenuOutcome::Changed
            }
            Event::PointerPressed { x, y, .. } => {
                if self.bounds.contains(*x, *y) {
                    self.pressed_inside = true;
                    MenuOutcome::Changed
                } else {
                    self.close();
                    MenuOutcome::Dismissed
                }
            }
            Event::PointerReleased { x, y, button } => {
                // Press on a title, drag into the menu and release chooses
                // (Classic pull-down behaviour), as does click-release inside.
                match self.item_at(*x, *y) {
                    Some(index) if *button != PointerButton::Middle => self.choose(index),
                    _ => MenuOutcome::Changed,
                }
            }
            Event::KeyPressed { key, .. } => match key {
                Key::Down => {
                    self.highlighted = self.step(self.highlighted, 1);
                    MenuOutcome::Changed
                }
                Key::Up => {
                    self.highlighted = self.step(self.highlighted, -1);
                    MenuOutcome::Changed
                }
                Key::Enter | Key::Space => match self.highlighted {
                    Some(index) => self.choose(index),
                    None => MenuOutcome::Changed,
                },
                Key::Escape => {
                    self.close();
                    MenuOutcome::Dismissed
                }
                Key::Left => MenuOutcome::Neighbour(-1),
                Key::Right => MenuOutcome::Neighbour(1),
                Key::Character(c) => {
                    let wanted = c.to_lowercase().to_string();
                    let found = self.items.iter().position(|item| {
                        item.selectable() && item.label.to_lowercase().starts_with(&wanted)
                    });
                    match found {
                        Some(index) => self.choose(index),
                        None => MenuOutcome::Changed,
                    }
                }
                _ => MenuOutcome::Changed,
            },
            Event::Accessibility(crate::event::AccessibilityRequest::Click(node)) => {
                match self.item_for_node(*node) {
                    Some(index) if self.items[index].selectable() => self.choose(index),
                    _ => MenuOutcome::Changed,
                }
            }
            Event::FocusChanged(false) => {
                self.close();
                MenuOutcome::Dismissed
            }
            Event::Scroll { .. } | Event::DoubleClicked { .. } => MenuOutcome::Changed,
            _ => MenuOutcome::Ignored,
        }
    }

    pub fn paint(&self, surface: &RasterSurface) -> Result<(), RenderError> {
        if !self.open {
            return Ok(());
        }
        let colors = palette();
        let bounds = self.bounds;
        surface.fill_rect(
            Rect {
                x: bounds.x + SHADOW as i32,
                y: bounds.y + SHADOW as i32,
                ..bounds
            },
            colors.bevel_shadow,
        )?;
        surface.fill_rect(bounds, colors.content)?;
        surface.stroke_rect(bounds, colors.border)?;
        let line_height = measure("Ag", &self.font).height as i32;
        for (index, (item, rect)) in self.items.iter().zip(self.item_rects()).enumerate() {
            if item.separator {
                surface.fill_rect(
                    Rect {
                        x: rect.x,
                        y: rect.y + rect.height as i32 / 2,
                        width: rect.width,
                        height: 1,
                    },
                    colors.bevel_shadow,
                )?;
                continue;
            }
            let highlighted = self.highlighted == Some(index);
            if highlighted {
                surface.fill_rect(rect, colors.menu_highlight)?;
            }
            let ink = if !item.enabled {
                colors.ink_disabled
            } else if highlighted {
                colors.menu_highlight_ink
            } else {
                colors.ink
            };
            let text_y = rect.y + (rect.height as i32 - line_height) / 2;
            if item.checked {
                surface.draw_text("✓", rect.x + 4, text_y, &self.font, ink)?;
            }
            surface.draw_text(&item.label, rect.x + PADDING_X, text_y, &self.font, ink)?;
            if let Some(shortcut) = &item.shortcut {
                let width = measure(shortcut, &self.font).width as i32;
                let x = rect.x + rect.width as i32 - width - 10;
                surface.draw_text(shortcut, x, text_y, &self.font, ink)?;
            }
        }
        Ok(())
    }

    pub fn item_node_id(&self, index: usize) -> u64 {
        (self.id << 32) | (index as u64 + 1)
    }

    pub fn item_for_node(&self, node: u64) -> Option<usize> {
        if node >> 32 != self.id || node & 0xffff_ffff == 0 {
            return None;
        }
        let index = (node & 0xffff_ffff) as usize - 1;
        (index < self.items.len()).then_some(index)
    }

    /// The node to focus while the menu is open.
    pub fn focus_node(&self) -> u64 {
        self.highlighted
            .map_or(self.id, |index| self.item_node_id(index))
    }

    /// `Menu` and `MenuItem` nodes; empty while closed.
    pub fn accessibility_nodes(&self) -> Vec<(u64, accesskit::Node)> {
        if !self.open {
            return Vec::new();
        }
        let mut menu = accesskit::Node::new(accesskit::Role::Menu);
        menu.set_bounds(super::bounds_of(self.bounds));
        let mut nodes = Vec::new();
        let mut children = Vec::new();
        for (index, (item, rect)) in self.items.iter().zip(self.item_rects()).enumerate() {
            if item.separator {
                continue;
            }
            let mut node = accesskit::Node::new(if item.checked {
                accesskit::Role::MenuItemCheckBox
            } else {
                accesskit::Role::MenuItem
            });
            node.set_label(item.label.as_str());
            if let Some(shortcut) = &item.shortcut {
                node.set_keyboard_shortcut(shortcut.as_str());
            }
            node.set_bounds(super::bounds_of(rect));
            if item.checked {
                node.set_toggled(accesskit::Toggled::True);
            }
            if item.enabled {
                node.add_action(accesskit::Action::Click);
            } else {
                node.set_disabled();
            }
            children.push(accesskit::NodeId(self.item_node_id(index)));
            nodes.push((self.item_node_id(index), node));
        }
        menu.set_children(children);
        nodes.insert(0, (self.id, menu));
        nodes
    }
}

/// A row of menu titles at the top of a window.
#[derive(Debug, Clone)]
pub struct MenuBar {
    pub id: u64,
    pub bounds: Rect,
    pub titles: Vec<String>,
    pub menus: Vec<Menu>,
    pub font: Font,
    active: Option<usize>,
    global: bool,
}

/// The height of a [`MenuBar`].
pub const MENU_BAR_HEIGHT: u32 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuBarOutcome {
    Ignored,
    Changed,
    /// Menu `menu`, item `item` was chosen.
    Chosen {
        menu: usize,
        item: usize,
    },
}

impl MenuBar {
    /// `menus` pairs each title with its menu; menu `i` gets accessibility
    /// id `id + 1 + i`.
    pub fn new(id: u64, menus: Vec<(&str, Vec<MenuItem>)>) -> Self {
        let titles = menus.iter().map(|(title, _)| (*title).to_owned()).collect();
        let menus = menus
            .into_iter()
            .enumerate()
            .map(|(index, (_, items))| Menu::new(id + 1 + index as u64, items))
            .collect();
        Self {
            id,
            bounds: Rect::default(),
            titles,
            menus,
            font: Font::system(12.0),
            active: None,
            global: false,
        }
    }

    pub fn is_open(&self) -> bool {
        self.active.is_some()
    }

    /// Whether the desktop's global menu bar shows these menus. The bar then
    /// takes no space in the window, paints nothing and takes no input
    /// except [`Event::MenuChosen`].
    pub fn is_global(&self) -> bool {
        self.global
    }

    pub fn set_global(&mut self, global: bool) {
        if global {
            self.close();
        }
        self.global = global;
    }

    /// The height the window should give the bar: [`MENU_BAR_HEIGHT`], or 0
    /// while the menus are global.
    pub fn height(&self) -> u32 {
        if self.global {
            0
        } else {
            MENU_BAR_HEIGHT
        }
    }

    pub fn active(&self) -> Option<usize> {
        self.active
    }

    fn title_rects(&self) -> Vec<Rect> {
        let mut x = self.bounds.x + 8;
        self.titles
            .iter()
            .map(|title| {
                let width = measure(title, &self.font).width as u32 + 16;
                let rect = Rect {
                    x,
                    y: self.bounds.y,
                    width,
                    height: self.bounds.height,
                };
                x += width as i32;
                rect
            })
            .collect()
    }

    /// Open menu `index` under its title.
    pub fn open(&mut self, index: usize, window: (u32, u32), keyboard: bool) {
        self.close();
        let Some(rect) = self.title_rects().get(index).copied() else {
            return;
        };
        let y = rect.y + rect.height as i32;
        if keyboard {
            self.menus[index].open_with_keyboard(rect.x, y, window);
        } else {
            self.menus[index].open_at(rect.x, y, window);
        }
        self.active = Some(index);
    }

    pub fn close(&mut self) {
        if let Some(index) = self.active.take() {
            self.menus[index].close();
        }
    }

    /// Handle an event; `window` is the window's logical size.
    pub fn handle(&mut self, event: &Event, window: (u32, u32)) -> MenuBarOutcome {
        match event {
            Event::MenuChosen { menu, item } => {
                let usable = self
                    .menus
                    .get(*menu)
                    .and_then(|m| m.items.get(*item))
                    .is_some_and(MenuItem::selectable);
                if !usable {
                    return MenuBarOutcome::Ignored;
                }
                self.close();
                return MenuBarOutcome::Chosen {
                    menu: *menu,
                    item: *item,
                };
            }
            Event::GlobalMenu(global) => {
                self.set_global(*global);
                return MenuBarOutcome::Changed;
            }
            _ if self.global => return MenuBarOutcome::Ignored,
            _ => {}
        }
        let titles = self.title_rects();
        if let Some(active) = self.active {
            // Moving across titles while a menu is open switches menus.
            if let Event::PointerMoved { x, y } = event {
                if let Some(index) = titles.iter().position(|rect| rect.contains(*x, *y)) {
                    if index != active {
                        self.open(index, window, false);
                        return MenuBarOutcome::Changed;
                    }
                }
            }
            if let Event::PointerPressed { x, y, .. } = event {
                if titles[active].contains(*x, *y) {
                    self.close();
                    return MenuBarOutcome::Changed;
                }
            }
            return match self.menus[active].handle(event) {
                MenuOutcome::Chosen(item) => {
                    self.active = None;
                    MenuBarOutcome::Chosen { menu: active, item }
                }
                MenuOutcome::Dismissed => {
                    self.active = None;
                    MenuBarOutcome::Changed
                }
                MenuOutcome::Neighbour(direction) => {
                    let count = self.menus.len() as i32;
                    let next = (active as i32 + direction).rem_euclid(count) as usize;
                    self.open(next, window, true);
                    MenuBarOutcome::Changed
                }
                MenuOutcome::Changed => MenuBarOutcome::Changed,
                MenuOutcome::Ignored => MenuBarOutcome::Ignored,
            };
        }
        match event {
            Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
            } => match titles.iter().position(|rect| rect.contains(*x, *y)) {
                Some(index) => {
                    self.open(index, window, false);
                    MenuBarOutcome::Changed
                }
                None => MenuBarOutcome::Ignored,
            },
            Event::KeyPressed {
                key: Key::Function(10),
                ..
            } if !self.menus.is_empty() => {
                self.open(0, window, true);
                MenuBarOutcome::Changed
            }
            Event::Accessibility(crate::event::AccessibilityRequest::Click(node)) => {
                match self.menu_for_title_node(*node) {
                    Some(index) => {
                        self.open(index, window, true);
                        MenuBarOutcome::Changed
                    }
                    None => MenuBarOutcome::Ignored,
                }
            }
            _ => MenuBarOutcome::Ignored,
        }
    }

    /// Paint the bar. Paint the open menu last, over everything, with
    /// [`Self::paint_menus`].
    pub fn paint(&self, surface: &RasterSurface) -> Result<(), RenderError> {
        if self.global {
            return Ok(());
        }
        let colors = palette();
        surface.fill_rect(self.bounds, colors.content)?;
        surface.fill_rect(
            Rect {
                y: self.bounds.y + self.bounds.height as i32 - 1,
                height: 1,
                ..self.bounds
            },
            colors.border,
        )?;
        let line_height = measure("Ag", &self.font).height as i32;
        for (index, (title, rect)) in self.titles.iter().zip(self.title_rects()).enumerate() {
            let active = self.active == Some(index);
            let title_rect = Rect {
                height: rect.height.saturating_sub(1),
                ..rect
            };
            if active {
                surface.fill_rect(title_rect, colors.menu_highlight)?;
            }
            let ink = if active {
                colors.menu_highlight_ink
            } else {
                colors.ink
            };
            let y = rect.y + (rect.height as i32 - line_height) / 2;
            surface.draw_text(title, rect.x + 8, y, &self.font, ink)?;
        }
        Ok(())
    }

    pub fn paint_menus(&self, surface: &RasterSurface) -> Result<(), RenderError> {
        match self.active {
            Some(index) => self.menus[index].paint(surface),
            None => Ok(()),
        }
    }

    pub fn focus_node(&self) -> Option<u64> {
        self.active.map(|index| self.menus[index].focus_node())
    }

    /// A `MenuBar` node with one `MenuItem` per title, plus the open menu.
    pub fn accessibility_nodes(&self) -> Vec<(u64, accesskit::Node)> {
        let mut bar = accesskit::Node::new(accesskit::Role::MenuBar);
        if self.global {
            // The shell's top bar exposes these menus; keep an empty, hidden
            // node so the app's tree still resolves this id.
            bar.set_hidden();
            return vec![(self.id, bar)];
        }
        bar.set_bounds(super::bounds_of(self.bounds));
        let mut nodes = Vec::new();
        let mut children = Vec::new();
        for ((title, rect), menu) in self.titles.iter().zip(self.title_rects()).zip(&self.menus) {
            // The title node doubles as the menu's id while it is closed.
            let title_id = menu.id | (1 << 62);
            let mut node = accesskit::Node::new(accesskit::Role::MenuItem);
            node.set_label(title.as_str());
            node.set_bounds(super::bounds_of(rect));
            node.add_action(accesskit::Action::Click);
            if menu.is_open() {
                node.set_expanded(true);
                node.set_children(vec![accesskit::NodeId(menu.id)]);
            }
            children.push(accesskit::NodeId(title_id));
            nodes.push((title_id, node));
            nodes.extend(menu.accessibility_nodes());
        }
        bar.set_children(children);
        nodes.insert(0, (self.id, bar));
        nodes
    }

    /// Map a title node id (from [`Self::accessibility_nodes`]) to its menu.
    pub fn menu_for_title_node(&self, node: u64) -> Option<usize> {
        self.menus
            .iter()
            .position(|menu| menu.id | (1 << 62) == node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::color::Color;
    use crate::foundation::geometry::Scale;

    fn items() -> Vec<MenuItem> {
        vec![
            MenuItem::new("Open").shortcut("Ctrl+O"),
            MenuItem::separator(),
            MenuItem::new("Rename").enabled(false),
            MenuItem::new("Move to Trash").shortcut("Delete"),
        ]
    }

    fn key(key: Key) -> Event {
        Event::KeyPressed {
            key,
            modifiers: Default::default(),
        }
    }

    #[test]
    fn keyboard_skips_separators_and_disabled_items() {
        let mut menu = Menu::new(9, items());
        assert_eq!(menu.handle(&key(Key::Down)), MenuOutcome::Ignored);
        menu.open_with_keyboard(10, 10, (400, 300));
        assert_eq!(menu.highlighted(), Some(0));
        menu.handle(&key(Key::Down));
        assert_eq!(menu.highlighted(), Some(3));
        menu.handle(&key(Key::Down));
        assert_eq!(menu.highlighted(), Some(0));
        menu.handle(&key(Key::Up));
        assert_eq!(menu.handle(&key(Key::Enter)), MenuOutcome::Chosen(3));
        assert!(!menu.is_open());
        menu.open_with_keyboard(10, 10, (400, 300));
        assert_eq!(
            menu.handle(&key(Key::Character('m'))),
            MenuOutcome::Chosen(3)
        );
        menu.open_with_keyboard(10, 10, (400, 300));
        // Disabled items are not reachable by type-ahead either.
        assert_eq!(menu.handle(&key(Key::Character('r'))), MenuOutcome::Changed);
        assert_eq!(menu.handle(&key(Key::Escape)), MenuOutcome::Dismissed);
    }

    #[test]
    fn pointer_choose_dismiss_and_clamping() {
        let mut menu = Menu::new(9, items());
        menu.open_at(390, 290, (400, 300));
        let bounds = menu.bounds();
        assert!(bounds.x + bounds.width as i32 <= 400 && bounds.y + bounds.height as i32 <= 300);
        let rects = menu.item_rects();
        let (x, y) = (rects[3].x + 5, rects[3].y + 5);
        menu.handle(&Event::PointerMoved { x, y });
        assert_eq!(menu.highlighted(), Some(3));
        menu.handle(&Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
        });
        assert_eq!(
            menu.handle(&Event::PointerReleased {
                x,
                y,
                button: PointerButton::Primary
            }),
            MenuOutcome::Chosen(3)
        );
        menu.open_at(0, 0, (400, 300));
        let disabled = menu.item_rects()[2];
        assert_eq!(
            menu.handle(&Event::PointerReleased {
                x: disabled.x + 5,
                y: disabled.y + 5,
                button: PointerButton::Primary
            }),
            MenuOutcome::Changed
        );
        assert_eq!(
            menu.handle(&Event::PointerPressed {
                x: 399,
                y: 299,
                button: PointerButton::Primary
            }),
            MenuOutcome::Dismissed
        );
    }

    #[test]
    fn menu_bar_opens_switches_and_chooses() {
        let mut bar = MenuBar::new(
            100,
            vec![
                ("File", items()),
                ("View", vec![MenuItem::new("Hidden Files").checked(true)]),
            ],
        );
        bar.bounds = Rect {
            x: 0,
            y: 0,
            width: 400,
            height: MENU_BAR_HEIGHT,
        };
        let window = (400, 300);
        assert_eq!(
            bar.handle(&key(Key::Function(10)), window),
            MenuBarOutcome::Changed
        );
        assert_eq!(bar.active(), Some(0));
        bar.handle(&key(Key::Right), window);
        assert_eq!(bar.active(), Some(1));
        assert_eq!(
            bar.handle(&key(Key::Enter), window),
            MenuBarOutcome::Chosen { menu: 1, item: 0 }
        );
        assert!(!bar.is_open());
        bar.handle(
            &Event::PointerPressed {
                x: 12,
                y: 5,
                button: PointerButton::Primary,
            },
            window,
        );
        assert_eq!(bar.active(), Some(0));
        let nodes = bar.accessibility_nodes();
        assert_eq!(nodes[0].1.role(), accesskit::Role::MenuBar);
        assert!(nodes
            .iter()
            .any(|(_, node)| node.label() == Some("Move to Trash")));
        let surface = RasterSurface::new(400, 300, Scale::new(2).unwrap()).unwrap();
        surface.clear(Color::WHITE).unwrap();
        bar.paint(&surface).unwrap();
        bar.paint_menus(&surface).unwrap();
    }

    #[test]
    fn accessibility_click_chooses_enabled_items_only() {
        let mut menu = Menu::new(9, items());
        menu.open_at(0, 0, (400, 300));
        let disabled = menu.item_node_id(2);
        assert_eq!(
            menu.handle(&Event::Accessibility(
                crate::event::AccessibilityRequest::Click(disabled)
            )),
            MenuOutcome::Changed
        );
        let open = menu.item_node_id(0);
        assert_eq!(
            menu.handle(&Event::Accessibility(
                crate::event::AccessibilityRequest::Click(open)
            )),
            MenuOutcome::Chosen(0)
        );
    }

    #[test]
    fn global_bar_takes_no_space_or_input_but_routes_shell_choices() {
        let window = (400, 300);
        let mut bar = MenuBar::new(20, vec![("File", items()), ("View", items())]);
        bar.bounds = Rect {
            x: 0,
            y: 0,
            width: 400,
            height: MENU_BAR_HEIGHT,
        };
        bar.open(1, window, true);
        assert_eq!(bar.height(), MENU_BAR_HEIGHT);
        assert_eq!(
            bar.handle(&Event::GlobalMenu(true), window),
            MenuBarOutcome::Changed
        );
        assert!(bar.is_global() && !bar.is_open());
        assert_eq!(bar.height(), 0);
        let press = Event::PointerPressed {
            x: 12,
            y: 5,
            button: PointerButton::Primary,
        };
        assert_eq!(bar.handle(&press, window), MenuBarOutcome::Ignored);
        assert_eq!(
            bar.handle(&key(Key::Function(10)), window),
            MenuBarOutcome::Ignored
        );
        assert_eq!(
            bar.handle(&Event::MenuChosen { menu: 1, item: 3 }, window),
            MenuBarOutcome::Chosen { menu: 1, item: 3 }
        );
        // Disabled items, separators and stale indices are refused.
        for (menu, item) in [(0, 2), (0, 1), (5, 0), (0, 9)] {
            assert_eq!(
                bar.handle(&Event::MenuChosen { menu, item }, window),
                MenuBarOutcome::Ignored
            );
        }
        let nodes = bar.accessibility_nodes();
        assert_eq!(nodes.len(), 1);
        assert!(nodes[0].1.is_hidden());
        // Painting a global bar leaves the surface untouched.
        let surface = RasterSurface::new(400, 40, Scale::new(1).unwrap()).unwrap();
        surface
            .clear(Color {
                red: 1,
                green: 2,
                blue: 3,
                alpha: 255,
            })
            .unwrap();
        bar.paint(&surface).unwrap();
        assert_eq!(surface.pixel(10, 10), Some(0xff01_0203));
        bar.handle(&Event::GlobalMenu(false), window);
        assert_eq!(bar.height(), MENU_BAR_HEIGHT);
        assert_eq!(bar.handle(&press, window), MenuBarOutcome::Changed);
    }

    #[test]
    fn check_items_are_checkable_even_when_unchecked() {
        assert!(MenuItem::new("Hidden").checked(false).checkable);
        assert!(MenuItem::new("Hidden").checkable().checkable);
        assert!(!MenuItem::new("Open").checkable);
    }
}
