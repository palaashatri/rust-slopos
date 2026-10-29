//! Window-independent virtual list control for Files, Software, Media and
//! future SLOPOS applications. No per-row widget allocations, GTK or polling.
use crate::event::{Event, Key, PointerButton};
use crate::foundation::{color::Color, geometry::Rect};
use crate::render::{RasterSurface, RenderError};
use crate::text::{measure, Font};

const ROW_HEIGHT: u32 = 25;
const MAX_ITEMS: usize = 8192;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
    pub id: u64,
    pub title: String,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListOutcome {
    Ignored,
    Changed,
    Activated(usize),
}

#[derive(Debug, Clone)]
pub struct ListView {
    pub bounds: Rect,
    pub focused: bool,
    pub id: u64,
    items: Vec<ListItem>,
    selected: Option<usize>,
    first_visible: usize,
}

impl ListView {
    pub fn new(id: u64, bounds: Rect) -> Self {
        Self {
            bounds,
            focused: false,
            id,
            items: Vec::new(),
            selected: None,
            first_visible: 0,
        }
    }

    pub fn items(&self) -> &[ListItem] {
        &self.items
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn visible_capacity(&self) -> usize {
        (self.bounds.height / ROW_HEIGHT).max(1) as usize
    }

    pub fn first_visible(&self) -> usize {
        self.first_visible
    }

    pub fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
        self.ensure_visible();
    }

    /// Bounded list memory; callers must paginate large providers rather than
    /// silently drop rows and pretend an incomplete directory is exhaustive.
    pub fn set_items(&mut self, items: Vec<ListItem>) -> Result<(), &'static str> {
        if items.len() > MAX_ITEMS {
            return Err("list is over 8192 rows; implement provider pagination");
        }
        self.items = items;
        self.first_visible = 0;
        self.selected = None;
        Ok(())
    }

    pub fn select(&mut self, index: Option<usize>) -> bool {
        let selection = index.filter(|i| *i < self.items.len());
        if self.selected == selection {
            return false;
        }
        self.selected = selection;
        self.ensure_visible();
        true
    }

    fn ensure_visible(&mut self) {
        let cap = self.visible_capacity();
        let max_start = self.items.len().saturating_sub(cap);
        if let Some(selected) = self.selected {
            if selected < self.first_visible {
                self.first_visible = selected;
            } else if selected >= self.first_visible.saturating_add(cap) {
                self.first_visible = selected.saturating_add(1).saturating_sub(cap);
            }
        }
        self.first_visible = self.first_visible.min(max_start);
    }

    pub fn handle(&mut self, event: &Event) -> ListOutcome {
        match event {
            Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
            } if self.bounds.contains(*x, *y) => {
                self.focused = true;
                let offset = (*y as i64 - self.bounds.y as i64) as usize / ROW_HEIGHT as usize;
                let index = self.first_visible.saturating_add(offset);
                if index < self.items.len() {
                    self.select(Some(index));
                }
                ListOutcome::Changed
            }
            Event::Scroll { dy, .. } if self.bounds.height > 0 => {
                let old = self.first_visible;
                let cap = self.visible_capacity();
                let max = self.items.len().saturating_sub(cap);
                let delta = (*dy as i64).clamp(-100, 100);
                self.first_visible = if delta >= 0 {
                    self.first_visible.saturating_add(delta as usize).min(max)
                } else {
                    self.first_visible.saturating_sub((-delta) as usize)
                };
                if self.first_visible != old {
                    ListOutcome::Changed
                } else {
                    ListOutcome::Ignored
                }
            }
            Event::KeyPressed { key, modifiers }
                if self.focused && !modifiers.alt && !modifiers.control && !modifiers.logo =>
            {
                let count = self.items.len();
                if count == 0 {
                    return ListOutcome::Ignored;
                }
                let old = self.selected;
                let current = old.unwrap_or(0);
                let new_index = match key {
                    Key::Up => Some(current.saturating_sub(1)),
                    Key::Down => Some(if old.is_none() {
                        0
                    } else {
                        (current + 1).min(count - 1)
                    }),
                    Key::Home => Some(0),
                    Key::End => Some(count - 1),
                    Key::PageUp => Some(current.saturating_sub(self.visible_capacity())),
                    Key::PageDown => Some(
                        current
                            .saturating_add(self.visible_capacity())
                            .min(count - 1),
                    ),
                    Key::Enter | Key::Space => {
                        return old.map_or(ListOutcome::Ignored, ListOutcome::Activated)
                    }
                    _ => None,
                };
                if let Some(index) = new_index {
                    if self.select(Some(index)) {
                        ListOutcome::Changed
                    } else {
                        ListOutcome::Ignored
                    }
                } else {
                    ListOutcome::Ignored
                }
            }
            _ => ListOutcome::Ignored,
        }
    }

    pub fn paint(&self, surface: &RasterSurface, font: &Font) -> Result<(), RenderError> {
        let bounds = self.bounds;
        surface.fill_rect(bounds, Color::WHITE)?;
        surface.stroke_rect(bounds, Color::BLACK)?;
        let end = self
            .items
            .len()
            .min(self.first_visible.saturating_add(self.visible_capacity()));
        for index in self.first_visible..end {
            let y =
                i64::from(bounds.y) + (index - self.first_visible) as i64 * i64::from(ROW_HEIGHT);
            if y > i64::from(i32::MAX) {
                break;
            }
            let y = y as i32;
            let rect = Rect {
                x: bounds.x.saturating_add(1),
                y,
                width: bounds.width.saturating_sub(2),
                height: ROW_HEIGHT,
            };
            if self.selected == Some(index) {
                surface.fill_rect(
                    rect,
                    if self.focused {
                        Color {
                            red: 0x32,
                            green: 0x52,
                            blue: 0x88,
                            alpha: 255,
                        }
                    } else {
                        Color {
                            red: 0xc3,
                            green: 0xc3,
                            blue: 0xc3,
                            alpha: 255,
                        }
                    },
                )?;
            }
            if bounds.width >= 24 {
                let foreground = if self.selected == Some(index) && self.focused {
                    Color::WHITE
                } else {
                    Color::BLACK
                };
                let text = fit_text(
                    &self.items[index].title,
                    font,
                    bounds.width.saturating_sub(24) as f32,
                );
                surface.draw_text(
                    &text,
                    bounds.x.saturating_add(9),
                    y.saturating_add(4),
                    font,
                    foreground,
                )?;
            }
        }
        Ok(())
    }

    pub fn accessibility_nodes(
        &self,
    ) -> (accesskit::Node, Vec<(accesskit::NodeId, accesskit::Node)>) {
        use accesskit::{Action, Node, NodeId, Role};
        let mut root = Node::new(Role::List);
        root.set_bounds(accesskit::Rect {
            x0: f64::from(self.bounds.x),
            y0: f64::from(self.bounds.y),
            x1: f64::from(self.bounds.x) + f64::from(self.bounds.width),
            y1: f64::from(self.bounds.y) + f64::from(self.bounds.height),
        });
        let mut nodes = Vec::new();
        let end = self
            .items
            .len()
            .min(self.first_visible.saturating_add(self.visible_capacity()));
        for index in self.first_visible..end {
            let item = &self.items[index];
            let mut node = Node::new(Role::ListItem);
            node.set_label(&item.title);
            node.set_description(&item.detail);
            node.add_action(Action::Click);
            node.add_action(Action::Focus);
            if self.selected == Some(index) {
                node.set_selected();
            }
            let id = NodeId(item.id);
            children.push(id);
            nodes.push((id, node));
        }
        root.set_children(children);
        (root, nodes)
    }

    /// Return the visible row for an accessibility child id, not a stale index.
    pub fn index_for_id(&self, id: u64) -> Option<usize> {
        let end = self
            .items
            .len()
            .min(self.first_visible.saturating_add(self.visible_capacity()));
        (self.first_visible..end).find(|&index| self.items[index].id == id)
    }
}

fn fit_text(text: &str, font: &Font, max_width: f32) -> String {
    if measure(text, font).width <= max_width {
        return text.to_owned();
    }
    let mut out = String::new();
    for c in text.chars().take(160) {
        let mut preview = out.clone();
        preview.push(c);
        preview.push('…');
        if measure(&preview, font).width > max_width {
            break;
        }
        out.push(c);
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Modifiers;
    use crate::foundation::geometry::Scale;

    fn bounds() -> Rect {
        Rect {
            x: 10,
            y: 12,
            width: 260,
            height: 100,
        }
    }

    fn data(count: usize) -> Vec<ListItem> {
        (0..count)
            .map(|n| ListItem {
                id: n as u64 + 1000,
                title: format!("Entry {n}"),
                detail: "File".into(),
            })
            .collect()
    }

    #[test]
    fn keyboard_selection_paging_and_activation() {
        let mut list = ListView::new(9, bounds());
        list.set_items(data(50)).unwrap();
        list.focused = true;
        let key = |key| Event::KeyPressed {
            key,
            modifiers: Modifiers::default(),
        };
        assert_eq!(list.handle(&key(Key::Down)), ListOutcome::Changed);
        assert_eq!(list.selected(), Some(0));
        assert_eq!(list.handle(&key(Key::PageDown)), ListOutcome::Changed);
        assert_eq!(list.selected(), Some(4));
        assert!(list.first_visible() > 0);
        assert_eq!(list.handle(&key(Key::Enter)), ListOutcome::Activated(4));
        assert_eq!(list.handle(&key(Key::End)), ListOutcome::Changed);
        assert_eq!(list.selected(), Some(49));
        assert_eq!(list.handle(&key(Key::Home)), ListOutcome::Changed);
        assert_eq!(list.selected(), Some(0));
    }

    #[test]
    fn pointer_scroll_and_bounds_are_clamped() {
        let mut list = ListView::new(9, bounds());
        list.set_items(data(30)).unwrap();
        assert_eq!(
            list.handle(&Event::Scroll {
                dx: 0,
                dy: i32::MAX
            }),
            ListOutcome::Changed
        );
        assert_eq!(list.first_visible(), 26);
        assert_eq!(
            list.handle(&Event::Scroll {
                dx: 0,
                dy: i32::MIN
            }),
            ListOutcome::Changed
        );
        assert_eq!(list.first_visible(), 0);
        assert_eq!(
            list.handle(&Event::PointerPressed {
                x: 11,
                y: 40,
                button: PointerButton::Primary
            }),
            ListOutcome::Changed
        );
        assert_eq!(list.selected(), Some(1));
        assert_eq!(
            list.handle(&Event::PointerPressed {
                x: -1,
                y: 40,
                button: PointerButton::Primary
            }),
            ListOutcome::Ignored
        );
    }

    #[test]
    fn row_cap_is_rejected_not_truncated() {
        let mut list = ListView::new(9, bounds());
        assert!(list.set_items(data(8193)).is_err());
        assert!(list.items().is_empty());
    }

    #[test]
    fn painting_and_accessibility_use_only_visible_rows() {
        let mut list = ListView::new(9, bounds());
        list.set_items(data(128)).unwrap();
        list.select(Some(80));
        let surface = RasterSurface::new(300, 150, Scale::new(1).unwrap()).unwrap();
        list.paint(&surface, &Font::system(13.0)).unwrap();
        let (_, nodes) = list.accessibility_nodes();
        assert_eq!(nodes.len(), 4);
        assert!(list.index_for_id(1080).is_some());
        assert!(list.index_for_id(1000).is_none());
    }
}
