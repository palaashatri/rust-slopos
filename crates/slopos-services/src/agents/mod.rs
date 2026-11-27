//! Session secret and pairing agents.
//!
//! * [`network_secret`]: the NetworkManager secret agent, which asks for Wi-Fi
//!   and VPN passwords when NetworkManager needs them (contract §18.10,
//!   GAP-SEC-03).
//! * [`bluez`]: the BlueZ pairing agent, which asks for PIN codes, passkeys
//!   and confirmations, and shows passkeys to type on the device (contract
//!   §18.11, GAP-SEC-04).
//!
//! The agents hold no X11 or toolkit types. Each request becomes a
//! [`DialogSpec`] sent to a host as a [`PromptCommand`] on a [`Presenter`];
//! the host draws the dialog with the SLOPOS toolkit and returns a
//! [`DialogAnswer`]. Nothing is registered with NetworkManager or BlueZ until
//! a host calls [`serve_session_agents`], so an absent host leaves the
//! system's own pairing and password behaviour unchanged.
//!
//! Secrets are never logged. Typed answers are wrapped in [`Secret`], whose
//! `Debug` output is redacted.

pub mod bluez;
pub mod network_secret;

#[cfg(test)]
mod tests;

use std::{
    fmt,
    future::Future,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use futures::StreamExt;
use tokio::{
    sync::{mpsc, oneshot},
    time::timeout,
};
use zbus::{fdo::DBusProxy, Connection};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// Identifier NetworkManager and BlueZ show in their logs for this agent.
pub const AGENT_IDENTIFIER: &str = "org.slopos.SessionAgent";

/// Sends dialog requests to the host that draws them.
pub type Presenter = mpsc::Sender<PromptCommand>;

/// A message from an agent to the dialog host.
#[derive(Debug)]
pub enum PromptCommand {
    /// Show `spec` as dialog `id`. The host answers on `reply`. If the host
    /// closes the dialog without an answer (see [`PromptCommand::Close`]) it
    /// drops `reply`. A dialog whose answer nobody waits for is sent with a
    /// reply receiver that is already dropped.
    Show {
        id: u64,
        spec: DialogSpec,
        reply: oneshot::Sender<DialogAnswer>,
    },
    /// Close dialog `id` if it is still open. Its reply is dropped.
    Close { id: u64 },
}

/// A dialog in toolkit terms: a title, a message, a row of buttons and at
/// most one text field. The first button is Cancel (Escape) and the last is
/// the default (Enter), as in `slopos_ui::controls::dialog::Dialog`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogSpec {
    pub title: String,
    pub message: String,
    pub buttons: Vec<String>,
    pub field: Option<FieldSpec>,
}

impl DialogSpec {
    /// Index of the Cancel button (the first one).
    pub const CANCEL: usize = 0;

    /// Index of the default button (the last one).
    pub fn default_button(&self) -> usize {
        self.buttons.len().saturating_sub(1)
    }
}

/// The text field of a dialog. The host keeps its accept button disabled
/// until the text is valid for `kind` and between `min_len` and `max_len`
/// characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldSpec {
    pub label: String,
    pub kind: FieldKind,
    pub min_len: usize,
    pub max_len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// Masked text: passwords and PINs.
    Secret,
    /// ASCII digits only: passkeys.
    Digits,
}

/// What the user did with a dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogAnswer {
    /// Button `index` was chosen; `text` is the field's contents, if any.
    Chosen { index: usize, text: Option<Secret> },
    /// The window was closed without a button.
    Dismissed,
}

/// Text the user typed that must not reach a log: passwords, PINs, passkeys.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

/// The host's answer to one `Show`, as seen by an agent.
#[derive(Debug)]
pub enum PromptOutcome {
    Answered(DialogAnswer),
    /// The dialog was closed by an agent (`Close`) before an answer.
    Cancelled,
    /// No host is listening.
    Unavailable,
}

static NEXT_PROMPT_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) fn next_prompt_id() -> u64 {
    NEXT_PROMPT_ID.fetch_add(1, Ordering::Relaxed)
}

/// Show a dialog and wait for the answer.
pub(crate) async fn show(presenter: &Presenter, id: u64, spec: DialogSpec) -> PromptOutcome {
    let (reply, answer) = oneshot::channel();
    if presenter
        .send(PromptCommand::Show { id, spec, reply })
        .await
        .is_err()
    {
        return PromptOutcome::Unavailable;
    }
    match answer.await {
        Ok(answer) => PromptOutcome::Answered(answer),
        Err(_) => PromptOutcome::Cancelled,
    }
}

/// Show a dialog whose answer is not awaited (display-only pairing prompts).
pub(crate) async fn show_detached(presenter: &Presenter, id: u64, spec: DialogSpec) {
    let (reply, _answer) = oneshot::channel();
    let _ = presenter
        .send(PromptCommand::Show { id, spec, reply })
        .await;
}

/// Close a dialog that an agent opened. A missing host or an already closed
/// dialog is not an error.
pub(crate) async fn close(presenter: &Presenter, id: u64) {
    let _ = presenter.send(PromptCommand::Close { id }).await;
}

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("D-Bus operation timed out")]
    Timeout,
    #[error("D-Bus error: {0}")]
    Bus(#[from] zbus::Error),
}

/// Connect to the system bus and serve both agents until the process exits.
///
/// Each agent registers with its service when the service has an owner, and
/// registers again whenever that service restarts (its owner changes), so
/// the agents need no polling. Errors from a single registration attempt are
/// not returned: the next owner change retries it.
pub async fn serve_session_agents(presenter: Presenter) -> Result<(), AgentError> {
    let system = timeout(CONNECT_TIMEOUT, Connection::system())
        .await
        .map_err(|_| AgentError::Timeout)??;
    let (network, bluez) = futures::join!(
        network_secret::serve(&system, presenter.clone()),
        bluez::serve(&system, presenter),
    );
    network.and(bluez)
}

/// Run `register` once now and again each time `service` gets an owner.
/// Subscribes before the first attempt, so an owner that appears meanwhile
/// is not missed.
pub(crate) async fn register_while_owned<F, Fut>(
    system: &Connection,
    service: &str,
    mut register: F,
) -> Result<(), AgentError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = zbus::Result<()>>,
{
    let dbus = DBusProxy::new(system).await?;
    let mut owners = dbus
        .receive_name_owner_changed_with_args(&[(0, service)])
        .await?;
    let _ = register().await;
    while let Some(change) = owners.next().await {
        let appeared = change
            .args()
            .map(|args| args.new_owner().is_some())
            .unwrap_or(false);
        if appeared {
            let _ = register().await;
        }
    }
    Ok(())
}

#[cfg(test)]
mod mod_tests {
    use super::*;

    #[test]
    fn secret_debug_output_is_redacted() {
        let secret = Secret::new("hunter2 correct horse");
        let rendered = format!("{secret:?}");
        assert!(!rendered.contains("hunter2"));
        assert_eq!(secret.expose(), "hunter2 correct horse");
        let answer = DialogAnswer::Chosen {
            index: 1,
            text: Some(Secret::new("123456")),
        };
        assert!(!format!("{answer:?}").contains("123456"));
    }

    #[test]
    fn cancel_is_first_and_default_is_last() {
        let spec = DialogSpec {
            title: "t".into(),
            message: "m".into(),
            buttons: vec!["Cancel".into(), "Connect".into()],
            field: None,
        };
        assert_eq!(spec.buttons[DialogSpec::CANCEL], "Cancel");
        assert_eq!(spec.default_button(), 1);
    }

    #[tokio::test]
    async fn show_reports_unavailable_without_a_host_and_cancelled_when_closed() {
        let (presenter, receiver) = mpsc::channel(4);
        drop(receiver);
        let spec = DialogSpec {
            title: "t".into(),
            message: "m".into(),
            buttons: vec!["Cancel".into()],
            field: None,
        };
        assert!(matches!(
            show(&presenter, 1, spec.clone()).await,
            PromptOutcome::Unavailable
        ));

        let (presenter, mut receiver) = mpsc::channel(4);
        let host = tokio::spawn(async move {
            // Drop the reply without answering, as a host does on Close.
            if let Some(PromptCommand::Show { reply, .. }) = receiver.recv().await {
                drop(reply);
            }
        });
        assert!(matches!(
            show(&presenter, 2, spec).await,
            PromptOutcome::Cancelled
        ));
        host.await.unwrap();
    }
}
