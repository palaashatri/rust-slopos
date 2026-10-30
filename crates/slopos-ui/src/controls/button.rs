//! Push button: the Classic plate with a centred label.
//!
//! The button is a plain state machine. Feed it events with
//! [`Button::handle`]; it reports [`ButtonOutcome::Activated`] when the user
//! clicks it (press and release inside) or presses Space/Enter while it has
//! keyboard focus. Dragging out of the button before releasing cancels.

use crate::event::{Event, Key, PointerButton};
use crate::foundation::{color::Color, geometry::Rect};
use crate::primitives::button_plate::{
    paint_button_plate_in, ButtonPlateState, ButtonPlateVariant,
};
use crate::render::{RasterSurface, RenderError};
use crate::text::{measure, Font};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonOutcome {
    /// The event did not concern this button.
    Ignored,
    /// The event changed how the button looks.
    Changed,
    /// The user activated the button.
    Activated,
}

#[derive(Debug, Clone)]
pub struct Button {
    pub label: String,
    pub variant: ButtonPlateVariant,
    pub bounds: Rect,
    pub enabled: bool,
    pub focused: bool,
    /// Accessibility node id; unique within the window.
    pub id: u64,
    /// Drawn instead of the label, which stays the accessible name. Toolbar
    /// buttons use it; the icon is dimmed while the button is disabled.
    pub icon: Option<crate::image::Image>,
    hovered: bool,
    pressed: bool,
}

impl Button {
    pub fn new(id: u64, label: &str, variant: ButtonPlateVariant) -> Self {
        let size = variant.dimensions();
        Self {
            label: label.to_owned(),
            variant,
            bounds: Rect {
                x: 0,
                y: 0,
                width: size.width,
                height: size.height,
            },
            enabled: true,
            focused: false,
            id,
            icon: None,
            hovered: false,
            pressed: false,
        }
    }

    pub fn at(mut self, x: i32, y: i32) -> Self {
        self.bounds.x = x;
        self.bounds.y = y;
        self
    }

    /// Place the button in a layout-defined rectangle.
    pub fn with_bounds(mut self, bounds: Rect) -> Self {
        self.bounds = bounds;
        self
    }

    /// Visual state, as the Classic spec names it.
    pub fn plate_state(&self) -> ButtonPlateState {
        if !self.enabled {
            ButtonPlateState::Disabled
        } else if self.pressed && self.hovered {
            ButtonPlateState::Pressed
        } else {
            ButtonPlateState::Rest
        }
    }

    pub fn handle(&mut self, event: &Event) -> ButtonOutcome {
        if !self.enabled {
            return ButtonOutcome::Ignored;
        }
        match event {
            Event::PointerMoved { x, y } => {
                let inside = self.bounds.contains(*x, *y);
                if inside != self.hovered {
                    self.hovered = inside;
                    if self.pressed {
                        return ButtonOutcome::Changed;
                    }
                }
                ButtonOutcome::Ignored
            }
            Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
            } if self.bounds.contains(*x, *y) => {
                self.hovered = true;
                self.pressed = true;
                ButtonOutcome::Changed
            }
            Event::PointerReleased {
                x,
                y,
                button: PointerButton::Primary,
            } if self.pressed => {
                self.pressed = false;
                if self.bounds.contains(*x, *y) {
                    ButtonOutcome::Activated
                } else {
                    ButtonOutcome::Changed
                }
            }
            Event::PointerLeft => {
                self.hovered = false;
                if self.pressed {
                    ButtonOutcome::Changed
                } else {
                    ButtonOutcome::Ignored
                }
            }
            Event::KeyPressed {
                key: Key::Space | Key::Enter,
                ..
            } if self.focused => ButtonOutcome::Activated,
            _ => ButtonOutcome::Ignored,
        }
    }

    pub fn paint(&self, surface: &RasterSurface, font: &Font) -> Result<(), RenderError> {
        let state = self.plate_state();
        paint_button_plate_in(surface, self.bounds, self.variant, state)?;
        let color = match state {
            ButtonPlateState::Pressed => Color::WHITE,
            ButtonPlateState::Disabled => Color {
                red: 0x80,
                green: 0x80,
                blue: 0x80,
                alpha: 0xff,
            },
            ButtonPlateState::Rest => Color::BLACK,
        };
        if let Some(icon) = &self.icon {
            let side = self.bounds.height.saturating_sub(6).min(16);
            let icon_rect = Rect {
                x: self.bounds.x + (self.bounds.width as i32 - side as i32) / 2,
                y: self.bounds.y + (self.bounds.height as i32 - side as i32) / 2,
                width: side,
                height: side,
            };
            surface.draw_image_scaled(icon, icon_rect.x, icon_rect.y, side, side)?;
            if state == ButtonPlateState::Disabled {
                let mut veil = crate::theme::palette().surface;
                veil.alpha = 0xa0;
                surface.fill_rect(icon_rect, veil)?;
            }
        } else {
            let extent = measure(&self.label, font);
            let x = self.bounds.x + (self.bounds.width as i32 - extent.width as i32) / 2;
            let y = self.bounds.y + (self.bounds.height as i32 - extent.height as i32) / 2;
            surface.draw_text(&self.label, x, y, font, color)?;
        }
        if self.focused && self.enabled {
            // A dotted focus ring just outside the plate keeps the Classic
            // shape intact while staying visible in every state.
            let ring = Rect {
                x: self.bounds.x - 2,
                y: self.bounds.y - 2,
                width: self.bounds.width + 4,
                height: self.bounds.height + 4,
            };
            paint_focus_ring(surface, ring)?;
        }
        Ok(())
    }

    pub fn accessibility_node(&self) -> accesskit::Node {
        let mut node = accesskit::Node::new(accesskit::Role::Button);
        node.set_label(self.label.as_str());
        node.set_bounds(accesskit::Rect {
            x0: f64::from(self.bounds.x),
            y0: f64::from(self.bounds.y),
            x1: f64::from(self.bounds.x) + f64::from(self.bounds.width),
            y1: f64::from(self.bounds.y) + f64::from(self.bounds.height),
        });
        if self.enabled {
            node.add_action(accesskit::Action::Click);
            node.add_action(accesskit::Action::Focus);
        } else {
            node.set_disabled();
        }
        node
    }
}

/// Alternating-pixel outline, the Classic way of showing keyboard focus.
pub fn paint_focus_ring(surface: &RasterSurface, ring: Rect) -> Result<(), RenderError> {
    let dot = |x: i32, y: i32| {
        surface.fill_rect(
            Rect {
                x,
                y,
                width: 1,
                height: 1,
            },
            Color::BLACK,
        )
    };
    let right = ring.x + ring.width as i32 - 1;
    let bottom = ring.y + ring.height as i32 - 1;
    for x in (ring.x..=right).step_by(2) {
        dot(x, ring.y)?;
        dot(x, bottom)?;
    }
    for y in (ring.y..=bottom).step_by(2) {
        dot(ring.x, y)?;
        dot(right, y)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::geometry::Scale;

    fn press(x: i32, y: i32) -> Event {
        Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
        }
    }

    fn release(x: i32, y: i32) -> Event {
        Event::PointerReleased {
            x,
            y,
            button: PointerButton::Primary,
        }
    }

    #[test]
    fn click_inside_activates_and_shows_pressed_state_meanwhile() {
        let mut button = Button::new(1, "OK", ButtonPlateVariant::Regular).at(10, 10);
        assert_eq!(button.handle(&press(20, 15)), ButtonOutcome::Changed);
        assert_eq!(button.plate_state(), ButtonPlateState::Pressed);
        assert_eq!(button.handle(&release(20, 15)), ButtonOutcome::Activated);
        assert_eq!(button.plate_state(), ButtonPlateState::Rest);
    }

    #[test]
    fn dragging_out_before_release_cancels() {
        let mut button = Button::new(1, "OK", ButtonPlateVariant::Regular).at(0, 0);
        button.handle(&press(5, 5));
        assert_eq!(
            button.handle(&Event::PointerMoved { x: 200, y: 5 }),
            ButtonOutcome::Changed
        );
        assert_eq!(button.plate_state(), ButtonPlateState::Rest);
        assert_eq!(button.handle(&release(200, 5)), ButtonOutcome::Changed);
    }

    #[test]
    fn keyboard_activation_needs_focus_and_disabled_buttons_ignore_input() {
        let mut button = Button::new(1, "OK", ButtonPlateVariant::Default);
        let space = Event::KeyPressed {
            key: Key::Space,
            modifiers: Default::default(),
        };
        assert_eq!(button.handle(&space), ButtonOutcome::Ignored);
        button.focused = true;
        assert_eq!(button.handle(&space), ButtonOutcome::Activated);
        button.enabled = false;
        assert_eq!(button.handle(&space), ButtonOutcome::Ignored);
        assert_eq!(button.handle(&press(5, 5)), ButtonOutcome::Ignored);
        assert_eq!(button.plate_state(), ButtonPlateState::Disabled);
    }

    #[test]
    fn painting_draws_plate_label_and_focus_ring() {
        let surface = RasterSurface::new(100, 40, Scale::new(1).unwrap()).unwrap();
        surface.clear(Color::WHITE).unwrap();
        let mut button = Button::new(1, "Apply", ButtonPlateVariant::Regular).at(10, 10);
        button.focused = true;
        button.paint(&surface, &Font::system(12.0)).unwrap();
        // Plate border at the left edge, focus dots outside it.
        assert_eq!(surface.pixel(10, 20), Some(0xff000000));
        assert_eq!(surface.pixel(8, 8), Some(0xff000000));
        let dark_inside = (14..86)
            .flat_map(|x| (13..27).map(move |y| (x, y)))
            .filter(|(x, y)| surface.pixel(*x, *y) != Some(0xffffffff))
            .count();
        assert!(dark_inside > 10, "label was not drawn");
    }

    #[test]
    fn accessibility_reports_role_label_and_actions() {
        let button = Button::new(7, "Equals", ButtonPlateVariant::Regular);
        let node = button.accessibility_node();
        assert_eq!(node.role(), accesskit::Role::Button);
        assert_eq!(node.label(), Some("Equals"));
        assert!(node.supports_action(accesskit::Action::Click));
    }

    #[test]
    fn icon_buttons_draw_the_icon_keep_the_label_as_name_and_dim_when_disabled() {
        use crate::foundation::geometry::Scale;
        let red = crate::image::Image::from_rgba(1, 1, vec![255, 0, 0, 255]).unwrap();
        let mut button = Button::new(7, "Back", ButtonPlateVariant::Regular).with_bounds(Rect {
            x: 0,
            y: 0,
            width: 30,
            height: 22,
        });
        button.icon = Some(red);
        let surface = RasterSurface::new(30, 22, Scale::new(1).unwrap()).unwrap();
        let font = Font::system(12.0);
        button.paint(&surface, &font).unwrap();
        let centre = surface.pixel(15, 11).unwrap();
        assert_eq!(centre, 0xffff_0000, "the icon is drawn in the middle");
        assert_eq!(button.accessibility_node().label(), Some("Back"));
        button.enabled = false;
        button.paint(&surface, &font).unwrap();
        let dimmed = surface.pixel(15, 11).unwrap();
        assert_ne!(dimmed, 0xffff_0000, "a disabled icon is veiled");
        assert_eq!(dimmed >> 24, 0xff);
    }
}
