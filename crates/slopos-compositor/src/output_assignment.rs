//! Pure multi-output assignment and geometry policy.
//!
//! Live backends use these helpers to keep fullscreen, Fill, Smart Zoom,
//! popups and restore geometry on one real output rather than treating the
//! entire multi-monitor canvas as a single monitor.

use crate::{
    calculate_presentation_geometry, clamp_window_to_work_area, parse_outputs_layout_spec,
    LaidOutOutput, WindowGeometry, WindowPresentationState, WindowRestoreState,
};
use std::collections::HashSet;

/// Maximum number of logical outputs accepted from the live session-control path.
pub const MAX_RUNTIME_OUTPUTS: usize = 16;
/// Maximum logical dimension accepted for one output.
pub const MAX_RUNTIME_OUTPUT_DIMENSION: i32 = 16_384;
/// Maximum absolute logical origin accepted before normalization.
pub const MAX_RUNTIME_OUTPUT_ORIGIN: i32 = 131_072;

/// Parse and validate a complete runtime output-layout request.
///
/// Parsing is strict: a malformed token rejects the whole transaction instead
/// of silently disabling an output. The current nested renderer uses one global
/// scale, so requests with a different scale are rejected until mixed-scale
/// buffers are implemented.
pub fn validated_runtime_output_layout(
    spec: &str,
    expected_scale_percent: u32,
) -> Result<(Vec<String>, Vec<LaidOutOutput>), String> {
    let token_count = spec
        .split(';')
        .filter(|token| !token.trim().is_empty())
        .count();
    let entries = parse_outputs_layout_spec(spec);
    if token_count == 0 || entries.is_empty() {
        return Err("output layout must contain at least one valid output".to_owned());
    }
    if entries.len() != token_count {
        return Err("output layout contains a malformed token".to_owned());
    }
    if entries.len() > MAX_RUNTIME_OUTPUTS {
        return Err(format!(
            "output layout exceeds the {MAX_RUNTIME_OUTPUTS}-output session limit"
        ));
    }

    let mut names = HashSet::with_capacity(entries.len());
    for entry in &entries {
        if !names.insert(entry.name.clone()) {
            return Err(format!("duplicate output name: {}", entry.name));
        }
        if entry.config.width > MAX_RUNTIME_OUTPUT_DIMENSION
            || entry.config.height > MAX_RUNTIME_OUTPUT_DIMENSION
        {
            return Err(format!(
                "output {} exceeds the {}-pixel logical dimension limit",
                entry.name, MAX_RUNTIME_OUTPUT_DIMENSION
            ));
        }
        if entry.x.unsigned_abs() > MAX_RUNTIME_OUTPUT_ORIGIN as u32
            || entry.y.unsigned_abs() > MAX_RUNTIME_OUTPUT_ORIGIN as u32
        {
            return Err(format!(
                "output {} origin exceeds the supported logical range",
                entry.name
            ));
        }
        if entry.scale_percent != expected_scale_percent {
            return Err(format!(
                "output {} requests scale {} but this session currently uses uniform scale {}",
                entry.name, entry.scale_percent, expected_scale_percent
            ));
        }
    }

    let output_names = entries.iter().map(|entry| entry.name.clone()).collect();
    let outputs = entries
        .iter()
        .map(|entry| entry.to_laid_out())
        .collect::<Vec<_>>();
    Ok((output_names, normalize_laid_out_outputs(&outputs)))
}

/// Move a normal window from one output coordinate system to another while
/// preserving its proportional placement and keeping it fully visible.
pub fn remap_geometry_between_outputs(
    geometry: WindowGeometry,
    old_output: WindowGeometry,
    new_output: WindowGeometry,
) -> WindowGeometry {
    let old_width = i64::from(old_output.width.max(1));
    let old_height = i64::from(old_output.height.max(1));
    let new_width = i64::from(new_output.width.max(1));
    let new_height = i64::from(new_output.height.max(1));
    let relative_x = i64::from(geometry.x).saturating_sub(i64::from(old_output.x));
    let relative_y = i64::from(geometry.y).saturating_sub(i64::from(old_output.y));
    let mapped_x =
        i64::from(new_output.x).saturating_add(relative_x.saturating_mul(new_width) / old_width);
    let mapped_y =
        i64::from(new_output.y).saturating_add(relative_y.saturating_mul(new_height) / old_height);
    let width = geometry.width.clamp(1, new_output.width.max(1));
    let height = geometry.height.clamp(1, new_output.height.max(1));
    let max_x = i64::from(new_output.x)
        .saturating_add(i64::from(new_output.width.max(1).saturating_sub(width)));
    let max_y = i64::from(new_output.y)
        .saturating_add(i64::from(new_output.height.max(1).saturating_sub(height)));
    WindowGeometry::new(
        clamp_i64_to_i32(mapped_x.clamp(i64::from(new_output.x), max_x)),
        clamp_i64_to_i32(mapped_y.clamp(i64::from(new_output.y), max_y)),
        width,
        height,
    )
}

/// Pure geometry transaction for moving one compositor-owned window between
/// outputs.  Backends apply the returned geometry and update their protocol
/// surface/configure state only after validating the active window and target
/// connector, so an invalid request cannot partially mutate either record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowOutputMigration {
    pub geometry: WindowGeometry,
    pub restore_geometry: Option<WindowGeometry>,
}

pub fn plan_window_output_migration(
    state: WindowPresentationState,
    current_geometry: WindowGeometry,
    restore_state: Option<&WindowRestoreState>,
    old_output: WindowGeometry,
    new_output: WindowGeometry,
    new_work_area: WindowGeometry,
) -> WindowOutputMigration {
    let remapped_current = remap_geometry_between_outputs(current_geometry, old_output, new_output);
    let restore_geometry = restore_state.map(|restore| {
        clamp_window_to_work_area(
            remap_geometry_between_outputs(restore.normal_geometry, old_output, new_output),
            new_work_area,
        )
    });
    let normal_geometry = restore_geometry.unwrap_or(remapped_current);
    let preferred_size = restore_state
        .map(|restore| {
            (
                restore.normal_geometry.width,
                restore.normal_geometry.height,
            )
        })
        .or(Some((current_geometry.width, current_geometry.height)));
    let geometry = match state {
        WindowPresentationState::Normal | WindowPresentationState::Minimized => {
            clamp_window_to_work_area(remapped_current, new_work_area)
        }
        WindowPresentationState::Fullscreen => new_output,
        state => {
            calculate_presentation_geometry(new_work_area, state, preferred_size, normal_geometry)
        }
    };
    WindowOutputMigration {
        geometry,
        restore_geometry,
    }
}

/// Convert a logical output description into compositor geometry.
pub fn output_geometry(output: &LaidOutOutput) -> WindowGeometry {
    WindowGeometry::new(
        output.x,
        output.y,
        output.config.width.max(1),
        output.config.height.max(1),
    )
}

/// Bounding rectangle for every output, including negative origins.
pub fn output_layout_bounds(outputs: &[LaidOutOutput]) -> Option<WindowGeometry> {
    let first = outputs.first()?;
    let mut min_x = i64::from(first.x);
    let mut min_y = i64::from(first.y);
    let mut max_x = i64::from(first.x) + i64::from(first.config.width.max(1));
    let mut max_y = i64::from(first.y) + i64::from(first.config.height.max(1));

    for output in &outputs[1..] {
        let width = i64::from(output.config.width.max(1));
        let height = i64::from(output.config.height.max(1));
        let x = i64::from(output.x);
        let y = i64::from(output.y);
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x.saturating_add(width));
        max_y = max_y.max(y.saturating_add(height));
    }

    Some(WindowGeometry::new(
        clamp_i64_to_i32(min_x),
        clamp_i64_to_i32(min_y),
        clamp_positive_i64_to_i32(max_x.saturating_sub(min_x)),
        clamp_positive_i64_to_i32(max_y.saturating_sub(min_y)),
    ))
}

/// Shift a layout so its union starts at compositor coordinate `(0, 0)`.
///
/// Wayland output positions remain relative to each other, while the nested
/// framebuffer no longer clips outputs placed left or above the nominal origin.
pub fn normalize_laid_out_outputs(outputs: &[LaidOutOutput]) -> Vec<LaidOutOutput> {
    let Some(bounds) = output_layout_bounds(outputs) else {
        return Vec::new();
    };
    outputs
        .iter()
        .map(|output| LaidOutOutput {
            config: output.config,
            x: output.x.saturating_sub(bounds.x),
            y: output.y.saturating_sub(bounds.y),
        })
        .collect()
}

/// Select the output containing a point, or the nearest output when the point
/// is outside every output or inside a layout gap.
pub fn output_index_for_point(
    outputs: &[LaidOutOutput],
    point_x: i32,
    point_y: i32,
) -> Option<usize> {
    let point_x = i64::from(point_x);
    let point_y = i64::from(point_y);
    let mut nearest: Option<(usize, i128)> = None;

    for (index, output) in outputs.iter().enumerate() {
        let geometry = output_geometry(output);
        let left = i64::from(geometry.x);
        let top = i64::from(geometry.y);
        let right = left.saturating_add(i64::from(geometry.width));
        let bottom = top.saturating_add(i64::from(geometry.height));

        if point_x >= left && point_x < right && point_y >= top && point_y < bottom {
            return Some(index);
        }

        let dx = axis_distance_to_half_open_rect(point_x, left, right);
        let dy = axis_distance_to_half_open_rect(point_y, top, bottom);
        let distance = i128::from(dx) * i128::from(dx) + i128::from(dy) * i128::from(dy);
        if nearest.is_none_or(|(_, best)| distance < best) {
            nearest = Some((index, distance));
        }
    }

    nearest.map(|(index, _)| index)
}

/// Select the output owning a window.
///
/// The output with the greatest intersection area wins. Ties prefer the output
/// containing the window centre, then retain stable layout order. Completely
/// off-screen windows use the nearest output to their centre.
pub fn output_index_for_geometry(
    outputs: &[LaidOutOutput],
    geometry: WindowGeometry,
) -> Option<usize> {
    if outputs.is_empty() {
        return None;
    }

    let geometry = WindowGeometry::new(
        geometry.x,
        geometry.y,
        geometry.width.max(1),
        geometry.height.max(1),
    );
    let centre_x = i64::from(geometry.x) + i64::from(geometry.width) / 2;
    let centre_y = i64::from(geometry.y) + i64::from(geometry.height) / 2;
    let mut best: Option<(usize, i64, bool)> = None;

    for (index, output) in outputs.iter().enumerate() {
        let output_geometry = output_geometry(output);
        let area = intersection_area(geometry, output_geometry);
        let centre_inside = point_inside_i64(centre_x, centre_y, output_geometry);
        let replace = best.is_none_or(|(_, best_area, best_contains)| {
            area > best_area || (area == best_area && centre_inside && !best_contains)
        });
        if replace {
            best = Some((index, area, centre_inside));
        }
    }

    let (best_index, best_area, _) = best.expect("non-empty outputs always produce a candidate");
    if best_area > 0 {
        Some(best_index)
    } else {
        output_index_for_point(
            outputs,
            clamp_i64_to_i32(centre_x),
            clamp_i64_to_i32(centre_y),
        )
    }
}

/// Return every output genuinely intersected by a surface geometry.
///
/// Unlike [`output_index_for_geometry`], this intentionally has no nearest-output
/// fallback: `wl_surface.enter`/`leave` must describe real scan-out intersection,
/// not merely the output that would own presentation policy for an off-screen
/// window.
pub fn intersecting_output_indices(
    outputs: &[LaidOutOutput],
    geometry: WindowGeometry,
) -> Vec<usize> {
    outputs
        .iter()
        .enumerate()
        .filter_map(|(index, output)| {
            geometries_intersect(geometry, output_geometry(output)).then_some(index)
        })
        .collect()
}

/// True when two logical rectangles overlap by at least one pixel.
pub fn geometries_intersect(a: WindowGeometry, b: WindowGeometry) -> bool {
    intersection_area(a, b) > 0
}

fn intersection_area(a: WindowGeometry, b: WindowGeometry) -> i64 {
    let a_left = i64::from(a.x);
    let a_top = i64::from(a.y);
    let a_right = a_left.saturating_add(i64::from(a.width.max(1)));
    let a_bottom = a_top.saturating_add(i64::from(a.height.max(1)));
    let b_left = i64::from(b.x);
    let b_top = i64::from(b.y);
    let b_right = b_left.saturating_add(i64::from(b.width.max(1)));
    let b_bottom = b_top.saturating_add(i64::from(b.height.max(1)));

    let width = a_right
        .min(b_right)
        .saturating_sub(a_left.max(b_left))
        .max(0);
    let height = a_bottom
        .min(b_bottom)
        .saturating_sub(a_top.max(b_top))
        .max(0);
    width.saturating_mul(height)
}

fn point_inside_i64(x: i64, y: i64, geometry: WindowGeometry) -> bool {
    let left = i64::from(geometry.x);
    let top = i64::from(geometry.y);
    let right = left.saturating_add(i64::from(geometry.width.max(1)));
    let bottom = top.saturating_add(i64::from(geometry.height.max(1)));
    x >= left && x < right && y >= top && y < bottom
}

fn axis_distance_to_half_open_rect(point: i64, start: i64, end: i64) -> i64 {
    if point < start {
        start.saturating_sub(point)
    } else if point >= end {
        point.saturating_sub(end.saturating_sub(1))
    } else {
        0
    }
}

fn clamp_i64_to_i32(value: i64) -> i32 {
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

fn clamp_positive_i64_to_i32(value: i64) -> i32 {
    value.clamp(1, i64::from(i32::MAX)) as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OutputConfig;

    fn output(x: i32, y: i32, width: i32, height: i32) -> LaidOutOutput {
        LaidOutOutput {
            config: OutputConfig { width, height },
            x,
            y,
        }
    }

    #[test]
    fn runtime_layout_validation_is_transactional_and_scale_honest() {
        let (names, outputs) = validated_runtime_output_layout(
            "LEFT:800x600@-800,0:s100;RIGHT:1024x768@0,0:s100",
            100,
        )
        .unwrap();
        assert_eq!(names, vec!["LEFT", "RIGHT"]);
        assert_eq!(outputs[0], output(0, 0, 800, 600));
        assert_eq!(outputs[1], output(800, 0, 1024, 768));

        assert!(
            validated_runtime_output_layout("LEFT:800x600@0,0:s100;broken-token", 100)
                .unwrap_err()
                .contains("malformed")
        );
        assert!(
            validated_runtime_output_layout("LEFT:800x600@0,0:s125", 100)
                .unwrap_err()
                .contains("uniform scale")
        );
        assert!(validated_runtime_output_layout(
            "LEFT:800x600@0,0:s100;LEFT:1024x768@800,0:s100",
            100
        )
        .unwrap_err()
        .contains("duplicate"));
    }

    #[test]
    fn topology_remap_preserves_relative_placement_and_visibility() {
        let old = WindowGeometry::new(0, 0, 1000, 800);
        let new = WindowGeometry::new(1000, 100, 2000, 1200);
        assert_eq!(
            remap_geometry_between_outputs(WindowGeometry::new(250, 200, 500, 400), old, new,),
            WindowGeometry::new(1500, 400, 500, 400)
        );
        assert_eq!(
            remap_geometry_between_outputs(
                WindowGeometry::new(900, 700, 900, 700),
                old,
                WindowGeometry::new(0, 0, 640, 480),
            ),
            WindowGeometry::new(0, 0, 640, 480)
        );
    }

    #[test]
    fn output_migration_preserves_normal_size_and_clamps_restore_state() {
        let old_output = WindowGeometry::new(0, 0, 1920, 1080);
        let new_output = WindowGeometry::new(1920, 0, 1280, 800);
        let work_area = WindowGeometry::new(1920, 24, 1280, 776);
        let restore = WindowRestoreState::new(
            WindowGeometry::new(1600, 900, 640, 480),
            WindowPresentationState::Normal,
            "DP-1",
            1,
        );
        let planned = plan_window_output_migration(
            WindowPresentationState::SmartZoomed,
            WindowGeometry::new(900, 200, 1000, 700),
            Some(&restore),
            old_output,
            new_output,
            work_area,
        );
        assert_eq!(planned.geometry, WindowGeometry::new(2240, 172, 640, 480));
        assert_eq!(
            planned.restore_geometry,
            Some(WindowGeometry::new(2560, 320, 640, 480))
        );
    }

    #[test]
    fn output_migration_fullscreen_uses_target_output_not_work_area() {
        let planned = plan_window_output_migration(
            WindowPresentationState::Fullscreen,
            WindowGeometry::new(20, 20, 400, 300),
            None,
            WindowGeometry::new(0, 0, 800, 600),
            WindowGeometry::new(800, 40, 1200, 900),
            WindowGeometry::new(800, 64, 1200, 876),
        );
        assert_eq!(planned.geometry, WindowGeometry::new(800, 40, 1200, 900));
        assert_eq!(planned.restore_geometry, None);
    }

    #[test]
    fn negative_and_offset_layouts_are_normalized_without_losing_relationships() {
        let normalized = normalize_laid_out_outputs(&[
            output(-1920, 120, 1920, 1080),
            output(0, -200, 2560, 1440),
        ]);
        assert_eq!(normalized[0], output(0, 320, 1920, 1080));
        assert_eq!(normalized[1], output(1920, 0, 2560, 1440));
        assert_eq!(
            output_layout_bounds(&normalized),
            Some(WindowGeometry::new(0, 0, 4480, 1440))
        );
    }

    #[test]
    fn greatest_window_overlap_selects_the_owning_output() {
        let outputs = [output(0, 0, 1000, 800), output(1000, 0, 1600, 900)];
        assert_eq!(
            output_index_for_geometry(&outputs, WindowGeometry::new(850, 100, 600, 500)),
            Some(1)
        );
        assert_eq!(
            output_index_for_geometry(&outputs, WindowGeometry::new(700, 100, 500, 500)),
            Some(0)
        );
    }

    #[test]
    fn equal_overlap_prefers_the_output_containing_the_window_centre() {
        let outputs = [output(0, 0, 1000, 800), output(1000, 0, 1000, 800)];
        assert_eq!(
            output_index_for_geometry(&outputs, WindowGeometry::new(750, 100, 500, 500)),
            Some(1)
        );
    }

    #[test]
    fn layout_gaps_and_offscreen_windows_choose_the_nearest_output() {
        let outputs = [output(0, 0, 800, 600), output(1200, 0, 800, 600)];
        assert_eq!(output_index_for_point(&outputs, 900, 300), Some(0));
        assert_eq!(output_index_for_point(&outputs, 1100, 300), Some(1));
        assert_eq!(
            output_index_for_geometry(&outputs, WindowGeometry::new(3000, 100, 400, 300)),
            Some(1)
        );
    }

    #[test]
    fn surface_membership_reports_every_real_intersection_without_nearest_fallback() {
        let outputs = [output(0, 0, 1000, 800), output(1000, 0, 1000, 800)];
        assert_eq!(
            intersecting_output_indices(&outputs, WindowGeometry::new(900, 100, 300, 400)),
            vec![0, 1]
        );
        assert!(
            intersecting_output_indices(&outputs, WindowGeometry::new(2400, 100, 200, 200))
                .is_empty()
        );
    }

    #[test]
    fn intersection_is_half_open_and_overflow_safe() {
        assert!(!geometries_intersect(
            WindowGeometry::new(0, 0, 100, 100),
            WindowGeometry::new(100, 0, 100, 100)
        ));
        assert!(geometries_intersect(
            WindowGeometry::new(i32::MAX - 20, 0, 100, 10),
            WindowGeometry::new(i32::MAX - 10, 0, 10, 10)
        ));
    }
}
