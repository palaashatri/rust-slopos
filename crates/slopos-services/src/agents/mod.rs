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
//! a host starts an agent. A host can register both agents with
//! [`serve_session_agents`], which returns a [`SessionAgents`] handle that
//! waits for a failure or shuts both down, or register one agent alone with
//! [`network_secret::serve`] or [`bluez::serve`]. No session bundled with
//! SLOPOS starts any of them yet, so the system's own pairing and password
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
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::Duration,
};

use futures::{stream::FuturesUnordered, StreamExt};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
    time::timeout,
};
use zbus::{fdo::DBusProxy, names::BusName, Connection};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
/// How often a registration is tried for one owner, and the wait between
/// tries. Each owner gets its own budget. A retry is a single timer, not a
/// polling loop.
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
    /// A WPA-PSK passphrase: 8 to 63 bytes, or exactly 64 hex digits.
    WpaPassphrase,
    /// A WPA3-SAE password: 1 to 255 bytes. SAE passwords may be shorter than
    /// a WPA-PSK passphrase, so they do not use the WPA rule.
    SaePassphrase,
}

impl FieldKind {
    fn accepts(self, text: &str) -> bool {
        match self {
            Self::Secret => true,
            Self::Digits => text.bytes().all(|byte| byte.is_ascii_digit()),
            Self::WpaPassphrase => crate::network::valid_wpa_passphrase(text),
            Self::SaePassphrase => crate::network::valid_sae_password(text),
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

/// Wait for room on the presenter's channel. The wait holds no agent lock.
/// Returns `None` when no host is listening.
pub(crate) async fn reserve(presenter: &Presenter) -> Option<mpsc::Permit<'_, PromptCommand>> {
    presenter.reserve().await.ok()
}

/// Publish `Show` through a reserved slot, without waiting. Call it while
/// holding the agent's lock, after checking that the request still owns its
/// dialog, so a `Close` queued by a cancellation can only follow it. Returns
/// the receiver for the answer.
pub(crate) fn publish_show(
    permit: mpsc::Permit<'_, PromptCommand>,
    id: u64,
    spec: DialogSpec,
) -> oneshot::Receiver<DialogAnswer> {
    let (reply, answer) = oneshot::channel();
    permit.send(PromptCommand::Show { id, spec, reply });
    answer
}

/// Publish a display-only `Show`. The reply receiver is dropped before the
/// command is sent, so a host that reads the command at once sees a closed
/// reply channel, as [`PromptCommand::Show`] promises.
pub(crate) fn publish_show_detached(
    permit: mpsc::Permit<'_, PromptCommand>,
    id: u64,
    spec: DialogSpec,
) {
    let (reply, answer) = oneshot::channel();
    drop(answer);
    permit.send(PromptCommand::Show { id, spec, reply });
}

/// Turn the answer receiver of a published dialog into the outcome.
pub(crate) async fn await_answer(answer: oneshot::Receiver<DialogAnswer>) -> PromptOutcome {
    match answer.await {
        Ok(answer) => PromptOutcome::Answered(answer),
        Err(_) => PromptOutcome::Cancelled,
    }
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

/// The running session agents: one watcher task per agent, each with a stop
/// signal. Dropping the handle also stops both watchers (their stop channels
/// close), but it does not wait for their teardown; call [`shutdown`] to wait.
///
/// [`shutdown`]: SessionAgents::shutdown
pub struct SessionAgents {
    stops: Vec<oneshot::Sender<()>>,
    tasks: FuturesUnordered<JoinHandle<Result<(), AgentError>>>,
}

impl SessionAgents {
    /// Wait until one agent ends. Returns its error at once, so a
    /// NetworkManager failure is not held back by a healthy BlueZ agent. The
    /// other agent keeps running until [`shutdown`](Self::shutdown).
    pub async fn wait(&mut self) -> Result<(), AgentError> {
        while let Some(joined) = self.tasks.next().await {
            joined_result(joined)?;
        }
        Ok(())
    }

    /// Stop both watchers, let each run its teardown (its open dialog closed,
    /// its registration undone, its interface removed), and wait for both.
    /// Returns the first error, if any.
    pub async fn shutdown(mut self) -> Result<(), AgentError> {
        for stop in self.stops.drain(..) {
            let _ = stop.send(());
        }
        let mut first = None;
        while let Some(joined) = self.tasks.next().await {
            if let Err(error) = joined_result(joined) {
                first.get_or_insert(error);
            }
        }
        match first {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

fn joined_result(
    joined: Result<Result<(), AgentError>, tokio::task::JoinError>,
) -> Result<(), AgentError> {
    match joined {
        Ok(result) => result,
        Err(join) => Err(AgentError::Task(join.to_string())),
    }
}

/// Connect to the system bus and start both agents. See
/// [`serve_session_agents_on`].
pub async fn serve_session_agents(presenter: Presenter) -> Result<SessionAgents, AgentError> {
    let system = timeout(CONNECT_TIMEOUT, Connection::system())
        .await
        .map_err(|_| AgentError::Timeout)??;
    Ok(serve_session_agents_on(&system, presenter))
}

/// Start both agents on the connected `system` bus, each in its own task.
pub fn serve_session_agents_on(system: &Connection, presenter: Presenter) -> SessionAgents {
    let (network_stop, network_stopped) = oneshot::channel();
    let (bluez_stop, bluez_stopped) = oneshot::channel();
    let network = tokio::spawn({
        let (system, presenter) = (system.clone(), presenter.clone());
        async move { network_secret::serve(&system, presenter, network_stopped).await }
    });
    let bluez = tokio::spawn({
        let system = system.clone();
        async move { bluez::serve(&system, presenter, bluez_stopped).await }
    });
    SessionAgents {
        stops: vec![network_stop, bluez_stop],
        tasks: [network, bluez].into_iter().collect(),
    }
}

/// The unique name of the current owner of `service`, or `None` when the
/// service has no owner. Asking does not start the service.
async fn current_owner(dbus: &DBusProxy<'_>, service: &str) -> zbus::Result<Option<String>> {
    match dbus.get_name_owner(BusName::try_from(service)?).await {
        Ok(owner) => Ok(Some(owner.to_string())),
        Err(zbus::fdo::Error::NameHasNoOwner(_)) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Register with one owner, with that owner's own retry budget. Every call
/// targets the unique name of that owner, so a retry cannot start a stopped
/// service or reach a newer owner. Returns the owner when the registration
/// completed, and `None` when it did not.
async fn register_for_owner<R, Fut>(
    dbus: &DBusProxy<'_>,
    service: &str,
    owner: String,
    register: &mut R,
) -> Option<String>
where
    R: FnMut(String) -> Fut,
    Fut: Future<Output = zbus::Result<()>>,
{
    let expected = owner.clone();
    let mut still_owned = move || {
        let expected = expected.clone();
        async move {
            let current = current_owner(dbus, service).await?;
            Ok::<bool, zbus::Error>(current.as_deref() == Some(expected.as_str()))
        }
    };
    let target = owner.clone();
    let mut attempt = move || register(target.clone());
    match register_with_retries(
        REGISTER_ATTEMPTS,
        REGISTER_RETRY_DELAY,
        &mut still_owned,
        &mut attempt,
    )
    .await
    {
        Ok(()) => Some(owner),
        Err(error) => {
            // The owner stays unregistered until its next owner change, but
            // the failure is reported rather than dropped.
            eprintln!("session agent: registering with {service} owner {owner} failed: {error}");
            None
        }
    }
}

/// Run `register` for each owner of `service`: once for the owner present
/// when the watch starts, and again for every owner that arrives later. An
/// owner that is already registered is not registered again, so an
/// acquisition that races the first check is not registered twice. `register`
/// and `on_owner_lost` both receive the owner's unique name, and
/// `on_owner_lost` runs when that owner goes away or is replaced.
///
/// The subscription is made before the first check, so no acquisition in
/// between is missed. A registration that fails for one owner does not stop
/// the watch: a later owner is still registered. The owner registered when
/// the watch ends is written into `registered`, so teardown can undo it.
pub(crate) async fn register_while_owned<R, Fut, L, LFut>(
    system: &Connection,
    service: &str,
    mut register: R,
    mut on_owner_lost: L,
    registered: &Mutex<Option<String>>,
    stop: &mut oneshot::Receiver<()>,
) -> Result<(), AgentError>
where
    R: FnMut(String) -> Fut,
    Fut: Future<Output = zbus::Result<()>>,
    L: FnMut(String) -> LFut,
    LFut: Future<Output = ()>,
{
    let dbus = DBusProxy::new(system).await?;
    let mut owners = dbus
        .receive_name_owner_changed_with_args(&[(0, service)])
        .await?;
    // The unique name this loop last tried to register with. An owner change
    // queued for that same name is not a new owner, so it is skipped.
    let mut tried: Option<String> = None;
    if let Some(owner) = current_owner(&dbus, service).await? {
        tried = Some(owner.clone());
        let done = register_for_owner(&dbus, service, owner, &mut register).await;
        set(registered, done);
    }
    loop {
        tokio::select! {
            _ = &mut *stop => break,
            change = owners.next() => {
                let Some(change) = change else { break };
                let Ok(args) = change.args() else { continue };
                let lost = args.old_owner().as_ref().map(ToString::to_string);
                let arrived = args.new_owner().as_ref().map(ToString::to_string);
                if let Some(lost) = lost {
                    // Requests from a departed or replaced owner are cancelled
                    // whether or not its registration succeeded.
                    if tried.as_deref() == Some(lost.as_str()) {
                        tried = None;
                    }
                    if get(registered).as_deref() == Some(lost.as_str()) {
                        set(registered, None);
                    }
                    on_owner_lost(lost).await;
                }
                if let Some(owner) = arrived {
                    if tried.as_deref() != Some(owner.as_str()) {
                        tried = Some(owner.clone());
                        let done = register_for_owner(&dbus, service, owner, &mut register).await;
                        set(registered, done);
                    }
                }
            }
        }
    }
    Ok(())
}

fn get(slot: &Mutex<Option<String>>) -> Option<String> {
    slot.lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

fn set(slot: &Mutex<Option<String>>, value: Option<String>) {
    *slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = value;
}

/// Call `register` up to `attempts` times. Between tries it waits `delay`,
/// then checks that the owner is still there, and stops quietly when it is
/// not: the next owner change registers again. Returns the last error when
/// every try failed.
pub(crate) async fn register_with_retries<H, HFut, F, Fut>(
    attempts: u32,
    delay: Duration,
    owner_still_present: &mut H,
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
    async fn publish_reports_unavailable_without_a_host_and_cancelled_when_closed() {
        let (presenter, receiver) = mpsc::channel(4);
        drop(receiver);
        assert!(reserve(&presenter).await.is_none(), "no host is listening");

        let (presenter, mut receiver) = mpsc::channel(4);
        let host = tokio::spawn(async move {
            // Drop the reply without answering, as a host does on Close.
            if let Some(PromptCommand::Show { reply, .. }) = receiver.recv().await {
                drop(reply);
            }
        });
        let spec = DialogSpec {
            title: "t".into(),
            message: "m".into(),
            buttons: vec!["Cancel".into()],
            field: None,
        };
        let permit = reserve(&presenter).await.expect("a host is listening");
        let answer = publish_show(permit, 2, spec);
        assert!(matches!(
            await_answer(answer).await,
            PromptOutcome::Cancelled
        ));
        host.await.unwrap();
    }

    #[tokio::test]
    async fn display_publish_hands_the_host_a_closed_reply_channel() {
        let (presenter, mut receiver) = mpsc::channel(4);
        let spec = DialogSpec {
            title: "t".into(),
            message: "m".into(),
            buttons: vec!["Close".into()],
            field: None,
        };
        let permit = reserve(&presenter).await.expect("a host is listening");
        publish_show_detached(permit, 3, spec);
        let Some(PromptCommand::Show { reply, .. }) = receiver.recv().await else {
            panic!("expected a Show command");
        };
        assert!(
            reply.is_closed(),
            "a display-only prompt has no one to answer"
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
        let mut owned = || async { Ok(true) };
        register_with_retries(3, Duration::ZERO, &mut owned, &mut register)
            .await
            .unwrap();
        assert_eq!(calls, 2, "the second try succeeds, so there is no third");

        let mut calls = 0;
        let mut always_fails = || {
            calls += 1;
            async { Err(zbus::Error::Failure("busy".into())) }
        };
        let error = register_with_retries(3, Duration::ZERO, &mut owned, &mut always_fails)
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
        let mut gone = || async { Ok(false) };
        register_with_retries(3, Duration::ZERO, &mut gone, &mut register)
            .await
            .unwrap();
        assert_eq!(calls, 1, "no retry once the owner has gone");
    }
}
