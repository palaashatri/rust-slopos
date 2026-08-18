//! Client window metadata and EWMH state queries.

use super::atoms::Atoms;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, Window};
use x11rb::rust_connection::RustConnection;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WindowInfo {
    pub window_id: Window,
    pub title: String,
    pub class_name: String,
    pub instance_name: String,
    pub is_fullscreen: bool,
    pub is_maximized: bool,
}

pub fn get_window_info(conn: &RustConnection, window: Window, atoms: &Atoms) -> WindowInfo {
    let title = get_window_title(conn, window, atoms).unwrap_or_default();
    let (instance_name, class_name) =
        get_window_class(conn, window, atoms).unwrap_or_else(|| (String::new(), String::new()));
    let (is_fullscreen, is_maximized) = get_window_state(conn, window, atoms);

    WindowInfo {
        window_id: window,
        title,
        class_name,
        instance_name,
        is_fullscreen,
        is_maximized,
    }
}

pub fn get_window_title(conn: &RustConnection, window: Window, atoms: &Atoms) -> Option<String> {
    // 1. Try _NET_WM_NAME (UTF8_STRING)
    if let Ok(cookie) =
        conn.get_property(false, window, atoms.net_wm_name, atoms.utf8_string, 0, 1024)
    {
        if let Ok(reply) = cookie.reply() {
            if reply.format == 8 && !reply.value.is_empty() {
                if let Ok(title) = String::from_utf8(reply.value) {
                    let trimmed = title.trim().to_string();
                    if !trimmed.is_empty() {
                        return Some(trimmed);
                    }
                }
            }
        }
    }

    // 2. Fallback to WM_NAME (STRING or compound text)
    if let Ok(cookie) = conn.get_property(false, window, atoms.wm_name, AtomEnum::STRING, 0, 1024) {
        if let Ok(reply) = cookie.reply() {
            if reply.format == 8 && !reply.value.is_empty() {
                let text = String::from_utf8_lossy(&reply.value).trim().to_string();
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
    }

    None
}

pub fn get_window_class(
    conn: &RustConnection,
    window: Window,
    atoms: &Atoms,
) -> Option<(String, String)> {
    let cookie = conn
        .get_property(false, window, atoms.wm_class, AtomEnum::STRING, 0, 1024)
        .ok()?;

    let reply = cookie.reply().ok()?;
    if reply.format != 8 || reply.value.is_empty() {
        return None;
    }

    // WM_CLASS contains two null-terminated strings: instance_name\0class_name\0
    let mut parts = reply
        .value
        .split(|&b| b == 0)
        .filter(|part| !part.is_empty())
        .map(|part| String::from_utf8_lossy(part).to_string());

    let instance = parts.next().unwrap_or_default();
    let class = parts.next().unwrap_or_else(|| instance.clone());

    Some((instance, class))
}

pub fn get_window_state(conn: &RustConnection, window: Window, atoms: &Atoms) -> (bool, bool) {
    let Ok(cookie) = conn.get_property(false, window, atoms.net_wm_state, AtomEnum::ATOM, 0, 32)
    else {
        return (false, false);
    };

    let Ok(reply) = cookie.reply() else {
        return (false, false);
    };

    if reply.format != 32 {
        return (false, false);
    }

    let Some(state_atoms) = reply.value32() else {
        return (false, false);
    };

    let mut is_fullscreen = false;
    let mut is_max_vert = false;
    let mut is_max_horz = false;

    for atom in state_atoms {
        if atom == atoms.net_wm_state_fullscreen {
            is_fullscreen = true;
        } else if atom == atoms.net_wm_state_maximized_vert {
            is_max_vert = true;
        } else if atom == atoms.net_wm_state_maximized_horz {
            is_max_horz = true;
        }
    }

    (is_fullscreen, is_max_vert && is_max_horz)
}

pub fn close_window(conn: &RustConnection, window: Window, atoms: &Atoms) {
    use x11rb::protocol::xproto::{ClientMessageEvent, EventMask};
    let event = ClientMessageEvent {
        response_type: x11rb::protocol::xproto::CLIENT_MESSAGE_EVENT,
        format: 32,
        sequence: 0,
        window,
        type_: atoms.net_close_window,
        data: [0, 0, 0, 0, 0].into(),
    };
    let root = conn.setup().roots[0].root;
    let _ = conn.send_event(
        false,
        root,
        EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
        event,
    );
    let _ = conn.flush();
}

pub fn minimize_window(conn: &RustConnection, window: Window, atoms: &Atoms) {
    use x11rb::protocol::xproto::{ClientMessageEvent, EventMask};
    let event = ClientMessageEvent {
        response_type: x11rb::protocol::xproto::CLIENT_MESSAGE_EVENT,
        format: 32,
        sequence: 0,
        window,
        type_: atoms.wm_change_state,
        data: [3 /* IconicState */, 0, 0, 0, 0].into(),
    };
    let root = conn.setup().roots[0].root;
    let _ = conn.send_event(
        false,
        root,
        EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
        event,
    );
    let _ = conn.flush();
}

pub fn toggle_maximize_window(conn: &RustConnection, window: Window, atoms: &Atoms) {
    use x11rb::protocol::xproto::{ClientMessageEvent, EventMask};
    let event = ClientMessageEvent {
        response_type: x11rb::protocol::xproto::CLIENT_MESSAGE_EVENT,
        format: 32,
        sequence: 0,
        window,
        type_: atoms.net_wm_state,
        data: [
            2, /* _NET_WM_STATE_TOGGLE */
            atoms.net_wm_state_maximized_vert,
            atoms.net_wm_state_maximized_horz,
            1, /* source indication: normal application */
            0,
        ]
        .into(),
    };
    let root = conn.setup().roots[0].root;
    let _ = conn.send_event(
        false,
        root,
        EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
        event,
    );
    let _ = conn.flush();
}

pub fn send_close_window(window: Window) {
    if let Ok((conn, _)) = RustConnection::connect(None) {
        if let Ok(atoms) = Atoms::intern(&conn) {
            close_window(&conn, window, &atoms);
        }
    }
}

pub fn send_minimize_window(window: Window) {
    if let Ok((conn, _)) = RustConnection::connect(None) {
        if let Ok(atoms) = Atoms::intern(&conn) {
            minimize_window(&conn, window, &atoms);
        }
    }
}

pub fn send_toggle_maximize_window(window: Window) {
    if let Ok((conn, _)) = RustConnection::connect(None) {
        if let Ok(atoms) = Atoms::intern(&conn) {
            toggle_maximize_window(&conn, window, &atoms);
        }
    }
}
