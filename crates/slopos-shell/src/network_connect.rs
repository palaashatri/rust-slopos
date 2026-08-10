//! NetworkManager Wi-Fi connect planning + best-effort nmcli spawn.
//!
//! Pure helpers validate requests and build nmcli-style argv. Execution is
//! best-effort: missing `nmcli` returns [`Err`] and never panics (macOS CI /
//! hosts without NetworkManager).

use std::process::Command;

/// Request to connect to a Wi-Fi network via NetworkManager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NmConnectRequest {
    pub ssid: String,
    pub password_optional: Option<String>,
}

impl NmConnectRequest {
    pub fn new(ssid: impl Into<String>) -> Self {
        Self {
            ssid: ssid.into(),
            password_optional: None,
        }
    }

    pub fn with_password(mut self, password: impl Into<String>) -> Self {
        self.password_optional = Some(password.into());
        self
    }
}

/// Validate an NM connect request (pure).
///
/// Rules:
/// - SSID non-empty
/// - SSID max 32 bytes (IEEE 802.11)
/// - SSID must not contain NUL
/// - Optional password must not contain NUL
pub fn validate_nm_connect_request(req: &NmConnectRequest) -> Result<(), String> {
    if req.ssid.is_empty() {
        return Err("ssid must be non-empty".to_string());
    }
    if req.ssid.len() > 32 {
        return Err("ssid exceeds 32 bytes".to_string());
    }
    if req.ssid.contains('\0') {
        return Err("ssid contains null byte".to_string());
    }
    if let Some(pw) = &req.password_optional {
        if pw.contains('\0') {
            return Err("password contains null byte".to_string());
        }
    }
    Ok(())
}

/// Ordered nmcli-style argv plan for connecting (pure, not executed).
///
/// Open network:
/// `["nmcli", "dev", "wifi", "connect", <ssid>]`
///
/// With password:
/// `["nmcli", "dev", "wifi", "connect", <ssid>, "password", <password>]`
///
/// Empty password string is treated as no password (open network argv).
pub fn nm_connect_plan(req: &NmConnectRequest) -> Vec<String> {
    let mut plan = vec![
        "nmcli".to_string(),
        "dev".to_string(),
        "wifi".to_string(),
        "connect".to_string(),
        req.ssid.clone(),
    ];
    if let Some(pw) = &req.password_optional {
        if !pw.is_empty() {
            plan.push("password".to_string());
            plan.push(pw.clone());
        }
    }
    plan
}

/// Validate then build a connect plan. Returns validation errors unchanged.
pub fn nm_connect_plan_validated(req: &NmConnectRequest) -> Result<Vec<String>, String> {
    validate_nm_connect_request(req)?;
    Ok(nm_connect_plan(req))
}

/// Human-readable one-line description of an nmcli argv plan (logs / UI).
pub fn describe_nm_connect_plan(plan: &[String]) -> String {
    if plan.is_empty() {
        return "exec: (empty plan)".to_string();
    }
    // Redact password argument value when present: … password <secret>
    let mut parts: Vec<&str> = Vec::with_capacity(plan.len());
    let mut redact_next = false;
    for arg in plan {
        if redact_next {
            parts.push("<redacted>");
            redact_next = false;
            continue;
        }
        if arg == "password" {
            redact_next = true;
        }
        parts.push(arg.as_str());
    }
    format!("exec: {}", parts.join(" "))
}

/// Best-effort spawn of a validated nmcli argv plan (like session `systemctl` spawn).
///
/// - Empty plan → `Err`
/// - Missing binary / spawn failure → `Err` (never panics)
/// - Child started successfully → `Ok(())` (does not wait for association)
pub fn execute_nm_connect_plan(plan: &[String]) -> Result<(), String> {
    if plan.is_empty() {
        return Err("nm connect plan is empty".to_string());
    }
    let program = &plan[0];
    let args = &plan[1..];
    match Command::new(program).args(args).spawn() {
        Ok(_child) => Ok(()),
        Err(err) => Err(format!(
            "could not spawn {}: {err}",
            describe_nm_connect_plan(plan)
        )),
    }
}

/// Validate → plan → best-effort nmcli spawn.
///
/// Pure validation errors and spawn failures both return `Err(String)`.
pub fn connect_wifi(req: &NmConnectRequest) -> Result<(), String> {
    let plan = nm_connect_plan_validated(req)?;
    execute_nm_connect_plan(&plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_accepts_normal_ssid() {
        let req = NmConnectRequest::new("HomeWiFi");
        assert!(validate_nm_connect_request(&req).is_ok());
    }

    #[test]
    fn validate_rejects_empty_ssid() {
        let req = NmConnectRequest::new("");
        let err = validate_nm_connect_request(&req).unwrap_err();
        assert!(err.contains("non-empty"));
    }

    #[test]
    fn validate_rejects_ssid_over_32_bytes() {
        let req = NmConnectRequest::new("a".repeat(33));
        let err = validate_nm_connect_request(&req).unwrap_err();
        assert!(err.contains("32"));
    }

    #[test]
    fn validate_accepts_ssid_exactly_32_bytes() {
        let req = NmConnectRequest::new("a".repeat(32));
        assert!(validate_nm_connect_request(&req).is_ok());
    }

    #[test]
    fn validate_rejects_null_in_ssid() {
        let req = NmConnectRequest {
            ssid: "bad\0ssid".into(),
            password_optional: None,
        };
        let err = validate_nm_connect_request(&req).unwrap_err();
        assert!(err.contains("null"));
    }

    #[test]
    fn validate_rejects_null_in_password() {
        let req = NmConnectRequest {
            ssid: "ok".into(),
            password_optional: Some("pw\0x".into()),
        };
        let err = validate_nm_connect_request(&req).unwrap_err();
        assert!(err.contains("null"));
    }

    #[test]
    fn nm_connect_plan_open_network() {
        let req = NmConnectRequest::new("Cafe");
        assert_eq!(
            nm_connect_plan(&req),
            vec!["nmcli", "dev", "wifi", "connect", "Cafe"]
        );
    }

    #[test]
    fn nm_connect_plan_with_password() {
        let req = NmConnectRequest::new("Home").with_password("s3cret");
        assert_eq!(
            nm_connect_plan(&req),
            vec!["nmcli", "dev", "wifi", "connect", "Home", "password", "s3cret"]
        );
    }

    #[test]
    fn nm_connect_plan_empty_password_is_open() {
        let req = NmConnectRequest {
            ssid: "Open".into(),
            password_optional: Some(String::new()),
        };
        assert_eq!(
            nm_connect_plan(&req),
            vec!["nmcli", "dev", "wifi", "connect", "Open"]
        );
    }

    #[test]
    fn nm_connect_plan_validated_fails_on_bad_ssid() {
        let req = NmConnectRequest::new("");
        assert!(nm_connect_plan_validated(&req).is_err());
    }

    #[test]
    fn nm_connect_plan_validated_ok() {
        let req = NmConnectRequest::new("Net").with_password("pw");
        let plan = nm_connect_plan_validated(&req).unwrap();
        assert_eq!(plan[0], "nmcli");
        assert!(plan.contains(&"password".to_string()));
    }

    #[test]
    fn describe_redacts_password() {
        let plan = nm_connect_plan(&NmConnectRequest::new("Home").with_password("s3cret"));
        let d = describe_nm_connect_plan(&plan);
        assert!(d.contains("nmcli"));
        assert!(d.contains("Home"));
        assert!(d.contains("<redacted>"));
        assert!(!d.contains("s3cret"));
    }

    #[test]
    fn execute_empty_plan_is_err() {
        let err = execute_nm_connect_plan(&[]).unwrap_err();
        assert!(err.contains("empty"));
    }

    #[test]
    fn execute_missing_binary_is_err_not_panic() {
        // Use a plan whose program cannot exist on any host.
        let plan = vec![
            "/nonexistent/slopos-i-nmcli-missing-binary-xyz".to_string(),
            "dev".into(),
            "wifi".into(),
            "connect".into(),
            "Test".into(),
        ];
        let err = execute_nm_connect_plan(&plan).unwrap_err();
        assert!(err.contains("could not spawn") || err.contains("No such file"));
    }

    #[test]
    fn connect_wifi_rejects_bad_ssid_before_spawn() {
        let err = connect_wifi(&NmConnectRequest::new("")).unwrap_err();
        assert!(err.contains("non-empty"));
    }

    #[test]
    fn connect_wifi_missing_nmcli_is_err_not_panic() {
        // When nmcli is absent (typical macOS CI), spawn fails cleanly.
        // When nmcli is present, spawn may succeed or fail later — either is Ok/Err, not panic.
        let req = NmConnectRequest::new("SLOPOS-ITestSsid");
        let _ = connect_wifi(&req);
    }
}
