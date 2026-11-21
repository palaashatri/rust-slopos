//! The `com.canonical.AppMenu.Registrar` service on the session bus.
//!
//! Qt and GTK applications that export their menus over `com.canonical.dbusmenu`
//! announce each window's menu with `RegisterWindow(window, object_path)` on
//! this service. The bar owns the name and feeds every call into the pure
//! [`Registrar`] state machine, which decides what is accepted. This module is
//! only the wiring:
//!
//! - zbus serves the methods. The caller's unique name is the menu's service,
//!   so a client can only register its own windows.
//! - Each accepted change is emitted as `WindowRegistered` or
//!   `WindowUnregistered` by one emitter thread, which also forwards it to the
//!   bar as an [`Effect`].
//! - A thread watches `NameOwnerChanged` and reports `ServiceVanished` when a
//!   client leaves the bus, so its menus disappear with it.
//! - An X connection selects `StructureNotify` on each registered window. A
//!   thread blocks on its events and reports `DestroyNotify` as
//!   `WindowDestroyed`. Without a display the registrar still works; it just
//!   cannot notice destroyed windows.
//!
//! Nothing polls. Every thread blocks on the bus or on X events.
//!
//! Deviation from the wording of the phase 2 brief: the wire methods are the
//! ones real clients call, `RegisterWindow(u windowId, o menuObjectPath)`, with
//! the service taken from the sender. The `WindowRegistered(u, s, o)` signal
//! carries the service name, as the protocol defines. A service argument on
//! `RegisterWindow` would let one client register another's menus, which the
//! registrar rules forbid.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;

use x11rb::connection::Connection as _;
use x11rb::protocol::xproto::{ChangeWindowAttributesAux, ConnectionExt as _, EventMask};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use zbus::blocking::connection::Builder;
use zbus::blocking::Connection;
use zbus::fdo;
use zbus::message::Header;
use zbus::zvariant::{ObjectPath, OwnedObjectPath};

use crate::menu_registrar::{Effect, Input, MenuRef, Registrar};

/// The well-known name the bar owns.
pub const SERVICE: &str = "com.canonical.AppMenu.Registrar";
/// The object that serves it.
pub const PATH: &str = "/com/canonical/AppMenu/Registrar";

/// The registrar's map, shared with the bar and with the watcher threads.
pub type State = Arc<Mutex<Registrar>>;

/// A running registrar. Dropping it releases the name.
pub struct Service {
    pub state: State,
    /// Every accepted change, in order, for the bar to re-resolve its window.
    pub effects: Receiver<Effect>,
    _connection: Connection,
}

impl Service {
    /// The menus to show for `window`; see [`Registrar::resolve`].
    pub fn resolve(&self, window: u32, property: Option<MenuRef>) -> Option<MenuRef> {
        lock(&self.state).resolve(window, property)
    }
}

fn lock(state: &State) -> MutexGuard<'_, Registrar> {
    state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Serves the registrar's methods.
struct Iface {
    state: State,
    changes: Sender<Effect>,
    x: Option<Arc<RustConnection>>,
}

impl Iface {
    fn apply(&self, input: Input) -> fdo::Result<()> {
        let effects = lock(&self.state)
            .apply(input)
            .map_err(|refusal| fdo::Error::Failed(format!("{refusal:?}")))?;
        for effect in effects {
            let _ = self.changes.send(effect);
        }
        Ok(())
    }

    /// Ask X for `DestroyNotify` on `window`. A round trip, so a window that
    /// no longer exists is refused here rather than registered.
    fn watch_destruction(&self, window: u32) -> fdo::Result<()> {
        let Some(x) = &self.x else {
            return Ok(());
        };
        let cookie = x
            .change_window_attributes(
                window,
                &ChangeWindowAttributesAux::new().event_mask(EventMask::STRUCTURE_NOTIFY),
            )
            .map_err(|error| fdo::Error::Failed(format!("window {window}: {error}")))?;
        cookie
            .check()
            .map_err(|error| fdo::Error::Failed(format!("window {window}: {error}")))
    }
}

#[zbus::interface(name = "com.canonical.AppMenu.Registrar")]
impl Iface {
    /// `RegisterWindow(u windowId, o menuObjectPath)`: the caller's menus for
    /// `window` live at `menu_object_path` on the caller's connection.
    fn register_window(
        &self,
        window_id: u32,
        menu_object_path: ObjectPath<'_>,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<()> {
        let sender = header
            .sender()
            .map(|name| name.to_string())
            .ok_or_else(|| fdo::Error::InvalidArgs("the call has no sender".into()))?;
        self.watch_destruction(window_id)?;
        self.apply(Input::Register {
            sender,
            window: window_id,
            path: menu_object_path.to_string(),
        })
    }

    /// `UnregisterWindow(u windowId)`: forget the caller's menus for `window`.
    fn unregister_window(
        &self,
        window_id: u32,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<()> {
        let sender = header
            .sender()
            .map(|name| name.to_string())
            .ok_or_else(|| fdo::Error::InvalidArgs("the call has no sender".into()))?;
        self.apply(Input::Unregister {
            sender,
            window: window_id,
        })
    }

    /// `GetMenuForWindow(u windowId) -> (s service, o menuObjectPath)`.
    fn get_menu_for_window(&self, window_id: u32) -> fdo::Result<(String, OwnedObjectPath)> {
        let menu = lock(&self.state)
            .menu(window_id)
            .cloned()
            .ok_or_else(|| fdo::Error::Failed(format!("window {window_id} has no menu")))?;
        let path = OwnedObjectPath::try_from(menu.path.as_str())
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        Ok((menu.service, path))
    }
}

/// Own the registrar name on `address` (the session bus when `None`).
///
/// Returns `None` when the name is taken, for example by another desktop's
/// registrar, or when the bus cannot be reached. The bar then keeps working
/// from the `_KDE_NET_WM_APPMENU_*` window properties alone.
pub fn start(address: Option<&str>, wake: impl Fn() + Send + Sync + 'static) -> Option<Service> {
    let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(wake);
    let state: State = Arc::new(Mutex::new(Registrar::new()));
    let (changes, internal) = mpsc::channel::<Effect>();
    let (bar, effects) = mpsc::channel::<Effect>();
    let x = connect_x();
    let builder = match address {
        Some(address) => Builder::address(address),
        None => Builder::session(),
    };
    let connection = builder
        .and_then(|builder| builder.name(SERVICE))
        .and_then(|builder| {
            builder.serve_at(
                PATH,
                Iface {
                    state: state.clone(),
                    changes: changes.clone(),
                    x: x.clone(),
                },
            )
        })
        .and_then(|builder| builder.build())
        .map_err(|error| log::warn!("could not own {SERVICE}: {error}"))
        .ok()?;

    let emitter = connection.clone();
    let emit_wake = wake.clone();
    thread::Builder::new()
        .name("slopos-appmenu-signals".into())
        .spawn(move || emit(&emitter, internal, &bar, emit_wake.as_ref()))
        .ok()?;

    if let Some(x) = x {
        let (state, changes) = (state.clone(), changes.clone());
        thread::Builder::new()
            .name("slopos-appmenu-x".into())
            .spawn(move || watch_destruction(&x, &state, &changes))
            .ok();
    }

    let owners = connection.clone();
    let (owner_state, owner_changes) = (state.clone(), changes);
    thread::Builder::new()
        .name("slopos-appmenu-owners".into())
        .spawn(move || watch_owners(&owners, &owner_state, &owner_changes))
        .ok();

    Some(Service {
        state,
        effects,
        _connection: connection,
    })
}

/// Turn each accepted change into a `WindowRegistered` or
/// `WindowUnregistered` signal, then hand it to the bar.
fn emit(connection: &Connection, changes: Receiver<Effect>, bar: &Sender<Effect>, wake: &dyn Fn()) {
    for effect in changes {
        let result = match &effect {
            Effect::Registered { window, menu } => match ObjectPath::try_from(menu.path.as_str()) {
                Ok(path) => connection.emit_signal(
                    Option::<&str>::None,
                    PATH,
                    SERVICE,
                    "WindowRegistered",
                    &(*window, menu.service.as_str(), path),
                ),
                Err(error) => Err(error.into()),
            },
            Effect::Unregistered { window } => connection.emit_signal(
                Option::<&str>::None,
                PATH,
                SERVICE,
                "WindowUnregistered",
                &(*window,),
            ),
        };
        if let Err(error) = result {
            log::warn!("could not emit {effect:?}: {error}");
        }
        let _ = bar.send(effect);
        wake();
    }
}

/// The X connection used for `DestroyNotify`, or `None` without a display.
fn connect_x() -> Option<Arc<RustConnection>> {
    let (connection, _) = x11rb::connect(None).ok()?;
    Some(Arc::new(connection))
}

/// Block on X events; a destroyed registered window leaves the registrar.
fn watch_destruction(x: &RustConnection, state: &State, changes: &Sender<Effect>) {
    while let Ok(event) = x.wait_for_event() {
        let Event::DestroyNotify(destroyed) = event else {
            continue;
        };
        let effects = lock(state)
            .apply(Input::WindowDestroyed {
                window: destroyed.window,
            })
            .unwrap_or_default();
        for effect in effects {
            let _ = changes.send(effect);
        }
    }
}

/// Block on `NameOwnerChanged`; a unique name that lost its owner has left.
fn watch_owners(connection: &Connection, state: &State, changes: &Sender<Effect>) {
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface("org.freedesktop.DBus")
        .and_then(|rule| rule.member("NameOwnerChanged"))
        .map(|rule| rule.build());
    let Ok(rule) = rule else {
        return;
    };
    if let Ok(dbus) = zbus::blocking::fdo::DBusProxy::new(connection) {
        let _ = dbus.add_match_rule(rule);
    }
    for message in zbus::blocking::MessageIterator::from(connection).flatten() {
        let header = message.header();
        if header.message_type() != zbus::message::Type::Signal
            || header.member().map(|m| m.as_str()) != Some("NameOwnerChanged")
        {
            continue;
        }
        let Ok((name, _old, new)) = message.body().deserialize::<(String, String, String)>() else {
            continue;
        };
        if !new.is_empty() || !name.starts_with(':') {
            continue;
        }
        let effects = lock(state)
            .apply(Input::ServiceVanished { name })
            .unwrap_or_default();
        for effect in effects {
            let _ = changes.send(effect);
        }
    }
}
