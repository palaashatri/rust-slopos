//! Pixel-exact software rendering for SLOPOS components.
//!
//! A [`RasterSurface`] is a premultiplied RGBA pixel buffer addressed in
//! *logical* pixels. Its integer [`Scale`] maps every logical pixel to an exact
//! `scale × scale` block of device pixels, so Classic geometry stays exact at
//! 1× and 2×. Rectangles are half-open and never anti-aliased.

use std::cell::{Ref, RefCell};

use tiny_skia::{FilterQuality, IntSize, Paint, Pixmap, PixmapPaint, Transform};

use crate::foundation::{
    color::Color,
    geometry::{Rect, Scale},
};
use crate::image::Image;
use crate::text::{self, Font};

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("surface dimensions must be nonzero")]
    InvalidDimensions,
    #[error("scaled geometry exceeds the raster coordinate range")]
    GeometryOverflow,
    #[error("could not encode PNG: {0}")]
    Encode(String),
}

pub struct RasterSurface {
    pixmap: RefCell<Pixmap>,
    scale: Scale,
    logical_width: u32,
    logical_height: u32,
}

impl RasterSurface {
    pub fn new(width: u32, height: u32, scale: Scale) -> Result<Self, RenderError> {
        if width == 0 || height == 0 {
            return Err(RenderError::InvalidDimensions);
        }
        let device = Rect {
            x: 0,
            y: 0,
            width,
            height,
        }
        .to_device(scale)
        .ok_or(RenderError::GeometryOverflow)?;
        // Keep device sizes within i32 so every coordinate stays representable.
        if device.width > i32::MAX as u32 || device.height > i32::MAX as u32 {
            return Err(RenderError::GeometryOverflow);
        }
        let pixmap =
            Pixmap::new(device.width, device.height).ok_or(RenderError::GeometryOverflow)?;
        Ok(Self {
            pixmap: RefCell::new(pixmap),
            scale,
            logical_width: width,
            logical_height: height,
        })
    }

    pub fn scale(&self) -> Scale {
        self.scale
    }

    pub fn logical_width(&self) -> u32 {
        self.logical_width
    }

    pub fn logical_height(&self) -> u32 {
        self.logical_height
    }

    pub fn device_width(&self) -> u32 {
        self.pixmap.borrow().width()
    }

    pub fn device_height(&self) -> u32 {
        self.pixmap.borrow().height()
    }

    /// Replace every pixel, including alpha.
    pub fn clear(&self, color: Color) -> Result<(), RenderError> {
        self.pixmap.borrow_mut().fill(to_skia(color));
        Ok(())
    }

    /// Fill a logical rectangle (source-over, no anti-aliasing). Parts outside
    /// the surface are clipped.
    pub fn fill_rect(&self, rect: Rect, color: Color) -> Result<(), RenderError> {
        let rect = rect
            .to_device(self.scale)
            .ok_or(RenderError::GeometryOverflow)?;
        self.fill_device_rect(rect, color);
        Ok(())
    }

    fn fill_device_rect(&self, rect: Rect, color: Color) {
        let mut pixmap = self.pixmap.borrow_mut();
        let (width, height) = (pixmap.width() as i64, pixmap.height() as i64);
        let left = i64::from(rect.x).clamp(0, width);
        let top = i64::from(rect.y).clamp(0, height);
        let right = (i64::from(rect.x) + i64::from(rect.width)).clamp(0, width);
        let bottom = (i64::from(rect.y) + i64::from(rect.height)).clamp(0, height);
        if left >= right || top >= bottom {
            return;
        }
        let Some(area) =
            tiny_skia::Rect::from_ltrb(left as f32, top as f32, right as f32, bottom as f32)
        else {
            return;
        };
        let mut paint = Paint::default();
        paint.set_color(to_skia(color));
        paint.anti_alias = false;
        pixmap.fill_rect(area, &paint, Transform::identity(), None);
    }

    /// Draw a 1-logical-pixel outline just inside `rect`.
    pub fn stroke_rect(&self, rect: Rect, color: Color) -> Result<(), RenderError> {
        if rect.width == 0 || rect.height == 0 {
            return Ok(());
        }
        let edge = |x: i32, y: i32, width: u32, height: u32| Rect {
            x,
            y,
            width,
            height,
        };
        let right = rect
            .x
            .checked_add(rect.width as i32 - 1)
            .ok_or(RenderError::GeometryOverflow)?;
        let bottom = rect
            .y
            .checked_add(rect.height as i32 - 1)
            .ok_or(RenderError::GeometryOverflow)?;
        self.fill_rect(edge(rect.x, rect.y, rect.width, 1), color)?;
        self.fill_rect(edge(rect.x, bottom, rect.width, 1), color)?;
        self.fill_rect(edge(rect.x, rect.y, 1, rect.height), color)?;
        self.fill_rect(edge(right, rect.y, 1, rect.height), color)
    }

    /// Composite another surface at a logical origin with nearest-neighbour
    /// scaling, so pixel art never blurs.
    pub fn draw_surface(
        &self,
        source: &RasterSurface,
        origin_x: i32,
        origin_y: i32,
    ) -> Result<(), RenderError> {
        let ratio = self.scale.factor() as f32 / source.scale.factor() as f32;
        let x = origin_x
            .checked_mul(self.scale.factor() as i32)
            .ok_or(RenderError::GeometryOverflow)?;
        let y = origin_y
            .checked_mul(self.scale.factor() as i32)
            .ok_or(RenderError::GeometryOverflow)?;
        let paint = PixmapPaint {
            quality: FilterQuality::Nearest,
            ..PixmapPaint::default()
        };
        let source = source.pixmap.borrow();
        self.pixmap.borrow_mut().draw_pixmap(
            0,
            0,
            source.as_ref(),
            &paint,
            Transform::from_row(ratio, 0.0, 0.0, ratio, x as f32, y as f32),
            None,
        );
        Ok(())
    }

    /// Draw an image at a logical origin, one image pixel per logical pixel.
    pub fn draw_image(
        &self,
        image: &Image,
        origin_x: i32,
        origin_y: i32,
    ) -> Result<(), RenderError> {
        self.draw_image_scaled(image, origin_x, origin_y, image.width(), image.height())
    }

    /// Draw an image into a logical box. Integer scale factors use nearest
    /// neighbour (pixel art stays crisp); other sizes use bicubic filtering.
    pub fn draw_image_scaled(
        &self,
        image: &Image,
        origin_x: i32,
        origin_y: i32,
        width: u32,
        height: u32,
    ) -> Result<(), RenderError> {
        let Some(source) = image.pixmap() else {
            return Ok(());
        };
        let factor = self.scale.factor() as f32;
        let sx = width as f32 * factor / image.width() as f32;
        let sy = height as f32 * factor / image.height() as f32;
        let integral = sx.fract() == 0.0 && sy.fract() == 0.0;
        let paint = PixmapPaint {
            quality: if integral {
                FilterQuality::Nearest
            } else {
                FilterQuality::Bicubic
            },
            ..PixmapPaint::default()
        };
        let x = origin_x as f32 * factor;
        let y = origin_y as f32 * factor;
        self.pixmap.borrow_mut().draw_pixmap(
            0,
            0,
            source.as_ref(),
            &paint,
            Transform::from_row(sx, 0.0, 0.0, sy, x, y),
            None,
        );
        Ok(())
    }

    /// Draw text with its top-left corner at a logical position.
    pub fn draw_text(
        &self,
        content: &str,
        x: i32,
        y: i32,
        font: &Font,
        color: Color,
    ) -> Result<(), RenderError> {
        let factor = self.scale.factor() as i32;
        let origin_x = x.checked_mul(factor).ok_or(RenderError::GeometryOverflow)?;
        let origin_y = y.checked_mul(factor).ok_or(RenderError::GeometryOverflow)?;
        let mut pixmap = self.pixmap.borrow_mut();
        let (width, height) = (pixmap.width() as i32, pixmap.height() as i32);
        let data = pixmap.data_mut();
        text::rasterize(
            content,
            font,
            self.scale.factor() as f32,
            color,
            |px, py, coverage_color| {
                let (dx, dy) = (origin_x + px, origin_y + py);
                if dx < 0 || dy < 0 || dx >= width || dy >= height {
                    return;
                }
                let offset = ((dy * width + dx) * 4) as usize;
                blend_straight_over(&mut data[offset..offset + 4], coverage_color);
            },
        );
        Ok(())
    }

    /// Premultiplied pixel at device coordinates as 0xAARRGGBB.
    pub fn pixel(&self, device_x: u32, device_y: u32) -> Option<u32> {
        let pixmap = self.pixmap.borrow();
        let pixel = pixmap.pixel(device_x, device_y)?;
        Some(
            (u32::from(pixel.alpha()) << 24)
                | (u32::from(pixel.red()) << 16)
                | (u32::from(pixel.green()) << 8)
                | u32::from(pixel.blue()),
        )
    }

    /// Premultiplied RGBA bytes, row-major, no padding.
    pub fn data(&self) -> Ref<'_, [u8]> {
        Ref::map(self.pixmap.borrow(), |pixmap| pixmap.data())
    }

    /// Encode the device pixels as a PNG (for snapshots and evidence).
    pub fn encode_png(&self) -> Result<Vec<u8>, RenderError> {
        self.pixmap
            .borrow()
            .encode_png()
            .map_err(|error| RenderError::Encode(error.to_string()))
    }

    pub fn save_png(&self, path: &std::path::Path) -> Result<(), RenderError> {
        let bytes = self.encode_png()?;
        std::fs::write(path, bytes).map_err(|error| RenderError::Encode(error.to_string()))
    }
}

pub(crate) fn int_size(width: u32, height: u32) -> Option<IntSize> {
    IntSize::from_wh(width, height)
}

fn to_skia(color: Color) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba8(color.red, color.green, color.blue, color.alpha)
}

/// Blend a straight-alpha colour over a premultiplied RGBA pixel.
fn blend_straight_over(destination: &mut [u8], color: Color) {
    let alpha = u32::from(color.alpha);
    if alpha == 0 {
        return;
    }
    let inverse = 255 - alpha;
    let channel = |source: u8, dest: u8| -> u8 {
        ((u32::from(source) * alpha + u32::from(dest) * inverse + 127) / 255) as u8
    };
    destination[0] = channel(color.red, destination[0]);
    destination[1] = channel(color.green, destination[1]);
    destination[2] = channel(color.blue, destination[2]);
    destination[3] = ((alpha * 255 + u32::from(destination[3]) * inverse + 127) / 255) as u8;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(surface: &RasterSurface, point_x: u32, point_y: u32) -> u32 {
        surface.pixel(point_x, point_y).unwrap()
    }

    #[test]
    fn drawing_preserves_half_open_edges_at_both_scales() {
        for factor in [1u32, 2] {
            let surface = RasterSurface::new(8, 6, Scale::new(factor).unwrap()).unwrap();
            surface.clear(Color::WHITE).unwrap();
            surface
                .fill_rect(
                    Rect {
                        x: 1,
                        y: 2,
                        width: 3,
                        height: 2,
                    },
                    Color::BLACK,
                )
                .unwrap();
            assert_eq!(pixel(&surface, factor, 2 * factor), 0xff000000);
            assert_eq!(pixel(&surface, 4 * factor - 1, 4 * factor - 1), 0xff000000);
            assert_eq!(pixel(&surface, 4 * factor, 2 * factor), 0xffffffff);
            assert_eq!(pixel(&surface, factor, 4 * factor), 0xffffffff);
            assert_eq!(pixel(&surface, factor - 1, 2 * factor), 0xffffffff);
        }
    }

    #[test]
    fn drawing_clips_without_wrapping_negative_coordinates() {
        let surface = RasterSurface::new(4, 4, Scale::new(1).unwrap()).unwrap();
        surface.clear(Color::WHITE).unwrap();
        surface
            .fill_rect(
                Rect {
                    x: -1,
                    y: -1,
                    width: 2,
                    height: 2,
                },
                Color::BLACK,
            )
            .unwrap();
        assert_eq!(pixel(&surface, 0, 0), 0xff000000);
        assert_eq!(pixel(&surface, 1, 0), 0xffffffff);
        surface.clear(Color::TRANSPARENT).unwrap();
        assert_eq!(pixel(&surface, 0, 0), 0);
    }

    #[test]
    fn surface_composition_preserves_logical_position_and_nearest_pixels() {
        let source = RasterSurface::new(3, 2, Scale::new(2).unwrap()).unwrap();
        source.clear(Color::WHITE).unwrap();
        source
            .fill_rect(
                Rect {
                    x: 1,
                    y: 0,
                    width: 1,
                    height: 1,
                },
                Color::BLACK,
            )
            .unwrap();

        for factor in [1u32, 2] {
            let target = RasterSurface::new(10, 5, Scale::new(factor).unwrap()).unwrap();
            target.clear(Color::TRANSPARENT).unwrap();
            target.draw_surface(&source, 2, 1).unwrap();
            assert_eq!(pixel(&target, 2 * factor, factor), 0xffffffff);
            assert_eq!(pixel(&target, 3 * factor, factor), 0xff000000);
            assert_eq!(pixel(&target, 5 * factor - 1, 3 * factor - 1), 0xffffffff);
            assert_eq!(pixel(&target, 5 * factor, factor), 0);
        }
    }

    #[test]
    fn images_keep_integer_scale_and_logical_origin() {
        let image = Image::from_rgba(1, 1, vec![255, 0, 0, 255]).unwrap();
        for factor in [1u32, 2] {
            let surface = RasterSurface::new(4, 3, Scale::new(factor).unwrap()).unwrap();
            surface.clear(Color::TRANSPARENT).unwrap();
            surface.draw_image(&image, 1, 1).unwrap();
            assert_eq!(pixel(&surface, factor, factor), 0xffff0000);
            assert_eq!(pixel(&surface, 2 * factor - 1, 2 * factor - 1), 0xffff0000);
            assert_eq!(pixel(&surface, 2 * factor, factor), 0);
            assert_eq!(pixel(&surface, factor - 1, factor), 0);
        }
    }

    #[test]
    fn intermediate_alpha_blends_over_opaque_pixels() {
        let half_red = Color {
            red: 255,
            green: 0,
            blue: 0,
            alpha: 128,
        };
        let scale = Scale::new(1).unwrap();
        let source = RasterSurface::new(2, 1, scale).unwrap();
        source.clear(Color::TRANSPARENT).unwrap();
        source
            .fill_rect(
                Rect {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1,
                },
                half_red,
            )
            .unwrap();
        let target = RasterSurface::new(2, 1, scale).unwrap();
        target.clear(Color::WHITE).unwrap();
        target.draw_surface(&source, 0, 0).unwrap();
        assert_eq!(pixel(&target, 0, 0), 0xffff7f7f);
        assert_eq!(pixel(&target, 1, 0), 0xffffffff);

        target.clear(Color::WHITE).unwrap();
        target
            .fill_rect(
                Rect {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1,
                },
                half_red,
            )
            .unwrap();
        assert_eq!(pixel(&target, 0, 0), 0xffff7f7f);
    }

    #[test]
    fn invalid_dimensions_and_scaled_overflow_are_errors() {
        let scale = Scale::new(2).unwrap();
        assert!(RasterSurface::new(0, 10, scale).is_err());
        assert!(RasterSurface::new(10, 0, scale).is_err());
        assert!(RasterSurface::new(u32::MAX, 1, scale).is_err());
        assert!(RasterSurface::new(i32::MAX as u32, 1, scale).is_err());
        let surface = RasterSurface::new(4, 4, scale).unwrap();
        assert!(surface
            .fill_rect(
                Rect {
                    x: i32::MAX,
                    y: 0,
                    width: 1,
                    height: 1
                },
                Color::BLACK
            )
            .is_err());
    }

    #[test]
    fn outlines_are_one_logical_pixel_inside_the_rect() {
        let surface = RasterSurface::new(5, 4, Scale::new(2).unwrap()).unwrap();
        surface.clear(Color::WHITE).unwrap();
        surface
            .stroke_rect(
                Rect {
                    x: 0,
                    y: 0,
                    width: 5,
                    height: 4,
                },
                Color::BLACK,
            )
            .unwrap();
        assert_eq!(pixel(&surface, 0, 0), 0xff000000);
        assert_eq!(pixel(&surface, 9, 7), 0xff000000);
        assert_eq!(pixel(&surface, 2, 2), 0xffffffff);
    }

    #[test]
    fn text_is_visible_at_integer_scales() {
        for factor in [1usize, 2] {
            let surface = RasterSurface::new(48, 24, Scale::new(factor as u32).unwrap()).unwrap();
            surface.clear(Color::WHITE).unwrap();
            surface
                .draw_text("SlopOS", 2, 2, &Font::system(12.0), Color::BLACK)
                .unwrap();
            let data = surface.data();
            let dark = data
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|pixel| pixel[0] < 128)
                .count();
            assert!(dark > 10, "text left only {dark} dark pixels");
            assert!(dark < 48 * 24 * factor * factor / 2);
        }
    }

    #[test]
    fn png_snapshots_round_trip_dimensions() {
        let surface = RasterSurface::new(3, 2, Scale::new(2).unwrap()).unwrap();
        surface.clear(Color::BLACK).unwrap();
        let png = surface.encode_png().unwrap();
        let decoded = Image::from_png(&png).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (6, 4));
    }

    #[test]
    fn button_plates_render_the_extracted_shapes_at_integer_scales() {
        use crate::primitives::button_plate::{
            paint_button_plate, ButtonPlateState, ButtonPlateVariant,
        };

        let scale = Scale::new(1).unwrap();
        let surface = RasterSurface::new(88, 28, scale).unwrap();
        surface.clear(Color::TRANSPARENT).unwrap();
        paint_button_plate(
            &surface,
            0,
            0,
            ButtonPlateVariant::Regular,
            ButtonPlateState::Rest,
        )
        .unwrap();
        assert_eq!(pixel(&surface, 2, 0), 0);
        assert_eq!(pixel(&surface, 3, 0), 0xff000000);
        assert_eq!(pixel(&surface, 0, 3), 0xff000000);
        assert_eq!(pixel(&surface, 1, 3), 0xffffffff);
        assert_eq!(pixel(&surface, 40, 10), 0xffffffff);

        surface.clear(Color::TRANSPARENT).unwrap();
        paint_button_plate(
            &surface,
            0,
            0,
            ButtonPlateVariant::Regular,
            ButtonPlateState::Pressed,
        )
        .unwrap();
        assert_eq!(pixel(&surface, 40, 10), 0xff000000);

        surface.clear(Color::TRANSPARENT).unwrap();
        paint_button_plate(
            &surface,
            0,
            0,
            ButtonPlateVariant::Default,
            ButtonPlateState::Rest,
        )
        .unwrap();
        assert_eq!(pixel(&surface, 5, 0), 0);
        assert_eq!(pixel(&surface, 6, 0), 0xff000000);
        assert_eq!(pixel(&surface, 6, 4), 0xffffffff);
        assert_eq!(pixel(&surface, 7, 4), 0xff000000);
        assert_eq!(pixel(&surface, 4, 10), 0xff000000);
        assert_eq!(pixel(&surface, 3, 10), 0xffffffff);
        assert_eq!(pixel(&surface, 20, 10), 0xffffffff);

        let high_density = RasterSurface::new(176, 56, Scale::new(2).unwrap()).unwrap();
        high_density.clear(Color::TRANSPARENT).unwrap();
        paint_button_plate(
            &high_density,
            0,
            0,
            ButtonPlateVariant::Default,
            ButtonPlateState::Pressed,
        )
        .unwrap();
        assert_eq!(pixel(&high_density, 7, 20), 0xffffffff);
        assert_eq!(pixel(&high_density, 10, 20), 0xff000000);
    }
}
