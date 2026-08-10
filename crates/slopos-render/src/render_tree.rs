use crate::{primitives, Color, FocusRingStyle, Renderer, Surface};

pub enum RenderNode {
    Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: Color,
        corner_radius: f32,
    },
    Text {
        x: f32,
        y: f32,
        text: String,
        font_size: f32,
        color: Color,
    },
    Image {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        texture_id: u32,
    },
    FocusRing {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        thickness: f32,
        color: Color,
        style: FocusRingStyle,
    },
    Group {
        children: Vec<RenderNode>,
    },
}

pub struct RenderTree {
    pub root: RenderNode,
}

impl RenderTree {
    pub fn new(root: RenderNode) -> Self {
        Self { root }
    }

    pub fn draw(&self, renderer: &mut Renderer, surface: &mut Surface) {
        Self::draw_node(&self.root, renderer, surface);
    }

    fn draw_node(node: &RenderNode, renderer: &mut Renderer, surface: &mut Surface) {
        let _surface_extent = surface.extent;
        match node {
            RenderNode::Rect {
                x,
                y,
                width,
                height,
                color,
                corner_radius,
            } => primitives::draw_rect(renderer, *x, *y, *width, *height, *color, *corner_radius),
            RenderNode::Text {
                x,
                y,
                text,
                font_size,
                color,
            } => primitives::draw_text(renderer, text, *x, *y, *font_size, *color),
            RenderNode::Image {
                x,
                y,
                width,
                height,
                texture_id,
            } => primitives::draw_image(renderer, *x, *y, *width, *height, *texture_id),
            RenderNode::FocusRing {
                x,
                y,
                width,
                height,
                thickness,
                color,
                style,
            } => primitives::draw_focus_ring(
                renderer, *x, *y, *width, *height, *thickness, *color, *style,
            ),
            RenderNode::Group { children } => {
                for child in children {
                    Self::draw_node(child, renderer, surface);
                }
            }
        }
    }
}
