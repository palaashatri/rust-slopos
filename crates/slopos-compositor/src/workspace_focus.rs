//! Pure helpers: focus + paint policy for virtual workspaces.
//!
//! Live compositor calls these after `WorkspaceState::cycle_*` / `activate`,
//! and when deciding SHM surface elements vs solid placeholders.

use crate::{WorkspaceId, WorkspaceState};

/// Choose which window should receive focus after a workspace change.
///
/// - Prefer the topmost id in `paint_order_bottom_to_top` that is **visible**
///   on the active workspace.
/// - If none, return `None` (caller clears keyboard focus / unfocuses hidden).
pub fn focus_window_after_workspace_switch<'a>(
    state: &WorkspaceState,
    paint_order_bottom_to_top: &[&'a str],
) -> Option<&'a str> {
    paint_order_bottom_to_top
        .iter()
        .rev()
        .copied()
        .find(|id| state.is_visible(id))
}

/// True when the currently focused window must be cleared after a workspace switch
/// (hidden on inactive desktop, or unknown/untracked).
pub fn should_clear_focus_after_workspace_switch(
    state: &WorkspaceState,
    focused_window_id: Option<&str>,
) -> bool {
    match focused_window_id {
        Some(id) => !state.is_visible(id),
        None => false,
    }
}

/// Whether a pointer hit-test candidate is allowed (visible on active workspace).
pub fn hit_test_allowed(state: &WorkspaceState, window_id: &str) -> bool {
    state.is_visible(window_id)
}

/// Assign a newly mapped window to the **active** workspace (default policy).
pub fn assign_new_window_to_active(
    state: &mut WorkspaceState,
    window_id: impl Into<String>,
) -> bool {
    let ws = state.active;
    state.assign_window(window_id, ws)
}

/// Optional: move window to workspace by index (window rules bridge).
pub fn move_window_to_index(
    state: &mut WorkspaceState,
    window_id: &str,
    workspace_index: u8,
) -> bool {
    match WorkspaceId::new(workspace_index) {
        Some(ws) => state.move_to_workspace(window_id, ws),
        None => false,
    }
}

/// Activate a workspace from a typed session-control index.
///
/// The control plane uses a raw `u8` so it can remain independent of the
/// compositor's private `WorkspaceId` representation. Invalid indices are
/// rejected without changing the active workspace.
pub fn activate_workspace_index(state: &mut WorkspaceState, workspace_index: u8) -> bool {
    match WorkspaceId::new(workspace_index) {
        Some(workspace) => state.activate(workspace),
        None => false,
    }
}

/// How a single **visible** window should be painted this frame.
///
/// Honesty contract:
/// - [`WindowPaintSource::SurfaceTree`]: client committed a buffer (or otherwise
///   produced surface render elements) — draw real SHM/client content.
/// - [`WindowPaintSource::Placeholder`]: no committed buffer yet — solid rect only.
///
/// Placeholders must never replace real surface content when elements exist.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowPaintSource {
    SurfaceTree,
    Placeholder,
}

/// Map "did `render_elements_from_surface_tree` yield anything?" to paint source.
pub fn window_paint_source(has_surface_elements: bool) -> WindowPaintSource {
    if has_surface_elements {
        WindowPaintSource::SurfaceTree
    } else {
        WindowPaintSource::Placeholder
    }
}

/// Filter a bottom→top paint order to ids that should present on the active
/// workspace (`is_visible`). Used by nested X11 and DRM listing/present paths.
pub fn visible_paint_order<'a>(
    state: &WorkspaceState,
    paint_order_bottom_to_top: &[&'a str],
) -> Vec<&'a str> {
    paint_order_bottom_to_top
        .iter()
        .copied()
        .filter(|id| state.is_visible(id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WorkspaceId;

    #[test]
    fn focus_picks_topmost_visible() {
        let mut st = WorkspaceState::new();
        assert!(st.assign_window("a", WorkspaceId(0)));
        assert!(st.assign_window("b", WorkspaceId(1)));
        assert!(st.assign_window("c", WorkspaceId(0)));
        // paint order bottom→top: a, b, c
        assert_eq!(
            focus_window_after_workspace_switch(&st, &["a", "b", "c"]),
            Some("c")
        );
        st.cycle_next();
        assert_eq!(
            focus_window_after_workspace_switch(&st, &["a", "b", "c"]),
            Some("b")
        );
    }

    #[test]
    fn focus_none_when_no_visible_windows() {
        let mut st = WorkspaceState::new();
        assert!(st.assign_window("a", WorkspaceId(1)));
        assert_eq!(focus_window_after_workspace_switch(&st, &["a"]), None);
        assert!(should_clear_focus_after_workspace_switch(&st, Some("a")));
        assert!(!should_clear_focus_after_workspace_switch(&st, None));
        st.activate(WorkspaceId(1));
        assert!(!should_clear_focus_after_workspace_switch(&st, Some("a")));
    }

    #[test]
    fn move_and_hit_test() {
        let mut st = WorkspaceState::new();
        assign_new_window_to_active(&mut st, "w");
        assert!(hit_test_allowed(&st, "w"));
        assert!(move_window_to_index(&mut st, "w", 3));
        assert!(!hit_test_allowed(&st, "w"));
        st.activate(WorkspaceId(3));
        assert!(hit_test_allowed(&st, "w"));
    }

    #[test]
    fn activate_workspace_index_accepts_valid_and_rejects_invalid_ids() {
        let mut st = WorkspaceState::new();
        assert!(activate_workspace_index(&mut st, 7));
        assert_eq!(st.active, WorkspaceId(7));
        assert!(!activate_workspace_index(&mut st, 8));
        assert_eq!(st.active, WorkspaceId(7));
    }

    #[test]
    fn paint_source_prefers_surface_tree() {
        assert_eq!(window_paint_source(true), WindowPaintSource::SurfaceTree);
        assert_eq!(window_paint_source(false), WindowPaintSource::Placeholder);
    }

    #[test]
    fn visible_paint_order_filters_inactive() {
        let mut st = WorkspaceState::new();
        assert!(st.assign_window("a", WorkspaceId(0)));
        assert!(st.assign_window("b", WorkspaceId(2)));
        assert!(st.assign_window("c", WorkspaceId(0)));
        assert_eq!(visible_paint_order(&st, &["a", "b", "c"]), vec!["a", "c"]);
        st.activate(WorkspaceId(2));
        assert_eq!(visible_paint_order(&st, &["a", "b", "c"]), vec!["b"]);
    }
}
