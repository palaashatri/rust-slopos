//! Native GTK/GIO global-menu bridge for X11 GtkApplication windows.
//!
//! GTK publishes the remote GMenuModel plus application/window action-group
//! object paths as X11 properties. GIO already provides safe client proxies for
//! those private wire protocols, so SLOPOS deliberately uses GDBusMenuModel and
//! GDBusActionGroup instead of reimplementing `org.gtk.Menus`/`org.gtk.Actions`.

use gio::prelude::*;
use gtk::prelude::*;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, Window};
use x11rb::rust_connection::RustConnection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GtkMenuExporter {
    pub window_id: Window,
    pub bus_name: String,
    pub menu_path: String,
    pub app_action_path: Option<String>,
    pub window_action_path: Option<String>,
}

pub fn detect(window_id: Window) -> Option<GtkMenuExporter> {
    let (connection, _) = RustConnection::connect(None).ok()?;
    let unique_atom = intern_atom(&connection, b"_GTK_UNIQUE_BUS_NAME")?;
    let menubar_atom = intern_atom(&connection, b"_GTK_MENUBAR_OBJECT_PATH")?;
    let app_menu_atom = intern_atom(&connection, b"_GTK_APP_MENU_OBJECT_PATH")?;
    let application_atom = intern_atom(&connection, b"_GTK_APPLICATION_OBJECT_PATH")?;
    let window_atom = intern_atom(&connection, b"_GTK_WINDOW_OBJECT_PATH")?;

    let bus_name = read_property(&connection, window_id, unique_atom)?;
    // Prefer the traditional menubar: it contains File/Edit/View/etc. The
    // app-menu path is only a fallback for applications that export no menubar.
    let menu_path = read_property(&connection, window_id, menubar_atom)
        .or_else(|| read_property(&connection, window_id, app_menu_atom))?;
    let app_action_path = read_property(&connection, window_id, application_atom);
    let window_action_path = read_property(&connection, window_id, window_atom);

    if !valid_bus_name(&bus_name) || !valid_object_path(&menu_path) {
        return None;
    }
    if app_action_path
        .as_deref()
        .is_some_and(|path| !valid_object_path(path))
        || window_action_path
            .as_deref()
            .is_some_and(|path| !valid_object_path(path))
    {
        return None;
    }

    // A GMenu model without either action group can still contain non-action
    // section headings, but presenting it as an application menu would create
    // dead controls. Fall back to the application's local menu instead.
    if app_action_path.is_none() && window_action_path.is_none() {
        return None;
    }

    Some(GtkMenuExporter {
        window_id,
        bus_name,
        menu_path,
        app_action_path,
        window_action_path,
    })
}

pub fn build_menu_bar(exporter: &GtkMenuExporter) -> Result<gtk::MenuBar, String> {
    let connection = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>)
        .map_err(|error| format!("connect to session bus: {error}"))?;
    let model = gio::DBusMenuModel::get(
        &connection,
        Some(exporter.bus_name.as_str()),
        exporter.menu_path.as_str(),
    );
    let menu_bar = gtk::MenuBar::from_model(&model);
    menu_bar.style_context().add_class("slopos-menu-bar");

    if let Some(path) = exporter.app_action_path.as_deref() {
        let group = gio::DBusActionGroup::get(
            &connection,
            Some(exporter.bus_name.as_str()),
            path,
        );
        menu_bar.insert_action_group("app", Some(&group));
    }
    if let Some(path) = exporter.window_action_path.as_deref() {
        let group = gio::DBusActionGroup::get(
            &connection,
            Some(exporter.bus_name.as_str()),
            path,
        );
        menu_bar.insert_action_group("win", Some(&group));
    }

    Ok(menu_bar)
}

fn intern_atom(connection: &RustConnection, name: &[u8]) -> Option<u32> {
    connection
        .intern_atom(false, name)
        .ok()?
        .reply()
        .ok()
        .map(|reply| reply.atom)
}

fn read_property(connection: &RustConnection, window: Window, atom: u32) -> Option<String> {
    let reply = connection
        .get_property(false, window, atom, AtomEnum::ANY, 0, 4096)
        .ok()?
        .reply()
        .ok()?;
    let value = String::from_utf8(reply.value).ok()?;
    let value = value.trim_matches('\0').trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn valid_bus_name(value: &str) -> bool {
    if value.len() > 255 {
        return false;
    }
    let Some(body) = value.strip_prefix(':') else {
        return false;
    };
    let mut components = body.split('.');
    let (Some(first), Some(second)) = (components.next(), components.next()) else {
        return false;
    };
    valid_bus_component(first)
        && valid_bus_component(second)
        && components.all(valid_bus_component)
}

fn valid_bus_component(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn valid_object_path(value: &str) -> bool {
    if value == "/" {
        return true;
    }
    if value.len() > 1024 {
        return false;
    }
    let Some(body) = value.strip_prefix('/') else {
        return false;
    };
    !body.is_empty()
        && body.split('/').all(|component| {
            let mut bytes = component.bytes();
            let Some(first) = bytes.next() else {
                return false;
            };
            (first.is_ascii_alphabetic() || first == b'_')
                && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
}

#[cfg(test)]
mod tests {
    use super::{valid_bus_name, valid_object_path};

    #[test]
    fn validates_gtk_exporter_addresses() {
        assert!(valid_bus_name(":1.42"));
        assert!(!valid_bus_name("org.example.App"));
        assert!(valid_object_path("/org/gtk/Test/menus/MenuBar"));
        assert!(valid_object_path("/org/gtk/Test/windows/0"));
        assert!(!valid_object_path("org/gtk/Test"));
        assert!(!valid_object_path("/org/gtk/menu-name"));
    }
}
