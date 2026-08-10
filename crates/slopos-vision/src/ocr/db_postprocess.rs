//! DB (Differentiable Binarization) text-detection post-processing.
//!
//! Reimplements PaddleOCR's `DBPostProcess` (with the CLI defaults
//! `thresh = 0.3`, `box_thresh = 0.6`, `unclip_ratio = 1.5`) plus the
//! `filter_tag_det_res` / `order_points_clockwise` cleanup and the
//! `get_mini_boxes` corner ordering.

use crate::geometry::{
    expand_polygon, get_mini_boxes, min_area_rect, order_points_clockwise, polygon_area,
    polygon_perimeter, Point, Quad,
};

pub const DB_THRESHOLD: f32 = 0.3;
pub const DB_BOX_THRESH: f32 = 0.6;
pub const DB_UNCLIP_RATIO: f64 = 1.5;
pub const DB_MAX_CANDIDATES: usize = 1000;

/// A detected text box in source-image coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct DetBox {
    pub quad: Quad,
    pub score: f32,
}

/// 8-connected-component labeling returning components as pixel lists.
fn label_components(mask: &[bool], ow: usize, oh: usize) -> Vec<Vec<(usize, usize)>> {
    let mut visited = vec![false; ow * oh];
    let mut components = Vec::new();
    for start in 0..(ow * oh) {
        if visited[start] || !mask[start] {
            continue;
        }
        let mut stack = vec![start];
        visited[start] = true;
        let mut component = Vec::new();
        while let Some(idx) = stack.pop() {
            component.push((idx % ow, idx / ow));
            let x = idx % ow;
            let y = idx / ow;
            for dy in -1i64..=1 {
                for dx in -1i64..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let nx = x as i64 + dx;
                    let ny = y as i64 + dy;
                    if nx < 0 || ny < 0 || nx >= ow as i64 || ny >= oh as i64 {
                        continue;
                    }
                    let nidx = ny as usize * ow + nx as usize;
                    if !visited[nidx] && mask[nidx] {
                        visited[nidx] = true;
                        stack.push(nidx);
                    }
                }
            }
        }
        if !component.is_empty() {
            components.push(component);
        }
        if components.len() >= DB_MAX_CANDIDATES {
            break;
        }
    }
    components
}

/// Boundary pixels of a component (any pixel with a neighbor outside the
/// component or at the image edge).
fn boundary_points(
    component: &[(usize, usize)],
    mask: &[bool],
    ow: usize,
    oh: usize,
) -> Vec<Point> {
    let mut pts = Vec::new();
    for &(x, y) in component {
        let mut on_boundary = false;
        'neighbors: for dy in -1i64..=1 {
            for dx in -1i64..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = x as i64 + dx;
                let ny = y as i64 + dy;
                if nx < 0 || ny < 0 || nx >= ow as i64 || ny >= oh as i64 {
                    on_boundary = true;
                    break 'neighbors;
                }
                if !mask[ny as usize * ow + nx as usize] {
                    on_boundary = true;
                    break 'neighbors;
                }
            }
        }
        if on_boundary {
            pts.push([x as f64, y as f64]);
        }
    }
    pts
}

/// Mean probability over the pixels inside `quad`.
fn box_score(prob: &[f32], ow: usize, oh: usize, quad: &Quad) -> f32 {
    let (x0, y0, x1, y1) = quad.bbox();
    let x0 = x0.floor().max(0.0) as usize;
    let y0 = y0.floor().max(0.0) as usize;
    let x1 = x1.ceil().min(ow as f64 - 1.0) as usize;
    let y1 = y1.ceil().min(oh as f64 - 1.0) as usize;
    if x1 < x0 || y1 < y0 {
        return 0.0;
    }
    let mut sum = 0f32;
    let mut count = 0u64;
    for y in y0..=y1 {
        for x in x0..=x1 {
            if quad.contains([x as f64, y as f64]) {
                sum += prob[y * ow + x];
                count += 1;
            }
        }
    }
    if count == 0 {
        0.0
    } else {
        sum / count as f32
    }
}

fn scale_quad(quad: &Quad, ow: f64, oh: f64, src_w: u32, src_h: u32) -> Quad {
    let map = |p: Point| -> Point {
        let x = (p[0] / ow * src_w as f64).round().clamp(0.0, src_w as f64);
        let y = (p[1] / oh * src_h as f64).round().clamp(0.0, src_h as f64);
        [x, y]
    };
    Quad {
        p: [
            map(quad.p[0]),
            map(quad.p[1]),
            map(quad.p[2]),
            map(quad.p[3]),
        ],
    }
}

/// Run the full DB post-processing pipeline over the model's probability map.
///
/// `prob` holds `oh * ow` values in `[0, 1]`. `src_w`/`src_h` are the original
/// source-image dimensions the map was resized from.
pub fn db_postprocess(prob: &[f32], oh: usize, ow: usize, src_w: u32, src_h: u32) -> Vec<DetBox> {
    let mask: Vec<bool> = prob.iter().map(|&p| p > DB_THRESHOLD).collect();
    let components = label_components(&mask, ow, oh);

    let mut boxes = Vec::new();
    for component in components {
        let pts = boundary_points(&component, &mask, ow, oh);
        if pts.len() < 3 {
            continue;
        }
        let rect = min_area_rect(&pts);
        let (quad, min_side) = get_mini_boxes(&rect);
        if min_side < 3.0 {
            continue;
        }

        let score = box_score(prob, ow, oh, &quad);
        if score < DB_BOX_THRESH {
            continue;
        }

        // Unclip: offset the quad outward by area * ratio / perimeter.
        let area = polygon_area(&quad.p);
        let perim = polygon_perimeter(&quad.p);
        let distance = if perim > 0.0 {
            area * DB_UNCLIP_RATIO / perim
        } else {
            0.0
        };
        let expanded = expand_polygon(&quad.p, distance);
        if expanded.len() < 3 {
            continue;
        }
        let rect2 = min_area_rect(&expanded);
        let (quad2, min_side2) = get_mini_boxes(&rect2);
        if min_side2 < 5.0 {
            continue;
        }

        let scaled = scale_quad(&quad2, ow as f64, oh as f64, src_w, src_h);
        let ordered = order_points_clockwise(scaled.p);

        // Reject degenerate boxes (same top edge or left edge).
        let d_top = dist(ordered.p[0], ordered.p[1]);
        let d_left = dist(ordered.p[0], ordered.p[3]);
        if d_top <= 3.0 || d_left <= 3.0 {
            continue;
        }

        boxes.push(DetBox {
            quad: ordered,
            score,
        });
    }
    boxes
}

fn dist(a: Point, b: Point) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// Sort detected boxes into reading order (top-to-bottom, left-to-right),
/// replicating PaddleOCR's `sorted_boxes`.
pub fn sorted_boxes(mut boxes: Vec<DetBox>) -> Vec<DetBox> {
    boxes.sort_by(|a, b| {
        let ay = a.quad.p[0][1];
        let by = b.quad.p[0][1];
        ay.partial_cmp(&by)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                a.quad.p[0][0]
                    .partial_cmp(&b.quad.p[0][0])
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });
    let n = boxes.len();
    for i in 0..n.saturating_sub(1) {
        let mut j = i as isize;
        while j >= 0 {
            let jj = j as usize;
            let same_row = (boxes[jj + 1].quad.p[0][1] - boxes[jj].quad.p[0][1]).abs() < 10.0;
            let swapped_order = boxes[jj + 1].quad.p[0][0] < boxes[jj].quad.p[0][0];
            if same_row && swapped_order {
                boxes.swap(jj, jj + 1);
                j -= 1;
            } else {
                break;
            }
        }
    }
    boxes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_quad(x: f64, y: f64, w: f64, h: f64) -> Quad {
        Quad {
            p: [[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
        }
    }

    #[test]
    fn box_score_averages_inside() {
        let ow = 10usize;
        let oh = 10usize;
        let prob = vec![0.5f32; ow * oh];
        let quad = make_quad(2.0, 2.0, 4.0, 4.0);
        assert!((box_score(&prob, ow, oh, &quad) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn db_postprocess_finds_solid_box() {
        // 100x100 map with a solid square of high probability.
        let ow = 100usize;
        let oh = 100usize;
        let mut prob = vec![0.0f32; ow * oh];
        for y in 30..70 {
            for x in 30..70 {
                prob[y * ow + x] = 0.9;
            }
        }
        let boxes = db_postprocess(&prob, oh, ow, 100, 100);
        assert_eq!(boxes.len(), 1);
        let b = &boxes[0];
        let (x0, y0, x1, y1) = b.quad.bbox();
        assert!((10.0..=25.0).contains(&x0), "x0 = {x0}");
        assert!((75.0..=90.0).contains(&x1), "x1 = {x1}");
        assert!((10.0..=25.0).contains(&y0), "y0 = {y0}");
        assert!((75.0..=90.0).contains(&y1), "y1 = {y1}");
        assert!(b.score > 0.85);
    }

    #[test]
    fn db_postprocess_ignores_noise() {
        // A single isolated high-probability pixel is too small to keep.
        let ow = 50usize;
        let oh = 50usize;
        let mut prob = vec![0.0f32; ow * oh];
        prob[25 * ow + 25] = 0.9;
        let boxes = db_postprocess(&prob, oh, ow, 50, 50);
        assert!(boxes.is_empty());
    }

    #[test]
    fn db_postprocess_scales_to_source() {
        // Map is half the source resolution; the box must scale by 2x.
        let ow = 50usize;
        let oh = 50usize;
        let mut prob = vec![0.0f32; ow * oh];
        for y in 15..35 {
            for x in 15..35 {
                prob[y * ow + x] = 0.9;
            }
        }
        let boxes = db_postprocess(&prob, oh, ow, 100, 100);
        assert_eq!(boxes.len(), 1);
        let (x0, y0, x1, y1) = boxes[0].quad.bbox();
        assert!((15.0..=30.0).contains(&x0), "x0 = {x0}");
        assert!((70.0..=85.0).contains(&x1), "x1 = {x1}");
        assert!((15.0..=30.0).contains(&y0), "y0 = {y0}");
        assert!((70.0..=85.0).contains(&y1), "y1 = {y1}");
    }

    #[test]
    fn sorted_boxes_orders_by_row_then_column() {
        let mut boxes = vec![
            DetBox {
                quad: make_quad(50.0, 0.0, 10.0, 5.0),
                score: 1.0,
            },
            DetBox {
                quad: make_quad(0.0, 0.0, 10.0, 5.0),
                score: 1.0,
            },
            DetBox {
                quad: make_quad(0.0, 20.0, 10.0, 5.0),
                score: 1.0,
            },
            DetBox {
                quad: make_quad(50.0, 20.0, 10.0, 5.0),
                score: 1.0,
            },
        ];
        boxes = sorted_boxes(boxes);
        // Reading order: (0,0), (50,0), (0,20), (50,20)
        assert_eq!(boxes[0].quad.p[0][0], 0.0);
        assert_eq!(boxes[0].quad.p[0][1], 0.0);
        assert_eq!(boxes[1].quad.p[0][0], 50.0);
        assert_eq!(boxes[1].quad.p[0][1], 0.0);
        assert_eq!(boxes[2].quad.p[0][1], 20.0);
        assert_eq!(boxes[2].quad.p[0][0], 0.0);
        assert_eq!(boxes[3].quad.p[0][0], 50.0);
        assert_eq!(boxes[3].quad.p[0][1], 20.0);
    }
}
