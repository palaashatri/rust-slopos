//! Decoded images: PNG files and rasterised SVG icons.

use std::path::Path;

use resvg::usvg;
use tiny_skia::{Pixmap, PremultipliedColorU8, Transform};

use crate::render::int_size;

#[derive(Debug, thiserror::Error)]
pub enum ImageError {
    #[error("could not read {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("invalid PNG: {0}")]
    Png(String),
    #[error("invalid SVG: {0}")]
    Svg(String),
    #[error("image dimensions must be nonzero and match the pixel data")]
    Dimensions,
}

/// A premultiplied RGBA image.
#[derive(Clone)]
pub struct Image {
    pixmap: Pixmap,
}

impl std::fmt::Debug for Image {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Image({}x{})", self.width(), self.height())
    }
}

impl Image {
    /// Build an image from straight (non-premultiplied) RGBA bytes.
    pub fn from_rgba(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self, ImageError> {
        if rgba.len() != width as usize * height as usize * 4 {
            return Err(ImageError::Dimensions);
        }
        let mut data = rgba;
        for [red, green, blue, alpha] in data.as_chunks_mut::<4>().0 {
            let color = tiny_skia::ColorU8::from_rgba(*red, *green, *blue, *alpha).premultiply();
            [*red, *green, *blue, *alpha] =
                [color.red(), color.green(), color.blue(), color.alpha()];
        }
        let size = int_size(width, height).ok_or(ImageError::Dimensions)?;
        let pixmap = Pixmap::from_vec(data, size).ok_or(ImageError::Dimensions)?;
        Ok(Self { pixmap })
    }

    pub fn from_png(bytes: &[u8]) -> Result<Self, ImageError> {
        Pixmap::decode_png(bytes)
            .map(|pixmap| Self { pixmap })
            .map_err(|error| ImageError::Png(error.to_string()))
    }

    /// Rasterise an SVG document into a `size × size` square, keeping its
    /// aspect ratio and centring it.
    pub fn from_svg(bytes: &[u8], size: u32) -> Result<Self, ImageError> {
        let tree = usvg::Tree::from_data(bytes, &usvg::Options::default())
            .map_err(|error| ImageError::Svg(error.to_string()))?;
        let mut pixmap = Pixmap::new(size.max(1), size.max(1)).ok_or(ImageError::Dimensions)?;
        let view = tree.size();
        let fit = (size as f32 / view.width()).min(size as f32 / view.height());
        let dx = (size as f32 - view.width() * fit) / 2.0;
        let dy = (size as f32 - view.height() * fit) / 2.0;
        resvg::render(
            &tree,
            Transform::from_row(fit, 0.0, 0.0, fit, dx, dy),
            &mut pixmap.as_mut(),
        );
        Ok(Self { pixmap })
    }

    /// Load a PNG or SVG file. SVGs are rasterised at `svg_size` pixels.
    pub fn load(path: &Path, svg_size: u32) -> Result<Self, ImageError> {
        let bytes = std::fs::read(path).map_err(|source| ImageError::Read {
            path: path.display().to_string(),
            source,
        })?;
        let is_svg = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"));
        if is_svg {
            Self::from_svg(&bytes, svg_size)
        } else {
            Self::from_png(&bytes)
        }
    }

    pub fn width(&self) -> u32 {
        self.pixmap.width()
    }

    pub fn height(&self) -> u32 {
        self.pixmap.height()
    }

    /// A copy resampled (bilinear) to `width × height`.
    pub fn scaled(&self, width: u32, height: u32) -> Result<Self, ImageError> {
        let mut pixmap = Pixmap::new(width, height).ok_or(ImageError::Dimensions)?;
        let paint = tiny_skia::PixmapPaint {
            quality: tiny_skia::FilterQuality::Bilinear,
            ..Default::default()
        };
        let transform = Transform::from_scale(
            width as f32 / self.width() as f32,
            height as f32 / self.height() as f32,
        );
        pixmap.draw_pixmap(0, 0, self.pixmap.as_ref(), &paint, transform, None);
        Ok(Self { pixmap })
    }

    pub(crate) fn pixmap(&self) -> Option<&Pixmap> {
        Some(&self.pixmap)
    }

    /// Premultiplied pixel, for tests.
    pub fn pixel(&self, x: u32, y: u32) -> Option<PremultipliedColorU8> {
        self.pixmap.pixel(x, y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_input_is_premultiplied_and_size_checked() {
        let image = Image::from_rgba(1, 1, vec![255, 0, 0, 128]).unwrap();
        let pixel = image.pixel(0, 0).unwrap();
        assert_eq!((pixel.red(), pixel.alpha()), (128, 128));
        assert!(Image::from_rgba(2, 2, vec![0; 4]).is_err());
    }

    #[test]
    fn svg_icons_render_centred_at_the_requested_size() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="20">
            <rect width="10" height="20" fill="#ff0000"/></svg>"##;
        let image = Image::from_svg(svg, 20).unwrap();
        assert_eq!((image.width(), image.height()), (20, 20));
        assert_eq!(image.pixel(0, 10).unwrap().alpha(), 0);
        assert_eq!(image.pixel(10, 10).unwrap().red(), 255);
    }

    #[test]
    fn scaling_resamples_to_the_requested_size() {
        let image = Image::from_rgba(2, 2, [0, 0, 0, 255].repeat(4)).unwrap();
        let scaled = image.scaled(8, 6).unwrap();
        assert_eq!((scaled.width(), scaled.height()), (8, 6));
        assert_eq!(scaled.pixel(4, 3).unwrap().alpha(), 255);
        assert!(image.scaled(0, 4).is_err());
    }
}
