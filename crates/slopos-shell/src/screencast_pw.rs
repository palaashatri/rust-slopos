//! PipeWire / portal screencast readiness and node discovery plan.
//!
//! Honest contract:
//! - Pure discovery **plans** and readiness checks (argv / env probes).
//! - Does **not** start a live PipeWire graph or export DMA-BUF streams.
//! - Portal [`crate::portal`] still uses protocol-level stream stubs until a
//!   session attaches a real node id from this discovery path.

use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

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

/// One real video source reported by the PipeWire graph.
///
/// This type is populated only from `pw-cli ls Node` output.  In particular,
/// a compositor output id or a local source id is never promoted to
/// `node_id` here.  Width and height are optional because PipeWire nodes may
/// expose their format only after negotiation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PipeWireVideoNode {
    pub node_id: u32,
    pub name: Option<String>,
    pub description: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// Result of the host graph query used by portal capability diagnostics.
///
/// `query_succeeded` means that PipeWire accepted the `pw-cli` graph query;
/// it does not mean that a screen-capture producer or permission decision is
/// present.  ScreenCast remains fail-closed until a real producer is wired to
/// the portal session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PipeWireGraphProbe {
    pub query_succeeded: bool,
    pub video_sources: Vec<PipeWireVideoNode>,
    pub note: String,
}

impl PipeWireGraphProbe {
    /// Whether the graph contains the exact node id supplied by a live source.
    pub fn contains_node(&self, node_id: u32) -> bool {
        self.video_sources
            .iter()
            .any(|source| source.node_id == node_id)
    }
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

/// Parse `pw-cli ls Node` output and retain only real `Video/Source` nodes.
///
/// The parser is intentionally strict about the media class and node id.  It
/// ignores audio, MIDI, driver, and malformed records rather than guessing a
/// screen source from their names or from local compositor metadata.
pub fn parse_pipewire_video_nodes(output: &str) -> Vec<PipeWireVideoNode> {
    #[derive(Default)]
    struct PendingNode {
        node_id: u32,
        media_class: Option<String>,
        name: Option<String>,
        description: Option<String>,
        width: Option<u32>,
        height: Option<u32>,
    }

    fn finish(
        pending: Option<PendingNode>,
        seen: &mut HashSet<u32>,
        result: &mut Vec<PipeWireVideoNode>,
    ) {
        let Some(node) = pending else {
            return;
        };
        if node.node_id == 0
            || node.media_class.as_deref() != Some("Video/Source")
            || !seen.insert(node.node_id)
        {
            return;
        }
        result.push(PipeWireVideoNode {
            node_id: node.node_id,
            name: node.name,
            description: node.description,
            width: node.width,
            height: node.height,
        });
    }

    let mut pending = None;
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("id ") {
            finish(pending.take(), &mut seen, &mut result);
            let Some(raw_id) = rest.split(',').next().map(str::trim) else {
                continue;
            };
            let Ok(node_id) = raw_id.parse::<u32>() else {
                continue;
            };
            pending = Some(PendingNode {
                node_id,
                ..PendingNode::default()
            });
            continue;
        }

        let Some(node) = pending.as_mut() else {
            continue;
        };
        let Some((raw_key, raw_value)) = line.split_once('=') else {
            continue;
        };
        let key = raw_key.trim();
        let value = unquote_pipewire_value(raw_value.trim());
        match key {
            "media.class" => node.media_class = Some(value),
            "node.name" => node.name = Some(value),
            "node.description" => node.description = Some(value),
            "video.width" => node.width = value.parse().ok(),
            "video.height" => node.height = value.parse().ok(),
            _ => {}
        }
    }
    finish(pending, &mut seen, &mut result);
    result
}

fn unquote_pipewire_value(value: &str) -> String {
    let value = value.trim();
    let Some(inner) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) else {
        return value.to_string();
    };

    let mut result = String::with_capacity(inner.len());
    let mut escaped = false;
    for character in inner.chars() {
        if escaped {
            result.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else {
            result.push(character);
        }
    }
    if escaped {
        result.push('\\');
    }
    result
}

/// Query the current session's PipeWire graph through `pw-cli`.
///
/// This is deliberately a discovery-only backend.  A successful query with
/// no video sources is distinct from a missing socket/tool, and neither case
/// authorizes the portal to claim a stream or return a PipeWire remote.
pub fn probe_pipewire_graph_host() -> PipeWireGraphProbe {
    let Some(xdg_runtime_dir) = std::env::var_os("XDG_RUNTIME_DIR") else {
        return PipeWireGraphProbe {
            query_succeeded: false,
            video_sources: Vec::new(),
            note: "XDG_RUNTIME_DIR unset".into(),
        };
    };
    let socket = default_pipewire_socket(&xdg_runtime_dir.to_string_lossy());
    if !Path::new(&socket).exists() {
        return PipeWireGraphProbe {
            query_succeeded: false,
            video_sources: Vec::new(),
            note: "PipeWire socket not found".into(),
        };
    }
    if !path_has_binary("pw-cli") {
        return PipeWireGraphProbe {
            query_succeeded: false,
            video_sources: Vec::new(),
            note: "pw-cli not on PATH".into(),
        };
    }

    // A portal method must not block indefinitely on a broken PipeWire
    // service.  `timeout` is part of the Linux runtime used by this backend;
    // if it is absent the probe fails closed rather than spawning an
    // unbounded child.
    let output = match Command::new("timeout")
        .args(["5", "pw-cli", "ls", "Node"])
        .output()
    {
        Ok(output) => output,
        Err(error) => {
            return PipeWireGraphProbe {
                query_succeeded: false,
                video_sources: Vec::new(),
                note: format!("pw-cli spawn failed: {error}"),
            };
        }
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return PipeWireGraphProbe {
            query_succeeded: false,
            video_sources: Vec::new(),
            note: if stderr.is_empty() {
                format!("pw-cli exited with status {}", output.status)
            } else {
                format!("pw-cli failed: {stderr}")
            },
        };
    }

    let video_sources = parse_pipewire_video_nodes(&String::from_utf8_lossy(&output.stdout));
    let note = if video_sources.is_empty() {
        "PipeWire graph queried; no Video/Source nodes".into()
    } else {
        format!(
            "PipeWire graph queried; {} Video/Source node(s) found",
            video_sources.len()
        )
    };
    PipeWireGraphProbe {
        query_succeeded: true,
        video_sources,
        note,
    }
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

/// Map discovery sources to real PipeWire node ids.
///
/// A source without `pw_node_id` is only descriptive metadata (for example a
/// monitor discovered from the compositor).  It must not be converted to the
/// source's local id because that would fabricate a PipeWire node id for
/// ScreenCast.Start.  Such sources are omitted until the permission-mediated
/// PipeWire graph reports an actual node.
pub fn source_ids_for_portal(sources: &[ScreencastSource]) -> Vec<u32> {
    sources.iter().filter_map(|s| s.pw_node_id).collect()
}

/// Whether Start can claim a non-stub backend for this readiness + selection.
pub fn can_claim_live_streams(ready: &ScreencastReadiness, selected: &[ScreencastSource]) -> bool {
    ready.backend == ScreencastBackend::PipeWire
        && ready.pipewire_socket_present
        && !selected.is_empty()
        && selected.iter().any(|s| s.pw_node_id.is_some())
}

/// Stronger live-stream gate that requires every selected node id to be
/// present in the current PipeWire graph query.
///
/// The legacy [`can_claim_live_streams`] helper is retained for callers that
/// already obtained node ids from an authoritative backend.  New portal code
/// should use this graph-bound predicate so a local output/window id cannot be
/// mistaken for a PipeWire node.
pub fn can_claim_live_streams_from_graph(
    ready: &ScreencastReadiness,
    graph: &PipeWireGraphProbe,
    selected: &[ScreencastSource],
) -> bool {
    ready.backend == ScreencastBackend::PipeWire
        && ready.pipewire_socket_present
        && graph.query_succeeded
        && !graph.video_sources.is_empty()
        && !selected.is_empty()
        && selected.iter().all(|source| {
            source
                .pw_node_id
                .is_some_and(|node_id| graph.contains_node(node_id))
        })
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
        let graph = PipeWireGraphProbe {
            query_succeeded: true,
            video_sources: vec![PipeWireVideoNode {
                node_id: 42,
                name: Some("slopos-output".into()),
                description: None,
                width: Some(1920),
                height: Some(1080),
            }],
            note: "test graph".into(),
        };
        assert!(can_claim_live_streams_from_graph(&r, &graph, &sources));
        let mut wrong = sources.clone();
        wrong[0].pw_node_id = Some(9001);
        assert!(!can_claim_live_streams_from_graph(&r, &graph, &wrong));
    }

    #[test]
    fn sources_from_outputs_and_plan() {
        let src = sources_from_outputs(&[("eDP-1".into(), 1920, 1080)], 100);
        assert_eq!(src.len(), 1);
        assert_eq!(src[0].id, 100);
        assert!(source_ids_for_portal(&src).is_empty());
        let mut live = src[0].clone();
        live.pw_node_id = Some(9001);
        assert_eq!(source_ids_for_portal(&[live]), vec![9001]);
        let plan = plan_list_pipewire_nodes();
        assert_eq!(plan.argv[0], "pw-cli");
    }

    #[test]
    fn parser_keeps_only_video_source_nodes_and_real_ids() {
        let output = r#"
id 42, type PipeWire:Interface:Node/3
        node.name = "slopos-output"
        node.description = "SLOPOS Output"
        media.class = "Video/Source"
        video.width = "1920"
        video.height = "1080"
id 43, type PipeWire:Interface:Node/3
        node.name = "audio-source"
        media.class = "Audio/Source"
id nope, type PipeWire:Interface:Node/3
        media.class = "Video/Source"
id 0, type PipeWire:Interface:Node/3
        media.class = "Video/Source"
"#;
        assert_eq!(
            parse_pipewire_video_nodes(output),
            vec![PipeWireVideoNode {
                node_id: 42,
                name: Some("slopos-output".into()),
                description: Some("SLOPOS Output".into()),
                width: Some(1920),
                height: Some(1080),
            }]
        );
    }

    #[test]
    fn parser_deduplicates_node_ids_and_rejects_wrong_media_class() {
        let output = r#"
id 7, type PipeWire:Interface:Node/3
        media.class = "Video/Source"
        node.name = "first"
id 7, type PipeWire:Interface:Node/3
        media.class = "Video/Source"
        node.name = "duplicate"
id 8, type PipeWire:Interface:Node/3
        media.class = "Video/Output"
"#;
        let nodes = parse_pipewire_video_nodes(output);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].node_id, 7);
        assert_eq!(nodes[0].name.as_deref(), Some("first"));
    }
}
