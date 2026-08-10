//! XCursor theme lookup and a safe bitmap fallback for `CursorImageStatus::Named`.
//!
//! A named cursor is normally an XCursor file on disk (for example,
//! `/usr/share/icons/*/cursors/left_ptr`). This module keeps the resolver
//! dependency-free, validates the bounded binary format before reading it, and
//! scales the nearest nominal image when the requested output scale is absent.
//! The procedural arrow remains the final safe fallback for clients that leave
//! the cursor `Named` or refer to a missing/invalid asset. See `AGENTS.md`, P1.
//!
//! Wiring this resolver into the DRM render-element list is a later step; this
//! module only resolves and produces pixels.

use std::cmp::Reverse;
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::OutputScale;

/// A small cursor image plus its hotspot, ready to hand to a renderer as raw
/// ARGB8888 bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CursorBitmap {
    pub width: i32,
    pub height: i32,
    /// Pixel offset from the image's top-left to the point the cursor
    /// represents (where clicks actually land). `(0, 0)` for [`default_arrow`]:
    /// the arrow's tip is the image's first pixel.
    pub hotspot: (i32, i32),
    /// `width * height * 4` bytes, row-major, one pixel per 4 bytes in
    /// `B, G, R, A` order — the in-memory byte layout of `DRM_FORMAT_ARGB8888`
    /// on a little-endian host, matching the `Argb8888` format `session_drm.rs`
    /// already negotiates for cursor plane buffers.
    pub argb: Vec<u8>,
}

/// Logical cursor size used when a client does not provide an explicit
/// `XCURSOR_SIZE` value.
pub const DEFAULT_CURSOR_SIZE: u32 = 24;

const DEFAULT_THEME_NAME: &str = "default";
const IMAGE_TYPE: u32 = 0xFFFD_0002;
const IMAGE_HEADER_SIZE: usize = 36;
const TOC_ENTRY_SIZE: usize = 12;
const FILE_HEADER_SIZE: usize = 16;
const MAX_XCURSOR_FILE_SIZE: u64 = 16 * 1024 * 1024;
const MAX_XCURSOR_IMAGES: u32 = 128;
const MAX_CURSOR_DIMENSION: u32 = 4096;
const MAX_CURSOR_PIXELS: u64 = 4 * 1024 * 1024;
const MAX_THEME_INHERIT_DEPTH: usize = 32;

/// Resolves named cursor assets from an explicit XCursor theme search path.
///
/// The search paths are directories containing theme directories, such as
/// `/usr/share/icons` or a temporary test root containing `Aurora/cursors`.
/// Keeping the paths explicit makes resolution deterministic for the
/// compositor and avoids consulting process-global environment state in the
/// hot path. [`Self::from_environment`] is available for the standard client
/// environment case.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CursorThemeResolver {
    theme_name: String,
    search_paths: Vec<PathBuf>,
    logical_size: u32,
}

impl CursorThemeResolver {
    /// Construct a resolver for `theme_name` and the supplied theme roots.
    pub fn new<I>(theme_name: impl Into<String>, search_paths: I) -> Self
    where
        I: IntoIterator<Item = PathBuf>,
    {
        let theme_name = theme_name.into();
        let theme_name = if is_safe_component(&theme_name) {
            theme_name
        } else {
            DEFAULT_THEME_NAME.to_owned()
        };

        let mut unique_paths = Vec::new();
        for path in search_paths {
            if path.as_os_str().is_empty() || unique_paths.iter().any(|known| known == &path) {
                continue;
            }
            unique_paths.push(path);
        }

        Self {
            theme_name,
            search_paths: unique_paths,
            logical_size: DEFAULT_CURSOR_SIZE,
        }
    }

    /// Build a resolver from the conventional `XCURSOR_*` and XDG settings.
    ///
    /// Environment values affect only future lookups on this resolver; no
    /// global process state is changed by resolving an asset.
    pub fn from_environment() -> Self {
        let theme_name =
            env::var("XCURSOR_THEME").unwrap_or_else(|_| DEFAULT_THEME_NAME.to_owned());
        let logical_size = env::var("XCURSOR_SIZE")
            .ok()
            .and_then(|value| value.trim().parse::<u32>().ok())
            .filter(|value| (1..=MAX_CURSOR_DIMENSION).contains(value))
            .unwrap_or(DEFAULT_CURSOR_SIZE);

        Self::new(theme_name, cursor_theme_search_paths()).with_logical_size(logical_size)
    }

    /// Set the requested logical cursor size before output scaling.
    pub fn with_logical_size(mut self, logical_size: u32) -> Self {
        self.logical_size = logical_size.clamp(1, MAX_CURSOR_DIMENSION);
        self
    }

    /// Resolve `cursor_name` at `scale`, returning `None` for a missing or
    /// malformed asset. A nearest nominal XCursor image is selected and
    /// resampled to the requested physical size when needed.
    pub fn resolve(&self, cursor_name: &str, scale: OutputScale) -> Option<CursorBitmap> {
        if !is_safe_component(cursor_name) {
            return None;
        }

        let asset = find_cursor_asset(
            &self.theme_name,
            cursor_name,
            &self.search_paths,
            &mut HashSet::new(),
            0,
        )?;
        let images = read_xcursor_images(&asset)?;
        let target_size = target_cursor_size(self.logical_size, scale);
        let image = choose_xcursor_image(&images, target_size)?;
        bitmap_from_xcursor_image(image, target_size)
    }

    /// Resolve a named cursor, using the scale-aware procedural arrow when the
    /// theme asset is absent, malformed, or unsafe to address.
    pub fn resolve_or_fallback(&self, cursor_name: &str, scale: OutputScale) -> CursorBitmap {
        self.resolve(cursor_name, scale)
            .unwrap_or_else(|| default_arrow_scaled(scale))
    }
}

/// Resolve one named cursor from explicit theme search roots.
pub fn resolve_named_cursor(
    theme_name: &str,
    cursor_name: &str,
    search_paths: &[PathBuf],
    scale: OutputScale,
) -> Option<CursorBitmap> {
    CursorThemeResolver::new(theme_name, search_paths.iter().cloned()).resolve(cursor_name, scale)
}

/// Resolve one named cursor, falling back to the visible procedural arrow.
pub fn resolve_named_cursor_or_fallback(
    theme_name: &str,
    cursor_name: &str,
    search_paths: &[PathBuf],
    scale: OutputScale,
) -> CursorBitmap {
    CursorThemeResolver::new(theme_name, search_paths.iter().cloned())
        .resolve_or_fallback(cursor_name, scale)
}

/// Return the standard XCursor theme roots in search order.
pub fn cursor_theme_search_paths() -> Vec<PathBuf> {
    let home = env::var_os("HOME").map(PathBuf::from);

    if let Some(xcursor_path) = env::var_os("XCURSOR_PATH") {
        return xcursor_path
            .to_string_lossy()
            .split(':')
            .filter(|entry| !entry.is_empty())
            .filter_map(|entry| expand_home(PathBuf::from(entry), home.as_deref()))
            .collect();
    }

    let mut paths = Vec::new();
    if let Some(data_home) = env::var_os("XDG_DATA_HOME") {
        if let Some(path) = expand_home(PathBuf::from(data_home), home.as_deref()) {
            paths.push(path);
        }
    } else if let Some(home) = home.as_deref() {
        paths.push(home.join(".local/share/icons"));
    }

    if let Some(home) = home.as_deref() {
        paths.push(home.join(".icons"));
    }

    if let Some(data_dirs) = env::var_os("XDG_DATA_DIRS") {
        paths.extend(
            data_dirs
                .to_string_lossy()
                .split(':')
                .filter(|entry| !entry.is_empty())
                .filter_map(|entry| expand_home(PathBuf::from(entry), home.as_deref()))
                .map(|path| path.join("icons")),
        );
    } else {
        paths.push(PathBuf::from("/usr/local/share/icons"));
        paths.push(PathBuf::from("/usr/share/icons"));
    }

    paths.push(PathBuf::from("/usr/share/pixmaps"));
    if let Some(home) = home.as_deref() {
        paths.push(home.join(".cursors"));
    }
    paths.push(PathBuf::from("/usr/share/cursors/xorg-x11"));
    paths
}

const WIDTH: i32 = 24;
const HEIGHT: i32 = 24;

/// Classic filled-black, white-outlined arrow pointer, hotspot at its tip `(0, 0)`.
///
/// Shape: a triangular arrowhead with its apex at the hotspot, plus a small
/// tail rectangle hanging off the triangle's base — the familiar desktop
/// pointer silhouette. Drawn pixel-by-pixel below rather than loaded from an
/// asset, so this crate needs no new dependency to have *a* visible pointer.
pub fn default_arrow() -> CursorBitmap {
    let mut argb = vec![0u8; (WIDTH * HEIGHT * 4) as usize];

    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let idx = ((y * WIDTH + x) * 4) as usize;
            if arrow_fill(x, y) {
                write_pixel(&mut argb, idx, 0x00, 0x00, 0x00); // opaque black
            } else if touches_fill(x, y) {
                write_pixel(&mut argb, idx, 0xFF, 0xFF, 0xFF); // opaque white outline
            }
            // else: left as zeroed bytes, i.e. fully transparent.
        }
    }

    CursorBitmap {
        width: WIDTH,
        height: HEIGHT,
        hotspot: (0, 0),
        argb,
    }
}

/// Scale the procedural fallback to the requested output scale.
pub fn default_arrow_scaled(scale: OutputScale) -> CursorBitmap {
    let target_size = target_cursor_size(DEFAULT_CURSOR_SIZE, scale);
    let fallback = default_arrow();
    scale_cursor_bitmap(fallback, DEFAULT_CURSOR_SIZE, target_size).unwrap_or_else(default_arrow)
}

#[derive(Debug)]
struct XCursorImage {
    nominal_size: u32,
    width: u32,
    height: u32,
    xhot: u32,
    yhot: u32,
    pixels: Vec<u8>,
}

fn find_cursor_asset(
    theme_name: &str,
    cursor_name: &str,
    search_paths: &[PathBuf],
    visited: &mut HashSet<String>,
    depth: usize,
) -> Option<PathBuf> {
    if depth > MAX_THEME_INHERIT_DEPTH
        || !is_safe_component(theme_name)
        || !is_safe_component(cursor_name)
        || !visited.insert(theme_name.to_owned())
    {
        return None;
    }

    let theme_dirs: Vec<PathBuf> = search_paths
        .iter()
        .map(|root| root.join(theme_name))
        .filter(|theme_dir| theme_dir.is_dir())
        .collect();

    for theme_dir in &theme_dirs {
        let asset = theme_dir.join("cursors").join(cursor_name);
        if asset.is_file() {
            return Some(asset);
        }
    }

    let mut inherited = Vec::new();
    for theme_dir in &theme_dirs {
        let index = theme_dir.join("index.theme");
        if let Some(names) = read_inherited_themes(&index) {
            for name in names {
                if !inherited.contains(&name) {
                    inherited.push(name);
                }
            }
        } else if theme_name != DEFAULT_THEME_NAME {
            inherited.push(DEFAULT_THEME_NAME.to_owned());
        }
    }
    if theme_dirs.is_empty() && theme_name != DEFAULT_THEME_NAME {
        inherited.push(DEFAULT_THEME_NAME.to_owned());
    }

    for inherited_name in inherited {
        if let Some(asset) = find_cursor_asset(
            &inherited_name,
            cursor_name,
            search_paths,
            visited,
            depth + 1,
        ) {
            return Some(asset);
        }
    }
    None
}

fn read_inherited_themes(index: &Path) -> Option<Vec<String>> {
    let contents = fs::read_to_string(index).ok()?;
    for line in contents.lines() {
        let Some((key, value)) = line.trim().split_once('=') else {
            continue;
        };
        if key.trim() != "Inherits" {
            continue;
        }
        let names = value
            .split(|character: char| {
                character.is_whitespace() || character == ',' || character == ';'
            })
            .filter(|name| is_safe_component(name))
            .map(str::to_owned)
            .collect();
        return Some(names);
    }
    None
}

fn read_xcursor_images(path: &Path) -> Option<Vec<XCursorImage>> {
    let length = fs::metadata(path).ok()?.len();
    if length == 0 || length > MAX_XCURSOR_FILE_SIZE || length > usize::MAX as u64 {
        return None;
    }
    parse_xcursor_images(&fs::read(path).ok()?)
}

fn parse_xcursor_images(data: &[u8]) -> Option<Vec<XCursorImage>> {
    if data.len() < FILE_HEADER_SIZE || data.get(..4)? != b"Xcur" {
        return None;
    }

    let header_size = usize::try_from(read_u32(data, 4)?).ok()?;
    let image_count = read_u32(data, 12)?;
    if header_size < FILE_HEADER_SIZE
        || header_size > data.len()
        || image_count > MAX_XCURSOR_IMAGES
    {
        return None;
    }

    let toc_bytes = usize::try_from(image_count)
        .ok()?
        .checked_mul(TOC_ENTRY_SIZE)?;
    let toc_end = header_size.checked_add(toc_bytes)?;
    if toc_end > data.len() {
        return None;
    }

    let mut images = Vec::new();
    for index in 0..image_count as usize {
        let toc = header_size.checked_add(index.checked_mul(TOC_ENTRY_SIZE)?)?;
        if read_u32(data, toc)? != IMAGE_TYPE {
            continue;
        }
        let position = usize::try_from(read_u32(data, toc + 8)?).ok()?;
        if let Some(image) = parse_xcursor_image(data, position) {
            images.push(image);
        }
    }
    (!images.is_empty()).then_some(images)
}

fn parse_xcursor_image(data: &[u8], offset: usize) -> Option<XCursorImage> {
    let image_end = offset.checked_add(IMAGE_HEADER_SIZE)?;
    if image_end > data.len()
        || read_u32(data, offset)? as usize != IMAGE_HEADER_SIZE
        || read_u32(data, offset + 4)? != IMAGE_TYPE
        || read_u32(data, offset + 12)? != 1
    {
        return None;
    }

    let nominal_size = read_u32(data, offset + 8)?;
    let width = read_u32(data, offset + 16)?;
    let height = read_u32(data, offset + 20)?;
    let xhot = read_u32(data, offset + 24)?;
    let yhot = read_u32(data, offset + 28)?;
    if nominal_size == 0
        || width == 0
        || height == 0
        || width > MAX_CURSOR_DIMENSION
        || height > MAX_CURSOR_DIMENSION
        || xhot >= width
        || yhot >= height
    {
        return None;
    }

    let pixel_count = u64::from(width).checked_mul(u64::from(height))?;
    if pixel_count > MAX_CURSOR_PIXELS {
        return None;
    }
    let pixel_bytes = usize::try_from(pixel_count.checked_mul(4)?).ok()?;
    let pixel_end = image_end.checked_add(pixel_bytes)?;
    if pixel_end > data.len() {
        return None;
    }

    Some(XCursorImage {
        nominal_size,
        width,
        height,
        xhot,
        yhot,
        pixels: data[image_end..pixel_end].to_vec(),
    })
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    Some(u32::from_le_bytes(data.get(offset..end)?.try_into().ok()?))
}

fn choose_xcursor_image(images: &[XCursorImage], target_size: u32) -> Option<&XCursorImage> {
    images.iter().min_by_key(|image| {
        (
            image.nominal_size.abs_diff(target_size),
            if image.nominal_size >= target_size {
                0
            } else {
                1
            },
            Reverse(image.nominal_size),
        )
    })
}

fn bitmap_from_xcursor_image(image: &XCursorImage, target_size: u32) -> Option<CursorBitmap> {
    let width = scaled_dimension(image.width, image.nominal_size, target_size)?;
    let height = scaled_dimension(image.height, image.nominal_size, target_size)?;
    let pixels = resize_nearest(&image.pixels, image.width, image.height, width, height)?;
    let hotspot = (
        scaled_hotspot(image.xhot, image.nominal_size, target_size, width)?,
        scaled_hotspot(image.yhot, image.nominal_size, target_size, height)?,
    );

    Some(CursorBitmap {
        width: i32::try_from(width).ok()?,
        height: i32::try_from(height).ok()?,
        hotspot,
        argb: pixels,
    })
}

fn scale_cursor_bitmap(
    bitmap: CursorBitmap,
    nominal_size: u32,
    target_size: u32,
) -> Option<CursorBitmap> {
    let width = u32::try_from(bitmap.width).ok()?;
    let height = u32::try_from(bitmap.height).ok()?;
    let xhot = u32::try_from(bitmap.hotspot.0).ok()?;
    let yhot = u32::try_from(bitmap.hotspot.1).ok()?;
    let image = XCursorImage {
        nominal_size,
        width,
        height,
        xhot,
        yhot,
        pixels: bitmap.argb,
    };
    bitmap_from_xcursor_image(&image, target_size)
}

fn target_cursor_size(logical_size: u32, scale: OutputScale) -> u32 {
    let target = f64::from(logical_size.max(1)) * scale.as_f64();
    if !target.is_finite() || target <= 0.0 {
        return 1;
    }
    target.round().clamp(1.0, f64::from(MAX_CURSOR_DIMENSION)) as u32
}

fn scaled_dimension(value: u32, nominal_size: u32, target_size: u32) -> Option<u32> {
    let numerator = u64::from(value).checked_mul(u64::from(target_size))?;
    let denominator = u64::from(nominal_size.max(1));
    let scaled = (numerator + denominator / 2) / denominator;
    u32::try_from(scaled.max(1))
        .ok()
        .filter(|value| *value <= MAX_CURSOR_DIMENSION)
}

fn scaled_hotspot(value: u32, nominal_size: u32, target_size: u32, dimension: u32) -> Option<i32> {
    let numerator = u64::from(value).checked_mul(u64::from(target_size))?;
    let denominator = u64::from(nominal_size.max(1));
    let scaled = (numerator + denominator / 2) / denominator;
    let scaled = u32::try_from(scaled).ok()?;
    i32::try_from(scaled.min(dimension.saturating_sub(1))).ok()
}

fn resize_nearest(
    source: &[u8],
    source_width: u32,
    source_height: u32,
    width: u32,
    height: u32,
) -> Option<Vec<u8>> {
    let source_len = u64::from(source_width)
        .checked_mul(u64::from(source_height))?
        .checked_mul(4)?;
    let destination_len = u64::from(width)
        .checked_mul(u64::from(height))?
        .checked_mul(4)?;
    if source_len != source.len() as u64 || destination_len > MAX_XCURSOR_FILE_SIZE {
        return None;
    }
    let destination_len = usize::try_from(destination_len).ok()?;
    let mut destination = vec![0u8; destination_len];
    for y in 0..height {
        let source_y = (u64::from(y) * u64::from(source_height) / u64::from(height))
            .min(u64::from(source_height - 1)) as u32;
        for x in 0..width {
            let source_x = (u64::from(x) * u64::from(source_width) / u64::from(width))
                .min(u64::from(source_width - 1)) as u32;
            let source_index = ((source_y * source_width + source_x) * 4) as usize;
            let destination_index = ((y * width + x) * 4) as usize;
            destination[destination_index..destination_index + 4]
                .copy_from_slice(&source[source_index..source_index + 4]);
        }
    }
    Some(destination)
}

fn is_safe_component(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && !value
            .chars()
            .any(|character| character == '/' || character == '\\' || character == '\0')
}

fn expand_home(path: PathBuf, home: Option<&Path>) -> Option<PathBuf> {
    let mut components = path.components();
    let first = components.next();
    if !matches!(
        first,
        Some(std::path::Component::Normal(component))
            if component == std::ffi::OsStr::new("~")
    ) {
        return Some(path);
    }
    let home = home?;
    let mut expanded = home.to_path_buf();
    for component in components {
        expanded.push(component.as_os_str());
    }
    Some(expanded)
}

/// Write one opaque BGRA pixel at byte offset `idx`.
fn write_pixel(buf: &mut [u8], idx: usize, b: u8, g: u8, r: u8) {
    buf[idx] = b;
    buf[idx + 1] = g;
    buf[idx + 2] = r;
    buf[idx + 3] = 0xFF;
}

/// True when `(x, y)` is part of the arrow's solid black silhouette.
///
/// Two pieces, sharing the row `y == 16` as a seam so they read as one shape:
/// - a right triangle, apex `(0, 0)`, right edge widening linearly down to
///   `(12, 16)` — the arrowhead;
/// - a narrow rectangle `x in [3, 7], y in [16, 21]` — the tail.
fn arrow_fill(x: i32, y: i32) -> bool {
    let in_head = (0..=16).contains(&y) && x >= 0 && x <= (y * 12) / 16;
    let in_tail = (3..=7).contains(&x) && (16..=21).contains(&y);
    in_head || in_tail
}

/// True when `(x, y)` is itself outside the silhouette but 8-connected to a
/// pixel that is inside it — i.e. it belongs on the 1px outline ring.
fn touches_fill(x: i32, y: i32) -> bool {
    if arrow_fill(x, y) {
        return false;
    }
    for dy in -1..=1 {
        for dx in -1..=1 {
            if (dx, dy) != (0, 0) && arrow_fill(x + dx, y + dy) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimensions_are_24x24_with_zero_hotspot() {
        let c = default_arrow();
        assert_eq!(c.width, 24);
        assert_eq!(c.height, 24);
        assert_eq!(c.hotspot, (0, 0));
    }

    #[test]
    fn buffer_length_matches_width_times_height_times_4() {
        let c = default_arrow();
        assert_eq!(c.argb.len(), (c.width * c.height * 4) as usize);
    }

    #[test]
    fn hotspot_pixel_is_opaque() {
        let c = default_arrow();
        // Hotspot (0, 0) is the image's first pixel: bytes [0..4), alpha at index 3.
        assert_eq!(c.argb[3], 0xFF, "hotspot pixel must be opaque");
    }

    #[test]
    fn image_contains_both_black_and_white_pixels() {
        let c = default_arrow();
        let mut has_black = false;
        let mut has_white = false;
        for px in c.argb.chunks_exact(4) {
            let (b, g, r, a) = (px[0], px[1], px[2], px[3]);
            if a == 0xFF && b == 0x00 && g == 0x00 && r == 0x00 {
                has_black = true;
            }
            if a == 0xFF && b == 0xFF && g == 0xFF && r == 0xFF {
                has_white = true;
            }
        }
        assert!(has_black, "expected at least one opaque black fill pixel");
        assert!(
            has_white,
            "expected at least one opaque white outline pixel"
        );
    }

    #[test]
    fn no_pixel_is_both_black_and_white() {
        // arrow_fill and touches_fill must be mutually exclusive by construction;
        // guard the invariant directly rather than only via the color scan above.
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                assert!(!(arrow_fill(x, y) && touches_fill(x, y)));
            }
        }
    }

    #[test]
    fn shape_is_non_trivial_and_bounded() {
        let c = default_arrow();
        let opaque = c.argb.chunks_exact(4).filter(|px| px[3] == 0xFF).count();
        // Comfortably more than a stray pixel, comfortably less than the whole canvas.
        assert!(opaque > 20);
        assert!(opaque < (c.width * c.height) as usize);
    }

    #[test]
    fn named_theme_lookup_loads_cursor_from_explicit_search_path() {
        let theme = TestTheme::new();
        theme.write_asset(
            "Aurora",
            "left_ptr",
            &[TestImage {
                nominal_size: 24,
                width: 2,
                height: 2,
                xhot: 1,
                yhot: 0,
                pixel: [0x10, 0x20, 0x30, 0x40],
            }],
        );

        let bitmap = CursorThemeResolver::new("Aurora", [theme.root.clone()])
            .resolve("left_ptr", crate::OutputScale::IDENTITY)
            .expect("named cursor asset should be found");

        assert_eq!(bitmap.width, 2);
        assert_eq!(bitmap.height, 2);
        assert_eq!(bitmap.hotspot, (1, 0));
        assert_eq!(&bitmap.argb[..4], &[0x10, 0x20, 0x30, 0x40]);
    }

    #[test]
    fn named_theme_lookup_selects_exact_scaled_image_and_hotspot() {
        let theme = TestTheme::new();
        theme.write_asset(
            "Aurora",
            "left_ptr",
            &[
                TestImage {
                    nominal_size: 24,
                    width: 24,
                    height: 24,
                    xhot: 3,
                    yhot: 4,
                    pixel: [0x24, 0x24, 0x24, 0xFF],
                },
                TestImage {
                    nominal_size: 48,
                    width: 48,
                    height: 48,
                    xhot: 7,
                    yhot: 9,
                    pixel: [0x48, 0x48, 0x48, 0xFF],
                },
            ],
        );

        let bitmap = CursorThemeResolver::new("Aurora", [theme.root.clone()])
            .resolve("left_ptr", crate::OutputScale::new(2, 1).unwrap())
            .expect("2x cursor asset should be found");

        assert_eq!((bitmap.width, bitmap.height), (48, 48));
        assert_eq!(bitmap.hotspot, (7, 9));
        assert_eq!(&bitmap.argb[..4], &[0x48, 0x48, 0x48, 0xFF]);
    }

    #[test]
    fn missing_scaled_asset_resizes_nearest_image_and_hotspot() {
        let theme = TestTheme::new();
        theme.write_asset(
            "Aurora",
            "left_ptr",
            &[TestImage {
                nominal_size: 24,
                width: 24,
                height: 24,
                xhot: 2,
                yhot: 3,
                pixel: [0xAA, 0xBB, 0xCC, 0xDD],
            }],
        );

        let bitmap = CursorThemeResolver::new("Aurora", [theme.root.clone()])
            .resolve("left_ptr", crate::OutputScale::new(2, 1).unwrap())
            .expect("nearest cursor asset should be used when 2x is missing");

        assert_eq!((bitmap.width, bitmap.height), (48, 48));
        assert_eq!(bitmap.hotspot, (4, 6));
        assert_eq!(bitmap.argb.len(), 48 * 48 * 4);
        assert_eq!(&bitmap.argb[..4], &[0xAA, 0xBB, 0xCC, 0xDD]);
    }

    #[test]
    fn missing_or_invalid_asset_returns_safe_scaled_arrow() {
        let theme = TestTheme::new();
        theme.write_raw("Aurora", "left_ptr", b"not-an-xcursor-file");
        let scale = crate::OutputScale::new(3, 2).unwrap();
        let resolver = CursorThemeResolver::new("Aurora", [theme.root.clone()]);

        assert!(resolver.resolve("missing", scale).is_none());
        assert!(resolver.resolve("left_ptr", scale).is_none());

        let fallback = resolver.resolve_or_fallback("left_ptr", scale);
        assert_eq!((fallback.width, fallback.height), (36, 36));
        assert_eq!(fallback.hotspot, (0, 0));
        assert_eq!(fallback.argb.len(), 36 * 36 * 4);
    }

    struct TestTheme {
        root: std::path::PathBuf,
    }

    impl TestTheme {
        fn new() -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};

            static NEXT_ID: AtomicU64 = AtomicU64::new(0);
            let root = loop {
                let candidate = std::env::temp_dir().join(format!(
                    "slopos-cursor-theme-{}-{}",
                    std::process::id(),
                    NEXT_ID.fetch_add(1, Ordering::Relaxed)
                ));
                match std::fs::create_dir(&candidate) {
                    Ok(()) => break candidate,
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("create temporary cursor theme: {error}"),
                }
            };
            Self { root }
        }

        fn write_asset(&self, theme: &str, cursor: &str, images: &[TestImage]) {
            let path = self.root.join(theme).join("cursors").join(cursor);
            std::fs::create_dir_all(path.parent().expect("cursor parent directory")).unwrap();
            std::fs::write(path, encode_xcursor(images)).unwrap();
        }

        fn write_raw(&self, theme: &str, cursor: &str, contents: &[u8]) {
            let path = self.root.join(theme).join("cursors").join(cursor);
            std::fs::create_dir_all(path.parent().expect("cursor parent directory")).unwrap();
            std::fs::write(path, contents).unwrap();
        }
    }

    impl Drop for TestTheme {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[derive(Clone, Copy)]
    struct TestImage {
        nominal_size: u32,
        width: u32,
        height: u32,
        xhot: u32,
        yhot: u32,
        pixel: [u8; 4],
    }

    fn encode_xcursor(images: &[TestImage]) -> Vec<u8> {
        const FILE_HEADER_SIZE: u32 = 16;
        const TOC_ENTRY_SIZE: u32 = 12;
        const IMAGE_HEADER_SIZE: u32 = 36;
        const IMAGE_TYPE: u32 = 0xFFFD_0002;

        let image_start = FILE_HEADER_SIZE + TOC_ENTRY_SIZE * images.len() as u32;
        let mut output = Vec::new();
        output.extend_from_slice(b"Xcur");
        push_u32(&mut output, FILE_HEADER_SIZE);
        push_u32(&mut output, 1);
        push_u32(&mut output, images.len() as u32);

        let mut next_image = image_start;
        for image in images {
            push_u32(&mut output, IMAGE_TYPE);
            push_u32(&mut output, image.nominal_size);
            push_u32(&mut output, next_image);
            next_image += IMAGE_HEADER_SIZE + image.width * image.height * 4;
        }

        for image in images {
            push_u32(&mut output, IMAGE_HEADER_SIZE);
            push_u32(&mut output, IMAGE_TYPE);
            push_u32(&mut output, image.nominal_size);
            push_u32(&mut output, 1);
            push_u32(&mut output, image.width);
            push_u32(&mut output, image.height);
            push_u32(&mut output, image.xhot);
            push_u32(&mut output, image.yhot);
            push_u32(&mut output, 0);
            for _ in 0..image.width * image.height {
                output.extend_from_slice(&image.pixel);
            }
        }
        output
    }

    fn push_u32(output: &mut Vec<u8>, value: u32) {
        output.extend_from_slice(&value.to_le_bytes());
    }
}
