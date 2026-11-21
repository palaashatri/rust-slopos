//! The `com.canonical.dbusmenu` client: fetch an application's menu tree over
//! the bus, turn it into the bar's menus, and send clicks back.
//!
//! - [`decode_node`] turns the wire values of `GetLayout` into the
//!   [`RawNode`] tree that `menu_registrar_layout` parses. Property types the
//!   menu protocol does not use are dropped.
//! - [`Bar::from_layout`] flattens a parsed [`Layout`] into the top bar's
//!   menus: one title per visible submenu, its entries with shortcuts and check
//!   state, and the id each clickable entry sends back.
//! - [`Client`] runs one worker thread for the blocking calls (`GetLayout`,
//!   `AboutToShow`, `Event`) and one for the `LayoutUpdated` and
//!   `ItemsPropertiesUpdated` signals. Both block on the bus.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use slopos_ui::controls::menu::MenuItem;
use zbus::blocking::connection::Builder;
use zbus::blocking::{Connection, MessageIterator, Proxy};
use zbus::zvariant::{OwnedValue, Value};

use crate::menu_registrar::MenuRef;
use crate::menu_registrar_layout::{Kind, Layout, RawNode, Value as Property};
use crate::menus::{Command, MenuSpec};

/// The interface every dbusmenu server exports.
pub const INTERFACE: &str = "com.canonical.dbusmenu";

/// Deepest submenu nesting the bar flattens into one title's list.
const MAX_BAR_DEPTH: usize = 4;

/// The root of a `GetLayout` reply, after the revision: id, properties and the
/// children as variants.
type RootReply = (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>);

/// Unwrap the `v` variants a property dictionary may nest.
fn unwrap_variant<'a, 'b>(value: &'a Value<'b>) -> &'a Value<'b> {
    match value {
        Value::Value(inner) => unwrap_variant(inner),
        other => other,
    }
}

/// A property value in the menu protocol's vocabulary, or `None` for a type the
/// protocol does not use for menus.
pub fn property(value: &Value<'_>) -> Option<Property> {
    match unwrap_variant(value) {
        Value::Str(text) => Some(Property::Str(text.as_str().to_owned())),
        Value::Bool(flag) => Some(Property::Bool(*flag)),
        Value::I32(number) => Some(Property::Int(*number)),
        // `aas`: the shortcut property, one key list per shortcut.
        Value::Array(outer) => outer
            .iter()
            .map(|element| match unwrap_variant(element) {
                Value::Array(inner) => inner
                    .iter()
                    .map(|token| match unwrap_variant(token) {
                        Value::Str(text) => Some(text.as_str().to_owned()),
                        _ => None,
                    })
                    .collect::<Option<Vec<String>>>(),
                _ => None,
            })
            .collect::<Option<Vec<Vec<String>>>>()
            .map(Property::StrLists),
        _ => None,
    }
}

/// Decode one `(ia{sv}av)` node. `None` when the structure is not a node,
/// which rejects the whole layout rather than drawing part of it.
pub fn decode_node(value: &Value<'_>) -> Option<RawNode> {
    let Value::Structure(structure) = unwrap_variant(value) else {
        return None;
    };
    match structure.fields() {
        [Value::I32(id), Value::Dict(dict), Value::Array(children)] => {
            let properties = dict
                .iter()
                .filter_map(|(key, value)| match unwrap_variant(key) {
                    Value::Str(name) => property(value).map(|p| (name.as_str().to_owned(), p)),
                    _ => None,
                })
                .collect();
            let children = children
                .iter()
                .map(decode_node)
                .collect::<Option<Vec<RawNode>>>()?;
            Some(RawNode {
                id: *id,
                properties,
                children,
            })
        }
        _ => None,
    }
}

/// The root of a `GetLayout` reply: the id asked for (0) with its children.
fn root_from_reply(children: &[OwnedValue]) -> Option<RawNode> {
    let children = children
        .iter()
        .map(|child| decode_node(child))
        .collect::<Option<Vec<RawNode>>>()?;
    Some(RawNode {
        id: 0,
        properties: Vec::new(),
        children,
    })
}

/// One entry of a bar menu: what it shows, and the id a click sends (`None`
/// for separators and headers, which do nothing when chosen).
type Entry = (MenuItem, Option<i32>);

/// The bar's view of an application's menu bar.
#[derive(Debug, Clone, Default)]
pub struct Bar {
    /// One spec per visible top-level submenu, without the System menu.
    pub specs: Vec<MenuSpec>,
    /// The id of each spec's top-level item, for `AboutToShow`.
    pub ids: Vec<i32>,
}

impl Bar {
    pub fn from_layout(layout: &Layout) -> Self {
        let mut bar = Self::default();
        for node in layout.items.iter().filter(|node| node.visible) {
            let title = &node.label.text;
            if !node.submenu || title.is_empty() {
                continue;
            }
            let mut entries = Vec::new();
            flatten(&node.children, 0, &mut entries);
            trim_separators(&mut entries);
            if entries.is_empty() {
                continue;
            }
            bar.specs.push(MenuSpec {
                title: title.clone(),
                items: entries
                    .into_iter()
                    .map(|(item, id)| (item, id.map(Command::DbusMenu)))
                    .collect(),
            });
            bar.ids.push(node.id);
        }
        bar
    }
}

/// Append the visible entries of `nodes`. A submenu inside a menu shows as a
/// disabled header followed by its entries, so its items are not mistaken for
/// the parent's.
fn flatten(nodes: &[crate::menu_registrar_layout::MenuNode], depth: usize, out: &mut Vec<Entry>) {
    for node in nodes.iter().filter(|node| node.visible) {
        if node.kind == Kind::Separator {
            out.push((MenuItem::separator(), None));
            continue;
        }
        let label = &node.label.text;
        if node.submenu {
            if !label.is_empty() {
                out.push((MenuItem::new(label).enabled(false), None));
            }
            if depth < MAX_BAR_DEPTH {
                flatten(&node.children, depth + 1, out);
            }
            continue;
        }
        if label.is_empty() {
            continue;
        }
        let mut item = MenuItem::new(label).enabled(node.enabled);
        if let Some(chord) = node.shortcuts.first() {
            item = item.shortcut(&chord.text());
        }
        if node.toggle.is_some() {
            item = item.checked(node.checked());
        }
        out.push((item, Some(node.id)));
    }
}

/// No leading, trailing or doubled separators.
fn trim_separators(entries: &mut Vec<Entry>) {
    let mut cleaned: Vec<Entry> = Vec::with_capacity(entries.len());
    for entry in entries.drain(..) {
        if entry.0.separator && cleaned.last().is_none_or(|(last, _)| last.separator) {
            continue;
        }
        cleaned.push(entry);
    }
    while cleaned.last().is_some_and(|(last, _)| last.separator) {
        cleaned.pop();
    }
    *entries = cleaned;
}

/// What the worker reports.
#[derive(Debug)]
pub enum Update {
    /// The parsed menu tree of `menu`.
    Layout(MenuRef, Layout),
    /// The exporter with this unique name changed its menus; fetch again.
    Changed(String),
    /// A call to `menu` failed.
    Failed(MenuRef, String),
}

enum Request {
    Fetch(MenuRef),
    /// `AboutToShow` for a top-level item; refetch when the app asks.
    Show(MenuRef, i32),
    Click(MenuRef, i32),
}

/// Fetches and clicks dbusmenu items off the UI thread.
pub struct Client {
    requests: Sender<Request>,
    pub updates: Receiver<Update>,
}

impl Client {
    /// Connect to `address` (the session bus when `None`). `wake` runs after
    /// every update.
    pub fn start(address: Option<&str>, wake: impl Fn() + Send + Sync + 'static) -> Option<Self> {
        let builder = match address {
            Some(address) => Builder::address(address),
            None => Builder::session(),
        };
        let connection = builder
            .and_then(|builder| builder.build())
            .map_err(|error| log::warn!("menu clients unavailable: {error}"))
            .ok()?;
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(wake);
        let (requests, receiver) = mpsc::channel::<Request>();
        let (sender, updates) = mpsc::channel::<Update>();

        let worker = connection.clone();
        let worker_sender = sender.clone();
        let worker_wake = wake.clone();
        thread::Builder::new()
            .name("slopos-dbusmenu".into())
            .spawn(move || {
                for request in receiver {
                    let update = match request {
                        Request::Fetch(menu) => match fetch(&worker, &menu) {
                            Ok(layout) => Update::Layout(menu, layout),
                            Err(error) => Update::Failed(menu, error),
                        },
                        Request::Show(menu, id) => match about_to_show(&worker, &menu, id) {
                            // The application says its menu changed; read it again.
                            Ok(true) => match fetch(&worker, &menu) {
                                Ok(layout) => Update::Layout(menu, layout),
                                Err(error) => Update::Failed(menu, error),
                            },
                            Ok(false) => continue,
                            Err(error) => Update::Failed(menu, error),
                        },
                        Request::Click(menu, id) => match click(&worker, &menu, id) {
                            Ok(()) => continue,
                            Err(error) => Update::Failed(menu, error),
                        },
                    };
                    if worker_sender.send(update).is_err() {
                        break;
                    }
                    worker_wake();
                }
            })
            .ok()?;

        let watcher = connection;
        thread::Builder::new()
            .name("slopos-dbusmenu-watch".into())
            .spawn(move || watch_changes(&watcher, &sender, wake.as_ref()))
            .ok()?;
        Some(Self { requests, updates })
    }

    /// Fetch (or re-fetch) `menu`'s tree; the answer arrives as an update.
    pub fn fetch(&self, menu: MenuRef) {
        let _ = self.requests.send(Request::Fetch(menu));
    }

    /// Tell `menu`'s application that top-level item `id` is about to open.
    pub fn about_to_show(&self, menu: MenuRef, id: i32) {
        let _ = self.requests.send(Request::Show(menu, id));
    }

    /// Send `Event(id, "clicked")` to `menu`'s application.
    pub fn click(&self, menu: MenuRef, id: i32) {
        let _ = self.requests.send(Request::Click(menu, id));
    }
}

fn proxy<'a>(connection: &'a Connection, menu: &'a MenuRef) -> zbus::Result<Proxy<'a>> {
    Proxy::new(
        connection,
        menu.service.as_str(),
        menu.path.as_str(),
        INTERFACE,
    )
}

fn fetch(connection: &Connection, menu: &MenuRef) -> Result<Layout, String> {
    let proxy = proxy(connection, menu).map_err(|error| error.to_string())?;
    // Parent 0 and depth -1: the whole tree, every property.
    let (revision, root): (u32, RootReply) = proxy
        .call("GetLayout", &(0_i32, -1_i32, Vec::<String>::new()))
        .map_err(|error| error.to_string())?;
    let root = root_from_reply(&root.2).ok_or("the menu layout is malformed")?;
    Layout::parse(revision, &root).map_err(|error| error.to_string())
}

fn about_to_show(connection: &Connection, menu: &MenuRef, id: i32) -> Result<bool, String> {
    let proxy = proxy(connection, menu).map_err(|error| error.to_string())?;
    proxy
        .call("AboutToShow", &(id,))
        .map_err(|error| error.to_string())
}

fn click(connection: &Connection, menu: &MenuRef, id: i32) -> Result<(), String> {
    let proxy = proxy(connection, menu).map_err(|error| error.to_string())?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u32);
    proxy
        .call::<_, _, ()>("Event", &(id, "clicked", Value::from(0_i32), timestamp))
        .map_err(|error| error.to_string())
}

/// Report exporters whose menus changed. Blocks on the bus.
fn watch_changes(connection: &Connection, sender: &Sender<Update>, wake: &dyn Fn()) {
    if let Ok(dbus) = zbus::blocking::fdo::DBusProxy::new(connection) {
        for member in ["LayoutUpdated", "ItemsPropertiesUpdated"] {
            let rule = zbus::MatchRule::builder()
                .msg_type(zbus::message::Type::Signal)
                .interface(INTERFACE)
                .and_then(|rule| rule.member(member))
                .map(|rule| rule.build());
            if let Ok(rule) = rule {
                let _ = dbus.add_match_rule(rule);
            }
        }
    }
    for message in MessageIterator::from(connection).flatten() {
        let header = message.header();
        if header.message_type() != zbus::message::Type::Signal
            || header.interface().map(|i| i.as_str()) != Some(INTERFACE)
        {
            continue;
        }
        let Some(sender_name) = header.sender().map(|s| s.to_string()) else {
            continue;
        };
        if sender.send(Update::Changed(sender_name)).is_err() {
            break;
        }
        wake();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> Value<'static> {
        Value::from(value.to_owned())
    }

    /// A node as the wire carries it, with its properties wrapped in `v`.
    fn wire(
        id: i32,
        props: Vec<(&str, Value<'static>)>,
        children: Vec<Value<'static>>,
    ) -> Value<'static> {
        let dict: HashMap<String, Value<'static>> = props
            .into_iter()
            .map(|(key, value)| (key.to_owned(), Value::Value(Box::new(value))))
            .collect();
        let children: Vec<Value<'static>> = children
            .into_iter()
            .map(|child| Value::Value(Box::new(child)))
            .collect();
        Value::from((id, dict, children))
    }

    fn layout_of(root_children: Vec<Value<'static>>) -> Layout {
        let root = root_from_reply(
            &root_children
                .into_iter()
                .map(|child| OwnedValue::try_from(child).unwrap())
                .collect::<Vec<_>>(),
        )
        .expect("decodes");
        Layout::parse(3, &root).expect("parses")
    }

    pub(super) fn file_menu() -> Value<'static> {
        wire(
            1,
            vec![
                ("label", text("_File")),
                ("children-display", text("submenu")),
            ],
            vec![
                wire(
                    2,
                    vec![
                        ("label", text("_Open")),
                        (
                            "shortcut",
                            Value::from(vec![vec!["Control".to_owned(), "o".to_owned()]]),
                        ),
                    ],
                    vec![],
                ),
                wire(3, vec![("type", text("separator"))], vec![]),
                wire(
                    4,
                    vec![
                        ("label", text("Show _Hidden")),
                        ("toggle-type", text("checkmark")),
                        ("toggle-state", Value::from(1_i32)),
                    ],
                    vec![],
                ),
                wire(
                    5,
                    vec![("label", text("_Gone")), ("enabled", Value::from(false))],
                    vec![],
                ),
                wire(
                    6,
                    vec![("label", text("_Hidden")), ("visible", Value::from(false))],
                    vec![],
                ),
            ],
        )
    }

    #[test]
    fn a_wire_node_decodes_its_id_properties_and_children() {
        let node = decode_node(&file_menu()).expect("decodes");
        assert_eq!(node.id, 1);
        assert!(node
            .properties
            .contains(&("label".to_owned(), Property::Str("_File".into()))));
        assert_eq!(node.children.len(), 5);
        assert!(node
            .properties
            .iter()
            .any(|(key, value)| key == "children-display"
                && *value == Property::Str("submenu".into())));
    }

    #[test]
    fn shortcuts_and_toggles_survive_the_decoder() {
        let node = decode_node(&file_menu()).expect("decodes");
        let open = &node.children[0];
        assert!(open.properties.contains(&(
            "shortcut".to_owned(),
            Property::StrLists(vec![vec!["Control".into(), "o".into()]])
        )));
        let hidden = &node.children[2];
        assert!(hidden
            .properties
            .contains(&("toggle-state".to_owned(), Property::Int(1))));
    }

    #[test]
    fn values_the_protocol_does_not_use_are_dropped() {
        let node = wire(
            9,
            vec![
                ("label", text("Kept")),
                ("data", Value::from(vec![1_u8, 2, 3])),
                ("weight", Value::from(2.5_f64)),
            ],
            vec![],
        );
        let node = decode_node(&node).expect("decodes");
        assert_eq!(node.properties.len(), 1);
        assert_eq!(node.properties[0].0, "label");
    }

    #[test]
    fn a_child_that_is_not_a_node_rejects_the_layout() {
        assert!(decode_node(&Value::from(7_i32)).is_none());
        // A node whose children are not nodes.
        let bad = Value::from((
            1_i32,
            HashMap::<String, Value<'static>>::new(),
            vec![Value::from(3_i32)],
        ));
        assert!(decode_node(&bad).is_none());
        // A child of the wrong shape inside the root rejects the root.
        assert!(root_from_reply(&[OwnedValue::try_from(Value::from(3_i32)).unwrap()]).is_none());
    }

    #[test]
    fn bar_titles_items_shortcuts_and_clickable_ids_follow_the_layout() {
        let layout = layout_of(vec![file_menu()]);
        let bar = Bar::from_layout(&layout);
        assert_eq!(bar.ids, vec![1]);
        assert_eq!(bar.specs.len(), 1);
        let spec = &bar.specs[0];
        assert_eq!(spec.title, "File");
        let labels: Vec<&str> = spec
            .items
            .iter()
            .map(|(item, _)| item.label.as_str())
            .collect();
        // The hidden item is gone; the disabled item stays, greyed out.
        assert_eq!(labels, ["Open", "", "Show Hidden", "Gone"]);
        let (open, open_command) = &spec.items[0];
        assert_eq!(open.shortcut.as_deref(), Some("Ctrl+O"));
        assert_eq!(*open_command, Some(Command::DbusMenu(2)));
        let (separator, separator_command) = &spec.items[1];
        assert!(separator.separator && separator_command.is_none());
        let (check, check_command) = &spec.items[2];
        assert!(check.checkable && check.checked);
        assert_eq!(*check_command, Some(Command::DbusMenu(4)));
        let (gone, gone_command) = &spec.items[3];
        assert!(!gone.enabled);
        assert_eq!(*gone_command, Some(Command::DbusMenu(5)));
    }

    #[test]
    fn top_level_items_that_are_not_submenus_and_empty_menus_are_not_titles() {
        let plain = wire(20, vec![("label", text("Quit"))], vec![]);
        let empty = wire(
            21,
            vec![
                ("label", text("Empty")),
                ("children-display", text("submenu")),
            ],
            vec![],
        );
        let bar = Bar::from_layout(&layout_of(vec![plain, empty, file_menu()]));
        assert_eq!(bar.specs.len(), 1);
        assert_eq!(bar.specs[0].title, "File");
        assert_eq!(bar.ids, vec![1]);
    }

    #[test]
    fn nested_submenus_show_a_header_and_no_separator_runs() {
        let nested = wire(
            30,
            vec![
                ("label", text("Edit")),
                ("children-display", text("submenu")),
            ],
            vec![
                wire(31, vec![("type", text("separator"))], vec![]),
                wire(
                    32,
                    vec![
                        ("label", text("Recent")),
                        ("children-display", text("submenu")),
                    ],
                    vec![wire(33, vec![("label", text("a.txt"))], vec![])],
                ),
                wire(34, vec![("type", text("separator"))], vec![]),
                wire(35, vec![("type", text("separator"))], vec![]),
            ],
        );
        let bar = Bar::from_layout(&layout_of(vec![nested]));
        let items = &bar.specs[0].items;
        let labels: Vec<&str> = items.iter().map(|(item, _)| item.label.as_str()).collect();
        assert_eq!(labels, ["Recent", "a.txt"]);
        assert!(!items[0].0.enabled && items[0].1.is_none());
        assert_eq!(items[1].1, Some(Command::DbusMenu(33)));
    }

    #[test]
    fn a_menu_deeper_than_the_bar_shows_stops_flattening() {
        let mut deep = wire(40, vec![("label", text("Leaf"))], vec![]);
        for id in (41..50).rev() {
            deep = wire(
                id,
                vec![
                    ("label", text("Level")),
                    ("children-display", text("submenu")),
                ],
                vec![deep],
            );
        }
        let top = wire(
            39,
            vec![
                ("label", text("Deep")),
                ("children-display", text("submenu")),
            ],
            vec![deep],
        );
        let bar = Bar::from_layout(&layout_of(vec![top]));
        let has_leaf = bar.specs[0]
            .items
            .iter()
            .any(|(item, _)| item.label == "Leaf");
        assert!(!has_leaf, "entries below the depth limit are not shown");
    }
}

/// Registrar and client over a private `dbus-daemon`, with a fake application
/// exporting a menu. Prints NOT_RUN when `dbus-daemon` is absent.
#[cfg(test)]
mod bus_tests {
    use super::tests::file_menu;
    use super::*;
    use crate::menu_registrar::Effect;
    use crate::registrar_bus;
    use std::io::{BufRead, BufReader};
    use std::process::{Child, Command as Process, Stdio};
    use std::sync::mpsc;
    use std::time::Duration;
    use zbus::blocking::connection::Builder as ConnectionBuilder;
    use zbus::zvariant::ObjectPath;

    const WAIT: Duration = Duration::from_secs(10);
    const WINDOW: u32 = 0x40_0001;
    const MENU_PATH: &str = "/com/example/MenuBar";

    struct Daemon(Child);

    impl Drop for Daemon {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn private_bus() -> Option<(Daemon, String)> {
        let mut daemon = Process::new("dbus-daemon")
            .args(["--session", "--print-address", "--nofork"])
            .stdout(Stdio::piped())
            .spawn()
            .ok()?;
        let mut address = String::new();
        BufReader::new(daemon.stdout.take()?)
            .read_line(&mut address)
            .ok()?;
        Some((Daemon(daemon), address.trim().to_owned()))
    }

    /// An application's dbusmenu server. Clicks go to `clicks`; `AboutToShow`
    /// reports that the menu changed.
    struct FakeMenu {
        clicks: mpsc::Sender<i32>,
    }

    #[zbus::interface(name = "com.canonical.dbusmenu")]
    impl FakeMenu {
        fn get_layout(
            &self,
            _parent_id: i32,
            _recursion_depth: i32,
            _property_names: Vec<String>,
        ) -> (u32, RootReply) {
            let file = OwnedValue::try_from(file_menu()).expect("a plain structure");
            (7, (0, HashMap::new(), vec![file]))
        }

        fn event(&self, id: i32, event_id: &str, _data: Value<'_>, _timestamp: u32) {
            if event_id == "clicked" {
                let _ = self.clicks.send(id);
            }
        }

        fn about_to_show(&self, _id: i32) -> bool {
            true
        }
    }

    #[test]
    fn registrar_and_client_show_a_registered_application_menu() {
        let Some((_daemon, address)) = private_bus() else {
            eprintln!(
                "NOT_RUN: dbus-daemon is not installed; the D-Bus integration test was skipped"
            );
            return;
        };
        let registrar = registrar_bus::start(Some(address.as_str()), || {})
            .expect("the registrar owns its name");

        // A listener that reports the registrar's signals as they arrive.
        let (signals, heard) = mpsc::channel::<String>();
        let listener = ConnectionBuilder::address(address.as_str())
            .and_then(|builder| builder.build())
            .expect("a listener connection");
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface(registrar_bus::SERVICE)
            .map(|rule| rule.build())
            .expect("a match rule");
        let dbus = zbus::blocking::fdo::DBusProxy::new(&listener).expect("a bus proxy");
        dbus.add_match_rule(rule).expect("the match rule is added");
        let listening = listener.clone();
        thread::spawn(move || {
            for message in zbus::blocking::MessageIterator::from(&listening).flatten() {
                if let Some(member) = message.header().member() {
                    let _ = signals.send(member.to_string());
                }
            }
        });

        // The application: its own connection, serving its menu at MENU_PATH.
        let (clicks, clicked) = mpsc::channel();
        let application = ConnectionBuilder::address(address.as_str())
            .and_then(|builder| builder.serve_at(MENU_PATH, FakeMenu { clicks }))
            .and_then(|builder| builder.build())
            .expect("the fake application connects");
        let service = application
            .unique_name()
            .expect("a unique name")
            .to_string();
        let registrar_proxy = zbus::blocking::Proxy::new(
            &application,
            registrar_bus::SERVICE,
            registrar_bus::PATH,
            registrar_bus::SERVICE,
        )
        .expect("a proxy for the registrar");
        registrar_proxy
            .call::<_, _, ()>(
                "RegisterWindow",
                &(WINDOW, ObjectPath::try_from(MENU_PATH).unwrap()),
            )
            .expect("RegisterWindow is accepted");

        // The bar learns of the registration, and the bus carries the signal.
        let menu = MenuRef {
            service: service.clone(),
            path: MENU_PATH.to_owned(),
        };
        assert_eq!(
            registrar.effects.recv_timeout(WAIT),
            Ok(Effect::Registered {
                window: WINDOW,
                menu: menu.clone(),
            })
        );
        assert_eq!(
            heard.recv_timeout(WAIT).as_deref(),
            Ok("WindowRegistered"),
            "the registrar emits WindowRegistered on the bus"
        );
        assert_eq!(registrar.resolve(WINDOW, None), Some(menu.clone()));

        // The client fetches the layout, and the bar builds its menus from it.
        let client = Client::start(Some(address.as_str()), || {}).expect("the client connects");
        client.fetch(menu.clone());
        let layout = match client.updates.recv_timeout(WAIT) {
            Ok(Update::Layout(from, layout)) => {
                assert_eq!(from, menu);
                layout
            }
            other => panic!("no layout from the application: {other:?}"),
        };
        let bar = Bar::from_layout(&layout);
        assert_eq!(bar.specs.len(), 1);
        assert_eq!(bar.specs[0].title, "File");
        assert_eq!(bar.ids, vec![1]);
        let (open, open_command) = &bar.specs[0].items[0];
        assert_eq!(open.label, "Open");
        assert_eq!(open.shortcut.as_deref(), Some("Ctrl+O"));
        assert_eq!(*open_command, Some(Command::DbusMenu(2)));

        // Opening a top-level item asks the application, which answers with a
        // fresh layout.
        client.about_to_show(menu.clone(), 1);
        assert!(matches!(
            client.updates.recv_timeout(WAIT),
            Ok(Update::Layout(..))
        ));

        // A click reaches the application as Event(id, "clicked").
        client.click(menu.clone(), 2);
        assert_eq!(clicked.recv_timeout(WAIT), Ok(2));

        // When the application leaves the bus, its registration goes with it.
        // The proxy holds its own clone of the connection, so it goes first.
        drop(registrar_proxy);
        drop(application);
        assert_eq!(
            registrar.effects.recv_timeout(WAIT),
            Ok(Effect::Unregistered { window: WINDOW })
        );
        assert_eq!(registrar.resolve(WINDOW, None), None);
    }
}
