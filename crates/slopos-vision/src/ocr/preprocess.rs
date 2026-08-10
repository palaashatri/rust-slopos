//! Input preprocessing for the PP-OCR detection and recognition models.
//!
//! Constants follow PaddleOCR `release/2.7` inference defaults (see
//! `tools/infer/predict_det.py`, `tools/infer/predict_rec.py`).

use image::{imageops, RgbImage};

/// Detection-model limit on the longest side, in pixels.
pub const DET_LIMIT_SIDE_LEN: u32 = 960;

/// Detection preprocessing result.
pub struct DetPreprocessed {
    /// `C * H * W` float data in NCHW order, normalized.
    pub data: Vec<f32>,
    /// Model input height.
    pub height: u32,
    /// Model input width.
    pub width: u32,
    /// `(src_h, src_w)` of the original image.
    pub src_h: u32,
    pub src_w: u32,
}

fn resize_to_multiple_of_32(v: f32) -> u32 {
    let snapped = (v / 32.0).round() as i64 * 32;
    snapped.max(32) as u32
}

/// `DetResizeForTest` + `NormalizeImage` + `ToCHWImage` for the detection
/// model. Uses `limit_side_len = 960`, `limit_type = "max"`, BGR input and
/// ImageNet mean/std.
///
/// PaddleOCR's pipeline decodes every image with `cv2.imread` (BGR) and the
/// model's `DecodeImage(img_mode: BGR)` keeps that order, so channel 0 of the
/// model input is Blue. `mean`/`std` are applied elementwise to that BGR
/// layout, so channel 0 (Blue) uses `mean[0]`.
pub fn det_preprocess(img: &RgbImage) -> DetPreprocessed {
    let (src_h, src_w) = (img.height(), img.width());
    let max_side = src_h.max(src_w) as f64;
    let ratio = if max_side > DET_LIMIT_SIDE_LEN as f64 {
        DET_LIMIT_SIDE_LEN as f64 / max_side
    } else {
        1.0
    };
    let resize_h = resize_to_multiple_of_32((src_h as f64 * ratio) as f32);
    let resize_w = resize_to_multiple_of_32((src_w as f64 * ratio) as f32);

    let resized: RgbImage =
        imageops::resize(img, resize_w, resize_h, imageops::FilterType::Triangle);

    let scale = 1.0f32 / 255.0;
    let mean = [0.485f32, 0.456, 0.406];
    let std = [0.229f32, 0.224, 0.225];

    let mut data = Vec::with_capacity((resize_w * resize_h * 3) as usize);
    for y in 0..resize_h {
        for x in 0..resize_w {
            let p = resized.get_pixel(x, y).0;
            // HWC -> CHW in BGR order: output channel 0 = source Blue.
            for (ch, src) in [2usize, 1, 0].into_iter().enumerate() {
                let v = (p[src] as f32 * scale - mean[ch]) / std[ch];
                data.push(v);
            }
        }
    }

    // Reorder HWC -> CHW.
    let (c, h, w) = (3usize, resize_h as usize, resize_w as usize);
    let hwc = std::mem::take(&mut data);
    let mut chw = vec![0f32; c * h * w];
    for (i, out) in chw.iter_mut().enumerate() {
        let ch = i / (h * w);
        let rem = i % (h * w);
        let y = rem / w;
        let x = rem % w;
        *out = hwc[y * w * c + x * c + ch];
    }

    DetPreprocessed {
        data: chw,
        height: resize_h,
        width: resize_w,
        src_h,
        src_w,
    }
}

/// Recognition-model target height.
pub const REC_IMG_H: u32 = 48;
/// Recognition-model default width for a square-ish crop.
pub const REC_IMG_W: u32 = 320;

/// Recognition preprocessing result for a single crop.
pub struct RecPreprocessed {
    /// `1 * 3 * H * W` float data in NCHW order.
    pub data: Vec<f32>,
    pub height: u32,
    pub width: u32,
}

/// `resize_norm_img` for a single text-line crop, following PaddleOCR's
/// `TextRecognizer.resize_norm_img` (the path PP-OCRv4's `SVTR_LCNet` takes,
/// since it is not in the `SVTR`/`SATRN` branch list in `predict_rec.py`).
///
/// The crop is resized to height `REC_IMG_H`, keeping its aspect ratio (width
/// `int(ceil(REC_IMG_H * ratio))`, capped at the padded width), normalized to
/// `[-1, 1]` via `x/255 -> -0.5 -> /0.5`, and zero-padded on the right.
///
/// The padded width defaults to `int(REC_IMG_H * max(REC_IMG_W/REC_IMG_H, ratio))`
/// for dynamic-width models; callers with a fixed-width model pass it via
/// `model_width`. Input is BGR (channel 0 = Blue), matching Paddle's
/// `cv2.imread`-based crop pipeline.
pub fn rec_preprocess(crop: &RgbImage, model_width: Option<u32>) -> RecPreprocessed {
    let h = crop.height() as f64;
    let w = crop.width() as f64;
    let ratio = if h > 0.0 { w / h } else { 1.0 };
    let max_wh_ratio = (REC_IMG_W as f64 / REC_IMG_H as f64).max(ratio);
    let img_w = match model_width {
        Some(fixed) => fixed,
        None => (REC_IMG_H as f64 * max_wh_ratio).round() as u32,
    };
    let img_w = img_w.max(1);

    let natural_w = (REC_IMG_H as f64 * ratio).ceil();
    let resized_w = if natural_w > img_w as f64 {
        img_w
    } else {
        natural_w as u32
    };

    let resized: RgbImage =
        imageops::resize(crop, resized_w, REC_IMG_H, imageops::FilterType::Triangle);

    let mut data = vec![0f32; 3 * REC_IMG_H as usize * img_w as usize];
    for y in 0..REC_IMG_H {
        for x in 0..resized_w {
            let p = resized.get_pixel(x, y).0;
            for (ch, src) in [2usize, 1, 0].into_iter().enumerate() {
                let v = p[src] as f32 / 127.5 - 1.0;
                let idx = ch * (REC_IMG_H as usize * img_w as usize)
                    + (y as usize) * img_w as usize
                    + x as usize;
                data[idx] = v;
            }
        }
    }

    RecPreprocessed {
        data,
        height: REC_IMG_H,
        width: img_w,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_img(w: u32, h: u32, rgb: [u8; 3]) -> RgbImage {
        let mut img = RgbImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                img.put_pixel(x, y, image::Rgb(rgb));
            }
        }
        img
    }

    #[test]
    fn det_resize_small_image_keeps_size() {
        let img = solid_img(100, 60, [255, 255, 255]);
        let p = det_preprocess(&img);
        assert_eq!(p.width, 96); // 100 -> snapped to 96 (mult of 32)
        assert_eq!(p.height, 64); // 60 -> snapped to 64
        assert_eq!(p.src_w, 100);
        assert_eq!(p.src_h, 60);
        assert_eq!(p.data.len(), (3 * 96 * 64) as usize);
    }

    #[test]
    fn det_large_image_is_downscaled_to_960() {
        let img = solid_img(2000, 1000, [255, 255, 255]);
        let p = det_preprocess(&img);
        assert!(p.width <= 960);
        assert!(p.height <= 960);
        assert!((p.width as f64 / p.height as f64 - 2.0).abs() < 0.2);
    }

    #[test]
    fn det_bgr_channel_order() {
        // A 32x32 pure-blue image is not resized (no snapping change).
        let img = solid_img(32, 32, [0, 0, 255]);
        let p = det_preprocess(&img);
        assert_eq!(p.width, 32);
        assert_eq!(p.height, 32);
        // CHW layout: channel 0 is Blue (value ~ 2.25 for (255/255 - .485)/.229),
        // channels 1/2 (G/R) sit near their ImageNet means -> negative.
        let plane = (32 * 32) as usize;
        for i in 0..plane {
            assert!(
                (p.data[i] - ((1.0 - 0.485) / 0.229)).abs() < 1e-4,
                "ch0 should be blue"
            );
            assert!(
                p.data[plane + i] < -1.0,
                "ch1 (green) should be far below zero"
            );
            assert!(
                p.data[2 * plane + i] < -1.0,
                "ch2 (red) should be far below zero"
            );
        }
    }

    #[test]
    fn rec_normalization_range() {
        let crop = solid_img(100, 48, [255, 255, 255]);
        let p = rec_preprocess(&crop, Some(320));
        assert_eq!(p.width, 320);
        assert_eq!(p.height, 48);
        // Aspect-preserved width: ceil(48 * 100/48) = 100. Resized region is
        // white -> (255/127.5 - 1) = 1.0; the rest is zero-padded.
        let plane = (48 * 320) as usize;
        for ch in 0..3 {
            for y in 0..48usize {
                for x in 0..100usize {
                    let v = p.data[ch * plane + y * 320 + x];
                    assert!(
                        (v - 1.0).abs() < 1e-4,
                        "expected 1.0 at ch={ch} ({x},{y}), got {v}"
                    );
                }
                for x in 100..320usize {
                    assert_eq!(p.data[ch * plane + y * 320 + x], 0.0, "padding must be 0.0");
                }
            }
        }
    }

    #[test]
    fn rec_dynamic_width_scales_with_ratio() {
        // Ratio 4.0 stays at the 320 floor (max(320/48, 4.0) = 320/48).
        let narrow = rec_preprocess(&solid_img(480, 120, [0, 0, 0]), None);
        assert_eq!(narrow.width, 320);
        assert_eq!(narrow.data.len(), (3 * 48 * 320) as usize);
        // Black -> (0/127.5 - 1) = -1.0 in the resized region (ceil(48*4)=192),
        // padding stays 0.0.
        let plane = (48 * 320) as usize;
        for ch in 0..3 {
            for y in 0..48usize {
                for x in 0..192usize {
                    let v = narrow.data[ch * plane + y * 320 + x];
                    assert!(
                        (v + 1.0).abs() < 1e-4,
                        "expected -1.0 at ch={ch} ({x},{y}), got {v}"
                    );
                }
                for x in 192..320usize {
                    assert_eq!(narrow.data[ch * plane + y * 320 + x], 0.0);
                }
            }
        }

        // Ratio 16.7 grows the padded width beyond 320.
        let wide = rec_preprocess(&solid_img(800, 48, [0, 0, 0]), None);
        assert_eq!(wide.width, 800);
        assert_eq!(wide.data.len(), (3 * 48 * 800) as usize);
    }

    #[test]
    fn rec_bgr_channel_order() {
        // Pure blue in the source RgbImage; BGR output puts it in channel 0.
        let crop = solid_img(48, 48, [0, 0, 255]);
        let p = rec_preprocess(&crop, Some(320));
        let plane = (48 * 320) as usize;
        for y in 0..48usize {
            for x in 0..48usize {
                let idx = y * 320 + x;
                assert!((p.data[idx] - 1.0).abs() < 1e-4, "ch0 should be blue ~ 1.0");
                assert!(
                    (p.data[plane + idx] + 1.0).abs() < 1e-4,
                    "ch1 (green) should be -1.0"
                );
                assert!(
                    (p.data[2 * plane + idx] + 1.0).abs() < 1e-4,
                    "ch2 (red) should be -1.0"
                );
            }
        }
    }
}
