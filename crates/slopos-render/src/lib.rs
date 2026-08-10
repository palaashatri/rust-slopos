pub mod event_loop;
pub mod font;
pub mod primitives;
pub mod render_tree;
pub mod renderer;
pub mod shader;
pub mod surface;
pub mod texture;
pub mod theme_renderer;
pub mod windowing;

pub use event_loop::{RetroAppHandler, RetroEventLoop};
pub use font::{rasterize_char, RasterGlyph, RetroFont};
pub use primitives::{DrawCommand, FocusRingStyle};
pub use render_tree::{RenderNode, RenderTree};
pub use renderer::{select_present_mode, select_surface_format, DisplayRenderPolicy, Renderer};
pub use shader::Shader;
pub use surface::Surface;
pub use texture::Texture;
pub use theme_renderer::ThemeRenderer;
pub use windowing::WindowHandle;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, RenderError>;

#[derive(Error, Debug)]
pub enum RenderError {
    #[error("wgpu error: {0}")]
    Wgpu(#[from] wgpu::RequestDeviceError),
    #[error("surface error: {0}")]
    Surface(String),
    #[error("shader compilation error: {0}")]
    Shader(String),
    #[error("texture error: {0}")]
    Texture(String),
    #[error("font error: {0}")]
    Font(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Extent2D {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const BLACK: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    pub const WHITE: Color = Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
    pub const TRANSPARENT: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };

    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub fn to_linear(&self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    pub fn to_u8(&self) -> [u8; 4] {
        [
            (self.r.clamp(0.0, 1.0) * 255.0) as u8,
            (self.g.clamp(0.0, 1.0) * 255.0) as u8,
            (self.b.clamp(0.0, 1.0) * 255.0) as u8,
            (self.a.clamp(0.0, 1.0) * 255.0) as u8,
        ]
    }
}

pub trait Renderable {
    fn draw(&self, renderer: &mut Renderer, surface: &mut Surface);
    fn extent(&self) -> Extent2D;
    fn set_position(&mut self, x: f32, y: f32);
}
