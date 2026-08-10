use slopos_render::{Color, RenderNode, RenderTree};

#[test]
fn test_render_tree() {
    let node = RenderNode::Rect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 100.0,
        color: Color::new(1.0, 0.0, 0.0, 1.0),
        corner_radius: 0.0,
    };

    let tree = RenderTree::new(node);
    match tree.root {
        RenderNode::Rect { width, .. } => assert_eq!(width, 100.0),
        _ => panic!("Expected rect node"),
    }
}
