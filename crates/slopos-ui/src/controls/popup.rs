//! Pop-up button: a plate showing the chosen value that opens a [`Menu`] of
//! the alternatives (resolution, output device, keyboard layout).
//!
//! Closed, Space, Enter, Up or Down (while focused) or a click opens the
//! menu with the current value highlighted and placed over the button, the
//! Classic pop-up position. Open, the menu takes every event: arrows move,
//! Enter or a click chooses, Escape or a click outside closes it. Pressing
//! on the button and releasing on an item chooses it in one gesture; a click
//! that releases where it pressed leaves the menu open.

use crate::event::{AccessibilityRequest, Event, Key, PointerButton};
use crate::foundation::{color::Color, geometry::Rect};
use crate::primitives::button_plate::{
    paint_button_plate_in, ButtonPlateState, ButtonPlateVariant,
};
use crate::render::{RasterSurface, RenderError};
use crate::text::{measure, Font};

use super::button::paint_focus_ring;
use super::menu::{Menu, MenuItem, MenuOutcome, ITEM_HEIGHT};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopUpOutcome {
    /// The event did not concern the pop-up.
    Ignored,
    /// It opened, closed or moved its highlight.
    Changed,
    /// The user chose a different item, at this index.
    Selected(usize),
}

#[derive(Debug, Clone)]
pub struct PopUpButton {
    /// Accessibility id; the menu uses `id + MENU_OFFSET`.
    pub id: u64,
    /// The accessible name, e.g. "Resolution".
    pub label: String,
    pub bounds: Rect,
    pub enabled: bool,
    pub focused: bool,
    items: Vec<String>,
    selected: Option<usize>,
    menu: Menu,
    /// Where the press that opened the menu happened, until its release.
    opening_press: Option<(i32, i32)>,
}

/// Menu ids sit far above control ids so their item nodes never collide.
const MENU_OFFSET: u64 = 1 << 24;

impl PopUpButton {
    pub fn new(id: u64, label: &str) -> Self {
        let size = ButtonPlateVariant::Regular.dimensions();
        Self {
            id,
            label: label.to_owned(),
            bounds: Rect {
                x: 0,
                y: 0,
                width: 160,
                height: size.height,
            },
            enabled: true,
            focused: false,
            items: Vec::new(),
            selected: None,
            menu: Menu::new(id + MENU_OFFSET, Vec::new()),
            opening_press: None,
        }
    }

    /// Replace the choices, keeping the selection when it is still in range.
    pub fn set_items(&mut self, items: Vec<String>, selected: Option<usize>) {
        if items != self.items {
            self.menu.close();
            self.opening_press = None;
        }
        self.selected = selected.filter(|&index| index < items.len());
        self.items = items;
    }

    pub fn items(&self) -> &[String] {
        &self.items
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn select(&mut self, index: Option<usize>) {
        self.selected = index.filter(|&index| index < self.items.len());
    }

    /// The shown value, or an empty string with nothing chosen.
    pub fn value(&self) -> &str {
        self.selected
            .and_then(|index| self.items.get(index))
            .map_or("", String::as_str)
    }

    pub fn is_open(&self) -> bool {
        self.menu.is_open()
    }

    pub fn close(&mut self) {
        self.menu.close();
        self.opening_press = None;
    }

    fn open(&mut self, window: (u32, u32)) {
        if !self.enabled || self.items.is_empty() {
            return;
        }
        self.menu.items = self
            .items
            .iter()
            .enumerate()
            .map(|(index, label)| MenuItem::new(label).checked(Some(index) == self.selected))
            .collect();
        self.menu.min_width = self.bounds.width;
        // The current item lies over the button.
        let index = self.selected.unwrap_or(0) as i32;
        let y = self.bounds.y - 2 - index * ITEM_HEIGHT as i32
            + (self.bounds.height as i32 - ITEM_HEIGHT as i32) / 2;
        self.menu.open_at(self.bounds.x, y, window);
        self.menu.highlight(self.selected.or(Some(0)));
    }

    fn chosen(&mut self, index: usize) -> PopUpOutcome {
        self.opening_press = None;
        if Some(index) == self.selected {
            PopUpOutcome::Changed
        } else {
            self.selected = Some(index);
            PopUpOutcome::Selected(index)
        }
    }

    /// `window` is the window's logical size, for placing the menu.
    pub fn handle(&mut self, event: &Event, window: (u32, u32)) -> PopUpOutcome {
        if self.menu.is_open() {
            if let (
                Some((px, py)),
                Event::PointerReleased {
                    x,
                    y,
                    button: PointerButton::Primary,
                },
            ) = (self.opening_press, event)
            {
                self.opening_press = None;
                if (x - px).abs() <= 2 && (y - py).abs() <= 2 {
                    // A plain click: keep the menu open.
                    return PopUpOutcome::Changed;
                }
            }
            if let Event::PointerMoved { x, y } = event {
                // Moving away from the opening press turns it into a drag.
                if let Some((px, py)) = self.opening_press {
                    if (x - px).abs() > 2 || (y - py).abs() > 2 {
                        self.opening_press = Some((i32::MIN / 2, i32::MIN / 2));
                    }
                }
            }
            return match self.menu.handle(event) {
                MenuOutcome::Chosen(index) => self.chosen(index),
                MenuOutcome::Dismissed => {
                    self.opening_press = None;
                    PopUpOutcome::Changed
                }
                MenuOutcome::Ignored => PopUpOutcome::Ignored,
                MenuOutcome::Changed | MenuOutcome::Neighbour(_) => PopUpOutcome::Changed,
            };
        }
        if !self.enabled {
            return PopUpOutcome::Ignored;
        }
        match event {
            Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
            } if self.bounds.contains(*x, *y) => {
                self.open(window);
                self.opening_press = Some((*x, *y));
                PopUpOutcome::Changed
            }
            Event::KeyPressed {
                key: Key::Space | Key::Enter | Key::Up | Key::Down,
                ..
            } if self.focused => {
                self.open(window);
                PopUpOutcome::Changed
            }
            Event::Accessibility(AccessibilityRequest::Click(node)) if *node == self.id => {
                self.open(window);
                PopUpOutcome::Changed
            }
            _ => PopUpOutcome::Ignored,
        }
    }

    /// The plate with the value and the pop-up arrows. Paint the menu last,
    /// over everything, with [`Self::paint_menu`].
    pub fn paint(&self, surface: &RasterSurface, font: &Font) -> Result<(), RenderError> {
        // Too small for the plate and arrows: draw nothing rather than fail.
        if self.bounds.width < 32 || self.bounds.height < 14 {
            return Ok(());
        }
        let state = if self.enabled {
            ButtonPlateState::Rest
        } else {
            ButtonPlateState::Disabled
        };
        paint_button_plate_in(surface, self.bounds, ButtonPlateVariant::Regular, state)?;
        let ink = if self.enabled {
            Color::BLACK
        } else {
            Color {
                red: 0x80,
                green: 0x80,
                blue: 0x80,
                alpha: 0xff,
            }
        };
        let extent = measure("Ag", font);
        let text_y = self.bounds.y + (self.bounds.height as i32 - extent.height as i32) / 2;
        let arrows_x = self.bounds.x + self.bounds.width as i32 - 18;
        let mut value = self.value().to_owned();
        let room = (arrows_x - self.bounds.x - 12).max(0) as f32;
        while !value.is_empty() && measure(&value, font).width > room {
            value.pop();
            while value.ends_with(' ') {
                value.pop();
            }
            if measure(&format!("{value}…"), font).width <= room {
                value.push('…');
                break;
            }
        }
        surface.draw_text(&value, self.bounds.x + 10, text_y, font, ink)?;
        // Up and down arrows: the Classic pop-up mark.
        let centre = self.bounds.y + self.bounds.height as i32 / 2;
        for row in 0..4 {
            let half = row as u32;
            let width = 1 + 2 * half;
            let x = arrows_x + 4 - half as i32;
            surface.fill_rect(
                Rect {
                    x,
                    y: centre - 6 + row,
                    width,
                    height: 1,
                },
                ink,
            )?;
            surface.fill_rect(
                Rect {
                    x,
                    y: centre + 5 - row,
                    width,
                    height: 1,
                },
                ink,
            )?;
        }
        if self.focused && self.enabled {
            paint_focus_ring(
                surface,
                Rect {
                    x: self.bounds.x - 2,
                    y: self.bounds.y - 2,
                    width: self.bounds.width + 4,
                    height: self.bounds.height + 4,
                },
            )?;
        }
        Ok(())
    }

    pub fn paint_menu(&self, surface: &RasterSurface) -> Result<(), RenderError> {
        self.menu.paint(surface)
    }

    /// The node to focus: the highlighted item while open, else the button.
    pub fn focus_node(&self) -> u64 {
        if self.menu.is_open() {
            self.menu.focus_node()
        } else {
            self.id
        }
    }

    /// A `ComboBox` node and, while open, its menu.
    pub fn accessibility_nodes(&self) -> Vec<(u64, accesskit::Node)> {
        let mut node = accesskit::Node::new(accesskit::Role::ComboBox);
        node.set_label(self.label.as_str());
        node.set_value(self.value());
        node.set_bounds(super::bounds_of(self.bounds));
        node.set_expanded(self.menu.is_open());
        if self.enabled {
            node.add_action(accesskit::Action::Click);
            node.add_action(accesskit::Action::Focus);
        } else {
            node.set_disabled();
        }
        let menu = self.menu.accessibility_nodes();
        if let Some((id, _)) = menu.first() {
            node.set_children(vec![accesskit::NodeId(*id)]);
        }
        let mut nodes = vec![(self.id, node)];
        nodes.extend(menu);
        nodes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Modifiers;
    use crate::foundation::geometry::Scale;

    const WINDOW: (u32, u32) = (400, 300);

    fn key(key: Key) -> Event {
        Event::KeyPressed {
            key,
            modifiers: Modifiers::default(),
        }
    }

    fn popup() -> PopUpButton {
        let mut popup = PopUpButton::new(7, "Resolution");
        popup.bounds = Rect {
            x: 100,
            y: 100,
            width: 160,
            height: 20,
        };
        popup.set_items(
            vec![
                "1920 × 1080".into(),
                "1280 × 720".into(),
                "800 × 600".into(),
            ],
            Some(1),
        );
        popup
    }

    #[test]
    fn keyboard_opens_moves_and_chooses() {
        let mut popup = popup();
        assert_eq!(
            popup.handle(&key(Key::Space), WINDOW),
            PopUpOutcome::Ignored
        );
        popup.focused = true;
        assert_eq!(
            popup.handle(&key(Key::Space), WINDOW),
            PopUpOutcome::Changed
        );
        assert!(popup.is_open());
        // The current value is highlighted and drawn over the button.
        assert_eq!(popup.menu.highlighted(), Some(1));
        let item_y = popup.menu.bounds().y + 2 + ITEM_HEIGHT as i32;
        assert!((item_y - popup.bounds.y).abs() <= 2, "{item_y}");
        assert!(popup.menu.bounds().width >= popup.bounds.width);
        popup.handle(&key(Key::Down), WINDOW);
        assert_eq!(
            popup.handle(&key(Key::Enter), WINDOW),
            PopUpOutcome::Selected(2)
        );
        assert!(!popup.is_open());
        assert_eq!(popup.value(), "800 × 600");
        // Choosing the same value again is not a change; Escape closes.
        popup.handle(&key(Key::Up), WINDOW);
        assert_eq!(
            popup.handle(&key(Key::Enter), WINDOW),
            PopUpOutcome::Changed
        );
        popup.handle(&key(Key::Down), WINDOW);
        assert_eq!(
            popup.handle(&key(Key::Escape), WINDOW),
            PopUpOutcome::Changed
        );
        assert!(!popup.is_open());
        assert_eq!(popup.selected(), Some(2));
    }

    #[test]
    fn click_keeps_it_open_and_drag_release_chooses() {
        let mut popup = popup();
        let press = |x, y| Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
        };
        let release = |x, y| Event::PointerReleased {
            x,
            y,
            button: PointerButton::Primary,
        };
        popup.handle(&press(150, 110), WINDOW);
        assert!(popup.is_open());
        assert_eq!(
            popup.handle(&release(150, 110), WINDOW),
            PopUpOutcome::Changed
        );
        assert!(popup.is_open());
        // A later click on the first item chooses it.
        let first = popup.menu.bounds().y + 2 + ITEM_HEIGHT as i32 / 2;
        popup.handle(&press(150, first), WINDOW);
        assert_eq!(
            popup.handle(&release(150, first), WINDOW),
            PopUpOutcome::Selected(0)
        );
        // Press, drag down one item, release: one gesture.
        popup.handle(&press(150, 110), WINDOW);
        let second = popup.menu.bounds().y + 2 + ITEM_HEIGHT as i32 * 3 / 2;
        popup.handle(&Event::PointerMoved { x: 150, y: second }, WINDOW);
        assert_eq!(
            popup.handle(&release(150, second), WINDOW),
            PopUpOutcome::Selected(1)
        );
        // A click outside closes without choosing.
        popup.handle(&press(150, 110), WINDOW);
        popup.handle(&release(150, 110), WINDOW);
        assert_eq!(popup.handle(&press(5, 5), WINDOW), PopUpOutcome::Changed);
        assert!(!popup.is_open());
        assert_eq!(popup.selected(), Some(1));
    }

    #[test]
    fn disabled_or_empty_never_opens_and_reports_combo_box_semantics() {
        let mut popup = popup();
        popup.focused = true;
        popup.enabled = false;
        assert_eq!(
            popup.handle(&key(Key::Space), WINDOW),
            PopUpOutcome::Ignored
        );
        popup.enabled = true;
        popup.set_items(Vec::new(), Some(3));
        assert_eq!(popup.selected(), None);
        popup.handle(&key(Key::Space), WINDOW);
        assert!(!popup.is_open());

        let mut popup = popup_with_long_value();
        let nodes = popup.accessibility_nodes();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].1.role(), accesskit::Role::ComboBox);
        assert_eq!(nodes[0].1.label(), Some("Resolution"));
        assert_eq!(nodes[0].1.is_expanded(), Some(false));
        popup.handle(
            &Event::Accessibility(AccessibilityRequest::Click(7)),
            WINDOW,
        );
        let nodes = popup.accessibility_nodes();
        assert_eq!(nodes[0].1.is_expanded(), Some(true));
        assert!(nodes.iter().any(|(_, n)| n.role() == accesskit::Role::Menu));
        assert_eq!(popup.focus_node(), popup.menu.item_node_id(0));
        // Choosing through the screen reader's item node.
        let item = popup.menu.item_node_id(1);
        assert_eq!(
            popup.handle(
                &Event::Accessibility(AccessibilityRequest::Click(item)),
                WINDOW
            ),
            PopUpOutcome::Selected(1)
        );
        // Long values are cut to fit; painting at any scale is safe, even
        // with a zero-sized button.
        for factor in [1, 2] {
            let surface = RasterSurface::new(400, 300, Scale::new(factor).unwrap()).unwrap();
            popup.paint(&surface, &Font::system(12.0)).unwrap();
            popup.bounds.width = 0;
            popup.paint(&surface, &Font::system(12.0)).unwrap();
            popup.bounds.width = 160;
        }
    }

    fn popup_with_long_value() -> PopUpButton {
        let mut popup = popup();
        popup.set_items(
            vec![
                "Built-in Retina Display, a very long description".into(),
                "Short".into(),
            ],
            Some(0),
        );
        popup
    }
}
