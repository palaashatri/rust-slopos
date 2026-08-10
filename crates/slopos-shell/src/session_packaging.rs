//! Session packaging health checks (greeter files, start script) — pure paths.
//!
//! Greeter/session evidence is packaging-only unless a host run proves live DM.
//! `session_entry_smoke_report` always sets `live_greeter_verified: false`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// Expected packaging artifacts for a greeter-capable install.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionPackagingLayout {
    pub wayland_session_desktop: PathBuf,
    pub xsession_desktop: PathBuf,
    pub start_script: PathBuf,
    pub user_service: PathBuf,
}

impl SessionPackagingLayout {
    /// Default FHS layout under a prefix (e.g. `/usr`).
    pub fn under_prefix(prefix: impl AsRef<Path>) -> Self {
        let p = prefix.as_ref();
        Self {
            wayland_session_desktop: p.join("share/wayland-sessions/slopos-i.desktop"),
            xsession_desktop: p.join("share/xsessions/slopos-i.desktop"),
            start_script: p.join("bin/start-slopos-i"),
            user_service: p.join("lib/systemd/user/slopos-i.service"),
        }
    }

    /// Repo-local packaging tree (development).
    pub fn from_repo_packaging(repo_root: impl AsRef<Path>) -> Self {
        let r = repo_root.as_ref();
        Self {
            wayland_session_desktop: r.join("packaging/slopos-i-wayland.desktop"),
            xsession_desktop: r.join("packaging/slopos-i.desktop"),
            start_script: r.join("scripts/start-slopos-i"),
            user_service: r.join("packaging/slopos-i.service"),
        }
    }
}

/// Result of checking packaging presence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PackagingHealth {
    pub wayland_session_ok: bool,
    pub xsession_ok: bool,
    pub start_script_ok: bool,
    pub user_service_ok: bool,
}

impl PackagingHealth {
    pub fn all_ok(&self) -> bool {
        self.wayland_session_ok && self.xsession_ok && self.start_script_ok && self.user_service_ok
    }

    pub fn score_points(&self) -> u8 {
        let mut n = 0u8;
        if self.wayland_session_ok {
            n += 25;
        }
        if self.xsession_ok {
            n += 25;
        }
        if self.start_script_ok {
            n += 25;
        }
        if self.user_service_ok {
            n += 25;
        }
        n
    }
}

/// Check which packaging files exist (pure filesystem probe).
pub fn check_packaging_health(layout: &SessionPackagingLayout) -> PackagingHealth {
    PackagingHealth {
        wayland_session_ok: layout.wayland_session_desktop.is_file(),
        xsession_ok: layout.xsession_desktop.is_file(),
        start_script_ok: layout.start_script.is_file(),
        user_service_ok: layout.user_service.is_file(),
    }
}

/// Greeter → session readiness checklist (pure content validation + files).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GreeterSessionReadiness {
    pub packaging: PackagingHealth,
    pub wayland_desktop_valid: bool,
    pub xsession_desktop_valid: bool,
    pub start_script_executable_bit: bool,
    pub desktop_names_ok: bool,
    pub notes: Vec<String>,
}

impl GreeterSessionReadiness {
    /// True when files exist, desktops validate, DesktopNames present, and the
    /// start script is executable. Does **not** claim a live display manager
    /// was exercised.
    pub fn install_ready(&self) -> bool {
        self.packaging.all_ok()
            && self.wayland_desktop_valid
            && self.xsession_desktop_valid
            && self.desktop_names_ok
            && self.start_script_executable_bit
    }

    pub fn score_points(&self) -> u8 {
        let mut n = 0u8;
        if self.packaging.wayland_session_ok {
            n += 15;
        }
        if self.packaging.xsession_ok {
            n += 15;
        }
        if self.packaging.start_script_ok {
            n += 15;
        }
        if self.packaging.user_service_ok {
            n += 10;
        }
        if self.wayland_desktop_valid {
            n += 15;
        }
        if self.xsession_desktop_valid {
            n += 15;
        }
        if self.desktop_names_ok {
            n += 10;
        }
        if self.start_script_executable_bit {
            n += 5;
        }
        n
    }
}

/// Structured session-entry smoke report (packaging evidence only).
///
/// Never claims a live greeter/DM login was exercised:
/// `live_greeter_verified` is **always** `false`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SessionEntrySmokeReport {
    /// Files present under the packaging layout.
    pub packaging: PackagingHealth,
    /// True when packaging + desktop validation + exec bit look install-ready.
    /// Does **not** imply live display-manager verification.
    pub install_ready: bool,
    /// Start script has the executable bit (Unix) or exists as a file (non-Unix).
    pub start_script_executable: bool,
    /// Both session desktops declare non-empty `DesktopNames`.
    pub desktop_names_ok: bool,
    /// Always `false`. Honest: this report never verifies a live greeter/DM.
    pub live_greeter_verified: bool,
}

impl SessionEntrySmokeReport {
    /// Compact JSON one-liner for verify scripts / CI logs.
    pub fn to_json_line(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|e| e.to_string())
    }
}

/// Build a pure session-entry smoke report from `layout`.
///
/// Aggregates packaging health, greeter `install_ready`, start-script
/// executable bit, and DesktopNames presence. Always sets
/// `live_greeter_verified: false` — no live DM claim.
pub fn session_entry_smoke_report(layout: &SessionPackagingLayout) -> SessionEntrySmokeReport {
    let readiness = check_greeter_session_readiness(layout);
    SessionEntrySmokeReport {
        packaging: readiness.packaging.clone(),
        install_ready: readiness.install_ready(),
        start_script_executable: readiness.start_script_executable_bit,
        desktop_names_ok: readiness.desktop_names_ok,
        live_greeter_verified: false,
    }
}

/// Probe greeter session readiness from layout paths (reads desktop files).
pub fn check_greeter_session_readiness(layout: &SessionPackagingLayout) -> GreeterSessionReadiness {
    let packaging = check_packaging_health(layout);
    let mut notes = Vec::new();

    let wayland_desktop_valid = read_and_validate(&layout.wayland_session_desktop, &mut notes);
    let xsession_desktop_valid = read_and_validate(&layout.xsession_desktop, &mut notes);

    let desktop_names_ok = desktop_names_present(&layout.wayland_session_desktop)
        && desktop_names_present(&layout.xsession_desktop);
    if !desktop_names_ok {
        notes.push(
            "DesktopNames=SLOPOS-I required on both wayland-sessions and xsessions entries".into(),
        );
    }

    let start_script_executable_bit = is_executable(&layout.start_script);
    if packaging.start_script_ok && !start_script_executable_bit {
        notes.push("start-slopos-i exists but executable bit not set".into());
    }

    // Systemd user unit sanity (ExecStart points at start-slopos-i).
    if packaging.user_service_ok {
        match std::fs::read_to_string(&layout.user_service) {
            Ok(unit) => {
                if !unit.contains("start-slopos-i") {
                    notes.push("user service unit does not reference start-slopos-i".into());
                } else {
                    notes.push("user service unit references start-slopos-i".into());
                }
            }
            Err(e) => notes.push(format!("cannot read user service: {e}")),
        }
    }

    if packaging.all_ok()
        && wayland_desktop_valid
        && xsession_desktop_valid
        && desktop_names_ok
        && start_script_executable_bit
    {
        notes.push(
            "Install artifacts OK — live greeter login still requires DM + seat on target host"
                .into(),
        );
    } else if packaging.all_ok() && wayland_desktop_valid && xsession_desktop_valid {
        notes.push(
            "Packaging present and desktops validate; check DesktopNames / executable bit for full install_ready"
                .into(),
        );
    }

    GreeterSessionReadiness {
        packaging,
        wayland_desktop_valid,
        xsession_desktop_valid,
        start_script_executable_bit,
        desktop_names_ok,
        notes,
    }
}

fn read_and_validate(path: &Path, notes: &mut Vec<String>) -> bool {
    match std::fs::read_to_string(path) {
        Ok(content) => match validate_session_desktop(&content) {
            Ok(()) => true,
            Err(errs) => {
                notes.push(format!("{}: {}", path.display(), errs.join("; ")));
                false
            }
        },
        Err(e) => {
            notes.push(format!("cannot read {}: {e}", path.display()));
            false
        }
    }
}

fn desktop_names_present(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .ok()
        .map(|c| {
            let keys = parse_desktop_keys(&c);
            keys.get("DesktopNames")
                .map(|v| !v.is_empty())
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

/// Parse `Key=Value` lines from a `.desktop` file into a map.
///
/// - Ignores blank lines and comments (`#...`).
/// - Ignores section headers (`[Desktop Entry]`).
/// - First occurrence of a key wins.
/// - Values keep surrounding whitespace after the first `=`.
pub fn parse_desktop_keys(content: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        map.entry(key.to_string())
            .or_insert_with(|| value.trim().to_string());
    }
    map
}

/// Validate a session `.desktop` entry for greeter use.
///
/// Requires:
/// - `Type=Application`
/// - `Exec` containing `start-slopos-i`
/// - `Name` non-empty
/// - `DesktopNames` containing `SLOPOS-I` (when present; recommended)
///
/// Soft checks (warnings via notes path, not hard errors unless absent on
/// readiness probe): `TryExec` should also mention `start-slopos-i`.
///
/// Returns `Ok(())` on success, or `Err` with one message per failed rule.
pub fn validate_session_desktop(content: &str) -> Result<(), Vec<String>> {
    let keys = parse_desktop_keys(content);
    let mut errors = Vec::new();

    match keys.get("Type").map(String::as_str) {
        Some("Application") => {}
        Some(other) => errors.push(format!("Type must be Application (got '{other}')")),
        None => errors.push("missing required key: Type".to_string()),
    }

    match keys.get("Exec").map(String::as_str) {
        Some(exec) if exec.contains("start-slopos-i") => {}
        Some(exec) => errors.push(format!("Exec must contain start-slopos-i (got '{exec}')")),
        None => errors.push("missing required key: Exec".to_string()),
    }

    match keys.get("Name").map(String::as_str) {
        Some(name) if !name.is_empty() => {}
        Some(_) => errors.push("Name must be non-empty".to_string()),
        None => errors.push("missing required key: Name".to_string()),
    }

    // DesktopNames is required for install_ready greeter checklist.
    match keys.get("DesktopNames").map(String::as_str) {
        Some(names) if names.split(';').any(|n| n.trim() == "SLOPOS-I") => {}
        Some(names) if !names.is_empty() => {
            // Present but not SLOPOS-I — still valid Type/Exec, note as error
            // only when empty; non-SLOPOS-I is a soft failure for readiness.
            if !names.to_ascii_lowercase().contains("slopos") {
                errors.push(format!(
                    "DesktopNames should include SLOPOS-I (got '{names}')"
                ));
            }
        }
        Some(_) => errors.push("DesktopNames must be non-empty".to_string()),
        None => errors.push("missing recommended key: DesktopNames=SLOPOS-I".to_string()),
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn repo_root() -> PathBuf {
        // Crate is crates/slopos-shell → repo root is ../..
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn repo_packaging_layout_is_complete() {
        let root = repo_root();
        let layout = SessionPackagingLayout::from_repo_packaging(&root);
        let health = check_packaging_health(&layout);
        assert!(
            health.all_ok(),
            "expected full packaging tree in repo: {health:?} layout={layout:?}"
        );
        assert_eq!(health.score_points(), 100);
    }

    #[test]
    fn under_prefix_paths_are_fhs() {
        let l = SessionPackagingLayout::under_prefix("/usr");
        assert!(l
            .wayland_session_desktop
            .ends_with("share/wayland-sessions/slopos-i.desktop"));
        assert!(l.start_script.ends_with("bin/start-slopos-i"));
    }

    #[test]
    fn parse_desktop_keys_extracts_name_exec_type() {
        let content = "\
[Desktop Entry]
# a comment
Name=SLOPOS-I
Exec=start-slopos-i
Type=Application
DesktopNames=SLOPOS-I
";
        let keys = parse_desktop_keys(content);
        assert_eq!(keys.get("Name").map(String::as_str), Some("SLOPOS-I"));
        assert_eq!(keys.get("Exec").map(String::as_str), Some("start-slopos-i"));
        assert_eq!(keys.get("Type").map(String::as_str), Some("Application"));
        assert_eq!(
            keys.get("DesktopNames").map(String::as_str),
            Some("SLOPOS-I")
        );
        assert!(!keys.contains_key("# a comment"));
    }

    #[test]
    fn parse_desktop_keys_first_occurrence_wins() {
        let content = "Name=First\nName=Second\n";
        let keys = parse_desktop_keys(content);
        assert_eq!(keys.get("Name").map(String::as_str), Some("First"));
    }

    #[test]
    fn validate_session_desktop_accepts_valid() {
        let content = "\
[Desktop Entry]
Name=SLOPOS-I
Exec=start-slopos-i
Type=Application
DesktopNames=SLOPOS-I
";
        assert!(validate_session_desktop(content).is_ok());
    }

    #[test]
    fn validate_session_desktop_accepts_absolute_exec() {
        let content = "\
Name=SLOPOS-I
Exec=/usr/local/bin/start-slopos-i
Type=Application
DesktopNames=SLOPOS-I
";
        assert!(validate_session_desktop(content).is_ok());
    }

    #[test]
    fn validate_session_desktop_requires_desktop_names() {
        let content = "\
Name=SLOPOS-I
Exec=start-slopos-i
Type=Application
";
        let err = validate_session_desktop(content).unwrap_err();
        assert!(err.iter().any(|e| e.contains("DesktopNames")));
    }

    #[test]
    fn validate_session_desktop_rejects_bad_type() {
        let content = "Name=X\nExec=start-slopos-i\nType=Link\n";
        let err = validate_session_desktop(content).unwrap_err();
        assert!(err.iter().any(|e| e.contains("Type")));
    }

    #[test]
    fn validate_session_desktop_rejects_missing_and_empty() {
        let content = "Type=Application\nName=\n";
        let err = validate_session_desktop(content).unwrap_err();
        assert!(err.iter().any(|e| e.contains("Exec")));
        assert!(err.iter().any(|e| e.contains("Name")));
    }

    #[test]
    fn validate_session_desktop_rejects_wrong_exec() {
        let content = "Name=X\nExec=gnome-session\nType=Application\n";
        let err = validate_session_desktop(content).unwrap_err();
        assert!(err.iter().any(|e| e.contains("start-slopos-i")));
    }

    #[test]
    fn packaging_slopos_i_desktop_validates() {
        let path = repo_root().join("packaging/slopos-i.desktop");
        let content =
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let keys = parse_desktop_keys(&content);
        assert_eq!(keys.get("Name").map(String::as_str), Some("SLOPOS-I"));
        assert!(
            keys.get("Exec")
                .is_some_and(|e| e.contains("start-slopos-i")),
            "Exec keys={keys:?}"
        );
        assert_eq!(keys.get("Type").map(String::as_str), Some("Application"));
        validate_session_desktop(&content).expect("packaging/slopos-i.desktop must validate");
    }

    #[test]
    fn packaging_slopos_i_wayland_desktop_validates() {
        let path = repo_root().join("packaging/slopos-i-wayland.desktop");
        let content =
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        validate_session_desktop(&content)
            .expect("packaging/slopos-i-wayland.desktop must validate");
    }

    #[test]
    fn repo_packaging_files_validate_via_layout() {
        let root = repo_root();
        let layout = SessionPackagingLayout::from_repo_packaging(&root);
        let health = check_packaging_health(&layout);
        assert!(health.all_ok(), "layout incomplete: {health:?}");

        for path in [&layout.wayland_session_desktop, &layout.xsession_desktop] {
            let content =
                fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            validate_session_desktop(&content).unwrap_or_else(|errs| {
                panic!("{} failed validate: {errs:?}", path.display());
            });
        }
    }

    #[test]
    fn greeter_session_readiness_repo() {
        let root = repo_root();
        let layout = SessionPackagingLayout::from_repo_packaging(&root);
        let ready = check_greeter_session_readiness(&layout);
        assert!(
            ready.install_ready(),
            "repo packaging should be greeter-install-ready: {ready:?}"
        );
        assert!(ready.score_points() >= 90, "score={}", ready.score_points());
    }

    #[test]
    fn session_entry_smoke_report_repo_honest() {
        let root = repo_root();
        let layout = SessionPackagingLayout::from_repo_packaging(&root);
        let report = session_entry_smoke_report(&layout);

        assert!(
            report.packaging.all_ok(),
            "repo packaging health incomplete: {:?}",
            report.packaging
        );
        assert!(
            report.install_ready,
            "repo packaging should be install_ready: {report:?}"
        );
        assert!(
            report.start_script_executable,
            "start-slopos-i must be executable in repo"
        );
        assert!(
            report.desktop_names_ok,
            "DesktopNames must be present on both session desktops"
        );
        // Honest: packaging smoke never verifies a live greeter/DM.
        assert!(
            !report.live_greeter_verified,
            "live_greeter_verified must always be false"
        );

        let line = report.to_json_line().expect("json one-liner");
        assert!(line.contains("\"live_greeter_verified\":false"), "{line}");
        assert!(line.contains("\"install_ready\":true"), "{line}");
        // Optional CI one-liner (cargo test -- --nocapture shows this once).
        eprintln!("session_entry_smoke_report {line}");
    }
}
