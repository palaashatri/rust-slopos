//! Wire-safe control and snapshot types for compositor-authoritative Spaces.
//!
//! The compositor owns the mutable model.  The shell receives a compact
//! projection here and sends typed commands back through the session control
//! socket; it never edits compositor window state directly.

use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

/// Name of the compositor-owned atomic manifest accompanying Space PNGs.
pub const SPACE_THUMBNAIL_MANIFEST_FILE: &str = "spaces-thumbnails.json";
/// Maximum encoded size accepted for the compositor-owned thumbnail manifest.
pub const MAX_SPACE_THUMBNAIL_MANIFEST_BYTES: u64 = 64 * 1024;
/// Maximum number of capture records accepted from one manifest.
pub const MAX_SPACE_THUMBNAIL_MANIFEST_ENTRIES: usize = 4096;
pub const MAX_SPACE_THUMBNAIL_WIDTH: u32 = 640;
pub const MAX_SPACE_THUMBNAIL_HEIGHT: u32 = 480;

/// One successfully captured Space image in the compositor-owned manifest.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SpaceThumbnailEntry {
    pub space_id: u64,
    pub width: u32,
    pub height: u32,
}

/// Complete thumbnail generation published after a refresh attempt.
///
/// Entries are deliberately limited to captures that completed successfully.
/// A shell reader must not infer freshness from a leftover PNG filename.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SpaceThumbnailManifest {
    pub session_epoch: u64,
    pub generation: u64,
    pub captures: Vec<SpaceThumbnailEntry>,
}

impl SpaceThumbnailManifest {
    /// Validate values before a consumer trusts the manifest as a file index.
    ///
    /// The shell applies an additional authoritative epoch/revision check, but
    /// this shared validation keeps malformed or amplification-heavy JSON out
    /// of every reader and test harness.
    pub fn is_valid(&self) -> bool {
        self.session_epoch != 0
            && self.generation != 0
            && self.captures.len() <= MAX_SPACE_THUMBNAIL_MANIFEST_ENTRIES
            && self.captures.iter().enumerate().all(|(index, entry)| {
                entry.space_id != 0
                    && entry.width > 0
                    && entry.height > 0
                    && entry.width <= MAX_SPACE_THUMBNAIL_WIDTH
                    && entry.height <= MAX_SPACE_THUMBNAIL_HEIGHT
                    && self
                        .captures
                        .iter()
                        .take(index)
                        .all(|previous| previous.space_id != entry.space_id)
            })
    }
}

/// Return the private, compositor-owned PNG path for one Space thumbnail.
///
/// The runtime directory is intentionally constrained to an absolute path
/// without `.`/`..` components.  Both the compositor writer and shell reader
/// use this helper so a malformed environment value cannot redirect thumbnail
/// I/O outside the session runtime directory.
pub fn space_thumbnail_path_for_runtime(runtime: &Path, id: u64) -> Option<PathBuf> {
    if id == 0 {
        return None;
    }
    let mut components = runtime.components();
    if !matches!(components.next(), Some(Component::RootDir))
        || components.any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return None;
    }

    Some(runtime.join(format!("spaces-thumbnail-{id}.png")))
}

/// Return the exact thumbnail path for the current private session, if the
/// session runtime environment is safe and absolute.
pub fn session_space_thumbnail_path(id: u64) -> Option<PathBuf> {
    std::env::var_os("SLOPOS_SESSION_RUNTIME_DIR")
        .map(PathBuf::from)
        .and_then(|runtime| space_thumbnail_path_for_runtime(&runtime, id))
}

/// Return the private manifest path for the current session runtime.
pub fn session_space_thumbnail_manifest_path() -> Option<PathBuf> {
    std::env::var_os("SLOPOS_SESSION_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|runtime| {
            let mut components = runtime.components();
            matches!(components.next(), Some(Component::RootDir))
                && components
                    .all(|component| !matches!(component, Component::CurDir | Component::ParentDir))
        })
        .map(|runtime| runtime.join(SPACE_THUMBNAIL_MANIFEST_FILE))
}

/// Space-level fullscreen policy exposed across the compositor/session bus.
///
/// The wire enum intentionally lives in the bus crate so shell and compositor
/// clients can exchange the persisted policy without depending on each
/// other's implementation types.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpaceClassification {
    #[default]
    Normal,
    Fullscreen,
}

/// Multi-display policy for the compositor-owned Spaces set.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpacesDisplayPolicy {
    #[default]
    SharedSpan,
    IndependentPerDisplay,
}

/// A command that changes the compositor-owned Spaces model.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum SpacesControlCommand {
    Select {
        id: u64,
    },
    Create {
        name: String,
    },
    Rename {
        id: u64,
        name: String,
    },
    Reorder {
        id: u64,
        order: usize,
    },
    Remove {
        id: u64,
    },
    MoveWindow {
        window_id: String,
        target: SpaceTargetWire,
    },
    MoveActiveWindow {
        target: SpaceTargetWire,
    },
    /// Move the compositor's currently focused native window to a named
    /// output while preserving its presentation state and restore metadata.
    /// XWayland and unavailable connectors are rejected by the compositor
    /// rather than being represented as a shell-side geometry mutation.
    MoveActiveWindowToOutput {
        output_id: String,
    },
    SetWallpaper {
        id: u64,
        wallpaper: Option<String>,
    },
    SetAppearance {
        id: u64,
        appearance: Option<String>,
    },
    SetClassification {
        id: u64,
        classification: SpaceClassification,
    },
    SetMultiMonitorPolicy {
        policy: SpacesDisplayPolicy,
    },
    AssignOutput {
        id: u64,
        output_id: Option<String>,
    },
    /// Assign an application ID to one Space, every Space, or the active
    /// Space default (`Current` clears a previously stored policy).
    SetApplicationPolicy {
        app_id: String,
        target: SpaceTargetWire,
    },
    /// Ask the compositor to refresh compositor-owned live thumbnails for all
    /// current Spaces.  The request is advisory: a backend without a real
    /// renderer leaves the thumbnail files absent rather than fabricating
    /// window imagery.
    RefreshThumbnails,
}

/// The wire form of a window membership target.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpaceTargetWire {
    Current,
    Id { id: u64 },
    All,
}

/// Authoritative readback of an application-to-Space policy.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ApplicationSpacePolicySnapshot {
    pub app_id: String,
    pub target: SpaceTargetWire,
}

/// One compositor-owned Space row exposed to shell chrome.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SpaceSnapshot {
    pub id: u64,
    pub order: usize,
    pub name: String,
    pub active: bool,
    pub window_count: usize,
    #[serde(default)]
    pub wallpaper: Option<String>,
    #[serde(default)]
    pub appearance: Option<String>,
    #[serde(default)]
    pub classification: SpaceClassification,
    #[serde(default)]
    pub output_id: Option<String>,
}

/// Monotonic compositor state projection used by shell reconciliation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SpacesSnapshot {
    /// Changes whenever a compositor session starts.  A shell can therefore
    /// accept a lower revision after a compositor restart without treating it
    /// as stale state from the previous session.
    #[serde(default)]
    pub session_epoch: u64,
    pub revision: u64,
    pub active_space: u64,
    #[serde(default)]
    pub multi_monitor_policy: SpacesDisplayPolicy,
    #[serde(default)]
    pub application_policies: Vec<ApplicationSpacePolicySnapshot>,
    pub spaces: Vec<SpaceSnapshot>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_thumbnail_command_round_trips_through_json() {
        let command = SpacesControlCommand::RefreshThumbnails;
        let encoded = serde_json::to_vec(&command).expect("encode thumbnail command");
        assert_eq!(
            serde_json::from_slice::<SpacesControlCommand>(&encoded)
                .expect("decode thumbnail command"),
            command
        );
    }

    #[test]
    fn thumbnail_path_is_private_to_an_absolute_runtime_directory() {
        assert_eq!(
            space_thumbnail_path_for_runtime(Path::new("relative/session"), 7),
            None
        );
        assert_eq!(
            space_thumbnail_path_for_runtime(Path::new("/run/user/1000/slopos"), 0),
            None
        );
        assert_eq!(
            space_thumbnail_path_for_runtime(Path::new("/run/user/1000/../other"), 7),
            None
        );
        assert_eq!(
            space_thumbnail_path_for_runtime(Path::new("/run/user/1000/slopos"), 7),
            Some(PathBuf::from(
                "/run/user/1000/slopos/spaces-thumbnail-7.png"
            ))
        );
    }

    #[test]
    fn thumbnail_manifest_round_trips_and_rejects_unsafe_runtime_paths() {
        let manifest = SpaceThumbnailManifest {
            session_epoch: 4,
            generation: 9,
            captures: vec![SpaceThumbnailEntry {
                space_id: 7,
                width: 640,
                height: 480,
            }],
        };
        let encoded = serde_json::to_vec(&manifest).expect("encode thumbnail manifest");
        assert_eq!(
            serde_json::from_slice::<SpaceThumbnailManifest>(&encoded)
                .expect("decode thumbnail manifest"),
            manifest
        );
        assert_eq!(
            space_thumbnail_path_for_runtime(Path::new("/run/user/1000/../slopos"), 7),
            None
        );
        assert!(manifest.is_valid());
        assert!(!SpaceThumbnailManifest {
            session_epoch: 4,
            generation: 9,
            captures: vec![
                SpaceThumbnailEntry {
                    space_id: 7,
                    width: 640,
                    height: 480,
                },
                SpaceThumbnailEntry {
                    space_id: 7,
                    width: 640,
                    height: 480,
                },
            ],
        }
        .is_valid());
        assert!(!SpaceThumbnailManifest {
            session_epoch: 0,
            generation: 9,
            captures: Vec::new(),
        }
        .is_valid());
    }
}
