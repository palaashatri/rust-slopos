//! Subject mask post-processing.
//!
//! A raw segmentation model emits a per-pixel probability map. This module
//! turns it into a clean alpha mask: hard thresholding, connected-component
//! filtering (largest subject, de-speckle), background-hole filling, and
//! optional edge feathering. All passes operate in row-major order so the
//! pipeline stays deterministic.

use crate::error::VisionError;
use crate::types::{MaskPostProcessOptions, SubjectMask};

/// Post-process a raw probability map into a subject mask.
pub fn postprocess_mask(
    prob: &[f32],
    width: u32,
    height: u32,
    options: &MaskPostProcessOptions,
) -> Result<SubjectMask, VisionError> {
    if width == 0 || height == 0 {
        return Err(VisionError::InvalidOutput("empty mask dimensions".into()));
    }
    let n = width as usize * height as usize;
    if prob.len() != n {
        return Err(VisionError::InvalidOutput(format!(
            "probability map has {} values but the mask is {width}x{height} ({n} values)",
            prob.len()
        )));
    }

    let threshold = options.threshold.clamp(0.0, 1.0);
    let mut binary = vec![0u8; n];
    for (i, &p) in prob.iter().enumerate() {
        binary[i] = if p >= threshold { 255 } else { 0 };
    }

    let mut binary = filter_components(binary, width, height, options);

    if options.fill_holes_below > 0 {
        fill_holes(&mut binary, width, height, options.fill_holes_below);
    }

    let alpha = if options.feather_radius > 0 {
        feather(&binary, width, height, options.feather_radius)
    } else {
        binary
    };

    Ok(SubjectMask {
        width,
        height,
        alpha,
        confidence: None,
    })
}

/// Connected-component labeling (8-connectivity). Returns the per-pixel label
/// (`0` = background) and the area of each label.
fn label_components(mask: &[u8], w: usize, h: usize) -> (Vec<u32>, Vec<u32>) {
    let mut labels = vec![0u32; w * h];
    let mut areas: Vec<u32> = Vec::new();
    let mut next = 1u32;
    let mut stack: Vec<(i64, i64)> = Vec::new();
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let idx = (y * w as i64 + x) as usize;
            if mask[idx] == 0 || labels[idx] != 0 {
                continue;
            }
            let label = next;
            next += 1;
            let mut area = 0u32;
            stack.push((x, y));
            labels[idx] = label;
            while let Some((cx, cy)) = stack.pop() {
                area += 1;
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        let nx = cx + dx;
                        let ny = cy + dy;
                        if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                            continue;
                        }
                        let nidx = (ny * w as i64 + nx) as usize;
                        if mask[nidx] != 0 && labels[nidx] == 0 {
                            labels[nidx] = label;
                            stack.push((nx, ny));
                        }
                    }
                }
            }
            areas.push(area);
        }
    }
    (labels, areas)
}

/// Remove small foreground components and (optionally) keep only the largest.
fn filter_components(
    binary: Vec<u8>,
    width: u32,
    height: u32,
    options: &MaskPostProcessOptions,
) -> Vec<u8> {
    let (w, h) = (width as usize, height as usize);
    let (labels, areas) = label_components(&binary, w, h);
    if areas.is_empty() {
        return binary;
    }
    let largest = areas
        .iter()
        .enumerate()
        .max_by_key(|&(_, &a)| a)
        .map(|(i, _)| i as u32 + 1)
        .unwrap_or(0);
    let min_area = options.remove_small_components_below;
    let mut out = binary;
    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            let label = labels[idx];
            if label == 0 {
                continue;
            }
            let area = areas[(label - 1) as usize];
            let keep = area >= min_area && (!options.keep_largest_component || label == largest);
            if !keep {
                out[idx] = 0;
            }
        }
    }
    out
}

/// Fill background holes (enclosed zero-regions) whose area is below `max_area`.
fn fill_holes(binary: &mut [u8], width: u32, height: u32, max_area: u32) {
    let (w, h) = (width as usize, height as usize);
    let background: Vec<u8> = binary
        .iter()
        .map(|&v| if v == 0 { 255 } else { 0 })
        .collect();
    let (labels, areas) = label_components(&background, w, h);
    if areas.is_empty() {
        return;
    }
    let mut touches_border = vec![false; areas.len()];
    for x in 0..w {
        for &y in &[0usize, h.saturating_sub(1)] {
            let l = labels[y * w + x];
            if l > 0 {
                touches_border[(l - 1) as usize] = true;
            }
        }
    }
    for y in 0..h {
        for &x in &[0usize, w.saturating_sub(1)] {
            let l = labels[y * w + x];
            if l > 0 {
                touches_border[(l - 1) as usize] = true;
            }
        }
    }
    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            let label = labels[idx];
            if label == 0 {
                continue;
            }
            let area = areas[(label - 1) as usize];
            if area < max_area && !touches_border[(label - 1) as usize] {
                binary[idx] = 255;
            }
        }
    }
}

/// Box-blur the mask to soften edges. Interior pixels stay solid.
fn feather(binary: &[u8], width: u32, height: u32, radius: u32) -> Vec<u8> {
    let (w, h) = (width as usize, height as usize);
    let (w1, h1) = (w + 1, h + 1);
    let mut integral = vec![0u64; w1 * h1];
    for y in 0..h {
        for x in 0..w {
            let v = if binary[y * w + x] > 0 { 255u64 } else { 0 };
            integral[(y + 1) * w1 + (x + 1)] =
                v + integral[y * w1 + (x + 1)] + integral[(y + 1) * w1 + x] - integral[y * w1 + x];
        }
    }
    let r = radius as usize;
    let mut out = vec![0u8; w * h];
    for y in 0..h {
        for x in 0..w {
            let x0 = x.saturating_sub(r);
            let y0 = y.saturating_sub(r);
            let x1 = (x + r + 1).min(w);
            let y1 = (y + r + 1).min(h);
            let sum = integral[y1 * w1 + x1] as i64
                - integral[y0 * w1 + x1] as i64
                - integral[y1 * w1 + x0] as i64
                + integral[y0 * w1 + x0] as i64;
            let area = ((x1 - x0) * (y1 - y0)) as i64;
            let value = (sum as f64 / area as f64 + 0.5).max(0.0) as u32;
            out[y * w + x] = value.min(255) as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prob_map(w: u32, h: u32, fg: impl Fn(u32, u32) -> bool) -> Vec<f32> {
        let mut v = vec![0f32; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                v[(y * w + x) as usize] = if fg(x, y) { 1.0 } else { 0.0 };
            }
        }
        v
    }

    #[test]
    fn threshold_produces_binary_alpha() {
        let p = prob_map(4, 4, |x, y| x == 0 && y == 0);
        let m = postprocess_mask(&p, 4, 4, &Default::default()).unwrap();
        assert_eq!(m.alpha.iter().filter(|&&a| a == 255).count(), 1);
        assert_eq!(m.width, 4);
        assert_eq!(m.height, 4);
    }

    #[test]
    fn keeps_largest_component() {
        let p = prob_map(10, 10, |x, y| (x < 4 && y < 4) || (x == 9 && y == 9));
        let m = postprocess_mask(&p, 10, 10, &Default::default()).unwrap();
        let fg: Vec<(usize, usize)> = m
            .alpha
            .iter()
            .enumerate()
            .filter(|&(_, &a)| a == 255)
            .map(|(i, _)| (i % 10, i / 10))
            .collect();
        assert_eq!(fg.len(), 16, "only the 4x4 component should remain");
        assert!(!fg.contains(&(9, 9)));
    }

    #[test]
    fn removes_small_components() {
        let p = prob_map(10, 10, |x, y| (x == 0 && y == 0) || (x >= 5 && y >= 5));
        let opts = MaskPostProcessOptions {
            remove_small_components_below: 10,
            keep_largest_component: false,
            ..Default::default()
        };
        let m = postprocess_mask(&p, 10, 10, &opts).unwrap();
        let fg = m.alpha.iter().filter(|&&a| a == 255).count();
        assert_eq!(fg, 25);
    }

    #[test]
    fn fills_enclosed_holes() {
        // A solid 6x6 block with a 2x2 hole in the middle.
        let p = prob_map(8, 8, |x, y| {
            (1..7).contains(&x)
                && (1..7).contains(&y)
                && !((3..5).contains(&x) && (3..5).contains(&y))
        });
        let opts = MaskPostProcessOptions {
            fill_holes_below: 100,
            ..Default::default()
        };
        let m = postprocess_mask(&p, 8, 8, &opts).unwrap();
        let filled = m.alpha[(4 * 8 + 4) as usize];
        assert_eq!(filled, 255);
        // The outside ring stays background.
        assert_eq!(m.alpha[0], 0);
    }

    #[test]
    fn feather_softens_but_keeps_interior() {
        let p = prob_map(20, 20, |x, y| (5..15).contains(&x) && (5..15).contains(&y));
        let opts = MaskPostProcessOptions {
            feather_radius: 2,
            ..Default::default()
        };
        let m = postprocess_mask(&p, 20, 20, &opts).unwrap();
        assert_eq!(m.alpha[(10 * 20 + 10) as usize], 255);
        // A pixel one step outside the original hard edge gets partial alpha.
        let just_outside = m.alpha[(4 * 20 + 10) as usize];
        assert!(just_outside > 0 && just_outside < 255, "got {just_outside}");
    }

    #[test]
    fn dimension_mismatch_is_error() {
        assert!(matches!(
            postprocess_mask(&[0.0; 4], 3, 3, &Default::default()),
            Err(VisionError::InvalidOutput(_))
        ));
    }
}
