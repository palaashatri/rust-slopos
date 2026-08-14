//! X11 AppMenu capability detection and bounded DBusMenu importing.
//!
//! SLOPOS never scrapes an upstream application's widgets or synthesizes
//! commands from its title.  Applications that participate in the X11 AppMenu
//! convention publish `_GTK_UNIQUE_BUS_NAME` and
//! `_GTK_APP_MENU_OBJECT_PATH` (or the older `_GTK_MENUBAR_OBJECT_PATH`) on
//! their top-level window.  When those properties are present, the optional
//! importer below asks the advertised `com.canonical.dbusmenu` object for a
//! bounded `GetLayout` tree and sends only the protocol's `clicked` event for
//! an imported item.  Any malformed response, unsupported tree shape, missing
//! session bus, or timeout is a hard fallback to the application's local menu.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, Window};
use x11rb::rust_connection::RustConnection;
use zbus::blocking::{Connection as DbusConnection, Proxy};
use zbus::zvariant::{OwnedValue, Structure, Value};

const DBUSMENU_INTERFACE: &str = "com.canonical.dbusmenu";
const DBUSMENU_GET_LAYOUT: &str = "GetLayout";
const DBUSMENU_EVENT: &str = "Event";
const MAX_LAYOUT_DEPTH: u32 = 4;
const MAX_MENU_ITEMS: usize = 256;
const MAX_LABEL_CHARS: usize = 256;

const LAYOUT_PROPERTIES: &[&str] = &["label", "enabled", "visible", "type", "children-display"];

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
    ExporterDetected,
}

/// A protocol-safe menu item.  The shell only renders these fields; it never
/// invents actions for properties it did not receive from DBusMenu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppMenuItem {
    pub id: i32,
    pub label: String,
    pub visible: bool,
    pub enabled: bool,
    pub kind: AppMenuItemKind,
    pub children: Vec<Self>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMenuItemKind {
    Standard,
    Separator,
    Submenu,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppMenuLayout {
    pub revision: u32,
    pub items: Vec<AppMenuItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppMenuImportError {
    Timeout,
    Dbus(String),
    InvalidLayout(String),
    LimitExceeded(&'static str),
}

impl std::fmt::Display for AppMenuImportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeout => formatter.write_str("DBusMenu request timed out"),
            Self::Dbus(error) => write!(formatter, "DBusMenu request failed: {error}"),
            Self::InvalidLayout(error) => write!(formatter, "invalid DBusMenu layout: {error}"),
            Self::LimitExceeded(limit) => write!(formatter, "DBusMenu {limit} limit exceeded"),
        }
    }
}

/// Fetch a menu layout without blocking the GTK main loop beyond `timeout`.
///
/// The worker is deliberately detached if the exporter ignores the request;
/// no partial response is ever exposed to the UI.  The request itself is
/// bounded to four levels and 256 visible items by `parse_layout`.
pub fn fetch_layout_with_timeout(
    exporter: &AppMenuExporter,
    timeout: Duration,
) -> Result<AppMenuLayout, AppMenuImportError> {
    let exporter = exporter.clone();
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(fetch_layout(&exporter));
    });
    receiver
        .recv_timeout(timeout)
        .map_err(|_| AppMenuImportError::Timeout)?
}

/// Fetch `GetLayout` from the advertised object and parse only the stable
/// DBusMenu properties used by a GTK menu.  This function runs on a worker
/// thread through `fetch_layout_with_timeout`.
fn fetch_layout(exporter: &AppMenuExporter) -> Result<AppMenuLayout, AppMenuImportError> {
    let connection =
        DbusConnection::session().map_err(|error| AppMenuImportError::Dbus(error.to_string()))?;
    let proxy = Proxy::new(
        &connection,
        exporter.bus_name.as_str(),
        exporter.object_path.as_str(),
        DBUSMENU_INTERFACE,
    )
    .map_err(|error| AppMenuImportError::Dbus(error.to_string()))?;
    let arguments = (0i32, MAX_LAYOUT_DEPTH, LAYOUT_PROPERTIES.to_vec());
    let reply = proxy
        .call_method(DBUSMENU_GET_LAYOUT, &arguments)
        .map_err(|error| AppMenuImportError::Dbus(error.to_string()))?;
    let reply_body = reply.body();
    let body: Structure<'_> = reply_body
        .deserialize()
        .map_err(|error| AppMenuImportError::Dbus(error.to_string()))?;
    let fields = body.into_fields();
    if fields.len() != 2 {
        return Err(AppMenuImportError::InvalidLayout(
            "GetLayout reply must contain revision and root".to_string(),
        ));
    }
    let mut fields = fields.into_iter();
    let revision = u32::try_from(fields.next().expect("checked field count"))
        .map_err(|_| AppMenuImportError::InvalidLayout("revision is not uint32".to_string()))?;
    let Value::Structure(root) = fields.next().expect("checked field count") else {
        return Err(AppMenuImportError::InvalidLayout(
            "GetLayout root is not a structure".to_string(),
        ));
    };
    parse_layout(revision, root)
}

/// Send the only action SLOPOS supports for imported items: DBusMenu's
/// `Event(id, "clicked", 0, timestamp)`.  Unknown IDs never reach this
/// function because they can only be captured from a parsed layout item.
pub fn activate(
    exporter: &AppMenuExporter,
    item_id: i32,
    timeout: Duration,
) -> Result<(), AppMenuImportError> {
    if item_id < 0 {
        return Err(AppMenuImportError::InvalidLayout(
            "negative item id".to_string(),
        ));
    }
    let exporter = exporter.clone();
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = (|| {
            let connection = DbusConnection::session()
                .map_err(|error| AppMenuImportError::Dbus(error.to_string()))?;
            let proxy = Proxy::new(
                &connection,
                exporter.bus_name.as_str(),
                exporter.object_path.as_str(),
                DBUSMENU_INTERFACE,
            )
            .map_err(|error| AppMenuImportError::Dbus(error.to_string()))?;
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                .min(u32::MAX as u128) as u32;
            let data = OwnedValue::try_from(Value::from(0u32))
                .map_err(|error| AppMenuImportError::Dbus(error.to_string()))?;
            proxy
                .call_noreply(DBUSMENU_EVENT, &(item_id, "clicked", data, timestamp))
                .map_err(|error| AppMenuImportError::Dbus(error.to_string()))
        })();
        let _ = sender.send(result);
    });
    receiver
        .recv_timeout(timeout)
        .map_err(|_| AppMenuImportError::Timeout)?
}

/// Parse the dynamic `(ia{sv}av)` DBusMenu layout returned by `GetLayout`.
///
/// The root node is not itself a visible menu item; its children become the
/// imported top-level entries.  Unknown properties/types are ignored, while
/// malformed required container shapes fail closed.
fn parse_layout<'a>(
    revision: u32,
    root: Structure<'a>,
) -> Result<AppMenuLayout, AppMenuImportError> {
    let mut budget = ParseBudget::default();
    let root = parse_wire_node(root, 0, &mut budget)?;
    Ok(AppMenuLayout {
        revision,
        items: root.children,
    })
}

#[derive(Default)]
struct ParseBudget {
    items: usize,
}

fn parse_wire_node<'a>(
    node: Structure<'a>,
    depth: u32,
    budget: &mut ParseBudget,
) -> Result<AppMenuItem, AppMenuImportError> {
    if depth > MAX_LAYOUT_DEPTH {
        return Err(AppMenuImportError::LimitExceeded("layout depth"));
    }
    let fields = node.into_fields();
    if fields.len() != 3 {
        return Err(AppMenuImportError::InvalidLayout(
            "layout node must contain id, properties, children".to_string(),
        ));
    }
    let mut fields = fields.into_iter();
    let id = i32::try_from(fields.next().expect("checked field count"))
        .map_err(|_| AppMenuImportError::InvalidLayout("item id is not int32".to_string()))?;
    let properties = parse_properties(fields.next().expect("checked field count"))?;
    let child_values = parse_children(fields.next().expect("checked field count"))?;

    let visible = properties
        .get("visible")
        .and_then(PropertyValue::as_bool)
        .unwrap_or(true);
    let enabled = properties
        .get("enabled")
        .and_then(PropertyValue::as_bool)
        .unwrap_or(true);
    let item_type = properties
        .get("type")
        .and_then(PropertyValue::as_text)
        .unwrap_or("standard");
    let children_display = properties
        .get("children-display")
        .and_then(PropertyValue::as_text);

    if item_type != "standard" && item_type != "separator" {
        return Err(AppMenuImportError::InvalidLayout(format!(
            "unsupported item type {item_type:?}"
        )));
    }
    if item_type == "separator" && !child_values.is_empty() {
        return Err(AppMenuImportError::InvalidLayout(
            "separator has children".to_string(),
        ));
    }
    if depth != 0 && !child_values.is_empty() && children_display != Some("submenu") {
        return Err(AppMenuImportError::InvalidLayout(
            "children are missing children-display=submenu".to_string(),
        ));
    }
    if child_values.len() > MAX_MENU_ITEMS {
        return Err(AppMenuImportError::LimitExceeded("children"));
    }

    let mut children = Vec::with_capacity(child_values.len());
    for child in child_values {
        children.push(parse_wire_node(child, depth + 1, budget)?);
    }
    let kind = if item_type == "separator" {
        AppMenuItemKind::Separator
    } else if children_display == Some("submenu") {
        AppMenuItemKind::Submenu
    } else {
        AppMenuItemKind::Standard
    };
    let label = properties
        .get("label")
        .and_then(PropertyValue::as_text)
        .unwrap_or_default()
        .to_string();
    validate_label(&label)?;

    // Invisible entries are still parsed and counted to avoid an exporter
    // hiding an unbounded tree behind the protocol's `visible` property, but
    // they are not rendered by the shell.
    budget.items = budget
        .items
        .checked_add(1)
        .ok_or(AppMenuImportError::LimitExceeded("item count"))?;
    if budget.items > MAX_MENU_ITEMS {
        return Err(AppMenuImportError::LimitExceeded("item count"));
    }

    Ok(AppMenuItem {
        id,
        label,
        visible,
        enabled,
        kind,
        children,
    })
}

/// DBusMenu `a{sv}` values are intentionally reduced to the small string and
/// boolean property set above.  Returning a map of `Option<String>` keeps an
/// absent property distinct from an explicitly empty label.
fn parse_properties<'a>(
    value: Value<'a>,
) -> Result<HashMap<String, PropertyValue>, AppMenuImportError> {
    let Value::Dict(dictionary) = value else {
        return Err(AppMenuImportError::InvalidLayout(
            "properties are not a dictionary".to_string(),
        ));
    };
    let mut properties = HashMap::new();
    for (key, value) in dictionary {
        let key = String::try_from(key).map_err(|_| {
            AppMenuImportError::InvalidLayout("property key is not string".to_string())
        })?;
        let value = match value {
            Value::Value(inner) => *inner,
            value => value,
        };
        match key.as_str() {
            "label" | "type" | "children-display" => {
                let text = String::try_from(value).map_err(|_| {
                    AppMenuImportError::InvalidLayout(format!("property {key} is not string"))
                })?;
                properties.insert(key, PropertyValue::Text(text));
            }
            "visible" | "enabled" => {
                let enabled = bool::try_from(value).map_err(|_| {
                    AppMenuImportError::InvalidLayout(format!("property {key} is not bool"))
                })?;
                properties.insert(key, PropertyValue::Bool(enabled));
            }
            _ => {}
        }
    }
    Ok(properties)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PropertyValue {
    Text(String),
    Bool(bool),
}

impl PropertyValue {
    fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(value) => Some(value),
            Self::Bool(_) => None,
        }
    }
    fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            Self::Text(_) => None,
        }
    }
}

fn parse_children<'a>(value: Value<'a>) -> Result<Vec<Structure<'a>>, AppMenuImportError> {
    let Value::Array(array) = value else {
        return Err(AppMenuImportError::InvalidLayout(
            "children are not an array".to_string(),
        ));
    };
    let mut children = Vec::with_capacity(array.len());
    for child in array.iter() {
        let child = child
            .try_clone()
            .map_err(|error| AppMenuImportError::InvalidLayout(error.to_string()))?;
        let child = match child {
            Value::Value(inner) => *inner,
            child => child,
        };
        let Value::Structure(structure) = child else {
            return Err(AppMenuImportError::InvalidLayout(
                "child is not a layout structure".to_string(),
            ));
        };
        children.push(structure);
    }
    Ok(children)
}

fn validate_label(label: &str) -> Result<(), AppMenuImportError> {
    if label.chars().count() > MAX_LABEL_CHARS {
        return Err(AppMenuImportError::LimitExceeded("label length"));
    }
    if label.chars().any(char::is_control) {
        return Err(AppMenuImportError::InvalidLayout(
            "label contains control characters".to_string(),
        ));
    }
    Ok(())
}

pub fn status_for_window(window_id: Window) -> (AppMenuStatus, Option<AppMenuExporter>) {
    let exporter = detect_exporter(window_id);
    let status = if exporter.is_some() {
        AppMenuStatus::ExporterDetected
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
    // `_GTK_UNIQUE_BUS_NAME` must contain a D-Bus unique name, not merely a
    // string that happens to begin with `:`.  Restricting this to the
    // dot-separated ASCII bus-name grammar prevents a malformed X11 property
    // from steering the worker toward an arbitrary destination.
    let Some(body) = value.strip_prefix(':') else {
        return false;
    };
    let components = body.split('.').collect::<Vec<_>>();
    value.len() <= 255
        && components.len() >= 2
        && components.iter().all(|component| {
            !component.is_empty()
                && component
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
        })
}

fn valid_object_path(value: &str) -> bool {
    // D-Bus object paths are `/` or slash-separated elements.  Each element
    // starts with an ASCII letter/underscore and continues with
    // ASCII letters, digits or underscores; empty elements are invalid.
    if value.len() > 1024 || value == "/" {
        return value == "/";
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
    use super::{
        parse_layout, valid_bus_name, valid_object_path, AppMenuImportError, AppMenuItemKind,
    };
    use zbus::zvariant::{Array, Dict, Signature, Structure, StructureBuilder, Value};

    fn properties(entries: Vec<(&'static str, Value<'static>)>) -> Dict<'static, 'static> {
        let mut dictionary = Dict::new(
            Signature::from_static_str_unchecked("s"),
            Signature::from_static_str_unchecked("v"),
        );
        for (key, value) in entries {
            dictionary.add(key, value).expect("valid DBusMenu property");
        }
        dictionary
    }

    fn node(
        id: i32,
        entries: Vec<(&'static str, Value<'static>)>,
        children: Vec<Structure<'static>>,
    ) -> Structure<'static> {
        let mut child_array = Array::new(Signature::from_static_str_unchecked("v"));
        for child in children {
            child_array
                .append(Value::Value(Box::new(Value::Structure(child))))
                .expect("valid DBusMenu child structure");
        }
        StructureBuilder::new()
            .append_field(Value::I32(id))
            .append_field(Value::Dict(properties(entries)))
            .append_field(Value::Array(child_array))
            .build()
    }

    #[test]
    fn accepts_only_session_bus_names() {
        assert!(valid_bus_name(":1.42"));
        assert!(valid_bus_name(":1.2.3"));
        assert!(valid_bus_name(":1foo.42"));
        assert!(!valid_bus_name("org.example.App"));
        assert!(!valid_bus_name(":"));
        assert!(!valid_bus_name(":1foo"));
        assert!(!valid_bus_name(":1."));
        assert!(!valid_bus_name(":1..2"));
        assert!(!valid_bus_name(":1.f?"));
        assert!(!valid_bus_name(":1 42"));
        assert!(!valid_bus_name(""));
    }

    #[test]
    fn accepts_dbus_object_paths_without_invalid_elements() {
        assert!(valid_object_path("/"));
        assert!(valid_object_path("/com/canonical/menu"));
        assert!(valid_object_path("/org/slopos_1/menu2"));
        assert!(!valid_object_path("com/canonical/menu"));
        assert!(!valid_object_path("/com//menu"));
        assert!(!valid_object_path("/com/1menu"));
        assert!(!valid_object_path("/com/menu-name"));
        assert!(!valid_object_path("/com/example/menu\n"));
    }

    #[test]
    fn parses_supported_items_without_inventing_commands() {
        let child = node(
            3,
            vec![
                ("label", Value::from("Undo")),
                ("enabled", Value::from(false)),
            ],
            vec![],
        );
        let root = node(
            0,
            vec![],
            vec![
                node(1, vec![("label", Value::from("File"))], vec![]),
                node(2, vec![("type", Value::from("separator"))], vec![]),
                node(
                    4,
                    vec![
                        ("label", Value::from("Edit")),
                        ("children-display", Value::from("submenu")),
                    ],
                    vec![child],
                ),
            ],
        );
        let layout = parse_layout(7, root).expect("valid DBusMenu layout");
        assert_eq!(layout.revision, 7);
        assert_eq!(layout.items.len(), 3);
        assert_eq!(layout.items[0].label, "File");
        assert_eq!(layout.items[0].kind, AppMenuItemKind::Standard);
        assert_eq!(layout.items[1].kind, AppMenuItemKind::Separator);
        assert_eq!(layout.items[2].kind, AppMenuItemKind::Submenu);
        assert_eq!(layout.items[2].children[0].label, "Undo");
        assert!(!layout.items[2].children[0].enabled);
    }

    #[test]
    fn rejects_unsupported_item_type() {
        let root = node(
            0,
            vec![],
            vec![node(
                1,
                vec![("type", Value::from("x-vendor-command"))],
                vec![],
            )],
        );
        assert!(matches!(
            parse_layout(1, root),
            Err(AppMenuImportError::InvalidLayout(message)) if message.contains("unsupported item type")
        ));
    }

    #[test]
    fn rejects_control_characters_in_labels() {
        let root = node(
            0,
            vec![],
            vec![node(1, vec![("label", Value::from("Bad\nLabel"))], vec![])],
        );
        assert!(matches!(
            parse_layout(1, root),
            Err(AppMenuImportError::InvalidLayout(message)) if message.contains("control characters")
        ));
    }
}
