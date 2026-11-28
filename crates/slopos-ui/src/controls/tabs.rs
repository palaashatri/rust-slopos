//! Tab strip: a row of equal-width tabs, one of which is selected.
//!
//! The strip is a plain state machine. A primary press on a tab selects it;
//! while the strip has keyboard focus, Left and Right move the selection one
//! tab (clamped at the ends), and Home and End jump to the first and last
//! tab. Tab and Shift+Tab are left to the window's [`super::focus::FocusChain`].
//! Assistive technology sees a `TabList` with one `Tab` node per label; a
//! Click on a tab node selects it.

use super::button::paint_focus_ring;
use crate::event::{AccessibilityRequest, Event, Key, PointerButton};
use crate::foundation::geometry::Rect;
use crate::render::{RasterSurface, RenderError};
use crate::text::{measure, Font};
use crate::theme::palette;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabOutcome {
    /// The event did not concern the strip.
    Ignored,
    /// The strip used the event but the selection did not change (a press
    /// on the selected tab, or a focus request); the window should give the
    /// strip keyboard focus.
    FocusRequested,
    /// Tab `index` is now selected; the window should repaint and give the
    /// strip keyboard focus.
    Selected(usize),
}

#[derive(Debug, Clone)]
pub struct TabStrip {
    /// Accessibility id of the strip; tab nodes are derived from it.
    pub id: u64,
    pub labels: Vec<String>,
    pub selected: usize,
    pub focused: bool,
    pub enabled: bool,
    /// The tab an assistive-technology focus request asked for. It only moves
    /// the screen reader's focus, not the selection, and it is cleared as soon
    /// as the selection changes or the strip itself is focused.
    focus_target: Option<usize>,
    bounds: Rect,
}

impl TabStrip {
    /// A strip with `labels` left to right; the first tab is selected.
    pub fn new(id: u64, labels: &[&str]) -> Self {
        Self {
            id,
            labels: labels.iter().map(|label| (*label).to_owned()).collect(),
            selected: 0,
            focused: false,
            enabled: true,
            focus_target: None,
            bounds: Rect::default(),
        }
    }

    pub fn bounds(&self) -> Rect {
        self.bounds
    }

    pub fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    /// Select `index`; returns false (and changes nothing) when there is no
    /// such tab or it is already selected.
    pub fn select(&mut self, index: usize) -> bool {
        if index >= self.labels.len() || index == self.selected {
            return false;
        }
        self.selected = index;
        self.focus_target = None;
        true
    }

    /// The rectangle of tab `index`. Tabs share the strip's width equally;
    /// the last one takes any remainder pixels.
    pub fn tab_rect(&self, index: usize) -> Option<Rect> {
        let count = self.labels.len() as u32;
        if index as u32 >= count || self.bounds.width == 0 {
            return None;
        }
        let each = self.bounds.width / count;
        let x = self.bounds.x + (each * index as u32) as i32;
        let width = if index as u32 + 1 == count {
            self.bounds.width - each * (count - 1)
        } else {
            each
        };
        Some(Rect {
            x,
            y: self.bounds.y,
            width,
            height: self.bounds.height,
        })
    }

    /// The tab under a point, if any.
    pub fn tab_at(&self, x: i32, y: i32) -> Option<usize> {
        (0..self.labels.len())
            .find(|&index| self.tab_rect(index).is_some_and(|rect| rect.contains(x, y)))
    }

    /// The accessibility id of tab `index`.
    pub fn tab_node_id(&self, index: usize) -> u64 {
        (self.id << 32) | (index as u64 + 1)
    }

    /// The node the screen reader should focus while the strip has focus.
    pub fn focus_node(&self) -> u64 {
        if !self.focused {
            return self.id;
        }
        match self.focus_target.unwrap_or(self.selected) {
            index if index < self.labels.len() => self.tab_node_id(index),
            _ => self.id,
        }
    }

    fn tab_for_node(&self, node: u64) -> Option<usize> {
        if node >> 32 != self.id || node & 0xffff_ffff == 0 {
            return None;
        }
        let index = (node & 0xffff_ffff) as usize - 1;
        (index < self.labels.len()).then_some(index)
    }

    /// Handle an event and report what changed.
    pub fn handle(&mut self, event: &Event) -> TabOutcome {
        if !self.enabled {
            return TabOutcome::Ignored;
        }
        match event {
            Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
            } => match self.tab_at(*x, *y) {
                Some(index) if self.select(index) => TabOutcome::Selected(index),
                Some(_) => {
                    self.focus_target = None;
                    TabOutcome::FocusRequested
                }
                None => TabOutcome::Ignored,
            },
            Event::KeyPressed { key, .. } if self.focused => {
                let last = self.labels.len().saturating_sub(1);
                let target = match key {
                    Key::Left => self.selected.saturating_sub(1),
                    Key::Right => (self.selected + 1).min(last),
                    Key::Home => 0,
                    Key::End => last,
                    _ => return TabOutcome::Ignored,
                };
                if self.select(target) {
                    TabOutcome::Selected(target)
                } else {
                    TabOutcome::Ignored
                }
            }
            Event::Accessibility(AccessibilityRequest::Click(node)) => {
                match self.tab_for_node(*node) {
                    Some(index) if self.select(index) => TabOutcome::Selected(index),
                    Some(_) => {
                        self.focus_target = None;
                        TabOutcome::FocusRequested
                    }
                    None => TabOutcome::Ignored,
                }
            }
            Event::Accessibility(AccessibilityRequest::Focus(node)) if *node == self.id => {
                self.focus_target = None;
                TabOutcome::FocusRequested
            }
            Event::Accessibility(AccessibilityRequest::Focus(node)) => {
                match self.tab_for_node(*node) {
                    Some(index) => {
                        self.focus_target = Some(index);
                        TabOutcome::FocusRequested
                    }
                    None => TabOutcome::Ignored,
                }
            }
            _ => TabOutcome::Ignored,
        }
    }

    pub fn paint(&self, surface: &RasterSurface, font: &Font) -> Result<(), RenderError> {
        let colors = palette();
        for index in 0..self.labels.len() {
            let Some(rect) = self.tab_rect(index) else {
                continue;
            };
            let selected = index == self.selected;
            let fill = if selected {
                colors.content
            } else {
                colors.surface
            };
            surface.fill_rect(rect, fill)?;
            surface.stroke_rect(rect, colors.bevel_shadow)?;
            let ink = if !self.enabled {
                colors.ink_disabled
            } else {
                colors.ink
            };
            let label = &self.labels[index];
            let extent = measure(label, font);
            let x = rect.x + (rect.width as i32 - extent.width as i32) / 2;
            let y = rect.y + (rect.height as i32 - extent.height as i32) / 2;
            surface.draw_text(label, x, y, font, ink)?;
            if selected && self.focused && self.enabled {
                paint_focus_ring(
                    surface,
                    Rect {
                        x: rect.x + 3,
                        y: rect.y + 3,
                        width: rect.width.saturating_sub(6),
                        height: rect.height.saturating_sub(6),
                    },
                )?;
            }
        }
        Ok(())
    }

    /// A `TabList` node and one `Tab` node per label.
    pub fn accessibility_nodes(&self) -> Vec<(u64, accesskit::Node)> {
        let mut list = accesskit::Node::new(accesskit::Role::TabList);
        list.set_bounds(super::bounds_of(self.bounds));
        list.add_action(accesskit::Action::Focus);
        let mut nodes = Vec::with_capacity(self.labels.len() + 1);
        let mut children = Vec::with_capacity(self.labels.len());
        for (index, label) in self.labels.iter().enumerate() {
            let Some(rect) = self.tab_rect(index) else {
                continue;
            };
            let id = self.tab_node_id(index);
            children.push(accesskit::NodeId(id));
            let mut tab = accesskit::Node::new(accesskit::Role::Tab);
            tab.set_label(label.as_str());
            tab.set_bounds(super::bounds_of(rect));
            tab.set_selected(index == self.selected);
            tab.add_action(accesskit::Action::Click);
            tab.add_action(accesskit::Action::Focus);
            nodes.push((id, tab));
        }
        list.set_children(children);
        nodes.insert(0, (self.id, list));
        nodes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Modifiers;
    use crate::foundation::{color::Color, geometry::Scale};

    fn strip() -> TabStrip {
        let mut strip = TabStrip::new(7, &["Processes", "CPU", "Memory"]);
        strip.set_bounds(Rect {
            x: 10,
            y: 10,
            width: 301,
            height: 24,
        });
        strip
    }

    fn press(x: i32, y: i32) -> Event {
        Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
        }
    }

    fn key(key: Key) -> Event {
        Event::KeyPressed {
            key,
            modifiers: Modifiers::default(),
        }
    }

    #[test]
    fn tabs_share_the_width_and_the_last_takes_the_remainder() {
        let strip = strip();
        assert_eq!(strip.tab_rect(0).unwrap().width, 100);
        assert_eq!(strip.tab_rect(1).unwrap().x, 110);
        assert_eq!(strip.tab_rect(2).unwrap().width, 101);
        assert_eq!(strip.tab_rect(3), None);
    }

    #[test]
    fn a_press_on_a_tab_selects_it_and_a_press_on_the_selected_tab_only_focuses() {
        let mut strip = strip();
        assert_eq!(strip.handle(&press(150, 15)), TabOutcome::Selected(1));
        assert_eq!(strip.selected, 1);
        assert_eq!(strip.handle(&press(155, 15)), TabOutcome::FocusRequested);
        assert_eq!(strip.handle(&press(5, 15)), TabOutcome::Ignored);
        assert_eq!(strip.selected, 1);
    }

    #[test]
    fn arrows_and_home_end_move_the_selection_only_while_focused() {
        let mut strip = strip();
        assert_eq!(strip.handle(&key(Key::Right)), TabOutcome::Ignored);
        strip.focused = true;
        assert_eq!(strip.handle(&key(Key::Right)), TabOutcome::Selected(1));
        assert_eq!(strip.handle(&key(Key::End)), TabOutcome::Selected(2));
        assert_eq!(strip.handle(&key(Key::Right)), TabOutcome::Ignored);
        assert_eq!(strip.handle(&key(Key::Home)), TabOutcome::Selected(0));
        assert_eq!(strip.handle(&key(Key::Left)), TabOutcome::Ignored);
        assert_eq!(strip.handle(&key(Key::Enter)), TabOutcome::Ignored);
    }

    #[test]
    fn accessibility_click_selects_and_the_tree_marks_the_selected_tab() {
        let mut strip = strip();
        let node = strip.tab_node_id(2);
        assert_eq!(
            strip.handle(&Event::Accessibility(AccessibilityRequest::Click(node))),
            TabOutcome::Selected(2)
        );
        let nodes = strip.accessibility_nodes();
        assert_eq!(nodes.len(), 4);
        assert_eq!(nodes[0].0, 7);
        assert_eq!(nodes[0].1.role(), accesskit::Role::TabList);
        let (id, tab) = &nodes[3];
        assert_eq!(*id, node);
        assert_eq!(tab.role(), accesskit::Role::Tab);
        assert!(tab.is_selected() == Some(true));
        assert_eq!(tab.label(), Some("Memory"));
        // A node that is not one of the strip's tabs is not ours.
        assert_eq!(
            strip.handle(&Event::Accessibility(AccessibilityRequest::Click(99))),
            TabOutcome::Ignored
        );
    }

    #[test]
    fn focus_node_follows_the_selected_tab_while_focused() {
        let mut strip = strip();
        assert_eq!(strip.focus_node(), 7);
        strip.focused = true;
        strip.select(1);
        assert_eq!(strip.focus_node(), strip.tab_node_id(1));
    }

    #[test]
    fn disabled_strip_ignores_everything() {
        let mut strip = strip();
        strip.enabled = false;
        assert_eq!(strip.handle(&press(150, 15)), TabOutcome::Ignored);
        assert_eq!(strip.selected, 0);
    }

    #[test]
    fn a_focus_request_on_another_tab_moves_the_focus_node_but_not_the_selection() {
        let mut strip = strip();
        let node = strip.tab_node_id(2);
        assert_eq!(
            strip.handle(&Event::Accessibility(AccessibilityRequest::Focus(node))),
            TabOutcome::FocusRequested
        );
        assert_eq!(strip.selected, 0);
        strip.focused = true;
        assert_eq!(strip.focus_node(), node);
        // Arrow keys select the next tab and bring the focus along with it.
        assert_eq!(strip.handle(&key(Key::Right)), TabOutcome::Selected(1));
        assert_eq!(strip.focus_node(), strip.tab_node_id(1));
    }

    #[test]
    fn focusing_the_strip_itself_returns_the_focus_to_the_selected_tab() {
        let mut strip = strip();
        strip.focused = true;
        strip.handle(&Event::Accessibility(AccessibilityRequest::Focus(
            strip.tab_node_id(2),
        )));
        assert_eq!(strip.focus_node(), strip.tab_node_id(2));
        strip.handle(&Event::Accessibility(AccessibilityRequest::Focus(7)));
        assert_eq!(strip.focus_node(), strip.tab_node_id(0));
    }

    fn opaque(color: Color) -> u32 {
        0xff00_0000
            | (u32::from(color.red) << 16)
            | (u32::from(color.green) << 8)
            | u32::from(color.blue)
    }

    #[test]
    fn painting_fills_the_selected_tab_differently_from_the_others() {
        let surface = RasterSurface::new(340, 48, Scale::new(1).unwrap()).unwrap();
        surface.clear(Color::WHITE).unwrap();
        let strip = strip();
        strip.paint(&surface, &Font::system(12.0)).unwrap();
        // Probe inside each tab's top-left corner, clear of the border and the label.
        assert_eq!(surface.pixel(12, 12), Some(opaque(palette().content)));
        assert_eq!(surface.pixel(122, 12), Some(opaque(palette().surface)));
    }

    #[test]
    fn a_focused_selected_tab_paints_a_focus_ring_inside_its_border() {
        let surface = RasterSurface::new(340, 48, Scale::new(1).unwrap()).unwrap();
        surface.clear(Color::WHITE).unwrap();
        let mut strip = strip();
        // Sample the whole first tab: the ring's dots sit on alternating rows and
        // columns, so a single probe line can miss them entirely.
        let sample = |surface: &RasterSurface| -> Vec<Option<u32>> {
            (10..60)
                .flat_map(|x| (10..34).map(move |y| (x, y)))
                .map(|(x, y)| surface.pixel(x, y))
                .collect()
        };
        strip.focused = false;
        strip.paint(&surface, &Font::system(12.0)).unwrap();
        let unfocused = sample(&surface);

        surface.clear(Color::WHITE).unwrap();
        strip.focused = true;
        strip.paint(&surface, &Font::system(12.0)).unwrap();
        let focused = sample(&surface);
        assert_ne!(unfocused, focused);
    }

    #[test]
    fn painting_at_two_scales_puts_the_selected_fill_at_the_scaled_position() {
        let surface = RasterSurface::new(340, 48, Scale::new(2).unwrap()).unwrap();
        surface.clear(Color::WHITE).unwrap();
        let strip = strip();
        strip.paint(&surface, &Font::system(12.0)).unwrap();
        // The logical probe (12, 12) is device (24, 24) at scale 2.
        assert_eq!(surface.pixel(24, 24), Some(opaque(palette().content)));
    }

    #[test]
    fn disabled_strip_still_paints_its_selected_fill() {
        let surface = RasterSurface::new(340, 48, Scale::new(1).unwrap()).unwrap();
        surface.clear(Color::WHITE).unwrap();
        let mut strip = strip();
        strip.enabled = false;
        strip.paint(&surface, &Font::system(12.0)).unwrap();
        assert_eq!(surface.pixel(12, 12), Some(opaque(palette().content)));
    }

    #[test]
    fn focus_ring_has_a_reference_pixel_only_while_focused() {
        let surface = RasterSurface::new(340, 48, Scale::new(1).unwrap()).unwrap();
        let mut strip = strip();
        strip.focused = false;
        surface.clear(Color::WHITE).unwrap();
        strip.paint(&surface, &Font::system(12.0)).unwrap();
        assert_ne!(surface.pixel(13, 13), Some(opaque(Color::BLACK)));

        strip.focused = true;
        surface.clear(Color::WHITE).unwrap();
        strip.paint(&surface, &Font::system(12.0)).unwrap();
        // The ring's top-left dot sits three logical pixels inside the tab's corner.
        assert_eq!(surface.pixel(13, 13), Some(opaque(Color::BLACK)));
    }

    #[test]
    fn disabled_strip_uses_disabled_ink_and_never_shows_a_focus_ring() {
        let font = Font::system(12.0);
        let region = |surface: &RasterSurface| -> Vec<Option<u32>> {
            (10..310)
                .flat_map(|x| (10..34).map(move |y| (x, y)))
                .map(|(x, y)| surface.pixel(x, y))
                .collect()
        };
        let paint = |strip: &TabStrip| {
            let surface = RasterSurface::new(340, 48, Scale::new(1).unwrap()).unwrap();
            surface.clear(Color::WHITE).unwrap();
            strip.paint(&surface, &font).unwrap();
            surface
        };

        // Both strips are unfocused, so the only difference is the ink.
        let enabled = paint(&strip());
        let mut off = strip();
        off.enabled = false;
        let disabled = paint(&off);
        assert_ne!(
            region(&enabled),
            region(&disabled),
            "the label uses the disabled ink"
        );

        // A disabled strip paints no focus ring even when it has focus.
        let mut off_focused = strip();
        off_focused.enabled = false;
        off_focused.focused = true;
        let focused = paint(&off_focused);
        assert_eq!(focused.pixel(13, 13), Some(opaque(palette().content)));
    }

    #[test]
    fn disabled_label_matches_a_reference_drawn_in_the_disabled_ink() {
        let font = Font::system(12.0);
        let mut disabled = strip();
        disabled.enabled = false;
        let rect = disabled.tab_rect(0).unwrap();
        let label = "Processes";
        let extent = measure(label, &font);
        let x = rect.x + (rect.width as i32 - extent.width as i32) / 2;
        let y = rect.y + (rect.height as i32 - extent.height as i32) / 2;

        let painted = RasterSurface::new(340, 48, Scale::new(1).unwrap()).unwrap();
        painted.clear(Color::WHITE).unwrap();
        disabled.paint(&painted, &font).unwrap();

        // The reference draws the same tab with the disabled ink and nothing else.
        let reference = RasterSurface::new(340, 48, Scale::new(1).unwrap()).unwrap();
        reference.clear(Color::WHITE).unwrap();
        reference.fill_rect(rect, palette().content).unwrap();
        reference.stroke_rect(rect, palette().bevel_shadow).unwrap();
        reference
            .draw_text(label, x, y, &font, palette().ink_disabled)
            .unwrap();

        // Compare the tab's interior, which holds the label, and skip the border.
        for py in rect.y + 2..rect.y + rect.height as i32 - 2 {
            for px in rect.x + 2..rect.x + rect.width as i32 - 2 {
                assert_eq!(
                    painted.pixel(px as u32, py as u32),
                    reference.pixel(px as u32, py as u32),
                    "pixel ({px}, {py}) differs from the disabled-ink reference"
                );
            }
        }
    }

    #[test]
    fn clicking_the_selected_tab_clears_a_pending_focus_request() {
        let mut strip = strip();
        strip.focused = true;
        assert_eq!(
            strip.handle(&Event::Accessibility(AccessibilityRequest::Focus(
                strip.tab_node_id(2)
            ))),
            TabOutcome::FocusRequested
        );
        assert_eq!(strip.focus_node(), strip.tab_node_id(2));

        assert_eq!(
            strip.handle(&Event::Accessibility(AccessibilityRequest::Click(
                strip.tab_node_id(0)
            ))),
            TabOutcome::FocusRequested
        );
        assert_eq!(strip.selected, 0);
        assert_eq!(strip.focus_node(), strip.tab_node_id(0));
    }

    #[test]
    fn malformed_geometry_paints_nothing_and_does_not_fail() {
        let surface = RasterSurface::new(340, 48, Scale::new(1).unwrap()).unwrap();
        surface.clear(Color::WHITE).unwrap();
        let mut strip = strip();
        strip.set_bounds(Rect {
            x: 10,
            y: 10,
            width: 0,
            height: 24,
        });
        strip.paint(&surface, &Font::system(12.0)).unwrap();
        assert_eq!(surface.pixel(12, 12), Some(0xffff_ffff));

        let empty = TabStrip::new(9, &[]);
        empty.paint(&surface, &Font::system(12.0)).unwrap();
        assert_eq!(empty.focus_node(), 9);
    }
}
