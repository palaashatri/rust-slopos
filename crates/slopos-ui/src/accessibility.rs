//! Helpers for building an app's AccessKit tree.

use accesskit::{Node, NodeId, Role, TreeId, TreeInfo, TreeUpdate};

/// Id of the window node created by [`window_tree`]; control ids must differ.
pub const WINDOW_NODE: u64 = 0;

/// A flat window tree: a `Window` node labelled `title` whose children are
/// `nodes` in order. Focus goes to `focus`, or to the window when `None` or
/// when `focus` is not one of the nodes.
pub fn window_tree(title: &str, nodes: Vec<(u64, Node)>, focus: Option<u64>) -> TreeUpdate {
    let mut root = Node::new(Role::Window);
    root.set_label(title);
    root.set_children(nodes.iter().map(|(id, _)| NodeId(*id)).collect::<Vec<_>>());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_falls_back_to_the_window() {
        let tree = window_tree("Files", vec![(5, Node::new(Role::Button))], Some(9));
        assert_eq!(tree.focus, NodeId(WINDOW_NODE));
        assert_eq!(tree.nodes.len(), 2);
        assert_eq!(tree.nodes[0].1.children(), &[NodeId(5)]);
        let tree = window_tree("Files", vec![(5, Node::new(Role::Button))], Some(5));
        assert_eq!(tree.focus, NodeId(5));
    }
}
