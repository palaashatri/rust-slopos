//! Single-authority window presentation state machine and zoom policy.
//!
//! Copyright (c) 2026 Palaash Atri
//! SPDX-License-Identifier: MIT

use crate::WindowGeometry;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum TilePlacement {
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum WindowPresentationState {
    #[default]
    Normal,
    Minimized,
    SmartZoomed,
    Filled,
    Fullscreen,
    Tiled(TilePlacement),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum WindowStackingIntent {
    Preserve,
    RestoreAt(usize),
}

/// Compositor-owned state captured before the first non-normal transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowRestoreState {
    pub normal_geometry: WindowGeometry,
    pub previous_state: WindowPresentationState,
    pub output_id: String,
    pub space_id: usize,
    pub stacking_intent: WindowStackingIntent,
}

impl WindowRestoreState {
    pub fn new(
        normal_geometry: WindowGeometry,
        previous_state: WindowPresentationState,
        output_id: impl Into<String>,
        space_id: usize,
    ) -> Self {
        Self {
            normal_geometry,
            previous_state,
            output_id: output_id.into(),
            space_id,
            stacking_intent: WindowStackingIntent::Preserve,
        }
    }

    pub fn with_stacking_intent(mut self, stacking_intent: WindowStackingIntent) -> Self {
        self.stacking_intent = stacking_intent;
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PresentationTransition {
    pub state: WindowPresentationState,
    pub geometry: WindowGeometry,
    pub restore_state: Option<WindowRestoreState>,
    /// Restore metadata consumed by a transition back to Normal.
    pub restored_from: Option<WindowRestoreState>,
}

/// Transition one compositor-owned window presentation state.
///
/// The first normal geometry, Space, output and stacking intent are retained
/// through any sequence of minimize, zoom, fill, tiling and fullscreen states.
/// Returning to Normal consumes that record and clamps the saved frame into the
/// current work area, which handles display removal and work-area changes.
#[allow(clippy::too_many_arguments)]
pub fn transition_presentation_state(
    current_state: WindowPresentationState,
    current_geometry: WindowGeometry,
    current_restore_state: Option<&WindowRestoreState>,
    target_state: WindowPresentationState,
    work_area: WindowGeometry,
    output_area: WindowGeometry,
    preferred_size: Option<(i32, i32)>,
    output_id: impl Into<String>,
    space_id: usize,
) -> PresentationTransition {
    // A normal window must not inherit a stale record from backend bookkeeping.
    let mut restore_state = (current_state != WindowPresentationState::Normal)
        .then(|| current_restore_state.cloned())
        .flatten();

    if target_state != WindowPresentationState::Normal && restore_state.is_none() {
        restore_state = Some(WindowRestoreState::new(
            current_geometry,
            current_state,
            output_id,
            space_id,
        ));
    }

    if target_state == WindowPresentationState::Normal {
        let geometry = if current_state == WindowPresentationState::Normal {
            current_geometry
        } else {
            restore_state
                .as_ref()
                .map(|restore| restore.normal_geometry)
                .unwrap_or(current_geometry)
        };
        return PresentationTransition {
            state: WindowPresentationState::Normal,
            geometry: clamp_geometry_to_area(geometry, work_area),
            restore_state: None,
            restored_from: restore_state,
        };
    }

    if target_state == WindowPresentationState::Minimized {
        // Minimize is a visibility transition. Geometry must remain untouched so
        // output changes cannot silently rewrite a hidden window's active state.
        return PresentationTransition {
            state: WindowPresentationState::Minimized,
            geometry: current_geometry,
            restore_state,
            restored_from: None,
        };
    }

    let original_normal = restore_state
        .as_ref()
        .map(|restore| restore.normal_geometry)
        .unwrap_or(current_geometry);
    let target_area = if target_state == WindowPresentationState::Fullscreen {
        output_area
    } else {
        work_area
    };

    PresentationTransition {
        state: target_state,
        geometry: calculate_presentation_geometry(
            target_area,
            target_state,
            preferred_size,
            original_normal,
        ),
        restore_state,
        restored_from: None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ZoomAction {
    SmartZoom,
    Fill,
    FullScreen,
    ShowLayoutMenu,
    Minimize,
    None,
}

impl ZoomAction {
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "smart_zoom" | "smartzoom" | "smart-zoom" => Self::SmartZoom,
            "fill" => Self::Fill,
            "full_screen" | "fullscreen" | "full-screen" => Self::FullScreen,
            "show_layout_menu" | "layout_menu" | "menu" => Self::ShowLayoutMenu,
            "minimize" => Self::Minimize,
            _ => Self::None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SmartZoom => "smart_zoom",
            Self::Fill => "fill",
            Self::FullScreen => "fullscreen",
            Self::ShowLayoutMenu => "layout_menu",
            Self::Minimize => "minimize",
            Self::None => "none",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ZoomPolicyConfig {
    pub zoom_button_action: ZoomAction,
    pub zoom_button_alternate_action: ZoomAction,
    pub titlebar_double_click_action: ZoomAction,
    pub show_layout_menu_on_hover: bool,
    pub edge_tiling: bool,
    pub edge_fill: bool,
    pub restore_last_geometry: bool,
}

impl Default for ZoomPolicyConfig {
    fn default() -> Self {
        Self {
            zoom_button_action: ZoomAction::SmartZoom,
            zoom_button_alternate_action: ZoomAction::Fill,
            titlebar_double_click_action: ZoomAction::SmartZoom,
            show_layout_menu_on_hover: true,
            edge_tiling: true,
            edge_fill: true,
            restore_last_geometry: true,
        }
    }
}

impl ZoomPolicyConfig {
    pub fn from_settings_map(map: &HashMap<String, String>) -> Self {
        let mut config = Self::default();
        if let Some(value) = map.get("zoom_button_action") {
            config.zoom_button_action = ZoomAction::parse(value);
        }
        if let Some(value) = map.get("zoom_button_alternate_action") {
            config.zoom_button_alternate_action = ZoomAction::parse(value);
        }
        if let Some(value) = map.get("titlebar_double_click_action") {
            config.titlebar_double_click_action = ZoomAction::parse(value);
        }
        if let Some(value) = map.get("show_layout_menu_on_hover") {
            config.show_layout_menu_on_hover = parse_bool(value, config.show_layout_menu_on_hover);
        }
        if let Some(value) = map.get("edge_tiling") {
            config.edge_tiling = parse_bool(value, config.edge_tiling);
        }
        if let Some(value) = map.get("edge_fill") {
            config.edge_fill = parse_bool(value, config.edge_fill);
        }
        if let Some(value) = map.get("restore_last_geometry") {
            config.restore_last_geometry = parse_bool(value, config.restore_last_geometry);
        }
        config
    }
}

fn parse_bool(value: &str, default: bool) -> bool {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => true,
        "false" | "0" | "no" | "off" => false,
        _ => default,
    }
}

/// Calculate target geometry in a sanitized, non-empty output/work area.
///
/// A one-pixel axis cannot be split into two non-empty disjoint regions. Both
/// corresponding tile placements therefore share that single pixel instead of
/// expanding outside the real output or producing a zero-sized configure.
pub fn calculate_presentation_geometry(
    work_area: WindowGeometry,
    state: WindowPresentationState,
    preferred_size: Option<(i32, i32)>,
    normal_geometry: WindowGeometry,
) -> WindowGeometry {
    let area = normalize_area(work_area);
    let target = match state {
        WindowPresentationState::Normal | WindowPresentationState::Minimized => normal_geometry,
        WindowPresentationState::Filled | WindowPresentationState::Fullscreen => area,
        WindowPresentationState::SmartZoomed => smart_zoom_geometry(area, preferred_size),
        WindowPresentationState::Tiled(placement) => tiled_geometry(area, placement),
    };
    clamp_geometry_to_normalized_area(target, area)
}

fn smart_zoom_geometry(area: WindowGeometry, preferred_size: Option<(i32, i32)>) -> WindowGeometry {
    let Some((preferred_width, preferred_height)) = preferred_size else {
        return area;
    };
    let minimum_width = area.width.min(200);
    let minimum_height = area.height.min(150);
    let width = preferred_width.clamp(minimum_width, area.width);
    let height = preferred_height.clamp(minimum_height, area.height);
    WindowGeometry::new(
        area.x.saturating_add((area.width - width) / 2),
        area.y.saturating_add((area.height - height) / 2),
        width,
        height,
    )
}

fn tiled_geometry(area: WindowGeometry, placement: TilePlacement) -> WindowGeometry {
    let ((left_x, left_width), (right_x, right_width)) = split_axis(area.x, area.width);
    let ((top_y, top_height), (bottom_y, bottom_height)) = split_axis(area.y, area.height);

    match placement {
        TilePlacement::Left => WindowGeometry::new(left_x, area.y, left_width, area.height),
        TilePlacement::Right => WindowGeometry::new(right_x, area.y, right_width, area.height),
        TilePlacement::TopLeft => WindowGeometry::new(left_x, top_y, left_width, top_height),
        TilePlacement::TopRight => WindowGeometry::new(right_x, top_y, right_width, top_height),
        TilePlacement::BottomLeft => {
            WindowGeometry::new(left_x, bottom_y, left_width, bottom_height)
        }
        TilePlacement::BottomRight => {
            WindowGeometry::new(right_x, bottom_y, right_width, bottom_height)
        }
    }
}

fn split_axis(origin: i32, extent: i32) -> ((i32, i32), (i32, i32)) {
    debug_assert!(extent >= 1);
    if extent == 1 {
        return ((origin, 1), (origin, 1));
    }
    let first = extent / 2;
    let second = extent - first;
    ((origin, first), (origin.saturating_add(first), second))
}

fn normalize_area(area: WindowGeometry) -> WindowGeometry {
    WindowGeometry::new(area.x, area.y, area.width.max(1), area.height.max(1))
}

fn clamp_geometry_to_area(desired: WindowGeometry, area: WindowGeometry) -> WindowGeometry {
    clamp_geometry_to_normalized_area(desired, normalize_area(area))
}

fn clamp_geometry_to_normalized_area(
    desired: WindowGeometry,
    area: WindowGeometry,
) -> WindowGeometry {
    debug_assert!(area.width >= 1 && area.height >= 1);
    let width = desired.width.clamp(1, area.width);
    let height = desired.height.clamp(1, area.height);
    let max_x = area.x.saturating_add(area.width.saturating_sub(width));
    let max_y = area.y.saturating_add(area.height.saturating_sub(height));
    WindowGeometry::new(
        desired.x.clamp(area.x, max_x),
        desired.y.clamp(area.y, max_y),
        width,
        height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_action_and_boolean_policy_parsing_are_stable() {
        assert_eq!(ZoomAction::parse("smart-zoom"), ZoomAction::SmartZoom);
        assert_eq!(ZoomAction::parse("full_screen"), ZoomAction::FullScreen);
        assert_eq!(ZoomAction::parse("unknown"), ZoomAction::None);

        let mut map = HashMap::new();
        map.insert("zoom_button_action".to_owned(), "fill".to_owned());
        map.insert("edge_tiling".to_owned(), "off".to_owned());
        map.insert("edge_fill".to_owned(), "not-a-bool".to_owned());
        let policy = ZoomPolicyConfig::from_settings_map(&map);
        assert_eq!(policy.zoom_button_action, ZoomAction::Fill);
        assert!(!policy.edge_tiling);
        assert!(policy.edge_fill);
    }

    #[test]
    fn first_restore_record_survives_every_non_normal_transition() {
        let normal = WindowGeometry::new(120, 88, 640, 420);
        let work = WindowGeometry::new(0, 24, 1280, 712);
        let output = WindowGeometry::new(0, 0, 1280, 800);

        let zoomed = transition_presentation_state(
            WindowPresentationState::Normal,
            normal,
            None,
            WindowPresentationState::SmartZoomed,
            work,
            output,
            Some((800, 500)),
            "output-a",
            3,
        );
        let original_restore = zoomed.restore_state.clone();
        let filled = transition_presentation_state(
            zoomed.state,
            zoomed.geometry,
            zoomed.restore_state.as_ref(),
            WindowPresentationState::Filled,
            work,
            output,
            None,
            "output-b",
            8,
        );
        let fullscreen = transition_presentation_state(
            filled.state,
            filled.geometry,
            filled.restore_state.as_ref(),
            WindowPresentationState::Fullscreen,
            work,
            output,
            None,
            "output-c",
            9,
        );
        let minimized = transition_presentation_state(
            fullscreen.state,
            fullscreen.geometry,
            fullscreen.restore_state.as_ref(),
            WindowPresentationState::Minimized,
            work,
            output,
            None,
            "output-d",
            10,
        );

        assert_eq!(filled.restore_state, original_restore);
        assert_eq!(fullscreen.restore_state, original_restore);
        assert_eq!(minimized.restore_state, original_restore);

        let restored = transition_presentation_state(
            minimized.state,
            minimized.geometry,
            minimized.restore_state.as_ref(),
            WindowPresentationState::Normal,
            work,
            output,
            None,
            "ignored",
            99,
        );
        assert_eq!(restored.geometry, normal);
        assert_eq!(restored.restored_from, original_restore);
        assert!(restored.restore_state.is_none());
    }

    #[test]
    fn restore_after_output_removal_is_clamped_to_current_work_area() {
        let old_normal = WindowGeometry::new(5000, -200, 2000, 1200);
        let old_work = WindowGeometry::new(0, 24, 3840, 2136);
        let old_output = WindowGeometry::new(0, 0, 3840, 2160);
        let fullscreen = transition_presentation_state(
            WindowPresentationState::Normal,
            old_normal,
            None,
            WindowPresentationState::Fullscreen,
            old_work,
            old_output,
            None,
            "DP-1",
            2,
        );

        let laptop_work = WindowGeometry::new(0, 24, 1280, 776);
        let restored = transition_presentation_state(
            fullscreen.state,
            fullscreen.geometry,
            fullscreen.restore_state.as_ref(),
            WindowPresentationState::Normal,
            laptop_work,
            WindowGeometry::new(0, 0, 1280, 800),
            None,
            "eDP-1",
            2,
        );
        assert_eq!(restored.geometry, laptop_work);
        let metadata = restored.restored_from.unwrap();
        assert_eq!(metadata.normal_geometry, old_normal);
        assert_eq!(metadata.output_id, "DP-1");
    }

    #[test]
    fn minimize_never_rewrites_current_geometry() {
        let current = WindowGeometry::new(-80, -10, 900, 700);
        let minimized = transition_presentation_state(
            WindowPresentationState::Normal,
            current,
            None,
            WindowPresentationState::Minimized,
            WindowGeometry::new(100, 200, 320, 240),
            WindowGeometry::new(100, 180, 320, 280),
            None,
            "output-a",
            7,
        );
        assert_eq!(minimized.geometry, current);
        assert_eq!(minimized.restore_state.unwrap().normal_geometry, current);
    }

    #[test]
    fn odd_sized_tiling_is_gapless() {
        let area = WindowGeometry::new(11, 23, 801, 601);
        let normal = WindowGeometry::new(20, 30, 640, 420);
        let left = calculate_presentation_geometry(
            area,
            WindowPresentationState::Tiled(TilePlacement::Left),
            None,
            normal,
        );
        let right = calculate_presentation_geometry(
            area,
            WindowPresentationState::Tiled(TilePlacement::Right),
            None,
            normal,
        );
        let top_left = calculate_presentation_geometry(
            area,
            WindowPresentationState::Tiled(TilePlacement::TopLeft),
            None,
            normal,
        );
        let bottom_right = calculate_presentation_geometry(
            area,
            WindowPresentationState::Tiled(TilePlacement::BottomRight),
            None,
            normal,
        );
        assert_eq!(left.width + right.width, area.width);
        assert_eq!(right.x, left.x + left.width);
        assert_eq!(top_left.height + bottom_right.height, area.height);
        assert_eq!(bottom_right.y, top_left.y + top_left.height);
    }

    #[test]
    fn one_pixel_axes_overlap_instead_of_escaping_or_becoming_zero() {
        let normal = WindowGeometry::new(-100, -100, 100, 100);
        for area in [
            WindowGeometry::new(50, 60, 1, 1),
            WindowGeometry::new(50, 60, 1, 9),
            WindowGeometry::new(50, 60, 9, 1),
            WindowGeometry::new(50, 60, 0, -10),
        ] {
            let normalized = normalize_area(area);
            for placement in [
                TilePlacement::Left,
                TilePlacement::Right,
                TilePlacement::TopLeft,
                TilePlacement::TopRight,
                TilePlacement::BottomLeft,
                TilePlacement::BottomRight,
            ] {
                let geometry = calculate_presentation_geometry(
                    area,
                    WindowPresentationState::Tiled(placement),
                    None,
                    normal,
                );
                assert!(geometry.width >= 1 && geometry.height >= 1);
                assert!(geometry.x >= normalized.x && geometry.y >= normalized.y);
                assert!(
                    geometry.x.saturating_add(geometry.width)
                        <= normalized.x.saturating_add(normalized.width)
                );
                assert!(
                    geometry.y.saturating_add(geometry.height)
                        <= normalized.y.saturating_add(normalized.height)
                );
            }
        }
    }

    #[test]
    fn every_target_is_contained_after_sanitization() {
        let area = WindowGeometry::new(100, 200, 800, 500);
        let normal = WindowGeometry::new(i32::MIN, i32::MAX, i32::MAX, -1);
        for state in [
            WindowPresentationState::Normal,
            WindowPresentationState::Minimized,
            WindowPresentationState::SmartZoomed,
            WindowPresentationState::Filled,
            WindowPresentationState::Fullscreen,
            WindowPresentationState::Tiled(TilePlacement::BottomRight),
        ] {
            let geometry = calculate_presentation_geometry(area, state, Some((-1, 900)), normal);
            assert!(geometry.x >= area.x && geometry.y >= area.y, "{state:?}");
            assert!(
                geometry.x.saturating_add(geometry.width) <= area.x.saturating_add(area.width),
                "{state:?}"
            );
            assert!(
                geometry.y.saturating_add(geometry.height) <= area.y.saturating_add(area.height),
                "{state:?}"
            );
        }
    }
}
