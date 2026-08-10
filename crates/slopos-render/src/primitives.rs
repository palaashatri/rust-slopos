use crate::{Color, Renderer};

#[derive(Debug, Clone, PartialEq)]
pub enum DrawCommand {
    Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: Color,
        corner_radius: f32,
    },
    Text {
        text: String,
        x: f32,
        y: f32,
        font_size: f32,
        color: Color,
    },
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        color: Color,
        thickness: f32,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusRingStyle {
    Inner,
    Outer,
    Classic,
    Mask,
}

pub fn draw_rect(
    renderer: &Renderer,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: Color,
    corner_radius: f32,
) {
    renderer.enqueue(DrawCommand::Rect {
        x,
        y,
        width,
        height,
        color,
        corner_radius,
    });
}

pub fn draw_text(renderer: &Renderer, text: &str, x: f32, y: f32, font_size: f32, color: Color) {
    renderer.enqueue(DrawCommand::Text {
        text: text.to_string(),
        x,
        y,
        font_size,
        color,
    });
}

pub fn draw_line(
    renderer: &Renderer,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    color: Color,
    thickness: f32,
) {
    renderer.enqueue(DrawCommand::Line {
        x1,
        y1,
        x2,
        y2,
        color,
        thickness,
    });
}

pub fn draw_image(renderer: &Renderer, x: f32, y: f32, width: f32, height: f32, texture_id: u32) {
    renderer.enqueue(DrawCommand::Image {
        x,
        y,
        width,
        height,
        texture_id,
    });
}

#[allow(clippy::too_many_arguments)]
pub fn draw_focus_ring(
    renderer: &Renderer,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    thickness: f32,
    color: Color,
    style: FocusRingStyle,
) {
    renderer.enqueue(DrawCommand::FocusRing {
        x,
        y,
        width,
        height,
        thickness,
        color,
        style,
    });
}
