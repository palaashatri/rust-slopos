//! Thumbnails for icon view (P1 item 5).
//!
//! The Image Viewer's decoder is private to that binary, so Files applies the
//! same kind of budget itself, before any pixel is decoded:
//! - only regular PNG and SVG files are thumbnailed, at most
//!   [`MAX_FILE_BYTES`] (SVG: [`MAX_SVG_BYTES`]);
//! - a PNG's dimensions are read from its header and refused above
//!   [`MAX_PIXELS`] before decoding;
//! - decoding runs on a worker thread, never on the UI thread, and results
//!   are kept in a [`Cache`] bounded by entry count and bytes.
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::SystemTime;

use slopos_ui::image::Image;

use crate::worker::{self, WakeSlot};

/// Largest image file thumbnailed.
pub const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
/// Largest SVG document thumbnailed (its text is parsed in full).
pub const MAX_SVG_BYTES: u64 = 1024 * 1024;
/// Largest PNG, in pixels, that is decoded for a thumbnail.
pub const MAX_PIXELS: u64 = 25_000_000;
/// Thumbnails fit inside a square of this many pixels.
pub const THUMB_SIDE: u32 = 64;
/// Cache limits: thumbnails kept at once, and their total RGBA bytes.
pub const CACHE_ENTRIES: usize = 512;
pub const CACHE_BYTES: usize = 16 * 1024 * 1024;

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Png,
    Svg,
}

/// Whether `path` names a format thumbnails are made for.
pub fn format_of(path: &Path) -> Option<Format> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => Some(Format::Png),
        "svg" => Some(Format::Svg),
        _ => None,
    }
}

/// The pixel size of a PNG from its IHDR chunk, or `None` if the bytes are
/// not a PNG header.
pub fn png_dimensions(header: &[u8]) -> Option<(u32, u32)> {
    if header.len() < 24 || header[..8] != PNG_SIGNATURE || &header[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(header[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(header[20..24].try_into().ok()?);
    Some((width, height))
}

/// Check a file's size, and a PNG's declared pixels, against the budget.
/// `header` is the first bytes of the file.
pub fn check_budget(format: Format, len: u64, header: &[u8]) -> Result<(), &'static str> {
    match format {
        Format::Png => {
            if len > MAX_FILE_BYTES {
                return Err("The image is too large for a thumbnail.");
            }
            let Some((width, height)) = png_dimensions(header) else {
                return Err("This PNG is damaged.");
            };
            if width == 0 || height == 0 {
                return Err("This image has no pixels.");
            }
            if u64::from(width) * u64::from(height) > MAX_PIXELS {
                return Err("The image has too many pixels for a thumbnail.");
            }
            Ok(())
        }
        Format::Svg => {
            if len > MAX_SVG_BYTES {
                Err("The SVG is too large for a thumbnail.")
            } else {
                Ok(())
            }
        }
    }
}

/// `width × height` scaled to fit inside `side × side`, keeping the aspect
/// ratio and never enlarging.
pub fn fit(width: u32, height: u32, side: u32) -> (u32, u32) {
    if width == 0 || height == 0 {
        return (1, 1);
    }
    if width <= side && height <= side {
        return (width, height);
    }
    let scale = f64::from(side) / f64::from(width.max(height));
    let w = (f64::from(width) * scale).round().max(1.0) as u32;
    let h = (f64::from(height) * scale).round().max(1.0) as u32;
    (w, h)
}

/// Make a thumbnail of `path`, applying the budget before any decoding.
pub fn generate(path: &Path) -> Result<Image, String> {
    let format = format_of(path).ok_or("Not an image thumbnail format.")?;
    let meta = std::fs::metadata(path).map_err(|error| error.to_string())?;
    if !meta.is_file() {
        return Err("This is not a regular file.".into());
    }
    let limit = match format {
        Format::Png => MAX_FILE_BYTES,
        Format::Svg => MAX_SVG_BYTES,
    };
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(limit + 1).read_to_end(&mut bytes))
        .map_err(|error| error.to_string())?;
    check_budget(format, bytes.len() as u64, &bytes).map_err(str::to_owned)?;
    match format {
        Format::Svg => Image::from_svg(&bytes, THUMB_SIDE).map_err(|error| error.to_string()),
        Format::Png => {
            let image = Image::from_png(&bytes).map_err(|error| error.to_string())?;
            let (width, height) = fit(image.width(), image.height(), THUMB_SIDE);
            image
                .scaled(width, height)
                .map_err(|error| error.to_string())
        }
    }
}

struct Slot {
    modified: Option<SystemTime>,
    tick: u64,
    /// `None` for a file that could not be thumbnailed; it is not retried.
    image: Option<Image>,
}

/// What the cache knows about a file's thumbnail.
pub enum Lookup<'a> {
    Image(&'a Image),
    /// Could not be made; show the generic icon.
    Failed,
    /// Not made yet, or made from an older version of the file.
    Missing,
}

/// Thumbnails kept under an entry count and a byte budget; the least recently
/// used go first. A file whose modification time changed is treated as missing.
pub struct Cache {
    slots: HashMap<PathBuf, Slot>,
    clock: u64,
    bytes: usize,
    max_entries: usize,
    max_bytes: usize,
}

impl Cache {
    pub fn new(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            slots: HashMap::new(),
            clock: 0,
            bytes: 0,
            max_entries: max_entries.max(1),
            max_bytes,
        }
    }

    pub fn lookup(&mut self, path: &Path, modified: Option<SystemTime>) -> Lookup<'_> {
        self.clock += 1;
        let tick = self.clock;
        match self.slots.get_mut(path) {
            Some(slot) if slot.modified == modified => {
                slot.tick = tick;
                match &slot.image {
                    Some(image) => Lookup::Image(image),
                    None => Lookup::Failed,
                }
            }
            _ => Lookup::Missing,
        }
    }

    pub fn insert(&mut self, path: PathBuf, modified: Option<SystemTime>, image: Option<Image>) {
        if let Some(old) = self.slots.remove(&path) {
            self.bytes -= slot_bytes(&old);
        }
        let slot = Slot {
            modified,
            tick: self.clock,
            image,
        };
        if slot_bytes(&slot) > self.max_bytes {
            // Larger than the whole budget: not kept (a failure costs no bytes
            // and is kept).
            return;
        }
        self.bytes += slot_bytes(&slot);
        self.slots.insert(path, slot);
        while self.slots.len() > self.max_entries || self.bytes > self.max_bytes {
            let Some(oldest) = self
                .slots
                .iter()
                .min_by_key(|(_, slot)| slot.tick)
                .map(|(path, _)| path.clone())
            else {
                break;
            };
            if let Some(old) = self.slots.remove(&oldest) {
                self.bytes -= slot_bytes(&old);
            }
        }
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    #[cfg(test)]
    pub fn bytes(&self) -> usize {
        self.bytes
    }
}

fn slot_bytes(slot: &Slot) -> usize {
    slot.image.as_ref().map_or(0, |image| {
        image.width() as usize * image.height() as usize * 4
    })
}

/// Requests a thumbnail from a background thread; the result comes back
/// through [`ThumbWorker::results`] and wakes the UI.
pub struct ThumbWorker {
    requests: mpsc::SyncSender<PathBuf>,
    pub results: mpsc::Receiver<(PathBuf, Result<Image, String>)>,
}

impl ThumbWorker {
    pub fn spawn(slot: WakeSlot) -> std::io::Result<Self> {
        // Few requests queue; the view asks again for whatever is still visible.
        let (requests, rx) = mpsc::sync_channel::<PathBuf>(8);
        let (tx, results) = mpsc::channel();
        thread::Builder::new()
            .name("files-thumbs".into())
            .spawn(move || {
                while let Ok(path) = rx.recv() {
                    let result = generate(&path);
                    if tx.send((path, result)).is_err() {
                        break;
                    }
                    worker::wake(&slot);
                }
            })?;
        Ok(Self { requests, results })
    }

    /// Queue `path`. Returns false when the queue is full; the caller asks
    /// again on a later repaint.
    pub fn request(&self, path: PathBuf) -> bool {
        self.requests.try_send(path).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn png_header(width: u32, height: u32) -> Vec<u8> {
        let mut header = PNG_SIGNATURE.to_vec();
        header.extend_from_slice(&13u32.to_be_bytes());
        header.extend_from_slice(b"IHDR");
        header.extend_from_slice(&width.to_be_bytes());
        header.extend_from_slice(&height.to_be_bytes());
        header
    }

    #[test]
    fn only_png_and_svg_are_thumbnailed() {
        assert_eq!(format_of(Path::new("a.PNG")), Some(Format::Png));
        assert_eq!(format_of(Path::new("logo.svg")), Some(Format::Svg));
        assert_eq!(format_of(Path::new("photo.jpg")), None);
        assert_eq!(format_of(Path::new("noext")), None);
    }

    #[test]
    fn png_header_dimensions_are_read_without_decoding() {
        assert_eq!(png_dimensions(&png_header(640, 480)), Some((640, 480)));
        assert_eq!(png_dimensions(b"not a png at all, just text"), None);
        let mut bad = png_header(1, 1);
        bad[12..16].copy_from_slice(b"IDAT");
        assert_eq!(png_dimensions(&bad), None);
    }

    #[test]
    fn oversized_and_pixel_heavy_images_are_refused_before_decoding() {
        let huge_pixels = png_header(10_000, 10_000);
        assert_eq!(
            check_budget(Format::Png, 100, &huge_pixels),
            Err("The image has too many pixels for a thumbnail.")
        );
        assert_eq!(
            check_budget(Format::Png, MAX_FILE_BYTES + 1, &png_header(8, 8)),
            Err("The image is too large for a thumbnail.")
        );
        assert_eq!(
            check_budget(Format::Png, 100, &png_header(0, 5)),
            Err("This image has no pixels.")
        );
        assert_eq!(
            check_budget(Format::Png, 100, b"junk"),
            Err("This PNG is damaged.")
        );
        assert!(check_budget(Format::Png, 100, &png_header(800, 600)).is_ok());
        assert_eq!(
            check_budget(Format::Svg, MAX_SVG_BYTES + 1, b""),
            Err("The SVG is too large for a thumbnail.")
        );
    }

    #[test]
    fn fit_keeps_aspect_and_never_enlarges() {
        assert_eq!(fit(640, 480, 64), (64, 48));
        assert_eq!(fit(480, 640, 64), (48, 64));
        assert_eq!(fit(32, 16, 64), (32, 16));
        assert_eq!(fit(0, 10, 64), (1, 1));
    }

    #[test]
    fn an_svg_becomes_a_thumbnail_and_a_damaged_file_fails_cleanly() {
        let tmp = tempfile::tempdir().unwrap();
        let svg = tmp.path().join("square.svg");
        std::fs::write(
            &svg,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="red"/></svg>"#,
        )
        .unwrap();
        let image = generate(&svg).unwrap();
        assert_eq!((image.width(), image.height()), (THUMB_SIDE, THUMB_SIDE));
        let broken = tmp.path().join("broken.png");
        std::fs::write(&broken, b"definitely not a png").unwrap();
        assert!(generate(&broken).is_err());
        assert!(generate(tmp.path()).is_err(), "a folder is not thumbnailed");
    }

    fn stamp(seconds: u64) -> Option<SystemTime> {
        Some(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
    }

    #[test]
    fn the_cache_is_bounded_by_entries_and_by_bytes_least_recent_first() {
        let mut cache = Cache::new(2, 1_000_000);
        cache.insert(PathBuf::from("a"), stamp(1), None);
        cache.insert(PathBuf::from("b"), stamp(1), None);
        // Touch "a" so "b" is the least recently used.
        assert!(matches!(
            cache.lookup(Path::new("a"), stamp(1)),
            Lookup::Failed
        ));
        cache.insert(PathBuf::from("c"), stamp(1), None);
        assert_eq!(cache.len(), 2);
        assert!(matches!(
            cache.lookup(Path::new("b"), stamp(1)),
            Lookup::Missing
        ));
        assert!(matches!(
            cache.lookup(Path::new("a"), stamp(1)),
            Lookup::Failed
        ));
    }

    #[test]
    fn a_changed_file_is_not_served_from_the_cache() {
        let mut cache = Cache::new(8, 1_000_000);
        cache.insert(PathBuf::from("pic"), stamp(1), None);
        assert!(matches!(
            cache.lookup(Path::new("pic"), stamp(2)),
            Lookup::Missing
        ));
        assert!(matches!(
            cache.lookup(Path::new("pic"), stamp(1)),
            Lookup::Failed
        ));
    }

    #[test]
    fn images_count_their_bytes_against_the_budget() {
        let image = Image::from_rgba(10, 10, vec![0; 10 * 10 * 4]).unwrap();
        let mut cache = Cache::new(8, 10 * 10 * 4 + 10);
        cache.insert(PathBuf::from("one"), stamp(1), Some(image.clone()));
        assert_eq!(cache.bytes(), 400);
        cache.insert(PathBuf::from("two"), stamp(1), Some(image.clone()));
        assert_eq!(cache.len(), 1, "the second image evicts the first");
        assert_eq!(cache.bytes(), 400);
        // An image bigger than the whole budget is not kept at all.
        let mut tiny = Cache::new(8, 100);
        tiny.insert(PathBuf::from("big"), stamp(1), Some(image));
        assert_eq!(tiny.len(), 0);
        assert_eq!(tiny.bytes(), 0);
    }
}
