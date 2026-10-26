//! The window list, read from the window manager over EWMH.
//!
//! A background thread with its own X connection listens for changes to the
//! root window's client list and active window (and to each client's title
//! and state), and reports a fresh [`Snapshot`] after each burst of changes.

use std::collections::HashSet;
use std::thread;

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ChangeWindowAttributesAux, ClientMessageEvent, ConnectionExt, EventMask,
};
use x11rb::protocol::Event as XEvent;
use x11rb::rust_connection::RustConnection;

use crate::model::Window;

x11rb::atom_manager! {
    Atoms: AtomsCookie {
        _NET_CLIENT_LIST_STACKING,
        _NET_CLIENT_LIST,
        _NET_ACTIVE_WINDOW,
        _NET_WM_NAME,
        _NET_WM_STATE,
        _NET_WM_STATE_SKIP_TASKBAR,
        _NET_WM_STATE_DEMANDS_ATTENTION,
        _NET_WM_WINDOW_TYPE,
        _NET_WM_WINDOW_TYPE_NORMAL,
        _NET_WM_WINDOW_TYPE_DIALOG,
        UTF8_STRING,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    /// Taskbar-worthy windows, bottom-most first.
    pub windows: Vec<Window>,
    pub active: Option<u32>,
}

/// Start watching. `on_change` runs on the watcher thread.
pub fn watch(
    skip_class: &'static str,
    on_change: impl Fn(Snapshot) + Send + 'static,
) -> Result<(), String> {
    let (conn, screen) = x11rb::connect(None).map_err(|error| error.to_string())?;
    let root = conn.setup().roots[screen].root;
    let atoms = Atoms::new(&conn)
        .map_err(|error| error.to_string())?
        .reply()
        .map_err(|error| error.to_string())?;
    thread::Builder::new()
        .name("dock-windows".into())
        .spawn(move || {
            if let Err(error) = run(&conn, root, &atoms, skip_class, &on_change) {
                log::warn!("Dock window tracking stopped: {error}");
            }
        })
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn run(
    conn: &RustConnection,
    root: u32,
    atoms: &Atoms,
    skip_class: &str,
    on_change: &dyn Fn(Snapshot),
) -> Result<(), Box<dyn std::error::Error>> {
    let property_mask = ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE);
    conn.change_window_attributes(root, &property_mask)?
        .check()?;
    let mut watched = HashSet::new();
    let mut last = None;
    loop {
        let snapshot = read(conn, root, atoms, skip_class)?;
        // Follow title and state changes on every client window we report.
        for window in &snapshot.windows {
            if watched.insert(window.id) {
                let _ = conn.change_window_attributes(window.id, &property_mask);
            }
        }
        watched.retain(|id| snapshot.windows.iter().any(|window| window.id == *id));
        conn.flush()?;
        if last.as_ref() != Some(&snapshot) {
            on_change(snapshot.clone());
            last = Some(snapshot);
        }
        // Sleep until something relevant changes, then drain the burst.
        loop {
            let event = conn.wait_for_event()?;
            let mut relevant = is_relevant(&event, root, atoms);
            while let Some(event) = conn.poll_for_event()? {
                relevant |= is_relevant(&event, root, atoms);
            }
            if relevant {
                break;
            }
        }
    }
}

fn is_relevant(event: &XEvent, root: u32, atoms: &Atoms) -> bool {
    match event {
        XEvent::PropertyNotify(property) if property.window == root => [
            atoms._NET_CLIENT_LIST_STACKING,
            atoms._NET_CLIENT_LIST,
            atoms._NET_ACTIVE_WINDOW,
        ]
        .contains(&property.atom),
        XEvent::PropertyNotify(property) => [
            atoms._NET_WM_NAME,
            AtomEnum::WM_NAME.into(),
            AtomEnum::WM_CLASS.into(),
            AtomEnum::WM_HINTS.into(),
            atoms._NET_WM_STATE,
        ]
        .contains(&property.atom),
        _ => false,
    }
}

fn read(
    conn: &RustConnection,
    root: u32,
    atoms: &Atoms,
    skip_class: &str,
) -> Result<Snapshot, Box<dyn std::error::Error>> {
    let mut ids = cardinals(
        conn,
        root,
        atoms._NET_CLIENT_LIST_STACKING,
        AtomEnum::WINDOW,
    )?;
    if ids.is_empty() {
        ids = cardinals(conn, root, atoms._NET_CLIENT_LIST, AtomEnum::WINDOW)?;
    }
    let active = cardinals(conn, root, atoms._NET_ACTIVE_WINDOW, AtomEnum::WINDOW)?
        .first()
        .copied()
        .filter(|id| *id != 0);
    let mut windows = Vec::new();
    for id in ids {
        // Windows can vanish between reading the list and reading them.
        if let Ok(Some(window)) = describe(conn, id, atoms, skip_class) {
            windows.push(window);
        }
    }
    Ok(Snapshot { windows, active })
}

fn describe(
    conn: &RustConnection,
    id: u32,
    atoms: &Atoms,
    skip_class: &str,
) -> Result<Option<Window>, Box<dyn std::error::Error>> {
    let types = cardinals(conn, id, atoms._NET_WM_WINDOW_TYPE, AtomEnum::ATOM)?;
    let transient = !cardinals(
        conn,
        id,
        AtomEnum::WM_TRANSIENT_FOR.into(),
        AtomEnum::WINDOW,
    )?
    .is_empty();
    let ordinary = if types.is_empty() {
        !transient
    } else {
        types.contains(&atoms._NET_WM_WINDOW_TYPE_NORMAL)
            || (types.contains(&atoms._NET_WM_WINDOW_TYPE_DIALOG) && !transient)
    };
    let state = cardinals(conn, id, atoms._NET_WM_STATE, AtomEnum::ATOM)?;
    if !ordinary || state.contains(&atoms._NET_WM_STATE_SKIP_TASKBAR) {
        return Ok(None);
    }
    let class = text(conn, id, AtomEnum::WM_CLASS.into(), AtomEnum::STRING.into())?;
    let mut parts = class.split('\0');
    let instance = parts.next().unwrap_or_default().to_owned();
    let class = parts.next().unwrap_or_default().to_owned();
    if class == skip_class {
        return Ok(None);
    }
    let mut title = text(conn, id, atoms._NET_WM_NAME, atoms.UTF8_STRING)?;
    if title.is_empty() {
        title = text(conn, id, AtomEnum::WM_NAME.into(), AtomEnum::ANY.into())?;
    }
    // WM_HINTS flags bit 8 is the ICCCM urgency hint.
    let hints = cardinals(conn, id, AtomEnum::WM_HINTS.into(), AtomEnum::WM_HINTS)?;
    let urgent = hints.first().is_some_and(|flags| flags & (1 << 8) != 0);
    Ok(Some(Window {
        id,
        instance,
        class,
        title,
        attention: urgent || state.contains(&atoms._NET_WM_STATE_DEMANDS_ATTENTION),
    }))
}

fn cardinals(
    conn: &RustConnection,
    window: u32,
    property: u32,
    kind: impl Into<u32>,
) -> Result<Vec<u32>, Box<dyn std::error::Error>> {
    let reply = conn
        .get_property(false, window, property, kind.into(), 0, 4096)?
        .reply()?;
    Ok(reply.value32().map(Iterator::collect).unwrap_or_default())
}

fn text(
    conn: &RustConnection,
    window: u32,
    property: u32,
    kind: u32,
) -> Result<String, Box<dyn std::error::Error>> {
    let reply = conn
        .get_property(false, window, property, kind, 0, 1024)?
        .reply()?;
    Ok(String::from_utf8_lossy(&reply.value)
        .trim_end_matches('\0')
        .to_owned())
}

/// Ask the window manager to raise and focus `window`.
pub fn activate(window: u32) -> Result<(), String> {
    let run = || -> Result<(), Box<dyn std::error::Error>> {
        let (conn, screen) = x11rb::connect(None)?;
        let root = conn.setup().roots[screen].root;
        let atom = conn
            .intern_atom(false, b"_NET_ACTIVE_WINDOW")?
            .reply()?
            .atom;
        // Source indication 2: a pager or taskbar acting for the user.
        let message = ClientMessageEvent::new(32, window, atom, [2, 0, 0, 0, 0]);
        conn.send_event(
            false,
            root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            message,
        )?;
        conn.flush()?;
        Ok(())
    };
    run().map_err(|error| error.to_string())
}
