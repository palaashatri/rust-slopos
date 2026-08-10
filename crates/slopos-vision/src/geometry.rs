//! 2D geometry helpers used by OCR and segmentation post-processing.
//!
//! These reimplement the small, well-defined geometric pieces the PaddleOCR
//! pipeline relies on (rotated minimum-area rectangles, polygon expansion for
//! unclipping, homography-based perspective crops) in pure Rust.

use image::RgbaImage;

pub type Point = [f64; 2];

/// A quadrilateral with corners ordered top-left, top-right, bottom-right,
/// bottom-left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quad {
    pub p: [Point; 4],
}

impl Quad {
    /// Axis-aligned bounding box.
    pub fn bbox(&self) -> (f64, f64, f64, f64) {
        let xs = self.p.map(|p| p[0]);
        let ys = self.p.map(|p| p[1]);
        let x0 = xs.iter().cloned().fold(f64::INFINITY, f64::min);
        let x1 = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let y0 = ys.iter().cloned().fold(f64::INFINITY, f64::min);
        let y1 = ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        (x0, y0, x1, y1)
    }

    pub fn width(&self) -> f64 {
        let (x0, _, x1, _) = self.bbox();
        (x1 - x0).max(0.0)
    }

    pub fn height(&self) -> f64 {
        let (_, y0, _, y1) = self.bbox();
        (y1 - y0).max(0.0)
    }

    /// Whether `p` is inside the (convex) quad.
    pub fn contains(&self, p: Point) -> bool {
        let n = self.p.len();
        for i in 0..n {
            let a = self.p[i];
            let b = self.p[(i + 1) % n];
            let cross = (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
            if cross < 0.0 {
                return false;
            }
        }
        true
    }
}

fn cross(o: Point, a: Point, b: Point) -> f64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

/// Convex hull via Andrew's monotone chain. Returns hull vertices in
/// counter-clockwise order, without closing the loop.
pub fn convex_hull(mut points: Vec<Point>) -> Vec<Point> {
    points.sort_by(|a, b| {
        a[0].partial_cmp(&b[0])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a[1].partial_cmp(&b[1]).unwrap_or(std::cmp::Ordering::Equal))
    });
    points.dedup();
    if points.len() <= 2 {
        return points;
    }
    let mut lower: Vec<Point> = Vec::new();
    for &p in &points {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<Point> = Vec::new();
    for &p in points.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// A rotated minimum-area rectangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RotatedRect {
    pub center: Point,
    pub width: f64,
    pub height: f64,
    /// Angle (radians) of the `width` axis relative to the x-axis.
    pub angle: f64,
}

impl RotatedRect {
    /// The four corner points, in arbitrary order.
    pub fn corners(&self) -> [Point; 4] {
        let (ux, uy) = (self.angle.cos(), self.angle.sin());
        let (nx, ny) = (-uy, ux);
        let hw = self.width / 2.0;
        let hh = self.height / 2.0;
        [
            [
                self.center[0] - hw * ux - hh * nx,
                self.center[1] - hw * uy - hh * ny,
            ],
            [
                self.center[0] + hw * ux - hh * nx,
                self.center[1] + hw * uy - hh * ny,
            ],
            [
                self.center[0] + hw * ux + hh * nx,
                self.center[1] + hw * uy + hh * ny,
            ],
            [
                self.center[0] - hw * ux + hh * nx,
                self.center[1] - hw * uy + hh * ny,
            ],
        ]
    }

    pub fn min_side(&self) -> f64 {
        self.width.min(self.height)
    }
}

/// Minimum-area rectangle of a point set (rotating-calipers scan over the
/// convex hull edges). Equivalent to OpenCV `minAreaRect`.
pub fn min_area_rect(points: &[Point]) -> RotatedRect {
    let hull = convex_hull(points.to_vec());
    let n = hull.len();
    match n {
        0 => RotatedRect {
            center: [0.0, 0.0],
            width: 0.0,
            height: 0.0,
            angle: 0.0,
        },
        1 => RotatedRect {
            center: hull[0],
            width: 0.0,
            height: 0.0,
            angle: 0.0,
        },
        2 => {
            let dx = hull[1][0] - hull[0][0];
            let dy = hull[1][1] - hull[0][1];
            let len = dx.hypot(dy);
            RotatedRect {
                center: [
                    (hull[0][0] + hull[1][0]) / 2.0,
                    (hull[0][1] + hull[1][1]) / 2.0,
                ],
                width: len,
                height: 0.0,
                angle: dy.atan2(dx),
            }
        }
        _ => {
            let mut best_area = f64::INFINITY;
            let mut best: Option<(f64, f64, f64, f64, Point, f64)> = None;
            for i in 0..n {
                let a = hull[i];
                let b = hull[(i + 1) % n];
                let edge = [b[0] - a[0], b[1] - a[1]];
                let len = edge[0].hypot(edge[1]);
                if len < 1e-9 {
                    continue;
                }
                let (ux, uy) = (edge[0] / len, edge[1] / len);
                let (nx, ny) = (-uy, ux);
                let mut min_u = f64::INFINITY;
                let mut max_u = f64::NEG_INFINITY;
                let mut min_n = f64::INFINITY;
                let mut max_n = f64::NEG_INFINITY;
                for &p in &hull {
                    let du = (p[0] - a[0]) * ux + (p[1] - a[1]) * uy;
                    let dn = (p[0] - a[0]) * nx + (p[1] - a[1]) * ny;
                    min_u = min_u.min(du);
                    max_u = max_u.max(du);
                    min_n = min_n.min(dn);
                    max_n = max_n.max(dn);
                }
                let area = (max_u - min_u) * (max_n - min_n);
                if area < best_area {
                    best_area = area;
                    best = Some((min_u, max_u, min_n, max_n, a, uy.atan2(ux)));
                }
            }
            let (min_u, max_u, min_n, max_n, a, angle) = best.unwrap();
            let (ux, uy) = (angle.cos(), angle.sin());
            let (nx, ny) = (-uy, ux);
            let cu = (min_u + max_u) / 2.0;
            let cn = (min_n + max_n) / 2.0;
            RotatedRect {
                center: [a[0] + cu * ux + cn * nx, a[1] + cu * uy + cn * ny],
                width: (max_u - min_u).max(0.0),
                height: (max_n - min_n).max(0.0),
                angle,
            }
        }
    }
}

/// Order the four corners of a rotated rectangle into the canonical
/// top-left / top-right / bottom-right / bottom-left order, replicating
/// PaddleOCR's `get_mini_boxes`.
pub fn get_mini_boxes(rect: &RotatedRect) -> (Quad, f64) {
    let corners = rect.corners();
    let mut points = corners.to_vec();
    points.sort_by(|a, b| a[0].partial_cmp(&b[0]).unwrap_or(std::cmp::Ordering::Equal));

    let (index_1, index_4) = if points[1][1] > points[0][1] {
        (0, 1)
    } else {
        (1, 0)
    };
    let (index_2, index_3) = if points[3][1] > points[2][1] {
        (2, 3)
    } else {
        (3, 2)
    };

    let quad = Quad {
        p: [
            points[index_1],
            points[index_2],
            points[index_3],
            points[index_4],
        ],
    };
    (quad, rect.min_side())
}

/// Reorder a quad so points are clockwise starting at the top-left, matching
/// PaddleOCR's `order_points_clockwise`.
pub fn order_points_clockwise(pts: [Point; 4]) -> Quad {
    let mut rect = [Point::default(); 4];
    let sums: Vec<f64> = pts.iter().map(|p| p[0] + p[1]).collect();
    let diffs: Vec<f64> = pts.iter().map(|p| p[1] - p[0]).collect();
    let mut min_s = 0usize;
    let mut max_s = 0usize;
    let mut min_d = 0usize;
    let mut max_d = 0usize;
    for i in 1..4 {
        if sums[i] < sums[min_s] {
            min_s = i;
        }
        if sums[i] > sums[max_s] {
            max_s = i;
        }
        if diffs[i] < diffs[min_d] {
            min_d = i;
        }
        if diffs[i] > diffs[max_d] {
            max_d = i;
        }
    }
    rect[0] = pts[min_s]; // top-left
    rect[2] = pts[max_s]; // bottom-right
    rect[1] = pts[min_d]; // top-right (min y - x)
    rect[3] = pts[max_d]; // bottom-left
    Quad { p: rect }
}

/// Signed shoelace area.
fn signed_polygon_area(pts: &[Point]) -> f64 {
    let mut area = 0.0;
    let n = pts.len();
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        area += a[0] * b[1] - b[0] * a[1];
    }
    area / 2.0
}

/// Polygon area (absolute value).
pub fn polygon_area(pts: &[Point]) -> f64 {
    signed_polygon_area(pts).abs()
}

/// Polygon perimeter.
pub fn polygon_perimeter(pts: &[Point]) -> f64 {
    let n = pts.len();
    let mut perim = 0.0;
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        perim += (b[0] - a[0]).hypot(b[1] - a[1]);
    }
    perim
}

/// Expand a convex polygon outward by `distance` by offsetting each edge and
/// intersecting adjacent offset lines (an approximation of PyclipperOffset
/// with `JT_ROUND` on convex quads). Returns the expanded polygon vertices.
pub fn expand_polygon(pts: &[Point], distance: f64) -> Vec<Point> {
    let n = pts.len();
    if n == 0 {
        return Vec::new();
    }
    let cx = pts.iter().map(|p| p[0]).sum::<f64>() / n as f64;
    let cy = pts.iter().map(|p| p[1]).sum::<f64>() / n as f64;

    // Per-edge outward normals and line offsets.
    let mut normals = Vec::with_capacity(n);
    let mut offsets = Vec::with_capacity(n);
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        let edge = [b[0] - a[0], b[1] - a[1]];
        let len = edge[0].hypot(edge[1]);
        if len < 1e-12 {
            normals.push([1.0, 0.0]);
            offsets.push(f64::INFINITY);
            continue;
        }
        let mut nx = -edge[1] / len;
        let mut ny = edge[0] / len;
        let mid = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
        if (mid[0] - cx) * nx + (mid[1] - cy) * ny < 0.0 {
            nx = -nx;
            ny = -ny;
        }
        normals.push([nx, ny]);
        offsets.push(nx * a[0] + ny * a[1] + distance);
    }

    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let (n1x, n1y) = (normals[i][0], normals[i][1]);
        let d1 = offsets[i];
        let (n2x, n2y) = (normals[(i + 1) % n][0], normals[(i + 1) % n][1]);
        let d2 = offsets[(i + 1) % n];
        let det = n1x * n2y - n1y * n2x;
        if det.abs() < 1e-12 {
            out.push(pts[(i + 1) % n]);
            continue;
        }
        let x = (d1 * n2y - n1y * d2) / det;
        let y = (n1x * d2 - d1 * n2x) / det;
        out.push([x, y]);
    }
    out
}

/// Solve a small square linear system via Gaussian elimination with partial
/// pivoting.
fn solve_linear(a: &mut [Vec<f64>], b: &mut [f64]) -> Option<Vec<f64>> {
    let n = b.len();
    for col in 0..n {
        let mut pivot = col;
        for row in col + 1..n {
            if a[row][col].abs() > a[pivot][col].abs() {
                pivot = row;
            }
        }
        if a[pivot][col].abs() < 1e-12 {
            return None;
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        for row in 0..n {
            if row != col {
                let factor = a[row][col] / a[col][col];
                let a_col_val = a[col].clone();
                for (a_row_k, a_col_k) in a[row][col..n].iter_mut().zip(&a_col_val[col..n]) {
                    *a_row_k -= factor * a_col_k;
                }
                b[row] -= factor * b[col];
            }
        }
    }
    let mut x = vec![0.0; n];
    for i in 0..n {
        x[i] = b[i] / a[i][i];
    }
    Some(x)
}

/// Homography mapping `src` quad corners onto `dst` quad corners (4-point
/// DLT with `h33 = 1`).
pub fn homography(src: &[Point; 4], dst: &[Point; 4]) -> Option<[[f64; 3]; 3]> {
    let mut a: Vec<Vec<f64>> = Vec::with_capacity(8);
    let mut b = Vec::with_capacity(8);
    for i in 0..4 {
        let (x, y) = (src[i][0], src[i][1]);
        let (dx, dy) = (dst[i][0], dst[i][1]);
        a.push(vec![x, y, 1.0, 0.0, 0.0, 0.0, -dx * x, -dx * y]);
        b.push(dx);
        a.push(vec![0.0, 0.0, 0.0, x, y, 1.0, -dy * x, -dy * y]);
        b.push(dy);
    }
    let h = solve_linear(&mut a, &mut b)?;
    Some([[h[0], h[1], h[2]], [h[3], h[4], h[5]], [h[6], h[7], 1.0]])
}

fn invert_homography(h: &[[f64; 3]; 3]) -> Option<[[f64; 3]; 3]> {
    let det = h[0][0] * (h[1][1] * h[2][2] - h[1][2] * h[2][1])
        - h[0][1] * (h[1][0] * h[2][2] - h[1][2] * h[2][0])
        + h[0][2] * (h[1][0] * h[2][1] - h[1][1] * h[2][0]);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let co =
        |r0: usize, c0: usize, r1: usize, c1: usize| h[r0][c0] * h[r1][c1] - h[r0][c1] * h[r1][c0];
    Some([
        [
            inv * co(1, 1, 2, 2),
            inv * co(0, 2, 2, 1),
            inv * co(0, 1, 1, 2),
        ],
        [
            inv * co(1, 2, 2, 0),
            inv * co(0, 0, 2, 2),
            inv * co(0, 2, 1, 0),
        ],
        [
            inv * co(1, 0, 2, 1),
            inv * co(0, 1, 2, 0),
            inv * co(0, 0, 1, 1),
        ],
    ])
}

/// Warp the region defined by `src` (source-image coordinates) into an
/// axis-aligned `width x height` output image, using inverse bilinear
/// sampling. Equivalent to OpenCV `warpPerspective` + `getPerspectiveTransform`.
pub fn warp_perspective(img: &RgbaImage, src: &Quad, width: u32, height: u32) -> RgbaImage {
    let dst = Quad {
        p: [
            [0.0, 0.0],
            [width as f64, 0.0],
            [width as f64, height as f64],
            [0.0, height as f64],
        ],
    };
    let h = match homography(&src.p, &dst.p) {
        Some(h) => h,
        None => {
            log::warn!("warp_perspective: degenerate homography");
            return img.clone();
        }
    };
    let h_inv = match invert_homography(&h) {
        Some(h) => h,
        None => {
            log::warn!("warp_perspective: non-invertible homography");
            return img.clone();
        }
    };

    let (iw, ih) = (img.width(), img.height());
    let mut out = RgbaImage::new(width, height);
    let apply = |h: &[[f64; 3]; 3], x: f64, y: f64| -> (f64, f64) {
        let w = h[2][0] * x + h[2][1] * y + h[2][2];
        if w.abs() < 1e-12 {
            return (f64::NAN, f64::NAN);
        }
        (
            (h[0][0] * x + h[0][1] * y + h[0][2]) / w,
            (h[1][0] * x + h[1][1] * y + h[1][2]) / w,
        )
    };

    for y in 0..height {
        for x in 0..width {
            let (sx, sy) = apply(&h_inv, x as f64 + 0.5, y as f64 + 0.5);
            if !(sx.is_finite() && sy.is_finite()) {
                continue;
            }
            if sx < 0.0 || sy < 0.0 || sx > (iw as f64 - 1.0) || sy > (ih as f64 - 1.0) {
                continue;
            }
            let x0 = sx.floor() as i64;
            let y0 = sy.floor() as i64;
            let fx = sx - x0 as f64;
            let fy = sy - y0 as f64;
            let sample = |px: i64, py: i64| -> [u8; 4] {
                let px = px.clamp(0, iw as i64 - 1) as u32;
                let py = py.clamp(0, ih as i64 - 1) as u32;
                img.get_pixel(px, py).0
            };
            let c00 = sample(x0, y0);
            let c10 = sample(x0 + 1, y0);
            let c01 = sample(x0, y0 + 1);
            let c11 = sample(x0 + 1, y0 + 1);
            let mut px = [0u8; 4];
            for ch in 0..4 {
                let top = c00[ch] as f64 * (1.0 - fx) + c10[ch] as f64 * fx;
                let bot = c01[ch] as f64 * (1.0 - fx) + c11[ch] as f64 * fx;
                px[ch] = (top * (1.0 - fy) + bot * fy).round().clamp(0.0, 255.0) as u8;
            }
            out.put_pixel(x, y, image::Rgba(px));
        }
    }
    out
}

/// Rotate an image 90 degrees counter-clockwise (used to rectify vertically
/// oriented text crops).
pub fn rotate90_ccw(img: &RgbaImage) -> RgbaImage {
    let (w, h) = (img.width(), img.height());
    let mut out = RgbaImage::new(h, w);
    for y in 0..h {
        for x in 0..w {
            // (x, y) -> (y, w - 1 - x)
            out.put_pixel(y, w - 1 - x, *img.get_pixel(x, y));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect_pts(cx: f64, cy: f64, w: f64, h: f64) -> Vec<Point> {
        vec![
            [cx - w / 2.0, cy - h / 2.0],
            [cx + w / 2.0, cy - h / 2.0],
            [cx + w / 2.0, cy + h / 2.0],
            [cx - w / 2.0, cy + h / 2.0],
        ]
    }

    #[test]
    fn convex_hull_of_rectangle() {
        let hull = convex_hull(rect_pts(0.0, 0.0, 10.0, 4.0));
        assert_eq!(hull.len(), 4);
    }

    #[test]
    fn min_area_rect_of_axis_aligned_rectangle() {
        let rect = min_area_rect(&rect_pts(5.0, 2.0, 10.0, 4.0));
        assert!((rect.width - 10.0).abs() < 1e-6, "width {}", rect.width);
        assert!((rect.height - 4.0).abs() < 1e-6, "height {}", rect.height);
        assert!((rect.center[0] - 5.0).abs() < 1e-6);
        assert!((rect.center[1] - 2.0).abs() < 1e-6);
    }

    #[test]
    fn min_area_rect_of_rotated_rectangle() {
        // Rotate a 10x4 rect by 30 degrees.
        let theta = 30f64.to_radians();
        let pts: Vec<Point> = rect_pts(0.0, 0.0, 10.0, 4.0)
            .iter()
            .map(|p| {
                [
                    p[0] * theta.cos() - p[1] * theta.sin(),
                    p[0] * theta.sin() + p[1] * theta.cos(),
                ]
            })
            .collect();
        let rect = min_area_rect(&pts);
        assert!((rect.width - 10.0).abs() < 1e-6, "width {}", rect.width);
        assert!((rect.height - 4.0).abs() < 1e-6, "height {}", rect.height);
    }

    #[test]
    fn get_mini_boxes_orders_corners() {
        let rect = min_area_rect(&rect_pts(10.0, 10.0, 20.0, 6.0));
        let (quad, min_side) = get_mini_boxes(&rect);
        assert!((min_side - 6.0).abs() < 1e-6);
        // Top-left corner has the minimum x+y.
        let sums: Vec<f64> = quad.p.iter().map(|p| p[0] + p[1]).collect();
        let min = sums.iter().cloned().fold(f64::INFINITY, f64::min);
        assert!((sums[0] - min).abs() < 1e-6);
        // Bottom-right has the maximum x+y.
        let max = sums.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        assert!((sums[2] - max).abs() < 1e-6);
    }

    #[test]
    fn polygon_area_and_perimeter() {
        let pts = rect_pts(0.0, 0.0, 10.0, 4.0);
        assert!((polygon_area(&pts) - 40.0).abs() < 1e-6);
        assert!((polygon_perimeter(&pts) - 28.0).abs() < 1e-6);
    }

    #[test]
    fn expand_polygon_grows() {
        let pts = rect_pts(0.0, 0.0, 10.0, 4.0);
        let expanded = expand_polygon(&pts, 2.0);
        assert_eq!(expanded.len(), 4);
        assert!(polygon_area(&expanded) > polygon_area(&pts));
    }

    #[test]
    fn quad_contains() {
        let quad = Quad {
            p: [[0.0, 0.0], [10.0, 0.0], [10.0, 5.0], [0.0, 5.0]],
        };
        assert!(quad.contains([5.0, 2.0]));
        assert!(!quad.contains([11.0, 2.0]));
        assert!(!quad.contains([5.0, -1.0]));
    }

    #[test]
    fn homography_identity() {
        let quad = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let h = homography(&quad, &quad).unwrap();
        for (i, p) in quad.iter().enumerate() {
            let w = h[2][0] * p[0] + h[2][1] * p[1] + h[2][2];
            let x = (h[0][0] * p[0] + h[0][1] * p[1] + h[0][2]) / w;
            let y = (h[1][0] * p[0] + h[1][1] * p[1] + h[1][2]) / w;
            assert!((x - p[0]).abs() < 1e-6, "corner {i}");
            assert!((y - p[1]).abs() < 1e-6, "corner {i}");
        }
    }

    #[test]
    fn warp_perspective_output_size() {
        let mut img = RgbaImage::new(20, 20);
        for y in 0..20 {
            for x in 0..20 {
                img.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
            }
        }
        let src = Quad {
            p: [[2.0, 2.0], [18.0, 2.0], [18.0, 18.0], [2.0, 18.0]],
        };
        let out = warp_perspective(&img, &src, 16, 16);
        assert_eq!(out.width(), 16);
        assert_eq!(out.height(), 16);
    }

    #[test]
    fn rotate90_ccw_swaps_dims() {
        let mut img = RgbaImage::new(3, 2);
        for y in 0..2 {
            for x in 0..3 {
                img.put_pixel(x, y, image::Rgba([x as u8, y as u8, 0, 255]));
            }
        }
        let out = rotate90_ccw(&img);
        assert_eq!(out.width(), 2);
        assert_eq!(out.height(), 3);
        // CCW rotation: out(0,0) came from img(w-1, 0) = (2, 0).
        assert_eq!(out.get_pixel(0, 0).0, [2, 0, 0, 255]);
        // out(1,0) came from img(2, 1).
        assert_eq!(out.get_pixel(1, 0).0, [2, 1, 0, 255]);
        // out(0, 2) came from img(0, 0).
        assert_eq!(out.get_pixel(0, 2).0, [0, 0, 0, 255]);
    }
}
