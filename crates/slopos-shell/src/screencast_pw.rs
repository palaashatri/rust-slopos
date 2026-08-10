//! PipeWire / portal screencast readiness and node discovery plan.
//!
//! Honest contract:
//! - Pure discovery **plans** and readiness checks (argv / env probes).
//! - Does **not** start a live PipeWire graph or export DMA-BUF streams.
//! - Portal [`crate::portal`] still uses protocol-level stream stubs until a
//!   session attaches a real node id from this discovery path.

use std::path::Path;

/// How screen content would be captured once PipeWire is live.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScreencastBackend {
    /// No capture backend available.
    Unavailable,
    /// Portal protocol stubs only (current default).
    PortalStub,
    /// PipeWire session manager present; nodes may be listed.
    PipeWire,
}

impl ScreencastBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::PortalStub => "portal_stub",
            Self::PipeWire => "pipewire",
        }
    }
}

/// One discoverable capture source (monitor or window).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScreencastSource {
    pub id: u32,
    pub name: String,
    pub source_type: ScreencastSourceType,
    pub width: u32,
    pub height: u32,
    /// PipeWire node id when known; `None` for pure portal placeholders.
    pub pw_node_id: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScreencastSourceType {
    Monitor,
    Window,
}

impl ScreencastSourceType {
    pub fn as_portal_bit(self) -> u32 {
        match self {
            Self::Monitor => 1, // SCREENCAST_SOURCE_TYPE_MONITOR
            Self::Window => 2,
        }
    }
}

/// Result of probing the host for screencast capability.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScreencastReadiness {
    pub backend: ScreencastBackend,
    pub pipewire_socket_present: bool,
    pub pw_cli_present: bool,
    pub xdg_runtime_dir: Option<String>,
    pub notes: Vec<String>,
}

/// Pure probe from environment + path existence (testable).
pub fn probe_screencast_readiness(
    xdg_runtime_dir: Option<&str>,
    pipewire_socket_exists: bool,
    pw_cli_on_path: bool,
) -> ScreencastReadiness {
    let mut notes = Vec::new();
    let socket_present = pipewire_socket_exists;
    if xdg_runtime_dir.is_none() {
        notes.push("XDG_RUNTIME_DIR unset".into());
    }
    if !socket_present {
        notes.push("PipeWire socket not found".into());
    }
    if !pw_cli_on_path {
        notes.push("pw-cli not on PATH (optional for listing)".into());
    }

    let backend = if socket_present {
        notes.push(
            "PipeWire socket present — live stream still requires portal Start + node export"
                .into(),
        );
        ScreencastBackend::PipeWire
    } else {
        notes.push("Falling back to portal protocol stubs".into());
        ScreencastBackend::PortalStub
    };

    ScreencastReadiness {
        backend,
        pipewire_socket_present: socket_present,
        pw_cli_present: pw_cli_on_path,
        xdg_runtime_dir: xdg_runtime_dir.map(str::to_string),
        notes,
    }
}

/// Default PipeWire socket path under XDG_RUNTIME_DIR.
pub fn default_pipewire_socket(xdg_runtime_dir: &str) -> String {
    format!("{xdg_runtime_dir}/pipewire-0")
}

/// Host probe using real env/paths (Linux session). Safe no-op-ish on macOS.
pub fn probe_screencast_readiness_host() -> ScreencastReadiness {
    let xdg = std::env::var("XDG_RUNTIME_DIR").ok();
    let socket_exists = xdg
        .as_ref()
        .map(|d| Path::new(&default_pipewire_socket(d)).exists())
        .unwrap_or(false);
    let pw_cli = path_has_binary("pw-cli");
    probe_screencast_readiness(xdg.as_deref(), socket_exists, pw_cli)
}

fn path_has_binary(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(name).is_file()))
        .unwrap_or(false)
}

/// Pure: build a monitor source list from output names (Settings / compositor).
pub fn sources_from_outputs(
    outputs: &[(String, u32, u32)],
    base_node_id: u32,
) -> Vec<ScreencastSource> {
    outputs
        .iter()
        .enumerate()
        .map(|(i, (name, w, h))| ScreencastSource {
            id: base_node_id + i as u32,
            name: name.clone(),
            source_type: ScreencastSourceType::Monitor,
            width: *w,
            height: *h,
            pw_node_id: None,
        })
        .collect()
}

/// Pure: merge window titles as window sources.
pub fn sources_from_windows(
    windows: &[(String, u32, u32)],
    base_node_id: u32,
) -> Vec<ScreencastSource> {
    windows
        .iter()
        .enumerate()
        .map(|(i, (title, w, h))| ScreencastSource {
            id: base_node_id + i as u32,
            name: title.clone(),
            source_type: ScreencastSourceType::Window,
            width: *w,
            height: *h,
            pw_node_id: None,
        })
        .collect()
}

/// Execution plan to list PipeWire nodes (caller may spawn; pure argv only).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PwListNodesPlan {
    pub argv: Vec<String>,
}

pub fn plan_list_pipewire_nodes() -> PwListNodesPlan {
    PwListNodesPlan {
        argv: vec!["pw-cli".into(), "ls".into(), "Node".into()],
    }
}

/// Map discovery sources → portal stream node ids (still may be placeholders).
pub fn source_ids_for_portal(sources: &[ScreencastSource]) -> Vec<u32> {
    sources
        .iter()
        .map(|s| s.pw_node_id.unwrap_or(s.id))
        .collect()
}

/// Whether Start can claim a non-stub backend for this readiness + selection.
pub fn can_claim_live_streams(ready: &ScreencastReadiness, selected: &[ScreencastSource]) -> bool {
    ready.backend == ScreencastBackend::PipeWire
        && ready.pipewire_socket_present
        && !selected.is_empty()
        && selected.iter().any(|s| s.pw_node_id.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_when_no_socket() {
        let r = probe_screencast_readiness(Some("/run/user/1000"), false, false);
        assert_eq!(r.backend, ScreencastBackend::PortalStub);
        assert!(!r.pipewire_socket_present);
    }

    #[test]
    fn pipewire_when_socket() {
        let r = probe_screencast_readiness(Some("/run/user/1000"), true, true);
        assert_eq!(r.backend, ScreencastBackend::PipeWire);
        assert!(!can_claim_live_streams(&r, &[]));
        let sources = vec![ScreencastSource {
            id: 1,
            name: "eDP-1".into(),
            source_type: ScreencastSourceType::Monitor,
            width: 1920,
            height: 1080,
            pw_node_id: Some(42),
        }];
        assert!(can_claim_live_streams(&r, &sources));
    }

    #[test]
    fn sources_from_outputs_and_plan() {
        let src = sources_from_outputs(&[("eDP-1".into(), 1920, 1080)], 100);
        assert_eq!(src.len(), 1);
        assert_eq!(src[0].id, 100);
        assert_eq!(source_ids_for_portal(&src), vec![100]);
        let plan = plan_list_pipewire_nodes();
        assert_eq!(plan.argv[0], "pw-cli");
    }
}
