use crate::{
    theme::ThemeContext, AccessibilityNode, AccessibilityRole, Event, EventResult,
    LayoutConstraint, Rect, Size, Widget, WidgetState,
};

#[derive(Debug, Clone)]
pub struct TreeNode {
    pub label: String,
    pub children: Vec<TreeNode>,
    pub expanded: bool,
    pub icon: Option<String>,
}

impl TreeNode {
    pub fn new<S: Into<String>>(label: S) -> Self {
        Self {
            label: label.into(),
            children: vec![],
            expanded: false,
            icon: None,
        }
    }
}

/// Pixel height of one visible row. Also the line-pitch `slopos-sdk`'s
/// `draw_tree_node` currently hardcodes; kept as a named constant here so a
/// future edit can make the painter read `TreeRow::rect` instead of
/// recomputing this same number.
pub const ROW_HEIGHT: f32 = 18.0;
/// Vertical inset before the first row, matching the painter's `rect.y + 8.0`.
const TOP_PADDING: f32 = 8.0;
/// Per-depth horizontal indent, matching the painter's `depth as f32 * 12.0`
/// text offset — the disclosure area shifts with the row's own indentation
/// rather than sitting in a fixed column nested rows never reach.
const INDENT_WIDTH: f32 = 12.0;
/// Width of the leading hit-area that toggles expansion, per the task spec
/// ("leading ~16px of the row").
const DISCLOSURE_WIDTH: f32 = 16.0;

/// One visible row of a `TreeView`, computed by `layout()` from the current
/// `roots` tree honouring each node's `expanded` flag. This is the single
/// source of truth for both hit-testing (see `handle_event`) and, eventually,
/// painting — a painter that walks `rows` instead of re-deriving geometry
/// from `TreeNode` can never disagree with what was actually clicked.
#[derive(Debug, Clone)]
pub struct TreeRow {
    /// Indices from the root(s) down to this node, e.g. `[0, 3]` = the fourth
    /// child of the first root. Matches `TreeView::selected_path`'s format.
    pub path: Vec<usize>,
    /// Nesting depth; `0` for a top-level root.
    pub depth: usize,
    pub label: String,
    pub has_children: bool,
    pub expanded: bool,
    /// Full-width row rect used for selection hit-testing.
    pub rect: Rect,
    /// Leading disclosure/toggle hit-area within `rect`. Zero-sized (and
    /// never hit) for leaf nodes.
    pub disclosure_rect: Rect,
}

pub struct TreeView {
    state: WidgetState,
    pub roots: Vec<TreeNode>,
    pub selected_path: Option<Vec<usize>>,
    /// Flat list of currently visible rows, rebuilt by `layout()`. Public so
    /// consumers (the SDK painter, tests) can walk the exact same geometry
    /// used for hit-testing instead of recomputing it.
    pub rows: Vec<TreeRow>,
}

impl Default for TreeView {
    fn default() -> Self {
        Self::new()
    }
}

impl TreeView {
    pub fn new() -> Self {
        Self {
            state: WidgetState::new(),
            roots: vec![],
            selected_path: None,
            rows: vec![],
        }
    }

    /// Rebuilds `rows` from `roots`, honouring each node's `expanded` flag.
    /// Depth-first, pre-order — a node's own row is always emitted before
    /// its children's rows, matching the painter's recursive walk.
    fn rebuild_rows(&mut self) {
        let rect = self.rect();
        let mut rows = Vec::new();
        let mut y = rect.y + TOP_PADDING;
        Self::collect_rows(&self.roots, &[], 0, rect, &mut y, &mut rows);
        self.rows = rows;
    }

    fn collect_rows(
        nodes: &[TreeNode],
        prefix: &[usize],
        depth: usize,
        rect: Rect,
        y: &mut f32,
        rows: &mut Vec<TreeRow>,
    ) {
        for (index, node) in nodes.iter().enumerate() {
            let mut path = prefix.to_vec();
            path.push(index);
            let has_children = !node.children.is_empty();
            let row_rect = Rect::new(rect.x, *y, rect.width, ROW_HEIGHT);
            let indent_x = rect.x + depth as f32 * INDENT_WIDTH;
            let disclosure_rect = if has_children {
                Rect::new(indent_x, *y, DISCLOSURE_WIDTH, ROW_HEIGHT)
            } else {
                Rect::new(indent_x, *y, 0.0, 0.0)
            };
            rows.push(TreeRow {
                depth,
                label: node.label.clone(),
                has_children,
                expanded: node.expanded,
                rect: row_rect,
                disclosure_rect,
                path: path.clone(),
            });
            *y += ROW_HEIGHT;
            if node.expanded && has_children {
                Self::collect_rows(&node.children, &path, depth + 1, rect, y, rows);
            }
        }
    }

    /// Flips `expanded` on the node at `path`, if it exists.
    fn toggle_expanded(&mut self, path: &[usize]) {
        if let Some(node) = Self::node_at_mut(&mut self.roots, path) {
            node.expanded = !node.expanded;
        }
    }

    fn node_at_mut<'a>(nodes: &'a mut [TreeNode], path: &[usize]) -> Option<&'a mut TreeNode> {
        let (&first, rest) = path.split_first()?;
        let node = nodes.get_mut(first)?;
        if rest.is_empty() {
            Some(node)
        } else {
            Self::node_at_mut(&mut node.children, rest)
        }
    }
}

impl Widget for TreeView {
    fn widget_state(&self) -> &WidgetState {
        &self.state
    }
    fn widget_state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        let width = constraint.max_width.min(200.0);
        let height = constraint.max_height.min(300.0);
        let size = constraint.clamp(Size::new(width, height));
        self.set_rect(Rect::new(
            self.rect().x,
            self.rect().y,
            size.width,
            size.height,
        ));
        self.rebuild_rows();
        size
    }

    fn draw(&self, _theme: &ThemeContext) {}

    fn handle_event(&mut self, event: &Event) -> EventResult {
        if let Event::MouseDown {
            button: crate::event::MouseButton::Left,
            point,
            ..
        } = event
        {
            if !self.rect().contains(*point) {
                return EventResult::Ignored;
            }

            let hit = self
                .rows
                .iter()
                .find(|row| row.rect.contains(*point))
                .cloned();
            if let Some(row) = hit {
                if row.has_children && row.disclosure_rect.contains(*point) {
                    self.toggle_expanded(&row.path);
                    self.rebuild_rows();
                } else {
                    self.selected_path = Some(row.path);
                }
                return EventResult::Handled;
            }
        }
        EventResult::Ignored
    }

    fn accessibility(&self) -> Option<AccessibilityNode> {
        Some(AccessibilityNode::new(AccessibilityRole::Tree, "files"))
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
    use crate::event::{Modifiers, MouseButton};
    use crate::Point;

    /// Builds the same sidebar shape `apps/finder` constructs: two expanded
    /// roots, "Favorites" (6 children) and "Locations" (2 children).
    fn finder_like_tree() -> Vec<TreeNode> {
        let mut favorites = TreeNode::new("Favorites");
        favorites.children.push(TreeNode::new("SLOPOS Share"));
        favorites.children.push(TreeNode::new("Recents"));
        favorites.children.push(TreeNode::new("Applications"));
        favorites.children.push(TreeNode::new("Desktop"));
        favorites.children.push(TreeNode::new("Documents"));
        favorites.children.push(TreeNode::new("Downloads"));
        favorites.expanded = true;

        let mut locations = TreeNode::new("Locations");
        locations.children.push(TreeNode::new("SLOPOS-I"));
        locations.children.push(TreeNode::new("Network"));
        locations.expanded = true;

        vec![favorites, locations]
    }

    #[test]
    fn layout_builds_one_row_per_visible_node_in_tree_order() {
        let mut tree = TreeView::new();
        tree.roots = finder_like_tree();
        tree.layout(LayoutConstraint::tight(Size::new(200.0, 300.0)));

        let labels: Vec<&str> = tree.rows.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(
            labels,
            vec![
                "Favorites",
                "SLOPOS Share",
                "Recents",
                "Applications",
                "Desktop",
                "Documents",
                "Downloads",
                "Locations",
                "SLOPOS-I",
                "Network",
            ]
        );

        // The row that used to be faked as a hardcoded percentage band now
        // carries its real path.
        let desktop_row = tree.rows.iter().find(|r| r.label == "Desktop").unwrap();
        assert_eq!(desktop_row.path, vec![0, 3]);
        assert_eq!(desktop_row.depth, 1);
    }

    #[test]
    fn collapsed_node_hides_its_children_from_rows() {
        let mut root = TreeNode::new("Favorites");
        root.children.push(TreeNode::new("SLOPOS Share"));
        root.children.push(TreeNode::new("Recents"));
        // expanded stays false (the default).

        let mut tree = TreeView::new();
        tree.roots = vec![root];
        tree.layout(LayoutConstraint::tight(Size::new(200.0, 300.0)));

        assert_eq!(tree.rows.len(), 1);
        assert!(tree.rows[0].has_children);
        assert!(!tree.rows[0].expanded);
    }

    #[test]
    fn clicking_a_row_selects_its_real_path_not_a_hardcoded_band() {
        let mut tree = TreeView::new();
        tree.roots = finder_like_tree();
        tree.set_rect(Rect::new(0.0, 0.0, 200.0, 300.0));
        tree.layout(LayoutConstraint::tight(Size::new(200.0, 300.0)));

        // "Desktop" is the 5th row (index 4); click well clear of the
        // leading disclosure strip.
        let row = tree.rows[4].clone();
        assert_eq!(row.label, "Desktop");
        let click_y = row.rect.y + 9.0;

        let result = tree.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point: Point::new(100.0, click_y),
            modifiers: Modifiers::NONE,
        });

        assert!(matches!(result, EventResult::Handled));
        assert_eq!(tree.selected_path, Some(vec![0, 3]));
    }

    #[test]
    fn clicking_favorites_row_selects_favorites_not_desktop() {
        // Regression test for the reported bug: clicking "Favorites" must
        // select Favorites ([0]), never the hardcoded Desktop path ([0, 3]).
        let mut tree = TreeView::new();
        tree.roots = finder_like_tree();
        tree.set_rect(Rect::new(0.0, 0.0, 200.0, 300.0));
        tree.layout(LayoutConstraint::tight(Size::new(200.0, 300.0)));

        let row = tree.rows[0].clone();
        assert_eq!(row.label, "Favorites");
        let click_y = row.rect.y + 9.0;

        tree.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point: Point::new(100.0, click_y),
            modifiers: Modifiers::NONE,
        });

        assert_eq!(tree.selected_path, Some(vec![0]));
    }

    #[test]
    fn clicking_disclosure_area_toggles_expansion_without_selecting() {
        let mut root = TreeNode::new("Locations");
        root.children.push(TreeNode::new("SLOPOS-I"));
        root.children.push(TreeNode::new("Network"));
        // starts collapsed

        let mut tree = TreeView::new();
        tree.roots = vec![root];
        tree.set_rect(Rect::new(0.0, 0.0, 200.0, 300.0));
        tree.layout(LayoutConstraint::tight(Size::new(200.0, 300.0)));
        assert_eq!(tree.rows.len(), 1);

        let row = tree.rows[0].clone();
        let click_point = Point::new(row.rect.x + 4.0, row.rect.y + 9.0);
        assert!(row.disclosure_rect.contains(click_point));

        let result = tree.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point: click_point,
            modifiers: Modifiers::NONE,
        });

        assert!(matches!(result, EventResult::Handled));
        assert!(tree.roots[0].expanded);
        assert_eq!(tree.selected_path, None);
        // Rows were rebuilt immediately: children are now visible.
        assert_eq!(tree.rows.len(), 3);
    }

    #[test]
    fn clicking_a_leaf_row_selects_even_inside_the_leading_strip() {
        let mut tree = TreeView::new();
        tree.roots = finder_like_tree();
        tree.set_rect(Rect::new(0.0, 0.0, 200.0, 300.0));
        tree.layout(LayoutConstraint::tight(Size::new(200.0, 300.0)));

        // "SLOPOS Share" (row index 1) has no children, so its disclosure_rect is
        // zero-sized and must never intercept the click.
        let row = tree.rows[1].clone();
        assert_eq!(row.label, "SLOPOS Share");
        assert!(!row.has_children);

        let result = tree.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point: Point::new(row.rect.x + 2.0, row.rect.y + 9.0),
            modifiers: Modifiers::NONE,
        });

        assert!(matches!(result, EventResult::Handled));
        assert_eq!(tree.selected_path, Some(vec![0, 0]));
    }

    #[test]
    fn click_outside_rect_is_ignored() {
        let mut tree = TreeView::new();
        tree.roots = finder_like_tree();
        tree.set_rect(Rect::new(0.0, 0.0, 200.0, 300.0));
        tree.layout(LayoutConstraint::tight(Size::new(200.0, 300.0)));

        let result = tree.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point: Point::new(500.0, 500.0),
            modifiers: Modifiers::NONE,
        });

        assert!(matches!(result, EventResult::Ignored));
        assert_eq!(tree.selected_path, None);
    }
}
