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
        Arc, Mutex,
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
/// Longest wait for one D-Bus call the watch makes to a manager. A manager
/// that accepts a call and never replies must not hold up the watch or a
/// shutdown.
const CALL_TIMEOUT: Duration = Duration::from_secs(5);
/// Longest wait for room in the Show queue when teardown closes a dialog. A
/// host that has not drained its queue must not hold up a shutdown.
const CLOSE_TIMEOUT: Duration = Duration::from_secs(1);
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

/// Publish a display-only `Show` through a reserved slot, without waiting.
/// The reply receiver is dropped before the command is sent, so a host that
/// reads the command at once sees a closed reply channel, as
/// [`PromptCommand::Show`] promises.
///
/// Returns false when the host's receiver was gone, so the command was
/// discarded and nothing will be shown. The channel gives this receipt, not
/// the host: a host that leaves after the command is queued is not seen here.
pub(crate) fn publish_show_detached(
    presenter: &Presenter,
    permit: mpsc::Permit<'_, PromptCommand>,
    id: u64,
    spec: DialogSpec,
) -> bool {
    let (reply, answer) = oneshot::channel();
    drop(answer);
    permit.send(PromptCommand::Show { id, spec, reply });
    !presenter.is_closed()
}

/// Turn the answer receiver of a published dialog into the outcome.
pub(crate) async fn await_answer(answer: oneshot::Receiver<DialogAnswer>) -> PromptOutcome {
    match answer.await {
        Ok(answer) => PromptOutcome::Answered(answer),
        Err(_) => PromptOutcome::Cancelled,
    }
}

/// Close a dialog from a method call. A missing host or an already closed
/// dialog is not an error. The call waits for room in the Show queue, as
/// the method cannot otherwise promise the dialog is closed.
pub(crate) async fn close(presenter: &Presenter, id: u64) {
    let _ = presenter.send(PromptCommand::Close { id }).await;
}

/// Close a dialog during teardown or an owner change. The wait for room in
/// the Show queue is bounded by [`CLOSE_TIMEOUT`], so a host that waits for
/// the shutdown before it drains its queue cannot block the shutdown.
pub(crate) async fn close_bounded(presenter: &Presenter, id: u64) {
    let _ = timeout(CLOSE_TIMEOUT, presenter.send(PromptCommand::Close { id })).await;
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

/// A unique bus name the watch serves or has registered with, shared with
/// the agent and with teardown. Empty when there is none.
#[derive(Clone, Default)]
pub(crate) struct OwnerSlot(Arc<Mutex<Option<String>>>);

impl OwnerSlot {
    fn lock(&self) -> std::sync::MutexGuard<'_, Option<String>> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(crate) fn get(&self) -> Option<String> {
        self.lock().clone()
    }

    pub(crate) fn set(&self, owner: &str) {
        *self.lock() = Some(owner.to_owned());
    }

    /// Clear the slot when it holds `owner`. Another owner's entry is kept.
    pub(crate) fn clear_if(&self, owner: &str) {
        let mut slot = self.lock();
        if slot.as_deref() == Some(owner) {
            *slot = None;
        }
    }

    pub(crate) fn take(&self) -> Option<String> {
        self.lock().take()
    }
}

/// The unique bus names of the managers this watch registered with and has not
/// undone. An entry stays after its owner loses the well-known name: the owner
/// may still be connected and still hold the agent, so only an unregister call
/// or the owner going away ends the registration. Teardown undoes each entry.
#[derive(Clone, Default)]
pub(crate) struct Registrations(Arc<Mutex<Vec<String>>>);

impl Registrations {
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<String>> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn contains(&self, owner: &str) -> bool {
        self.lock().iter().any(|recorded| recorded == owner)
    }

    fn record(&self, owner: &str) {
        let mut recorded = self.lock();
        if !recorded.iter().any(|name| name == owner) {
            recorded.push(owner.to_owned());
        }
    }

    fn forget(&self, owner: &str) {
        self.lock().retain(|recorded| recorded != owner);
    }

    /// Take every recorded owner, for teardown.
    pub(crate) fn take_all(&self) -> Vec<String> {
        std::mem::take(&mut *self.lock())
    }
}

/// One registration call to `owner`. The owner is recorded when the call
/// starts, because the call may take effect even if the watch stops and the
/// call is dropped, or if its reply never arrives. A reply settles it: a
/// registration that stands is kept with [`keep`](Self::keep), and one that
/// was refused or is not this watch's is undone with
/// [`withdraw`](Self::withdraw), which removes only the record this call made.
pub(crate) struct RegistrationCall {
    registrations: Registrations,
    owner: String,
    recorded_here: bool,
}

impl RegistrationCall {
    pub(crate) fn begin(registrations: &Registrations, owner: &str) -> Self {
        let recorded_here = !registrations.contains(owner);
        registrations.record(owner);
        Self {
            registrations: registrations.clone(),
            owner: owner.to_owned(),
            recorded_here,
        }
    }

    /// The registration stands, or may: teardown must undo it.
    pub(crate) fn keep(self) {}

    /// Not registered by this call: forget the record it made. A record left
    /// by an earlier uncertain call is not this call's to forget.
    pub(crate) fn withdraw(self) {
        if self.recorded_here {
            self.registrations.forget(&self.owner);
        }
    }
}

/// Where one owner's registration ended.
#[derive(Debug)]
pub(crate) enum Attempt {
    /// The registration completed.
    Registered,
    /// Every try failed, and the owner went away before the next try.
    Abandoned,
    /// Every try failed while the owner was still on the bus.
    Failed(AgentError),
    /// The watch stopped first. A call that was still waiting for its reply
    /// was dropped, and its [`RegistrationCall`] has recorded it.
    Stopped,
}

/// Register with one owner, with that owner's own retry budget. Every call
/// targets the unique name of that owner, so a retry cannot start a stopped
/// service or reach a newer owner. Whether a registration stands, and so needs
/// undoing at teardown, is recorded by `register` itself through a
/// [`RegistrationCall`], because only the call knows what its reply said.
async fn register_for_owner<R, Fut>(
    dbus: &DBusProxy<'_>,
    service: &str,
    owner: String,
    register: &mut R,
    stop: &mut oneshot::Receiver<()>,
) -> Attempt
where
    R: FnMut(String) -> Fut,
    Fut: Future<Output = Result<(), AgentError>>,
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
    let outcome = register_with_retries(
        REGISTER_ATTEMPTS,
        REGISTER_RETRY_DELAY,
        &mut still_owned,
        &mut attempt,
        stop,
    )
    .await;
    if let Attempt::Failed(error) = &outcome {
        // The owner stays unregistered until its next owner change, but
        // the failure is reported rather than dropped.
        eprintln!("session agent: registering with {service} owner {owner} failed: {error}");
    }
    outcome
}

/// Whether an owner that just arrived should be registered with: it is not
/// the owner the watch already tried, and it is still the owner on the bus.
/// An acquisition can be released before the watch reads its event, and that
/// owner must not be registered with.
fn should_register_arrival(tried: Option<&str>, arrived: &str, current: Option<&str>) -> bool {
    tried != Some(arrived) && current == Some(arrived)
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
/// the watch: a later owner is still registered. `register` records each
/// registration that stands or may stand (see [`RegistrationCall`]). A lost
/// owner's record is kept, because that owner may still be connected and hold
/// the agent, so teardown still undoes it.
pub(crate) async fn register_while_owned<R, Fut, L, LFut>(
    system: &Connection,
    service: &str,
    mut register: R,
    mut on_owner_lost: L,
    stop: &mut oneshot::Receiver<()>,
) -> Result<(), AgentError>
where
    R: FnMut(String) -> Fut,
    Fut: Future<Output = Result<(), AgentError>>,
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
        let attempt = register_for_owner(&dbus, service, owner, &mut register, stop);
        if matches!(attempt.await, Attempt::Stopped) {
            return Ok(());
        }
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
                    on_owner_lost(lost).await;
                }
                if let Some(owner) = arrived {
                    // A failed owner query skips the owner rather than ending the watch.
                    let current = current_owner(&dbus, service).await.ok().flatten();
                    if should_register_arrival(tried.as_deref(), &owner, current.as_deref()) {
                        tried = Some(owner.clone());
                        let attempt = register_for_owner(
                            &dbus, service, owner, &mut register, stop,
                        );
                        if matches!(attempt.await, Attempt::Stopped) {
                            break;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// Call `register` up to `attempts` times. Between tries it waits `delay`,
/// then checks that the owner is still there, and gives up as
/// [`Attempt::Abandoned`] when it is not: the next owner change registers
/// again. Stopping `stop` ends the wait or the call at once. Returns
/// [`Attempt::Failed`] with the last error when every try failed.
pub(crate) async fn register_with_retries<H, HFut, F, Fut>(
    attempts: u32,
    delay: Duration,
    owner_still_present: &mut H,
    register: &mut F,
    stop: &mut oneshot::Receiver<()>,
) -> Attempt
where
    H: FnMut() -> HFut,
    HFut: Future<Output = zbus::Result<bool>>,
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<(), AgentError>>,
{
    let mut last_error = None;
    for attempt in 0..attempts {
        if attempt > 0 {
            tokio::select! {
                _ = &mut *stop => return Attempt::Stopped,
                _ = tokio::time::sleep(delay) => {}
            }
            match owner_still_present().await {
                Ok(true) => {}
                Ok(false) => return Attempt::Abandoned,
                Err(error) => return Attempt::Failed(error.into()),
            }
        }
        tokio::select! {
            _ = &mut *stop => return Attempt::Stopped,
            result = register() => match result {
                Ok(()) => return Attempt::Registered,
                Err(error) => last_error = Some(error),
            },
        }
    }
    match last_error {
        Some(error) => Attempt::Failed(error),
        // No try was made, so nothing could have been applied.
        None => Attempt::Abandoned,
    }
}

/// Wait at most `limit` for one D-Bus call. A call that does not return in
/// time is [`AgentError::Timeout`], and it may still have taken effect.
pub(crate) async fn bounded_for<T>(
    limit: Duration,
    call: impl Future<Output = zbus::Result<T>>,
) -> Result<T, AgentError> {
    match timeout(limit, call).await {
        Ok(result) => Ok(result?),
        Err(_) => Err(AgentError::Timeout),
    }
}

/// Wait at most [`CALL_TIMEOUT`] for one D-Bus registration or unregister call.
pub(crate) async fn bounded<T>(
    call: impl Future<Output = zbus::Result<T>>,
) -> Result<T, AgentError> {
    bounded_for(CALL_TIMEOUT, call).await
}

/// Whether the bus still has the connection `unique`, a unique bus name. The
/// well-known name is not consulted: it may have passed to another owner.
pub(crate) async fn owner_connected(system: &Connection, unique: &str) -> zbus::Result<bool> {
    let dbus = DBusProxy::new(system).await?;
    Ok(current_owner(&dbus, unique).await?.is_some())
}

/// Undo one registration at teardown by calling `unregister`, which must
/// target the owner the registration was made with. A failed unregister is
/// suppressed only when `still_connected` confirms that owner's connection is
/// gone, since then its registration went with it. A failed or timed-out
/// check keeps the unregister error. Both calls are bounded.
pub(crate) async fn undo_registration<U, UFut, P, PFut>(
    unregister: U,
    still_connected: P,
) -> Result<(), AgentError>
where
    U: FnOnce() -> UFut,
    UFut: Future<Output = zbus::Result<()>>,
    P: FnOnce() -> PFut,
    PFut: Future<Output = zbus::Result<bool>>,
{
    let Err(error) = bounded(unregister()).await else {
        return Ok(());
    };
    match bounded(still_connected()).await {
        Ok(false) => Ok(()),
        _ => Err(error),
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
        assert!(publish_show_detached(&presenter, permit, 3, spec));
        let Some(PromptCommand::Show { reply, .. }) = receiver.recv().await else {
            panic!("expected a Show command");
        };
        assert!(
            reply.is_closed(),
            "a display-only prompt has no one to answer"
        );
    }

    #[tokio::test]
    async fn display_publish_reports_a_host_that_left_after_the_reservation() {
        // The host reserves room and then leaves before the command is sent:
        // the command is discarded, so the caller must learn it was not shown.
        let (presenter, receiver) = mpsc::channel(4);
        let permit = reserve(&presenter).await.expect("a host is listening");
        drop(receiver);
        let spec = DialogSpec {
            title: "t".into(),
            message: "m".into(),
            buttons: vec!["Close".into()],
            field: None,
        };
        assert!(!publish_show_detached(&presenter, permit, 4, spec));
    }

    #[tokio::test]
    async fn retries_stop_at_the_first_success_and_report_the_last_failure() {
        let (_stop_tx, mut stop) = oneshot::channel::<()>();
        let mut calls = 0;
        let mut register = || {
            calls += 1;
            let succeed = calls == 2;
            async move {
                if succeed {
                    Ok(())
                } else {
                    Err(AgentError::Bus(zbus::Error::Failure("busy".into())))
                }
            }
        };
        let mut owned = || async { Ok(true) };
        let outcome =
            register_with_retries(3, Duration::ZERO, &mut owned, &mut register, &mut stop).await;
        assert!(matches!(outcome, Attempt::Registered));
        assert_eq!(calls, 2, "the second try succeeds, so there is no third");

        let mut calls = 0;
        let mut always_fails = || {
            calls += 1;
            async { Err(AgentError::Bus(zbus::Error::Failure("busy".into()))) }
        };
        let outcome =
            register_with_retries(3, Duration::ZERO, &mut owned, &mut always_fails, &mut stop)
                .await;
        assert!(matches!(outcome, Attempt::Failed(AgentError::Bus(_))));
        assert_eq!(calls, 3);
    }

    #[tokio::test]
    async fn retries_are_abandoned_when_the_owner_has_gone() {
        // Once the owner has gone, a retry would register with nobody, so the
        // attempt ends as abandoned after its first failure.
        let (_stop_tx, mut stop) = oneshot::channel::<()>();
        let mut calls = 0;
        let mut register = || {
            calls += 1;
            async { Err(AgentError::Bus(zbus::Error::Failure("busy".into()))) }
        };
        let mut gone = || async { Ok(false) };
        let outcome =
            register_with_retries(3, Duration::ZERO, &mut gone, &mut register, &mut stop).await;
        assert!(matches!(outcome, Attempt::Abandoned), "{outcome:?}");
        assert_eq!(calls, 1, "no retry once the owner has gone");
    }

    #[tokio::test]
    async fn stop_interrupts_a_registration_the_manager_never_answers() {
        let (stop_tx, mut stop) = oneshot::channel::<()>();
        let mut register = || async { std::future::pending::<Result<(), AgentError>>().await };
        let mut owned = || async { Ok(true) };
        let stopper = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            let _ = stop_tx.send(());
        });
        let outcome =
            register_with_retries(3, Duration::ZERO, &mut owned, &mut register, &mut stop).await;
        stopper.await.unwrap();
        assert!(matches!(outcome, Attempt::Stopped), "{outcome:?}");
    }

    #[tokio::test]
    async fn a_call_that_never_replies_times_out() {
        let result: Result<(), AgentError> = bounded_for(
            Duration::from_millis(10),
            std::future::pending::<zbus::Result<()>>(),
        )
        .await;
        assert!(matches!(result, Err(AgentError::Timeout)), "{result:?}");
    }

    #[test]
    fn an_arrived_owner_is_registered_once_and_only_while_it_is_current() {
        // Already tried: a queued change for the same name is skipped.
        assert!(!should_register_arrival(Some(":1.7"), ":1.7", Some(":1.7")));
        // Released again before the watch read its event: not the owner now.
        assert!(!should_register_arrival(None, ":1.7", None));
        assert!(!should_register_arrival(None, ":1.7", Some(":1.8")));
        // A new owner that is still the owner is registered with.
        assert!(should_register_arrival(None, ":1.7", Some(":1.7")));
        assert!(should_register_arrival(Some(":1.6"), ":1.7", Some(":1.7")));
    }

    #[test]
    fn owner_slot_clears_only_the_matching_owner() {
        let slot = OwnerSlot::default();
        slot.set(":1.20");
        slot.clear_if(":1.21");
        assert_eq!(slot.get().as_deref(), Some(":1.20"));
        slot.clear_if(":1.20");
        assert_eq!(slot.get(), None);
    }

    #[test]
    fn a_registration_call_records_its_owner_only_until_a_refusal_undoes_it() {
        let registrations = Registrations::default();
        // Refused: the record this call made is removed.
        RegistrationCall::begin(&registrations, ":1.20").withdraw();
        assert!(registrations.take_all().is_empty());

        // Confirmed: the record stands.
        RegistrationCall::begin(&registrations, ":1.20").keep();
        assert_eq!(registrations.take_all(), vec![":1.20".to_owned()]);
    }

    #[test]
    fn a_refusal_does_not_undo_a_record_an_uncertain_call_left() {
        let registrations = Registrations::default();
        // The first call may have applied: its reply never came.
        RegistrationCall::begin(&registrations, ":1.20").keep();
        // A later call finds the agent already there and is refused as ours.
        RegistrationCall::begin(&registrations, ":1.20").withdraw();
        assert_eq!(registrations.take_all(), vec![":1.20".to_owned()]);
    }

    #[test]
    fn a_lost_owner_keeps_its_record_beside_its_replacement() {
        let registrations = Registrations::default();
        RegistrationCall::begin(&registrations, ":1.20").keep();
        RegistrationCall::begin(&registrations, ":1.21").keep();
        assert_eq!(
            registrations.take_all(),
            vec![":1.20".to_owned(), ":1.21".to_owned()],
            "both owners are undone at teardown, once each"
        );
    }
}
