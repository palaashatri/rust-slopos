//! Extra FreeDesktop portal pure handlers (Secret, Print, Inhibit).
//!
//! Complements [`crate::portal`] Screenshot / Settings / OpenURI / FileChooser /
//! ScreenCast. These are protocol-level plans — no keyring or CUPS I/O here.
//!
//! # Inhibit cookies
//! Pure [`handle_inhibit`] issues cookies; [`register_inhibit_cookie`] /
//! [`release_inhibit_cookie`] keep a **process-wide** table the shell can poll
//! via [`active_inhibits`] / [`active_idle_inhibit_state`] each frame. This is
//! not logind Inhibit — only SLOPOS-I idle policy consumes it.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

/// org.freedesktop.impl.portal.Secret — Retrieve secret request (pure).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortalSecretRequest {
    pub app_id: String,
    /// Opaque token from the sandboxed app (not a password).
    pub token: Vec<u8>,
}

/// Result of a secret retrieve plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PortalSecretResult {
    /// Host keyring would be queried; `label` is the lookup key.
    Lookup { label: String },
    /// Rejected (empty app_id / invalid).
    Rejected { reason: String },
}

/// Pure Secret portal: validate and plan a keyring lookup label.
pub fn handle_secret_retrieve(req: &PortalSecretRequest) -> PortalSecretResult {
    if req.app_id.trim().is_empty() {
        return PortalSecretResult::Rejected {
            reason: "empty app_id".into(),
        };
    }
    if req.app_id.contains('\0') {
        return PortalSecretResult::Rejected {
            reason: "app_id contains null".into(),
        };
    }
    // Label scheme: slopos-i.portal.secret.<app_id>
    let label = format!("slopos-i.portal.secret.{}", req.app_id.trim());
    PortalSecretResult::Lookup { label }
}

/// Print portal request (simplified).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortalPrintRequest {
    pub title: String,
    /// Absolute path or URI of document to print.
    pub document_uri: String,
    pub modal: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PortalPrintResult {
    /// Ready to hand off to CUPS/lp.
    Queued {
        job_id: u64,
        argv: Vec<String>,
    },
    Rejected {
        reason: String,
    },
}

static NEXT_PRINT_JOB: AtomicU64 = AtomicU64::new(1);

/// Pure Print: validate and build `lp` argv plan.
pub fn handle_print_request(req: &PortalPrintRequest) -> PortalPrintResult {
    if req.title.trim().is_empty() {
        return PortalPrintResult::Rejected {
            reason: "empty title".into(),
        };
    }
    let uri = req.document_uri.trim();
    if uri.is_empty() {
        return PortalPrintResult::Rejected {
            reason: "empty document_uri".into(),
        };
    }
    if uri.contains('\0') {
        return PortalPrintResult::Rejected {
            reason: "document_uri contains null".into(),
        };
    }
    // Accept file: or absolute path.
    let path = if let Some(rest) = uri.strip_prefix("file://") {
        rest.to_string()
    } else if uri.starts_with('/') {
        uri.to_string()
    } else {
        return PortalPrintResult::Rejected {
            reason: "document_uri must be file:// or absolute path".into(),
        };
    };
    let job_id = NEXT_PRINT_JOB.fetch_add(1, Ordering::Relaxed);
    PortalPrintResult::Queued {
        job_id,
        argv: vec!["lp".into(), "-t".into(), req.title.trim().into(), path],
    }
}

/// Session inhibit portal (idle/sleep/logout) — pure token table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InhibitFlag {
    Logout = 1,
    UserSwitch = 2,
    Suspend = 4,
    Idle = 8,
}

impl InhibitFlag {
    pub fn from_bits(bits: u32) -> Vec<InhibitFlag> {
        let mut out = Vec::new();
        if bits & 1 != 0 {
            out.push(Self::Logout);
        }
        if bits & 2 != 0 {
            out.push(Self::UserSwitch);
        }
        if bits & 4 != 0 {
            out.push(Self::Suspend);
        }
        if bits & 8 != 0 {
            out.push(Self::Idle);
        }
        out
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Logout => "logout",
            Self::UserSwitch => "switch",
            Self::Suspend => "suspend",
            Self::Idle => "idle",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortalInhibitRequest {
    pub app_id: String,
    pub window: String,
    pub flags: u32,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortalInhibitCookie {
    pub cookie: u32,
    pub flags: u32,
    pub app_id: String,
    pub reason: String,
}

static NEXT_INHIBIT: AtomicU64 = AtomicU64::new(1);

/// Process-wide active portal inhibit cookies (D-Bus Inhibit + pure tests).
fn inhibit_cookie_store() -> &'static Mutex<Vec<PortalInhibitCookie>> {
    static STORE: OnceLock<Mutex<Vec<PortalInhibitCookie>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(Vec::new()))
}

/// Pure Inhibit: issue a cookie for the requested flags (does not store it).
pub fn handle_inhibit(req: &PortalInhibitRequest) -> Result<PortalInhibitCookie, String> {
    if req.app_id.trim().is_empty() {
        return Err("empty app_id".into());
    }
    if req.flags == 0 {
        return Err("no inhibit flags".into());
    }
    let flags = InhibitFlag::from_bits(req.flags);
    if flags.is_empty() {
        return Err("unrecognized inhibit flags".into());
    }
    let cookie = NEXT_INHIBIT.fetch_add(1, Ordering::Relaxed) as u32;
    Ok(PortalInhibitCookie {
        cookie,
        flags: req.flags,
        app_id: req.app_id.trim().to_string(),
        reason: req.reason.clone(),
    })
}

/// Issue a cookie **and** register it in the process-wide store.
///
/// Portal D-Bus `Inhibit` and shell tests should use this so idle policy can
/// poll [`active_inhibits`].
pub fn handle_inhibit_and_register(
    req: &PortalInhibitRequest,
) -> Result<PortalInhibitCookie, String> {
    let cookie = handle_inhibit(req)?;
    register_inhibit_cookie(cookie.clone());
    Ok(cookie)
}

/// Register (or replace) an inhibit cookie in the process-wide store.
pub fn register_inhibit_cookie(cookie: PortalInhibitCookie) {
    if let Ok(mut guard) = inhibit_cookie_store().lock() {
        guard.retain(|c| c.cookie != cookie.cookie);
        guard.push(cookie);
    }
}

/// Release a previously registered cookie. Returns `true` if it was present.
pub fn release_inhibit_cookie(cookie_id: u32) -> bool {
    match inhibit_cookie_store().lock() {
        Ok(mut guard) => {
            let before = guard.len();
            guard.retain(|c| c.cookie != cookie_id);
            guard.len() < before
        }
        Err(_) => false,
    }
}

/// Snapshot of active portal inhibit cookies.
pub fn active_inhibits() -> Vec<PortalInhibitCookie> {
    inhibit_cookie_store()
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default()
}

/// True when any active cookie blocks idle lock/suspend.
pub fn portal_blocks_idle() -> bool {
    active_inhibits().iter().any(inhibit_blocks_idle)
}

/// Merge active portal cookies into an [`IdleInhibitState`] (Media for idle/suspend flags).
pub fn active_idle_inhibit_state() -> crate::idle_policy::IdleInhibitState {
    let mut state = crate::idle_policy::IdleInhibitState::new();
    for cookie in active_inhibits() {
        if let Some(reason) = inhibit_to_idle_reason(&cookie) {
            state.add(reason);
        }
    }
    state
}

/// Clear the process-wide store (tests only).
pub fn clear_inhibit_store_for_tests() {
    if let Ok(mut guard) = inhibit_cookie_store().lock() {
        guard.clear();
    }
}

/// Whether an inhibit cookie blocks idle lock (flag Idle or Suspend).
pub fn inhibit_blocks_idle(cookie: &PortalInhibitCookie) -> bool {
    cookie.flags & (InhibitFlag::Idle as u32 | InhibitFlag::Suspend as u32) != 0
}

/// Map portal inhibit → idle_policy reasons.
pub fn inhibit_to_idle_reason(
    cookie: &PortalInhibitCookie,
) -> Option<crate::idle_policy::InhibitReason> {
    if inhibit_blocks_idle(cookie) {
        Some(crate::idle_policy::InhibitReason::Media)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_lookup_and_reject() {
        let ok = handle_secret_retrieve(&PortalSecretRequest {
            app_id: "org.example.App".into(),
            token: vec![1, 2],
        });
        match ok {
            PortalSecretResult::Lookup { label } => {
                assert!(label.contains("org.example.App"));
            }
            _ => panic!("expected lookup"),
        }
        assert!(matches!(
            handle_secret_retrieve(&PortalSecretRequest {
                app_id: String::new(),
                token: vec![],
            }),
            PortalSecretResult::Rejected { .. }
        ));
    }

    #[test]
    fn print_lp_plan() {
        let r = handle_print_request(&PortalPrintRequest {
            title: "Report".into(),
            document_uri: "file:///tmp/doc.pdf".into(),
            modal: true,
        });
        match r {
            PortalPrintResult::Queued { argv, .. } => {
                assert_eq!(argv[0], "lp");
                assert!(argv.iter().any(|a| a == "/tmp/doc.pdf"));
            }
            _ => panic!("expected queued"),
        }
        assert!(matches!(
            handle_print_request(&PortalPrintRequest {
                title: "x".into(),
                document_uri: "http://evil".into(),
                modal: false,
            }),
            PortalPrintResult::Rejected { .. }
        ));
    }

    #[test]
    fn inhibit_cookie_and_idle() {
        let c = handle_inhibit(&PortalInhibitRequest {
            app_id: "player".into(),
            window: String::new(),
            flags: InhibitFlag::Idle as u32 | InhibitFlag::Suspend as u32,
            reason: "playing".into(),
        })
        .unwrap();
        assert!(inhibit_blocks_idle(&c));
        assert!(inhibit_to_idle_reason(&c).is_some());
        assert!(handle_inhibit(&PortalInhibitRequest {
            app_id: "x".into(),
            window: String::new(),
            flags: 0,
            reason: String::new(),
        })
        .is_err());
    }

    #[test]
    fn inhibit_store_register_release_and_idle_merge() {
        clear_inhibit_store_for_tests();
        assert!(!portal_blocks_idle());
        assert!(active_inhibits().is_empty());

        let c = handle_inhibit_and_register(&PortalInhibitRequest {
            app_id: "video".into(),
            window: String::new(),
            flags: InhibitFlag::Idle as u32,
            reason: "watching".into(),
        })
        .unwrap();
        assert_eq!(active_inhibits().len(), 1);
        assert!(portal_blocks_idle());
        let state = active_idle_inhibit_state();
        assert!(state.is_inhibited());
        assert!(state
            .reasons()
            .contains(&crate::idle_policy::InhibitReason::Media));

        // Logout-only flags do not block idle.
        let logout = handle_inhibit_and_register(&PortalInhibitRequest {
            app_id: "installer".into(),
            window: String::new(),
            flags: InhibitFlag::Logout as u32,
            reason: "installing".into(),
        })
        .unwrap();
        assert!(!inhibit_blocks_idle(&logout));
        // Still blocked by the Idle cookie.
        assert!(portal_blocks_idle());

        assert!(release_inhibit_cookie(c.cookie));
        assert!(!release_inhibit_cookie(c.cookie));
        // Logout cookie remains but does not block idle.
        assert!(!portal_blocks_idle());
        assert!(release_inhibit_cookie(logout.cookie));
        assert!(active_inhibits().is_empty());
        clear_inhibit_store_for_tests();
    }
}
