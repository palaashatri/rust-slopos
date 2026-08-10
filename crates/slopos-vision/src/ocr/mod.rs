//! The OCR engine: PP-OCRv4 detection + recognition models running through
//! the pure-Rust `rten` runtime, with Apache-2.0 weights.

pub mod ctc;
pub mod db_postprocess;
pub mod preprocess;

use crate::error::VisionError;
use crate::geometry::{order_points_clockwise, warp_perspective, Quad};
use crate::types::{OcrOptions, OcrResult, PixelRect, TextLine, TextWord};
use ctc::{decode_ctc, CharDict};
use db_postprocess::{db_postprocess, sorted_boxes, DetBox};
use image::RgbImage;
use rten::{Model, RunOptions, ThreadPool, Value};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Recognition output length cap (time steps). PP-OCR recognition models emit
/// a fixed number of steps; anything far above 100 indicates a corrupt output.
const MAX_REC_TIME_STEPS: usize = 128;

/// The minimum recognition confidence that survives the drop-score filter.
const DROP_SCORE: f32 = 0.5;

pub struct OcrEngine {
    det: Model,
    rec: Model,
    rec_model_width: Option<u32>,
    dict: CharDict,
    pool: Arc<ThreadPool>,
}

impl OcrEngine {
    /// Load the detection model, recognition model, and dictionary.
    ///
    /// Model files are expected to have been hash-verified by the caller
    /// against the model manifest.
    pub fn load(det_path: &Path, rec_path: &Path, dict_path: &Path) -> Result<Self, VisionError> {
        let det = Model::load_file(det_path)
            .map_err(|err| VisionError::ModelLoad(format!("{}: {err}", det_path.display())))?;
        let rec = Model::load_file(rec_path)
            .map_err(|err| VisionError::ModelLoad(format!("{}: {err}", rec_path.display())))?;

        if det.input_ids().len() != 1 || det.output_ids().len() != 1 {
            return Err(VisionError::InvalidOutput(
                "detection model must have exactly one input and one output".into(),
            ));
        }
        if rec.input_ids().len() != 1 || rec.output_ids().len() != 1 {
            return Err(VisionError::InvalidOutput(
                "recognition model must have exactly one input and one output".into(),
            ));
        }

        let rec_model_width = match rec.input_shape(0) {
            Some(shape) if shape.len() == 4 => match shape[3] {
                rten::Dimension::Fixed(w) => Some(w as u32),
                _ => None,
            },
            _ => None,
        };

        let dict = CharDict::load(dict_path).map_err(|err| {
            VisionError::Unsupported(format!("recognition dictionary could not be loaded: {err}"))
        })?;
        log::info!(
            "OCR engine loaded: {} classes, rec model width = {:?}",
            dict.num_classes(),
            rec_model_width
        );

        Ok(Self {
            det,
            rec,
            rec_model_width,
            dict,
            pool: Arc::new(ThreadPool::with_num_threads(1)),
        })
    }

    /// Number of classes the recognition model emits. Exposed for validation
    /// and capability reporting.
    pub fn num_classes(&self) -> usize {
        self.dict.num_classes()
    }

    /// Run a 4D f32 model and return `(shape, data)`.
    fn run(
        &self,
        model: &Model,
        input: Vec<f32>,
        shape: [usize; 4],
    ) -> Result<([usize; 4], Vec<f32>), VisionError> {
        let value = Value::from_shape(shape, input).map_err(|err| {
            VisionError::InvalidOutput(format!("input construction failed: {err}"))
        })?;
        let opts = RunOptions::default().with_thread_pool(Some(self.pool.clone()));
        let out = model
            .run_one(value.into(), Some(opts))
            .map_err(|err| VisionError::Inference(err.to_string()))?;
        out.into_shape_vec::<f32, 4>()
            .map_err(|err| VisionError::InvalidOutput(format!("unexpected output: {err}")))
    }

    /// Run a model whose output is 3D `[1, T, C]` and return `(shape, data)`.
    /// The recognition model emits logits as `[1, time_steps, classes]`.
    fn run_3d(
        &self,
        model: &Model,
        input: Vec<f32>,
        shape: [usize; 4],
    ) -> Result<([usize; 3], Vec<f32>), VisionError> {
        let value = Value::from_shape(shape, input).map_err(|err| {
            VisionError::InvalidOutput(format!("input construction failed: {err}"))
        })?;
        let opts = RunOptions::default().with_thread_pool(Some(self.pool.clone()));
        let out = model
            .run_one(value.into(), Some(opts))
            .map_err(|err| VisionError::Inference(err.to_string()))?;
        out.into_shape_vec::<f32, 3>()
            .map_err(|err| VisionError::InvalidOutput(format!("unexpected output: {err}")))
    }

    /// Detect text-line boxes in `image`.
    fn detect_boxes(&self, image: &RgbImage) -> Result<Vec<DetBox>, VisionError> {
        let (src_h, src_w) = (image.height(), image.width());
        let det = preprocess::det_preprocess(image);
        let (h, w) = (det.height as usize, det.width as usize);
        let ([_, _, oh, ow], prob) = self.run(&self.det, det.data, [1, 3, h, w])?;
        if oh == 0 || ow == 0 {
            return Err(VisionError::InvalidOutput(format!(
                "detection output has empty spatial dims: {oh}x{ow}"
            )));
        }
        let boxes = db_postprocess(&prob, oh, ow, src_w, src_h);
        Ok(boxes)
    }

    /// Crop a detected quad, rectify it, and recognize the text line.
    fn recognize_quad(
        &self,
        rgba: &image::RgbaImage,
        quad: &Quad,
    ) -> Result<(String, f32), VisionError> {
        let w = quad.width();
        let h = quad.height();
        let w = w.round().max(1.0) as u32;
        let h = h.round().max(1.0) as u32;
        let mut crop = warp_perspective(rgba, quad, w, h);
        if h as f64 >= w as f64 * 1.5 {
            crop = crate::geometry::rotate90_ccw(&crop);
        }
        let crop_rgb = image::DynamicImage::ImageRgba8(crop).to_rgb8();
        let rec = preprocess::rec_preprocess(&crop_rgb, self.rec_model_width);
        let (rh, rw) = (rec.height as usize, rec.width as usize);
        let ([_, _, _], logits) = self.run_3d(&self.rec, rec.data, [1, 3, rh, rw])?;

        let time_steps = logits.len() / self.dict.num_classes();
        if time_steps > MAX_REC_TIME_STEPS || time_steps == 0 {
            return Err(VisionError::InvalidOutput(format!(
                "recognition output has {time_steps} time steps; expected 1..={MAX_REC_TIME_STEPS}"
            )));
        }
        let (text, score) = decode_ctc(&logits, time_steps, self.dict.num_classes(), &self.dict);
        Ok((text, score))
    }

    /// Run OCR over the image.
    pub fn extract_text(
        &self,
        image: &image::DynamicImage,
        options: &OcrOptions,
    ) -> Result<OcrResult, VisionError> {
        check_cancel(options.cancel.as_deref())?;
        let (image_width, image_height) = (image.width(), image.height());
        let rgba = image.to_rgba8();
        let rgb = image.to_rgb8();

        let boxes = self.detect_boxes(&rgb)?;
        let boxes = sorted_boxes(boxes);

        let mut words = Vec::with_capacity(boxes.len());
        for det in &boxes {
            check_cancel(options.cancel.as_deref())?;
            let (text, score) = self.recognize_quad(&rgba, &det.quad)?;
            if text.is_empty() || score < options.min_confidence.max(DROP_SCORE) {
                continue;
            }
            let (x0, y0, x1, y1) = det.quad.bbox();
            let bounds = PixelRect::new(
                x0.floor().max(0.0) as u32,
                y0.floor().max(0.0) as u32,
                (x1.ceil() - x0.floor()).max(0.0) as u32,
                (y1.ceil() - y0.floor()).max(0.0) as u32,
            );
            words.push(TextWord {
                text,
                bounds,
                confidence: Some(score),
            });
        }

        let lines = group_lines(words);
        Ok(OcrResult {
            image_width,
            image_height,
            lines,
        })
    }
}

/// Group reading-ordered words into lines by vertical overlap.
fn group_lines(words: Vec<TextWord>) -> Vec<TextLine> {
    let mut lines: Vec<TextLine> = Vec::new();
    for word in words {
        if let Some(line) = lines.last_mut() {
            let overlap = vertical_overlap(&line.bounds, &word.bounds);
            let line_h = line.bounds.height.max(1) as f32;
            if overlap / line_h >= 0.5 {
                line.words.push(word.clone());
                line.text.push(' ');
                line.text.push_str(&word.text);
                line.bounds = PixelRect::union(&line.bounds, &word.bounds);
                line.confidence = Some(mean_confidence(&line.words));
                continue;
            }
        }
        let confidence = word.confidence;
        lines.push(TextLine {
            text: word.text.clone(),
            bounds: word.bounds,
            confidence,
            words: vec![word],
        });
    }
    lines
}

fn mean_confidence(words: &[TextWord]) -> f32 {
    let values: Vec<f32> = words.iter().filter_map(|w| w.confidence).collect();
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f32>() / values.len() as f32
    }
}

fn vertical_overlap(a: &PixelRect, b: &PixelRect) -> f32 {
    let a_top = a.y as f32;
    let a_bot = (a.y + a.height) as f32;
    let b_top = b.y as f32;
    let b_bot = (b.y + b.height) as f32;
    (a_bot.min(b_bot) - a_top.max(b_top)).max(0.0)
}

fn check_cancel(cancel: Option<&AtomicBool>) -> Result<(), VisionError> {
    if cancel.map(|c| c.load(Ordering::Relaxed)).unwrap_or(false) {
        return Err(VisionError::Cancelled);
    }
    Ok(())
}

// Keep `order_points_clockwise` referenced so degenerate-quad ordering stays
// explicit and tested.
#[allow(unused)]
fn _canonical_quad(pts: [crate::geometry::Point; 4]) -> Quad {
    order_points_clockwise(pts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertical_overlap_counts() {
        let a = PixelRect::new(0, 0, 10, 20);
        let b = PixelRect::new(0, 10, 10, 20);
        assert!((vertical_overlap(&a, &b) - 10.0).abs() < 1e-6);
        let c = PixelRect::new(0, 100, 10, 20);
        assert_eq!(vertical_overlap(&a, &c), 0.0);
    }

    #[test]
    fn group_lines_splits_far_rows() {
        let words = vec![
            TextWord {
                text: "one".into(),
                bounds: PixelRect::new(0, 0, 20, 10),
                confidence: Some(0.9),
            },
            TextWord {
                text: "two".into(),
                bounds: PixelRect::new(0, 100, 20, 10),
                confidence: Some(0.9),
            },
        ];
        let lines = group_lines(words);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "one");
        assert_eq!(lines[1].text, "two");
    }

    #[test]
    fn group_lines_joins_same_row() {
        let words = vec![
            TextWord {
                text: "one".into(),
                bounds: PixelRect::new(0, 0, 20, 10),
                confidence: Some(0.9),
            },
            TextWord {
                text: "two".into(),
                bounds: PixelRect::new(20, 0, 20, 10),
                confidence: Some(0.8),
            },
        ];
        let lines = group_lines(words);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "one two");
        assert_eq!(lines[0].words.len(), 2);
    }

    #[test]
    fn check_cancel_flags() {
        let flag = Arc::new(AtomicBool::new(false));
        assert!(check_cancel(Some(&flag)).is_ok());
        flag.store(true, Ordering::Relaxed);
        assert!(matches!(
            check_cancel(Some(&flag)),
            Err(VisionError::Cancelled)
        ));
        assert!(check_cancel(None).is_ok());
    }
}
