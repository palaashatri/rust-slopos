//! Alpha compositing for subject cutouts.
//!
//! The segmentation pipeline produces a source-resolution mask; compositing
//! stamps it onto the source image's alpha channel so the subject can be
//! exported as a transparent-background image.

use crate::error::VisionError;
use crate::types::SubjectMask;
use image::RgbaImage;

/// Apply `mask` to `image`'s alpha channel, producing a cutout.
///
/// The mask must match the image's dimensions. The RGB channels are copied
/// unchanged; only the alpha channel is replaced.
pub fn composite_subject(image: &RgbaImage, mask: &SubjectMask) -> Result<RgbaImage, VisionError> {
    let (w, h) = (image.width(), image.height());
    if mask.width != w || mask.height != h {
        return Err(VisionError::InvalidOutput(format!(
            "mask dimensions {}x{} do not match image {}x{}",
            mask.width, mask.height, w, h
        )));
    }
    let n = (w * h) as usize;
    if mask.alpha.len() != n {
        return Err(VisionError::InvalidOutput(format!(
            "mask has {} alpha values but the image has {n} pixels",
            mask.alpha.len()
        )));
    }

    let mut out = image.clone();
    for (px, a) in out.pixels_mut().zip(mask.alpha.iter()) {
        px.0[3] = *a;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn composite_sets_alpha_channel() {
        let mut img = RgbaImage::new(2, 1);
        img.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
        img.put_pixel(1, 0, Rgba([0, 255, 0, 255]));
        let mask = SubjectMask {
            width: 2,
            height: 1,
            alpha: vec![0, 128],
            confidence: None,
        };
        let out = composite_subject(&img, &mask).unwrap();
        assert_eq!(out.get_pixel(0, 0).0, [255, 0, 0, 0]);
        assert_eq!(out.get_pixel(1, 0).0, [0, 255, 0, 128]);
    }

    #[test]
    fn composite_rejects_mismatched_dims() {
        let img = RgbaImage::new(4, 4);
        let mask = SubjectMask {
            width: 3,
            height: 4,
            alpha: vec![255; 12],
            confidence: None,
        };
        assert!(matches!(
            composite_subject(&img, &mask),
            Err(VisionError::InvalidOutput(_))
        ));
    }
}
