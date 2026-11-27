//! Modal "Open With" chooser: a list of applications that can open a file,
//! with the default one marked, and Cancel / Open.
//!
//! The caller supplies the items (the application lookup lives with the
//! desktop-entry code, not in the toolkit). Activating a row with Enter or a
//! double click, or pressing Open, chooses it.

use super::button::{Button, ButtonOutcome};
use super::focus::FocusChain;
use super::list::{ListOutcome, ListView};
use crate::event::{AccessibilityRequest, Event, Key, Modifiers};
use crate::foundation::geometry::Rect;
use crate::primitives::button_plate::ButtonPlateVariant;
use crate::render::{RasterSurface, RenderError};
use crate::text::Font;
use crate::theme::palette;

const ROW_HEIGHT: u32 = 18;
const ID_LIST: u64 = 1;
const ID_CANCEL: u64 = 2;
const ID_OPEN: u64 = 3;

/// One application offered in the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenWithItem {
    pub label: String,
    /// The application the system opens the type with by default.
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenWithOutcome {
    Ignored,
    Changed,
    /// The index into the items the user chose.
    Chosen(usize),
    Cancelled,
}

#[derive(Debug)]
pub struct OpenWithChooser {
    pub id: u64,
    pub title: String,
    pub font: Font,
    subject: String,
    items: Vec<OpenWithItem>,
    status: Option<String>,
    list: ListView,
    cancel: Button,
    open: Button,
    focus: FocusChain,
    bounds: Rect,
}

impl OpenWithChooser {
    /// `file_name` is shown in the prompt, for example "photo.png".
    pub fn new(id: u64, title: &str, file_name: &str, items: Vec<OpenWithItem>) -> Self {
        let status = items
            .is_empty()
            .then(|| format!("No application can open “{file_name}”."));
        let mut chooser = Self {
            id,
            title: title.to_owned(),
            font: Font::system(12.0),
            subject: file_name.to_owned(),
            list: ListView::new(id + ID_LIST, "Applications", ROW_HEIGHT),
            cancel: Button::new(id + ID_CANCEL, "Cancel", ButtonPlateVariant::Regular),
            open: Button::new(id + ID_OPEN, "Open", ButtonPlateVariant::Default),
            focus: FocusChain::new(vec![id + ID_LIST, id + ID_CANCEL, id + ID_OPEN]),
            bounds: Rect::default(),
            status,
            items,
        };
        chooser.list.reset(chooser.items.len());
        if !chooser.items.is_empty() {
            // Start on the default application when there is one.
            let start = chooser
                .items
                .iter()
                .position(|item| item.is_default)
                .unwrap_or(0);
            chooser.list.select_only(start);
        }
        chooser.focus.focus(id + ID_LIST);
        chooser.sync_focus();
        chooser
    }

    pub fn items(&self) -> &[OpenWithItem] {
        &self.items
    }

    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    /// The row labels, with the default application marked.
    fn labels(&self) -> Vec<String> {
        self.items
            .iter()
            .map(|item| {
                if item.is_default {
                    format!("{} (default)", item.label)
                } else {
                    item.label.clone()
                }
            })
            .collect()
    }

    fn sync_focus(&mut self) {
        let focused = self.focus.focused();
        self.list.focused = focused == Some(self.list.id);
        self.cancel.focused = focused == Some(self.cancel.id);
        self.open.focused = focused == Some(self.open.id);
    }

    fn choose_cursor(&mut self) -> OpenWithOutcome {
        match self.list.cursor() {
            Some(index) if index < self.items.len() => OpenWithOutcome::Chosen(index),
            _ => {
                self.status = Some("Choose an application first.".into());
                OpenWithOutcome::Changed
            }
        }
    }

    /// Lay out to fill most of a window of logical size `window`.
    pub fn layout(&mut self, window: (u32, u32)) {
        let width = window.0.saturating_sub(40).clamp(240, 480);
        let height = window.1.saturating_sub(40).clamp(160, 360);
        let x = (window.0 as i32 - width as i32) / 2;
        let y = (window.1 as i32 - height as i32) / 2;
        self.bounds = Rect {
            x,
            y,
            width,
            height,
        };
        let inner_x = x + 12;
        let inner_width = width - 24;
        let buttons_y = y + height as i32 - 32;
        let list_top = y + 52;
        self.list.set_bounds(Rect {
            x: inner_x + 1,
            y: list_top,
            width: inner_width - 2,
            height: (buttons_y - 18 - list_top).max(ROW_HEIGHT as i32) as u32,
        });
        self.open.bounds = Rect {
            x: x + width as i32 - 12 - 80,
            y: buttons_y,
            width: 80,
            height: 20,
        };
        self.cancel.bounds = Rect {
            x: self.open.bounds.x - 90,
            y: buttons_y,
            width: 80,
            height: 20,
        };
    }

    pub fn handle(&mut self, event: &Event, modifiers: Modifiers) -> OpenWithOutcome {
        if self.focus.handle(event).is_some() {
            self.sync_focus();
            return OpenWithOutcome::Changed;
        }
        match event {
            Event::KeyPressed {
                key: Key::Escape, ..
            } => return OpenWithOutcome::Cancelled,
            Event::Accessibility(AccessibilityRequest::Focus(id)) => {
                if self.focus.focus(*id) {
                    self.sync_focus();
                }
                return OpenWithOutcome::Changed;
            }
            Event::Accessibility(AccessibilityRequest::Click(id)) if *id == self.cancel.id => {
                return OpenWithOutcome::Cancelled
            }
            Event::Accessibility(AccessibilityRequest::Click(id)) if *id == self.open.id => {
                return self.choose_cursor();
            }
            Event::Accessibility(AccessibilityRequest::Click(id)) => {
                if let Some(index) = self.list.row_for_node(*id) {
                    if index < self.items.len() {
                        return OpenWithOutcome::Chosen(index);
                    }
                }
            }
            _ => {}
        }
        let labels = self.labels();
        let label = |index: usize| labels.get(index).cloned().unwrap_or_default();
        match self.list.handle(event, modifiers, &label) {
            ListOutcome::Activated(index) if index < self.items.len() => {
                return OpenWithOutcome::Chosen(index)
            }
            ListOutcome::FocusRequested => {
                self.focus.focus(self.list.id);
                self.sync_focus();
                return OpenWithOutcome::Changed;
            }
            ListOutcome::Ignored => {}
            _ => return OpenWithOutcome::Changed,
        }
        if self.cancel.handle(event) == ButtonOutcome::Activated {
            return OpenWithOutcome::Cancelled;
        }
        if self.open.handle(event) == ButtonOutcome::Activated {
            return self.choose_cursor();
        }
        OpenWithOutcome::Changed
    }

    pub fn paint(&mut self, surface: &RasterSurface) -> Result<(), RenderError> {
        let colors = palette();
        surface.fill_rect(
            Rect {
                x: 0,
                y: 0,
                width: surface.logical_width(),
                height: surface.logical_height(),
            },
            colors.scrim,
        )?;
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
        surface.draw_text(
            &self.title,
            bounds.x + 12,
            bounds.y + 8,
            &self.font.clone().bold(),
            colors.ink,
        )?;
        surface.draw_text(
            &format!("Choose an application to open “{}”.", self.subject),
            bounds.x + 12,
            bounds.y + 28,
            &self.font,
            colors.ink_secondary,
        )?;
        let font = &self.font;
        let items = &self.items;
        let line_height = crate::text::measure("Ag", font).height as i32;
        self.list.paint(surface, |surface, index, rect, _| {
            let y = rect.y + (rect.height as i32 - line_height) / 2;
            let label = items
                .get(index)
                .map(|item| item.label.as_str())
                .unwrap_or_default();
            surface.draw_text(label, rect.x + 6, y, font, colors.ink)?;
            if items.get(index).is_some_and(|item| item.is_default) {
                let default = "Default";
                let width = crate::text::measure(default, font).width as i32;
                surface.draw_text(
                    default,
                    rect.x + rect.width as i32 - width - 6,
                    y,
                    font,
                    colors.ink_secondary,
                )?;
            }
            Ok(())
        })?;
        if let Some(status) = &self.status {
            let list = self.list.bounds();
            surface.draw_text(
                status,
                list.x,
                list.y + list.height as i32 + 2,
                &self.font,
                colors.danger,
            )?;
        }
        self.cancel.paint(surface, &self.font)?;
        self.open.paint(surface, &self.font)?;
        Ok(())
    }

    pub fn focus_node(&self) -> u64 {
        if self.list.focused {
            self.list.focus_node()
        } else {
            self.focus.focused().unwrap_or(self.id)
        }
    }

    pub fn accessibility_nodes(&self) -> Vec<(u64, accesskit::Node)> {
        let mut dialog = accesskit::Node::new(accesskit::Role::Dialog);
        dialog.set_label(self.title.as_str());
        dialog.set_modal();
        dialog.set_bounds(super::bounds_of(self.bounds));
        let labels = self.labels();
        let label = |index: usize| labels.get(index).cloned().unwrap_or_default();
        let mut nodes = self.list.accessibility_nodes(&label);
        nodes.push((self.cancel.id, self.cancel.accessibility_node()));
        nodes.push((self.open.id, self.open.accessibility_node()));
        dialog.set_children(vec![
            accesskit::NodeId(self.list.id),
            accesskit::NodeId(self.cancel.id),
            accesskit::NodeId(self.open.id),
        ]);
        nodes.insert(0, (self.id, dialog));
        nodes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::geometry::Scale;

    fn key(key: Key) -> Event {
        Event::KeyPressed {
            key,
            modifiers: Modifiers::default(),
        }
    }

    fn items() -> Vec<OpenWithItem> {
        vec![
            OpenWithItem {
                label: "Viewer".into(),
                is_default: false,
            },
            OpenWithItem {
                label: "Editor".into(),
                is_default: true,
            },
        ]
    }

    #[test]
    fn starts_on_the_default_and_keyboard_chooses() {
        let mut chooser = OpenWithChooser::new(700, "Open With", "a.png", items());
        chooser.layout((480, 360));
        assert_eq!(chooser.list.cursor(), Some(1));
        assert_eq!(
            chooser.handle(&key(Key::Enter), Modifiers::default()),
            OpenWithOutcome::Chosen(1)
        );
        chooser.handle(&key(Key::Up), Modifiers::default());
        assert_eq!(
            chooser.handle(&key(Key::Enter), Modifiers::default()),
            OpenWithOutcome::Chosen(0)
        );
        assert_eq!(
            chooser.handle(&key(Key::Escape), Modifiers::default()),
            OpenWithOutcome::Cancelled
        );
    }

    #[test]
    fn default_is_marked_for_assistive_technology_and_paint() {
        let mut chooser = OpenWithChooser::new(700, "Open With", "a.png", items());
        chooser.layout((480, 360));
        let rows: Vec<String> = chooser
            .accessibility_nodes()
            .iter()
            .filter_map(|(_, node)| node.label().map(str::to_owned))
            .collect();
        assert!(rows.iter().any(|row| row == "Editor (default)"), "{rows:?}");
        for factor in [1, 2] {
            let surface = RasterSurface::new(480, 360, Scale::new(factor).unwrap()).unwrap();
            chooser.paint(&surface).unwrap();
        }
    }

    #[test]
    fn empty_list_explains_and_refuses_to_choose() {
        let mut chooser = OpenWithChooser::new(700, "Open With", "a.bin", Vec::new());
        chooser.layout((480, 360));
        assert!(chooser.status().unwrap().contains("a.bin"));
        assert_eq!(
            chooser.handle(
                &Event::Accessibility(AccessibilityRequest::Click(700 + ID_OPEN)),
                Modifiers::default()
            ),
            OpenWithOutcome::Changed
        );
    }
}
