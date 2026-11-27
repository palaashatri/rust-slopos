//! Input-method client for IBus (`org.freedesktop.IBus`) over D-Bus.
//!
//! [`InputMethod`] is what the X11 host drives: it finds the IBus bus, creates
//! one input context for the window, forwards key presses while a text field
//! asks for text input, and turns the context's signals into
//! [`InputMethodEvent`]s. When IBus is not running, or the connection fails
//! or drops, [`InputMethod::status`] says "unavailable" with the reason, and
//! keys go straight to the application as before.
//!
//! The wire format is in [`protocol`]. Signals are read by one listener
//! thread that blocks on the connection (no polling) and wakes the event loop
//! through the window's wake pipe.

pub mod protocol;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use zbus::blocking::{connection, fdo::DBusProxy, Connection, MessageIterator, Proxy};
use zbus::message::Type;
use zbus::zvariant::OwnedObjectPath;
use zbus::MatchRule;

use crate::input_method::{InputMethodEvent, InputMethodStatus};
use protocol::{BUS_INTERFACE, BUS_PATH, CONTEXT_INTERFACE, SERVICE};

/// What the listener thread sends to the event loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incoming {
    Input(InputMethodEvent),
    /// The bus connection ended; the reason is for diagnostics.
    Disconnected(String),
}

/// What [`InputMethod::drain`] hands the event loop.
pub use crate::input_method::Update;

fn text(error: impl std::fmt::Display) -> String {
    error.to_string()
}

/// How long one call to the input method may take before it is given up.
/// Calls come from the UI thread (focus, caret location, key presses), so a
/// hung input method must not hold the window; the timeout ends the input
/// method and the key reaches the application.
pub const CONTEXT_CALL_TIMEOUT: Duration = Duration::from_millis(200);

/// Run `call` on a worker thread and wait at most [`CONTEXT_CALL_TIMEOUT`]
/// for its result. `what` names the call in the error. A call that does not
/// finish in time returns an error; the worker ends on its own when zbus
/// gives up on it.
fn bounded<T: Send + 'static>(
    what: &'static str,
    call: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let (done, outcome) = mpsc::channel();
    thread::Builder::new()
        .name("slopos-ibus-call".into())
        .spawn(move || {
            let _ = done.send(call());
        })
        .map_err(text)?;
    match outcome.recv_timeout(CONTEXT_CALL_TIMEOUT) {
        Ok(reply) => reply,
        Err(mpsc::RecvTimeoutError::Timeout) => Err(format!(
            "{what} did not answer within {} ms",
            CONTEXT_CALL_TIMEOUT.as_millis()
        )),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(format!("{what} was not sent")),
    }
}

/// A connection to one IBus input context.
pub struct IbusClient {
    connection: Connection,
    context: OwnedObjectPath,
}

impl IbusClient {
    /// Connect to the IBus bus at `address`, create an input context and
    /// start the listener thread. `incoming` receives the context's text
    /// signals; `wake` is called after each one so the event loop wakes up.
    pub fn connect(
        address: &str,
        incoming: Sender<Incoming>,
        wake: impl Fn() + Send + 'static,
    ) -> Result<Self, String> {
        let connection = connection::Builder::address(address)
            .map_err(text)?
            .build()
            .map_err(text)?;
        // Both calls wait on the daemon, so they get the same bound as the
        // context calls: a silent daemon fails the connect instead of
        // holding the window.
        let bus_connection = connection.clone();
        let context: OwnedObjectPath = bounded("CreateInputContext", move || {
            let bus =
                Proxy::new(&bus_connection, SERVICE, BUS_PATH, BUS_INTERFACE).map_err(text)?;
            bus.call("CreateInputContext", &"SLOPOS")
                .map_err(|error| format!("CreateInputContext failed: {error}"))
        })?;
        // Signals reach a connection only with a match rule (on a bus, the
        // daemon routes by rule). The rule names this context's path.
        let rule_connection = connection.clone();
        let context_path = context.to_string();
        bounded("AddMatch", move || -> Result<(), String> {
            let rule = MatchRule::builder()
                .msg_type(Type::Signal)
                .interface(CONTEXT_INTERFACE)
                .map_err(text)?
                .path(context_path.as_str())
                .map_err(text)?
                .build();
            DBusProxy::new(&rule_connection)
                .map_err(text)?
                .add_match_rule(rule)
                .map(|_| ())
                .map_err(text)
        })?;

        let listener = connection.clone();
        let path = context.to_string();
        thread::Builder::new()
            .name("slopos-ibus".into())
            .spawn(move || listen(listener, path, incoming, wake))
            .map_err(text)?;

        let client = Self {
            connection,
            context,
        };
        client.context_call("SetCapabilities", protocol::capabilities())?;
        Ok(client)
    }

    /// Call `method` on this input context, waiting at most
    /// [`CONTEXT_CALL_TIMEOUT`]. The call runs on a worker thread so the
    /// caller is never blocked past that bound. A timed-out call returns an
    /// error; the worker ends on its own when zbus gives up on it.
    fn context_call<B>(&self, method: &'static str, body: B) -> Result<zbus::Message, String>
    where
        B: zbus::zvariant::DynamicType + zbus::export::serde::Serialize + Send + 'static,
    {
        let connection = self.connection.clone();
        let context = self.context.to_string();
        bounded(method, move || {
            Proxy::new(&connection, SERVICE, context.as_str(), CONTEXT_INTERFACE)
                .map_err(text)
                .and_then(|proxy| {
                    proxy
                        .call_method(method, &body)
                        .map_err(|error| format!("{method} failed: {error}"))
                })
        })
    }

    /// The text field gained keyboard focus in the input method's view.
    pub fn focus_in(&self) -> Result<(), String> {
        self.context_call("FocusIn", ()).map(|_| ())
    }

    /// The text field lost focus; the input method ends any composition.
    pub fn focus_out(&self) -> Result<(), String> {
        self.context_call("FocusOut", ()).map(|_| ())
    }

    /// Where the caret is, in root-window device pixels. Candidate windows
    /// are placed next to it.
    pub fn set_cursor_location(
        &self,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> Result<(), String> {
        self.context_call("SetCursorLocation", (x, y, width, height))
            .map(|_| ())
    }

    /// Offer a key press to the input method. `Ok(true)` means it consumed the
    /// key (the application must not see it). `keycode` is the evdev code
    /// (X keycode minus 8) and `state` uses X modifier bits, which IBus shares.
    pub fn process_key_event(&self, keysym: u32, keycode: u32, state: u32) -> Result<bool, String> {
        let reply = self.context_call("ProcessKeyEvent", (keysym, keycode, state))?;
        reply.body().deserialize::<bool>().map_err(text)
    }
}

/// Deliver the context's signals until the connection ends.
fn listen(connection: Connection, context: String, incoming: Sender<Incoming>, wake: impl Fn()) {
    for received in MessageIterator::from(&connection) {
        match received {
            Ok(message) => {
                if let Some(event) = decode(&message, &context) {
                    if incoming.send(Incoming::Input(event)).is_err() {
                        return;
                    }
                    wake();
                }
            }
            Err(error) => {
                let _ = incoming.send(Incoming::Disconnected(text(error)));
                wake();
                return;
            }
        }
    }
    let _ = incoming.send(Incoming::Disconnected("the IBus connection closed".into()));
    wake();
}

/// The event for one of the context's text signals, if `message` is one.
fn decode(message: &zbus::Message, context: &str) -> Option<InputMethodEvent> {
    let header = message.header();
    if header.message_type() != Type::Signal {
        return None;
    }
    if header.interface().map(|name| name.as_str()) != Some(CONTEXT_INTERFACE) {
        return None;
    }
    if header.path().map(|path| path.as_str()) != Some(context) {
        return None;
    }
    match header.member()?.as_str() {
        "CommitText" => {
            let text: zbus::zvariant::OwnedValue = message.body().deserialize().ok()?;
            protocol::commit_event(&text)
        }
        "UpdatePreeditText" => {
            let (text, cursor, visible): (zbus::zvariant::OwnedValue, u32, bool) =
                message.body().deserialize().ok()?;
            protocol::preedit_event(&text, cursor, visible)
        }
        "HidePreeditText" => Some(InputMethodEvent::PreeditHidden),
        _ => None,
    }
}

/// The bus address to use: `IBUS_ADDRESS` when set, else the address file
/// IBus writes under `bus_dir` for this display. IBus names those files
/// `<machine-id>-<host>-<display number>`, so the display number is the
/// match. Errors say why IBus cannot be reached.
pub fn resolve_address(
    env_address: Option<&str>,
    bus_dir: &Path,
    display: Option<&str>,
) -> Result<String, String> {
    if let Some(address) = env_address.map(str::trim).filter(|value| !value.is_empty()) {
        return Ok(address.to_owned());
    }
    let display = display.ok_or("DISPLAY is not set")?;
    let number =
        display_number(display).ok_or_else(|| format!("unrecognised DISPLAY {display}"))?;
    let entries = fs::read_dir(bus_dir)
        .map_err(|_| format!("no IBus address directory at {}", bus_dir.display()))?;
    let suffix = format!("-{number}");
    let mut names: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(&suffix))
        })
        .collect();
    names.sort();
    for path in names {
        let contents = fs::read_to_string(&path).unwrap_or_default();
        if let Some(address) = contents
            .lines()
            .find_map(|line| line.strip_prefix("IBUS_ADDRESS="))
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Ok(address.to_owned());
        }
    }
    Err(format!("IBus is not running for display {number}"))
}

/// The display number in a `DISPLAY` value such as `:0`, `:1.0` or `host:2`.
fn display_number(display: &str) -> Option<&str> {
    let after = display.rsplit(':').next()?;
    let number = after.split('.').next()?;
    (!number.is_empty() && number.chars().all(|c| c.is_ascii_digit())).then_some(number)
}

fn config_home() -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"))
}

/// Input-method state for one window. Created by [`InputMethod::start`];
/// the event loop calls [`InputMethod::drain`] after it wakes.
pub struct InputMethod {
    client: Option<IbusClient>,
    incoming: Option<Receiver<Incoming>>,
    status: InputMethodStatus,
    pending: Vec<Update>,
    text_input: bool,
    window_focus: bool,
    context_focus: bool,
    cursor: Option<(i32, i32, i32, i32)>,
    sent_cursor: Option<(i32, i32, i32, i32)>,
}

impl InputMethod {
    /// Find IBus for this session and connect. Failure is not an error: the
    /// status reports it and the window works without an input method.
    pub fn start(wake: impl Fn() + Send + 'static) -> Self {
        let env_address = env::var("IBUS_ADDRESS").ok();
        let display = env::var("DISPLAY").ok();
        let bus_dir = config_home().join("ibus").join("bus");
        let address = resolve_address(env_address.as_deref(), &bus_dir, display.as_deref());
        match address {
            Ok(address) => Self::connect_or_unavailable(&address, wake),
            Err(reason) => Self::unavailable(reason),
        }
    }

    /// Connect to the IBus bus at `address`, or return an input method that
    /// reports why it could not connect.
    pub fn connect_or_unavailable(address: &str, wake: impl Fn() + Send + 'static) -> Self {
        Self::connect(address, wake).unwrap_or_else(Self::unavailable)
    }

    /// Connect to the IBus bus at `address`.
    pub fn connect(address: &str, wake: impl Fn() + Send + 'static) -> Result<Self, String> {
        let (sender, receiver) = mpsc::channel();
        let client = IbusClient::connect(address, sender, wake)?;
        Ok(Self {
            client: Some(client),
            incoming: Some(receiver),
            status: InputMethodStatus::Active,
            pending: vec![Update::Status(InputMethodStatus::Active)],
            text_input: false,
            window_focus: false,
            context_focus: false,
            cursor: None,
            sent_cursor: None,
        })
    }

    fn unavailable(reason: String) -> Self {
        log::info!("input method unavailable: {reason}");
        let status = InputMethodStatus::Unavailable(reason);
        Self {
            client: None,
            incoming: None,
            pending: vec![Update::Status(status.clone())],
            status,
            text_input: false,
            window_focus: false,
            context_focus: false,
            cursor: None,
            sent_cursor: None,
        }
    }

    pub fn status(&self) -> &InputMethodStatus {
        &self.status
    }

    /// Whether an input method is connected (its status is `Active`).
    pub fn is_active(&self) -> bool {
        self.client.is_some()
    }

    /// Whether key presses are offered to the input method right now.
    pub fn wants_keys(&self) -> bool {
        self.client.is_some() && self.context_focus
    }

    /// Whether the window has keyboard focus.
    pub fn set_window_focus(&mut self, focused: bool) {
        self.window_focus = focused;
        self.sync_focus();
    }

    /// Whether the application wants text composed (a text field has focus).
    pub fn set_text_input(&mut self, wanted: bool) {
        self.text_input = wanted;
        self.sync_focus();
    }

    /// Tell the input method where the caret is, in root-window device
    /// pixels. Sent only when it changes.
    pub fn set_cursor_location(&mut self, x: i32, y: i32, width: i32, height: i32) {
        self.cursor = Some((x, y, width, height));
        self.send_cursor();
    }

    /// Offer a key press. `true` means IBus consumed it. Any failure, a
    /// timeout included, ends the input method and returns `false`, so the
    /// key reaches the application.
    pub fn process_key(&mut self, keysym: u32, keycode: u32, state: u32) -> bool {
        if !self.wants_keys() {
            return false;
        }
        let Some(client) = &self.client else {
            return false;
        };
        match client.process_key_event(keysym, keycode, state) {
            Ok(handled) => handled,
            Err(reason) => {
                self.fail(reason);
                false
            }
        }
    }

    /// The updates since the last call: IBus text and status changes.
    pub fn drain(&mut self) -> Vec<Update> {
        let received: Vec<Incoming> = self
            .incoming
            .as_ref()
            .map(|incoming| incoming.try_iter().collect())
            .unwrap_or_default();
        for message in received {
            match message {
                Incoming::Input(event) => self.pending.push(Update::Input(event)),
                Incoming::Disconnected(reason) => self.fail(reason),
            }
        }
        std::mem::take(&mut self.pending)
    }

    /// Keep the context focused exactly while the window is focused and a text
    /// field wants text input.
    fn sync_focus(&mut self) {
        let wanted = self.text_input && self.window_focus && self.client.is_some();
        if wanted == self.context_focus {
            return;
        }
        let Some(client) = &self.client else {
            return;
        };
        let result = if wanted {
            client.focus_in()
        } else {
            client.focus_out()
        };
        match result {
            Ok(()) => {
                self.context_focus = wanted;
                // Focus out may reset the daemon's view of the caret, so the
                // location is sent again on the next focus in.
                self.sent_cursor = None;
                if wanted {
                    self.send_cursor();
                }
            }
            Err(reason) => self.fail(reason),
        }
    }

    fn send_cursor(&mut self) {
        let (Some(cursor), Some(client)) = (self.cursor, &self.client) else {
            return;
        };
        if !self.context_focus || self.sent_cursor == Some(cursor) {
            return;
        }
        let (x, y, width, height) = cursor;
        match client.set_cursor_location(x, y, width, height) {
            Ok(()) => self.sent_cursor = Some(cursor),
            Err(reason) => self.fail(reason),
        }
    }

    /// The input method stopped working: keep the text it already sent, end
    /// any composition, drop the connection and report it.
    fn fail(&mut self, reason: String) {
        log::warn!("input method unavailable: {reason}");
        // Text the listener queued before the failure is the user's input
        // and is delivered before the status.
        if let Some(incoming) = self.incoming.take() {
            for message in incoming.try_iter() {
                if let Incoming::Input(event) = message {
                    self.pending.push(Update::Input(event));
                }
            }
        }
        self.pending
            .push(Update::Input(InputMethodEvent::PreeditHidden));
        self.client = None;
        self.context_focus = false;
        self.sent_cursor = None;
        let status = InputMethodStatus::Unavailable(reason);
        self.status = status.clone();
        self.pending.push(Update::Status(status));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("slopos-ibus-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn display_numbers_are_read_from_display_values() {
        assert_eq!(display_number(":0"), Some("0"));
        assert_eq!(display_number(":1.0"), Some("1"));
        assert_eq!(display_number("myhost:12"), Some("12"));
        assert_eq!(display_number(""), None);
        assert_eq!(display_number("host:"), None);
        assert_eq!(display_number("unix"), None);
    }

    #[test]
    fn environment_address_wins_and_blank_is_ignored() {
        let dir = scratch("env");
        assert_eq!(
            resolve_address(Some(" unix:path=/x "), &dir, None).unwrap(),
            "unix:path=/x"
        );
        assert!(resolve_address(Some(" "), &dir, None)
            .unwrap_err()
            .contains("DISPLAY"));
    }

    #[test]
    fn address_file_for_the_display_is_used() {
        let dir = scratch("files");
        fs::write(
            dir.join("0123abcd-0"),
            "IBUS_ADDRESS=unix:abstract=/tmp/ibus-zero\nIBUS_DAEMON_PID=1\n",
        )
        .unwrap();
        fs::write(
            dir.join("0123abcd-1"),
            "IBUS_ADDRESS=unix:abstract=/tmp/ibus-one\n",
        )
        .unwrap();
        assert_eq!(
            resolve_address(None, &dir, Some(":0")).unwrap(),
            "unix:abstract=/tmp/ibus-zero"
        );
        assert_eq!(
            resolve_address(None, &dir, Some(":1.0")).unwrap(),
            "unix:abstract=/tmp/ibus-one"
        );
        // A display with no daemon: the reason names it.
        assert_eq!(
            resolve_address(None, &dir, Some(":7")).unwrap_err(),
            "IBus is not running for display 7"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_bus_directory_is_reported_as_unavailable_with_reason() {
        let missing = env::temp_dir().join("slopos-ibus-does-not-exist");
        let reason = resolve_address(None, &missing, Some(":0")).unwrap_err();
        assert!(reason.starts_with("no IBus address directory"), "{reason}");
        assert!(resolve_address(None, &missing, None)
            .unwrap_err()
            .contains("DISPLAY is not set"));
    }

    #[test]
    fn unavailable_input_reports_its_reason_and_takes_no_keys() {
        let mut input = InputMethod::unavailable("no daemon for test".into());
        assert!(matches!(
            input.status(),
            InputMethodStatus::Unavailable(reason) if reason == "no daemon for test"
        ));
        assert_eq!(
            input.drain(),
            vec![Update::Status(InputMethodStatus::Unavailable(
                "no daemon for test".into()
            ))]
        );
        assert!(!input.process_key(0x61, 38, 0));
        input.set_text_input(true);
        input.set_window_focus(true);
        assert!(!input.wants_keys());
        assert!(input.drain().is_empty());
    }

    #[test]
    fn a_failing_daemon_keeps_queued_text_and_hides_the_preedit() {
        let (sender, receiver) = mpsc::channel();
        let mut input = InputMethod {
            client: None,
            incoming: Some(receiver),
            status: InputMethodStatus::Active,
            pending: Vec::new(),
            text_input: true,
            window_focus: true,
            context_focus: true,
            cursor: None,
            sent_cursor: None,
        };
        // Queued by the listener before the daemon failed.
        sender
            .send(Incoming::Input(InputMethodEvent::Preedit {
                text: "ni".into(),
                cursor: 2,
            }))
            .unwrap();
        sender
            .send(Incoming::Input(InputMethodEvent::Commit("你".into())))
            .unwrap();

        input.fail("daemon went away".into());

        assert_eq!(
            input.drain(),
            vec![
                Update::Input(InputMethodEvent::Preedit {
                    text: "ni".into(),
                    cursor: 2,
                }),
                Update::Input(InputMethodEvent::Commit("你".into())),
                Update::Input(InputMethodEvent::PreeditHidden),
                Update::Status(InputMethodStatus::Unavailable("daemon went away".into())),
            ]
        );
        assert!(!input.is_active());
        assert!(!input.wants_keys());
        assert!(input.drain().is_empty());
        // The listener's next send finds the receiver gone and stops.
        assert!(sender
            .send(Incoming::Input(InputMethodEvent::Commit("x".into())))
            .is_err());
    }
}
