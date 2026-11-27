//! A fake systemd-logind on a test bus. Run under a session bus:
//!
//!     dbus-run-session -- cargo test -p slopos-session --test logind_fake
//!
//! Without a session bus (DBUS_SESSION_BUS_ADDRESS unset) the tests say so and
//! return without checking anything; that is NOT a pass for the D-Bus path.

use std::sync::{mpsc, Arc, Mutex, MutexGuard};
use std::time::Duration;

use slopos_session::lock::logind::{
    self, EventSink, LogindEvent, LOGIN1, MANAGER_IFACE, MANAGER_PATH,
};
use slopos_session::lock::watch::{decide, Decision};
use zbus::blocking::connection::Builder;
use zbus::zvariant::OwnedObjectPath;

const SESSION_PATH: &str = "/org/freedesktop/login1/session/_1";
const SESSION_IFACE: &str = "org.freedesktop.login1.Session";

/// Answers `GetSessionByPID` the way logind does for this process.
struct FakeManager;

#[zbus::interface(name = "org.freedesktop.login1.Manager")]
impl FakeManager {
    // logind's name is GetSessionByPID; zbus would derive GetSessionByPid.
    #[zbus(name = "GetSessionByPID")]
    fn get_session_by_pid(&self, _pid: u32) -> OwnedObjectPath {
        OwnedObjectPath::try_from(SESSION_PATH).unwrap()
    }
}

struct FakeSession;

#[zbus::interface(name = "org.freedesktop.login1.Session")]
impl FakeSession {
    fn noop(&self) {}
}

/// Each test serves the fake login1 name on the shared test bus; the tests
/// run one at a time so they do not contend for it.
static BUS: Mutex<()> = Mutex::new(());

fn one_test_at_a_time() -> MutexGuard<'static, ()> {
    BUS.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn session_bus_or_skip() -> Option<zbus::blocking::Connection> {
    if std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_none() {
        eprintln!("SKIPPED: no session bus; run under dbus-run-session. Nothing was checked.");
        return None;
    }
    Some(zbus::blocking::Connection::session().expect("session bus"))
}

/// Serve the fake login1 under its well-known name on the test bus.
fn serve_fake_logind() -> zbus::blocking::Connection {
    Builder::session()
        .unwrap()
        .name(LOGIN1)
        .unwrap()
        .serve_at(MANAGER_PATH, FakeManager)
        .unwrap()
        .serve_at(SESSION_PATH, FakeSession)
        .unwrap()
        .build()
        .unwrap()
}

fn channel_sink() -> (EventSink, mpsc::Receiver<LogindEvent>) {
    let (sender, receiver) = mpsc::channel();
    let sink: EventSink = Arc::new(move |event| {
        let _ = sender.send(event);
    });
    (sink, receiver)
}

fn emit_prepare_for_sleep(server: &zbus::blocking::Connection, start: bool) {
    server
        .emit_signal(
            None::<()>,
            MANAGER_PATH,
            MANAGER_IFACE,
            "PrepareForSleep",
            &(start,),
        )
        .unwrap();
}

#[test]
fn prepare_for_sleep_requests_a_lock_before_suspend() {
    let _turn = one_test_at_a_time();
    let Some(client) = session_bus_or_skip() else {
        return;
    };
    let server = serve_fake_logind();
    let (sink, events) = channel_sink();
    logind::watch_manager(&client, sink).expect("subscribe to PrepareForSleep");

    emit_prepare_for_sleep(&server, true);
    let event = events
        .recv_timeout(Duration::from_secs(10))
        .expect("PrepareForSleep reached the watcher");
    assert_eq!(event, LogindEvent::PrepareForSleep(true));
    // No locker runs yet, so the watcher starts one.
    assert_eq!(decide(event, false), Decision::Lock);
    // A locker that already runs is not started twice.
    assert_eq!(decide(event, true), Decision::Nothing);

    // Waking up does not lock.
    emit_prepare_for_sleep(&server, false);
    let woke = events
        .recv_timeout(Duration::from_secs(10))
        .expect("resume reached the watcher");
    assert_eq!(decide(woke, false), Decision::Nothing);
}

#[test]
fn the_session_object_path_is_found_for_this_process() {
    let _turn = one_test_at_a_time();
    let Some(client) = session_bus_or_skip() else {
        return;
    };
    let _server = serve_fake_logind();
    std::env::remove_var("XDG_SESSION_ID");
    let path = logind::session_path(&client).expect("GetSessionByPID answered");
    assert_eq!(path.as_str(), SESSION_PATH);
}

#[test]
fn session_lock_and_unlock_signals_reach_the_sinks() {
    let _turn = one_test_at_a_time();
    let Some(client) = session_bus_or_skip() else {
        return;
    };
    let server = serve_fake_logind();
    let (sink, events) = channel_sink();
    logind::watch_session(&client, SESSION_PATH, sink).expect("subscribe to Lock and Unlock");

    for (name, expected) in [("Lock", LogindEvent::Lock), ("Unlock", LogindEvent::Unlock)] {
        server
            .emit_signal(None::<()>, SESSION_PATH, SESSION_IFACE, name, &())
            .unwrap();
        let event = events
            .recv_timeout(Duration::from_secs(10))
            .unwrap_or_else(|_| panic!("{name} reached the sink"));
        assert_eq!(event, expected);
    }
    assert_eq!(decide(LogindEvent::Lock, false), Decision::Lock);
    assert_eq!(decide(LogindEvent::Unlock, false), Decision::Nothing);
}
