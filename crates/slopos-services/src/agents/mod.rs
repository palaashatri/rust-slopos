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
//! a host starts an agent. [`serve_session_agents`] starts both, and each
//! agent's `serve` function can be started alone. No session bundled with
//! SLOPOS calls any of them yet, so the system's own pairing and password
//! behaviour is unchanged.
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

use futures::{stream::FuturesUnordered, StreamExt};
use tokio::{
    sync::{mpsc, oneshot},
    time::timeout,
};
use zbus::{fdo::DBusProxy, names::BusName, Connection};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
/// How often a registration is tried while its service still has an owner,
/// and the wait between tries. A retry is a single timer, not a polling loop.
const REGISTER_ATTEMPTS: u32 = 3;
const REGISTER_RETRY_DELAY: Duration = Duration::from_millis(500);

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

/// The text field of a dialog. Its lengths are in UTF-8 bytes. The host keeps
/// its accept button disabled until [`FieldSpec::accepts`] is true for the
/// text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldSpec {
    pub label: String,
    pub kind: FieldKind,
    pub min_len: usize,
    pub max_len: usize,
}

impl FieldSpec {
    /// Whether `text` may be accepted: its length in bytes is within
    /// `min_len..=max_len`, and it passes the rule of the field's kind. The
    /// agent checks answers with the same method.
    pub fn accepts(&self, text: &str) -> bool {
        (self.min_len..=self.max_len).contains(&text.len()) && self.kind.accepts(text)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// Masked text with no rule beyond the length: VPN passwords and PINs.
    Secret,
    /// ASCII digits only: passkeys.
    Digits,
    /// A Wi-Fi passphrase: 8 to 63 bytes, or exactly 64 hex digits.
    WpaPassphrase,
}

impl FieldKind {
    fn accepts(self, text: &str) -> bool {
        match self {
            Self::Secret => true,
            Self::Digits => text.bytes().all(|byte| byte.is_ascii_digit()),
            Self::WpaPassphrase => crate::network::valid_wpa_passphrase(text),
        }
    }
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

/// The unique bus name that sent a method call. Owner changes are matched
/// against it.
pub(crate) fn sender_of(header: &zbus::message::Header<'_>) -> String {
    header.sender().map(ToString::to_string).unwrap_or_default()
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
/// Returns false when no host is listening. The reply receiver is dropped
/// before the command is sent, so a host that reads the command at once sees
/// a closed reply channel, as [`PromptCommand::Show`] promises.
pub(crate) async fn show_detached(presenter: &Presenter, id: u64, spec: DialogSpec) -> bool {
    let (reply, answer) = oneshot::channel();
    drop(answer);
    presenter
        .send(PromptCommand::Show { id, spec, reply })
        .await
        .is_ok()
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
    #[error("agent task failed: {0}")]
    Task(String),
}

/// Connect to the system bus and serve both agents. See
/// [`serve_session_agents_on`].
pub async fn serve_session_agents(presenter: Presenter) -> Result<(), AgentError> {
    let system = timeout(CONNECT_TIMEOUT, Connection::system())
        .await
        .map_err(|_| AgentError::Timeout)??;
    serve_session_agents_on(&system, presenter).await
}

/// Serve both agents on the connected `system` bus, each in its own task.
///
/// Returns the first error from either agent as soon as it happens, so a
/// NetworkManager failure is not hidden behind a healthy BlueZ agent. The
/// other agent keeps running until the process exits. Each agent registers
/// with its service when the service has an owner, and registers again
/// whenever that service restarts (its owner changes), so the agents need no
/// polling.
pub async fn serve_session_agents_on(
    system: &Connection,
    presenter: Presenter,
) -> Result<(), AgentError> {
    let network = tokio::spawn({
        let (system, presenter) = (system.clone(), presenter.clone());
        async move { network_secret::serve(&system, presenter).await }
    });
    let bluez = tokio::spawn({
        let system = system.clone();
        async move { bluez::serve(&system, presenter).await }
    });
    let mut tasks: FuturesUnordered<_> = [network, bluez].into_iter().collect();
    while let Some(joined) = tasks.next().await {
        match joined {
            Ok(Ok(())) => {}
            Ok(Err(error)) => return Err(error),
            Err(join) => return Err(AgentError::Task(join.to_string())),
        }
    }
    Ok(())
}

/// Whether `service` currently has an owner. Checking first means a stopped
/// service is not started by a registration call.
async fn owner_present(dbus: &DBusProxy<'_>, service: &str) -> zbus::Result<bool> {
    dbus.name_has_owner(BusName::try_from(service)?).await
}

/// Run `register` while `service` has an owner, and again each time it gets
/// one. `on_owner_lost` runs with the unique name of an owner that went away
/// or was replaced, so an agent can cancel that owner's pending requests.
///
/// Subscribes before the first check, so an owner that appears meanwhile is
/// not missed. A registration that still fails after its retries ends the
/// call with that error.
pub(crate) async fn register_while_owned<F, Fut, L, LFut>(
    system: &Connection,
    service: &str,
    mut register: F,
    mut on_owner_lost: L,
) -> Result<(), AgentError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = zbus::Result<()>>,
    L: FnMut(String) -> LFut,
    LFut: Future<Output = ()>,
{
    let dbus = DBusProxy::new(system).await?;
    let mut owners = dbus
        .receive_name_owner_changed_with_args(&[(0, service)])
        .await?;
    if owner_present(&dbus, service).await? {
        register_with_retries(
            REGISTER_ATTEMPTS,
            REGISTER_RETRY_DELAY,
            || owner_present(&dbus, service),
            &mut register,
        )
        .await?;
    }
    while let Some(change) = owners.next().await {
        let Ok(args) = change.args() else {
            continue;
        };
        if let Some(old) = args.old_owner().as_ref() {
            on_owner_lost(old.to_string()).await;
        }
        if args.new_owner().is_some() {
            register_with_retries(
                REGISTER_ATTEMPTS,
                REGISTER_RETRY_DELAY,
                || owner_present(&dbus, service),
                &mut register,
            )
            .await?;
        }
    }
    Ok(())
}

/// Call `register` up to `attempts` times. Between tries it waits `delay`,
/// then checks that the service still has an owner, and stops quietly when
/// it does not: the next owner change registers again. Returns the last
/// error when every try failed.
pub(crate) async fn register_with_retries<H, HFut, F, Fut>(
    attempts: u32,
    delay: Duration,
    mut owner_still_present: H,
    register: &mut F,
) -> Result<(), AgentError>
where
    H: FnMut() -> HFut,
    HFut: Future<Output = zbus::Result<bool>>,
    F: FnMut() -> Fut,
    Fut: Future<Output = zbus::Result<()>>,
{
    let mut last_error = None;
    for attempt in 0..attempts {
        if attempt > 0 {
            tokio::time::sleep(delay).await;
            if !owner_still_present().await? {
                return Ok(());
            }
        }
        match register().await {
            Ok(()) => return Ok(()),
            Err(error) => last_error = Some(error),
        }
    }
    match last_error {
        Some(error) => Err(error.into()),
        None => Ok(()),
    }
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

    #[test]
    fn field_accepts_checks_bytes_and_the_kind_rule() {
        let digits = FieldSpec {
            label: "Passkey".into(),
            kind: FieldKind::Digits,
            min_len: 6,
            max_len: 6,
        };
        assert!(digits.accepts("123456"));
        assert!(!digits.accepts("12345"));
        assert!(!digits.accepts("12345a"));

        let pin = FieldSpec {
            label: "PIN".into(),
            kind: FieldKind::Secret,
            min_len: 1,
            max_len: 16,
        };
        assert!(pin.accepts(&"é".repeat(8)), "16 bytes");
        assert!(!pin.accepts(&"é".repeat(9)), "18 bytes is over the limit");
    }

    #[test]
    fn wifi_field_uses_the_passphrase_rule_for_connect() {
        let wifi = FieldSpec {
            label: "Password".into(),
            kind: FieldKind::WpaPassphrase,
            min_len: 8,
            max_len: 64,
        };
        assert!(wifi.accepts("correct horse"));
        assert!(wifi.accepts(&"ab".repeat(32)), "64 hex digits");
        assert!(!wifi.accepts(&"x".repeat(64)), "64 non-hex characters");
        assert!(!wifi.accepts("1234567"));
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

    #[tokio::test]
    async fn show_detached_hands_the_host_a_closed_reply_channel() {
        let (presenter, mut receiver) = mpsc::channel(4);
        let spec = DialogSpec {
            title: "t".into(),
            message: "m".into(),
            buttons: vec!["Close".into()],
            field: None,
        };
        assert!(show_detached(&presenter, 3, spec.clone()).await);
        let Some(PromptCommand::Show { reply, .. }) = receiver.recv().await else {
            panic!("expected a Show command");
        };
        assert!(
            reply.is_closed(),
            "a display-only prompt has no one to answer"
        );

        drop(receiver);
        assert!(
            !show_detached(&presenter, 4, spec).await,
            "no host is listening"
        );
    }

    #[tokio::test]
    async fn retries_stop_at_the_first_success_and_report_the_last_failure() {
        let mut calls = 0;
        let mut register = || {
            calls += 1;
            let succeed = calls == 2;
            async move {
                if succeed {
                    Ok(())
                } else {
                    Err(zbus::Error::Failure("busy".into()))
                }
            }
        };
        let owned = || async { Ok(true) };
        register_with_retries(3, Duration::ZERO, owned, &mut register)
            .await
            .unwrap();
        assert_eq!(calls, 2, "the second try succeeds, so there is no third");

        let mut calls = 0;
        let mut always_fails = || {
            calls += 1;
            async { Err(zbus::Error::Failure("busy".into())) }
        };
        let error = register_with_retries(3, Duration::ZERO, owned, &mut always_fails)
            .await
            .unwrap_err();
        assert!(matches!(error, AgentError::Bus(_)));
        assert_eq!(calls, 3);
    }

    #[tokio::test]
    async fn retries_stop_quietly_when_the_owner_has_gone() {
        let mut calls = 0;
        let mut register = || {
            calls += 1;
            async { Err(zbus::Error::Failure("busy".into())) }
        };
        let gone = || async { Ok(false) };
        register_with_retries(3, Duration::ZERO, gone, &mut register)
            .await
            .unwrap();
        assert_eq!(calls, 1, "no retry once the service has no owner");
    }
}
