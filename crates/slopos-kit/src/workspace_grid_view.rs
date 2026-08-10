use crate::{
    event::{KeyCode, MouseButton},
    theme::ThemeContext,
    AccessibilityNode, AccessibilityRole, Event, EventResult, ImageView, LayoutConstraint, Point,
    Rect, Size, Visibility, Widget, WidgetState,
};

pub struct WorkspaceGridView {
    state: WidgetState,
    pub active_index: usize,
    /// Cell that receives keyboard navigation and Enter/Space activation.
    /// This is intentionally independent from `active_index`: the latter is
    /// the compositor-authoritative Space, while this field is local pending
    /// keyboard focus until a selection is committed.
    pub focused_index: usize,
    /// Stable compositor Space IDs aligned with [`Self::items`]. The grid
    /// does not mutate these IDs; the shell replaces them from its
    /// authoritative Spaces snapshot.
    pub space_ids: Vec<u64>,
    pub items: Vec<String>,
    /// Number of ordinary windows currently assigned to each item.
    ///
    /// The shell fills this from the compositor's Spaces snapshot.  Keeping
    /// counts parallel to `items` lets the renderer show live membership
    /// without making the toolkit invent window records or geometry.
    pub window_counts: Vec<usize>,
    /// Compositor-produced Space captures aligned with [`Self::items`].
    /// Missing captures are represented by `None`; the grid never fabricates
    /// imagery for a Space whose compositor renderer is unavailable.
    thumbnails: Vec<Option<ImageView>>,
    /// Cell clicked most recently, drained by [`WorkspaceGridView::take_activated`].
    activated: Option<usize>,
    /// Pointer press retained until release so a click can be distinguished
    /// from a real drag without optimistically switching Spaces.
    pointer_press: Option<PointerPress>,
    /// A target Space currently under a pointer drag, if any.
    drag_target: Option<usize>,
    /// Target Space committed when a drag is released over another cell.
    dropped: Option<usize>,
}

#[derive(Clone, Copy, Debug)]
struct PointerPress {
    cell: usize,
    point: Point,
}

/// Cell geometry constants shared by the SDK painter and `handle_event`'s
/// hit-testing — one source of truth so a click always lands on the cell the
/// user sees.
pub const GRID_MARGIN: f32 = 8.0;
pub const GRID_GUTTER: f32 = 6.0;
pub const GRID_COLS: usize = 2;
pub const GRID_ROWS: usize = 2;
const GRID_MIN_CELL_HEIGHT: f32 = 34.0;
const GRID_THUMBNAIL_MARGIN: f32 = 6.0;
const GRID_THUMBNAIL_LABEL_HEIGHT: f32 = 16.0;
const GRID_THUMBNAIL_GAP: f32 = 3.0;
const POINTER_DRAG_THRESHOLD: f32 = 8.0;

impl Default for WorkspaceGridView {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceGridView {
    pub fn new() -> Self {
        Self {
            state: WidgetState::new(),
            active_index: 0,
            focused_index: 0,
            space_ids: Vec::new(),
            items: Vec::new(),
            window_counts: Vec::new(),
            thumbnails: Vec::new(),
            activated: None,
            pointer_press: None,
            drag_target: None,
            dropped: None,
        }
    }

    /// Replace the compositor-owned captures associated with the current
    /// ordered Space snapshot. The vector is intentionally allowed to be
    /// shorter than `items`; absent entries simply render the metadata-only
    /// cell.
    pub fn set_thumbnails(&mut self, thumbnails: Vec<Option<ImageView>>) {
        self.thumbnails = thumbnails.into_iter().take(self.items.len()).collect();
    }

    /// Return the decoded compositor capture for one Space, if available.
    pub fn thumbnail(&self, index: usize) -> Option<&ImageView> {
        self.thumbnails.get(index).and_then(Option::as_ref)
    }

    /// Destination rect for the thumbnail while preserving its source aspect
    /// ratio. The label occupies a small strip below it; cells without a
    /// compositor capture return `Rect::ZERO`.
    pub fn thumbnail_rect(&self, index: usize) -> Rect {
        let Some(image) = self.thumbnail(index) else {
            return Rect::ZERO;
        };
        let cell = self.cell_rect(index);
        if cell.width <= 0.0 || cell.height <= 0.0 {
            return Rect::ZERO;
        }
        let max_width = (cell.width - GRID_THUMBNAIL_MARGIN * 2.0).max(0.0);
        let max_height = (cell.height
            - GRID_THUMBNAIL_MARGIN * 2.0
            - GRID_THUMBNAIL_LABEL_HEIGHT
            - GRID_THUMBNAIL_GAP)
            .max(0.0);
        if max_width <= 0.0 || max_height <= 0.0 {
            return Rect::ZERO;
        }
        let source_width = image.display_width().max(1) as f32;
        let source_height = image.display_height().max(1) as f32;
        let aspect = source_width / source_height;
        let (width, height) = if max_width / max_height <= aspect {
            (max_width, max_width / aspect)
        } else {
            (max_height * aspect, max_height)
        };
        Rect::new(
            cell.x + (cell.width - width) * 0.5,
            cell.y + GRID_THUMBNAIL_MARGIN + (max_height - height) * 0.5,
            width,
            height,
        )
    }

    /// Number of rows required to display all current items in the two-column
    /// overview.  Empty grids have no rows and therefore no hit targets.
    pub fn rows(&self) -> usize {
        self.items.len().div_ceil(GRID_COLS)
    }

    /// Screen rect of grid cell `index` (row-major over the dynamic two-column
    /// grid), given the widget's current rect.
    pub fn cell_rect(&self, index: usize) -> Rect {
        if index >= self.items.len() {
            return Rect::ZERO;
        }
        let r = self.rect();
        let grid = Rect::new(
            r.x + GRID_MARGIN,
            r.y + GRID_MARGIN,
            (r.width - GRID_MARGIN * 2.0).max(0.0),
            (r.height - GRID_MARGIN * 2.0).max(0.0),
        );
        let cell_w = (grid.width - GRID_GUTTER) / GRID_COLS as f32;
        let rows = self.rows();
        let cell_h = if rows == 0 {
            0.0
        } else {
            (grid.height - GRID_GUTTER * rows.saturating_sub(1) as f32) / rows as f32
        };
        let row = index / GRID_COLS;
        let col = index % GRID_COLS;
        Rect::new(
            grid.x + col as f32 * (cell_w + GRID_GUTTER),
            grid.y + row as f32 * (cell_h + GRID_GUTTER),
            cell_w,
            cell_h,
        )
    }

    /// Cell containing `point`, if any.
    pub fn cell_at(&self, point: Point) -> Option<usize> {
        (0..self.items.len()).find(|&i| self.cell_rect(i).contains(point))
    }

    /// Index of the most recently pressed cell; drains exactly once.
    pub fn take_activated(&mut self) -> Option<usize> {
        self.activated.take()
    }

    /// Drain a pointer drag that was released over another Space cell.
    pub fn take_dropped(&mut self) -> Option<usize> {
        self.dropped.take()
    }

    /// Return the current pointer drag target for the renderer's drop-target
    /// affordance. The target is transient and never changes compositor state.
    pub fn drag_target(&self) -> Option<usize> {
        self.drag_target
    }

    /// Keep keyboard focus valid after the shell replaces the live Space
    /// snapshot while this overview is open.
    pub fn normalize_focus(&mut self) {
        if self.items.is_empty() {
            self.focused_index = 0;
        } else if self.focused_index >= self.items.len() {
            self.focused_index = self.active_index.min(self.items.len().saturating_sub(1));
        }
    }

    fn move_focus(&mut self, key: KeyCode) -> bool {
        self.normalize_focus();
        let len = self.items.len();
        if len == 0 {
            return false;
        }

        let current = self.focused_index;
        let next = match key {
            KeyCode::ArrowLeft if !current.is_multiple_of(GRID_COLS) => Some(current - 1),
            KeyCode::ArrowRight if current % GRID_COLS + 1 < GRID_COLS && current + 1 < len => {
                Some(current + 1)
            }
            KeyCode::ArrowUp if current >= GRID_COLS => Some(current - GRID_COLS),
            KeyCode::ArrowDown if current + GRID_COLS < len => Some(current + GRID_COLS),
            _ => None,
        };
        if let Some(next) = next {
            self.focused_index = next;
        }
        true
    }

    fn pointer_dragging(&self, point: Point) -> bool {
        let Some(press) = self.pointer_press else {
            return false;
        };
        let dx = point.x - press.point.x;
        let dy = point.y - press.point.y;
        dx.is_finite()
            && dy.is_finite()
            && dx.mul_add(dx, dy * dy) >= POINTER_DRAG_THRESHOLD * POINTER_DRAG_THRESHOLD
    }
}

impl Widget for WorkspaceGridView {
    fn widget_state(&self) -> &WidgetState {
        &self.state
    }
    fn widget_state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }

    fn focusable(&self) -> bool {
        self.state.enabled && self.state.visibility == Visibility::Visible
    }

    fn wants_click_focus(&self) -> bool {
        self.focusable()
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        let rows = self.rows().max(1);
        let dynamic_height = GRID_MARGIN * 2.0
            + rows as f32 * GRID_MIN_CELL_HEIGHT
            + GRID_GUTTER * rows.saturating_sub(1) as f32;
        let size = constraint.clamp(Size::new(240.0, 160.0_f32.max(dynamic_height)));
        self.set_rect(Rect::new(
            self.rect().x,
            self.rect().y,
            size.width,
            size.height,
        ));
        size
    }

    fn draw(&self, _theme: &ThemeContext) {}

    fn handle_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::FocusIn => {
                self.state.focused = true;
                self.normalize_focus();
                EventResult::Handled
            }
            Event::FocusOut => {
                self.state.focused = false;
                EventResult::Handled
            }
            Event::MouseDown {
                button: MouseButton::Left,
                point,
                ..
            } => {
                if !self.rect().contains(*point) {
                    return EventResult::Ignored;
                }
                match self.cell_at(*point) {
                    Some(cell) => {
                        self.state.focused = true;
                        self.focused_index = cell;
                        self.pointer_press = Some(PointerPress {
                            cell,
                            point: *point,
                        });
                        self.drag_target = None;
                        self.dropped = None;
                        EventResult::Handled
                    }
                    None => EventResult::Ignored,
                }
            }
            Event::MouseMove { point, .. } => {
                if self.pointer_press.is_none() {
                    return EventResult::Ignored;
                }
                if self.pointer_dragging(*point) {
                    self.drag_target = self.cell_at(*point);
                }
                // PointerDispatcher's implicit capture keeps this widget
                // informed even when a drag leaves the grid.
                EventResult::Handled
            }
            Event::MouseUp {
                button: MouseButton::Left,
                point,
                ..
            } => {
                let dragging = self.drag_target.is_some() || self.pointer_dragging(*point);
                let Some(press) = self.pointer_press.take() else {
                    return EventResult::Ignored;
                };
                if dragging {
                    let target = self.cell_at(*point);
                    self.drag_target = None;
                    if target.is_some() && target != Some(press.cell) {
                        self.dropped = target;
                    }
                } else if self.cell_at(*point) == Some(press.cell) {
                    self.activated = Some(press.cell);
                }
                EventResult::Handled
            }
            Event::MouseLeave => {
                self.pointer_press = None;
                self.drag_target = None;
                EventResult::Handled
            }
            Event::KeyDown { key, modifiers }
                if !modifiers.meta && !modifiers.control && !modifiers.alt =>
            {
                if !self.state.focused {
                    return EventResult::Ignored;
                }
                match key {
                    KeyCode::ArrowLeft
                    | KeyCode::ArrowRight
                    | KeyCode::ArrowUp
                    | KeyCode::ArrowDown => {
                        self.move_focus(*key);
                        EventResult::Handled
                    }
                    KeyCode::Enter | KeyCode::Space => {
                        self.normalize_focus();
                        if self.focused_index < self.items.len() {
                            self.activated = Some(self.focused_index);
                        }
                        EventResult::Handled
                    }
                    _ => EventResult::Ignored,
                }
            }
            _ => EventResult::Ignored,
        }
    }

    fn accessibility(&self) -> Option<AccessibilityNode> {
        let mut list = AccessibilityNode::new(AccessibilityRole::List, "Spaces");
        list.rect = self.rect();
        list.state.focused = self.state.focused;

        for (index, label) in self.items.iter().enumerate() {
            let mut cell = AccessibilityNode::new(AccessibilityRole::ListItem, label);
            cell.index = index;
            // The list is a direct parent in the widget's accessibility
            // subtree. The eventual AT-SPI exporter may assign a different
            // top-level index, but the child relationship remains explicit.
            cell.parent = Some(0);
            cell.rect = self.cell_rect(index);
            cell.state.selected = index == self.active_index;
            cell.state.focused = self.state.focused && index == self.focused_index;

            let mut description = Vec::new();
            if let Some(id) = self.space_ids.get(index) {
                description.push(format!("Stable Space ID {id}"));
            }
            if let Some(count) = self.window_counts.get(index) {
                let noun = if *count == 1 { "window" } else { "windows" };
                description.push(format!("{count} {noun}"));
            }
            cell.description = description.join("; ");
            list.children.push(cell);
        }

        Some(list)
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
    use crate::event::Modifiers;

    fn grid() -> WorkspaceGridView {
        let mut g = WorkspaceGridView::new();
        g.items = (0..4).map(|i| format!("Desktop {}", i + 1)).collect();
        g.space_ids = (1..=4).collect();
        g.window_counts = vec![0; 4];
        g.set_rect(Rect::new(100.0, 100.0, 240.0, 160.0));
        g
    }

    fn center(rect: Rect) -> Point {
        Point::new(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5)
    }

    fn press(x: f32, y: f32) -> Event {
        Event::MouseDown {
            button: MouseButton::Left,
            point: Point::new(x, y),
            modifiers: Modifiers::NONE,
        }
    }

    fn release(x: f32, y: f32) -> Event {
        Event::MouseUp {
            button: MouseButton::Left,
            point: Point::new(x, y),
            modifiers: Modifiers::NONE,
        }
    }

    fn move_to(x: f32, y: f32) -> Event {
        Event::MouseMove {
            point: Point::new(x, y),
            modifiers: Modifiers::NONE,
        }
    }

    fn key(key: KeyCode) -> Event {
        Event::KeyDown {
            key,
            modifiers: Modifiers::NONE,
        }
    }

    #[test]
    fn each_cell_center_activates_that_cell() {
        for i in 0..4 {
            let mut g = grid();
            let c = g.cell_rect(i);
            let result = g.handle_event(&press(c.x + c.width * 0.5, c.y + c.height * 0.5));
            assert!(matches!(result, EventResult::Handled), "cell {i}");
            let result = g.handle_event(&release(c.x + c.width * 0.5, c.y + c.height * 0.5));
            assert!(matches!(result, EventResult::Handled), "cell {i} release");
            assert_eq!(g.take_activated(), Some(i));
            assert_eq!(g.take_activated(), None, "drains exactly once");
        }
    }

    #[test]
    fn drag_between_cells_reports_drop_without_switch_activation() {
        let mut g = grid();
        let source = center(g.cell_rect(0));
        let target = center(g.cell_rect(1));

        assert!(matches!(
            g.handle_event(&press(source.x, source.y)),
            EventResult::Handled
        ));
        assert!(matches!(
            g.handle_event(&move_to(target.x, target.y)),
            EventResult::Handled
        ));
        assert_eq!(g.drag_target(), Some(1));
        assert_eq!(g.take_activated(), None);

        assert!(matches!(
            g.handle_event(&release(target.x, target.y)),
            EventResult::Handled
        ));
        assert_eq!(g.take_dropped(), Some(1));
        assert_eq!(g.take_activated(), None);
        assert_eq!(g.drag_target(), None);
    }

    #[test]
    fn drag_released_outside_grid_is_cancelled() {
        let mut g = grid();
        let source = center(g.cell_rect(0));
        assert!(matches!(
            g.handle_event(&press(source.x, source.y)),
            EventResult::Handled
        ));
        assert!(matches!(
            g.handle_event(&move_to(10.0, 10.0)),
            EventResult::Handled
        ));
        assert_eq!(g.drag_target(), None);
        assert!(matches!(
            g.handle_event(&release(10.0, 10.0)),
            EventResult::Handled
        ));
        assert_eq!(g.take_dropped(), None);
        assert_eq!(g.take_activated(), None);
    }

    #[test]
    fn press_in_the_margin_is_ignored() {
        let mut g = grid();
        // Inside the widget rect but within the 8px outer margin.
        let result = g.handle_event(&press(102.0, 102.0));
        assert!(matches!(result, EventResult::Ignored));
        assert_eq!(g.take_activated(), None);
    }

    #[test]
    fn press_outside_the_widget_is_ignored() {
        let mut g = grid();
        let result = g.handle_event(&press(10.0, 10.0));
        assert!(matches!(result, EventResult::Ignored));
        assert_eq!(g.take_activated(), None);
    }

    #[test]
    fn cells_tile_row_major() {
        let g = grid();
        let c0 = g.cell_rect(0);
        let c1 = g.cell_rect(1);
        let c2 = g.cell_rect(2);
        assert!(c1.x > c0.x && (c1.y - c0.y).abs() < f32::EPSILON);
        assert!(c2.y > c0.y && (c2.x - c0.x).abs() < f32::EPSILON);
    }

    #[test]
    fn dynamic_items_add_rows_and_only_their_cells_hit() {
        let mut g = WorkspaceGridView::new();
        g.items = (0..5).map(|i| format!("Space {}", i + 1)).collect();
        g.set_rect(Rect::new(0.0, 0.0, 240.0, 260.0));

        assert_eq!(g.rows(), 3);
        let fifth = g.cell_rect(4);
        assert!(fifth.y > g.cell_rect(0).y);
        assert!(g.cell_at(Point::new(fifth.x + 1.0, fifth.y + 1.0)) == Some(4));
        let unused = g.cell_rect(5);
        assert!(unused.width == 0.0 && unused.height == 0.0);
        assert_eq!(g.cell_at(Point::new(239.0, 259.0)), None);
    }

    #[test]
    fn empty_grid_has_no_hit_targets() {
        let mut g = WorkspaceGridView::new();
        g.set_rect(Rect::new(0.0, 0.0, 240.0, 160.0));
        assert_eq!(g.rows(), 0);
        let unused = g.cell_rect(0);
        assert!(unused.width == 0.0 && unused.height == 0.0);
        assert_eq!(g.cell_at(Point::new(100.0, 100.0)), None);
    }

    #[test]
    fn keyboard_navigation_is_focus_gated_and_bounds_safe() {
        let mut g = grid();
        assert!(matches!(
            g.handle_event(&key(KeyCode::ArrowRight)),
            EventResult::Ignored
        ));
        assert_eq!(g.focused_index, 0);

        g.widget_state_mut().focused = true;
        assert!(matches!(
            g.handle_event(&key(KeyCode::ArrowRight)),
            EventResult::Handled
        ));
        assert_eq!(g.focused_index, 1);
        assert!(matches!(
            g.handle_event(&key(KeyCode::ArrowDown)),
            EventResult::Handled
        ));
        assert_eq!(g.focused_index, 3);
        assert!(matches!(
            g.handle_event(&key(KeyCode::ArrowRight)),
            EventResult::Handled
        ));
        assert_eq!(g.focused_index, 3, "right at row edge must not wrap");
        assert!(matches!(
            g.handle_event(&key(KeyCode::ArrowDown)),
            EventResult::Handled
        ));
        assert_eq!(
            g.focused_index, 3,
            "down beyond the last row must not overflow"
        );
        assert!(matches!(
            g.handle_event(&key(KeyCode::ArrowLeft)),
            EventResult::Handled
        ));
        assert_eq!(g.focused_index, 2);
        assert!(matches!(
            g.handle_event(&key(KeyCode::ArrowUp)),
            EventResult::Handled
        ));
        assert_eq!(g.focused_index, 0);
    }

    #[test]
    fn enter_and_space_activate_the_focused_cell_once() {
        let mut g = grid();
        g.widget_state_mut().focused = true;
        g.focused_index = 2;

        assert!(matches!(
            g.handle_event(&key(KeyCode::Enter)),
            EventResult::Handled
        ));
        assert_eq!(g.take_activated(), Some(2));
        assert_eq!(g.take_activated(), None);

        assert!(matches!(
            g.handle_event(&key(KeyCode::Space)),
            EventResult::Handled
        ));
        assert_eq!(g.take_activated(), Some(2));
    }

    #[test]
    fn focus_is_clamped_when_items_shrink_or_empty() {
        let mut g = grid();
        g.focused_index = 3;
        g.items.truncate(2);
        g.normalize_focus();
        assert_eq!(g.focused_index, 0, "focus falls back to the active Space");

        g.items.clear();
        g.normalize_focus();
        assert_eq!(g.focused_index, 0);
        g.widget_state_mut().focused = true;
        assert!(matches!(
            g.handle_event(&key(KeyCode::Enter)),
            EventResult::Handled
        ));
        assert_eq!(
            g.take_activated(),
            None,
            "empty grids cannot activate a stale cell"
        );
    }

    #[test]
    fn accessibility_exposes_dynamic_space_cells_and_state() {
        let mut g = grid();
        g.active_index = 2;
        g.focused_index = 1;
        g.widget_state_mut().focused = true;

        let node = g.accessibility().expect("workspace accessibility node");
        assert_eq!(node.role, AccessibilityRole::List);
        assert_eq!(node.label, "Spaces");
        assert_eq!(node.rect.x, g.rect().x);
        assert_eq!(node.rect.y, g.rect().y);
        assert_eq!(node.rect.width, g.rect().width);
        assert_eq!(node.rect.height, g.rect().height);
        assert_eq!(node.children.len(), g.items.len());

        let focused = &node.children[1];
        assert_eq!(focused.role, AccessibilityRole::ListItem);
        assert_eq!(focused.label, "Desktop 2");
        assert!(!focused.state.selected);
        assert!(focused.state.focused);
        assert_eq!(focused.index, 1);
        assert_eq!(focused.parent, Some(0));
        assert_eq!(focused.rect.x, g.cell_rect(1).x);
        assert_eq!(focused.rect.y, g.cell_rect(1).y);
        assert_eq!(focused.rect.width, g.cell_rect(1).width);
        assert_eq!(focused.rect.height, g.cell_rect(1).height);
        assert_eq!(focused.description, "Stable Space ID 2; 0 windows");

        let active = &node.children[2];
        assert!(active.state.selected);
        assert!(!active.state.focused);
        assert_eq!(active.description, "Stable Space ID 3; 0 windows");
    }

    #[test]
    fn accessibility_tracks_dynamic_items_without_stale_cells_or_metadata() {
        let mut g = WorkspaceGridView::new();
        g.items = vec!["Personal".into(), "Work".into(), "Video".into()];
        g.space_ids = vec![11, 22];
        g.window_counts = vec![1, 3, 99, 100];
        g.active_index = 99;
        g.focused_index = 99;

        let node = g.accessibility().expect("workspace accessibility node");
        assert_eq!(node.children.len(), 3);
        assert!(node.children.iter().all(|child| !child.state.selected));
        assert!(node.children.iter().all(|child| !child.state.focused));
        assert_eq!(node.children[0].description, "Stable Space ID 11; 1 window");
        assert_eq!(
            node.children[1].description,
            "Stable Space ID 22; 3 windows"
        );
        assert_eq!(node.children[2].description, "99 windows");
        assert_eq!(node.children[2].rect.x, g.cell_rect(2).x);
        assert_eq!(node.children[2].rect.y, g.cell_rect(2).y);
    }

    #[test]
    fn thumbnail_geometry_is_inside_cell_and_preserves_aspect_ratio() {
        let mut g = grid();
        g.set_thumbnails(vec![Some(
            ImageView::new(640, 400, vec![0; 640 * 400 * 4]).unwrap(),
        )]);

        let cell = g.cell_rect(0);
        let thumbnail = g.thumbnail_rect(0);
        assert!(thumbnail.width > 0.0 && thumbnail.height > 0.0);
        assert!(cell.contains(Point::new(thumbnail.x, thumbnail.y)));
        assert!(cell.contains(Point::new(
            thumbnail.x + thumbnail.width,
            thumbnail.y + thumbnail.height
        )));
        let expected = 640.0 / 400.0;
        assert!(((thumbnail.width / thumbnail.height) - expected).abs() < 0.001);
    }

    #[test]
    fn missing_thumbnail_has_no_draw_rect() {
        let g = grid();
        let rect = g.thumbnail_rect(0);
        assert_eq!(rect.width, 0.0);
        assert_eq!(rect.height, 0.0);
    }
}
