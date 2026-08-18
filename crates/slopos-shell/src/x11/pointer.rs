//! Pointer position query via x11rb.

use x11rb::protocol::xproto::{ConnectionExt, Window};
use x11rb::rust_connection::RustConnection;

pub fn get_pointer_position(conn: &RustConnection, root: Window) -> Option<(i32, i32)> {
    let reply = conn.query_pointer(root).ok()?.reply().ok()?;
    Some((reply.root_x as i32, reply.root_y as i32))
}

pub fn is_pointer_near_bottom(
    conn: &RustConnection,
    root: Window,
    screen_height: i32,
    is_currently_visible: bool,
) -> bool {
    let Some((_, y)) = get_pointer_position(conn, root) else {
        return false;
    };
    let threshold = if is_currently_visible {
        screen_height - 65
    } else {
        screen_height - 12
    };
    y >= threshold
}
