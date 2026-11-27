//! Polkit authentication agent (contract §18.1, GAP-SEC-02).
//!
//! - [`state`]: the request state machine and identity choice (pure).
//! - [`helper`]: the `polkit-agent-helper-1` protocol and the PAM conversation.
//! - [`agent`]: the D-Bus object and the Authority registration.
//!
//! The agent collects a password and hands it to the helper; the helper and
//! polkit verify it. Nothing in this module stores or logs a password.

pub mod agent;
pub mod helper;
pub mod state;
