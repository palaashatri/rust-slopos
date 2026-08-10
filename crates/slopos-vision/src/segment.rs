//! Subject segmentation via a U<sup>2</sup>-Net-style model.
//!
//! The default architecture is `u2netp` (the lightweight U<sup>2</sup>-Net
//! variant) running through `rten`, following rembg's `U2netpSession`
//! preprocessing exactly: inputs are stretched to 320×320 with LANCZOS,
//! divided by the image maximum, normalized with ImageNet statistics, the
//! first (side-output) channel of the probability map is min-max normalized,
//! and the mask is upsampled back to the source resolution before post-
//! processing. `u2netp.onnx` exposes seven outputs (six side outputs and the
//! fused output); rembg uses output index 0, which we replicate here.

use crate::error::VisionError;
use crate::mask;
use crate::types::{SegmentationOptions, SubjectMask};
use image::{imageops, DynamicImage, RgbImage};
use rten::{Model, NodeId, RunOptions, ThreadPool, Value, ValueOrView};
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;

/// Model input size (square).
pub const SEG_IMG_SIZE: u32 = 320;

pub struct SegmentEngine {
    model: Model,
    input_id: NodeId,
    pool: Arc<ThreadPool>,
}

impl SegmentEngine {
    /// Load the segmentation model. The file is expected to have been
    /// hash-verified by the caller.
    pub fn load(path: &Path) -> Result<Self, VisionError> {
        let model = Model::load_file(path)
            .map_err(|err| VisionError::ModelLoad(format!("{}: {err}", path.display())))?;
        if model.input_ids().is_empty() || model.output_ids().is_empty() {
            return Err(VisionError::InvalidOutput(
                "segmentation model must have at least one input and one output".into(),
            ));
        }
        let input_id = model.input_ids()[0];
        Ok(Self {
            model,
            input_id,
            pool: Arc::new(ThreadPool::with_num_threads(1)),
        })
    }

    /// Segment the main subject of `image`, returning a source-resolution mask.
    pub fn segment(
        &self,
        image: &DynamicImage,
        options: &SegmentationOptions,
    ) -> Result<SubjectMask, VisionError> {
        if options
            .cancel
            .as_deref()
            .map(|c| c.load(Ordering::Relaxed))
            .unwrap_or(false)
        {
            return Err(VisionError::Cancelled);
        }
        let (src_w, src_h) = (image.width(), image.height());
        let rgb = image.to_rgb8();

        let resized: RgbImage = imageops::resize(
            &rgb,
            SEG_IMG_SIZE,
            SEG_IMG_SIZE,
            imageops::FilterType::Lanczos3,
        );

        // rembg: im_ary = im_ary / max(im_ary, 1e-6), then (x - mean) / std.
        let max_val = resized
            .pixels()
            .map(|p| p.0.iter().map(|&v| v as f32).fold(0.0f32, f32::max))
            .fold(0.0f32, f32::max)
            .max(1e-6);
        let mean = [0.485f32, 0.456, 0.406];
        let std = [0.229f32, 0.224, 0.225];
        let side = SEG_IMG_SIZE as usize;
        let mut data = vec![0f32; 3 * side * side];
        for y in 0..side {
            for x in 0..side {
                let p = resized.get_pixel(x as u32, y as u32).0;
                for ch in 0..3 {
                    let v = (p[ch] as f32 / max_val - mean[ch]) / std[ch];
                    data[ch * side * side + y * side + x] = v;
                }
            }
        }

        let value = Value::from_shape([1, 3, side, side], data).map_err(|err| {
            VisionError::InvalidOutput(format!("input construction failed: {err}"))
        })?;
        let opts = RunOptions::default().with_thread_pool(Some(self.pool.clone()));
        let output_id = self.model.output_ids()[0];
        let mut results = self
            .model
            .run(
                vec![(self.input_id, ValueOrView::Value(value))],
                &[output_id],
                Some(opts),
            )
            .map_err(|err| VisionError::Inference(err.to_string()))?;
        let out = take_segmentation_output(&mut results)?;
        let ([_, channels, oh, ow], prob) = out
            .into_shape_vec::<f32, 4>()
            .map_err(|err| VisionError::InvalidOutput(format!("unexpected output: {err}")))?;
        validate_segmentation_output(channels, oh, ow, prob.len())?;

        // rembg min-max normalizes the raw side output before scaling it back.
        let mi = prob.iter().cloned().fold(f32::INFINITY, f32::min);
        let ma = prob.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let normalized: Vec<f32> = if (ma - mi).abs() < 1e-9 {
            vec![0.0; prob.len()]
        } else {
            prob.iter().map(|&v| (v - mi) / (ma - mi)).collect()
        };

        let src_prob = resize_map(&normalized, ow, oh, src_w, src_h);
        mask::postprocess_mask(&src_prob, src_w, src_h, &options.mask_post)
    }
}

fn take_segmentation_output<T>(results: &mut Vec<T>) -> Result<T, VisionError> {
    results
        .pop()
        .ok_or_else(|| VisionError::InvalidOutput("segmentation returned no output".into()))
}

fn validate_segmentation_output(
    channels: usize,
    height: usize,
    width: usize,
    values: usize,
) -> Result<(), VisionError> {
    if channels != 1 {
        return Err(VisionError::InvalidOutput(format!(
            "segmentation output has {channels} channels; expected 1"
        )));
    }
    if height == 0 || width == 0 || values == 0 {
        return Err(VisionError::InvalidOutput(format!(
            "segmentation output has invalid dimensions: channels={channels}, height={height}, width={width}"
        )));
    }
    Ok(())
}

/// Bilinear-resize a row-major `[ch][cw]` single-channel map to `[nh][nw]`.
fn resize_map(src: &[f32], cw: usize, ch: usize, nw: u32, nh: u32) -> Vec<f32> {
    if nw == 0 || nh == 0 {
        return Vec::new();
    }
    let scale_x = cw as f64 / nw as f64;
    let scale_y = ch as f64 / nh as f64;
    let mut out = vec![0f32; (nw as usize) * (nh as usize)];
    for y in 0..nh as usize {
        let fy = y as f64 * scale_y;
        let y0 = (fy.floor() as usize).min(ch - 1);
        let y1 = (fy.ceil() as usize).min(ch - 1);
        let wy = (fy - y0 as f64) as f32;
        for x in 0..nw as usize {
            let fx = x as f64 * scale_x;
            let x0 = (fx.floor() as usize).min(cw - 1);
            let x1 = (fx.ceil() as usize).min(cw - 1);
            let wx = (fx - x0 as f64) as f32;
            let v00 = src[y0 * cw + x0];
            let v01 = src[y0 * cw + x1];
            let v10 = src[y1 * cw + x0];
            let v11 = src[y1 * cw + x1];
            let top = v00 * (1.0 - wx) + v01 * wx;
            let bot = v10 * (1.0 - wx) + v11 * wx;
            out[y * nw as usize + x] = top * (1.0 - wy) + bot * wy;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_segmentation_output_is_structured_error() {
        let mut results: Vec<u8> = Vec::new();
        let error = take_segmentation_output(&mut results).unwrap_err();
        assert!(matches!(
            error,
            VisionError::InvalidOutput(message) if message == "segmentation returned no output"
        ));
    }

    #[test]
    fn zero_sized_segmentation_output_is_structured_error() {
        for (height, width, values) in [(0, 4, 0), (4, 0, 0), (4, 4, 0)] {
            let error = validate_segmentation_output(1, height, width, values).unwrap_err();
            assert!(
                matches!(error, VisionError::InvalidOutput(message) if message.contains("invalid dimensions"))
            );
        }
    }

    #[test]
    fn resize_map_upsamples_exactly() {
        // 2x2 constant map upsampled to 4x4 stays constant.
        let src = vec![0.5f32; 4];
        let out = resize_map(&src, 2, 2, 4, 4);
        assert_eq!(out.len(), 16);
        assert!(out.iter().all(|&v| (v - 0.5).abs() < 1e-6));
    }

    #[test]
    fn resize_map_upsample_corner() {
        // Single bright pixel in a 2x2 map; the top-left corner of the output
        // should be near 1.0, the bottom-right near 0.0.
        let src = vec![1.0f32, 0.0, 0.0, 0.0];
        let out = resize_map(&src, 2, 2, 4, 4);
        assert!((out[0] - 1.0).abs() < 1e-6);
        assert!(out[15] < 0.1);
    }

    #[test]
    fn resize_map_downsample_matches() {
        let src: Vec<f32> = (0..16).map(|i| i as f32 / 16.0).collect();
        let out = resize_map(&src, 4, 4, 2, 2);
        assert_eq!(out.len(), 4);
        // Point sampling: output (0,0) maps to src (0,0), output (1,1) to src (2,2).
        assert!((out[0] - src[0]).abs() < 1e-6);
        assert!((out[3] - src[2 * 4 + 2]).abs() < 1e-6);
    }
}
