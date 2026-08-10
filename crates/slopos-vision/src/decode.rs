//! Image decoding with allocation guards.
//!
//! Decoding is guarded against decompression bombs: the encoded dimensions
//! are read from the format header *before* a full decode, and decoding is
//! refused when the declared pixel count exceeds `max_pixels`.

use crate::error::VisionError;
use image::{DynamicImage, ImageReader};
use std::fs;
use std::io::Cursor;
use std::path::Path;

/// Maximum encoded image size accepted by default for file-based decode.
pub const DEFAULT_MAX_ENCODED_INPUT_BYTES: u64 = 64 * 1024 * 1024;

fn check_encoded_input_size(encoded_bytes: u64, max_encoded_bytes: u64) -> Result<(), VisionError> {
    if encoded_bytes > max_encoded_bytes {
        return Err(VisionError::EncodedImageTooLarge {
            max_bytes: max_encoded_bytes,
            actual_bytes: encoded_bytes,
        });
    }
    Ok(())
}

pub(crate) fn read_image_limited(
    path: &Path,
    max_encoded_bytes: u64,
    max_pixels: u64,
) -> Result<DynamicImage, VisionError> {
    let encoded_bytes = fs::metadata(path)?.len();
    check_encoded_input_size(encoded_bytes, max_encoded_bytes)?;

    let data = fs::read(path)?;
    check_encoded_input_size(data.len() as u64, max_encoded_bytes)?;
    decode_image_limited(&data, max_pixels)
}

/// Decode `data` as an image, refusing images whose declared dimensions
/// exceed `max_pixels` (a decompression-bomb guard).
pub fn decode_image_limited(data: &[u8], max_pixels: u64) -> Result<DynamicImage, VisionError> {
    let reader = ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .map_err(|err| VisionError::Decode(err.to_string()))?;

    if reader.format().is_none() {
        return Err(VisionError::UnsupportedFormat(
            "could not determine image format".to_string(),
        ));
    }

    let (width, height) = ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .map_err(|err| VisionError::Decode(err.to_string()))?
        .into_dimensions()
        .map_err(|err| VisionError::Decode(err.to_string()))?;
    let pixels = (width as u64).saturating_mul(height as u64);
    if pixels > max_pixels {
        return Err(VisionError::ImageTooLarge {
            max: max_pixels,
            pixels,
        });
    }

    let image = reader
        .decode()
        .map_err(|err| VisionError::Decode(err.to_string()))?;
    Ok(image)
}

/// Number of source pixels above which an inference job is refused by default.
pub const DEFAULT_MAX_SOURCE_PIXELS: u64 = 40_000_000;

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;
    use tempfile::tempdir;

    fn encode_png(w: u32, h: u32) -> Vec<u8> {
        let img = RgbaImage::from_pixel(w, h, image::Rgba([255, 0, 0, 255]));
        let mut out = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut out);
        img.write_with_encoder(encoder).unwrap();
        out
    }

    #[test]
    fn decodes_valid_png() {
        let data = encode_png(8, 8);
        let img = decode_image_limited(&data, 1000).unwrap();
        assert_eq!(img.width(), 8);
        assert_eq!(img.height(), 8);
    }

    #[test]
    fn rejects_oversized_dimensions_before_decode() {
        // A tiny valid PNG whose declared dims (8x8) fit within the guard.
        let data = encode_png(8, 8);
        assert!(decode_image_limited(&data, 63).is_err());
    }

    #[test]
    fn rejects_unknown_format() {
        let data = b"not an image at all, definitely not one";
        match decode_image_limited(data, 1000) {
            Err(VisionError::UnsupportedFormat(_)) | Err(VisionError::Decode(_)) => {}
            other => panic!("expected format error, got {other:?}"),
        }
    }

    #[test]
    fn rejects_garbage_bytes() {
        let data = vec![0u8; 128];
        assert!(decode_image_limited(&data, 1000).is_err());
    }

    #[test]
    fn rejects_encoded_file_before_full_read() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("oversized.bin");
        std::fs::write(&path, [0u8; 4]).unwrap();

        let err = read_image_limited(&path, 3, 1000).unwrap_err();
        assert!(matches!(
            err,
            VisionError::EncodedImageTooLarge {
                max_bytes: 3,
                actual_bytes: 4
            }
        ));
    }
}
