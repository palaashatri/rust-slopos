//! Adapter from the renderer-neutral command stream to the existing
//! pixel-exact SLOPOS software renderer.

use crate::render::{RasterSurface, RenderError};
use crate::scene::{Command, Renderer, Scene};

pub struct RasterRenderer<'a> {
    surface: &'a RasterSurface,
}

impl<'a> RasterRenderer<'a> {
    pub fn new(surface: &'a RasterSurface) -> Self {
        Self { surface }
    }
}

impl Renderer for RasterRenderer<'_> {
    type Error = RenderError;

    fn draw(&mut self, command: Command) -> Result<(), Self::Error> {
        match command {
            Command::Clear(color) => self.surface.clear(color),
            Command::FillRect { rect, color } => self.surface.fill_rect(rect, color),
            Command::StrokeRect { rect, color } => self.surface.stroke_rect(rect, color),
        }
    }
}

pub fn replay_to_raster(scene: &Scene, surface: &RasterSurface) -> Result<(), RenderError> {
    scene.replay(&mut RasterRenderer::new(surface))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::{
        color::Color,
        geometry::{Rect, Scale},
    };

    #[test]
    fn scene_matches_reference_pixels_at_integer_scales() {
        for factor in [1u32, 2] {
            let surface = RasterSurface::new(6, 5, Scale::new(factor).unwrap()).unwrap();
            let mut scene = Scene::new(3);
            scene.push(Command::Clear(Color::WHITE)).unwrap();
            scene.push(Command::FillRect {
                rect: Rect { x: 1, y: 1, width: 3, height: 2 },
                color: Color::BLACK,
            }).unwrap();
            replay_to_raster(&scene, &surface).unwrap();
            assert_eq!(surface.pixel(factor, factor), Some(0xff000000));
            assert_eq!(surface.pixel(4 * factor, factor), Some(0xffffffff));
            assert_eq!(surface.pixel(factor, 3 * factor), Some(0xffffffff));
        }
    }

    #[test]
    fn replay_reports_invalid_geometry_without_panicking() {
        let surface = RasterSurface::new(2, 2, Scale::new(2).unwrap()).unwrap();
        let mut scene = Scene::new(1);
        scene.push(Command::FillRect {
            rect: Rect { x: i32::MAX, y: 0, width: 1, height: 1 },
            color: Color::BLACK,
        }).unwrap();
        assert!(matches!(replay_to_raster(&scene, &surface), Err(RenderError::GeometryOverflow)));
    }
}
