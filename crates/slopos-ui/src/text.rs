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

/// Caret positions for single-line `content`: one `(byte index, x)` pair
/// for every char boundary including the end, sorted by byte index, with `x`
/// in logical pixels from the start of the line. Glyph clusters that cover
/// several chars (ligatures, combining marks) are divided evenly; right-to-left
/// runs place the leading edge on the right.
pub fn caret_stops(content: &str, font: &Font) -> Vec<(usize, f32)> {
    let line = content.split('\n').next().unwrap_or("");
    let mut engine = engine().lock().unwrap_or_else(|poison| poison.into_inner());
    let buffer = shaped(&mut engine, line, font, 1.0);
    let mut stops: Vec<(usize, f32)> = Vec::with_capacity(line.len() + 1);
    let mut end_x: f32 = 0.0;
    if let Some(run) = buffer.layout_runs().next() {
        for glyph in run.glyphs {
            let Some(cluster) = line.get(glyph.start..glyph.end) else {
                continue;
            };
            let count = cluster.chars().count().max(1) as f32;
            for (index, (offset, _)) in cluster.char_indices().enumerate() {
                let fraction = index as f32 / count;
                let x = if glyph.level.is_rtl() {
                    glyph.x + glyph.w * (1.0 - fraction)
                } else {
                    glyph.x + glyph.w * fraction
                };
                stops.push((glyph.start + offset, x));
            }
            end_x = end_x.max(glyph.x + glyph.w);
        }
    }
    stops.sort_by_key(|stop| stop.0);
    stops.dedup_by_key(|stop| stop.0);
    // Chars that produced no glyph (for example a missing font) still need a
    // caret stop; give them the position of the previous stop.
    let mut complete = Vec::with_capacity(line.len() + 1);
    let mut next = stops.iter().peekable();
    let mut last_x = 0.0;
    for (offset, _) in line.char_indices() {
        while next.peek().is_some_and(|stop| stop.0 < offset) {
            next.next();
        }
        if let Some(stop) = next.peek().filter(|stop| stop.0 == offset) {
            last_x = stop.1;
        }
        complete.push((offset, last_x));
    }
    complete.push((line.len(), end_x));
    complete
}

/// The char boundary in single-line `content` nearest to logical `x`.
pub fn hit_test(content: &str, font: &Font, x: f32) -> usize {
    caret_stops(content, font)
        .into_iter()
        .min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs()))
        .map(|stop| stop.0)
        .unwrap_or(0)
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
    fn caret_stops_cover_every_char_boundary_in_order() {
        let font = Font::system(12.0);
        for text in ["", "abc", "héllo wörld", "e\u{301}x", "日本語", "שלום abc"] {
            let stops = caret_stops(text, &font);
            let expected: Vec<usize> = text
                .char_indices()
                .map(|(i, _)| i)
                .chain([text.len()])
                .collect();
            let got: Vec<usize> = stops.iter().map(|s| s.0).collect();
            assert_eq!(got, expected, "{text:?}");
        }
        let ltr = caret_stops("abc", &font);
        assert!(ltr.windows(2).all(|w| w[1].1 > w[0].1));
        assert!((ltr[3].1 - measure("abc", &font).width).abs() <= 1.0);
    }

    #[test]
    fn hit_testing_picks_the_nearest_boundary() {
        let font = Font::system(12.0);
        let stops = caret_stops("hello", &font);
        assert_eq!(hit_test("hello", &font, -50.0), 0);
        assert_eq!(hit_test("hello", &font, 1000.0), 5);
        assert_eq!(hit_test("hello", &font, stops[2].1 + 0.4), 2);
    }

    #[test]
    fn bold_text_is_not_narrower() {
        let regular = measure("Control Panels", &Font::system(12.0));
        let bold = measure("Control Panels", &Font::system(12.0).bold());
        assert!(bold.width >= regular.width);
    }
}
