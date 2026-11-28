//! Bounded image decoding for the Image Viewer (IMAGE-01).
//!
//! Adapted from the archived compositor-era Preview decoder
//! (`archive/2026-10-30-wayland-compositor-and-apps:apps/preview/src/viewer.rs`,
//! first-party MIT source), without its vision-service coupling.
//!
//! Every file is checked before and while decoding:
//! - it must be a regular file no larger than [`MAX_ENCODED_BYTES`], and its
//!   size must not change while it is read;
//! - the format is sniffed from the content (a PNG named `.jpg` is a PNG),
//!   and only PNG, JPEG, GIF, BMP, WebP and TIFF are decoded; SVG is
//!   rasterised through resvg when the content looks like SVG;
//! - the declared dimensions must be non-zero and at most
//!   [`MAX_SOURCE_PIXELS`] *before* decoding, and the decoder's own
//!   allocation limit ([`MAX_DECODE_ALLOC`]) stops decompression bombs that
//!   lie in their headers;
//! - EXIF orientation is applied, so photos display upright.
//!
//! [`DecodeCache`] keeps recently decoded images under a byte budget so
//! next/previous is instant without unbounded memory.

use std::collections::VecDeque;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits};
use slopos_ui::image::Image;

/// Largest file the viewer reads.
pub const MAX_ENCODED_BYTES: u64 = 64 * 1024 * 1024;
/// Largest decoded image, in pixels (40 MP, about 160 MB as RGBA).
pub const MAX_SOURCE_PIXELS: u64 = 40_000_000;
/// Decoder allocation ceiling, a little above one maximal RGBA frame.
pub const MAX_DECODE_ALLOC: u64 = 192 * 1024 * 1024;
/// Largest side of an image, in pixels.
pub const MAX_SIDE: u32 = 32_768;
/// SVGs are rasterised so their longer side is this many pixels.
pub const SVG_RASTER_SIZE: u32 = 2048;
/// Decoded-image cache budget. Sized for the 512 MB Pi Zero 2 W goal: a
/// decode of a 40 MP JPEG peaks near 280 MB, and the image on screen is held
/// outside the cache, so the cache stays small.
pub const CACHE_BUDGET_BYTES: usize = 64 * 1024 * 1024;
const _: () = assert!(CACHE_BUDGET_BYTES < MAX_DECODE_ALLOC as usize);

/// Extensions offered in the Open dialog and used for folder navigation.
pub const EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "jpe", "gif", "bmp", "webp", "tif", "tiff", "svg",
];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    #[error("can't open this file: {0}")]
    Io(String),
    #[error("this is not a regular file")]
    NotAFile,
    #[error("the file is too large ({0} bytes; the limit is {MAX_ENCODED_BYTES})")]
    TooLarge(u64),
    #[error("the file changed while it was being read")]
    Changed,
    #[error("this file is not a supported image")]
    Unsupported,
    #[error("the image has no pixels")]
    Empty,
    #[error("the image is too large to open (over {MAX_SOURCE_PIXELS} pixels or {MAX_SIDE} pixels on a side)")]
    TooManyPixels { width: u32, height: u32 },
    #[error("the image is damaged: {0}")]
    Damaged(String),
    #[error("the file ends before the image does, so it may be incomplete")]
    Truncated,
    #[error("the open was cancelled")]
    Cancelled,
}

/// A decoded image ready for display.
#[derive(Debug)]
pub struct Decoded {
    pub image: Image,
    /// Format name for the status line, e.g. "JPEG".
    pub format: &'static str,
    /// Size after orientation.
    pub width: u32,
    pub height: u32,
    /// Whether EXIF orientation changed the stored pixels.
    pub reoriented: bool,
    /// Bytes the decoded pixels occupy (for cache accounting).
    pub bytes: usize,
}

fn format_name(format: ImageFormat) -> Option<&'static str> {
    Some(match format {
        ImageFormat::Png => "PNG",
        ImageFormat::Jpeg => "JPEG",
        ImageFormat::Gif => "GIF",
        ImageFormat::Bmp => "BMP",
        ImageFormat::WebP => "WebP",
        ImageFormat::Tiff => "TIFF",
        _ => return None,
    })
}

/// Map a decoder error: limit violations are "too large", running out of data
/// is "truncated", and anything else is damage.
fn classify(error: image::ImageError, width: u32, height: u32) -> DecodeError {
    match error {
        image::ImageError::Limits(_) => DecodeError::TooManyPixels { width, height },
        image::ImageError::IoError(io) if io.kind() == std::io::ErrorKind::UnexpectedEof => {
            DecodeError::Truncated
        }
        other => DecodeError::Damaged(other.to_string()),
    }
}

/// Whether a JPEG reaches its end-of-image marker (FF D9) after its last scan.
/// Entropy-coded data never holds FF D9 (a 0xFF byte there is always followed
/// by 00 or a restart marker), so any FF D9 after the last start-of-scan header
/// closes the image. The decoder alone would show a cut-off file as grey.
fn jpeg_is_complete(data: &[u8]) -> bool {
    let Some(scan) = data.windows(2).rposition(|pair| pair == [0xFF, 0xDA]) else {
        return false;
    };
    data[scan + 2..].windows(2).any(|pair| pair == [0xFF, 0xD9])
}

fn limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    limits
}

fn looks_like_svg(data: &[u8]) -> bool {
    let head = &data[..data.len().min(1024)];
    let text = String::from_utf8_lossy(head);
    let text = text.trim_start_matches('\u{feff}').trim_start();
    text.starts_with("<svg") || (text.starts_with("<?xml") && text.contains("<svg"))
}

/// Read a file within the size limit, refusing non-regular files and files
/// that change size while being read.
pub fn read_bounded(path: &Path) -> Result<Vec<u8>, DecodeError> {
    let meta = fs::metadata(path).map_err(|error| DecodeError::Io(error.to_string()))?;
    if !meta.is_file() {
        return Err(DecodeError::NotAFile);
    }
    if meta.len() > MAX_ENCODED_BYTES {
        return Err(DecodeError::TooLarge(meta.len()));
    }
    let mut data = Vec::with_capacity(meta.len() as usize);
    fs::File::open(path)
        .and_then(|file| file.take(MAX_ENCODED_BYTES + 1).read_to_end(&mut data))
        .map_err(|error| DecodeError::Io(error.to_string()))?;
    if data.len() as u64 != meta.len() {
        return Err(DecodeError::Changed);
    }
    Ok(data)
}

/// Decode in-memory bytes with every limit applied. The app uses the
/// cancellable [`decode_bytes_checked`]; this form is for tests.
#[cfg(test)]
pub fn decode_bytes(data: &[u8]) -> Result<Decoded, DecodeError> {
    decode_bytes_checked(data, &|| false)
}

/// Like [`decode_bytes`], but gives up with [`DecodeError::Cancelled`] when
/// `cancelled` reports true. It is asked before reading, before rasterising or
/// decoding, and after the header is read. The pixel decode itself is one call
/// in the `image` crate and cannot be interrupted part way through.
pub fn decode_bytes_checked(
    data: &[u8],
    cancelled: &dyn Fn() -> bool,
) -> Result<Decoded, DecodeError> {
    if data.len() as u64 > MAX_ENCODED_BYTES {
        return Err(DecodeError::TooLarge(data.len() as u64));
    }
    if data.is_empty() {
        return Err(DecodeError::Empty);
    }
    if cancelled() {
        return Err(DecodeError::Cancelled);
    }
    if looks_like_svg(data) {
        let image = Image::from_svg(data, SVG_RASTER_SIZE)
            .map_err(|error| DecodeError::Damaged(error.to_string()))?;
        let (width, height) = (image.width(), image.height());
        return Ok(Decoded {
            image,
            format: "SVG",
            width,
            height,
            reoriented: false,
            bytes: width as usize * height as usize * 4,
        });
    }
    let reader = || {
        let mut reader = ImageReader::new(Cursor::new(data))
            .with_guessed_format()
            .map_err(|error| DecodeError::Damaged(error.to_string()))?;
        reader.limits(limits());
        Ok::<_, DecodeError>(reader)
    };
    let guessed = reader()?.format().ok_or(DecodeError::Unsupported)?;
    let format = format_name(guessed).ok_or(DecodeError::Unsupported)?;
    if guessed == ImageFormat::Jpeg && !jpeg_is_complete(data) {
        return Err(DecodeError::Truncated);
    }
    let (width, height) = reader()?
        .into_dimensions()
        .map_err(|error| classify(error, 0, 0))?;
    if width == 0 || height == 0 {
        return Err(DecodeError::Empty);
    }
    if u64::from(width) * u64::from(height) > MAX_SOURCE_PIXELS {
        return Err(DecodeError::TooManyPixels { width, height });
    }
    if cancelled() {
        return Err(DecodeError::Cancelled);
    }
    let mut decoder = reader()?
        .into_decoder()
        .map_err(|error| classify(error, width, height))?;
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    if cancelled() {
        return Err(DecodeError::Cancelled);
    }
    let mut image =
        DynamicImage::from_decoder(decoder).map_err(|error| classify(error, width, height))?;
    let reoriented = orientation != image::metadata::Orientation::NoTransforms;
    image.apply_orientation(orientation);
    let rgba = image.into_rgba8();
    let (width, height) = rgba.dimensions();
    let image = Image::from_rgba(width, height, rgba.into_raw())
        .map_err(|error| DecodeError::Damaged(error.to_string()))?;
    Ok(Decoded {
        image,
        format,
        width,
        height,
        reoriented,
        bytes: width as usize * height as usize * 4,
    })
}

/// Read and decode `path` with every limit applied. The app uses
/// [`decode_path_checked`]; this form is for tests.
#[cfg(test)]
pub fn decode_path(path: &Path) -> Result<Decoded, DecodeError> {
    decode_path_checked(path, &|| false)
}

/// Read and decode `path`, stopping early as [`decode_bytes_checked`] does.
pub fn decode_path_checked(
    path: &Path,
    cancelled: &dyn Fn() -> bool,
) -> Result<Decoded, DecodeError> {
    if cancelled() {
        return Err(DecodeError::Cancelled);
    }
    decode_bytes_checked(&read_bounded(path)?, cancelled)
}

/// Whether `path` has an image extension the viewer browses.
pub fn is_image_name(path: &Path) -> bool {
    path.extension()
        .map(|ext| ext.to_string_lossy().to_lowercase())
        .is_some_and(|ext| EXTENSIONS.contains(&ext.as_str()))
}

/// Directory entries read while listing a folder. A folder with more entries
/// than this is listed only in part.
pub const MAX_SCANNED_ENTRIES: usize = 50_000;
/// Image files kept from one folder listing.
pub const MAX_LISTED_IMAGES: usize = 10_000;

/// The image files in one folder, and whether the listing stopped early.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Listing {
    /// Sorted case-insensitively.
    pub images: Vec<PathBuf>,
    /// The folder held more entries or images than the limits allow, so
    /// `images` is only its first part.
    pub truncated: bool,
}

/// The image files directly inside `dir`, sorted case-insensitively. The name
/// is checked before any file-system call, so a folder of other files costs
/// one `readdir` pass. An unreadable folder gives an empty listing.
pub fn images_in(dir: &Path) -> Listing {
    list_bounded(dir, MAX_SCANNED_ENTRIES, MAX_LISTED_IMAGES)
}

fn list_bounded(dir: &Path, max_scanned: usize, max_images: usize) -> Listing {
    let Ok(entries) = fs::read_dir(dir) else {
        return Listing::default();
    };
    let mut listing = Listing::default();
    for (scanned, entry) in entries.enumerate() {
        if scanned == max_scanned {
            listing.truncated = true;
            break;
        }
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if !is_image_name(&path) || !path.is_file() {
            continue;
        }
        if listing.images.len() == max_images {
            listing.truncated = true;
            break;
        }
        listing.images.push(path);
    }
    listing.images.sort_by_cached_key(|candidate| {
        candidate
            .file_name()
            .map(|name| name.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    });
    listing
}

/// Images in the same folder as `path` and the index of `path` among them.
/// Unreadable folders, or a `path` cut off by the listing limits, give just
/// `path`.
pub fn siblings(path: &Path) -> (Listing, usize) {
    let only_this = || Listing {
        images: vec![path.to_path_buf()],
        truncated: false,
    };
    let Some(dir) = path.parent() else {
        return (only_this(), 0);
    };
    let listing = images_in(dir);
    match listing
        .images
        .iter()
        .position(|candidate| candidate == path)
    {
        Some(index) => (listing, index),
        None => (
            Listing {
                images: vec![path.to_path_buf()],
                truncated: listing.truncated,
            },
            0,
        ),
    }
}

/// Recently decoded images, least recently used evicted first, within a byte
/// budget. An image larger than the whole budget is returned but not kept.
#[derive(Debug)]
pub struct DecodeCache {
    budget: usize,
    used: usize,
    entries: VecDeque<(PathBuf, Arc<Decoded>)>,
}

impl DecodeCache {
    pub fn new(budget: usize) -> Self {
        Self {
            budget,
            used: 0,
            entries: VecDeque::new(),
        }
    }

    #[cfg(test)]
    pub fn used(&self) -> usize {
        self.used
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn get(&mut self, path: &Path) -> Option<Arc<Decoded>> {
        let index = self.entries.iter().position(|(key, _)| key == path)?;
        let entry = self.entries.remove(index)?;
        let image = entry.1.clone();
        self.entries.push_back(entry);
        Some(image)
    }

    pub fn insert(&mut self, path: PathBuf, decoded: Arc<Decoded>) {
        if let Some(index) = self.entries.iter().position(|(key, _)| *key == path) {
            if let Some((_, old)) = self.entries.remove(index) {
                self.used -= old.bytes;
            }
        }
        if decoded.bytes > self.budget {
            return;
        }
        while self.used + decoded.bytes > self.budget {
            match self.entries.pop_front() {
                Some((_, old)) => self.used -= old.bytes,
                None => break,
            }
        }
        self.used += decoded.bytes;
        self.entries.push_back((path, decoded));
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use image::{ImageEncoder, RgbaImage};
    use std::cell::Cell;

    pub fn png(width: u32, height: u32) -> Vec<u8> {
        let mut image = RgbaImage::new(width, height);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            *pixel = image::Rgba([(x * 40) as u8, (y * 40) as u8, 200, 255]);
        }
        let mut out = Vec::new();
        image::codecs::png::PngEncoder::new(&mut out)
            .write_image(
                image.as_raw(),
                width,
                height,
                image::ExtendedColorType::Rgba8,
            )
            .unwrap();
        out
    }

    pub fn encode(format: ImageFormat) -> Vec<u8> {
        let image = DynamicImage::ImageRgb8(image::RgbImage::from_fn(6, 4, |x, _| {
            image::Rgb([x as u8 * 40, 10, 10])
        }));
        let mut out = Cursor::new(Vec::new());
        image.write_to(&mut out, format).unwrap();
        out.into_inner()
    }

    fn chunk(kind: &[u8], data: &[u8]) -> Vec<u8> {
        let mut out = (data.len() as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let mut crc_input = kind.to_vec();
        crc_input.extend_from_slice(data);
        out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
        out
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xffff_ffffu32;
        for &byte in data {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 != 0 {
                    (crc >> 1) ^ 0xedb8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    /// A PNG whose IHDR claims `width × height` but carries one tiny IDAT.
    fn bomb(width: u32, height: u32) -> Vec<u8> {
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut ihdr = width.to_be_bytes().to_vec();
        ihdr.extend_from_slice(&height.to_be_bytes());
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
        out.extend(chunk(b"IHDR", &ihdr));
        out.extend(chunk(
            b"IDAT",
            &[0x78, 0x9c, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01],
        ));
        out.extend(chunk(b"IEND", &[]));
        out
    }

    /// Insert an eXIf chunk with an Orientation tag before the first IDAT.
    fn with_orientation(png: &[u8], orientation: u16) -> Vec<u8> {
        // Little-endian TIFF header, one IFD entry: tag 0x0112, SHORT, 1.
        let mut exif = b"II*\x00\x08\x00\x00\x00\x01\x00".to_vec();
        exif.extend_from_slice(&0x0112u16.to_le_bytes());
        exif.extend_from_slice(&3u16.to_le_bytes());
        exif.extend_from_slice(&1u32.to_le_bytes());
        exif.extend_from_slice(&orientation.to_le_bytes());
        exif.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
        let idat = png.windows(4).position(|w| w == b"IDAT").unwrap() - 4;
        let mut out = png[..idat].to_vec();
        out.extend(chunk(b"eXIf", &exif));
        out.extend_from_slice(&png[idat..]);
        out
    }

    #[test]
    fn common_formats_decode_by_content() {
        for (format, name) in [
            (ImageFormat::Png, "PNG"),
            (ImageFormat::Jpeg, "JPEG"),
            (ImageFormat::Gif, "GIF"),
            (ImageFormat::Bmp, "BMP"),
            (ImageFormat::WebP, "WebP"),
            (ImageFormat::Tiff, "TIFF"),
        ] {
            let decoded = decode_bytes(&encode(format)).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(
                (decoded.format, decoded.width, decoded.height),
                (name, 6, 4)
            );
        }
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="red"/></svg>"#;
        let decoded = decode_bytes(svg).unwrap();
        assert_eq!(decoded.format, "SVG");
        assert!(decoded.width <= SVG_RASTER_SIZE);
    }

    /// Insert an APP1 Exif segment with an Orientation tag after SOI.
    fn jpeg_with_orientation(jpeg: &[u8], orientation: u16) -> Vec<u8> {
        let mut tiff = b"II*\x00\x08\x00\x00\x00\x01\x00".to_vec();
        tiff.extend_from_slice(&0x0112u16.to_le_bytes());
        tiff.extend_from_slice(&3u16.to_le_bytes());
        tiff.extend_from_slice(&1u32.to_le_bytes());
        tiff.extend_from_slice(&orientation.to_le_bytes());
        tiff.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
        let mut payload = b"Exif\x00\x00".to_vec();
        payload.extend(tiff);
        let mut out = jpeg[..2].to_vec();
        out.extend_from_slice(&[0xff, 0xe1]);
        out.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
        out.extend(payload);
        out.extend_from_slice(&jpeg[2..]);
        out
    }

    #[test]
    fn jpeg_exif_orientation_is_applied() {
        let jpeg = encode(ImageFormat::Jpeg);
        let rotated = decode_bytes(&jpeg_with_orientation(&jpeg, 6)).unwrap();
        assert!(rotated.reoriented);
        assert_eq!(
            (rotated.format, rotated.width, rotated.height),
            ("JPEG", 4, 6)
        );
        let mirrored = decode_bytes(&jpeg_with_orientation(&jpeg, 2)).unwrap();
        assert_eq!((mirrored.width, mirrored.height), (6, 4));
        assert!(mirrored.reoriented);
    }

    #[test]
    fn exif_orientation_rotates_the_pixels() {
        let rotated = decode_bytes(&with_orientation(&png(6, 4), 6)).unwrap();
        assert!(rotated.reoriented);
        assert_eq!((rotated.width, rotated.height), (4, 6));
        let plain = decode_bytes(&with_orientation(&png(6, 4), 1)).unwrap();
        assert!(!plain.reoriented);
        assert_eq!((plain.width, plain.height), (6, 4));
    }

    #[test]
    fn hostile_inputs_fail_closed() {
        // Header claims 50 000 × 50 000: refused before any allocation.
        assert!(matches!(
            decode_bytes(&bomb(50_000, 50_000)),
            Err(DecodeError::TooManyPixels { .. })
        ));
        // Under the side limit but over the pixel budget.
        assert!(matches!(
            decode_bytes(&bomb(20_000, 20_000)),
            Err(DecodeError::TooManyPixels { .. })
        ));
        // Within limits but the data is missing: damaged, no panic.
        assert!(matches!(
            decode_bytes(&bomb(100, 100)),
            Err(DecodeError::Damaged(_))
        ));
        let jpeg = encode(ImageFormat::Jpeg);
        assert!(decode_bytes(&jpeg[..jpeg.len() / 2]).is_err());
        assert_eq!(decode_bytes(b"").unwrap_err(), DecodeError::Empty);
        assert!(matches!(
            decode_bytes(b"plain text, not an image"),
            Err(DecodeError::Unsupported | DecodeError::Damaged(_))
        ));
        assert!(matches!(
            decode_bytes(b"<svg"),
            Err(DecodeError::Damaged(_))
        ));
        assert!(decode_bytes(&bomb(0, 10)).is_err());
    }

    #[test]
    fn files_are_checked_before_reading() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_bounded(dir.path()).unwrap_err(), DecodeError::NotAFile);
        let big = dir.path().join("big.png");
        let file = fs::File::create(&big).unwrap();
        file.set_len(MAX_ENCODED_BYTES + 1).unwrap();
        assert!(matches!(decode_path(&big), Err(DecodeError::TooLarge(_))));
        // A PNG named .jpg still decodes as PNG.
        let lying = dir.path().join("photo.jpg");
        fs::write(&lying, png(3, 3)).unwrap();
        assert_eq!(decode_path(&lying).unwrap().format, "PNG");
        assert!(matches!(
            decode_path(&dir.path().join("missing.png")),
            Err(DecodeError::Io(_))
        ));
    }

    #[test]
    fn siblings_sort_and_skip_non_images() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["b.PNG", "a.jpg", "c.txt", "d.svg"] {
            fs::write(dir.path().join(name), b"x").unwrap();
        }
        fs::create_dir(dir.path().join("e.png")).unwrap();
        let (listing, index) = siblings(&dir.path().join("b.PNG"));
        let list = listing.images;
        let names: Vec<String> = list
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["a.jpg", "b.PNG", "d.svg"]);
        assert_eq!(index, 1);
    }

    #[test]
    fn cache_stays_within_budget_and_evicts_oldest() {
        let budget = 3 * 6 * 4 * 4;
        let mut cache = DecodeCache::new(budget);
        for name in ["a", "b", "c", "d"] {
            let decoded = Arc::new(decode_bytes(&png(6, 4)).unwrap());
            cache.insert(PathBuf::from(name), decoded);
            assert!(cache.used() <= budget);
        }
        assert_eq!(cache.len(), 3);
        assert!(cache.get(Path::new("a")).is_none());
        assert!(cache.get(Path::new("b")).is_some());
        cache.insert(
            PathBuf::from("e"),
            Arc::new(decode_bytes(&png(6, 4)).unwrap()),
        );
        // "b" was used recently, so "c" went instead.
        assert!(cache.get(Path::new("b")).is_some());
        assert!(cache.get(Path::new("c")).is_none());
        let huge = Arc::new(decode_bytes(&png(64, 64)).unwrap());
        cache.insert(PathBuf::from("huge"), huge);
        assert!(cache.get(Path::new("huge")).is_none());
        assert!(cache.used() <= budget);
    }

    #[test]
    fn cancelled_decodes_stop_with_their_own_error() {
        let data = png(6, 4);
        assert_eq!(
            decode_bytes_checked(&data, &|| true).unwrap_err(),
            DecodeError::Cancelled
        );
        // Cancelled at a later checkpoint, after the header has been read.
        let checks = Cell::new(0);
        let late = || {
            checks.set(checks.get() + 1);
            checks.get() > 1
        };
        assert_eq!(
            decode_bytes_checked(&data, &late).unwrap_err(),
            DecodeError::Cancelled
        );
        assert!(
            checks.get() > 1,
            "decoding must have more than one checkpoint"
        );
        assert!(decode_bytes_checked(&data, &|| false).is_ok());
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.png");
        fs::write(&file, &data).unwrap();
        assert_eq!(
            decode_path_checked(&file, &|| true).unwrap_err(),
            DecodeError::Cancelled
        );
    }

    #[test]
    fn truncated_files_are_reported_as_truncated() {
        let jpeg = encode(ImageFormat::Jpeg);
        // The end-of-image marker is missing, so the file was cut short.
        assert_eq!(
            decode_bytes(&jpeg[..jpeg.len() - 10]).unwrap_err(),
            DecodeError::Truncated
        );
        assert_eq!(
            decode_bytes(&jpeg[..jpeg.len() / 2]).unwrap_err(),
            DecodeError::Truncated
        );
        // Bytes after the end-of-image marker, as camera files carry, are fine.
        let mut trailed = jpeg.clone();
        trailed.extend_from_slice(b"\x00trailer bytes");
        assert!(decode_bytes(&trailed).is_ok());
        let png = png(64, 48);
        assert_eq!(
            decode_bytes(&png[..png.len() / 2]).unwrap_err(),
            DecodeError::Truncated
        );
    }

    #[test]
    fn decoded_size_is_refused_from_a_tiny_file() {
        // 6401 × 6401 is just over the 40 MP budget, in a file under 100 bytes.
        let data = bomb(6_401, 6_401);
        assert!(data.len() < 100);
        assert_eq!(
            decode_bytes(&data).unwrap_err(),
            DecodeError::TooManyPixels {
                width: 6_401,
                height: 6_401
            }
        );
    }

    #[test]
    fn folder_listing_keeps_only_images_in_order() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["B.png", "a.JPG", "notes.txt"] {
            fs::write(dir.path().join(name), b"x").unwrap();
        }
        let names: Vec<String> = images_in(dir.path())
            .images
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["a.JPG", "B.png"]);
        assert!(images_in(&dir.path().join("missing")).images.is_empty());
    }

    #[test]
    fn folder_listing_is_bounded_and_says_so() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["a.png", "b.png", "c.png", "notes.txt"] {
            fs::write(dir.path().join(name), b"x").unwrap();
        }
        // Image limit: the first two images are kept and the rest flagged.
        let by_images = list_bounded(dir.path(), 100, 2);
        assert_eq!(by_images.images.len(), 2);
        assert!(by_images.truncated);
        // Entry limit: scanning stops after two entries, whatever they are.
        let by_entries = list_bounded(dir.path(), 2, 100);
        assert!(by_entries.truncated);
        assert!(by_entries.images.len() <= 2);
        // Within both limits nothing is flagged.
        let whole = list_bounded(dir.path(), 100, 100);
        assert_eq!(whole.images.len(), 3);
        assert!(!whole.truncated);
    }
}
