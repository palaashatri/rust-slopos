use gtk::cairo::{Antialias, Context, Filter, Format, ImageSurface, Operator};

use crate::foundation::{
    color::Color,
    geometry::{Rect, Scale},
};

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("surface dimensions must be nonzero")]
    InvalidDimensions,
    #[error("scaled geometry exceeds the raster coordinate range")]
    GeometryOverflow,
    #[error(transparent)]
    Cairo(#[from] gtk::cairo::Error),
}

pub struct RasterSurface {
    image: ImageSurface,
    scale: Scale,
}

impl RasterSurface {
    pub fn new(width: u32, height: u32, scale: Scale) -> Result<Self, RenderError> {
        if width == 0 || height == 0 {
            return Err(RenderError::InvalidDimensions);
        }
        let dimensions = Rect {
            x: 0,
            y: 0,
            width,
            height,
        }
        .to_device(scale)
        .ok_or(RenderError::GeometryOverflow)?;
        let width = i32::try_from(dimensions.width).map_err(|_| RenderError::GeometryOverflow)?;
        let height = i32::try_from(dimensions.height).map_err(|_| RenderError::GeometryOverflow)?;
        Ok(Self {
            image: ImageSurface::create(Format::ARgb32, width, height)?,
            scale,
        })
    }

    pub fn clear(&self, color: Color) -> Result<(), RenderError> {
        let context = Context::new(&self.image)?;
        context.set_operator(Operator::Source);
        set_color(&context, color);
        context.paint()?;
        Ok(())
    }

    pub fn fill_rect(&self, rect: Rect, color: Color) -> Result<(), RenderError> {
        let rect = rect
            .to_device(self.scale)
            .ok_or(RenderError::GeometryOverflow)?;
        let context = Context::new(&self.image)?;
        context.set_antialias(Antialias::None);
        set_color(&context, color);
        context.rectangle(
            f64::from(rect.x),
            f64::from(rect.y),
            f64::from(rect.width),
            f64::from(rect.height),
        );
        context.fill()?;
        Ok(())
    }

    pub fn draw_surface(
        &self,
        source: &RasterSurface,
        origin_x: i32,
        origin_y: i32,
    ) -> Result<(), RenderError> {
        let context = Context::new(&self.image)?;
        context.save()?;
        let source_scale = f64::from(source.scale.factor());
        let scale_ratio = f64::from(self.scale.factor()) / source_scale;
        context.scale(scale_ratio, scale_ratio);
        let result = context
            .set_source_surface(
                &source.image,
                f64::from(origin_x) * source_scale,
                f64::from(origin_y) * source_scale,
            )
            .and_then(|()| {
                context.source().set_filter(Filter::Nearest);
                context.paint()
            });
        let restore = context.restore();
        result.and(restore)?;
        Ok(())
    }

    pub fn draw_pixbuf(
        &self,
        pixbuf: &gtk::gdk_pixbuf::Pixbuf,
        origin_x: i32,
        origin_y: i32,
    ) -> Result<(), RenderError> {
        use gtk::gdk::prelude::GdkContextExt;

        let context = Context::new(&self.image)?;
        context.save()?;
        let scale = f64::from(self.scale.factor());
        context.scale(scale, scale);
        context.set_source_pixbuf(pixbuf, f64::from(origin_x), f64::from(origin_y));
        context.source().set_filter(Filter::Nearest);
        let result = context.paint();
        let restore = context.restore();
        result.and(restore)?;
        Ok(())
    }

    pub fn draw_text(
        &self,
        text: &str,
        x: i32,
        y: i32,
        font: &gtk::pango::FontDescription,
        color: Color,
    ) -> Result<(), RenderError> {
        let context = Context::new(&self.image)?;
        context.save()?;
        let scale = f64::from(self.scale.factor());
        context.scale(scale, scale);
        set_color(&context, color);
        context.move_to(f64::from(x), f64::from(y));
        let layout = pangocairo::functions::create_layout(&context);
        layout.set_font_description(Some(font));
        layout.set_text(text);
        pangocairo::functions::show_layout(&context, &layout);
        let result = context.status();
        let restore = context.restore();
        result.and(restore)?;
        Ok(())
    }

    pub(crate) fn paint_to(&self, context: &Context) -> Result<(), RenderError> {
        context.save()?;
        let reciprocal_scale = 1.0 / f64::from(self.scale.factor());
        context.scale(reciprocal_scale, reciprocal_scale);
        let result = context
            .set_source_surface(&self.image, 0.0, 0.0)
            .and_then(|()| {
                context.source().set_filter(Filter::Nearest);
                context.paint()
            });
        let restore = context.restore();
        result.and(restore)?;
        Ok(())
    }
}

fn set_color(context: &Context, color: Color) {
    context.set_source_rgba(
        f64::from(color.red) / 255.0,
        f64::from(color.green) / 255.0,
        f64::from(color.blue) / 255.0,
        f64::from(color.alpha) / 255.0,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(surface: &mut RasterSurface, point_x: usize, point_y: usize) -> u32 {
        let stride = surface.image.stride() as usize;
        let bytes = surface.image.data().unwrap();
        let offset = point_y * stride + point_x * 4;
        u32::from_ne_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    #[test]
    fn drawing_preserves_half_open_edges_at_both_scales() {
        for factor in [1, 2] {
            let mut surface = RasterSurface::new(8, 6, Scale::new(factor).unwrap()).unwrap();
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
            let factor = factor as usize;
            assert_eq!(pixel(&mut surface, factor, 2 * factor), 0xff000000);
            assert_eq!(
                pixel(&mut surface, 4 * factor - 1, 4 * factor - 1),
                0xff000000
            );
            assert_eq!(pixel(&mut surface, 4 * factor, 2 * factor), 0xffffffff);
            assert_eq!(pixel(&mut surface, factor, 4 * factor), 0xffffffff);
            assert_eq!(pixel(&mut surface, factor - 1, 2 * factor), 0xffffffff);
        }
    }

    #[test]
    fn drawing_clips_without_wrapping_negative_coordinates() {
        let mut surface = RasterSurface::new(4, 4, Scale::new(1).unwrap()).unwrap();
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
        assert_eq!(pixel(&mut surface, 0, 0), 0xff000000);
        assert_eq!(pixel(&mut surface, 1, 0), 0xffffffff);
        surface.clear(Color::TRANSPARENT).unwrap();
        assert_eq!(pixel(&mut surface, 0, 0), 0);
    }

    #[test]
    fn composition_keeps_pixel_scale_and_restores_the_target_transform() {
        let source = RasterSurface::new(8, 6, Scale::new(2).unwrap()).unwrap();
        source.clear(Color::WHITE).unwrap();
        source
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
        let mut target = RasterSurface::new(16, 12, Scale::new(1).unwrap()).unwrap();
        let context = Context::new(&target.image).unwrap();
        context.scale(2.0, 2.0);
        source.paint_to(&context).unwrap();
        assert_eq!(context.matrix().xx(), 2.0);
        assert_eq!(context.matrix().yy(), 2.0);
        drop(context);
        assert_eq!(pixel(&mut target, 2, 4), 0xff000000);
        assert_eq!(pixel(&mut target, 7, 7), 0xff000000);
        assert_eq!(pixel(&mut target, 8, 4), 0xffffffff);
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

        for factor in [1, 2] {
            let mut target = RasterSurface::new(10, 5, Scale::new(factor).unwrap()).unwrap();
            target.clear(Color::TRANSPARENT).unwrap();
            target.draw_surface(&source, 2, 1).unwrap();

            let factor = factor as usize;
            assert_eq!(pixel(&mut target, 2 * factor, factor), 0xffffffff);
            assert_eq!(pixel(&mut target, 3 * factor, factor), 0xff000000);
            assert_eq!(
                pixel(&mut target, 5 * factor - 1, 3 * factor - 1),
                0xffffffff
            );
            assert_eq!(pixel(&mut target, 5 * factor, factor), 0);
        }
    }

    #[test]
    fn pixbuf_images_keep_integer_scale_and_logical_origin() {
        let image =
            gtk::gdk_pixbuf::Pixbuf::new(gtk::gdk_pixbuf::Colorspace::Rgb, true, 8, 1, 1).unwrap();
        image.fill(0xff0000ff);

        for factor in [1, 2] {
            let mut surface = RasterSurface::new(4, 3, Scale::new(factor).unwrap()).unwrap();
            surface.clear(Color::TRANSPARENT).unwrap();
            surface.draw_pixbuf(&image, 1, 1).unwrap();

            let factor = factor as usize;
            assert_eq!(pixel(&mut surface, factor, factor), 0xffff0000);
            assert_eq!(
                pixel(&mut surface, 2 * factor - 1, 2 * factor - 1),
                0xffff0000
            );
            assert_eq!(pixel(&mut surface, 2 * factor, factor), 0);
            assert_eq!(pixel(&mut surface, factor - 1, factor), 0);
        }
    }

    #[test]
    fn intermediate_alpha_blends_and_transparent_composition_preserves_pixels() {
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
                Color {
                    red: 255,
                    green: 0,
                    blue: 0,
                    alpha: 128,
                },
            )
            .unwrap();
        let mut target = RasterSurface::new(2, 1, scale).unwrap();
        target.clear(Color::WHITE).unwrap();
        let context = Context::new(&target.image).unwrap();
        source.paint_to(&context).unwrap();
        drop(context);
        assert_eq!(pixel(&mut target, 0, 0), 0xffff7f7f);
        assert_eq!(pixel(&mut target, 1, 0), 0xffffffff);
        target.clear(Color::WHITE).unwrap();
        target
            .fill_rect(
                Rect {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1,
                },
                Color {
                    red: 255,
                    green: 0,
                    blue: 0,
                    alpha: 128,
                },
            )
            .unwrap();
        assert_eq!(pixel(&mut target, 0, 0), 0xffff7f7f);
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
    fn pango_text_is_visible_at_integer_scales() {
        for factor in [1, 2] {
            let scale = Scale::new(factor).unwrap();
            let mut surface = RasterSurface::new(40, 24, scale).unwrap();
            surface.clear(Color::WHITE).unwrap();
            let font = gtk::pango::FontDescription::from_string("Sans 12");
            surface
                .draw_text("SlopOS", 2, 2, &font, Color::BLACK)
                .unwrap();

            let factor = factor as usize;
            let bytes = surface.image.data().unwrap();
            let dark_pixels = bytes
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|pixel| pixel.iter().any(|channel| *channel != 0xff))
                .count();
            assert!(dark_pixels > 0);
            assert!(dark_pixels < (40 * factor) * (24 * factor));
        }
    }

    #[test]
    fn button_plates_render_the_extracted_shapes_at_integer_scales() {
        use crate::primitives::button_plate::{
            paint_button_plate, ButtonPlateState, ButtonPlateVariant,
        };

        let scale = Scale::new(1).unwrap();
        let mut surface = RasterSurface::new(88, 28, scale).unwrap();
        surface.clear(Color::TRANSPARENT).unwrap();
        paint_button_plate(
            &surface,
            0,
            0,
            ButtonPlateVariant::Regular,
            ButtonPlateState::Rest,
        )
        .unwrap();
        assert_eq!(pixel(&mut surface, 2, 0), 0);
        assert_eq!(pixel(&mut surface, 3, 0), 0xff000000);
        assert_eq!(pixel(&mut surface, 0, 3), 0xff000000);
        assert_eq!(pixel(&mut surface, 1, 3), 0xffffffff);
        assert_eq!(pixel(&mut surface, 40, 10), 0xffffffff);

        surface.clear(Color::TRANSPARENT).unwrap();
        paint_button_plate(
            &surface,
            0,
            0,
            ButtonPlateVariant::Regular,
            ButtonPlateState::Pressed,
        )
        .unwrap();
        assert_eq!(pixel(&mut surface, 40, 10), 0xff000000);

        surface.clear(Color::TRANSPARENT).unwrap();
        paint_button_plate(
            &surface,
            0,
            0,
            ButtonPlateVariant::Default,
            ButtonPlateState::Rest,
        )
        .unwrap();
        assert_eq!(pixel(&mut surface, 5, 0), 0);
        assert_eq!(pixel(&mut surface, 6, 0), 0xff000000);
        assert_eq!(pixel(&mut surface, 6, 4), 0xffffffff);
        assert_eq!(pixel(&mut surface, 7, 4), 0xff000000);
        assert_eq!(pixel(&mut surface, 4, 10), 0xff000000);
        assert_eq!(pixel(&mut surface, 3, 10), 0xffffffff);
        assert_eq!(pixel(&mut surface, 20, 10), 0xffffffff);

        let mut high_density = RasterSurface::new(176, 56, Scale::new(2).unwrap()).unwrap();
        high_density.clear(Color::TRANSPARENT).unwrap();
        paint_button_plate(
            &high_density,
            0,
            0,
            ButtonPlateVariant::Default,
            ButtonPlateState::Pressed,
        )
        .unwrap();
        assert_eq!(pixel(&mut high_density, 7, 20), 0xffffffff);
        assert_eq!(pixel(&mut high_density, 10, 20), 0xff000000);
    }
}
