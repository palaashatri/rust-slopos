//! The System Information window: each fact on its own labelled row, and a
//! button that copies all of them to the clipboard.
//!
//! The facts are read once when the window opens (see [`crate::facts`]). A
//! value that could not be read is drawn as "Unavailable" with the reason.

use slopos_ui::accessibility::{text_label, window_tree};
use slopos_ui::controls::button::{Button, ButtonOutcome};
use slopos_ui::controls::dialog::wrap;
use slopos_ui::event::{AccessibilityRequest, Event, Key};
use slopos_ui::foundation::geometry::Rect;
use slopos_ui::platform::{App, Control};
use slopos_ui::primitives::button_plate::ButtonPlateVariant;
use slopos_ui::render::{RasterSurface, RenderError};
use slopos_ui::text::Font;
use slopos_ui::theme::palette;

use crate::facts::Fact;

pub const WIDTH: u32 = 560;
pub const HEIGHT: u32 = 480;

const TITLE: &str = "System Information";
const MARGIN: i32 = 12;
const PANEL_PADDING: i32 = 8;
const LABEL_WIDTH: i32 = 136;
const LINE_HEIGHT: i32 = 16;
const ROW_GAP: i32 = 6;
const BUTTON_WIDTH: u32 = 120;
const BUTTON_HEIGHT: u32 = 26;

const COPY_NODE: u64 = 10;
const STATUS_NODE: u64 = 11;
/// Fact rows are label nodes numbered from here.
const FACT_NODE: u64 = 100;
const COPY_LABEL: &str = "Copy details";
const COPIED_STATUS: &str = "Copied to the clipboard.";

pub struct SystemInformationApp {
    facts: Vec<Fact>,
    copy: Button,
    copied: bool,
    body: Font,
    label: Font,
}

impl SystemInformationApp {
    pub fn new(facts: Vec<Fact>) -> Self {
        let body = Font::system(12.0);
        let label = Font::system(12.0).bold();
        let mut app = Self {
            facts,
            copy: Button::new(COPY_NODE, COPY_LABEL, ButtonPlateVariant::Regular),
            copied: false,
            body,
            label,
        };
        let copy_top = app.panel().y + app.panel().height as i32 + MARGIN;
        app.copy = app.copy.with_bounds(Rect {
            x: MARGIN,
            y: copy_top,
            width: BUTTON_WIDTH,
            height: BUTTON_HEIGHT,
        });
        app
    }

    #[cfg(test)]
    pub fn facts(&self) -> &[Fact] {
        &self.facts
    }

    /// Every fact as "Name: value" lines, for the clipboard.
    pub fn details(&self) -> String {
        self.facts
            .iter()
            .map(|fact| format!("{}: {}", fact.name, fact.value.display()))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Height of the window's content in logical pixels. A window taller
    /// than this shows every row; a shorter one clips the last rows.
    #[cfg(test)]
    pub fn content_height(&self) -> i32 {
        self.copy.bounds.y + BUTTON_HEIGHT as i32 + MARGIN
    }

    fn value_width() -> u32 {
        (WIDTH as i32 - 2 * MARGIN - 2 * PANEL_PADDING - LABEL_WIDTH).max(1) as u32
    }

    /// The wrapped lines of every fact's value.
    fn wrapped_values(&self) -> Vec<Vec<String>> {
        self.facts
            .iter()
            .map(|fact| wrap(&fact.value.display(), &self.body, Self::value_width()))
            .collect()
    }

    /// The box that holds the rows.
    fn panel(&self) -> Rect {
        let rows: i32 = self
            .wrapped_values()
            .iter()
            .map(|lines| row_height(lines.len()))
            .sum();
        Rect {
            x: MARGIN,
            y: MARGIN,
            width: WIDTH - 2 * MARGIN as u32,
            height: (rows + 2 * PANEL_PADDING) as u32,
        }
    }

    fn copy_details(&mut self, control: &mut Control) {
        control.set_clipboard(&self.details());
        self.copied = true;
        control.request_redraw();
    }

    fn paint_rows(&self, surface: &RasterSurface, panel: Rect) -> Result<(), RenderError> {
        let colors = palette();
        let mut y = panel.y + PANEL_PADDING;
        let value_x = MARGIN + PANEL_PADDING + LABEL_WIDTH;
        for (fact, lines) in self.facts.iter().zip(self.wrapped_values()) {
            surface.draw_text(
                fact.name,
                MARGIN + PANEL_PADDING,
                y,
                &self.label,
                colors.ink_secondary,
            )?;
            for (index, line) in lines.iter().enumerate() {
                let line_y = y + index as i32 * LINE_HEIGHT;
                surface.draw_text(line, value_x, line_y, &self.body, colors.ink)?;
            }
            y += row_height(lines.len());
        }
        Ok(())
    }
}

/// Height of a row whose value takes `lines` lines (at least one).
fn row_height(lines: usize) -> i32 {
    lines.max(1) as i32 * LINE_HEIGHT + ROW_GAP
}

impl App for SystemInformationApp {
    fn event(&mut self, event: &Event, control: &mut Control) {
        match event {
            Event::CloseRequested => control.quit(),
            Event::KeyPressed { key: Key::Tab, .. } => {
                self.copy.focused = !self.copy.focused;
                control.request_redraw();
            }
            Event::Accessibility(AccessibilityRequest::Click(id)) if *id == COPY_NODE => {
                self.copy_details(control);
            }
            Event::Accessibility(AccessibilityRequest::Focus(id)) if *id == COPY_NODE => {
                if !self.copy.focused {
                    self.copy.focused = true;
                    control.request_redraw();
                }
            }
            _ => match self.copy.handle(event) {
                ButtonOutcome::Activated => self.copy_details(control),
                ButtonOutcome::Changed => control.request_redraw(),
                ButtonOutcome::Ignored => {}
            },
        }
    }

    fn paint(&mut self, surface: &RasterSurface) {
        let colors = palette();
        let _ = surface.clear(colors.surface);
        let panel = self.panel();
        let _ = surface.fill_rect(panel, colors.content);
        let _ = surface.stroke_rect(panel, colors.border);
        let _ = surface.with_clip(panel, || self.paint_rows(surface, panel));
        let _ = self.copy.paint(surface, &self.body);
        if self.copied {
            let _ = surface.draw_text(
                COPIED_STATUS,
                self.copy.bounds.x + self.copy.bounds.width as i32 + MARGIN,
                self.copy.bounds.y + (BUTTON_HEIGHT as i32 - LINE_HEIGHT) / 2,
                &self.body,
                colors.ink,
            );
        }
    }

    fn accessibility(&self) -> Option<accesskit::TreeUpdate> {
        let mut nodes: Vec<(u64, accesskit::Node)> = self
            .facts
            .iter()
            .enumerate()
            .map(|(index, fact)| {
                (
                    FACT_NODE + index as u64,
                    text_label(format!("{}: {}", fact.name, fact.value.display())),
                )
            })
            .collect();
        nodes.push((COPY_NODE, self.copy.accessibility_node()));
        if self.copied {
            nodes.push((STATUS_NODE, text_label(COPIED_STATUS)));
        }
        let focus = self.copy.focused.then_some(COPY_NODE);
        Some(window_tree(TITLE, nodes, focus))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::{fake, gather};
    use slopos_ui::foundation::geometry::Scale;

    fn healthy() -> SystemInformationApp {
        SystemInformationApp::new(gather(&fake::healthy()))
    }

    fn key(app: &mut SystemInformationApp, key: Key) -> Control {
        let mut control = Control::default();
        app.event(
            &Event::KeyPressed {
                key,
                modifiers: Default::default(),
            },
            &mut control,
        );
        control
    }

    fn labels(app: &SystemInformationApp) -> Vec<String> {
        let update = app.accessibility().expect("a tree");
        update
            .nodes
            .iter()
            .filter_map(|(_, node)| node.value().map(str::to_owned))
            .collect()
    }

    #[test]
    fn copy_details_puts_every_fact_on_the_clipboard() {
        let mut app = healthy();
        let mut control = Control::default();
        app.event(
            &Event::Accessibility(AccessibilityRequest::Click(COPY_NODE)),
            &mut control,
        );
        let clipboard = control.pending_clipboard().expect("clipboard text");
        assert!(clipboard.contains("Kernel: 6.1.0-18-amd64"));
        assert!(clipboard.contains("Memory: 16.0 MiB"));
        assert_eq!(clipboard.lines().count(), app.facts().len());
        assert!(labels(&app).contains(&COPIED_STATUS.to_owned()));
    }

    #[test]
    fn keyboard_moves_focus_to_copy_and_enter_copies() {
        let mut app = healthy();
        let control = key(&mut app, Key::Tab);
        assert!(control.pending_clipboard().is_none());
        assert!(app.copy.focused);
        let update = app.accessibility().unwrap();
        assert_eq!(update.focus, accesskit::NodeId(COPY_NODE));

        let control = key(&mut app, Key::Enter);
        assert!(control
            .pending_clipboard()
            .unwrap()
            .starts_with("SLOPOS version:"));
    }

    #[test]
    fn enter_does_nothing_until_copy_has_focus() {
        let mut app = healthy();
        let control = key(&mut app, Key::Enter);
        assert!(control.pending_clipboard().is_none());
    }

    #[test]
    fn unreadable_values_are_named_as_unavailable_in_the_tree() {
        let app = SystemInformationApp::new(gather(&fake::Fake::default()));
        let text = labels(&app);
        assert!(text.iter().any(|line| line
            == "Kernel: Unavailable (cannot read /proc/sys/kernel/osrelease: not found)"));
        assert!(text
            .iter()
            .any(|line| line.starts_with("Storage (/): Unavailable")));
    }

    #[test]
    fn a_healthy_system_fits_the_window() {
        let app = healthy();
        assert!(
            app.content_height() <= HEIGHT as i32,
            "{}",
            app.content_height()
        );
    }

    #[test]
    fn the_copy_button_sits_below_the_panel() {
        let app = healthy();
        assert!(app.copy.bounds.y >= app.panel().y + app.panel().height as i32);
    }

    #[test]
    fn paints_at_1x_and_2x_with_real_content() {
        for factor in [1, 2] {
            let scale = Scale::new(factor).unwrap();
            let surface = RasterSurface::new(WIDTH, HEIGHT, scale).unwrap();
            let mut app = healthy();
            app.paint(&surface);
            assert_eq!(surface.device_width(), WIDTH * factor);
            assert_eq!(surface.device_height(), HEIGHT * factor);

            let other = RasterSurface::new(WIDTH, HEIGHT, scale).unwrap();
            let mut empty = SystemInformationApp::new(Vec::new());
            empty.paint(&other);
            assert_ne!(
                surface.data().to_vec(),
                other.data().to_vec(),
                "text must change the picture at {factor}x"
            );
        }
    }

    #[test]
    fn the_focus_ring_is_drawn_only_when_copy_has_focus() {
        let scale = Scale::new(1).unwrap();
        let mut app = healthy();
        let plain = RasterSurface::new(WIDTH, HEIGHT, scale).unwrap();
        app.paint(&plain);
        key(&mut app, Key::Tab);
        let focused = RasterSurface::new(WIDTH, HEIGHT, scale).unwrap();
        app.paint(&focused);
        assert_ne!(plain.data().to_vec(), focused.data().to_vec());
    }
}
