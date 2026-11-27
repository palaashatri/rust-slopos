//! The IBus client against a fake IBus daemon on a private D-Bus bus.
//!
//! The test starts its own `dbus-daemon` (no session bus needed), registers
//! `org.freedesktop.IBus` on it from a fake that answers the input context
//! calls and emits the text signals, then drives the real client and the
//! text field. The real IBus daemon is not used here: that is NOT_RUN.
#![cfg(feature = "ibus")]

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use slopos_ui::controls::text_field::{TextField, TextFieldOutcome};
use slopos_ui::event::Event;
use slopos_ui::foundation::geometry::Rect;
use slopos_ui::input_method::{InputMethodEvent, InputMethodStatus};
use slopos_ui::platform::ibus::protocol::{encode_text, CONTEXT_INTERFACE, SERVICE};
use slopos_ui::platform::ibus::{IbusClient, Incoming, InputMethod, Update, CONTEXT_CALL_TIMEOUT};
use zbus::blocking::{connection, MessageIterator};
use zbus::zvariant::OwnedObjectPath;

const WAIT: Duration = Duration::from_secs(10);
const CONTEXT_PATH: &str = "/org/freedesktop/IBus/InputContext_7";

/// A private `dbus-daemon`, killed when dropped.
struct PrivateBus {
    child: Child,
    address: String,
}

impl PrivateBus {
    fn start() -> Self {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--print-address=1", "--nofork"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("dbus-daemon must be installed to run this test");
        let stdout = child.stdout.take().expect("piped stdout");
        let mut line = String::new();
        BufReader::new(stdout)
            .read_line(&mut line)
            .expect("dbus-daemon prints its address");
        let address = line.trim().to_owned();
        assert!(address.starts_with("unix:"), "unexpected address {address}");
        Self { child, address }
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The fake daemon's log of calls it answered, as `Member(args)` strings.
type Calls = Arc<Mutex<Vec<String>>>;

/// A call the fake daemon logs but never answers, the way a stuck daemon
/// behaves.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Hang {
    Nothing,
    ProcessKeyEvent,
    CreateInputContext,
}

/// Serve `org.freedesktop.IBus` on `address` until the bus goes away.
/// Keys: `a` and `b` extend a composition (`ab` shown as a preedit); space
/// commits the composition as `阿`; anything else is not handled.
/// `hang` names the one call that is logged and never answered.
fn serve_fake_ibus(address: String, calls: Calls, ready: mpsc::Sender<()>, hang: Hang) {
    let connection = connection::Builder::address(address.as_str())
        .and_then(|builder| builder.build())
        .expect("fake IBus connects to the private bus");
    connection
        .request_name(SERVICE)
        .expect("fake IBus takes the IBus name");
    let _ = ready.send(());
    let mut composition = String::new();
    for received in MessageIterator::from(&connection) {
        let Ok(message) = received else { break };
        let header = message.header();
        let Some(member) = header.member().map(|member| member.as_str().to_owned()) else {
            continue;
        };
        let Some(interface) = header.interface().map(|name| name.as_str().to_owned()) else {
            continue;
        };
        if interface != SERVICE && interface != CONTEXT_INTERFACE {
            continue;
        }
        match member.as_str() {
            "CreateInputContext" => {
                calls.lock().unwrap().push("CreateInputContext".into());
                if hang == Hang::CreateInputContext {
                    continue;
                }
                let path = OwnedObjectPath::try_from(CONTEXT_PATH).unwrap();
                connection.reply(&message, &path).unwrap();
            }
            "SetCapabilities" => {
                let caps: u32 = message.body().deserialize().unwrap();
                calls
                    .lock()
                    .unwrap()
                    .push(format!("SetCapabilities({caps})"));
                connection.reply(&message, &()).unwrap();
            }
            "FocusIn" | "FocusOut" | "Reset" => {
                calls.lock().unwrap().push(member.clone());
                connection.reply(&message, &()).unwrap();
            }
            "SetCursorLocation" => {
                let rect: (i32, i32, i32, i32) = message.body().deserialize().unwrap();
                calls
                    .lock()
                    .unwrap()
                    .push(format!("SetCursorLocation{rect:?}"));
                connection.reply(&message, &()).unwrap();
            }
            "ProcessKeyEvent" => {
                let (keysym, keycode, state): (u32, u32, u32) =
                    message.body().deserialize().unwrap();
                calls
                    .lock()
                    .unwrap()
                    .push(format!("ProcessKeyEvent({keysym:#x},{keycode},{state})"));
                if hang == Hang::ProcessKeyEvent {
                    continue;
                }
                let handled = match keysym {
                    0x61 | 0x62 => {
                        let letter = char::from_u32(keysym).unwrap();
                        composition.push(letter);
                        let cursor = composition.chars().count() as u32;
                        connection
                            .emit_signal(
                                None::<&str>,
                                CONTEXT_PATH,
                                CONTEXT_INTERFACE,
                                "UpdatePreeditText",
                                &(encode_text(&composition, true), cursor, true),
                            )
                            .unwrap();
                        true
                    }
                    0x20 if !composition.is_empty() => {
                        connection
                            .emit_signal(
                                None::<&str>,
                                CONTEXT_PATH,
                                CONTEXT_INTERFACE,
                                "CommitText",
                                &encode_text("阿", false),
                            )
                            .unwrap();
                        connection
                            .emit_signal(
                                None::<&str>,
                                CONTEXT_PATH,
                                CONTEXT_INTERFACE,
                                "HidePreeditText",
                                &(),
                            )
                            .unwrap();
                        composition.clear();
                        true
                    }
                    _ => false,
                };
                connection.reply(&message, &handled).unwrap();
            }
            _ => {}
        }
    }
}

/// Start the fake and return its call log.
fn start_fake(bus: &PrivateBus) -> Calls {
    start_fake_with(bus, Hang::Nothing)
}

/// Start the fake; `hang` names the call it logs but never answers.
fn start_fake_with(bus: &PrivateBus, hang: Hang) -> Calls {
    let calls: Calls = Arc::new(Mutex::new(Vec::new()));
    let (ready_tx, ready_rx) = mpsc::channel();
    let address = bus.address.clone();
    let log = Arc::clone(&calls);
    thread::spawn(move || serve_fake_ibus(address, log, ready_tx, hang));
    ready_rx
        .recv_timeout(WAIT)
        .expect("fake IBus registers its name");
    calls
}

fn field() -> TextField {
    let mut field = TextField::new(1, "Name").with_bounds(Rect {
        x: 0,
        y: 0,
        width: 200,
        height: 22,
    });
    field.focused = true;
    field
}

#[test]
fn client_forwards_keys_and_receives_preedit_and_commit() {
    let bus = PrivateBus::start();
    let calls = start_fake(&bus);
    let (incoming_tx, incoming_rx): (_, Receiver<Incoming>) = mpsc::channel();
    let client = IbusClient::connect(&bus.address, incoming_tx, || {})
        .expect("client connects to the fake daemon");
    client.focus_in().unwrap();

    assert!(
        client.process_key_event(0x61, 38, 0).unwrap(),
        "a is consumed"
    );
    assert_eq!(
        incoming_rx.recv_timeout(WAIT).unwrap(),
        Incoming::Input(InputMethodEvent::Preedit {
            text: "a".into(),
            cursor: 1,
        })
    );
    assert!(client.process_key_event(0x62, 56, 0).unwrap());
    assert_eq!(
        incoming_rx.recv_timeout(WAIT).unwrap(),
        Incoming::Input(InputMethodEvent::Preedit {
            text: "ab".into(),
            cursor: 2,
        })
    );
    assert!(client.process_key_event(0x20, 65, 0).unwrap());
    assert_eq!(
        incoming_rx.recv_timeout(WAIT).unwrap(),
        Incoming::Input(InputMethodEvent::Commit("阿".into()))
    );
    assert_eq!(
        incoming_rx.recv_timeout(WAIT).unwrap(),
        Incoming::Input(InputMethodEvent::PreeditHidden)
    );
    // A key the daemon does not use comes back unhandled.
    assert!(!client.process_key_event(0x78, 53, 0).unwrap());

    let calls = calls.lock().unwrap().clone();
    assert!(calls.contains(&"CreateInputContext".to_string()));
    assert!(calls.contains(&"SetCapabilities(9)".to_string()));
    assert!(calls.contains(&"FocusIn".to_string()));
    assert!(calls.contains(&"ProcessKeyEvent(0x61,38,0)".to_string()));
}

/// Apply IME updates to `text` until `want` outcomes have arrived. Each
/// wake from the listener is a point to look again; nothing is polled.
fn apply_updates(
    input: &mut InputMethod,
    text: &mut TextField,
    wakes: &Receiver<()>,
    want: usize,
) -> Vec<TextFieldOutcome> {
    let mut outcomes = Vec::new();
    while outcomes.len() < want {
        for update in input.drain() {
            if let Update::Input(event) = update {
                outcomes.push(text.handle(&Event::InputMethod(event)));
            }
        }
        if outcomes.len() < want {
            wakes
                .recv_timeout(WAIT)
                .expect("the daemon's signal wakes the loop");
        }
    }
    outcomes
}

#[test]
fn text_field_shows_the_preedit_then_takes_the_commit() {
    let bus = PrivateBus::start();
    let _calls = start_fake(&bus);
    let (wake_tx, wakes) = mpsc::channel();
    let mut input = InputMethod::connect(&bus.address, move || {
        let _ = wake_tx.send(());
    })
    .expect("input method connects");
    assert_eq!(input.status(), &InputMethodStatus::Active);
    assert_eq!(
        input.drain(),
        vec![Update::Status(InputMethodStatus::Active)]
    );

    // Nothing is offered to IBus until a text field asks for text input with
    // the window focused.
    assert!(!input.process_key(0x61, 38, 0));
    input.set_window_focus(true);
    assert!(!input.wants_keys());
    input.set_text_input(true);
    assert!(input.wants_keys());

    let mut text = field();

    assert!(input.process_key(0x61, 38, 0));
    let outcomes = apply_updates(&mut input, &mut text, &wakes, 1);
    assert_eq!(outcomes, vec![TextFieldOutcome::Changed]);
    assert_eq!(text.preedit(), Some(("a", 1)));
    assert_eq!(text.text(), "", "the composition is not committed yet");

    assert!(input.process_key(0x62, 56, 0));
    let outcomes = apply_updates(&mut input, &mut text, &wakes, 1);
    assert_eq!(outcomes, vec![TextFieldOutcome::Changed]);
    assert_eq!(text.preedit(), Some(("ab", 2)));

    // Space commits: the commit arrives first, then the hide.
    assert!(input.process_key(0x20, 65, 0));
    let outcomes = apply_updates(&mut input, &mut text, &wakes, 2);
    assert_eq!(
        outcomes,
        vec![TextFieldOutcome::Edited, TextFieldOutcome::Ignored]
    );
    assert_eq!(text.text(), "阿");
    assert_eq!(text.preedit(), None);
}

#[test]
fn focus_and_caret_location_reach_the_daemon_only_when_wanted() {
    let bus = PrivateBus::start();
    let calls = start_fake(&bus);
    let mut input = InputMethod::connect(&bus.address, || {}).unwrap();
    input.drain();
    input.set_text_input(true);
    input.set_window_focus(true);
    input.set_cursor_location(10, 20, 1, 22);
    input.set_cursor_location(10, 20, 1, 22); // unchanged: not sent again
    input.set_text_input(false);
    input.set_window_focus(false);
    let log = calls.lock().unwrap().clone();
    let focus_ins = log.iter().filter(|call| *call == "FocusIn").count();
    let focus_outs = log.iter().filter(|call| *call == "FocusOut").count();
    assert_eq!((focus_ins, focus_outs), (1, 1), "{log:?}");
    let cursors = log
        .iter()
        .filter(|call| call.starts_with("SetCursorLocation"))
        .count();
    assert_eq!(cursors, 1, "{log:?}");
    assert!(
        log.contains(&"SetCursorLocation(10, 20, 1, 22)".to_string()),
        "{log:?}"
    );

    // Focus again: the location is sent again, because focus out may have
    // reset the daemon's view of the caret. Setting it again is not resent.
    input.set_text_input(true);
    input.set_window_focus(true);
    input.set_cursor_location(10, 20, 1, 22);
    let log = calls.lock().unwrap().clone();
    let cursors = log
        .iter()
        .filter(|call| call.starts_with("SetCursorLocation"))
        .count();
    assert_eq!(cursors, 2, "{log:?}");
}

#[test]
fn losing_the_daemon_reports_unavailable_and_keys_go_to_the_app() {
    let mut bus = PrivateBus::start();
    let _calls = start_fake(&bus);
    let (wake_tx, wake_rx) = mpsc::channel();
    let mut input = InputMethod::connect(&bus.address, move || {
        let _ = wake_tx.send(());
    })
    .unwrap();
    input.drain();
    input.set_text_input(true);
    input.set_window_focus(true);
    assert!(input.wants_keys());

    // The daemon goes away: the listener reports the closed connection.
    bus.child.kill().unwrap();
    wake_rx
        .recv_timeout(WAIT)
        .expect("the listener wakes the loop");
    let updates = input.drain();
    let status = updates.iter().find_map(|update| match update {
        Update::Status(status) => Some(status.clone()),
        Update::Input(_) => None,
    });
    let Some(InputMethodStatus::Unavailable(reason)) = status else {
        panic!("expected an unavailable status, got {updates:?}");
    };
    assert!(!reason.is_empty());
    assert!(!input.wants_keys());
    assert!(!input.process_key(0x61, 38, 0));
}

#[test]
fn connecting_to_a_missing_bus_fails_with_a_reason() {
    let error = InputMethod::connect("unix:path=/nonexistent/slopos-ibus-test", || {})
        .err()
        .expect("no daemon at that address");
    assert!(!error.is_empty());
}

#[test]
fn a_hung_daemon_is_given_up_within_the_call_timeout() {
    let bus = PrivateBus::start();
    let calls = start_fake_with(&bus, Hang::ProcessKeyEvent);
    let (wake_tx, _wake_rx) = mpsc::channel();
    let mut input = InputMethod::connect(&bus.address, move || {
        let _ = wake_tx.send(());
    })
    .expect("client connects to the fake daemon");
    input.drain();
    input.set_text_input(true);
    input.set_window_focus(true);
    assert!(input.wants_keys());

    let started = Instant::now();
    let handled = input.process_key(0x61, 38, 0);
    let elapsed = started.elapsed();

    // The key is not consumed, so it reaches the application.
    assert!(!handled);
    // The call waited for the timeout, then gave up well before zbus's
    // default 25 s. The bound is loose enough for a loaded CI runner.
    assert!(
        elapsed >= CONTEXT_CALL_TIMEOUT,
        "returned after {elapsed:?}"
    );
    assert!(elapsed < Duration::from_secs(1), "blocked for {elapsed:?}");

    assert!(!input.wants_keys());
    let reason = input
        .drain()
        .into_iter()
        .find_map(|update| match update {
            Update::Status(InputMethodStatus::Unavailable(reason)) => Some(reason),
            _ => None,
        })
        .expect("the input method reports itself unavailable");
    assert!(reason.contains("ProcessKeyEvent"), "{reason}");
    assert!(calls
        .lock()
        .unwrap()
        .iter()
        .any(|call| call.starts_with("ProcessKeyEvent")));

    // Later keys go straight to the application without waiting again.
    let again = Instant::now();
    assert!(!input.process_key(0x61, 38, 0));
    assert!(again.elapsed() < CONTEXT_CALL_TIMEOUT);
}

#[test]
fn a_daemon_that_never_creates_a_context_fails_the_connect_quickly() {
    let bus = PrivateBus::start();
    let calls = start_fake_with(&bus, Hang::CreateInputContext);

    // The client gives up on the silent daemon within the call bound.
    let (incoming_tx, _incoming_rx) = mpsc::channel();
    let started = Instant::now();
    let error = IbusClient::connect(&bus.address, incoming_tx, || {})
        .err()
        .expect("connect fails when CreateInputContext is never answered");
    let elapsed = started.elapsed();
    assert!(error.contains("CreateInputContext"), "{error}");
    assert!(elapsed < Duration::from_secs(1), "blocked for {elapsed:?}");

    // The window starts without an input method, and says why.
    let started = Instant::now();
    let input = InputMethod::connect_or_unavailable(&bus.address, || {});
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(!input.is_active());
    assert!(!input.wants_keys());
    match input.status() {
        InputMethodStatus::Unavailable(reason) => {
            assert!(reason.contains("CreateInputContext"), "{reason}");
        }
        other => panic!("expected unavailable, got {other}"),
    }
    assert!(calls
        .lock()
        .unwrap()
        .iter()
        .any(|call| call == "CreateInputContext"));
}
