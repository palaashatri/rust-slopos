use crate::{
    theme::ThemeContext, AccessibilityNode, Event, EventResult, LayoutConstraint, Rect, Size,
    Widget, WidgetState,
};

#[derive(Debug, Clone)]
/// Represents a single selectable icon item within an [`IconView`].
pub struct IconItem {
    /// The display label/name of the item.
    pub label: String,
    /// Optional resource path or identifier of the icon graphic.
    pub icon: Option<String>,
    /// Whether this specific item is currently selected by the user.
    pub selected: bool,
    /// The physical layout bounds of this item, computed during layout.
    pub rect: Rect,
}

/// Selects how an [`IconView`] positions its items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IconViewLayoutMode {
    /// Arrange items in the normal Finder-style grid.
    #[default]
    Grid,
    /// Arrange items in the SLOPOS desktop column layout.
    Desktop,
}

/// Logical width of a desktop icon hit/nameplate cell.
///
/// Desktop labels are painted inside this same cell, so the visible
/// nameplate and its pointer hit target cannot diverge at the right edge.
pub const DESKTOP_ITEM_WIDTH: f32 = 104.0;

/// A grid or desktop-style icon grid view widget.
/// Supports both file list grids (like Finder) and the standard desktop layout.
pub struct IconView {
    state: WidgetState,
    /// The list of icons rendered inside this view.
    pub items: Vec<IconItem>,
    /// The target icon square dimensions (width/height).
    pub icon_size: f32,
    /// The spacing margin between items.
    pub spacing: f32,
    /// The explicit item layout strategy. Defaults to [`IconViewLayoutMode::Grid`].
    pub layout_mode: IconViewLayoutMode,
    /// Callback triggered upon double-clicking an icon item.
    pub on_double_click: Option<Box<dyn FnMut(usize) + Send>>,
    /// Index of the most recently double-clicked item, drained by
    /// [`IconView::take_activated`]. The polling twin of `on_double_click`,
    /// so apps can react to activation from their `update()`/event loop
    /// without restructuring around callbacks.
    activated: Option<usize>,
}

impl Default for IconView {
    fn default() -> Self {
        Self::new()
    }
}

impl IconView {
    /// Creates a new, empty `IconView` with default sizing configuration.
    pub fn new() -> Self {
        Self {
            state: WidgetState::new(),
            items: vec![],
            icon_size: 64.0,
            spacing: 8.0,
            layout_mode: IconViewLayoutMode::Grid,
            on_double_click: None,
            activated: None,
        }
    }

    /// Returns the index of the item double-clicked since the last call,
    /// exactly once per activation.
    pub fn take_activated(&mut self) -> Option<usize> {
        self.activated.take()
    }
}

impl Widget for IconView {
    fn widget_state(&self) -> &WidgetState {
        &self.state
    }
    fn widget_state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }

    /// Lays out icon items.
    ///
    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        let width = constraint.max_width;
        let height = constraint.max_height;
        let size = constraint.clamp(Size::new(width, height));
        let r = Rect::new(self.rect().x, self.rect().y, size.width, size.height);
        self.set_rect(r);

        let icon_size = self.icon_size;
        if self.layout_mode == IconViewLayoutMode::Desktop {
            let item_width = DESKTOP_ITEM_WIDTH.max(icon_size);
            let right_x = r.x + size.width - item_width - 28.0;
            let mut app_y = r.y + 28.0;
            let trash_y = r.y + size.height - icon_size - 34.0;
            for item in &mut self.items {
                let y = match item.label.as_str() {
                    "Trash" => trash_y,
                    _ => {
                        let y = app_y;
                        app_y += 72.0;
                        y
                    }
                };
                item.rect = Rect::new(right_x, y, item_width, 52.0);
            }
        } else {
            let cell_w = 84.0;
            let cell_h = 68.0;
            let cols = (size.width / cell_w).max(1.0) as usize;
            for (i, item) in self.items.iter_mut().enumerate() {
                let col = i % cols;
                let row = i / cols;
                item.rect = Rect::new(
                    r.x + col as f32 * cell_w + (cell_w - icon_size) * 0.5,
                    r.y + row as f32 * cell_h + 10.0,
                    icon_size,
                    52.0,
                );
            }
        }
        size
    }

    fn draw(&self, _theme: &ThemeContext) {}

    fn handle_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::DoubleClick { point, .. } => {
                for (i, item) in self.items.iter().enumerate() {
                    if item.rect.contains(*point) {
                        self.activated = Some(i);
                        if let Some(cb) = &mut self.on_double_click {
                            (cb)(i);
                        }
                        return EventResult::Handled;
                    }
                }
                EventResult::Ignored
            }
            Event::MouseDown {
                button: crate::event::MouseButton::Left,
                point,
                ..
            } => {
                let mut hit = false;
                for item in &mut self.items {
                    if item.rect.contains(*point) {
                        item.selected = true;
                        hit = true;
                    } else {
                        item.selected = false;
                    }
                }
                if hit {
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
            _ => EventResult::Ignored,
        }
    }

    fn accessibility(&self) -> Option<AccessibilityNode> {
        None
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn desktop_item(label: &str) -> IconItem {
        IconItem {
            label: label.to_string(),
            icon: None,
            selected: false,
            rect: Rect::ZERO,
        }
    }

    #[test]
    fn default_layout_stays_grid_for_desktop_named_items() {
        let mut icons = IconView::new();
        icons.icon_size = 56.0;
        icons.items = vec![desktop_item("Hard Disk"), desktop_item("Trash")];
        icons.set_rect(Rect::new(0.0, 24.0, 1280.0, 776.0));

        icons.layout(LayoutConstraint::tight(Size::new(1280.0, 776.0)));

        let desktop_column_x = 1280.0 - icons.icon_size - 28.0;
        assert_ne!(icons.items[0].rect.x, desktop_column_x);
        assert_ne!(icons.items[1].rect.x, desktop_column_x);
        assert_eq!(icons.items[0].rect.x, 14.0);
        assert_eq!(icons.items[1].rect.x, 98.0);
    }

    #[test]
    fn desktop_layout_uses_right_aligned_column_with_bottom_trash() {
        let mut icons = IconView::new();
        icons.layout_mode = IconViewLayoutMode::Desktop;
        icons.icon_size = 56.0;
        icons.items = vec![
            desktop_item("Hard Disk"),
            desktop_item("Home"),
            desktop_item("Applications"),
            desktop_item("TextEdit"),
            desktop_item("Trash"),
        ];
        icons.set_rect(Rect::new(0.0, 24.0, 1280.0, 776.0));

        icons.layout(LayoutConstraint::tight(Size::new(1280.0, 776.0)));

        let expected_x = 1280.0 - DESKTOP_ITEM_WIDTH - 28.0;
        for item in &icons.items {
            assert_eq!(item.rect.x, expected_x);
            assert_eq!(item.rect.width, DESKTOP_ITEM_WIDTH);
            assert!(item.rect.x >= icons.rect().x);
            assert_eq!(
                item.rect.x + item.rect.width,
                icons.rect().x + icons.rect().width - 28.0
            );
            assert!(item.rect.y >= icons.rect().y);
            assert!(item.rect.y + item.rect.height <= icons.rect().y + icons.rect().height);
        }

        let trash = icons
            .items
            .iter()
            .find(|item| item.label == "Trash")
            .expect("trash icon exists");
        assert_eq!(
            trash.rect.y,
            icons.rect().y + icons.rect().height - icons.icon_size - 34.0
        );
        let textedit = icons
            .items
            .iter()
            .find(|item| item.label == "TextEdit")
            .expect("textedit icon exists");
        assert!(textedit.rect.y < trash.rect.y);
    }

    #[test]
    fn desktop_nameplate_edges_share_the_icon_hit_target() {
        let mut icons = IconView::new();
        icons.layout_mode = IconViewLayoutMode::Desktop;
        icons.icon_size = 56.0;
        icons.items = vec![desktop_item("Applications")];
        icons.set_rect(Rect::new(0.0, 24.0, 1280.0, 776.0));
        icons.layout(LayoutConstraint::tight(Size::new(1280.0, 776.0)));

        let rect = icons.items[0].rect;
        for point in [
            crate::Point::new(rect.x + 1.0, rect.y + 40.0),
            crate::Point::new(rect.x + rect.width - 1.0, rect.y + 40.0),
        ] {
            let result = icons.handle_event(&Event::MouseDown {
                button: crate::event::MouseButton::Left,
                point,
                modifiers: crate::event::Modifiers::NONE,
            });
            assert!(matches!(result, EventResult::Handled));
            assert!(icons.items[0].selected);
        }

        let result = icons.handle_event(&Event::DoubleClick {
            button: crate::event::MouseButton::Left,
            point: crate::Point::new(rect.x + rect.width - 1.0, rect.y + 40.0),
            modifiers: crate::event::Modifiers::NONE,
        });
        assert!(matches!(result, EventResult::Handled));
        assert_eq!(icons.take_activated(), Some(0));
    }
}
