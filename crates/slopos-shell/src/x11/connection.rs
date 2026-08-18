//! Persistent X11 connection management.

use super::atoms::Atoms;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ChangeWindowAttributesAux, ConnectionExt, EventMask, Window};
use x11rb::rust_connection::RustConnection;

pub struct X11Connection {
    conn: RustConnection,
    screen_num: usize,
    root: Window,
    atoms: Atoms,
}

impl X11Connection {
    pub fn connect() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let (conn, screen_num) = x11rb::connect(None)?;
        let root = conn.setup().roots[screen_num].root;
        let atoms = Atoms::intern(&conn)?;

        // Subscribe to PropertyChange on root window so we receive active window,
        // desktop and client list updates.
        let values = ChangeWindowAttributesAux::new().event_mask(
            EventMask::PROPERTY_CHANGE
                | EventMask::STRUCTURE_NOTIFY
                | EventMask::SUBSTRUCTURE_NOTIFY,
        );
        conn.change_window_attributes(root, &values)?;
        conn.flush()?;

        Ok(Self {
            conn,
            screen_num,
            root,
            atoms,
        })
    }

    pub fn raw_conn(&self) -> &RustConnection {
        &self.conn
    }

    pub fn screen_num(&self) -> usize {
        self.screen_num
    }

    pub fn root(&self) -> Window {
        self.root
    }

    pub fn atoms(&self) -> &Atoms {
        &self.atoms
    }

    pub fn subscribe_window_events(
        &self,
        window: Window,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let values = ChangeWindowAttributesAux::new()
            .event_mask(EventMask::PROPERTY_CHANGE | EventMask::STRUCTURE_NOTIFY);
        let _ = self.conn.change_window_attributes(window, &values);
        let _ = self.conn.flush();
        Ok(())
    }
}
