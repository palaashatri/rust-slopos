//! Text shaping, measurement and rasterisation.
//!
//! Uses cosmic-text (HarfBuzz-compatible shaping through rustybuzz, BiDi,
//! font fallback) with system fonts. One shared font database is loaded on
//! first use.

use std::sync::{Mutex, OnceLock};

use cosmic_text::{
    Attrs, Buffer, Color as TextColor, Family, FontSystem, Metrics, Shaping, SwashCache, Weight,
};

use crate::foundation::color::Color;

/// A font request in logical pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct Font {
    pub family: FontFamily,
    pub size: f32,
    pub bold: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontFamily {
    /// The desktop's UI sans-serif face.
    System,
    Monospace,
    Named(String),
}

impl Font {
    /// The system UI font at `size` logical pixels.
    pub fn system(size: f32) -> Self {
        Self {
            family: FontFamily::System,
            size,
            bold: false,
        }
    }

    pub fn monospace(size: f32) -> Self {
        Self {
            family: FontFamily::Monospace,
            size,
            bold: false,
        }
    }

    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    fn line_height(&self) -> f32 {
        (self.size * 1.25).ceil()
    }
}

/// Measured text extent in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextExtent {
    pub width: f32,
    pub height: f32,
}

struct Engine {
    fonts: FontSystem,
    cache: SwashCache,
}

fn engine() -> &'static Mutex<Engine> {
    static ENGINE: OnceLock<Mutex<Engine>> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let mut fonts = FontSystem::new();
        // Prefer the faces SLOPOS ships and themes against.
        let database = fonts.db_mut();
        for family in ["Liberation Sans", "DejaVu Sans", "Noto Sans"] {
            if database
                .faces()
                .any(|face| face.families.iter().any(|(name, _)| name == family))
            {
                database.set_sans_serif_family(family);
                break;
            }
        }
        for family in ["Liberation Mono", "DejaVu Sans Mono", "Noto Sans Mono"] {
            if database
                .faces()
                .any(|face| face.families.iter().any(|(name, _)| name == family))
            {
                database.set_monospace_family(family);
                break;
            }
        }
        Mutex::new(Engine {
            fonts,
            cache: SwashCache::new(),
        })
    })
}

fn attrs(font: &Font) -> Attrs<'_> {
    let family = match &font.family {
        FontFamily::System => Family::SansSerif,
        FontFamily::Monospace => Family::Monospace,
        FontFamily::Named(name) => Family::Name(name),
    };
    Attrs::new().family(family).weight(if font.bold {
        Weight::BOLD
    } else {
        Weight::NORMAL
    })
}

fn shaped(engine: &mut Engine, content: &str, font: &Font, scale: f32) -> Buffer {
    let metrics = Metrics::new(font.size * scale, font.line_height() * scale);
    let mut buffer = Buffer::new(&mut engine.fonts, metrics);
    buffer.set_size(None, None);
    buffer.set_text(content, &attrs(font), Shaping::Advanced, None);
    buffer.shape_until_scroll(&mut engine.fonts, false);
    buffer
}

/// Width and height of `content` in logical pixels.
pub fn measure(content: &str, font: &Font) -> TextExtent {
    let mut engine = engine().lock().unwrap_or_else(|poison| poison.into_inner());
    let buffer = shaped(&mut engine, content, font, 1.0);
    let mut width: f32 = 0.0;
    let mut lines = 0;
    for run in buffer.layout_runs() {
        width = width.max(run.line_w);
        lines += 1;
    }
    TextExtent {
        width: width.ceil(),
        height: font.line_height() * lines.max(1) as f32,
    }
}

/// Rasterise `content` at `scale`, calling `plot(x, y, color)` with device
/// coordinates relative to the text's top-left corner and the colour (with
/// coverage in alpha) of each covered pixel.
pub(crate) fn rasterize<F: FnMut(i32, i32, Color)>(
    content: &str,
    font: &Font,
    scale: f32,
    color: Color,
    mut plot: F,
) {
    let mut engine = engine().lock().unwrap_or_else(|poison| poison.into_inner());
    let mut buffer = shaped(&mut engine, content, font, scale);
    let Engine { fonts, cache } = &mut *engine;
    let base = TextColor::rgba(color.red, color.green, color.blue, color.alpha);
    buffer.draw(fonts, cache, base, |x, y, width, height, pixel| {
        if pixel.a() == 0 {
            return;
        }
        let pixel = Color {
            red: pixel.r(),
            green: pixel.g(),
            blue: pixel.b(),
            alpha: pixel.a(),
        };
        for dy in 0..height as i32 {
            for dx in 0..width as i32 {
                plot(x + dx, y + dy, pixel);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measurement_grows_with_text_and_lines() {
        let font = Font::system(12.0);
        let short = measure("Hi", &font);
        let long = measure("Hello, SLOPOS", &font);
        assert!(long.width > short.width && short.width > 0.0);
        let two_lines = measure("one\ntwo", &font);
        assert_eq!(two_lines.height, 2.0 * short.height);
    }

    #[test]
    fn bold_text_is_not_narrower() {
        let regular = measure("Control Panels", &Font::system(12.0));
        let bold = measure("Control Panels", &Font::system(12.0).bold());
        assert!(bold.width >= regular.width);
    }
}
