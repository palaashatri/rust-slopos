//! Capability detection for the X11 application-menu conventions.
//!
//! SLOPOS deliberately does not scrape an upstream application's widgets or
//! synthesize commands from its title.  Applications that participate in the
//! X11 AppMenu convention publish these properties on their top-level window:
//! `_GTK_UNIQUE_BUS_NAME` and `_GTK_APP_MENU_OBJECT_PATH`.  We detect those
//! properties so the shell can report the capability honestly.  Importing the
//! `com.canonical.dbusmenu` tree is intentionally a separate, unsupported
//! capability until it can be implemented and tested without changing the
//! upstream application's command semantics.

use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, Window};
use x11rb::rust_connection::RustConnection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppMenuExporter {
    pub window_id: Window,
    pub bus_name: String,
    pub object_path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMenuStatus {
    ShellOwned,
    NoExporter,
    ExporterDetectedNoImporter,
}

pub fn status_for_window(window_id: Window) -> (AppMenuStatus, Option<AppMenuExporter>) {
    let exporter = detect_exporter(window_id);
    let status = if exporter.is_some() {
        AppMenuStatus::ExporterDetectedNoImporter
    } else {
        AppMenuStatus::NoExporter
    };
    (status, exporter)
}

pub fn detect_exporter(window_id: Window) -> Option<AppMenuExporter> {
    let (connection, _) = RustConnection::connect(None).ok()?;
    let unique_atom = intern_atom(&connection, b"_GTK_UNIQUE_BUS_NAME")?;
    let object_atom = intern_atom(&connection, b"_GTK_APP_MENU_OBJECT_PATH")?;
    let menubar_atom = intern_atom(&connection, b"_GTK_MENUBAR_OBJECT_PATH")?;
    let bus_name = read_property(&connection, window_id, unique_atom)?;
    let object_path = read_property(&connection, window_id, object_atom)
        .or_else(|| read_property(&connection, window_id, menubar_atom))?;
    if !valid_bus_name(&bus_name) || !valid_object_path(&object_path) {
        return None;
    }
    Some(AppMenuExporter {
        window_id,
        bus_name,
        object_path,
    })
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
    value.starts_with(':')
        && value.len() <= 255
        && value.chars().all(|character| {
            !character.is_control() && !character.is_whitespace() && character != '\0'
        })
}

fn valid_object_path(value: &str) -> bool {
    value.starts_with('/')
        && value.len() <= 1024
        && value
            .chars()
            .all(|character| !character.is_control() && character != '\0')
}

#[cfg(test)]
mod tests {
    use super::{valid_bus_name, valid_object_path};

    #[test]
    fn accepts_only_session_bus_names() {
        assert!(valid_bus_name(":1.42"));
        assert!(!valid_bus_name("org.example.App"));
        assert!(!valid_bus_name(":1 42"));
        assert!(!valid_bus_name(""));
    }

    #[test]
    fn accepts_absolute_object_paths_without_controls() {
        assert!(valid_object_path("/com/canonical/menu"));
        assert!(!valid_object_path("com/canonical/menu"));
        assert!(!valid_object_path("/com/example/menu\n"));
    }
}
