//! systemd-logind over D-Bus: the signals that ask for a lock, the session
//! object's path, and the sleep delay inhibitor.
//!
//! Signal threads block on the bus and call `on_event` for each signal; they
//! never poll. The match rules name the `org.freedesktop.login1` sender, so
//! another client cannot send these signals.

use std::os::fd::OwnedFd;
use std::sync::Arc;
use std::thread;

use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::OwnedObjectPath;

pub const LOGIN1: &str = "org.freedesktop.login1";
pub const MANAGER_PATH: &str = "/org/freedesktop/login1";
pub const MANAGER_IFACE: &str = "org.freedesktop.login1.Manager";
pub const SESSION_IFACE: &str = "org.freedesktop.login1.Session";

/// A logind signal the lock responds to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogindEvent {
    /// The system is about to sleep (`true`) or has woken (`false`).
    PrepareForSleep(bool),
    /// Someone asked to lock this session (`loginctl lock-session`).
    Lock,
    /// Someone asked to unlock this session (`loginctl unlock-session`).
    Unlock,
}

/// Where logind events go. Shared by the signal threads.
pub type EventSink = Arc<dyn Fn(LogindEvent) + Send + Sync>;

/// The session object for this login: `$XDG_SESSION_ID` when set, otherwise
/// the session of this process.
pub fn session_path(bus: &Connection) -> zbus::Result<OwnedObjectPath> {
    let manager = Proxy::new(bus, LOGIN1, MANAGER_PATH, MANAGER_IFACE)?;
    match std::env::var("XDG_SESSION_ID") {
        Ok(id) if !id.trim().is_empty() => manager.call("GetSession", &(id.trim().to_owned(),)),
        _ => manager.call("GetSessionByPID", &(std::process::id(),)),
    }
}

/// Deliver `PrepareForSleep` from the manager object.
pub fn watch_manager(bus: &Connection, on_event: EventSink) -> zbus::Result<()> {
    let proxy = Proxy::new(bus, LOGIN1, MANAGER_PATH, MANAGER_IFACE)?;
    let signals = proxy.receive_signal("PrepareForSleep")?;
    spawn_pump("slopos-logind-sleep", signals, on_event, |message| {
        message
            .body()
            .deserialize::<bool>()
            .ok()
            .map(LogindEvent::PrepareForSleep)
    });
    Ok(())
}

/// Deliver `Lock` and `Unlock` from the session object at `path`.
pub fn watch_session(bus: &Connection, path: &str, on_event: EventSink) -> zbus::Result<()> {
    let proxy = Proxy::new(bus, LOGIN1, path.to_owned(), SESSION_IFACE)?;
    let locks = proxy.receive_signal("Lock")?;
    spawn_pump("slopos-logind-lock", locks, on_event.clone(), |_| {
        Some(LogindEvent::Lock)
    });
    let unlocks = proxy.receive_signal("Unlock")?;
    spawn_pump("slopos-logind-unlock", unlocks, on_event, |_| {
        Some(LogindEvent::Unlock)
    });
    Ok(())
}

/// Ask logind for a delay inhibitor on sleep. While the returned descriptor
/// is open, suspend waits (up to logind's delay limit) for PrepareForSleep to
/// be answered; closing it lets suspend go on.
pub fn inhibit_sleep(bus: &Connection) -> zbus::Result<OwnedFd> {
    let manager = Proxy::new(bus, LOGIN1, MANAGER_PATH, MANAGER_IFACE)?;
    let fd: zbus::zvariant::OwnedFd = manager.call(
        "Inhibit",
        &(
            "sleep",
            "SLOPOS-I",
            "Lock the screen before sleeping",
            "delay",
        ),
    )?;
    Ok(fd.into())
}

fn spawn_pump(
    name: &str,
    signals: zbus::blocking::SignalIterator<'static>,
    on_event: EventSink,
    decode: fn(&zbus::Message) -> Option<LogindEvent>,
) {
    let spawned = thread::Builder::new().name(name.to_owned()).spawn(move || {
        for message in signals {
            if let Some(event) = decode(&message) {
                on_event(event);
            }
        }
        log::info!("logind signal stream ended");
    });
    if let Err(error) = spawned {
        log::error!("cannot watch logind signals: {error}");
    }
}
