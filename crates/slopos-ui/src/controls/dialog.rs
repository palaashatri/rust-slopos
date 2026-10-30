//! Modal dialog drawn inside the app window: a title, a wrapped message, an
//! optional text field and a row of buttons.
//!
//! While open, the dialog takes every input event, so the window behind it
//! cannot be used (application-modal). Enter chooses the default button,
//! Escape the cancel button, Tab cycles focus between the field and the
//! buttons. The window behind is dimmed with the theme's scrim.

use super::button::{Button, ButtonOutcome};
use super::focus::FocusChain;
use super::text_field::{TextField, TextFieldOutcome};
use crate::event::{AccessibilityRequest, Event, Key};
use crate::foundation::geometry::Rect;
use crate::primitives::button_plate::ButtonPlateVariant;
use crate::render::{RasterSurface, RenderError};
use crate::text::{measure, Font};
use crate::theme::palette;

const WIDTH: u32 = 360;
const MARGIN: i32 = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogOutcome {
    /// No dialog is open; the event is for the window.
    Ignored,
    /// The dialog used the event.
    Changed,
    /// Button `index` was chosen; the dialog closed. `text` is the field's
    /// contents when the dialog had one.
    Chosen {
        index: usize,
        text: Option<String>,
    },
    /// Copy text to the clipboard / request a paste for the field.
    Copy(String),
    PasteRequested,
}

#[derive(Debug, Clone)]
pub struct Dialog {
    pub id: u64,
    pub title: String,
    pub message: String,
    pub buttons: Vec<Button>,
    pub field: Option<TextField>,
    /// Index of the button Enter chooses.
    pub default: usize,
    /// Index of the button Escape chooses.
    pub cancel: usize,
    pub font: Font,
    bounds: Rect,
    focus: FocusChain,
    lines: Vec<String>,
    /// Laid out by [`Self::layout_standalone`]: no scrim behind it.
    standalone: bool,
}

impl Dialog {
    /// A dialog with `buttons` labels left to right; the last is the default
    /// and the first is the cancel button.
    pub fn new(id: u64, title: &str, message: &str, buttons: &[&str]) -> Self {
        let count = buttons.len().max(1);
        let buttons = buttons
            .iter()
            .enumerate()
            .map(|(index, label)| {
                let variant = if index + 1 == count {
                    ButtonPlateVariant::Default
                } else {
                    ButtonPlateVariant::Regular
                };
                Button::new(id + 10 + index as u64, label, variant)
            })
            .collect();
        Self {
            id,
            title: title.to_owned(),
            message: message.to_owned(),
            buttons,
            field: None,
            default: count - 1,
            cancel: 0,
            font: Font::system(12.0),
            bounds: Rect::default(),
            focus: FocusChain::default(),
            lines: Vec::new(),
            standalone: false,
        }
    }

    /// Add a text field pre-filled with `text`, all selected.
    pub fn with_field(mut self, label: &str, text: &str) -> Self {
        let mut field = TextField::new(self.id + 1, label);
        field.set_text(text);
        field.select_all();
        self.field = Some(field);
        self
    }

    pub fn bounds(&self) -> Rect {
        self.bounds
    }

    /// Make button `index` the one Enter chooses and the one drawn as
    /// default. Destructive prompts point this at Cancel.
    pub fn set_default(&mut self, index: usize) {
        if index >= self.buttons.len() {
            return;
        }
        self.default = index;
        for (i, button) in self.buttons.iter_mut().enumerate() {
            button.variant = if i == index {
                ButtonPlateVariant::Default
            } else {
                ButtonPlateVariant::Regular
            };
        }
    }

    pub fn field_text(&self) -> Option<&str> {
        self.field.as_ref().map(TextField::text)
    }

    /// Lay out centred in a window of logical size `window`, and give the
    /// field (or the default button) focus. Call before the first event and
    /// whenever the window is resized.
    pub fn layout(&mut self, window: (u32, u32)) {
        self.standalone = false;
        self.layout_at(window, None);
    }

    /// Lay out at the top-left of its own surface (a popup window), at most
    /// `max_width` wide, with no scrim. Returns the surface size it needs,
    /// shadow included.
    pub fn layout_standalone(&mut self, max_width: u32) -> (u32, u32) {
        self.standalone = true;
        self.layout_at((max_width + 16, u32::MAX / 4), Some((0, 0)));
        (self.bounds.width + 3, self.bounds.height + 3)
    }

    fn layout_at(&mut self, window: (u32, u32), origin: Option<(i32, i32)>) {
        // Wide enough for the button row, within the window.
        let buttons: u32 = self
            .buttons
            .iter()
            .map(|button| Self::button_width(button, &self.font) + 10)
            .sum::<u32>()
            .saturating_sub(10);
        let width = WIDTH
            .max(buttons + 2 * MARGIN as u32)
            .min(window.0.saturating_sub(16))
            .max(160);
        self.lines = wrap(&self.message, &self.font, width - 2 * MARGIN as u32);
        let line_height = measure("Ag", &self.font).height as i32;
        let title_height = line_height + 8;
        let message_height = self.lines.len() as i32 * line_height;
        let field_height = if self.field.is_some() { 30 } else { 0 };
        let height =
            (MARGIN + title_height + message_height + 12 + field_height + 20 + MARGIN) as u32;
        let (x, y) = origin.unwrap_or((
            (window.0 as i32 - width as i32) / 2,
            ((window.1 as i32 - height as i32) / 3).max(0),
        ));
        self.bounds = Rect {
            x,
            y,
            width,
            height,
        };
        let mut cursor_y = y + MARGIN + title_height + message_height + 12;
        if let Some(field) = &mut self.field {
            field.bounds = Rect {
                x: x + MARGIN,
                y: cursor_y,
                width: width - 2 * MARGIN as u32,
                height: 22,
            };
            cursor_y += field_height;
        }
        let mut right = x + width as i32 - MARGIN;
        for button in self.buttons.iter_mut().rev() {
            let button_width = Self::button_width(button, &self.font);
            right -= button_width as i32;
            button.bounds = Rect {
                x: right,
                y: cursor_y,
                width: button_width,
                height: 20,
            };
            right -= 10;
        }
        let mut ids: Vec<u64> = self.field.iter().map(|field| field.id).collect();
        ids.extend(self.buttons.iter().map(|button| button.id));
        let focused = self.focus.focused();
        self.focus.set_ids(ids);
        if focused.is_none() {
            let first = self
                .field
                .as_ref()
                .map(|field| field.id)
                .or_else(|| self.buttons.get(self.default).map(|button| button.id));
            if let Some(first) = first {
                self.focus.focus(first);
            }
        }
        self.sync_focus();
    }

    fn button_width(button: &Button, font: &Font) -> u32 {
        let label_width = measure(&button.label, font).width as u32 + 24;
        label_width.max(button.variant.dimensions().width.min(80))
    }

    fn sync_focus(&mut self) {
        let focused = self.focus.focused();
        if let Some(field) = &mut self.field {
            field.focused = focused == Some(field.id);
        }
        for button in &mut self.buttons {
            button.focused = focused == Some(button.id);
        }
    }

    fn choose(&self, index: usize) -> DialogOutcome {
        DialogOutcome::Chosen {
            index,
            text: self.field_text().map(str::to_owned),
        }
    }

    pub fn handle(&mut self, event: &Event) -> DialogOutcome {
        if self.focus.handle(event).is_some() {
            self.sync_focus();
            return DialogOutcome::Changed;
        }
        let field_focused = self.field.as_ref().is_some_and(|field| field.focused);
        match event {
            Event::KeyPressed {
                key: Key::Escape, ..
            } => return self.choose(self.cancel),
            Event::KeyPressed {
                key: Key::Enter, ..
            } if field_focused || !self.buttons.iter().any(|button| button.focused) => {
                return self.choose(self.default)
            }
            Event::Accessibility(AccessibilityRequest::Focus(id)) => {
                if self.focus.focus(*id) {
                    self.sync_focus();
                }
                return DialogOutcome::Changed;
            }
            Event::Accessibility(AccessibilityRequest::SetValue(id, value)) => {
                if let Some(field) = self.field.as_mut().filter(|field| field.id == *id) {
                    field.set_text(value);
                }
                return DialogOutcome::Changed;
            }
            _ => {}
        }
        if let Some(field) = &mut self.field {
            match field.handle(event) {
                TextFieldOutcome::Ignored => {}
                TextFieldOutcome::FocusRequested => {
                    let id = field.id;
                    self.focus.focus(id);
                    self.sync_focus();
                    return DialogOutcome::Changed;
                }
                TextFieldOutcome::Copy(text) => return DialogOutcome::Copy(text),
                TextFieldOutcome::PasteRequested => return DialogOutcome::PasteRequested,
                TextFieldOutcome::Submitted => return self.choose(self.default),
                TextFieldOutcome::Cancelled => return self.choose(self.cancel),
                _ => return DialogOutcome::Changed,
            }
        }
        for index in 0..self.buttons.len() {
            let activated = match event {
                Event::Accessibility(AccessibilityRequest::Click(id)) => {
                    *id == self.buttons[index].id
                }
                _ => self.buttons[index].handle(event) == ButtonOutcome::Activated,
            };
            if activated {
                return self.choose(index);
            }
        }
        // Modal: everything else is swallowed.
        DialogOutcome::Changed
    }

    pub fn paint(&mut self, surface: &RasterSurface) -> Result<(), RenderError> {
        let colors = palette();
        let window = Rect {
            x: 0,
            y: 0,
            width: surface.logical_width(),
            height: surface.logical_height(),
        };
        if !self.standalone {
            surface.fill_rect(window, colors.scrim)?;
        }
        let bounds = self.bounds;
        surface.fill_rect(
            Rect {
                x: bounds.x + 3,
                y: bounds.y + 3,
                ..bounds
            },
            colors.bevel_shadow,
        )?;
        surface.fill_rect(bounds, colors.surface)?;
        surface.stroke_rect(bounds, colors.border)?;
        surface.stroke_rect(
            Rect {
                x: bounds.x + 3,
                y: bounds.y + 3,
                width: bounds.width.saturating_sub(6),
                height: bounds.height.saturating_sub(6),
            },
            colors.bevel_shadow,
        )?;
        let line_height = measure("Ag", &self.font).height as i32;
        let x = bounds.x + MARGIN;
        let mut y = bounds.y + MARGIN;
        surface.draw_text(&self.title, x, y, &self.font.clone().bold(), colors.ink)?;
        y += line_height + 8;
        for line in &self.lines {
            surface.draw_text(line, x, y, &self.font, colors.ink)?;
            y += line_height;
        }
        if let Some(field) = &mut self.field {
            field.paint(surface)?;
        }
        for button in &self.buttons {
            button.paint(surface, &self.font)?;
        }
        Ok(())
    }

    pub fn focus_node(&self) -> u64 {
        self.focus.focused().unwrap_or(self.id)
    }

    pub fn accessibility_nodes(&self) -> Vec<(u64, accesskit::Node)> {
        let mut dialog = accesskit::Node::new(if self.field.is_some() {
            accesskit::Role::Dialog
        } else {
            accesskit::Role::AlertDialog
        });
        dialog.set_label(self.title.as_str());
        dialog.set_description(self.message.as_str());
        dialog.set_modal();
        dialog.set_bounds(super::bounds_of(self.bounds));
        let mut nodes = Vec::new();
        if let Some(field) = &self.field {
            nodes.push((field.id, field.accessibility_node()));
        }
        nodes.extend(
            self.buttons
                .iter()
                .map(|button| (button.id, button.accessibility_node())),
        );
        dialog.set_children(
            nodes
                .iter()
                .map(|(id, _)| accesskit::NodeId(*id))
                .collect::<Vec<_>>(),
        );
        nodes.insert(0, (self.id, dialog));
        nodes
    }
}

/// Greedy word wrap of `text` to `width` logical pixels; explicit newlines
/// start new lines and a single over-long word gets its own line.
pub fn wrap(text: &str, font: &Font, width: u32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if line.is_empty() {
                word.to_owned()
            } else {
                format!("{line} {word}")
            };
            if measure(&candidate, font).width as u32 > width && !line.is_empty() {
                lines.push(std::mem::replace(&mut line, word.to_owned()));
            } else {
                line = candidate;
            }
        }
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Modifiers;
    use crate::foundation::geometry::Scale;

    fn key(key: Key) -> Event {
        Event::KeyPressed {
            key,
            modifiers: Modifiers::default(),
        }
    }

    #[test]
    fn enter_and_escape_choose_default_and_cancel() {
        let mut dialog = Dialog::new(
            50,
            "Replace?",
            "An item named “a.txt” already exists.",
            &["Cancel", "Replace"],
        );
        dialog.layout((640, 480));
        assert!(dialog.buttons[1].focused);
        assert_eq!(
            dialog.handle(&key(Key::Escape)),
            DialogOutcome::Chosen {
                index: 0,
                text: None
            }
        );
        assert_eq!(
            dialog.handle(&key(Key::Enter)),
            DialogOutcome::Chosen {
                index: 1,
                text: None
            }
        );
        // Tab to Cancel, Space activates the focused button.
        dialog.handle(&key(Key::Tab));
        assert!(dialog.buttons[0].focused);
        assert_eq!(
            dialog.handle(&key(Key::Space)),
            DialogOutcome::Chosen {
                index: 0,
                text: None
            }
        );
        // Other input is swallowed while modal.
        assert_eq!(
            dialog.handle(&Event::Scroll { dx: 0, dy: 3 }),
            DialogOutcome::Changed
        );
    }

    #[test]
    fn field_dialog_returns_edited_text_and_paints_at_both_scales() {
        let mut dialog = Dialog::new(60, "Rename", "New name:", &["Cancel", "Rename"])
            .with_field("Name", "old.txt");
        dialog.layout((400, 300));
        assert!(dialog.field.as_ref().unwrap().focused);
        for c in "new.txt".chars() {
            dialog.handle(&key(Key::Character(c)));
        }
        assert_eq!(
            dialog.handle(&key(Key::Enter)),
            DialogOutcome::Chosen {
                index: 1,
                text: Some("new.txt".into())
            }
        );
        assert_eq!(
            dialog.handle(&Event::Accessibility(AccessibilityRequest::Click(70))),
            DialogOutcome::Chosen {
                index: 0,
                text: Some("new.txt".into())
            }
        );
        for factor in [1, 2] {
            let surface = RasterSurface::new(400, 300, Scale::new(factor).unwrap()).unwrap();
            surface
                .clear(crate::foundation::color::Color::WHITE)
                .unwrap();
            dialog.paint(&surface).unwrap();
        }
        let nodes = dialog.accessibility_nodes();
        assert_eq!(nodes[0].1.role(), accesskit::Role::Dialog);
        assert!(nodes[0].1.is_modal());
    }

    #[test]
    fn many_buttons_widen_the_dialog_and_stay_inside_it() {
        let mut dialog = Dialog::new(
            80,
            "Conflict",
            "Pick one.",
            &["Cancel", "Skip", "Keep Both", "Replace"],
        );
        dialog.layout((760, 540));
        let bounds = dialog.bounds();
        for button in &dialog.buttons {
            assert!(
                button.bounds.x >= bounds.x + MARGIN,
                "{} leaks left",
                button.label
            );
            assert!(button.bounds.x + button.bounds.width as i32 <= bounds.x + bounds.width as i32);
        }
        // A narrow window still keeps the dialog on screen.
        dialog.layout((200, 300));
        assert!(dialog.bounds().width <= 184);
    }

    #[test]
    fn wrapping_respects_width_and_newlines() {
        let font = Font::system(12.0);
        let lines = wrap("one two three four five six seven\nnext", &font, 80);
        assert!(lines.len() >= 3);
        assert_eq!(lines.last().unwrap(), "next");
        for line in &lines {
            assert!(measure(line, &font).width as u32 <= 80 || !line.contains(' '));
        }
        assert_eq!(wrap("", &font, 80), vec![String::new()]);
    }

    #[test]
    fn standalone_dialogs_sit_at_the_origin_without_a_scrim() {
        use crate::foundation::geometry::Scale;
        let mut dialog = Dialog::new(
            5,
            "Shut Down",
            "Shut down this computer now?",
            &["Cancel", "Shut Down"],
        );
        let (width, height) = dialog.layout_standalone(420);
        assert_eq!((dialog.bounds().x, dialog.bounds().y), (0, 0));
        assert_eq!(
            (width, height),
            (dialog.bounds().width + 3, dialog.bounds().height + 3)
        );
        assert!(dialog.buttons.iter().all(|b| b.bounds.x >= 0
            && b.bounds.x + b.bounds.width as i32 <= dialog.bounds().width as i32
            && b.bounds.y + b.bounds.height as i32 <= dialog.bounds().height as i32));
        let surface = RasterSurface::new(width, height, Scale::new(1).unwrap()).unwrap();
        surface
            .clear(crate::foundation::color::Color::WHITE)
            .unwrap();
        dialog.paint(&surface).unwrap();
        // The shadow corner (outside the dialog body) is untouched: no scrim.
        assert_eq!(surface.pixel(width - 1, 0), Some(0xffff_ffff));
    }
}
