//! Session crash recovery — pure checkpoint plan (no process spawn).
//!
//! Serializes a session checkpoint (Wayland display, compositor/shell bins,
//! running clients) and produces an ordered recovery plan after a crash.

use serde::{Deserialize, Serialize};

/// A client process tracked in the session checkpoint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointClient {
    pub bundle_id: String,
    pub pid: u32,
}

/// Snapshot of a running session used for crash recovery.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionCheckpoint {
    pub wayland_display: String,
    pub compositor_bin: String,
    pub shell_bin: String,
    pub clients: Vec<CheckpointClient>,
}

/// Ordered steps to restore a session from a checkpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecoveryStep {
    RestartCompositor {
        bin: String,
        wayland_display: String,
    },
    RestartShell {
        bin: String,
    },
    RelaunchClient {
        bundle_id: String,
    },
}

impl SessionCheckpoint {
    pub fn new(
        wayland_display: impl Into<String>,
        compositor_bin: impl Into<String>,
        shell_bin: impl Into<String>,
    ) -> Self {
        Self {
            wayland_display: wayland_display.into(),
            compositor_bin: compositor_bin.into(),
            shell_bin: shell_bin.into(),
            clients: Vec::new(),
        }
    }

    pub fn with_client(mut self, bundle_id: impl Into<String>, pid: u32) -> Self {
        self.clients.push(CheckpointClient {
            bundle_id: bundle_id.into(),
            pid,
        });
        self
    }

    /// Serialize checkpoint to JSON.
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|e| e.to_string())
    }

    /// Serialize checkpoint to pretty-printed JSON.
    pub fn to_json_pretty(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }

    /// Deserialize checkpoint from JSON.
    pub fn from_json(s: &str) -> Result<Self, String> {
        serde_json::from_str(s).map_err(|e| e.to_string())
    }

    /// Serialize to a simple line-oriented text format (no JSON dependency for
    /// consumers that prefer plain text).
    ///
    /// Format:
    /// ```text
    /// wayland_display=<value>
    /// compositor_bin=<value>
    /// shell_bin=<value>
    /// client=<bundle_id>\t<pid>
    /// ```
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("wayland_display={}\n", self.wayland_display));
        out.push_str(&format!("compositor_bin={}\n", self.compositor_bin));
        out.push_str(&format!("shell_bin={}\n", self.shell_bin));
        for c in &self.clients {
            out.push_str(&format!("client={}\t{}\n", c.bundle_id, c.pid));
        }
        out
    }

    /// Parse the simple text format produced by [`SessionCheckpoint::to_text`].
    pub fn from_text(s: &str) -> Result<Self, String> {
        let mut wayland_display = None;
        let mut compositor_bin = None;
        let mut shell_bin = None;
        let mut clients = Vec::new();

        for (lineno, raw) in s.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(format!("line {}: expected key=value", lineno + 1));
            };
            match key {
                "wayland_display" => wayland_display = Some(value.to_string()),
                "compositor_bin" => compositor_bin = Some(value.to_string()),
                "shell_bin" => shell_bin = Some(value.to_string()),
                "client" => {
                    let Some((bundle_id, pid_s)) = value.split_once('\t') else {
                        return Err(format!(
                            "line {}: client must be bundle_id\\tpid",
                            lineno + 1
                        ));
                    };
                    let pid: u32 = pid_s
                        .parse()
                        .map_err(|_| format!("line {}: invalid pid '{pid_s}'", lineno + 1))?;
                    clients.push(CheckpointClient {
                        bundle_id: bundle_id.to_string(),
                        pid,
                    });
                }
                other => {
                    return Err(format!("line {}: unknown key '{other}'", lineno + 1));
                }
            }
        }

        Ok(Self {
            wayland_display: wayland_display
                .ok_or_else(|| "missing wayland_display".to_string())?,
            compositor_bin: compositor_bin.ok_or_else(|| "missing compositor_bin".to_string())?,
            shell_bin: shell_bin.ok_or_else(|| "missing shell_bin".to_string())?,
            clients,
        })
    }
}

/// Build an ordered recovery plan from a checkpoint.
///
/// Order: compositor → shell → each client (by checkpoint order).
pub fn recovery_plan(checkpoint: &SessionCheckpoint) -> Vec<RecoveryStep> {
    let mut steps = Vec::with_capacity(2 + checkpoint.clients.len());
    steps.push(RecoveryStep::RestartCompositor {
        bin: checkpoint.compositor_bin.clone(),
        wayland_display: checkpoint.wayland_display.clone(),
    });
    steps.push(RecoveryStep::RestartShell {
        bin: checkpoint.shell_bin.clone(),
    });
    for c in &checkpoint.clients {
        steps.push(RecoveryStep::RelaunchClient {
            bundle_id: c.bundle_id.clone(),
        });
    }
    steps
}

/// Whether a crash recovery should be attempted.
///
/// Rules:
/// - Never recover on clean exit (`exit_code == 0`).
/// - Never recover when `restart_count >= max`.
/// - Otherwise recover on non-zero exit (crash / abnormal termination).
pub fn should_attempt_recovery(exit_code: i32, restart_count: u32, max: u32) -> bool {
    if exit_code == 0 {
        return false;
    }
    if restart_count >= max {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_checkpoint() -> SessionCheckpoint {
        SessionCheckpoint::new("wayland-1", "/usr/bin/labwc", "/usr/bin/slopos-shell")
            .with_client("com.slopos.finder", 1001)
            .with_client("com.slopos.terminal", 1002)
    }

    #[test]
    fn recovery_plan_order() {
        let cp = sample_checkpoint();
        let plan = recovery_plan(&cp);
        assert_eq!(
            plan,
            vec![
                RecoveryStep::RestartCompositor {
                    bin: "/usr/bin/labwc".into(),
                    wayland_display: "wayland-1".into(),
                },
                RecoveryStep::RestartShell {
                    bin: "/usr/bin/slopos-shell".into(),
                },
                RecoveryStep::RelaunchClient {
                    bundle_id: "com.slopos.finder".into(),
                },
                RecoveryStep::RelaunchClient {
                    bundle_id: "com.slopos.terminal".into(),
                },
            ]
        );
    }

    #[test]
    fn recovery_plan_empty_clients() {
        let cp = SessionCheckpoint::new("wayland-0", "labwc", "slopos-shell");
        let plan = recovery_plan(&cp);
        assert_eq!(plan.len(), 2);
        assert!(matches!(plan[0], RecoveryStep::RestartCompositor { .. }));
        assert!(matches!(plan[1], RecoveryStep::RestartShell { .. }));
    }

    #[test]
    fn json_roundtrip() {
        let cp = sample_checkpoint();
        let json = cp.to_json().unwrap();
        let back = SessionCheckpoint::from_json(&json).unwrap();
        assert_eq!(cp, back);
    }

    #[test]
    fn text_roundtrip() {
        let cp = sample_checkpoint();
        let text = cp.to_text();
        let back = SessionCheckpoint::from_text(&text).unwrap();
        assert_eq!(cp, back);
    }

    #[test]
    fn text_ignores_comments_and_blanks() {
        let text = "\
# comment
wayland_display=wayland-2

compositor_bin=labwc
shell_bin=slopos-shell
client=com.slopos.app\t42
";
        let cp = SessionCheckpoint::from_text(text).unwrap();
        assert_eq!(cp.wayland_display, "wayland-2");
        assert_eq!(cp.clients.len(), 1);
        assert_eq!(cp.clients[0].pid, 42);
    }

    #[test]
    fn text_rejects_missing_fields() {
        let err = SessionCheckpoint::from_text("wayland_display=w\n").unwrap_err();
        assert!(err.contains("missing"));
    }

    #[test]
    fn should_attempt_recovery_rules() {
        // Clean exit → never
        assert!(!should_attempt_recovery(0, 0, 3));
        // Crash with budget left → yes
        assert!(should_attempt_recovery(1, 0, 3));
        assert!(should_attempt_recovery(139, 2, 3));
        // Budget exhausted → no
        assert!(!should_attempt_recovery(1, 3, 3));
        assert!(!should_attempt_recovery(1, 5, 3));
        // max == 0 → never
        assert!(!should_attempt_recovery(1, 0, 0));
    }
}
