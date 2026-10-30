//! Value controls: [`Slider`], [`Checkbox`] and [`ProgressBar`].

use crate::event::{AccessibilityRequest, Event, Key, PointerButton};
use crate::foundation::geometry::Rect;
use crate::render::{RasterSurface, RenderError};
use crate::text::{measure, Font};
use crate::theme::palette;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ValueOutcome {
    Ignored,
    /// Look changed (pressed, focus); value did not.
    Changed,
    /// The value changed (still dragging when `final` is false).
    Value {
        value: f64,
        last: bool,
    },
    FocusRequested,
}

/// A horizontal slider over `min..=max` in `step` increments.
#[derive(Debug, Clone)]
pub struct Slider {
    pub id: u64,
    pub label: String,
    pub bounds: Rect,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub enabled: bool,
    pub focused: bool,
    value: f64,
    dragging: bool,
}

const KNOB_WIDTH: u32 = 11;

impl Slider {
    pub fn new(id: u64, label: &str, min: f64, max: f64, step: f64) -> Self {
        let (min, max) = if min <= max { (min, max) } else { (max, min) };
        Self {
            id,
            label: label.to_owned(),
            bounds: Rect {
                x: 0,
                y: 0,
                width: 160,
                height: 20,
            },
            min,
            max,
            step: if step > 0.0 { step } else { 1.0 },
            enabled: true,
            focused: false,
            value: min,
            dragging: false,
        }
    }

    pub fn value(&self) -> f64 {
        self.value
    }

    /// Set the value, snapped to the step and clamped; NaN is ignored.
    pub fn set_value(&mut self, value: f64) -> bool {
        if value.is_nan() {
            return false;
        }
        let snapped = self.min + ((value - self.min) / self.step).round() * self.step;
        let clamped = snapped.clamp(self.min, self.max);
        let changed = clamped != self.value;
        self.value = clamped;
        changed
    }

    pub fn dragging(&self) -> bool {
        self.dragging
    }

    fn track(&self) -> Rect {
        Rect {
            x: self.bounds.x + KNOB_WIDTH as i32 / 2,
            y: self.bounds.y + self.bounds.height as i32 / 2 - 2,
            width: self.bounds.width.saturating_sub(KNOB_WIDTH),
            height: 4,
        }
    }

    fn fraction(&self) -> f64 {
        if self.max > self.min {
            (self.value - self.min) / (self.max - self.min)
        } else {
            0.0
        }
    }

    fn knob(&self) -> Rect {
        let track = self.track();
        let x = track.x + (f64::from(track.width) * self.fraction()).round() as i32;
        Rect {
            x: x - KNOB_WIDTH as i32 / 2,
            y: self.bounds.y + 2,
            width: KNOB_WIDTH,
            height: self.bounds.height.saturating_sub(4),
        }
    }

    fn value_at(&self, x: i32) -> f64 {
        let track = self.track();
        let fraction = f64::from(x - track.x) / f64::from(track.width.max(1));
        self.min + fraction.clamp(0.0, 1.0) * (self.max - self.min)
    }

    fn report(&mut self, value: f64, last: bool) -> ValueOutcome {
        if self.set_value(value) || last {
            ValueOutcome::Value {
                value: self.value,
                last,
            }
        } else {
            ValueOutcome::Ignored
        }
    }

    pub fn handle(&mut self, event: &Event) -> ValueOutcome {
        if !self.enabled {
            return ValueOutcome::Ignored;
        }
        match event {
            Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
            } if self.bounds.contains(*x, *y) => {
                self.dragging = true;
                let outcome = self.report(self.value_at(*x), false);
                if !self.focused {
                    ValueOutcome::FocusRequested
                } else if outcome == ValueOutcome::Ignored {
                    ValueOutcome::Changed
                } else {
                    outcome
                }
            }
            Event::PointerMoved { x, .. } if self.dragging => self.report(self.value_at(*x), false),
            Event::PointerReleased {
                x,
                button: PointerButton::Primary,
                ..
            } if self.dragging => {
                self.dragging = false;
                self.report(self.value_at(*x), true)
            }
            Event::KeyPressed { key, .. } if self.focused => {
                let page = ((self.max - self.min) / 10.0).max(self.step);
                let target = match key {
                    Key::Left | Key::Down => self.value - self.step,
                    Key::Right | Key::Up => self.value + self.step,
                    Key::PageDown => self.value - page,
                    Key::PageUp => self.value + page,
                    Key::Home => self.min,
                    Key::End => self.max,
                    _ => return ValueOutcome::Ignored,
                };
                self.report(target, true)
            }
            Event::Accessibility(AccessibilityRequest::Increment(id)) if *id == self.id => {
                self.report(self.value + self.step, true)
            }
            Event::Accessibility(AccessibilityRequest::Decrement(id)) if *id == self.id => {
                self.report(self.value - self.step, true)
            }
            _ => ValueOutcome::Ignored,
        }
    }

    pub fn paint(&self, surface: &RasterSurface) -> Result<(), RenderError> {
        let colors = palette();
        let track = self.track();
        surface.fill_rect(track, colors.track)?;
        surface.stroke_rect(track, colors.bevel_shadow)?;
        let filled = Rect {
            width: (self.knob().x + KNOB_WIDTH as i32 / 2 - track.x).max(0) as u32,
            ..track
        };
        if self.enabled {
            surface.fill_rect(filled, colors.thumb)?;
        }
        let knob = self.knob();
        surface.fill_rect(knob, colors.surface)?;
        surface.stroke_rect(
            knob,
            if self.enabled {
                colors.border
            } else {
                colors.ink_disabled
            },
        )?;
        if self.focused && self.enabled {
            super::button::paint_focus_ring(
                surface,
                Rect {
                    x: self.bounds.x - 2,
                    y: self.bounds.y - 1,
                    width: self.bounds.width + 4,
                    height: self.bounds.height + 2,
                },
            )?;
        }
        Ok(())
    }

    pub fn accessibility_node(&self) -> accesskit::Node {
        let mut node = accesskit::Node::new(accesskit::Role::Slider);
        node.set_label(self.label.as_str());
        node.set_bounds(super::bounds_of(self.bounds));
        node.set_numeric_value(self.value);
        node.set_min_numeric_value(self.min);
        node.set_max_numeric_value(self.max);
        node.set_numeric_value_step(self.step);
        if self.enabled {
            node.add_action(accesskit::Action::Focus);
            node.add_action(accesskit::Action::Increment);
            node.add_action(accesskit::Action::Decrement);
        } else {
            node.set_disabled();
        }
        node
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToggleOutcome {
    Ignored,
    Changed,
    Toggled(bool),
    FocusRequested,
}

/// A labelled check box (also used for on/off switches in panels).
#[derive(Debug, Clone)]
pub struct Checkbox {
    pub id: u64,
    pub label: String,
    pub bounds: Rect,
    pub checked: bool,
    pub enabled: bool,
    pub focused: bool,
    pub font: Font,
    pressed: bool,
}

impl Checkbox {
    pub fn new(id: u64, label: &str, checked: bool) -> Self {
        let font = Font::system(12.0);
        let width = measure(label, &font).width as u32 + 22;
        Self {
            id,
            label: label.to_owned(),
            bounds: Rect {
                x: 0,
                y: 0,
                width,
                height: 18,
            },
            checked,
            enabled: true,
            focused: false,
            font,
            pressed: false,
        }
    }

    pub fn at(mut self, x: i32, y: i32) -> Self {
        self.bounds.x = x;
        self.bounds.y = y;
        self
    }

    fn toggle(&mut self) -> ToggleOutcome {
        self.checked = !self.checked;
        ToggleOutcome::Toggled(self.checked)
    }

    pub fn handle(&mut self, event: &Event) -> ToggleOutcome {
        if !self.enabled {
            return ToggleOutcome::Ignored;
        }
        match event {
            Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
            } if self.bounds.contains(*x, *y) => {
                self.pressed = true;
                ToggleOutcome::Changed
            }
            Event::PointerReleased {
                x,
                y,
                button: PointerButton::Primary,
            } if self.pressed => {
                self.pressed = false;
                if self.bounds.contains(*x, *y) {
                    self.toggle()
                } else {
                    ToggleOutcome::Changed
                }
            }
            Event::KeyPressed {
                key: Key::Space, ..
            } if self.focused => self.toggle(),
            Event::Accessibility(AccessibilityRequest::Click(id)) if *id == self.id => {
                self.toggle()
            }
            _ => ToggleOutcome::Ignored,
        }
    }

    pub fn paint(&self, surface: &RasterSurface) -> Result<(), RenderError> {
        let colors = palette();
        let ink = if self.enabled {
            colors.ink
        } else {
            colors.ink_disabled
        };
        let size = 12;
        let check_box = Rect {
            x: self.bounds.x,
            y: self.bounds.y + (self.bounds.height as i32 - size) / 2,
            width: size as u32,
            height: size as u32,
        };
        surface.fill_rect(
            check_box,
            if self.pressed {
                colors.bevel_shadow
            } else {
                colors.content
            },
        )?;
        surface.stroke_rect(check_box, ink)?;
        if self.checked {
            // A Classic "X"-free check: two strokes drawn with 2x2 dots.
            let points = [
                (2, 6),
                (3, 7),
                (4, 8),
                (5, 7),
                (6, 6),
                (7, 5),
                (8, 4),
                (9, 3),
            ];
            for (dx, dy) in points {
                surface.fill_rect(
                    Rect {
                        x: check_box.x + dx,
                        y: check_box.y + dy - 1,
                        width: 2,
                        height: 2,
                    },
                    ink,
                )?;
            }
        }
        let line_height = measure("Ag", &self.font).height as i32;
        let text_y = self.bounds.y + (self.bounds.height as i32 - line_height) / 2;
        surface.draw_text(&self.label, self.bounds.x + 18, text_y, &self.font, ink)?;
        if self.focused && self.enabled {
            super::button::paint_focus_ring(
                surface,
                Rect {
                    x: self.bounds.x - 2,
                    y: self.bounds.y - 1,
                    width: self.bounds.width + 4,
                    height: self.bounds.height + 2,
                },
            )?;
        }
        Ok(())
    }

    pub fn accessibility_node(&self) -> accesskit::Node {
        let mut node = accesskit::Node::new(accesskit::Role::CheckBox);
        node.set_label(self.label.as_str());
        node.set_bounds(super::bounds_of(self.bounds));
        node.set_toggled(if self.checked {
            accesskit::Toggled::True
        } else {
            accesskit::Toggled::False
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

/// A determinate progress bar, or an indeterminate one drawn as a static
/// stripe pattern (no animation timer).
#[derive(Debug, Clone)]
pub struct ProgressBar {
    pub id: u64,
    pub label: String,
    pub bounds: Rect,
    /// `Some(0.0..=1.0)` for determinate progress, `None` for unknown.
    pub fraction: Option<f64>,
}

impl ProgressBar {
    pub fn new(id: u64, label: &str) -> Self {
        Self {
            id,
            label: label.to_owned(),
            bounds: Rect {
                x: 0,
                y: 0,
                width: 200,
                height: 12,
            },
            fraction: Some(0.0),
        }
    }

    pub fn paint(&self, surface: &RasterSurface) -> Result<(), RenderError> {
        let colors = palette();
        surface.fill_rect(self.bounds, colors.track)?;
        let inner = Rect {
            x: self.bounds.x + 1,
            y: self.bounds.y + 1,
            width: self.bounds.width.saturating_sub(2),
            height: self.bounds.height.saturating_sub(2),
        };
        match self.fraction {
            Some(fraction) => {
                let fraction = if fraction.is_nan() {
                    0.0
                } else {
                    fraction.clamp(0.0, 1.0)
                };
                surface.fill_rect(
                    Rect {
                        width: (f64::from(inner.width) * fraction).round() as u32,
                        ..inner
                    },
                    colors.menu_highlight,
                )?;
            }
            None => {
                surface.with_clip(inner, || {
                    let mut x = inner.x;
                    while x < inner.x + inner.width as i32 {
                        surface.fill_rect(
                            Rect {
                                x,
                                width: 6,
                                ..inner
                            },
                            colors.thumb,
                        )?;
                        x += 12;
                    }
                    Ok(())
                })?;
            }
        }
        surface.stroke_rect(self.bounds, colors.border)
    }

    pub fn accessibility_node(&self) -> accesskit::Node {
        let mut node = accesskit::Node::new(accesskit::Role::ProgressIndicator);
        node.set_label(self.label.as_str());
        node.set_bounds(super::bounds_of(self.bounds));
        if let Some(fraction) = self.fraction {
            node.set_numeric_value((fraction.clamp(0.0, 1.0) * 100.0).round());
            node.set_min_numeric_value(0.0);
            node.set_max_numeric_value(100.0);
        }
        node
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::color::Color;
    use crate::foundation::geometry::Scale;

    fn key(key: Key) -> Event {
        Event::KeyPressed {
            key,
            modifiers: Default::default(),
        }
    }

    #[test]
    fn slider_snaps_clamps_and_reports_drags() {
        let mut slider = Slider::new(4, "Volume", 0.0, 100.0, 5.0);
        slider.bounds = Rect {
            x: 0,
            y: 0,
            width: 111,
            height: 20,
        };
        assert!(slider.set_value(42.0));
        assert_eq!(slider.value(), 40.0);
        assert!(!slider.set_value(f64::NAN));
        slider.set_value(1e9);
        assert_eq!(slider.value(), 100.0);
        slider.focused = true;
        assert_eq!(
            slider.handle(&key(Key::Left)),
            ValueOutcome::Value {
                value: 95.0,
                last: true
            }
        );
        slider.handle(&key(Key::Home));
        assert_eq!(slider.value(), 0.0);
        slider.handle(&Event::PointerPressed {
            x: 55,
            y: 10,
            button: PointerButton::Primary,
        });
        assert!(slider.dragging());
        assert_eq!(slider.value(), 50.0);
        slider.handle(&Event::PointerMoved { x: 1000, y: 10 });
        assert_eq!(slider.value(), 100.0);
        assert_eq!(
            slider.handle(&Event::PointerReleased {
                x: 1000,
                y: 10,
                button: PointerButton::Primary
            }),
            ValueOutcome::Value {
                value: 100.0,
                last: true
            }
        );
        assert_eq!(
            slider.handle(&Event::Accessibility(AccessibilityRequest::Decrement(4))),
            ValueOutcome::Value {
                value: 95.0,
                last: true
            }
        );
        slider.enabled = false;
        assert_eq!(slider.handle(&key(Key::End)), ValueOutcome::Ignored);
    }

    #[test]
    fn checkbox_toggles_by_click_space_and_accessibility() {
        let mut check = Checkbox::new(5, "Wi-Fi", false).at(10, 10);
        let inside = (12, 15);
        check.handle(&Event::PointerPressed {
            x: inside.0,
            y: inside.1,
            button: PointerButton::Primary,
        });
        assert_eq!(
            check.handle(&Event::PointerReleased {
                x: inside.0,
                y: inside.1,
                button: PointerButton::Primary
            }),
            ToggleOutcome::Toggled(true)
        );
        // Drag out cancels.
        check.handle(&Event::PointerPressed {
            x: inside.0,
            y: inside.1,
            button: PointerButton::Primary,
        });
        assert_eq!(
            check.handle(&Event::PointerReleased {
                x: 500,
                y: 500,
                button: PointerButton::Primary
            }),
            ToggleOutcome::Changed
        );
        assert!(check.checked);
        assert_eq!(check.handle(&key(Key::Space)), ToggleOutcome::Ignored);
        check.focused = true;
        assert_eq!(
            check.handle(&key(Key::Space)),
            ToggleOutcome::Toggled(false)
        );
        assert_eq!(
            check.handle(&Event::Accessibility(AccessibilityRequest::Click(5))),
            ToggleOutcome::Toggled(true)
        );
        assert_eq!(
            check.accessibility_node().toggled(),
            Some(accesskit::Toggled::True)
        );
    }

    #[test]
    fn progress_paints_fraction_and_handles_bad_values() {
        let surface = RasterSurface::new(220, 20, Scale::new(1).unwrap()).unwrap();
        surface.clear(Color::WHITE).unwrap();
        let mut bar = ProgressBar::new(6, "Download");
        bar.bounds = Rect {
            x: 0,
            y: 0,
            width: 202,
            height: 12,
        };
        bar.fraction = Some(0.5);
        bar.paint(&surface).unwrap();
        assert_eq!(surface.pixel(50, 6), Some(0xff333399));
        assert_eq!(surface.pixel(150, 6), Some(0xffeeeeee));
        bar.fraction = Some(f64::NAN);
        bar.paint(&surface).unwrap();
        bar.fraction = None;
        bar.paint(&surface).unwrap();
        assert_eq!(bar.accessibility_node().numeric_value(), None);
    }
}
