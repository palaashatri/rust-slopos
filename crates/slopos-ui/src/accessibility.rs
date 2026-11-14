//! Helpers for building an app's AccessKit tree.

use accesskit::{Node, NodeId, Role, TreeId, TreeInfo, TreeUpdate};

/// Id of the window node created by [`window_tree`]; control ids must differ.
pub const WINDOW_NODE: u64 = 0;

/// A window tree: a `Window` node labelled `title` whose children are the
/// `nodes` no other node lists as a child, in order (so a list can carry its
/// rows). Focus goes to `focus`, or to the window when `None` or when
/// `focus` is not one of the nodes.
pub fn window_tree(title: &str, nodes: Vec<(u64, Node)>, focus: Option<u64>) -> TreeUpdate {
    let nested: std::collections::HashSet<NodeId> = nodes
        .iter()
        .flat_map(|(_, node)| node.children().iter().copied())
        .collect();
    let mut root = Node::new(Role::Window);
    root.set_label(title);
    root.set_children(
        nodes
            .iter()
            .map(|(id, _)| NodeId(*id))
            .filter(|id| !nested.contains(id))
            .collect::<Vec<_>>(),
    );
    let focus = focus
        .filter(|id| nodes.iter().any(|(other, _)| other == id))
        .unwrap_or(WINDOW_NODE);
    let mut all = Vec::with_capacity(nodes.len() + 1);
    all.push((NodeId(WINDOW_NODE), root));
    all.extend(nodes.into_iter().map(|(id, node)| (NodeId(id), node)));
    TreeUpdate {
        nodes: all,
        tree: Some(TreeInfo::new(NodeId(WINDOW_NODE))),
        tree_id: TreeId::ROOT,
        focus: NodeId(focus),
    }
}

/// A text label, such as a status line or Calculator's result.
///
/// AccessKit takes a `Label` node's accessible name and text from its
/// *value*, not its `label` property: a `Label` with only `set_label` reaches
/// AT-SPI with an empty name and no text, so screen readers say nothing.
pub fn text_label(text: impl Into<String>) -> Node {
    let mut node = Node::new(Role::Label);
    node.set_value(text);
    node
}

/// Maps a window's trees, built in logical pixels, onto a window drawn at
/// `factor`.
///
/// Apps describe bounds in the same logical units they paint in. The host
/// renders at an integer scale, so assistive technology must see every
/// rectangle multiplied by it; otherwise a screen reader or magnifier points
/// at the wrong control at 2x. AccessKit applies a node's transform to all
/// of its descendants, so one scale on the tree's root is enough.
///
/// The root is known from the last update that carried `TreeInfo`; AccessKit
/// lets a later update omit it while still replacing the root node, and that
/// replacement must keep the scale. Updates that leave the root out are not
/// changed.
#[derive(Debug, Default)]
pub struct TreeScaler {
    root: Option<NodeId>,
}

impl TreeScaler {
    pub fn scale(&mut self, tree: &mut TreeUpdate, factor: u32) {
        if let Some(info) = &tree.tree {
            self.root = Some(info.root);
        }
        let Some(root) = self.root else {
            return;
        };
        let Some((_, node)) = tree.nodes.iter_mut().find(|(id, _)| *id == root) else {
            return;
        };
        let scale = accesskit::Affine::scale(f64::from(factor));
        let transform = node.transform().map_or(scale, |existing| scale * *existing);
        if transform == accesskit::Affine::IDENTITY {
            node.clear_transform();
        } else {
            node.set_transform(transform);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_scaler_scales_from_the_root() {
        let mut tree = window_tree("Calculator", vec![(5, Node::new(Role::Button))], None);
        TreeScaler::default().scale(&mut tree, 2);
        assert_eq!(
            tree.nodes[0].1.transform(),
            Some(&accesskit::Affine::scale(2.0))
        );
        assert_eq!(tree.nodes[1].1.transform(), None);

        let mut tree = window_tree("Calculator", vec![], None);
        TreeScaler::default().scale(&mut tree, 1);
        assert_eq!(tree.nodes[0].1.transform(), None);

        // An app transform (here a translation) is kept, under the scale.
        let mut tree = window_tree("Calculator", vec![], None);
        tree.nodes[0]
            .1
            .set_transform(accesskit::Affine::translate((3.0, 4.0)));
        TreeScaler::default().scale(&mut tree, 2);
        let combined = *tree.nodes[0].1.transform().unwrap();
        assert_eq!(
            combined * accesskit::Point::new(1.0, 1.0),
            accesskit::Point::new(8.0, 10.0)
        );
    }

    #[test]
    fn tree_scaler_keeps_the_scale_on_a_replacement_root() {
        let mut scaler = TreeScaler::default();
        // Before any full tree the root is unknown, so nothing is touched.
        let mut early = window_tree("Calculator", vec![], None);
        early.tree = None;
        scaler.scale(&mut early, 2);
        assert_eq!(early.nodes[0].1.transform(), None);

        let mut full = window_tree("Calculator", vec![(5, Node::new(Role::Button))], None);
        scaler.scale(&mut full, 2);
        assert_eq!(
            full.nodes[0].1.transform(),
            Some(&accesskit::Affine::scale(2.0))
        );

        // A later update that replaces the root without TreeInfo keeps the scale.
        let mut replacement = window_tree("Calculator", vec![(5, Node::new(Role::Button))], None);
        replacement.tree = None;
        scaler.scale(&mut replacement, 2);
        assert_eq!(
            replacement.nodes[0].1.transform(),
            Some(&accesskit::Affine::scale(2.0))
        );

        // An update with only a child leaves that child alone.
        let mut child_only = TreeUpdate {
            nodes: vec![(NodeId(5), Node::new(Role::Button))],
            tree: None,
            tree_id: TreeId::ROOT,
            focus: NodeId(5),
        };
        scaler.scale(&mut child_only, 2);
        assert_eq!(child_only.nodes[0].1.transform(), None);
    }

    #[test]
    fn text_labels_carry_their_text_as_the_value() {
        let node = text_label("Result 42");
        assert_eq!(node.role(), Role::Label);
        assert_eq!(node.value(), Some("Result 42"));
    }

    #[test]
    fn focus_falls_back_to_the_window() {
        let tree = window_tree("Files", vec![(5, Node::new(Role::Button))], Some(9));
        assert_eq!(tree.focus, NodeId(WINDOW_NODE));
        assert_eq!(tree.nodes.len(), 2);
        assert_eq!(tree.nodes[0].1.children(), &[NodeId(5)]);
        let tree = window_tree("Files", vec![(5, Node::new(Role::Button))], Some(5));
        assert_eq!(tree.focus, NodeId(5));
        let mut list = Node::new(Role::ListBox);
        list.set_children(vec![NodeId(7)]);
        let tree = window_tree(
            "Files",
            vec![(6, list), (7, Node::new(Role::ListBoxOption))],
            Some(7),
        );
        assert_eq!(tree.nodes[0].1.children(), &[NodeId(6)]);
        assert_eq!(tree.focus, NodeId(7));
    }
}
