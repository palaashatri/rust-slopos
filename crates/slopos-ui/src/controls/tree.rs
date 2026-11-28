//! Tree view: a hierarchy of labelled rows that can be expanded and collapsed.
//!
//! The app supplies the hierarchy as [`TreeNode`]s. The control flattens the
//! expanded part into visible rows and hands them to a [`ListView`], which
//! owns scrolling, selection and type-ahead, so a tree costs no more per frame
//! than a list. Keyboard: Up, Down, Home, End, PageUp and PageDown move the
//! cursor as in a list; Right expands a collapsed row or, on an expanded row,
//! moves to its first child; Left collapses an expanded row or moves to the
//! parent of the cursor row; Enter activates. A primary press on the
//! disclosure box toggles a row, and a press elsewhere on it selects the row.
//!
//! Accessibility: a `Tree` node whose children are one `TreeItem` per visible
//! row, each carrying its level, its expanded state (rows with children only)
//! and its selection.

use std::collections::BTreeSet;

use super::list::{ListOutcome, ListView};
use crate::event::{Event, Key, Modifiers, PointerButton};
use crate::foundation::geometry::Rect;
use crate::render::{RasterSurface, RenderError};
use crate::theme::palette;

/// Horizontal space given to each depth level, disclosure box included.
const INDENT: u32 = 14;
/// Side of the disclosure box drawn for rows with children.
const DISCLOSURE: u32 = 9;
/// The most rows a tree flattens to. Anything beyond is not shown, which
/// keeps the visible model and the accessibility tree bounded.
pub const MAX_TREE_ROWS: usize = 100_000;
/// At most this many row nodes are exposed to assistive technology at once.
const MAX_ACCESSIBLE_ROWS: usize = 256;

/// One node of the hierarchy the app supplies. Keys must be unique to the
/// tree; they identify a row across rebuilds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeNode {
    pub key: u64,
    pub label: String,
    pub children: Vec<TreeNode>,
}

/// A visible row: a node whose ancestors are all expanded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeRow {
    pub key: u64,
    pub depth: u32,
    pub label: String,
    pub has_children: bool,
    pub expanded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeOutcome {
    /// The event did not concern the tree.
    Ignored,
    /// Scroll position, expansion or focus look changed; repaint.
    Changed,
    /// The selection or cursor moved.
    SelectionChanged,
    /// Enter, or a double-click, on the row with this key.
    Activated(u64),
    /// A secondary click or the Menu key; `key` is `None` below the last row.
    ContextMenu { key: Option<u64>, x: i32, y: i32 },
    /// A press landed in the tree; the app should give it keyboard focus.
    FocusRequested,
}

#[derive(Debug, Clone)]
pub struct TreeView {
    pub id: u64,
    pub label: String,
    focused: bool,
    enabled: bool,
    roots: Vec<TreeNode>,
    expanded: BTreeSet<u64>,
    rows: Vec<TreeRow>,
    list: ListView,
}

/// Pre-order flattening of the expanded part of `roots`, bounded by
/// [`MAX_TREE_ROWS`]. Uses an explicit stack, so depth cannot exhaust the
/// call stack.
fn flatten(roots: &[TreeNode], expanded: &BTreeSet<u64>) -> Vec<TreeRow> {
    let mut rows = Vec::new();
    let mut stack: Vec<(&TreeNode, u32)> = roots.iter().rev().map(|node| (node, 0)).collect();
    while let Some((node, depth)) = stack.pop() {
        if rows.len() >= MAX_TREE_ROWS {
            break;
        }
        let has_children = !node.children.is_empty();
        let is_open = has_children && expanded.contains(&node.key);
        rows.push(TreeRow {
            key: node.key,
            depth,
            label: node.label.clone(),
            has_children,
            expanded: is_open,
        });
        if is_open {
            for child in node.children.iter().rev() {
                stack.push((child, depth.saturating_add(1)));
            }
        }
    }
    rows
}

fn indent_of(depth: u32) -> i32 {
    i32::try_from(depth.saturating_mul(INDENT)).unwrap_or(i32::MAX)
}

/// Box with a minus (expanded) or plus (collapsed) drawn inside it.
fn paint_disclosure(
    surface: &RasterSurface,
    bounds: Rect,
    expanded: bool,
) -> Result<(), RenderError> {
    let ink = palette().ink_secondary;
    surface.stroke_rect(bounds, ink)?;
    let half = (DISCLOSURE / 2) as i32;
    let arm = (DISCLOSURE / 2 + 1) as u32;
    surface.fill_rect(
        Rect {
            x: bounds.x + 2,
            y: bounds.y + half,
            width: arm,
            height: 1,
        },
        ink,
    )?;
    if !expanded {
        surface.fill_rect(
            Rect {
                x: bounds.x + half,
                y: bounds.y + 2,
                width: 1,
                height: arm,
            },
            ink,
        )?;
    }
    Ok(())
}

impl TreeView {
    pub fn new(id: u64, label: &str, row_height: u32) -> Self {
        Self {
            id,
            label: label.to_owned(),
            focused: false,
            enabled: true,
            roots: Vec::new(),
            expanded: BTreeSet::new(),
            rows: Vec::new(),
            list: ListView::new(id, label, row_height),
        }
    }

    pub fn bounds(&self) -> Rect {
        self.list.bounds()
    }

    pub fn set_bounds(&mut self, bounds: Rect) {
        self.list.set_bounds(bounds);
    }

    pub fn focused(&self) -> bool {
        self.focused
    }

    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        self.list.focused = focused;
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.list.enabled = enabled;
    }

    /// The visible rows, top to bottom.
    pub fn rows(&self) -> &[TreeRow] {
        &self.rows
    }

    pub fn is_expanded(&self, key: u64) -> bool {
        self.expanded.contains(&key)
    }

    /// Keys of the selected visible rows, top to bottom.
    pub fn selection_keys(&self) -> Vec<u64> {
        self.list
            .selection()
            .into_iter()
            .filter_map(|index| self.rows.get(index))
            .map(|row| row.key)
            .collect()
    }

    /// Key of the cursor row, if any.
    pub fn cursor_key(&self) -> Option<u64> {
        self.list
            .cursor()
            .and_then(|index| self.rows.get(index))
            .map(|row| row.key)
    }

    /// Replace the hierarchy. Expansion state is kept for keys that still
    /// exist, and the selection follows its keys where they survive.
    pub fn set_roots(&mut self, roots: Vec<TreeNode>) {
        let keep = self.selection_keys();
        let fallback = self.cursor_key();
        self.roots = roots;
        self.rebuild(&keep, fallback);
    }

    /// Select the visible row with `key` and scroll it into view. False when
    /// no visible row has that key.
    pub fn select_key(&mut self, key: u64) -> bool {
        match self.rows.iter().position(|row| row.key == key) {
            Some(index) => {
                self.list.select_only(index);
                true
            }
            None => false,
        }
    }

    /// Expand the row `key`. False when there is no visible row with
    /// children for it, or it is already expanded.
    pub fn expand(&mut self, key: u64) -> bool {
        self.set_expanded(key, true)
    }

    /// Collapse the row `key`. A selection inside it moves to that row.
    pub fn collapse(&mut self, key: u64) -> bool {
        self.set_expanded(key, false)
    }

    fn set_expanded(&mut self, key: u64, expanded: bool) -> bool {
        let (has_children, is_open) = match self.rows.iter().find(|row| row.key == key) {
            Some(row) => (row.has_children, row.expanded),
            None => return false,
        };
        if !has_children || is_open == expanded {
            return false;
        }
        if expanded {
            self.expanded.insert(key);
        } else {
            self.expanded.remove(&key);
        }
        let keep = self.selection_keys();
        let fallback = if expanded {
            self.cursor_key()
        } else {
            Some(key)
        };
        self.rebuild(&keep, fallback);
        true
    }

    /// Re-flatten after a change in the hierarchy or expansion. The rows in
    /// `keep` are selected again where they are still visible; when none is,
    /// `fallback` is selected if it is visible.
    fn rebuild(&mut self, keep: &[u64], fallback: Option<u64>) {
        self.rows = flatten(&self.roots, &self.expanded);
        self.list.set_len(self.rows.len());
        let rows = &self.rows;
        let index_of = |key: u64| rows.iter().position(|row| row.key == key);
        let mut indices: Vec<usize> = keep.iter().filter_map(|&key| index_of(key)).collect();
        if indices.is_empty() {
            indices.extend(fallback.and_then(index_of));
        }
        self.list.set_selection(&indices);
    }

    /// Handle an event. `pointer_modifiers` are the modifiers held during a
    /// pointer event.
    pub fn handle(&mut self, event: &Event, pointer_modifiers: Modifiers) -> TreeOutcome {
        if !self.enabled {
            return TreeOutcome::Ignored;
        }
        if let Event::KeyPressed { key, .. } = event {
            if self.focused && matches!(key, Key::Left | Key::Right) {
                return self.horizontal(matches!(key, Key::Right));
            }
        }
        if let Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
        } = event
        {
            if let Some(outcome) = self.disclosure_press(*x, *y) {
                return outcome;
            }
        }
        let rows = &self.rows;
        let label_of = |index: usize| {
            rows.get(index)
                .map_or_else(String::new, |row| row.label.clone())
        };
        let outcome = self.list.handle(event, pointer_modifiers, &label_of);
        match outcome {
            ListOutcome::Ignored => TreeOutcome::Ignored,
            ListOutcome::Changed => TreeOutcome::Changed,
            ListOutcome::SelectionChanged => TreeOutcome::SelectionChanged,
            ListOutcome::FocusRequested => TreeOutcome::FocusRequested,
            ListOutcome::Activated(index) => self
                .rows
                .get(index)
                .map_or(TreeOutcome::Ignored, |row| TreeOutcome::Activated(row.key)),
            ListOutcome::ContextMenu { index, x, y } => TreeOutcome::ContextMenu {
                key: index
                    .and_then(|index| self.rows.get(index))
                    .map(|row| row.key),
                x,
                y,
            },
        }
    }

    /// Left or Right with the cursor on a row.
    fn horizontal(&mut self, right: bool) -> TreeOutcome {
        let Some(index) = self.list.cursor() else {
            return TreeOutcome::Ignored;
        };
        let Some(row) = self.rows.get(index) else {
            return TreeOutcome::Ignored;
        };
        let (key, depth, has_children, is_open) =
            (row.key, row.depth, row.has_children, row.expanded);
        if right {
            if !has_children {
                return TreeOutcome::Ignored;
            }
            if !is_open {
                self.set_expanded(key, true);
                return TreeOutcome::Changed;
            }
            // The first child of an expanded row is the next visible row.
            return match self.rows.get(index + 1) {
                Some(next) if next.depth > depth => {
                    self.list.select_only(index + 1);
                    TreeOutcome::SelectionChanged
                }
                _ => TreeOutcome::Ignored,
            };
        }
        if is_open {
            self.set_expanded(key, false);
            return TreeOutcome::Changed;
        }
        // The parent is the nearest row above with a smaller depth.
        match self.rows[..index].iter().rposition(|row| row.depth < depth) {
            Some(parent) => {
                self.list.select_only(parent);
                TreeOutcome::SelectionChanged
            }
            None => TreeOutcome::Ignored,
        }
    }

    /// A primary press on the disclosure box of a row toggles it.
    fn disclosure_press(&mut self, x: i32, y: i32) -> Option<TreeOutcome> {
        let index = self.list.row_at(x, y)?;
        let row = self.rows.get(index)?;
        if !row.has_children {
            return None;
        }
        let (key, is_open) = (row.key, row.expanded);
        let rect = self.list.row_rect(index);
        let left = rect.x.saturating_add(indent_of(row.depth));
        let right = left.saturating_add(INDENT as i32);
        if x < left || x >= right {
            return None;
        }
        self.set_expanded(key, !is_open);
        Some(if self.focused {
            TreeOutcome::Changed
        } else {
            TreeOutcome::FocusRequested
        })
    }

    /// Paint the rows. For each visible row the tree draws the selection, the
    /// disclosure box and the focus ring, then calls
    /// `paint_label(surface, row, label_rect, selected)` with the rectangle
    /// to the right of the indent and disclosure, clipped to the tree.
    pub fn paint(
        &self,
        surface: &RasterSurface,
        mut paint_label: impl FnMut(&RasterSurface, &TreeRow, Rect, bool) -> Result<(), RenderError>,
    ) -> Result<(), RenderError> {
        let rows = &self.rows;
        self.list.paint(surface, |surface, index, rect, selected| {
            let Some(row) = rows.get(index) else {
                return Ok(());
            };
            let left = rect.x.saturating_add(indent_of(row.depth));
            if row.has_children {
                let box_rect = Rect {
                    x: left.saturating_add(3),
                    y: rect.y + (rect.height as i32 - DISCLOSURE as i32) / 2,
                    width: DISCLOSURE,
                    height: DISCLOSURE,
                };
                paint_disclosure(surface, box_rect, row.expanded)?;
            }
            let text_left = left.saturating_add(INDENT as i32);
            let inset = text_left.saturating_sub(rect.x).max(0) as u32;
            let label_rect = Rect {
                x: text_left,
                y: rect.y,
                width: rect.width.saturating_sub(inset),
                height: rect.height,
            };
            paint_label(surface, row, label_rect, selected)
        })
    }

    /// The accessibility id of the node for visible row `index`.
    pub fn row_node_id(&self, index: usize) -> u64 {
        self.list.row_node_id(index)
    }

    /// The node a screen reader should focus while the tree has focus.
    pub fn focus_node(&self) -> u64 {
        self.list.focus_node()
    }

    /// Map an accessibility node id back to the index of a visible row.
    pub fn row_for_node(&self, node: u64) -> Option<usize> {
        self.list.row_for_node(node)
    }

    /// A `Tree` node and one `TreeItem` per visible row (capped).
    pub fn accessibility_nodes(&self) -> Vec<(u64, accesskit::Node)> {
        let mut tree = accesskit::Node::new(accesskit::Role::Tree);
        tree.set_label(self.label.as_str());
        tree.set_bounds(super::bounds_of(self.list.bounds()));
        tree.add_action(accesskit::Action::Focus);
        let visible: Vec<usize> = self.list.visible_rows().take(MAX_ACCESSIBLE_ROWS).collect();
        tree.set_children(
            visible
                .iter()
                .map(|&index| accesskit::NodeId(self.list.row_node_id(index)))
                .collect::<Vec<_>>(),
        );
        let mut nodes = vec![(self.id, tree)];
        for index in visible {
            let Some(row) = self.rows.get(index) else {
                continue;
            };
            let mut item = accesskit::Node::new(accesskit::Role::TreeItem);
            item.set_label(row.label.as_str());
            item.set_bounds(super::bounds_of(self.list.row_rect(index)));
            item.set_level(row.depth as usize + 1);
            if row.has_children {
                item.set_expanded(row.expanded);
            }
            item.set_selected(self.list.is_selected(index));
            item.add_action(accesskit::Action::Click);
            item.add_action(accesskit::Action::Focus);
            nodes.push((self.list.row_node_id(index), item));
        }
        nodes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::{color::Color, geometry::Scale};

    fn node(key: u64, label: &str, children: Vec<TreeNode>) -> TreeNode {
        TreeNode {
            key,
            label: label.to_owned(),
            children,
        }
    }

    /// Documents (1) > Letters (11) > draft.txt (111), Photos (12); Music (2).
    fn sample() -> Vec<TreeNode> {
        vec![
            node(
                1,
                "Documents",
                vec![
                    node(11, "Letters", vec![node(111, "draft.txt", Vec::new())]),
                    node(12, "Photos", Vec::new()),
                ],
            ),
            node(2, "Music", Vec::new()),
        ]
    }

    fn tree() -> TreeView {
        let mut tree = TreeView::new(7, "Folders", 20);
        tree.set_bounds(Rect {
            x: 10,
            y: 10,
            width: 200,
            height: 100,
        });
        tree.set_roots(sample());
        tree.set_focused(true);
        tree
    }

    fn key(key: Key) -> Event {
        Event::KeyPressed {
            key,
            modifiers: Modifiers::default(),
        }
    }

    fn keys(tree: &TreeView) -> Vec<u64> {
        tree.rows().iter().map(|row| row.key).collect()
    }

    fn opaque(color: Color) -> u32 {
        0xff00_0000
            | (u32::from(color.red) << 16)
            | (u32::from(color.green) << 8)
            | u32::from(color.blue)
    }

    #[test]
    fn right_expands_then_enters_the_first_child_and_left_climbs_back() {
        let mut tree = tree();
        assert_eq!(keys(&tree), vec![1, 2]);
        tree.select_key(1);
        assert_eq!(
            tree.handle(&key(Key::Right), Modifiers::default()),
            TreeOutcome::Changed
        );
        assert_eq!(keys(&tree), vec![1, 11, 12, 2]);
        assert_eq!(
            tree.handle(&key(Key::Right), Modifiers::default()),
            TreeOutcome::SelectionChanged
        );
        assert_eq!(tree.cursor_key(), Some(11));
        // Left on a collapsed child moves to its parent.
        assert_eq!(
            tree.handle(&key(Key::Left), Modifiers::default()),
            TreeOutcome::SelectionChanged
        );
        assert_eq!(tree.cursor_key(), Some(1));
        // Left on an expanded row collapses it.
        assert_eq!(
            tree.handle(&key(Key::Left), Modifiers::default()),
            TreeOutcome::Changed
        );
        assert_eq!(keys(&tree), vec![1, 2]);
    }

    #[test]
    fn left_and_right_on_leaves_and_without_focus_are_ignored() {
        let mut tree = tree();
        tree.expand(1);
        tree.select_key(2);
        assert_eq!(
            tree.handle(&key(Key::Right), Modifiers::default()),
            TreeOutcome::Ignored
        );
        assert_eq!(
            tree.handle(&key(Key::Left), Modifiers::default()),
            TreeOutcome::Ignored
        );
        tree.set_focused(false);
        tree.select_key(1);
        assert_eq!(
            tree.handle(&key(Key::Right), Modifiers::default()),
            TreeOutcome::Ignored
        );
        assert!(!tree.is_expanded(1));
    }

    #[test]
    fn disclosure_press_toggles_and_a_press_elsewhere_selects() {
        let mut tree = tree();
        let press = |x, y| Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
        };
        // Depth 0 disclosure zone is x in [10, 24).
        assert_eq!(
            tree.handle(&press(12, 15), Modifiers::default()),
            TreeOutcome::Changed
        );
        assert!(tree.is_expanded(1));
        assert_eq!(keys(&tree), vec![1, 11, 12, 2]);
        assert_eq!(
            tree.handle(&press(60, 15), Modifiers::default()),
            TreeOutcome::SelectionChanged
        );
        assert_eq!(tree.selection_keys(), vec![1]);
        // Depth 1 zone is [24, 38): the Letters row toggles.
        assert_eq!(
            tree.handle(&press(26, 35), Modifiers::default()),
            TreeOutcome::Changed
        );
        assert!(tree.is_expanded(11));
        assert_eq!(keys(&tree), vec![1, 11, 111, 12, 2]);
    }

    #[test]
    fn enter_activates_and_down_walks_the_visible_rows() {
        let mut tree = tree();
        tree.select_key(1);
        assert_eq!(
            tree.handle(&key(Key::Down), Modifiers::default()),
            TreeOutcome::SelectionChanged
        );
        assert_eq!(tree.cursor_key(), Some(2));
        assert_eq!(
            tree.handle(&key(Key::Enter), Modifiers::default()),
            TreeOutcome::Activated(2)
        );
    }

    #[test]
    fn collapsing_moves_a_hidden_selection_to_the_collapsed_row() {
        let mut tree = tree();
        tree.expand(1);
        tree.expand(11);
        tree.select_key(111);
        assert!(tree.collapse(1));
        assert_eq!(keys(&tree), vec![1, 2]);
        assert_eq!(tree.selection_keys(), vec![1]);
        assert!(tree.is_expanded(11), "the nested expansion is remembered");
    }

    #[test]
    fn expanding_unknown_or_leaf_rows_does_nothing() {
        let mut tree = tree();
        assert!(!tree.expand(999));
        assert!(!tree.expand(2), "Music has no children");
        assert!(tree.expand(1));
        assert!(!tree.expand(1), "already expanded");
    }

    #[test]
    fn accessibility_marks_level_expansion_and_selection() {
        let mut tree = tree();
        tree.expand(1);
        tree.select_key(11);
        let nodes = tree.accessibility_nodes();
        assert_eq!(nodes[0].1.role(), accesskit::Role::Tree);
        assert_eq!(nodes.len(), 1 + tree.rows().len());
        let (id, letters) = nodes
            .iter()
            .find(|(id, _)| *id == tree.row_node_id(1))
            .expect("a node for the Letters row");
        assert_eq!(*id, tree.row_node_id(1));
        assert_eq!(letters.role(), accesskit::Role::TreeItem);
        assert_eq!(letters.label(), Some("Letters"));
        assert_eq!(letters.level(), Some(2));
        assert_eq!(letters.is_expanded(), Some(false));
        assert_eq!(letters.is_selected(), Some(true));
        let (_, music) = nodes
            .iter()
            .find(|(_, node)| node.label() == Some("Music"))
            .expect("a node for Music");
        assert_eq!(music.is_expanded(), None, "leaves carry no expanded state");
        assert_eq!(tree.focus_node(), tree.row_node_id(1));
        assert_eq!(tree.row_for_node(tree.row_node_id(1)), Some(1));
        assert_eq!(tree.row_for_node(tree.id), None);
    }

    /// Paint at `scale` and probe logical points through the device pixel
    /// at their top-left corner.
    fn painted(scale: u32, tree: &TreeView) -> RasterSurface {
        let surface = RasterSurface::new(240, 120, Scale::new(scale).unwrap()).unwrap();
        surface.clear(Color::WHITE).unwrap();
        tree.paint(&surface, |_, _, _, _| Ok(())).unwrap();
        surface
    }

    #[test]
    fn selected_row_fill_and_disclosure_glyph_have_reference_pixels_at_1x_and_2x() {
        for scale in [1, 2] {
            let mut tree = tree();
            tree.select_key(2);
            let surface = painted(scale, &tree);
            let at = |x: i32, y: i32| (x as u32 * scale, y as u32 * scale);
            // Row 1 (Music) spans y 30..50 and is selected: probe clear of text.
            let (px, py) = at(150, 40);
            assert_eq!(
                surface.pixel(px, py),
                Some(opaque(palette().selection)),
                "scale {scale}"
            );
            // Row 0 (Documents) is collapsed: the plus's vertical arm is ink.
            let (px, py) = at(17, 17);
            assert_eq!(
                surface.pixel(px, py),
                Some(opaque(palette().ink_secondary)),
                "scale {scale}"
            );

            tree.expand(1);
            let surface = painted(scale, &tree);
            // Expanded: the minus has no vertical arm at that point.
            let (px, py) = at(17, 17);
            assert_eq!(
                surface.pixel(px, py),
                Some(opaque(palette().content)),
                "scale {scale}"
            );
            // The minus's horizontal arm is drawn at the box's middle row.
            let (px, py) = at(15, 19);
            assert_eq!(
                surface.pixel(px, py),
                Some(opaque(palette().ink_secondary)),
                "scale {scale}"
            );
        }
    }

    #[test]
    fn malformed_geometry_and_empty_trees_never_fail() {
        let surface = RasterSurface::new(240, 120, Scale::new(1).unwrap()).unwrap();
        let mut tree = tree();
        tree.set_bounds(Rect {
            x: 10,
            y: 10,
            width: 0,
            height: 0,
        });
        tree.paint(&surface, |_, _, _, _| Ok(())).unwrap();
        assert_eq!(tree.focus_node(), tree.id);

        let mut empty = TreeView::new(8, "Empty", 0);
        empty.set_focused(true);
        empty.set_roots(Vec::new());
        assert!(empty.rows().is_empty());
        assert_eq!(
            empty.handle(&key(Key::Down), Modifiers::default()),
            TreeOutcome::Ignored
        );
        assert_eq!(
            empty.handle(&key(Key::Right), Modifiers::default()),
            TreeOutcome::Ignored
        );
        assert!(!empty.expand(1));
        empty.paint(&surface, |_, _, _, _| Ok(())).unwrap();
        assert!(!empty.select_key(1));
    }

    #[test]
    fn deep_chains_flatten_without_recursion_and_paint_clipped() {
        let mut chain = node(200, "leaf", Vec::new());
        for key in (0..200).rev() {
            chain = node(key, "level", vec![chain]);
        }
        let mut tree = TreeView::new(9, "Deep", 20);
        tree.set_bounds(Rect {
            x: 0,
            y: 0,
            width: 200,
            height: 100,
        });
        tree.set_roots(vec![chain]);
        for key in 0..200 {
            assert!(tree.expand(key));
        }
        assert_eq!(tree.rows().len(), 201);
        assert_eq!(tree.rows().last().map(|row| row.depth), Some(200));
        let surface = RasterSurface::new(200, 100, Scale::new(1).unwrap()).unwrap();
        tree.select_key(200);
        tree.paint(&surface, |_, _, _, _| Ok(())).unwrap();
    }

    #[test]
    fn flattening_is_bounded_by_the_row_limit() {
        let roots: Vec<TreeNode> = (0..MAX_TREE_ROWS as u64 + 500)
            .map(|key| node(key, "", Vec::new()))
            .collect();
        let mut tree = TreeView::new(10, "Many", 20);
        tree.set_roots(roots);
        assert_eq!(tree.rows().len(), MAX_TREE_ROWS);
    }
}
