//! EWMH root window and client state queries via x11rb.

use super::atoms::Atoms;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, Window};
use x11rb::rust_connection::RustConnection;

pub fn get_active_window(conn: &RustConnection, root: Window, atoms: &Atoms) -> Option<Window> {
    let reply = conn
        .get_property(false, root, atoms.net_active_window, AtomEnum::WINDOW, 0, 1)
        .ok()?
        .reply()
        .ok()?;

    if reply.format != 32 || reply.value.is_empty() {
        return None;
    }

    let win = reply.value32()?.next()?;
    if win == 0 {
        None
    } else {
        Some(win)
    }
}

pub fn get_current_desktop(conn: &RustConnection, root: Window, atoms: &Atoms) -> Option<u32> {
    let reply = conn
        .get_property(
            false,
            root,
            atoms.net_current_desktop,
            AtomEnum::CARDINAL,
            0,
            1,
        )
        .ok()?
        .reply()
        .ok()?;

    let desktop = reply.value32()?.next();
    desktop
}

pub fn get_number_of_desktops(conn: &RustConnection, root: Window, atoms: &Atoms) -> Option<u32> {
    let reply = conn
        .get_property(
            false,
            root,
            atoms.net_number_of_desktops,
            AtomEnum::CARDINAL,
            0,
            1,
        )
        .ok()?
        .reply()
        .ok()?;

    let count = reply.value32()?.next();
    count
}

pub fn get_client_list(conn: &RustConnection, root: Window, atoms: &Atoms) -> Vec<Window> {
    let Ok(cookie) = conn.get_property(
        false,
        root,
        atoms.net_client_list,
        AtomEnum::WINDOW,
        0,
        1024,
    ) else {
        return Vec::new();
    };

    let Ok(reply) = cookie.reply() else {
        return Vec::new();
    };

    if reply.format != 32 {
        return Vec::new();
    }

    reply
        .value32()
        .map(|iter| iter.collect())
        .unwrap_or_default()
}
