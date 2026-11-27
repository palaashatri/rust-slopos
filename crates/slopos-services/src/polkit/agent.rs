//! The polkit authentication agent on the system bus.
//!
//! The agent registers with `org.freedesktop.PolicyKit1.Authority`
//! (`RegisterAuthenticationAgent`, subject `unix-process`) and serves
//! `org.freedesktop.PolicyKit1.AuthenticationAgent` at its own object path.
//! polkit calls `BeginAuthentication`, which returns at once; a request thread
//! then runs the helper, asks the UI for the password through a [`PromptSink`]
//! and, when the helper succeeds, calls `AuthenticationAgentResponse2` on the
//! Authority. `CancelAuthentication` stops a request. The agent never checks
//! or stores a password: the helper and polkit do that.
//!
//! Target: polkit's agent API as shipped in polkit 0.105 and later (the
//! `Response2` call carries the uid). Not run against a real polkitd yet.

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use tokio::sync::oneshot;
use zbus::{
    zvariant::{OwnedValue, Value},
    Connection,
};

use super::{
    helper::{CancelToken, Conversation, Helper, HelperOutcome},
    state::{choose_identity, AuthRequest, Failure, Identity, Phase},
};
use crate::polkit::helper::username_for_uid;

pub use zeroize::Zeroizing;

pub const AUTHORITY_SERVICE: &str = "org.freedesktop.PolicyKit1";
pub const AUTHORITY_PATH: &str = "/org/freedesktop/PolicyKit1/Authority";
pub const AUTHORITY_INTERFACE: &str = "org.freedesktop.PolicyKit1.Authority";
pub const AGENT_INTERFACE: &str = "org.freedesktop.PolicyKit1.AuthenticationAgent";
/// Where the agent serves its interface. Any path works; polkit learns it
/// from the registration call.
pub const AGENT_OBJECT_PATH: &str = "/org/slopos/PolicyKit1/AuthenticationAgent";

/// A `(sa{sv})` subject or identity as it crosses D-Bus.
pub type Wire = (String, HashMap<String, OwnedValue>);

#[derive(Debug, thiserror::Error)]
pub enum PolkitError {
    #[error("polkit D-Bus error: {0}")]
    Bus(#[from] zbus::Error),
    #[error("cannot read this process's start time: {0}")]
    StartTime(#[source] std::io::Error),
    #[error("polkit reused authentication cookie {0:?}")]
    DuplicateCookie(String),
    #[error("polkit offered no identity this agent can authenticate as")]
    NoIdentity,
    #[error("no account exists for uid {0}")]
    UnknownUser(u32),
    #[error("could not start a request thread: {0}")]
    Thread(#[source] std::io::Error),
    #[error("could not encode a D-Bus value: {0}")]
    Encode(#[from] zbus::zvariant::Error),
}

/// What the UI needs to show one password prompt.
#[derive(Debug)]
pub struct PromptRequest {
    pub cookie: String,
    pub action_id: String,
    /// polkit's message, shown as the dialog text.
    pub message: String,
    /// The account the password is for.
    pub username: String,
    /// True when polkit retried after the helper rejected the last password.
    pub wrong_password: bool,
    /// Answer exactly once. Dropping it counts as cancelled.
    pub reply: oneshot::Sender<PromptReply>,
}

#[derive(Debug)]
pub enum PromptReply {
    Password(Zeroizing<String>),
    Cancel,
}

/// Commands from the agent to the UI.
#[derive(Debug)]
pub enum UiCommand {
    Prompt(PromptRequest),
    /// The request with this cookie was cancelled; close its dialog and drop
    /// its reply.
    Cancel { cookie: String },
    /// Show a failure message (dialog with one OK button).
    Error(String),
}

/// Where the agent sends UI commands. Implemented for `mpsc::Sender`; the
/// window process wraps its sender so it can wake its event loop too.
pub trait PromptSink: Send + Sync + 'static {
    /// False when the UI is gone.
    fn send(&self, command: UiCommand) -> bool;
}

impl PromptSink for std::sync::mpsc::Sender<UiCommand> {
    fn send(&self, command: UiCommand) -> bool {
        std::sync::mpsc::Sender::send(self, command).is_ok()
    }
}

/// Decode one identity from polkit's `(sa{sv})`. Only `unix-user` with a
/// `uid` is usable; anything else is kept as `Other`.
pub fn identity_from_wire(kind: &str, details: &HashMap<String, OwnedValue>) -> Identity {
    if kind == "unix-user" {
        if let Some(uid) = details
            .get("uid")
            .and_then(|value| u32::try_from(value.clone()).ok())
        {
            return Identity::UnixUser { uid };
        }
    }
    Identity::Other {
        kind: kind.to_owned(),
    }
}

/// The start time (field 22 of `/proc/<pid>/stat`, in clock ticks). The
/// command name can contain spaces and parentheses, so fields are counted
/// after the last `)`.
pub fn parse_start_time(stat: &str) -> Option<u64> {
    let after_comm = stat.get(stat.rfind(')')? + 1..)?;
    // After ')' the first field is the state (field 3); starttime is field 22.
    after_comm.split_whitespace().nth(22 - 3)?.parse().ok()
}

fn unix_process_subject() -> Result<Wire, PolkitError> {
    let stat = std::fs::read_to_string("/proc/self/stat").map_err(PolkitError::StartTime)?;
    let start_time = parse_start_time(&stat).ok_or_else(|| {
        PolkitError::StartTime(std::io::Error::other("unparsable /proc/self/stat"))
    })?;
    let mut details = HashMap::new();
    details.insert(
        "pid".to_owned(),
        OwnedValue::try_from(Value::from(std::process::id()))?,
    );
    details.insert(
        "start-time".to_owned(),
        OwnedValue::try_from(Value::from(start_time))?,
    );
    Ok(("unix-process".to_owned(), details))
}

fn unix_user_wire(uid: u32) -> Result<Wire, PolkitError> {
    let mut details = HashMap::new();
    details.insert(
        "uid".to_owned(),
        OwnedValue::try_from(Value::from(uid))?,
    );
    Ok(("unix-user".to_owned(), details))
}

fn current_uid() -> u32 {
    // SAFETY: getuid(2) has no preconditions and cannot fail.
    unsafe { libc::getuid() }
}

struct ActiveRequest {
    request: Mutex<AuthRequest>,
    cancel: CancelToken,
}

/// State shared by the D-Bus object and the request threads.
struct Shared {
    agent_uid: u32,
    connection: Connection,
    sink: Arc<dyn PromptSink>,
    helper: Arc<dyn Helper>,
    active: Mutex<HashMap<String, Arc<ActiveRequest>>>,
    /// The last helper run rejected the password, so polkit's next
    /// BeginAuthentication is a retry that shows "Wrong password".
    last_wrong_password: AtomicBool,
}

/// polkit's BeginAuthentication: validate, record the request and start its
/// thread. Returns before any prompt is shown.
fn begin(
    shared: &Arc<Shared>,
    action_id: String,
    message: String,
    cookie: String,
    identities: Vec<Wire>,
) -> zbus::fdo::Result<()> {
    if lock(&shared.active).contains_key(&cookie) {
        return Err(zbus::fdo::Error::Failed(
            PolkitError::DuplicateCookie(cookie).to_string(),
        ));
    }
    let offered: Vec<Identity> = identities
        .iter()
        .map(|(kind, details)| identity_from_wire(kind, details))
        .collect();
    let identity = choose_identity(&offered, shared.agent_uid)
        .cloned()
        .ok_or_else(|| zbus::fdo::Error::Failed(PolkitError::NoIdentity.to_string()))?;
    let uid = match &identity {
        Identity::UnixUser { uid } => *uid,
        Identity::Other { .. } => {
            return Err(zbus::fdo::Error::Failed(PolkitError::NoIdentity.to_string()))
        }
    };
    let username = username_for_uid(uid)
        .ok_or_else(|| zbus::fdo::Error::Failed(PolkitError::UnknownUser(uid).to_string()))?;
    let mut request = AuthRequest::new(&cookie, &action_id, &message, identity);
    let after_wrong = shared.last_wrong_password.load(Ordering::SeqCst);
    request
        .begin(after_wrong)
        .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))?;
    let active = Arc::new(ActiveRequest {
        request: Mutex::new(request),
        cancel: CancelToken::default(),
    });
    lock(&shared.active).insert(cookie.clone(), Arc::clone(&active));
    let worker = Arc::clone(shared);
    std::thread::Builder::new()
        .name("polkit-request".into())
        .spawn(move || run(&worker, &active, &cookie, &username, uid))
        .map_err(|error| zbus::fdo::Error::Failed(PolkitError::Thread(error).to_string()))?;
    Ok(())
}

/// polkit's CancelAuthentication: stop the request, kill its helper and close
/// its dialog. Unknown cookies are ignored (polkit may cancel late).
fn cancel(shared: &Shared, cookie: &str) {
    let Some(active) = lock(&shared.active).get(cookie).cloned() else {
        return;
    };
    let _ = lock(&active.request).cancel();
    active.cancel.cancel();
    shared.sink.send(UiCommand::Cancel {
        cookie: cookie.to_owned(),
    });
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

/// One request thread: run the helper, then settle the request.
fn run(shared: &Arc<Shared>, active: &Arc<ActiveRequest>, cookie: &str, username: &str, uid: u32) {
    let mut conversation = UiConversation {
        shared,
        active,
        cookie,
        action_id: lock(&active.request).action_id.clone(),
        message: lock(&active.request).message.clone(),
        username,
    };
    let outcome = shared
        .helper
        .authenticate(username, cookie, &mut conversation, &active.cancel);
    match outcome {
        HelperOutcome::Authenticated => respond_authorized(shared, active, cookie, uid),
        HelperOutcome::WrongPassword => {
            let _ = lock(&active.request).fail(Failure::WrongPassword, "reject");
        }
        HelperOutcome::Failed(_) => {
            let _ = lock(&active.request).fail(Failure::Helper, "fail");
        }
        HelperOutcome::Cancelled => {
            let _ = lock(&active.request).cancel();
        }
        HelperOutcome::Unavailable(_) => {
            let _ = lock(&active.request).fail(Failure::HelperUnavailable, "fail");
        }
    }
    lock(&shared.active).remove(cookie);
    let phase = {
        let mut request = lock(&active.request);
        // A request the helper never settled (for example it authenticated
        // without asking) must not stay live.
        if matches!(request.phase(), Phase::Idle | Phase::Prompting | Phase::Verifying) {
            let _ = request.fail(Failure::Helper, "fail");
        }
        request.phase()
    };
    shared
        .last_wrong_password
        .store(phase == Phase::Failed(Failure::WrongPassword), Ordering::SeqCst);
    match phase {
        Phase::Failed(Failure::WrongPassword) | Phase::Answered | Phase::Cancelled => {}
        Phase::Failed(failure) => {
            shared.sink.send(UiCommand::Error(failure.message().to_owned()));
        }
        Phase::Idle | Phase::Prompting | Phase::Verifying => {}
    }
}

/// The helper said SUCCESS: report the identity to polkit. Only a request
/// still waiting for its answer may be authorized, so a cancel that raced
/// the helper wins.
fn respond_authorized(shared: &Arc<Shared>, active: &Arc<ActiveRequest>, cookie: &str, uid: u32) {
    if lock(&active.request).phase() != Phase::Verifying {
        return;
    }
    let result = futures::executor::block_on(respond(&shared.connection, shared.agent_uid, cookie, uid));
    let mut request = lock(&active.request);
    match result {
        Ok(()) => {
            let _ = request.verified();
        }
        Err(_) => {
            let _ = request.fail(Failure::Helper, "fail");
        }
    }
}

/// `AuthenticationAgentResponse2(uid, cookie, identity)`. The first uid is
/// the agent's own user; the identity names the account authenticated.
async fn respond(
    connection: &Connection,
    agent_uid: u32,
    cookie: &str,
    identity_uid: u32,
) -> Result<(), PolkitError> {
    let identity = unix_user_wire(identity_uid)?;
    connection
        .call_method(
            Some(AUTHORITY_SERVICE),
            AUTHORITY_PATH,
            Some(AUTHORITY_INTERFACE),
            "AuthenticationAgentResponse2",
            &(agent_uid, cookie, identity),
        )
        .await?;
    Ok(())
}

/// The helper's conversation: each PAM prompt becomes a UI prompt.
struct UiConversation<'a> {
    shared: &'a Arc<Shared>,
    active: &'a Arc<ActiveRequest>,
    cookie: &'a str,
    action_id: String,
    message: String,
    username: &'a str,
}

impl Conversation for UiConversation<'_> {
    fn secret(&mut self, _prompt: &str) -> Option<Zeroizing<String>> {
        if self.active.cancel.is_cancelled() {
            return None;
        }
        let wrong_password = lock(&self.active.request).shows_wrong_password();
        let (reply, answer) = oneshot::channel();
        let sent = self.shared.sink.send(UiCommand::Prompt(PromptRequest {
            cookie: self.cookie.to_owned(),
            action_id: self.action_id.clone(),
            message: self.message.clone(),
            username: self.username.to_owned(),
            wrong_password,
            reply,
        }));
        if !sent {
            return None;
        }
        match answer.blocking_recv() {
            Ok(PromptReply::Password(password)) => {
                lock(&self.active.request).submit().ok()?;
                Some(password)
            }
            Ok(PromptReply::Cancel) | Err(_) => None,
        }
    }

    fn error(&mut self, text: &str) {
        self.shared.sink.send(UiCommand::Error(text.to_owned()));
    }

    fn info(&mut self, _text: &str) {}
}

/// The D-Bus object polkit calls.
pub struct AgentObject {
    shared: Arc<Shared>,
}

#[zbus::interface(name = "org.freedesktop.PolicyKit1.AuthenticationAgent")]
impl AgentObject {
    async fn begin_authentication(
        &self,
        action_id: String,
        message: String,
        icon_name: String,
        details: HashMap<String, String>,
        cookie: String,
        identities: Vec<Wire>,
    ) -> zbus::fdo::Result<()> {
        // The icon and details are polkit's presentation hints; the dialog
        // uses the message and the action id.
        let _ = (icon_name, details);
        begin(&self.shared, action_id, message, cookie, identities)
    }

    async fn cancel_authentication(&self, cookie: String) -> zbus::fdo::Result<()> {
        cancel(&self.shared, &cookie);
        Ok(())
    }
}

/// A running agent: served on `connection` and registered with the
/// Authority.
pub struct AgentService {
    connection: Connection,
    subject: Wire,
    object_path: String,
}

impl AgentService {
    /// Serve the agent object on `connection` and register it. `connection`
    /// is the system bus in production and a private bus in tests.
    pub async fn start(
        connection: Connection,
        sink: Arc<dyn PromptSink>,
        helper: Arc<dyn Helper>,
        object_path: &str,
    ) -> Result<Self, PolkitError> {
        let shared = Arc::new(Shared {
            agent_uid: current_uid(),
            connection: connection.clone(),
            sink,
            helper,
            active: Mutex::new(HashMap::new()),
            last_wrong_password: AtomicBool::new(false),
        });
        connection
            .object_server()
            .at(object_path, AgentObject { shared })
            .await?;
        let subject = unix_process_subject()?;
        let locale = std::env::var("LANG").unwrap_or_else(|_| "C".to_owned());
        connection
            .call_method(
                Some(AUTHORITY_SERVICE),
                AUTHORITY_PATH,
                Some(AUTHORITY_INTERFACE),
                "RegisterAuthenticationAgent",
                &(&subject, locale.as_str(), object_path),
            )
            .await?;
        Ok(Self {
            connection,
            subject,
            object_path: object_path.to_owned(),
        })
    }

    /// Unregister and stop serving the object.
    pub async fn stop(self) -> Result<(), PolkitError> {
        self.connection
            .call_method(
                Some(AUTHORITY_SERVICE),
                AUTHORITY_PATH,
                Some(AUTHORITY_INTERFACE),
                "UnregisterAuthenticationAgent",
                &(&self.subject, self.object_path.as_str()),
            )
            .await?;
        self.connection
            .object_server()
            .remove::<AgentObject, _>(self.object_path.as_str())
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_time_is_field_22_even_when_the_command_has_parentheses() {
        let mut fields = vec!["S".to_owned()];
        fields.extend((1..=19).map(|n| if n == 19 { "4242".to_owned() } else { n.to_string() }));
        let stat = format!("123 (slopos (agent) x) {}", fields.join(" "));
        assert_eq!(parse_start_time(&stat), Some(4242));
        assert_eq!(parse_start_time("garbage"), None);
    }

    #[test]
    fn unix_user_identity_needs_a_uid() {
        let mut details = HashMap::new();
        details.insert("uid".to_owned(), OwnedValue::try_from(Value::from(0u32)).unwrap());
        assert_eq!(
            identity_from_wire("unix-user", &details),
            Identity::UnixUser { uid: 0 }
        );
        assert_eq!(
            identity_from_wire("unix-user", &HashMap::new()),
            Identity::Other {
                kind: "unix-user".into()
            }
        );
        assert_eq!(
            identity_from_wire("unix-group", &details),
            Identity::Other {
                kind: "unix-group".into()
            }
        );
    }
}

#[cfg(test)]
mod bus_tests {
    //! The agent against a fake Authority on a private bus. No polkitd, no
    //! helper binary: the helper is a fake, and the test plays the Authority.

    use super::*;
    use std::{
        io::{BufRead, BufReader},
        process::{Command, Stdio},
        sync::mpsc as std_mpsc,
        time::Duration,
    };
    use zbus::connection::Builder;

    const WAIT: Duration = Duration::from_secs(10);

    struct BusDaemon(std::process::Child);

    impl Drop for BusDaemon {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn private_bus() -> (BusDaemon, String) {
        let mut daemon = BusDaemon(
            Command::new("dbus-daemon")
                .args(["--session", "--nofork", "--print-address=1"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .expect("dbus-daemon is installed"),
        );
        let mut address = String::new();
        BufReader::new(daemon.0.stdout.as_mut().unwrap())
            .read_line(&mut address)
            .unwrap();
        (daemon, address.trim().to_owned())
    }

    /// Stands in for `org.freedesktop.PolicyKit1.Authority` and records what
    /// the agent sends it.
    struct FakeAuthority {
        registered: std_mpsc::Sender<(Wire, String)>,
        responses: std_mpsc::Sender<(u32, String, Wire)>,
    }

    #[zbus::interface(name = "org.freedesktop.PolicyKit1.Authority")]
    impl FakeAuthority {
        fn register_authentication_agent(&self, subject: Wire, locale: String, object_path: String) {
            let _ = locale;
            let _ = self.registered.send((subject, object_path));
        }

        fn unregister_authentication_agent(&self, subject: Wire, object_path: String) {
            let _ = (subject, object_path);
        }

        fn authentication_agent_response2(&self, uid: u32, cookie: String, identity: Wire) {
            let _ = self.responses.send((uid, cookie, identity));
        }
    }

    /// Accepts only "correct horse", like a real helper would for a known
    /// password.
    struct AcceptingHelper;

    impl Helper for AcceptingHelper {
        fn authenticate(
            &self,
            username: &str,
            cookie: &str,
            conversation: &mut dyn Conversation,
            cancel: &CancelToken,
        ) -> HelperOutcome {
            let _ = (username, cookie, cancel);
            match conversation.secret("Password:") {
                Some(password) if password.as_str() == "correct horse" => {
                    HelperOutcome::Authenticated
                }
                Some(_) => HelperOutcome::WrongPassword,
                None => HelperOutcome::Cancelled,
            }
        }
    }

    struct Harness {
        _bus: BusDaemon,
        authority: Connection,
        agent_name: String,
        registered: std_mpsc::Receiver<(Wire, String)>,
        responses: std_mpsc::Receiver<(u32, String, Wire)>,
        commands: std_mpsc::Receiver<UiCommand>,
        _service: AgentService,
    }

    impl Harness {
        async fn start() -> Self {
            let (bus, address) = private_bus();
            let (registered_tx, registered) = std_mpsc::channel();
            let (responses_tx, responses) = std_mpsc::channel();
            let authority = Builder::address(address.as_str())
                .unwrap()
                .name(AUTHORITY_SERVICE)
                .unwrap()
                .serve_at(
                    AUTHORITY_PATH,
                    FakeAuthority {
                        registered: registered_tx,
                        responses: responses_tx,
                    },
                )
                .unwrap()
                .build()
                .await
                .unwrap();
            let agent = Builder::address(address.as_str())
                .unwrap()
                .build()
                .await
                .unwrap();
            let agent_name = agent.unique_name().unwrap().to_string();
            let (sink_tx, commands) = std_mpsc::channel();
            let service = AgentService::start(
                agent,
                Arc::new(sink_tx),
                Arc::new(AcceptingHelper),
                AGENT_OBJECT_PATH,
            )
            .await
            .expect("the agent registers with the fake Authority");
            Self {
                _bus: bus,
                authority,
                agent_name,
                registered,
                responses,
                commands,
                _service: service,
            }
        }

        /// polkit's BeginAuthentication, sent by the fake Authority to the agent.
        async fn begin(&self, cookie: &str) -> zbus::Result<()> {
            let identities: Vec<Wire> = vec![
                ("unix-group".to_owned(), HashMap::new()),
                unix_user_wire(current_uid()).unwrap(),
            ];
            self.authority
                .call_method(
                    Some(self.agent_name.as_str()),
                    AGENT_OBJECT_PATH,
                    Some(AGENT_INTERFACE),
                    "BeginAuthentication",
                    &(
                        "org.example.install",
                        "Install software.",
                        "",
                        HashMap::<String, String>::new(),
                        cookie,
                        identities,
                    ),
                )
                .await
                .map(drop)
        }

        /// polkit's CancelAuthentication.
        async fn cancel(&self, cookie: &str) {
            self.authority
                .call_method(
                    Some(self.agent_name.as_str()),
                    AGENT_OBJECT_PATH,
                    Some(AGENT_INTERFACE),
                    "CancelAuthentication",
                    &(cookie,),
                )
                .await
                .unwrap();
        }

        fn next_command(&self) -> UiCommand {
            self.commands.recv_timeout(WAIT).expect("a UI command")
        }

        fn next_prompt(&self) -> PromptRequest {
            match self.next_command() {
                UiCommand::Prompt(request) => request,
                other => panic!("expected a prompt, got {other:?}"),
            }
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn registers_as_a_unix_process_agent_at_its_path() {
        let harness = Harness::start().await;
        let (subject, path) = harness.registered.recv_timeout(WAIT).unwrap();
        assert_eq!(subject.0, "unix-process");
        assert_eq!(path, AGENT_OBJECT_PATH);
        assert_eq!(
            u32::try_from(subject.1["pid"].clone()).unwrap(),
            std::process::id()
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn correct_password_is_answered_to_the_authority() {
        let harness = Harness::start().await;
        harness.registered.recv_timeout(WAIT).unwrap();
        harness.begin("cookie-ok").await.unwrap();

        let request = harness.next_prompt();
        assert_eq!(request.cookie, "cookie-ok");
        assert_eq!(request.action_id, "org.example.install");
        assert_eq!(request.message, "Install software.");
        assert!(!request.wrong_password);
        let PromptRequest { reply, .. } = request;
        reply
            .send(PromptReply::Password(Zeroizing::new("correct horse".to_owned())))
            .unwrap();

        let (uid, cookie, identity) = harness.responses.recv_timeout(WAIT).unwrap();
        assert_eq!(uid, current_uid(), "the agent's own uid comes first");
        assert_eq!(cookie, "cookie-ok");
        assert_eq!(identity.0, "unix-user");
        assert_eq!(
            u32::try_from(identity.1["uid"].clone()).unwrap(),
            current_uid(),
            "the agent's own user is the identity chosen"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_user_cancelling_the_dialog_answers_nothing() {
        let harness = Harness::start().await;
        harness.registered.recv_timeout(WAIT).unwrap();
        harness.begin("cookie-user-cancel").await.unwrap();
        let PromptRequest { reply, .. } = harness.next_prompt();
        reply.send(PromptReply::Cancel).unwrap();
        assert!(harness.responses.recv_timeout(Duration::from_millis(500)).is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn polkit_cancelling_closes_the_prompt_and_answers_nothing() {
        let harness = Harness::start().await;
        harness.registered.recv_timeout(WAIT).unwrap();
        harness.begin("cookie-cancel").await.unwrap();
        let request = harness.next_prompt();
        assert_eq!(request.cookie, "cookie-cancel");

        harness.cancel("cookie-cancel").await;
        match harness.next_command() {
            UiCommand::Cancel { cookie } => assert_eq!(cookie, "cookie-cancel"),
            other => panic!("expected a cancel, got {other:?}"),
        }
        // The UI drops its reply when it closes the dialog.
        drop(request);
        assert!(harness.responses.recv_timeout(Duration::from_millis(500)).is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_begin_with_no_usable_identity_is_refused_without_a_prompt() {
        let harness = Harness::start().await;
        harness.registered.recv_timeout(WAIT).unwrap();
        let identities: Vec<Wire> = vec![("unix-group".to_owned(), HashMap::new())];
        let result = harness
            .authority
            .call_method(
                Some(harness.agent_name.as_str()),
                AGENT_OBJECT_PATH,
                Some(AGENT_INTERFACE),
                "BeginAuthentication",
                &(
                    "org.example.install",
                    "Install software.",
                    "",
                    HashMap::<String, String>::new(),
                    "cookie-group",
                    identities,
                ),
            )
            .await;
        assert!(result.is_err());
        assert!(harness.commands.recv_timeout(Duration::from_millis(300)).is_err());
    }
}
