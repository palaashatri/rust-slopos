//! Multi-monitor arrangement pure model (KScreen-class layout policy).
//!
//! Pure helpers place logical outputs, pick a primary, and produce an apply plan
//! the compositor / Settings UI can consume. No DRM/Wayland I/O lives here.

use serde::{Deserialize, Serialize};

/// How multiple outputs relate to each other.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ArrangeMode {
    /// Outputs placed left-to-right along x.
    #[default]
    ExtendRight,
    /// Outputs stacked top-to-bottom along y.
    ExtendDown,
    /// All outputs show the same content at (0,0); largest defines logical size.
    Mirror,
    /// Only the primary output is active; others disabled.
    PrimaryOnly,
}

impl ArrangeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExtendRight => "extend_right",
            Self::ExtendDown => "extend_down",
            Self::Mirror => "mirror",
            Self::PrimaryOnly => "primary_only",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "extend" | "extend_right" | "side" | "side_by_side" | "sbs" => Some(Self::ExtendRight),
            "extend_down" | "stack" | "stacked" | "vertical" => Some(Self::ExtendDown),
            "mirror" | "clone" | "same" => Some(Self::Mirror),
            "primary" | "primary_only" | "single" => Some(Self::PrimaryOnly),
            _ => None,
        }
    }
}

/// One physical / logical output as known to Settings.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DisplayOutput {
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// Scale factor ×100 (100 = 1.0, 200 = 2.0).
    pub scale_percent: u32,
    pub enabled: bool,
    pub is_primary: bool,
    /// Refresh rate in mHz (60000 = 60 Hz).
    pub refresh_mhz: u32,
}

impl DisplayOutput {
    pub fn new(name: impl Into<String>, width: u32, height: u32) -> Self {
        Self {
            name: name.into(),
            width,
            height,
            scale_percent: 100,
            enabled: true,
            is_primary: false,
            refresh_mhz: 60_000,
        }
    }

    pub fn with_scale(mut self, scale_percent: u32) -> Self {
        self.scale_percent = scale_percent.max(50).min(400);
        self
    }

    pub fn with_refresh_mhz(mut self, refresh_mhz: u32) -> Self {
        self.refresh_mhz = refresh_mhz.max(30_000);
        self
    }

    /// Logical pixels after scale (rounded).
    pub fn logical_size(&self) -> (u32, u32) {
        let s = self.scale_percent.max(1) as u64;
        let w = (self.width as u64 * 100 / s).max(1) as u32;
        let h = (self.height as u64 * 100 / s).max(1) as u32;
        (w, h)
    }
}

/// Placed output with origin in the global layout space.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlacedOutput {
    pub output: DisplayOutput,
    pub x: i32,
    pub y: i32,
}

/// Full multi-monitor arrangement request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisplayArrangement {
    pub mode: ArrangeMode,
    pub outputs: Vec<DisplayOutput>,
}

/// Concrete apply plan (no side effects).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DisplayApplyStep {
    SetPrimary {
        name: String,
    },
    Place {
        name: String,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        scale_percent: u32,
        refresh_mhz: u32,
    },
    Disable {
        name: String,
    },
    /// Env / compositor hint string (for nested / DRM bridge).
    EmitLayoutEnv {
        key: String,
        value: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisplayApplyPlan {
    pub steps: Vec<DisplayApplyStep>,
    pub placed: Vec<PlacedOutput>,
    pub logical_width: u32,
    pub logical_height: u32,
}

impl DisplayApplyPlan {
    /// Return the canonical layout payload for the compositor session-control
    /// request.  The layout is produced by the same plan that drives the
    /// nested bootstrap environment, so Settings and shell startup cannot
    /// silently diverge on output names, geometry or scale.
    pub fn layout_value(&self) -> Option<&str> {
        self.steps.iter().find_map(|step| match step {
            DisplayApplyStep::EmitLayoutEnv { key, value } if key == "SLOPOS_OUTPUTS_LAYOUT" => {
                Some(value.as_str())
            }
            _ => None,
        })
    }
}

/// Validate arrangement: ≥1 enabled output, exactly one primary among enabled
/// (or auto-pick first enabled).
pub fn normalize_arrangement(mut arr: DisplayArrangement) -> Result<DisplayArrangement, String> {
    if arr.outputs.is_empty() {
        return Err("no outputs".into());
    }
    let enabled: Vec<usize> = arr
        .outputs
        .iter()
        .enumerate()
        .filter(|(_, o)| o.enabled)
        .map(|(i, _)| i)
        .collect();
    if enabled.is_empty() {
        return Err("no enabled outputs".into());
    }
    let primary_count = enabled
        .iter()
        .filter(|&&i| arr.outputs[i].is_primary)
        .count();
    if primary_count == 0 {
        arr.outputs[enabled[0]].is_primary = true;
    } else if primary_count > 1 {
        let mut seen = false;
        for i in 0..arr.outputs.len() {
            if arr.outputs[i].is_primary {
                if seen || !arr.outputs[i].enabled {
                    arr.outputs[i].is_primary = false;
                } else {
                    seen = true;
                }
            }
        }
    }
    for o in &mut arr.outputs {
        if o.width == 0 || o.height == 0 {
            return Err(format!("output {} has zero size", o.name));
        }
        o.scale_percent = o.scale_percent.clamp(50, 400);
    }
    Ok(arr)
}

/// Pure layout: place outputs according to mode.
pub fn place_outputs(arr: &DisplayArrangement) -> Result<Vec<PlacedOutput>, String> {
    let arr = normalize_arrangement(arr.clone())?;
    let mut enabled: Vec<DisplayOutput> =
        arr.outputs.iter().filter(|o| o.enabled).cloned().collect();
    if enabled.is_empty() {
        return Err("no enabled outputs".into());
    }
    // Primary first for stable placement origin.
    enabled.sort_by(|a, b| {
        b.is_primary
            .cmp(&a.is_primary)
            .then_with(|| a.name.cmp(&b.name))
    });

    match arr.mode {
        ArrangeMode::PrimaryOnly => {
            let primary = enabled
                .into_iter()
                .find(|o| o.is_primary)
                .ok_or_else(|| "no primary".to_string())?;
            Ok(vec![PlacedOutput {
                output: primary,
                x: 0,
                y: 0,
            }])
        }
        ArrangeMode::Mirror => Ok(enabled
            .into_iter()
            .map(|output| PlacedOutput { output, x: 0, y: 0 })
            .collect()),
        ArrangeMode::ExtendRight => {
            let mut x: i32 = 0;
            let mut out = Vec::with_capacity(enabled.len());
            for output in enabled {
                let (lw, _) = output.logical_size();
                out.push(PlacedOutput { output, x, y: 0 });
                x = x.saturating_add(lw as i32);
            }
            Ok(out)
        }
        ArrangeMode::ExtendDown => {
            let mut y: i32 = 0;
            let mut out = Vec::with_capacity(enabled.len());
            for output in enabled {
                let (_, lh) = output.logical_size();
                out.push(PlacedOutput { output, x: 0, y });
                y = y.saturating_add(lh as i32);
            }
            Ok(out)
        }
    }
}

/// Bounding box of placed outputs (logical pixels).
pub fn arrangement_bounds(placed: &[PlacedOutput]) -> (u32, u32) {
    let mut max_x = 0i32;
    let mut max_y = 0i32;
    for p in placed {
        let (lw, lh) = p.output.logical_size();
        max_x = max_x.max(p.x.saturating_add(lw as i32));
        max_y = max_y.max(p.y.saturating_add(lh as i32));
    }
    (max_x.max(0) as u32, max_y.max(0) as u32)
}

/// Build a full apply plan from an arrangement.
pub fn plan_display_apply(arr: &DisplayArrangement) -> Result<DisplayApplyPlan, String> {
    let arr = normalize_arrangement(arr.clone())?;
    let placed = place_outputs(&arr)?;
    let (logical_width, logical_height) = arrangement_bounds(&placed);
    let mut steps = Vec::new();

    if let Some(primary) = placed.iter().find(|p| p.output.is_primary) {
        steps.push(DisplayApplyStep::SetPrimary {
            name: primary.output.name.clone(),
        });
    }

    let enabled_names: std::collections::HashSet<&str> =
        placed.iter().map(|p| p.output.name.as_str()).collect();

    for p in &placed {
        steps.push(DisplayApplyStep::Place {
            name: p.output.name.clone(),
            x: p.x,
            y: p.y,
            width: p.output.width,
            height: p.output.height,
            scale_percent: p.output.scale_percent,
            refresh_mhz: p.output.refresh_mhz,
        });
    }

    for o in &arr.outputs {
        if !enabled_names.contains(o.name.as_str()) {
            steps.push(DisplayApplyStep::Disable {
                name: o.name.clone(),
            });
        }
    }

    // Nested compositor bridge: compact layout env value.
    let layout_value = placed
        .iter()
        .map(|p| {
            format!(
                "{}:{}x{}@{},{}:s{}",
                p.output.name, p.output.width, p.output.height, p.x, p.y, p.output.scale_percent
            )
        })
        .collect::<Vec<_>>()
        .join(";");
    steps.push(DisplayApplyStep::EmitLayoutEnv {
        key: "SLOPOS_OUTPUTS_LAYOUT".into(),
        value: layout_value,
    });

    Ok(DisplayApplyPlan {
        steps,
        placed,
        logical_width,
        logical_height,
    })
}

/// Parse `SLOPOS_OUTPUT_LAYOUT` style mode names (alias of [`ArrangeMode::parse`]).
pub fn arrange_mode_from_env_value(value: Option<&str>) -> ArrangeMode {
    value.and_then(ArrangeMode::parse).unwrap_or_default()
}

/// Live apply: set process env vars for every [`DisplayApplyStep::EmitLayoutEnv`].
///
/// Returns the `(key, value)` pairs that were applied. Other plan steps
/// (Place / Disable / SetPrimary) are left for compositor/DRM paths — this
/// helper only bridges the nested layout env contract
/// (`SLOPOS_OUTPUTS_LAYOUT`).
pub fn apply_display_plan_env(plan: &DisplayApplyPlan) -> Vec<(String, String)> {
    let mut applied = Vec::new();
    for step in &plan.steps {
        if let DisplayApplyStep::EmitLayoutEnv { key, value } = step {
            // SAFETY: single-threaded shell startup / settings path; layout env
            // is process-local configuration for nested compositor children.
            std::env::set_var(key, value);
            applied.push((key.clone(), value.clone()));
        }
    }
    applied
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dual() -> DisplayArrangement {
        DisplayArrangement {
            mode: ArrangeMode::ExtendRight,
            outputs: vec![DisplayOutput::new("eDP-1", 1920, 1080).with_scale(100), {
                let mut o = DisplayOutput::new("HDMI-1", 2560, 1440);
                o.is_primary = false;
                o
            }],
        }
    }

    #[test]
    fn normalize_picks_primary() {
        let arr = normalize_arrangement(dual()).unwrap();
        assert_eq!(arr.outputs.iter().filter(|o| o.is_primary).count(), 1);
        assert!(arr.outputs[0].is_primary);
    }

    #[test]
    fn extend_right_places_side_by_side() {
        let mut arr = dual();
        arr.outputs[0].is_primary = true;
        let plan = plan_display_apply(&arr).unwrap();
        assert_eq!(plan.placed.len(), 2);
        assert_eq!(plan.placed[0].x, 0);
        assert_eq!(plan.placed[1].x, 1920);
        assert!(plan.logical_width >= 1920 + 2560);
        assert!(plan
            .steps
            .iter()
            .any(|s| matches!(s, DisplayApplyStep::SetPrimary { .. })));
    }

    #[test]
    fn mirror_all_at_origin() {
        let mut arr = dual();
        arr.mode = ArrangeMode::Mirror;
        arr.outputs[0].is_primary = true;
        let placed = place_outputs(&arr).unwrap();
        assert!(placed.iter().all(|p| p.x == 0 && p.y == 0));
    }

    #[test]
    fn primary_only_disables_secondary() {
        let mut arr = dual();
        arr.mode = ArrangeMode::PrimaryOnly;
        arr.outputs[0].is_primary = true;
        let plan = plan_display_apply(&arr).unwrap();
        assert_eq!(plan.placed.len(), 1);
        assert!(plan
            .steps
            .iter()
            .any(|s| matches!(s, DisplayApplyStep::Disable { name } if name == "HDMI-1")));
    }

    #[test]
    fn scale_affects_logical_extent() {
        let mut o = DisplayOutput::new("eDP-1", 2000, 1000).with_scale(200);
        o.is_primary = true;
        let arr = DisplayArrangement {
            mode: ArrangeMode::ExtendRight,
            outputs: vec![o],
        };
        let plan = plan_display_apply(&arr).unwrap();
        assert_eq!(plan.logical_width, 1000);
        assert_eq!(plan.logical_height, 500);
    }

    #[test]
    fn parse_modes() {
        assert_eq!(ArrangeMode::parse("mirror"), Some(ArrangeMode::Mirror));
        assert_eq!(
            arrange_mode_from_env_value(Some("stack")),
            ArrangeMode::ExtendDown
        );
    }

    #[test]
    fn apply_display_plan_env_sets_layout() {
        let mut arr = dual();
        arr.outputs[0].is_primary = true;
        let plan = plan_display_apply(&arr).unwrap();
        let applied = apply_display_plan_env(&plan);
        let layout = applied
            .iter()
            .find(|(k, _)| k == "SLOPOS_OUTPUTS_LAYOUT")
            .map(|(_, v)| v.as_str())
            .expect("expected SLOPOS_OUTPUTS_LAYOUT in applied pairs");
        // Assert on the returned pairs (stable under parallel tests that also
        // touch process env). Process env is set best-effort for live apply.
        assert!(layout.contains("eDP-1"), "layout value={layout}");
        assert!(layout.contains("HDMI-1"), "layout value={layout}");
        // Cleanup so other tests are not polluted (best-effort).
        std::env::remove_var("SLOPOS_OUTPUTS_LAYOUT");
    }
}
