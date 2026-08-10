//! Polkit authentication agent — pure request handling + Linux session-bus path.
//!
//! Pure types/handlers are unit-tested. On Linux, [`try_register_polkit_agent`]
//! registers a best-effort agent on the session bus (non-fatal if polkit absent).

use std::collections::HashMap;

/// One polkit authentication request (mirrors org.freedesktop.PolicyKit1.AuthenticationAgent).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolkitAuthRequest {
    pub action_id: String,
    pub message: String,
    pub icon_name: String,
    pub cookie: String,
    pub identities: Vec<String>,
}

/// Result of handling an auth request in-process (before interactive UI).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PolkitAuthDecision {
    /// User supplied a password; agent would complete authentication.
    Authenticated { identity: String },
    /// User cancelled.
    Cancelled,
    /// Request rejected (invalid / empty).
    Rejected { reason: String },
}

/// Validate a polkit request (pure).
pub fn validate_polkit_request(req: &PolkitAuthRequest) -> Result<(), String> {
    if req.action_id.trim().is_empty() {
        return Err("action_id empty".into());
    }
    if req.cookie.trim().is_empty() {
        return Err("cookie empty".into());
    }
    if req.identities.is_empty() {
        return Err("no identities".into());
    }
    Ok(())
}

/// Pure decision helper used by the agent when a password is provided or cancelled.
pub fn handle_polkit_auth(
    req: &PolkitAuthRequest,
    password: Option<&str>,
    cancel: bool,
) -> PolkitAuthDecision {
    if let Err(reason) = validate_polkit_request(req) {
        return PolkitAuthDecision::Rejected { reason };
    }
    if cancel {
        return PolkitAuthDecision::Cancelled;
    }
    match password {
        Some(p) if !p.is_empty() => PolkitAuthDecision::Authenticated {
            identity: req
                .identities
                .first()
                .cloned()
                .unwrap_or_else(|| "unix-user:0".into()),
        },
        _ => PolkitAuthDecision::Rejected {
            reason: "empty password".into(),
        },
    }
}

/// In-memory agent state for tests / non-interactive session.
#[derive(Clone, Debug, Default)]
pub struct PolkitAgentState {
    pub pending: HashMap<String, PolkitAuthRequest>,
    pub completed: Vec<(String, PolkitAuthDecision)>,
}

impl PolkitAgentState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn begin(&mut self, req: PolkitAuthRequest) -> Result<(), String> {
        validate_polkit_request(&req)?;
        self.pending.insert(req.cookie.clone(), req);
        Ok(())
    }

    pub fn complete(
        &mut self,
        cookie: &str,
        password: Option<&str>,
        cancel: bool,
    ) -> Option<PolkitAuthDecision> {
        let req = self.pending.remove(cookie)?;
        let decision = handle_polkit_auth(&req, password, cancel);
        self.completed.push((cookie.to_string(), decision.clone()));
        Some(decision)
    }
}

/// Well-known path/name constants for the session agent.
pub const POLKIT_AGENT_BUS_NAME: &str = "org.slopos-i.PolicyKit1.AuthenticationAgent";
pub const POLKIT_AGENT_PATH: &str = "/org/slopos-i/PolicyKit1/AuthenticationAgent";
pub const POLKIT_AGENT_INTERFACE: &str = "org.freedesktop.PolicyKit1.AuthenticationAgent";

/// Best-effort register polkit agent on the session bus (Linux).
pub fn try_register_polkit_agent() -> bool {
    #[cfg(target_os = "linux")]
    {
        match linux::register() {
            Ok(()) => {
                tracing::info!(
                    bus = POLKIT_AGENT_BUS_NAME,
                    "polkit authentication agent registered"
                );
                true
            }
            Err(err) => {
                tracing::debug!(error = %err, "polkit agent registration skipped");
                false
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use parking_lot::Mutex;
    use std::sync::{Arc, Mutex as StdMutex};
    use zbus::blocking::connection::Builder as ConnectionBuilder;
    use zbus::blocking::Connection;
    use zbus::interface;

    static REG: StdMutex<Option<Connection>> = StdMutex::new(None);

    struct AgentIface {
        state: Arc<Mutex<PolkitAgentState>>,
    }

    #[interface(name = "org.freedesktop.PolicyKit1.AuthenticationAgent")]
    impl AgentIface {
        /// Simplified BeginAuthentication — stores request for shell UI completion.
        fn begin_authentication(
            &self,
            action_id: &str,
            message: &str,
            icon_name: &str,
            cookie: &str,
            identities: Vec<String>,
        ) -> zbus::fdo::Result<()> {
            let req = PolkitAuthRequest {
                action_id: action_id.into(),
                message: message.into(),
                icon_name: icon_name.into(),
                cookie: cookie.into(),
                identities,
            };
            self.state
                .lock()
                .begin(req)
                .map_err(zbus::fdo::Error::Failed)?;
            Ok(())
        }

        fn cancel_authentication(&self, cookie: &str) -> zbus::fdo::Result<()> {
            let _ = self.state.lock().complete(cookie, None, true);
            Ok(())
        }
    }

    pub fn register() -> Result<(), String> {
        let state = Arc::new(Mutex::new(PolkitAgentState::new()));
        let conn = ConnectionBuilder::session()
            .map_err(|e| e.to_string())?
            .name(POLKIT_AGENT_BUS_NAME)
            .map_err(|e| e.to_string())?
            .serve_at(POLKIT_AGENT_PATH, AgentIface { state })
            .map_err(|e| e.to_string())?
            .build()
            .map_err(|e| e.to_string())?;
        *REG.lock().unwrap() = Some(conn);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> PolkitAuthRequest {
        PolkitAuthRequest {
            action_id: "org.freedesktop.packagekit.package-install".into(),
            message: "Install package".into(),
            icon_name: "package".into(),
            cookie: "c-1".into(),
            identities: vec!["unix-user:1000".into()],
        }
    }

    #[test]
    fn validate_rejects_empty_action() {
        let mut r = sample();
        r.action_id.clear();
        assert!(validate_polkit_request(&r).is_err());
    }

    #[test]
    fn handle_auth_password_and_cancel() {
        let r = sample();
        assert!(matches!(
            handle_polkit_auth(&r, Some("secret"), false),
            PolkitAuthDecision::Authenticated { .. }
        ));
        assert_eq!(
            handle_polkit_auth(&r, None, true),
            PolkitAuthDecision::Cancelled
        );
        assert!(matches!(
            handle_polkit_auth(&r, Some(""), false),
            PolkitAuthDecision::Rejected { .. }
        ));
    }

    #[test]
    fn agent_state_begin_complete() {
        let mut s = PolkitAgentState::new();
        s.begin(sample()).unwrap();
        assert_eq!(s.pending.len(), 1);
        let d = s.complete("c-1", Some("x"), false).unwrap();
        assert!(matches!(d, PolkitAuthDecision::Authenticated { .. }));
        assert!(s.pending.is_empty());
        assert_eq!(s.completed.len(), 1);
    }

    #[test]
    fn try_register_polkit_is_safe_off_linux() {
        let _ = try_register_polkit_agent();
    }
}
