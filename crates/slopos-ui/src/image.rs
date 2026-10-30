//! Decoded images: PNG files and rasterised SVG icons.

use std::path::Path;
use std::sync::{Arc, OnceLock};

use resvg::usvg;
use tiny_skia::{Pixmap, PremultipliedColorU8, Transform};

use crate::foundation::color::Color;
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

/// The system font database for SVG text, loaded once.
fn system_fonts() -> Arc<usvg::fontdb::Database> {
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut database = usvg::fontdb::Database::new();
            database.load_system_fonts();
            Arc::new(database)
        })
        .clone()
}

/// How a desktop background image covers the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundMode {
    /// Scale to cover the screen, cropping the overflow evenly.
    Fill,
    /// Repeat at 1:1 from the top-left corner (patterns stay crisp).
    Tile,
    /// One copy at 1:1 in the middle.
    Center,
}

impl BackgroundMode {
    /// Parse a saved mode; anything unknown is `Fill`.
    pub fn parse(text: &str) -> Self {
        match text.trim() {
            "tile" => Self::Tile,
            "center" => Self::Center,
            _ => Self::Fill,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fill => "fill",
            Self::Tile => "tile",
            Self::Center => "center",
        }
    }
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
        let mut options = usvg::Options::default();
        // Text in icons (some draw letters) needs fonts; load them only then.
        if bytes.windows(5).any(|window| window == b"<text") {
            options.fontdb = system_fonts();
        }
        let tree = usvg::Tree::from_data(bytes, &options)
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
        self.scaled_with(width, height, false)
    }

    /// A copy resampled to `width × height`. `nearest` keeps hard pixel edges
    /// (integer-ratio pixel art); otherwise the image is filtered bilinearly.
    pub fn scaled_with(&self, width: u32, height: u32, nearest: bool) -> Result<Self, ImageError> {
        let mut pixmap = Pixmap::new(width, height).ok_or(ImageError::Dimensions)?;
        let paint = tiny_skia::PixmapPaint {
            quality: if nearest {
                tiny_skia::FilterQuality::Nearest
            } else {
                tiny_skia::FilterQuality::Bilinear
            },
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

    /// The part of the image inside `x, y, width × height`, clipped to the
    /// image.
    pub fn cropped(&self, x: u32, y: u32, width: u32, height: u32) -> Result<Self, ImageError> {
        let x = x.min(self.width());
        let y = y.min(self.height());
        let width = width.min(self.width() - x);
        let height = height.min(self.height() - y);
        let rect = tiny_skia::IntRect::from_xywh(x as i32, y as i32, width, height)
            .ok_or(ImageError::Dimensions)?;
        self.pixmap
            .clone_rect(rect)
            .map(|pixmap| Self { pixmap })
            .ok_or(ImageError::Dimensions)
    }

    /// Premultiplied RGBA bytes, row by row.
    pub fn data(&self) -> &[u8] {
        self.pixmap.data()
    }

    /// A `width × height` desktop background: this image placed by `mode`
    /// over an opaque `backdrop`.
    pub fn compose_background(
        &self,
        width: u32,
        height: u32,
        mode: BackgroundMode,
        backdrop: Color,
    ) -> Result<Self, ImageError> {
        let mut pixmap = Pixmap::new(width, height).ok_or(ImageError::Dimensions)?;
        pixmap.fill(tiny_skia::Color::from_rgba8(
            backdrop.red,
            backdrop.green,
            backdrop.blue,
            255,
        ));
        let (image_width, image_height) = (self.width() as f32, self.height() as f32);
        let paint = tiny_skia::PixmapPaint {
            quality: tiny_skia::FilterQuality::Bilinear,
            ..Default::default()
        };
        match mode {
            BackgroundMode::Fill => {
                let scale = (width as f32 / image_width).max(height as f32 / image_height);
                let dx = (width as f32 - image_width * scale) / 2.0;
                let dy = (height as f32 - image_height * scale) / 2.0;
                pixmap.draw_pixmap(
                    0,
                    0,
                    self.pixmap.as_ref(),
                    &paint,
                    Transform::from_row(scale, 0.0, 0.0, scale, dx, dy),
                    None,
                );
            }
            BackgroundMode::Center => {
                let x = (width as i64 - self.width() as i64) / 2;
                let y = (height as i64 - self.height() as i64) / 2;
                pixmap.draw_pixmap(
                    x as i32,
                    y as i32,
                    self.pixmap.as_ref(),
                    &paint,
                    Transform::identity(),
                    None,
                );
            }
            BackgroundMode::Tile => {
                let shader = tiny_skia::Pattern::new(
                    self.pixmap.as_ref(),
                    tiny_skia::SpreadMode::Repeat,
                    tiny_skia::FilterQuality::Nearest,
                    1.0,
                    Transform::identity(),
                );
                let rect = tiny_skia::Rect::from_xywh(0.0, 0.0, width as f32, height as f32)
                    .ok_or(ImageError::Dimensions)?;
                let paint = tiny_skia::Paint {
                    shader,
                    ..Default::default()
                };
                pixmap.fill_rect(rect, &paint, Transform::identity(), None);
            }
        }
        Ok(Self { pixmap })
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

    #[test]
    fn nearest_scaling_keeps_pixel_edges() {
        let image = Image::from_rgba(
            2,
            2,
            vec![
                255, 0, 0, 255, 255, 0, 0, 255, 0, 0, 255, 255, 0, 0, 255, 255,
            ],
        )
        .unwrap();
        let scaled = image.scaled_with(4, 4, true).unwrap();
        let pixel = |x, y| scaled.pixel(x, y).unwrap();
        // Each source pixel becomes a solid 2×2 block with no blended edge.
        assert_eq!(pixel(1, 0).red(), 255);
        assert_eq!(pixel(1, 0).blue(), 0);
        assert_eq!(pixel(2, 1).red(), 255);
        assert_eq!(pixel(2, 1).blue(), 0);
        assert_eq!(pixel(2, 2).blue(), 255);
        assert_eq!(pixel(2, 2).red(), 0);
        assert_eq!(pixel(3, 3).blue(), 255);
    }

    #[test]
    fn backgrounds_fill_tile_and_center() {
        let red = Color {
            red: 255,
            green: 0,
            blue: 0,
            alpha: 255,
        };
        // A 2x1 image: white then black.
        let image = Image::from_rgba(2, 1, vec![255, 255, 255, 255, 0, 0, 0, 255]).unwrap();
        let tile = image
            .compose_background(5, 2, BackgroundMode::Tile, red)
            .unwrap();
        let rgb = |image: &Image, x, y| {
            let pixel = image.pixel(x, y).unwrap();
            (pixel.red(), pixel.green(), pixel.blue(), pixel.alpha())
        };
        assert_eq!(rgb(&tile, 0, 1), (255, 255, 255, 255));
        assert_eq!(rgb(&tile, 3, 0), (0, 0, 0, 255));
        assert_eq!(rgb(&tile, 4, 1), (255, 255, 255, 255));
        let center = image
            .compose_background(6, 3, BackgroundMode::Center, red)
            .unwrap();
        assert_eq!(rgb(&center, 0, 0), (255, 0, 0, 255));
        assert_eq!(rgb(&center, 2, 1), (255, 255, 255, 255));
        assert_eq!(rgb(&center, 3, 1), (0, 0, 0, 255));
        // Fill covers the whole screen: no backdrop shows anywhere.
        let fill = image
            .compose_background(7, 9, BackgroundMode::Fill, red)
            .unwrap();
        for (x, y) in [(0, 0), (6, 8), (3, 4)] {
            assert_ne!(rgb(&fill, x, y), (255, 0, 0, 255));
        }
        assert!(image
            .compose_background(0, 9, BackgroundMode::Fill, red)
            .is_err());
        assert_eq!(BackgroundMode::parse("tile\n"), BackgroundMode::Tile);
        assert_eq!(BackgroundMode::parse("max"), BackgroundMode::Fill);
        assert_eq!(
            BackgroundMode::parse(BackgroundMode::Center.as_str()),
            BackgroundMode::Center
        );
    }

    #[test]
    fn crops_are_clipped_to_the_image() {
        let image = Image::from_rgba(
            2,
            2,
            vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ],
        )
        .unwrap();
        let corner = image.cropped(1, 1, 5, 5).unwrap();
        assert_eq!((corner.width(), corner.height()), (1, 1));
        assert_eq!(corner.pixel(0, 0).unwrap().blue(), 255);
        assert_eq!(corner.pixel(0, 0).unwrap().red(), 255);
        assert!(image.cropped(2, 2, 1, 1).is_err());
    }
}
