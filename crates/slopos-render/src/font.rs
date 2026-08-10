use ab_glyph::{Font as AbFont, FontArc, PxScale, ScaleFont};
use cosmic_text::{Attrs, Buffer, CacheKey, Family, FontSystem, Metrics, Shaping, SwashCache};
use cosmic_text::{SwashContent, Wrap};
use fontdb::{Query, Stretch, Style, Weight};
use parking_lot::Mutex;
use slopos_fonts::RecoveryFallbackContract;
use std::collections::HashMap;
use std::fs;
use std::ops::Range;
use std::sync::{Arc, OnceLock};

pub struct RetroFont {
    pub font_system: Arc<Mutex<FontSystem>>,
}

impl Default for RetroFont {
    fn default() -> Self {
        Self::new()
    }
}

impl RetroFont {
    pub fn new() -> Self {
        Self {
            font_system: shared_font_system(),
        }
    }

    pub fn font_system(&self) -> Arc<Mutex<FontSystem>> {
        self.font_system.clone()
    }
}

static SHARED_FONT_SYSTEM: OnceLock<Arc<Mutex<FontSystem>>> = OnceLock::new();

fn shared_font_system() -> Arc<Mutex<FontSystem>> {
    SHARED_FONT_SYSTEM
        .get_or_init(|| Arc::new(Mutex::new(FontSystem::new())))
        .clone()
}

const DEFAULT_TEXT_FONT_SIZE: f32 = 13.0;
const DEFAULT_TEXT_LINE_HEIGHT: f32 = 14.0;
const DEFAULT_TEXT_SCALE: f32 = 1.0;
const MAX_LAYOUT_LINES: usize = 4096;
const MAX_LAYOUT_WIDTH: f32 = 1_000_000.0;
const MAX_TEXT_SCALE: f32 = 8.0;

/// Wrapping policy used by the shared shaped-text layout service.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum TextWrap {
    None,
    Glyph,
    Word,
    #[default]
    WordOrGlyph,
}

impl TextWrap {
    fn to_cosmic(self) -> Wrap {
        match self {
            Self::None => Wrap::None,
            Self::Glyph => Wrap::Glyph,
            Self::Word => Wrap::Word,
            Self::WordOrGlyph => Wrap::WordOrGlyph,
        }
    }
}

/// Logical text layout settings. Font sizes and widths are expressed in UI
/// units; `scale` is the physical framebuffer pixels per UI unit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextLayoutOptions {
    pub font_size: f32,
    pub line_height: f32,
    pub max_width: Option<f32>,
    pub scale: f32,
    pub wrap: TextWrap,
}

impl TextLayoutOptions {
    pub fn new(font_size: f32, scale: f32) -> Self {
        let line_height = if font_size.is_finite() && font_size > 0.0 {
            font_size * (DEFAULT_TEXT_LINE_HEIGHT / DEFAULT_TEXT_FONT_SIZE)
        } else {
            DEFAULT_TEXT_LINE_HEIGHT
        };
        Self {
            font_size,
            line_height,
            max_width: None,
            scale,
            wrap: TextWrap::default(),
        }
    }

    pub fn with_max_width(mut self, max_width: f32) -> Self {
        self.max_width = Some(max_width);
        self
    }

    pub fn without_max_width(mut self) -> Self {
        self.max_width = None;
        self
    }

    pub fn with_line_height(mut self, line_height: f32) -> Self {
        self.line_height = line_height;
        self
    }

    pub fn with_wrap(mut self, wrap: TextWrap) -> Self {
        self.wrap = wrap;
        self
    }

    fn normalized(self) -> Self {
        let font_size = if self.font_size.is_finite() && self.font_size > 0.0 {
            self.font_size
        } else {
            DEFAULT_TEXT_FONT_SIZE
        };
        let line_height = if self.line_height.is_finite() && self.line_height > 0.0 {
            self.line_height
        } else {
            font_size * (DEFAULT_TEXT_LINE_HEIGHT / DEFAULT_TEXT_FONT_SIZE)
        };
        let scale = if self.scale.is_finite() && self.scale > 0.0 {
            self.scale.clamp(0.25, MAX_TEXT_SCALE)
        } else {
            DEFAULT_TEXT_SCALE
        };
        let max_width = self
            .max_width
            .filter(|width| width.is_finite())
            .map(|width| width.max(0.0).min(MAX_LAYOUT_WIDTH / scale));

        Self {
            font_size,
            line_height,
            max_width,
            scale,
            wrap: self.wrap,
        }
    }
}

impl Default for TextLayoutOptions {
    fn default() -> Self {
        Self::new(DEFAULT_TEXT_FONT_SIZE, DEFAULT_TEXT_SCALE)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TextLayoutCacheKey {
    text: String,
    font_size_bits: u32,
    line_height_bits: u32,
    max_width_bits: Option<u32>,
    scale_bits: u32,
    wrap: TextWrap,
}

impl TextLayoutCacheKey {
    fn new(text: &str, options: TextLayoutOptions) -> Self {
        Self {
            text: text.to_owned(),
            font_size_bits: options.font_size.to_bits(),
            line_height_bits: options.line_height.to_bits(),
            max_width_bits: options.max_width.map(f32::to_bits),
            scale_bits: options.scale.to_bits(),
            wrap: options.wrap,
        }
    }
}

/// A glyph positioned by the shaped layout service.
#[derive(Debug, Clone)]
pub struct ShapedGlyph {
    /// Logical x position relative to the layout origin.
    pub x: f32,
    /// Logical baseline y position relative to the layout origin.
    pub baseline_y: f32,
    /// Logical advance supplied by the shaper.
    pub advance: f32,
    /// Start of the source cluster in UTF-8 bytes.
    pub cluster_start: usize,
    /// End of the source cluster in UTF-8 bytes.
    pub cluster_end: usize,
    fallback_char: Option<char>,
    raster: Option<RasterGlyph>,
    bitmap_fallback: bool,
}

impl ShapedGlyph {
    pub fn fallback_char(&self) -> Option<char> {
        self.fallback_char
    }

    pub fn raster(&self) -> Option<&RasterGlyph> {
        self.raster.as_ref()
    }

    pub fn uses_bitmap_fallback(&self) -> bool {
        self.bitmap_fallback
    }
}

/// A shaped and laid out UTF-8 string with physical glyph raster data ready
/// for a software/immediate-mode renderer.
#[derive(Debug, Clone)]
pub struct TextLayout {
    glyphs: Vec<ShapedGlyph>,
    cluster_ranges: Vec<Range<usize>>,
    line_widths: Vec<f32>,
    width: f32,
    height: f32,
    scale: f32,
    physical_font_size: f32,
    bitmap_fallback: bool,
    fallback_family: Option<&'static str>,
}

impl TextLayout {
    pub fn glyphs(&self) -> &[ShapedGlyph] {
        &self.glyphs
    }

    /// Cluster ranges for the first laid-out line. Ranges are UTF-8 byte
    /// ranges and never split a shaped cluster.
    pub fn cluster_ranges(&self) -> &[Range<usize>] {
        &self.cluster_ranges
    }

    pub fn width(&self) -> f32 {
        self.width
    }

    pub fn first_line_width(&self) -> f32 {
        self.line_widths.first().copied().unwrap_or(0.0)
    }

    pub fn line_count(&self) -> usize {
        self.line_widths.len()
    }

    pub fn height(&self) -> f32 {
        self.height
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    pub fn physical_font_size(&self) -> f32 {
        self.physical_font_size
    }

    pub fn uses_bitmap_fallback(&self) -> bool {
        self.bitmap_fallback
    }

    /// Logical font-service family handled by the renderer's bitmap fallback,
    /// when this layout needed that recovery path.
    pub fn fallback_family(&self) -> Option<&'static str> {
        self.fallback_family
    }
}

const TEXT_LAYOUT_CACHE_CAPACITY: usize = 256;

/// Bounded cache for shaped layouts and their scale-specific glyph rasters.
pub struct TextLayoutCache {
    font_system: Arc<Mutex<FontSystem>>,
    swash_cache: SwashCache,
    entries: HashMap<TextLayoutCacheKey, TextLayout>,
}

impl Default for TextLayoutCache {
    fn default() -> Self {
        Self::with_font_system_arc(shared_font_system())
    }
}

impl TextLayoutCache {
    /// Construct a cache around a caller-owned cosmic font system. This is
    /// useful for applications that manage a specific font database and for
    /// recovery tests with no installed shaping fonts.
    pub fn with_font_system(font_system: FontSystem) -> Self {
        Self::with_font_system_arc(Arc::new(Mutex::new(font_system)))
    }

    fn with_font_system_arc(font_system: Arc<Mutex<FontSystem>>) -> Self {
        Self {
            font_system,
            swash_cache: SwashCache::new(),
            entries: HashMap::new(),
        }
    }

    pub fn layout(&mut self, text: &str, options: TextLayoutOptions) -> TextLayout {
        let options = options.normalized();
        let key = TextLayoutCacheKey::new(text, options);
        if let Some(layout) = self.entries.get(&key) {
            return layout.clone();
        }

        let layout = build_text_layout(text, options, &self.font_system, &mut self.swash_cache);
        if !self.entries.contains_key(&key) && self.entries.len() >= TEXT_LAYOUT_CACHE_CAPACITY {
            self.entries.clear();
            self.swash_cache.image_cache.clear();
            self.swash_cache.outline_command_cache.clear();
        }
        self.entries.insert(key, layout.clone());
        layout
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.swash_cache.image_cache.clear();
        self.swash_cache.outline_command_cache.clear();
    }

    pub fn cached_layout_count(&self) -> usize {
        self.entries.len()
    }
}

static GLOBAL_TEXT_LAYOUT_CACHE: OnceLock<Mutex<TextLayoutCache>> = OnceLock::new();

fn global_text_layout_cache() -> &'static Mutex<TextLayoutCache> {
    GLOBAL_TEXT_LAYOUT_CACHE.get_or_init(|| Mutex::new(TextLayoutCache::default()))
}

/// Shape and lay out text using the shared system font database.
pub fn shape_text(text: &str, options: TextLayoutOptions) -> TextLayout {
    global_text_layout_cache().lock().layout(text, options)
}

/// Return text truncated at a shaped cluster boundary and append a measured
/// ellipsis when the first line does not fit.
pub fn ellipsize_text(text: &str, max_width: f32, options: TextLayoutOptions) -> String {
    if !max_width.is_finite() || max_width <= 0.0 {
        return String::new();
    }

    let options = options.without_max_width();
    let full = shape_text(text, options);
    if full.first_line_width() <= max_width {
        return text.to_owned();
    }

    let ellipsis = "...";
    let ellipsis_width = shape_text(ellipsis, options).first_line_width();
    if ellipsis_width >= max_width {
        return ellipsis.to_owned();
    }

    let mut prefix_end = 0;
    for range in full.cluster_ranges() {
        if range.end > text.len() {
            break;
        }
        let candidate = &text[..range.end];
        let candidate_width = shape_text(candidate, options).first_line_width();
        if candidate_width + ellipsis_width > max_width {
            break;
        }
        prefix_end = range.end;
    }

    let mut result = text[..prefix_end].to_owned();
    result.push_str(ellipsis);
    result
}

fn build_text_layout(
    text: &str,
    options: TextLayoutOptions,
    font_system: &Arc<Mutex<FontSystem>>,
    swash_cache: &mut SwashCache,
) -> TextLayout {
    let physical_font_size = options.font_size * options.scale;
    let physical_line_height = options.line_height * options.scale;
    let physical_width = options
        .max_width
        .map(|width| (width * options.scale).min(MAX_LAYOUT_WIDTH))
        .unwrap_or(MAX_LAYOUT_WIDTH);
    let physical_height = physical_line_height * MAX_LAYOUT_LINES as f32;

    let mut font_system = font_system.lock();
    if font_system.db().faces().next().is_none() {
        return build_bitmap_fallback_layout(text, options);
    }
    let mut buffer = Buffer::new(
        &mut font_system,
        Metrics::new(physical_font_size, physical_line_height),
    );
    buffer.set_size(&mut font_system, physical_width, physical_height);
    buffer.set_wrap(
        &mut font_system,
        if options.max_width.is_some() {
            options.wrap.to_cosmic()
        } else {
            Wrap::None
        },
    );
    buffer.set_text(&mut font_system, text, Attrs::new(), Shaping::Advanced);

    let mut glyphs = Vec::new();
    let mut line_widths = Vec::new();
    let mut first_line_cluster_ranges = Vec::new();
    let mut bitmap_fallback = false;

    for run in buffer.layout_runs() {
        let first_line = line_widths.is_empty();
        line_widths.push(run.line_w / options.scale);
        for glyph in run.glyphs {
            let physical = glyph.physical((0.0, 0.0), 1.0);
            let fallback_char = run
                .text
                .get(glyph.start..glyph.end)
                .and_then(|cluster| cluster.chars().next());
            if first_line {
                first_line_cluster_ranges.push(glyph.start..glyph.end);
            }

            let cosmic_raster =
                rasterize_cosmic_glyph(&mut font_system, swash_cache, physical.cache_key);
            let used_bitmap_fallback = cosmic_raster.is_none();
            let raster = cosmic_raster
                .or_else(|| fallback_char.and_then(|ch| rasterize_char(ch, physical_font_size)));
            bitmap_fallback |= used_bitmap_fallback;

            glyphs.push(ShapedGlyph {
                x: physical.x as f32 / options.scale,
                baseline_y: (run.line_y + physical.y as f32) / options.scale,
                advance: glyph.w / options.scale,
                cluster_start: glyph.start,
                cluster_end: glyph.end,
                fallback_char,
                raster,
                bitmap_fallback: used_bitmap_fallback,
            });
        }
    }

    if glyphs.is_empty() && !text.is_empty() {
        return build_bitmap_fallback_layout(text, options);
    }

    let cluster_ranges = merge_cluster_ranges(first_line_cluster_ranges);
    let width = line_widths.iter().copied().fold(0.0, f32::max);
    let line_count = line_widths.len();
    TextLayout {
        glyphs,
        cluster_ranges,
        line_widths,
        width,
        height: line_count as f32 * options.line_height,
        scale: options.scale,
        physical_font_size,
        bitmap_fallback,
        fallback_family: bitmap_fallback.then_some(RecoveryFallbackContract::family()),
    }
}

fn merge_cluster_ranges(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    ranges.retain(|range| range.start < range.end);
    ranges.sort_by_key(|range| (range.start, range.end));
    let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        if let Some(last) = merged.last_mut() {
            if range.start < last.end {
                last.end = last.end.max(range.end);
                continue;
            }
        }
        merged.push(range);
    }
    merged
}

fn build_bitmap_fallback_layout(text: &str, options: TextLayoutOptions) -> TextLayout {
    let physical_font_size = options.font_size * options.scale;
    let physical_line_height = options.line_height * options.scale;
    let max_width = options
        .max_width
        .map(|width| width * options.scale)
        .unwrap_or(MAX_LAYOUT_WIDTH)
        .min(MAX_LAYOUT_WIDTH);
    let mut glyphs = Vec::new();
    let mut line_widths = vec![0.0];
    let mut cluster_ranges = Vec::new();
    let mut x = 0.0;
    let mut line = 0usize;

    for (index, ch) in text.char_indices() {
        if ch == '\n' {
            line += 1;
            line_widths.push(0.0);
            x = 0.0;
            continue;
        }
        let raster = rasterize_char(ch, physical_font_size);
        let advance = raster
            .as_ref()
            .map(|glyph| glyph.advance)
            .unwrap_or(7.0 * options.scale);
        if options.max_width.is_some()
            && options.wrap != TextWrap::None
            && x > 0.0
            && x + advance > max_width
        {
            line += 1;
            line_widths.push(0.0);
            x = 0.0;
        }

        let end = index + ch.len_utf8();
        if line == 0 {
            cluster_ranges.push(index..end);
        }
        let baseline_y = line as f32 * physical_line_height + options.font_size * options.scale;
        glyphs.push(ShapedGlyph {
            x: x / options.scale,
            baseline_y: baseline_y / options.scale,
            advance: advance / options.scale,
            cluster_start: index,
            cluster_end: end,
            fallback_char: Some(ch),
            raster,
            bitmap_fallback: true,
        });
        x += advance;
        if let Some(width) = line_widths.last_mut() {
            *width = x / options.scale;
        }
    }

    let width = line_widths.iter().copied().fold(0.0, f32::max);
    TextLayout {
        glyphs,
        cluster_ranges: merge_cluster_ranges(cluster_ranges),
        line_widths: line_widths.clone(),
        width,
        height: line_widths.len() as f32 * options.line_height,
        scale: options.scale,
        physical_font_size,
        bitmap_fallback: true,
        fallback_family: Some(RecoveryFallbackContract::family()),
    }
}

fn rasterize_cosmic_glyph(
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    cache_key: CacheKey,
) -> Option<RasterGlyph> {
    let image = swash_cache
        .get_image(font_system, cache_key)
        .as_ref()?
        .clone();
    let width = image.placement.width;
    let height = image.placement.height;
    let pixel_count = width.checked_mul(height)? as usize;
    let data = match image.content {
        SwashContent::Mask => image.data,
        SwashContent::Color | SwashContent::SubpixelMask => {
            image.data.chunks_exact(4).map(|pixel| pixel[3]).collect()
        }
    };
    if data.len() != pixel_count {
        return None;
    }

    let ascent = image.placement.top.max(0) as f32;
    let descent = (height as f32 - ascent).max(0.0);
    Some(RasterGlyph {
        data,
        width,
        height,
        advance: 0.0,
        bearing_x: image.placement.left as f32,
        bearing_y: -(image.placement.top as f32),
        top: -(image.placement.top as f32),
        ascent,
        descent,
    })
}

static AB_FONT: OnceLock<Option<FontArc>> = OnceLock::new();

const SYSTEM_FONT_FALLBACKS: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf",
    "/usr/share/fonts/truetype/freefont/FreeSans.ttf",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
    "/System/Library/Fonts/Supplemental/Helvetica.ttf",
    "/Library/Fonts/Arial.ttf",
];

fn load_ab_font() -> Option<FontArc> {
    let mut font_sys = FontSystem::new();
    let query = Query {
        families: &[Family::SansSerif],
        weight: Weight::NORMAL,
        stretch: Stretch::Normal,
        style: Style::Normal,
    };
    let font_id = font_sys.db_mut().query(&query);
    if let Some(id) = font_id {
        if let Some(data) = font_sys.db().with_face_data(id, |data, _| data.to_vec()) {
            if let Ok(font) = FontArc::try_from_vec(data) {
                return Some(font);
            }
        }
    }

    for path in SYSTEM_FONT_FALLBACKS {
        if let Ok(data) = fs::read(path) {
            if let Ok(font) = FontArc::try_from_vec(data) {
                return Some(font);
            }
        }
    }

    log::warn!("no usable system sans-serif font found; falling back to bitmap glyphs");
    None
}

/// Rasterized glyph data with exact typographic bearings and metrics.
#[derive(Debug, Clone)]
pub struct RasterGlyph {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub advance: f32,
    /// Horizontal left-side bearing (min x offset from glyph origin).
    pub bearing_x: f32,
    /// Vertical top bearing relative to baseline (min y offset from baseline).
    pub bearing_y: f32,
    /// Top offset relative to baseline (legacy alias for bearing_y).
    pub top: f32,
    /// Font ascent at this size (in pixels): distance from baseline to line top.
    pub ascent: f32,
    /// Font descent at this size (in pixels): distance from baseline to line bottom.
    pub descent: f32,
}

/// The cache key uses the exact physical pixel size used by `ab_glyph`.
/// Keeping the float bits avoids rounding two nearby scales into the same
/// raster while still giving the hash map a stable, comparable key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphCacheKey {
    ch: char,
    font_size_bits: u32,
}

impl GlyphCacheKey {
    fn new(ch: char, font_size: f32) -> Option<Self> {
        if !font_size.is_finite() || font_size <= 0.0 {
            return None;
        }

        Some(Self {
            ch,
            font_size_bits: font_size.to_bits(),
        })
    }
}

const GLYPH_CACHE_CAPACITY: usize = 4096;

#[derive(Default)]
struct GlyphCache {
    entries: HashMap<GlyphCacheKey, Option<RasterGlyph>>,
}

impl GlyphCache {
    fn lookup(&self, key: GlyphCacheKey) -> Option<Option<RasterGlyph>> {
        self.entries.get(&key).cloned()
    }

    fn insert(&mut self, key: GlyphCacheKey, glyph: Option<RasterGlyph>) {
        if !self.entries.contains_key(&key) && self.entries.len() >= GLYPH_CACHE_CAPACITY {
            // Clearing at a fixed entry count keeps memory bounded without
            // depending on hash-map iteration order for eviction.
            self.entries.clear();
        }
        self.entries.insert(key, glyph);
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

static RASTER_CACHE: OnceLock<Mutex<GlyphCache>> = OnceLock::new();

fn raster_cache() -> &'static Mutex<GlyphCache> {
    RASTER_CACHE.get_or_init(|| Mutex::new(GlyphCache::default()))
}

pub fn rasterize_char(ch: char, font_size: f32) -> Option<RasterGlyph> {
    if ch.is_control() {
        return None;
    }
    let key = GlyphCacheKey::new(ch, font_size)?;
    if let Some(cached) = raster_cache().lock().lookup(key) {
        return cached;
    }

    let glyph = rasterize_char_uncached(ch, font_size);
    raster_cache().lock().insert(key, glyph.clone());
    glyph
}

fn rasterize_char_uncached(ch: char, font_size: f32) -> Option<RasterGlyph> {
    let font = AB_FONT.get_or_init(load_ab_font).as_ref()?;
    let glyph_id = AbFont::glyph_id(font, ch);
    if glyph_id.0 == 0 && !ch.is_control() {
        return None;
    }
    let px_scale = PxScale::from(font_size);
    let scaled_font = font.as_scaled(px_scale);
    let advance = scaled_font.h_advance(glyph_id);
    let ascent = scaled_font.ascent();
    let descent = scaled_font.descent();
    let glyph = glyph_id.with_scale(px_scale);
    let Some(outlined) = AbFont::outline_glyph(font, glyph) else {
        return Some(RasterGlyph {
            data: Vec::new(),
            width: 0,
            height: 0,
            advance,
            bearing_x: 0.0,
            bearing_y: -ascent,
            top: -ascent,
            ascent,
            descent,
        });
    };
    let bounds = outlined.px_bounds();
    let width = bounds.width().ceil() as u32;
    let height = bounds.height().ceil() as u32;
    let bearing_x = bounds.min.x;
    let bearing_y = bounds.min.y;
    let top = bounds.min.y;
    if width == 0 || height == 0 {
        return Some(RasterGlyph {
            data: Vec::new(),
            width: 0,
            height: 0,
            advance,
            bearing_x,
            bearing_y,
            top,
            ascent,
            descent,
        });
    }
    let mut data = vec![0u8; (width * height) as usize];
    outlined.draw(|x, y, coverage| {
        let ix = x as usize;
        let iy = y as usize;
        if ix < width as usize && iy < height as usize {
            data[iy * width as usize + ix] = (coverage * 255.0) as u8;
        }
    });
    Some(RasterGlyph {
        data,
        width,
        height,
        advance,
        bearing_x,
        bearing_y,
        top,
        ascent,
        descent,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_glyph() -> RasterGlyph {
        RasterGlyph {
            data: vec![255],
            width: 1,
            height: 1,
            advance: 1.0,
            bearing_x: 0.0,
            bearing_y: 0.0,
            top: 0.0,
            ascent: 1.0,
            descent: 0.0,
        }
    }

    #[test]
    fn cache_key_is_stable_and_scale_sensitive() {
        let base = GlyphCacheKey::new('A', 13.0).expect("valid size");

        assert_eq!(base, GlyphCacheKey::new('A', 13.0).expect("valid size"));
        assert_ne!(base, GlyphCacheKey::new('A', 13.25).expect("valid size"));
        assert_ne!(base, GlyphCacheKey::new('B', 13.0).expect("valid size"));
    }

    #[test]
    fn cache_reuses_hits_and_preserves_missing_glyphs() {
        let mut cache = GlyphCache::default();
        let present_key = GlyphCacheKey::new('A', 13.0).expect("valid size");
        let missing_key = GlyphCacheKey::new('\u{1f600}', 13.0).expect("valid size");
        let glyph = sample_glyph();

        cache.insert(present_key, Some(glyph.clone()));
        cache.insert(missing_key, None);

        let Some(Some(cached)) = cache.lookup(present_key) else {
            panic!("present glyph was not cached");
        };
        assert_eq!(cached.data, glyph.data);
        assert_eq!(cached.advance, glyph.advance);
        assert!(matches!(cache.lookup(missing_key), Some(None)));
        assert!(cache
            .lookup(GlyphCacheKey::new('A', 13.25).expect("valid size"))
            .is_none());
    }

    #[test]
    fn rasterize_char_populates_each_exact_scale_entry() {
        let first_key = GlyphCacheKey::new('Q', 13.0).expect("valid size");
        let second_key = GlyphCacheKey::new('Q', 13.25).expect("valid size");

        let _ = rasterize_char('Q', 13.0);
        assert!(raster_cache().lock().lookup(first_key).is_some());

        let _ = rasterize_char('Q', 13.25);
        assert!(raster_cache().lock().lookup(second_key).is_some());
    }

    #[test]
    fn cache_capacity_is_bounded() {
        let mut cache = GlyphCache::default();
        let glyph = sample_glyph();

        for index in 0..=GLYPH_CACHE_CAPACITY {
            let ch = char::from_u32(0x1000 + index as u32).expect("test character");
            let key = GlyphCacheKey::new(ch, 13.0).expect("valid size");
            cache.insert(key, Some(glyph.clone()));
        }

        assert!(cache.len() <= GLYPH_CACHE_CAPACITY);
    }

    #[test]
    fn invalid_font_sizes_are_rejected_before_font_lookup() {
        for size in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(GlyphCacheKey::new('A', size).is_none());
            assert!(rasterize_char('A', size).is_none());
        }
    }

    #[test]
    fn shaped_layout_preserves_unicode_cluster_boundaries() {
        let mut cache = TextLayoutCache::default();
        let text = "Cafe\u{301} 日本語🙂";
        let layout = cache.layout(text, TextLayoutOptions::new(13.0, 1.0));

        assert!(layout.width() > 0.0);
        assert!(!layout.glyphs().is_empty());
        assert!(layout.glyphs().iter().all(|glyph| {
            glyph.cluster_start <= glyph.cluster_end
                && text.is_char_boundary(glyph.cluster_start)
                && text.is_char_boundary(glyph.cluster_end)
        }));
        assert!(
            layout
                .cluster_ranges()
                .iter()
                .any(|range| text.get(range.clone()) == Some("e\u{301}")),
            "clusters={:?}, glyphs={:?}",
            layout.cluster_ranges(),
            layout.glyphs()
        );
    }

    #[test]
    fn advanced_shaping_keeps_combining_marks_in_one_cluster() {
        let mut cache = TextLayoutCache::default();
        let text = "e\u{301}";
        let layout = cache.layout(text, TextLayoutOptions::new(13.0, 1.0));

        assert_eq!(layout.cluster_ranges().len(), 1);
        assert_eq!(layout.cluster_ranges()[0], 0..text.len());
        assert!(layout.glyphs().len() <= text.chars().count());
    }

    #[test]
    fn bitmap_recovery_is_used_when_the_shaping_font_system_is_empty() {
        let font_system =
            FontSystem::new_with_locale_and_db("en-US".to_owned(), fontdb::Database::new());
        let mut cache = TextLayoutCache::with_font_system(font_system);
        let layout = cache.layout("A", TextLayoutOptions::new(13.0, 1.0));

        assert!(
            layout.uses_bitmap_fallback(),
            "glyphs={:?}",
            layout.glyphs()
        );
        assert_eq!(layout.glyphs().len(), 1);
    }

    #[test]
    fn bitmap_recovery_reports_the_font_service_contract_family() {
        let font_system =
            FontSystem::new_with_locale_and_db("en-US".to_owned(), fontdb::Database::new());
        let mut cache = TextLayoutCache::with_font_system(font_system);
        let layout = cache.layout("A", TextLayoutOptions::new(13.0, 1.0));

        assert!(layout.uses_bitmap_fallback());
        assert_eq!(
            layout.fallback_family(),
            Some(slopos_fonts::RecoveryFallbackContract::family())
        );
        assert_eq!(
            slopos_fonts::RecoveryFallbackContract::provider(),
            "slopos-render bitmap fallback"
        );
        assert!(!slopos_fonts::RecoveryFallbackContract::has_embedded_font_bytes());
        assert_eq!(layout.glyphs()[0].fallback_char(), Some('A'));
    }

    #[test]
    fn shaped_layout_wraps_and_reports_the_measured_line_width() {
        let mut cache = TextLayoutCache::default();
        let options = TextLayoutOptions::new(13.0, 1.0).with_max_width(42.0);
        let layout = cache.layout("one two three four", options);

        assert!(layout.line_count() > 1);
        assert!(layout.width() <= 42.0 + 0.01);
        assert!(layout.first_line_width() <= 42.0 + 0.01);
    }

    #[test]
    fn layout_cache_key_includes_fractional_scale() {
        let mut cache = TextLayoutCache::default();
        let at_one = cache.layout("Scale", TextLayoutOptions::new(13.0, 1.0));
        let at_fractional = cache.layout("Scale", TextLayoutOptions::new(13.0, 1.25));

        assert_eq!(cache.cached_layout_count(), 2);
        assert_ne!(at_one.scale(), at_fractional.scale());
        assert_ne!(
            at_one.physical_font_size().to_bits(),
            at_fractional.physical_font_size().to_bits()
        );

        let _same = cache.layout("Scale", TextLayoutOptions::new(13.0, 1.0));
        assert_eq!(cache.cached_layout_count(), 2);
    }
}
