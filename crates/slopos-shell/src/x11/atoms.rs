//! X11 Atom cache for EWMH, ICCCM, and GTK protocol integration.

use x11rb::protocol::xproto::{Atom, ConnectionExt};
use x11rb::rust_connection::RustConnection;

#[derive(Debug, Clone, Copy)]
pub struct Atoms {
    pub net_active_window: Atom,
    pub net_wm_name: Atom,
    pub wm_name: Atom,
    pub wm_class: Atom,
    pub net_wm_state: Atom,
    pub net_wm_state_fullscreen: Atom,
    pub net_wm_state_maximized_vert: Atom,
    pub net_wm_state_maximized_horz: Atom,
    pub net_current_desktop: Atom,
    pub net_number_of_desktops: Atom,
    pub net_client_list: Atom,
    pub net_workarea: Atom,
    pub utf8_string: Atom,
    pub string: Atom,
    pub gtk_unique_bus_name: Atom,
    pub gtk_menubar_object_path: Atom,
    pub gtk_app_menu_object_path: Atom,
    pub gtk_application_object_path: Atom,
    pub gtk_window_object_path: Atom,
    pub net_close_window: Atom,
    pub wm_change_state: Atom,
}

impl Atoms {
    pub fn intern(conn: &RustConnection) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let net_active_window = conn.intern_atom(false, b"_NET_ACTIVE_WINDOW")?;
        let net_wm_name = conn.intern_atom(false, b"_NET_WM_NAME")?;
        let wm_name = conn.intern_atom(false, b"WM_NAME")?;
        let wm_class = conn.intern_atom(false, b"WM_CLASS")?;
        let net_wm_state = conn.intern_atom(false, b"_NET_WM_STATE")?;
        let net_wm_state_fullscreen = conn.intern_atom(false, b"_NET_WM_STATE_FULLSCREEN")?;
        let net_wm_state_maximized_vert =
            conn.intern_atom(false, b"_NET_WM_STATE_MAXIMIZED_VERT")?;
        let net_wm_state_maximized_horz =
            conn.intern_atom(false, b"_NET_WM_STATE_MAXIMIZED_HORZ")?;
        let net_current_desktop = conn.intern_atom(false, b"_NET_CURRENT_DESKTOP")?;
        let net_number_of_desktops = conn.intern_atom(false, b"_NET_NUMBER_OF_DESKTOPS")?;
        let net_client_list = conn.intern_atom(false, b"_NET_CLIENT_LIST")?;
        let net_workarea = conn.intern_atom(false, b"_NET_WORKAREA")?;
        let utf8_string = conn.intern_atom(false, b"UTF8_STRING")?;
        let string = conn.intern_atom(false, b"STRING")?;
        let gtk_unique_bus_name = conn.intern_atom(false, b"_GTK_UNIQUE_BUS_NAME")?;
        let gtk_menubar_object_path = conn.intern_atom(false, b"_GTK_MENUBAR_OBJECT_PATH")?;
        let gtk_app_menu_object_path = conn.intern_atom(false, b"_GTK_APP_MENU_OBJECT_PATH")?;
        let gtk_application_object_path =
            conn.intern_atom(false, b"_GTK_APPLICATION_OBJECT_PATH")?;
        let gtk_window_object_path = conn.intern_atom(false, b"_GTK_WINDOW_OBJECT_PATH")?;
        let net_close_window = conn.intern_atom(false, b"_NET_CLOSE_WINDOW")?;
        let wm_change_state = conn.intern_atom(false, b"WM_CHANGE_STATE")?;

        Ok(Self {
            net_active_window: net_active_window.reply()?.atom,
            net_wm_name: net_wm_name.reply()?.atom,
            wm_name: wm_name.reply()?.atom,
            wm_class: wm_class.reply()?.atom,
            net_wm_state: net_wm_state.reply()?.atom,
            net_wm_state_fullscreen: net_wm_state_fullscreen.reply()?.atom,
            net_wm_state_maximized_vert: net_wm_state_maximized_vert.reply()?.atom,
            net_wm_state_maximized_horz: net_wm_state_maximized_horz.reply()?.atom,
            net_current_desktop: net_current_desktop.reply()?.atom,
            net_number_of_desktops: net_number_of_desktops.reply()?.atom,
            net_client_list: net_client_list.reply()?.atom,
            net_workarea: net_workarea.reply()?.atom,
            utf8_string: utf8_string.reply()?.atom,
            string: string.reply()?.atom,
            gtk_unique_bus_name: gtk_unique_bus_name.reply()?.atom,
            gtk_menubar_object_path: gtk_menubar_object_path.reply()?.atom,
            gtk_app_menu_object_path: gtk_app_menu_object_path.reply()?.atom,
            gtk_application_object_path: gtk_application_object_path.reply()?.atom,
            gtk_window_object_path: gtk_window_object_path.reply()?.atom,
            net_close_window: net_close_window.reply()?.atom,
            wm_change_state: wm_change_state.reply()?.atom,
        })
    }
}
