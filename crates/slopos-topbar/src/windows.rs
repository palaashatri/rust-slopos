//! The focused window, as the top bar needs it: which window it is, its
//! class and title, whether it is fullscreen, and where it exports its
//! menus. A thread with its own X connection blocks on PropertyNotify for
//! `_NET_ACTIVE_WINDOW` (and the active window's own properties) and sends
//! a fresh [`Active`] whenever something the bar shows changes.
//!
//! Window actions (close, minimise, zoom) are EWMH client messages to the
//! window manager.

use std::sync::mpsc::{self, Receiver};
use std::thread;

use slopos_ui::platform::menu_import::MenuSource;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ChangeWindowAttributesAux, ClientMessageEvent, ConnectionExt, EventMask, Window,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;

x11rb::atom_manager! {
    Atoms: AtomsCookie {
        _NET_ACTIVE_WINDOW,
        _NET_WM_NAME,
        _NET_WM_STATE,
        _NET_WM_STATE_FULLSCREEN,
        _NET_WM_STATE_MAXIMIZED_VERT,
        _NET_WM_STATE_MAXIMIZED_HORZ,
        _NET_CLOSE_WINDOW,
        _NET_WM_WINDOW_TYPE,
        _NET_WM_WINDOW_TYPE_DOCK,
        _NET_WM_WINDOW_TYPE_DESKTOP,
        WM_CHANGE_STATE,
        UTF8_STRING,
        _GTK_UNIQUE_BUS_NAME,
        _GTK_MENUBAR_OBJECT_PATH,
        _GTK_APP_MENU_OBJECT_PATH,
        _GTK_APPLICATION_OBJECT_PATH,
        _GTK_WINDOW_OBJECT_PATH,
    }
}

/// The focused application window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Active {
    pub window: Window,
    pub title: String,
    /// `WM_CLASS` class, e.g. "slopos-files".
    pub class: String,
    pub fullscreen: bool,
    pub menus: Option<MenuSource>,
}

/// Watch the focused window; `None` means the desktop (no application
/// window, or a dock/desktop surface). `wake` runs after every update.
pub fn watch(wake: impl Fn() + Send + 'static) -> Option<Receiver<Option<Active>>> {
    let (conn, screen) = x11rb::connect(None).ok()?;
    let root = conn.setup().roots[screen].root;
    let atoms = Atoms::new(&conn).ok()?.reply().ok()?;
    let (sender, receiver) = mpsc::channel();
    conn.change_window_attributes(
        root,
        &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
    )
    .ok()?;
    conn.flush().ok()?;
    thread::Builder::new()
        .name("topbar-windows".into())
        .spawn(move || {
            let mut last: Option<Option<Active>> = None;
            let mut watched: Option<Window> = None;
            loop {
                let current = active(&conn, root, &atoms);
                let window = current.as_ref().map(|active| active.window);
                if window != watched {
                    if let Some(window) = window {
                        // Follow the focused window's title, state and menus.
                        let _ = conn.change_window_attributes(
                            window,
                            &ChangeWindowAttributesAux::new()
                                .event_mask(EventMask::PROPERTY_CHANGE),
                        );
                        let _ = conn.flush();
                    }
                    watched = window;
                }
                if last.as_ref() != Some(&current) {
                    if sender.send(current.clone()).is_err() {
                        return;
                    }
                    wake();
                    last = Some(current);
                }
                // Block until a property we care about changes.
                loop {
                    let Ok(event) = conn.wait_for_event() else {
                        return;
                    };
                    if let Event::PropertyNotify(notify) = event {
                        let relevant = (notify.window == root
                            && notify.atom == atoms._NET_ACTIVE_WINDOW)
                            || (Some(notify.window) == watched
                                && [
                                    atoms._NET_WM_NAME,
                                    atoms._NET_WM_STATE,
                                    atoms._GTK_MENUBAR_OBJECT_PATH,
                                    atoms._GTK_UNIQUE_BUS_NAME,
                                    u32::from(AtomEnum::WM_CLASS),
                                ]
                                .contains(&notify.atom));
                        if relevant {
                            break;
                        }
                    }
                }
            }
        })
        .ok()?;
    Some(receiver)
}

fn property_string(conn: &RustConnection, window: Window, atom: u32) -> Option<String> {
    let reply = conn
        .get_property(false, window, atom, AtomEnum::ANY, 0, 1024)
        .ok()?
        .reply()
        .ok()?;
    let text = String::from_utf8_lossy(&reply.value)
        .trim_matches('\0')
        .trim()
        .to_owned();
    (!text.is_empty()).then_some(text)
}

fn property_atoms(conn: &RustConnection, window: Window, atom: u32) -> Vec<u32> {
    conn.get_property(false, window, atom, AtomEnum::ATOM, 0, 64)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .and_then(|reply| reply.value32().map(Iterator::collect))
        .unwrap_or_default()
}

fn active(conn: &RustConnection, root: Window, atoms: &Atoms) -> Option<Active> {
    let window = conn
        .get_property(
            false,
            root,
            atoms._NET_ACTIVE_WINDOW,
            AtomEnum::WINDOW,
            0,
            1,
        )
        .ok()?
        .reply()
        .ok()?
        .value32()?
        .next()
        .filter(|&window| window != 0)?;
    let kinds = property_atoms(conn, window, atoms._NET_WM_WINDOW_TYPE);
    if kinds.contains(&atoms._NET_WM_WINDOW_TYPE_DOCK)
        || kinds.contains(&atoms._NET_WM_WINDOW_TYPE_DESKTOP)
    {
        return None;
    }
    let class = property_string(conn, window, AtomEnum::WM_CLASS.into())
        .map(|value| value.split('\0').next_back().unwrap_or("").to_owned())
        .unwrap_or_default();
    let title = property_string(conn, window, atoms._NET_WM_NAME)
        .or_else(|| property_string(conn, window, AtomEnum::WM_NAME.into()))
        .unwrap_or_default();
    let fullscreen =
        property_atoms(conn, window, atoms._NET_WM_STATE).contains(&atoms._NET_WM_STATE_FULLSCREEN);
    let menus = property_string(conn, window, atoms._GTK_UNIQUE_BUS_NAME).and_then(|bus_name| {
        let menu_path = property_string(conn, window, atoms._GTK_MENUBAR_OBJECT_PATH)
            .or_else(|| property_string(conn, window, atoms._GTK_APP_MENU_OBJECT_PATH))?;
        valid_source(MenuSource {
            bus_name,
            menu_path,
            app_path: property_string(conn, window, atoms._GTK_APPLICATION_OBJECT_PATH),
            window_path: property_string(conn, window, atoms._GTK_WINDOW_OBJECT_PATH),
        })
    });
    Some(Active {
        window,
        title,
        class,
        fullscreen,
        menus,
    })
}

/// Reject malformed bus names and object paths a window could advertise.
pub fn valid_source(source: MenuSource) -> Option<MenuSource> {
    let valid_path = |path: &str| {
        path == "/"
            || (path.starts_with('/')
                && path.len() <= 1024
                && path[1..].split('/').all(|part| {
                    !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                }))
    };
    let valid_name = |name: &str| {
        let body = name.strip_prefix(':').unwrap_or(name);
        name.len() <= 255
            && body.split('.').count() >= 2
            && body.split('.').all(|part| {
                !part.is_empty()
                    && part
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            })
    };
    let paths_ok = valid_path(&source.menu_path)
        && source.app_path.as_deref().is_none_or(valid_path)
        && source.window_path.as_deref().is_none_or(valid_path);
    (valid_name(&source.bus_name) && paths_ok).then_some(source)
}

/// A window-manager request about `window`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowAction {
    Close,
    Minimize,
    Zoom,
}

/// Ask the window manager to close, minimise or zoom `window`.
pub fn request(window: Window, action: WindowAction) -> Result<(), String> {
    let (conn, screen) = x11rb::connect(None).map_err(|e| e.to_string())?;
    let root = conn.setup().roots[screen].root;
    let atoms = Atoms::new(&conn)
        .map_err(|e| e.to_string())?
        .reply()
        .map_err(|e| e.to_string())?;
    let (kind, data) = match action {
        // Source 2: a pager/taskbar-style request.
        WindowAction::Close => (atoms._NET_CLOSE_WINDOW, [0, 2, 0, 0, 0]),
        WindowAction::Minimize => (atoms.WM_CHANGE_STATE, [3, 0, 0, 0, 0]),
        WindowAction::Zoom => (
            atoms._NET_WM_STATE,
            [
                2,
                atoms._NET_WM_STATE_MAXIMIZED_VERT,
                atoms._NET_WM_STATE_MAXIMIZED_HORZ,
                2,
                0,
            ],
        ),
    };
    let message = ClientMessageEvent::new(32, window, kind, data);
    conn.send_event(
        false,
        root,
        EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
        message,
    )
    .map_err(|e| e.to_string())?;
    conn.flush().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(bus: &str, path: &str) -> MenuSource {
        MenuSource {
            bus_name: bus.into(),
            menu_path: path.into(),
            app_path: Some("/org/slopos/files".into()),
            window_path: None,
        }
    }

    #[test]
    fn advertised_menu_addresses_are_validated() {
        assert!(valid_source(source(":1.42", "/org/slopos/files/menus/MenuBar")).is_some());
        assert!(
            valid_source(source("org.gnome.Gedit", "/org/gnome/Gedit/menus/menubar")).is_some()
        );
        assert!(valid_source(source("invalid", "/x")).is_none());
        assert!(valid_source(source(":1.4", "relative/path")).is_none());
        assert!(valid_source(source(":1.4", "/has-dash")).is_none());
        let mut bad_actions = source(":1.4", "/ok");
        bad_actions.app_path = Some("/bad path".into());
        assert!(valid_source(bad_actions).is_none());
    }
}
