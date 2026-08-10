//! Shell-side projection of compositor-authoritative SLOPOS Spaces.
//!
//! The local value is only a render/input mirror.  A live shell reconciles it
//! from [`slopos_bus::SpacesSnapshot`] and sends mutations back to the
//! compositor; it must not invent ordinary-window membership or geometry.

use slopos_bus::{
    ApplicationSpacePolicySnapshot, SpaceClassification, SpaceTargetWire, SpacesDisplayPolicy,
};

/// Default number of Spaces used before the compositor publishes a snapshot.
/// This is a startup compatibility value, not a production upper bound.
pub const SHELL_DESKTOP_COUNT: usize = 8;

/// Legacy indexed control compatibility count.  Dynamic Spaces use stable IDs.
pub const COMPOSITOR_WORKSPACE_COUNT: usize = 8;

/// Pure bridge: shell active index ↔ compositor workspace id (0..7).
pub fn shell_index_to_compositor(index: usize) -> Option<u8> {
    if index < COMPOSITOR_WORKSPACE_COUNT {
        Some(index as u8)
    } else {
        None
    }
}

/// Whether a window on `window_workspace` is visible when shell active is `active`.
pub fn window_visible_on_active(active: usize, window_workspace: usize) -> bool {
    active == window_workspace && active < SHELL_DESKTOP_COUNT
}

pub struct WorkspaceManager {
    pub workspaces: Vec<Workspace>,
    pub active: usize,
    pub total: usize,
    /// Stable compositor Space ID corresponding to `active`.
    pub active_id: u64,
    /// Last compositor snapshot revision accepted by this mirror.
    pub revision: u64,
    /// Compositor session epoch for restart-safe revision reconciliation.
    pub session_epoch: u64,
    /// Current compositor-owned multi-display policy.
    pub multi_monitor_policy: SpacesDisplayPolicy,
    /// Validated compositor-owned application-to-Space policies. This is a
    /// render/readback mirror only; mutations still travel through the typed
    /// Spaces control bus.
    pub application_policies: Vec<ApplicationSpacePolicySnapshot>,
}

pub struct Workspace {
    pub id: u64,
    pub name: String,
    pub background: Option<String>,
    pub wallpaper: Option<String>,
    pub appearance: Option<String>,
    pub classification: SpaceClassification,
    pub output_id: Option<String>,
    pub window_count: usize,
}

impl Default for WorkspaceManager {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceManager {
    pub fn new() -> Self {
        let workspaces = (0..SHELL_DESKTOP_COUNT)
            .map(|i| Workspace {
                id: (i + 1) as u64,
                name: format!("Desktop {}", i + 1),
                background: None,
                wallpaper: None,
                appearance: None,
                classification: SpaceClassification::Normal,
                output_id: None,
                window_count: 0,
            })
            .collect();
        Self {
            workspaces,
            active: 0,
            total: SHELL_DESKTOP_COUNT,
            active_id: 1,
            revision: 0,
            session_epoch: 0,
            multi_monitor_policy: SpacesDisplayPolicy::SharedSpan,
            application_policies: Vec::new(),
        }
    }

    /// Reconcile the local shell mirror with one compositor snapshot.
    ///
    /// Rows are sorted by their authoritative order, metadata is copied by
    /// stable Space ID, and stale revisions are ignored. Invalid snapshots
    /// are rejected without partially changing the mirror.
    pub fn apply_snapshot(&mut self, snapshot: &slopos_bus::SpacesSnapshot) -> bool {
        if snapshot.spaces.is_empty() {
            return false;
        }

        let epoch_changed = snapshot.session_epoch != 0
            && self.session_epoch != 0
            && snapshot.session_epoch != self.session_epoch;
        if !epoch_changed && snapshot.revision < self.revision {
            return false;
        }

        let mut rows = snapshot.spaces.clone();
        rows.sort_by_key(|row| row.order);
        let valid_order = rows
            .iter()
            .enumerate()
            .all(|(index, row)| row.order == index);
        let unique_ids = rows
            .iter()
            .map(|row| row.id)
            .collect::<std::collections::HashSet<_>>()
            .len()
            == rows.len();
        let active_rows = rows.iter().filter(|row| row.active).count();
        let active_index = rows
            .iter()
            .position(|row| row.id == snapshot.active_space && row.active);
        let Some(active_index) = active_index else {
            return false;
        };
        let mut names = std::collections::HashSet::new();
        let valid_names_and_metadata = rows.iter().all(|row| {
            let name_valid = !row.name.is_empty()
                && row.name.trim() == row.name
                && !row.name.chars().any(char::is_control)
                && names.insert(row.name.to_lowercase());
            let metadata_valid = [row.wallpaper.as_deref(), row.appearance.as_deref()]
                .into_iter()
                .flatten()
                .all(|value| {
                    !value.is_empty()
                        && !value.chars().any(char::is_control)
                        && value.trim() == value
                });
            let output_valid = row.output_id.as_deref().is_none_or(|value| {
                !value.is_empty() && !value.chars().any(char::is_control) && value.trim() == value
            });
            name_valid && metadata_valid && output_valid
        });
        let policy_valid = snapshot.multi_monitor_policy != SpacesDisplayPolicy::SharedSpan
            || rows.iter().all(|row| row.output_id.is_none());
        let space_ids = rows
            .iter()
            .map(|row| row.id)
            .collect::<std::collections::HashSet<_>>();
        let mut application_ids = std::collections::HashSet::new();
        let application_policies_valid = snapshot.application_policies.iter().all(|policy| {
            !policy.app_id.is_empty()
                && policy.app_id.trim() == policy.app_id
                && !policy.app_id.chars().any(char::is_control)
                && application_ids.insert(policy.app_id.as_str())
                && match policy.target {
                    SpaceTargetWire::Id { id } => space_ids.contains(&id),
                    SpaceTargetWire::All => true,
                    // `Current` clears a policy in the compositor model and
                    // is therefore never a persisted/readback policy row.
                    SpaceTargetWire::Current => false,
                }
        });
        if !valid_order
            || !unique_ids
            || active_rows != 1
            || !valid_names_and_metadata
            || !policy_valid
            || !application_policies_valid
        {
            return false;
        }

        let workspaces = rows
            .into_iter()
            .map(|row| Workspace {
                id: row.id,
                name: row.name,
                background: row.wallpaper.clone(),
                wallpaper: row.wallpaper,
                appearance: row.appearance,
                classification: row.classification,
                output_id: row.output_id,
                window_count: row.window_count,
            })
            .collect::<Vec<_>>();
        self.workspaces = workspaces;
        self.active = active_index;
        self.total = self.workspaces.len();
        self.active_id = snapshot.active_space;
        self.revision = snapshot.revision;
        self.multi_monitor_policy = snapshot.multi_monitor_policy;
        self.application_policies = snapshot.application_policies.clone();
        if snapshot.session_epoch != 0 {
            self.session_epoch = snapshot.session_epoch;
        }
        true
    }

    /// Activate desktop by index (shell `0..total`). Mirrors compositor `activate`.
    pub fn switch_to(&mut self, index: usize) -> bool {
        if index < self.total {
            self.active = index;
            self.active_id = self.workspaces[index].id;
            true
        } else {
            false
        }
    }

    /// Alias for [`Self::switch_to`] (compositor naming).
    pub fn activate(&mut self, index: usize) -> bool {
        self.switch_to(index)
    }

    /// Cycle forward, wrapping. Mirrors compositor `cycle_next`.
    pub fn next(&mut self) {
        if self.total == 0 {
            return;
        }
        self.active = (self.active + 1) % self.total;
        self.active_id = self.workspaces[self.active].id;
    }

    /// Alias for [`Self::next`].
    pub fn cycle_next(&mut self) {
        self.next();
    }

    /// Cycle backward, wrapping. Mirrors compositor `cycle_prev`.
    pub fn previous(&mut self) {
        if self.total == 0 {
            return;
        }
        self.active = if self.active == 0 {
            self.total - 1
        } else {
            self.active - 1
        };
        self.active_id = self.workspaces[self.active].id;
    }

    /// Alias for [`Self::previous`].
    pub fn cycle_prev(&mut self) {
        self.previous();
    }

    pub fn active_workspace(&self) -> Option<&Workspace> {
        self.workspaces.get(self.active)
    }

    pub fn add_workspace(&mut self, name: &str) {
        let id = self
            .workspaces
            .iter()
            .map(|workspace| workspace.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        self.workspaces.push(Workspace {
            id,
            name: name.to_string(),
            background: None,
            wallpaper: None,
            appearance: None,
            classification: SpaceClassification::Normal,
            output_id: None,
            window_count: 0,
        });
        self.total += 1;
        self.active_id = self.workspaces[self.active].id;
    }

    /// Compositor-aligned summary line for session logs.
    pub fn summary_line(&self) -> String {
        format!(
            "shell-workspace active={}/{} name={}",
            self.active,
            self.total,
            self.active_workspace()
                .map(|w| w.name.as_str())
                .unwrap_or("?")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eight_desktops_align_with_compositor() {
        assert_eq!(SHELL_DESKTOP_COUNT, COMPOSITOR_WORKSPACE_COUNT);
        assert_eq!(SHELL_DESKTOP_COUNT, 8);
        let wm = WorkspaceManager::new();
        assert_eq!(wm.total, 8);
        assert_eq!(wm.workspaces.len(), 8);
        assert_eq!(shell_index_to_compositor(7), Some(7));
        assert_eq!(shell_index_to_compositor(8), None);
        assert!(window_visible_on_active(2, 2));
        assert!(!window_visible_on_active(2, 3));
    }

    #[test]
    fn cycle_wraps_eight() {
        let mut wm = WorkspaceManager::new();
        for _ in 0..7 {
            wm.next();
        }
        assert_eq!(wm.active, 7);
        wm.next();
        assert_eq!(wm.active, 0);
        wm.previous();
        assert_eq!(wm.active, 7);
    }
}
