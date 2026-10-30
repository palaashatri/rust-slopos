//! Export an app's [`MenuBar`] to the desktop's global menu bar.
//!
//! The SLOPOS shell shows the focused window's menus in its top bar by
//! reading the GTK/GIO menu protocol: the window names a D-Bus connection
//! and object paths in `_GTK_UNIQUE_BUS_NAME`, `_GTK_MENUBAR_OBJECT_PATH`
//! and `_GTK_APPLICATION_OBJECT_PATH`, and the app serves `org.gtk.Menus`
//! (the menu structure) and `org.gtk.Actions` (enabled/checked state and
//! activation) there. This module serves both for a toolkit [`MenuBar`], so
//! a SLOPOS app's menus appear in the shell exactly like a GtkApplication's.
//!
//! The menus move to the top bar only while the shell holds the
//! [`SHELL_NAME`] bus name; the app gets [`crate::event::Event::GlobalMenu`]
//! whenever that changes and draws its own in-window bar otherwise, so a
//! crashed or absent shell never leaves an app without menus.
//!
//! The layout and action tables are plain functions over [`Snapshot`] and
//! are tested without a bus.

use std::collections::HashMap;

use crate::controls::menu::{MenuBar, MenuItem};

/// The well-known name the shell owns while it shows global menus.
pub const SHELL_NAME: &str = "org.slopos.GlobalMenu";

/// The plain menu content the bus sees: titles plus items.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Snapshot {
    pub titles: Vec<String>,
    pub menus: Vec<Vec<MenuItem>>,
}

impl Snapshot {
    pub fn of(bar: &MenuBar) -> Self {
        Self {
            titles: bar.titles.clone(),
            menus: bar.menus.iter().map(|menu| menu.items.clone()).collect(),
        }
    }

    /// Whether the menu structure differs (labels, shortcuts, separators),
    /// as opposed to only enabled/checked state.
    pub fn structure_differs(&self, other: &Self) -> bool {
        self.titles != other.titles
            || self.menus.len() != other.menus.len()
            || self.menus.iter().zip(&other.menus).any(|(a, b)| {
                a.len() != b.len()
                    || a.iter().zip(b).any(|(a, b)| {
                        a.label != b.label
                            || a.shortcut != b.shortcut
                            || a.separator != b.separator
                            || checkable(a) != checkable(b)
                    })
            })
    }
}

fn checkable(item: &MenuItem) -> bool {
    item.checkable || item.checked
}

/// One attribute or link of an exported menu item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attribute {
    Text(String),
    /// A `:section` or `:submenu` link to (group, menu).
    Link(u32, u32),
}

/// An exported menu item: attribute name to value.
pub type Item = Vec<(&'static str, Attribute)>;

/// The `org.gtk.Menus` content of group 0, keyed by menu id. Menu 0 is the
/// bar; menu `1 + i` is title `i`'s pull-down, made of sections split at
/// separators; sections use ids from `1 + titles.len()` on.
pub fn layout(snapshot: &Snapshot) -> Vec<(u32, Vec<Item>)> {
    let count = snapshot.titles.len() as u32;
    let mut bar = Vec::new();
    let mut menus = Vec::new();
    let mut sections = Vec::new();
    let mut next_section = 1 + count;
    for (index, title) in snapshot.titles.iter().enumerate() {
        let menu_id = 1 + index as u32;
        bar.push(vec![
            ("label", Attribute::Text(escape_mnemonic(title))),
            (":submenu", Attribute::Link(0, menu_id)),
        ]);
        let items = snapshot.menus.get(index).map(Vec::as_slice).unwrap_or(&[]);
        let mut links = Vec::new();
        for group in split_sections(items) {
            let section_id = next_section;
            next_section += 1;
            links.push(vec![(":section", Attribute::Link(0, section_id))]);
            let content = group
                .into_iter()
                .map(|(item_index, item)| {
                    let mut attributes = vec![
                        ("label", Attribute::Text(escape_mnemonic(&item.label))),
                        (
                            "action",
                            Attribute::Text(format!("app.{}", action_name(index, item_index))),
                        ),
                    ];
                    if let Some(accel) = item.shortcut.as_deref().and_then(accelerator) {
                        attributes.push(("accel", Attribute::Text(accel)));
                    }
                    attributes
                })
                .collect();
            sections.push((section_id, content));
        }
        menus.push((menu_id, links));
    }
    let mut all = vec![(0, bar)];
    all.extend(menus);
    all.extend(sections);
    all
}

/// Group a menu's items into separator-delimited runs, dropping empty runs.
fn split_sections(items: &[MenuItem]) -> Vec<Vec<(usize, &MenuItem)>> {
    let mut sections = vec![Vec::new()];
    for (index, item) in items.iter().enumerate() {
        if item.separator {
            sections.push(Vec::new());
        } else if let Some(last) = sections.last_mut() {
            last.push((index, item));
        }
    }
    sections.retain(|section| !section.is_empty());
    sections
}

/// The action for menu `menu`, item `item`.
pub fn action_name(menu: usize, item: usize) -> String {
    format!("menu-{menu}-{item}")
}

/// Parse [`action_name`] back.
pub fn parse_action(name: &str) -> Option<(usize, usize)> {
    let (menu, item) = name.strip_prefix("menu-")?.split_once('-')?;
    Some((menu.parse().ok()?, item.parse().ok()?))
}

/// An `org.gtk.Actions` description: enabled, plus a boolean state for
/// check items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionState {
    pub enabled: bool,
    pub checked: Option<bool>,
}

pub fn actions(snapshot: &Snapshot) -> Vec<(String, ActionState)> {
    let mut actions = Vec::new();
    for (menu, items) in snapshot.menus.iter().enumerate() {
        for (index, item) in items.iter().enumerate() {
            if item.separator {
                continue;
            }
            actions.push((
                action_name(menu, index),
                ActionState {
                    enabled: item.enabled,
                    checked: checkable(item).then_some(item.checked),
                },
            ));
        }
    }
    actions
}

/// Action names with a new enabled flag or check state.
pub type Changes = Vec<(String, bool)>;

/// The enable and state changes between two snapshots of the same structure.
pub fn action_changes(old: &Snapshot, new: &Snapshot) -> (Changes, Changes) {
    let old: HashMap<String, ActionState> = actions(old).into_iter().collect();
    let mut enabled = Vec::new();
    let mut states = Vec::new();
    for (name, state) in actions(new) {
        let Some(before) = old.get(&name) else {
            continue;
        };
        if before.enabled != state.enabled {
            enabled.push((name.clone(), state.enabled));
        }
        if let (Some(was), Some(now)) = (before.checked, state.checked) {
            if was != now {
                states.push((name, now));
            }
        }
    }
    (enabled, states)
}

/// GTK reads `_` in a menu label as a mnemonic marker.
fn escape_mnemonic(label: &str) -> String {
    label.replace('_', "__")
}

/// Convert shortcut text such as "Ctrl+Shift+N" to a GTK accelerator
/// ("<Control><Shift>n"). Unknown keys give `None`; the item then shows no
/// shortcut rather than a wrong one.
pub fn accelerator(text: &str) -> Option<String> {
    let parts: Vec<&str> = text.split('+').collect();
    // "Ctrl++" splits into ["Ctrl", "", ""]: the key is a literal plus.
    let (modifiers, key) = if text.ends_with("++") {
        (&parts[..parts.len() - 2], "+")
    } else if text == "+" {
        (&[][..], "+")
    } else {
        let (key, modifiers) = parts.split_last()?;
        (modifiers, *key)
    };
    let mut accel = String::new();
    for modifier in modifiers {
        accel.push_str(match *modifier {
            "Ctrl" | "Control" => "<Control>",
            "Shift" => "<Shift>",
            "Alt" => "<Alt>",
            "Super" => "<Super>",
            _ => return None,
        });
    }
    let name = match key {
        "Delete" | "Del" => "Delete".to_owned(),
        "Backspace" => "BackSpace".to_owned(),
        "Enter" | "Return" => "Return".to_owned(),
        "Esc" | "Escape" => "Escape".to_owned(),
        "Space" => "space".to_owned(),
        "Tab" => "Tab".to_owned(),
        "Left" | "Right" | "Up" | "Down" | "Home" | "End" => key.to_owned(),
        "PageUp" | "Page Up" => "Page_Up".to_owned(),
        "PageDown" | "Page Down" => "Page_Down".to_owned(),
        "+" | "Plus" => "plus".to_owned(),
        "-" | "−" | "Minus" => "minus".to_owned(),
        "=" => "equal".to_owned(),
        "," => "comma".to_owned(),
        "." => "period".to_owned(),
        "/" => "slash".to_owned(),
        _ if key.len() > 1 && key.starts_with('F') && key[1..].parse::<u8>().is_ok() => {
            key.to_owned()
        }
        _ => {
            let mut chars = key.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if c.is_ascii_alphanumeric() => c.to_ascii_lowercase().to_string(),
                _ => return None,
            }
        }
    };
    accel.push_str(&name);
    Some(accel)
}

/// A request from the bus for the app's event loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {
    /// The shell activated menu `menu`, item `item`.
    Activate { menu: usize, item: usize },
    /// The shell started (`true`) or stopped showing global menus.
    ShellPresent(bool),
}

/// Turn a D-Bus object path component out of an app id ("slopos-files").
pub fn path_component(app_id: &str) -> String {
    let component: String = app_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if component.is_empty() {
        "app".to_owned()
    } else {
        component
    }
}

#[cfg(feature = "global-menu")]
pub use bus::Exporter;

#[cfg(feature = "global-menu")]
mod bus {
    use std::collections::HashMap;
    use std::sync::mpsc::Sender;
    use std::sync::{Arc, Mutex};
    use std::thread;

    use zbus::blocking::Connection;
    use zbus::zvariant::{OwnedValue, Signature, Value};
    use zbus::SignalContext;

    use super::{
        action_changes, actions, layout, parse_action, path_component, ActionState, Attribute,
        Message, Snapshot, SHELL_NAME,
    };
    use crate::platform::Proxy;

    type Items = Vec<HashMap<String, OwnedValue>>;
    type Description = (bool, Signature<'static>, Vec<OwnedValue>);

    struct Shared {
        snapshot: Mutex<Snapshot>,
        sender: Mutex<Sender<Message>>,
        proxy: Proxy,
    }

    impl Shared {
        fn send(&self, message: Message) {
            if let Ok(sender) = self.sender.lock() {
                let _ = sender.send(message);
            }
            self.proxy.wake();
        }

        fn snapshot(&self) -> Snapshot {
            self.snapshot
                .lock()
                .map(|snapshot| snapshot.clone())
                .unwrap_or_default()
        }
    }

    fn value(attribute: &Attribute) -> Option<OwnedValue> {
        let value = match attribute {
            Attribute::Text(text) => Value::from(text.as_str()),
            Attribute::Link(group, menu) => Value::from((*group, *menu)),
        };
        OwnedValue::try_from(value).ok()
    }

    fn items(items: &[super::Item]) -> Items {
        items
            .iter()
            .map(|item| {
                item.iter()
                    .filter_map(|(name, attribute)| Some(((*name).to_owned(), value(attribute)?)))
                    .collect()
            })
            .collect()
    }

    struct Menus(Arc<Shared>);

    #[zbus::interface(name = "org.gtk.Menus")]
    impl Menus {
        fn start(&self, groups: Vec<u32>) -> Vec<(u32, u32, Items)> {
            if !groups.contains(&0) {
                return Vec::new();
            }
            layout(&self.0.snapshot())
                .iter()
                .map(|(menu, content)| (0, *menu, items(content)))
                .collect()
        }

        fn end(&self, _groups: Vec<u32>) {}

        #[zbus(signal)]
        async fn changed(
            context: &SignalContext<'_>,
            changes: Vec<(u32, u32, u32, u32, Items)>,
        ) -> zbus::Result<()>;
    }

    struct Actions(Arc<Shared>);

    impl Actions {
        fn table(&self) -> HashMap<String, ActionState> {
            actions(&self.0.snapshot()).into_iter().collect()
        }
    }

    #[zbus::interface(name = "org.gtk.Actions")]
    impl Actions {
        fn list(&self) -> Vec<String> {
            actions(&self.0.snapshot())
                .into_iter()
                .map(|(name, _)| name)
                .collect()
        }

        fn describe(&self, name: String) -> zbus::fdo::Result<Description> {
            self.table()
                .get(&name)
                .map(|state| description(*state))
                .ok_or_else(|| zbus::fdo::Error::InvalidArgs(format!("no action {name}")))
        }

        fn describe_all(&self) -> HashMap<String, Description> {
            self.table()
                .into_iter()
                .map(|(name, state)| (name, description(state)))
                .collect()
        }

        fn activate(
            &self,
            name: String,
            _parameter: Vec<OwnedValue>,
            _platform_data: HashMap<String, OwnedValue>,
        ) {
            let enabled = self.table().get(&name).is_some_and(|state| state.enabled);
            if let (true, Some((menu, item))) = (enabled, parse_action(&name)) {
                self.0.send(Message::Activate { menu, item });
            }
        }

        fn set_state(
            &self,
            name: String,
            value: OwnedValue,
            _platform_data: HashMap<String, OwnedValue>,
        ) {
            // A check item toggles through its own command.
            let wanted = bool::try_from(value).ok();
            let state = self.table().get(&name).copied();
            if let (Some(wanted), Some(state), Some((menu, item))) =
                (wanted, state, parse_action(&name))
            {
                if state.enabled && state.checked.is_some_and(|checked| checked != wanted) {
                    self.0.send(Message::Activate { menu, item });
                }
            }
        }

        #[zbus(signal)]
        #[allow(clippy::type_complexity)]
        async fn changed(
            context: &SignalContext<'_>,
            removals: Vec<String>,
            enable_changes: HashMap<String, bool>,
            state_changes: HashMap<String, OwnedValue>,
            additions: HashMap<String, Description>,
        ) -> zbus::Result<()>;
    }

    fn description(state: ActionState) -> Description {
        let value = state
            .checked
            .and_then(|checked| OwnedValue::try_from(Value::from(checked)).ok());
        (
            state.enabled,
            Signature::from_static_str_unchecked(""),
            value.into_iter().collect(),
        )
    }

    /// Serves one window's menus on the session bus.
    pub struct Exporter {
        connection: Connection,
        shared: Arc<Shared>,
        menu_path: String,
        action_path: String,
        shell_present: bool,
    }

    impl Exporter {
        /// Connect to the session bus and serve `snapshot`. `None` when there
        /// is no session bus; the app then keeps its in-window menus.
        pub fn start(
            app_id: &str,
            snapshot: Snapshot,
            sender: Sender<Message>,
            proxy: Proxy,
        ) -> Option<Self> {
            let base = format!("/org/slopos/{}", path_component(app_id));
            let menu_path = format!("{base}/menus/MenuBar");
            let shared = Arc::new(Shared {
                snapshot: Mutex::new(snapshot),
                sender: Mutex::new(sender),
                proxy,
            });
            let connection = zbus::blocking::connection::Builder::session()
                .and_then(|builder| builder.serve_at(menu_path.as_str(), Menus(shared.clone())))
                .and_then(|builder| builder.serve_at(base.as_str(), Actions(shared.clone())))
                .and_then(|builder| builder.build());
            let connection = match connection {
                Ok(connection) => connection,
                Err(error) => {
                    log::info!("global menu unavailable, using in-window menus: {error}");
                    return None;
                }
            };
            let shell_present = watch_shell(&connection, shared.clone());
            Some(Self {
                connection,
                shared,
                menu_path,
                action_path: base,
                shell_present,
            })
        }

        /// Whether the shell held [`SHELL_NAME`] when the export started.
        pub fn shell_present(&self) -> bool {
            self.shell_present
        }

        pub fn unique_name(&self) -> Option<String> {
            self.connection.unique_name().map(|name| name.to_string())
        }

        pub fn menu_path(&self) -> &str {
            &self.menu_path
        }

        pub fn action_path(&self) -> &str {
            &self.action_path
        }

        /// Publish `next` if it differs from what the bus last saw.
        pub fn update(&self, next: Snapshot) {
            let previous = {
                let Ok(mut current) = self.shared.snapshot.lock() else {
                    return;
                };
                if *current == next {
                    return;
                }
                std::mem::replace(&mut *current, next.clone())
            };
            if previous.structure_differs(&next) {
                self.publish_structure(&previous, &next);
            } else {
                self.publish_states(&previous, &next);
            }
        }

        fn publish_structure(&self, previous: &Snapshot, next: &Snapshot) {
            let old: HashMap<u32, usize> = layout(previous)
                .into_iter()
                .map(|(menu, content)| (menu, content.len()))
                .collect();
            let changes = layout(next)
                .iter()
                .map(|(menu, content)| {
                    let removed = old.get(menu).copied().unwrap_or(0) as u32;
                    (0, *menu, 0, removed, items(content))
                })
                .collect();
            if let Ok(menus) = self
                .connection
                .object_server()
                .interface::<_, Menus>(self.menu_path.as_str())
            {
                let _ = zbus::block_on(Menus::changed(menus.signal_context(), changes));
            }
            let removals: Vec<String> = actions(previous).into_iter().map(|(n, _)| n).collect();
            let additions = actions(next)
                .into_iter()
                .map(|(name, state)| (name, description(state)))
                .collect();
            self.emit_actions(removals, HashMap::new(), HashMap::new(), additions);
        }

        fn publish_states(&self, previous: &Snapshot, next: &Snapshot) {
            let (enabled, states) = action_changes(previous, next);
            if enabled.is_empty() && states.is_empty() {
                return;
            }
            let states = states
                .into_iter()
                .filter_map(|(name, on)| Some((name, OwnedValue::try_from(Value::from(on)).ok()?)))
                .collect();
            self.emit_actions(
                Vec::new(),
                enabled.into_iter().collect(),
                states,
                HashMap::new(),
            );
        }

        fn emit_actions(
            &self,
            removals: Vec<String>,
            enabled: HashMap<String, bool>,
            states: HashMap<String, OwnedValue>,
            additions: HashMap<String, Description>,
        ) {
            if let Ok(group) = self
                .connection
                .object_server()
                .interface::<_, Actions>(self.action_path.as_str())
            {
                let _ = zbus::block_on(Actions::changed(
                    group.signal_context(),
                    removals,
                    enabled,
                    states,
                    additions,
                ));
            }
        }
    }

    /// Report whether the shell owns [`SHELL_NAME`] now, and send
    /// [`Message::ShellPresent`] when that changes. The watcher thread
    /// blocks on the bus's NameOwnerChanged signal.
    fn watch_shell(connection: &Connection, shared: Arc<Shared>) -> bool {
        let Ok(dbus) = zbus::blocking::fdo::DBusProxy::new(connection) else {
            return false;
        };
        let changes = dbus.receive_name_owner_changed_with_args(&[(0, SHELL_NAME)]);
        let present = SHELL_NAME
            .try_into()
            .ok()
            .and_then(|name| dbus.name_has_owner(name).ok())
            .unwrap_or(false);
        if let Ok(changes) = changes {
            let _ = thread::Builder::new()
                .name("slopos-global-menu".into())
                .spawn(move || {
                    for signal in changes {
                        if let Ok(args) = signal.args() {
                            let present = args.new_owner().is_some();
                            shared.send(Message::ShellPresent(present));
                        }
                    }
                });
        }
        present
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> Snapshot {
        Snapshot {
            titles: vec!["File".into(), "View".into()],
            menus: vec![
                vec![
                    MenuItem::new("Open").shortcut("Ctrl+O"),
                    MenuItem::separator(),
                    MenuItem::new("Close_Window").shortcut("Ctrl+W"),
                ],
                vec![MenuItem::new("Hidden Files")
                    .shortcut("Ctrl+H")
                    .checkable()
                    .enabled(false)],
            ],
        }
    }

    #[test]
    fn layout_links_bar_menus_and_sections() {
        let layout = layout(&snapshot());
        let ids: Vec<u32> = layout.iter().map(|(id, _)| *id).collect();
        // Bar, two pull-downs, then File's two sections and View's one.
        assert_eq!(ids, vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(
            layout[0].1[0],
            vec![
                ("label", Attribute::Text("File".into())),
                (":submenu", Attribute::Link(0, 1)),
            ]
        );
        assert_eq!(
            layout[1].1,
            vec![
                vec![(":section", Attribute::Link(0, 3))],
                vec![(":section", Attribute::Link(0, 4))],
            ]
        );
        assert_eq!(
            layout[4].1[0],
            vec![
                ("label", Attribute::Text("Close__Window".into())),
                ("action", Attribute::Text("app.menu-0-2".into())),
                ("accel", Attribute::Text("<Control>w".into())),
            ]
        );
    }

    #[test]
    fn actions_carry_enabled_and_check_state() {
        let mut snapshot = snapshot();
        assert_eq!(
            actions(&snapshot),
            vec![
                (
                    "menu-0-0".to_owned(),
                    ActionState {
                        enabled: true,
                        checked: None
                    }
                ),
                (
                    "menu-0-2".to_owned(),
                    ActionState {
                        enabled: true,
                        checked: None
                    }
                ),
                (
                    "menu-1-0".to_owned(),
                    ActionState {
                        enabled: false,
                        checked: Some(false)
                    }
                ),
            ]
        );
        let before = snapshot.clone();
        snapshot.menus[1][0].enabled = true;
        snapshot.menus[1][0].checked = true;
        assert!(!before.structure_differs(&snapshot));
        assert_eq!(
            action_changes(&before, &snapshot),
            (
                vec![("menu-1-0".to_owned(), true)],
                vec![("menu-1-0".to_owned(), true)]
            )
        );
        snapshot.menus[0][0].label = "Open…".into();
        assert!(before.structure_differs(&snapshot));
    }

    #[test]
    fn action_names_round_trip() {
        assert_eq!(parse_action(&action_name(3, 12)), Some((3, 12)));
        assert_eq!(parse_action("menu-x-1"), None);
        assert_eq!(parse_action("quit"), None);
    }

    #[test]
    fn shortcuts_become_gtk_accelerators() {
        assert_eq!(accelerator("Ctrl+O").as_deref(), Some("<Control>o"));
        assert_eq!(
            accelerator("Ctrl+Shift+N").as_deref(),
            Some("<Control><Shift>n")
        );
        assert_eq!(
            accelerator("Shift+Delete").as_deref(),
            Some("<Shift>Delete")
        );
        assert_eq!(accelerator("Alt+Left").as_deref(), Some("<Alt>Left"));
        assert_eq!(accelerator("F2").as_deref(), Some("F2"));
        assert_eq!(accelerator("+").as_deref(), Some("plus"));
        assert_eq!(accelerator("Ctrl++").as_deref(), Some("<Control>plus"));
        assert_eq!(accelerator("−").as_deref(), Some("minus"));
        assert_eq!(accelerator("0").as_deref(), Some("0"));
        assert_eq!(accelerator("Hyper+X"), None);
        assert_eq!(accelerator("Ctrl+Ünknown"), None);
    }

    #[test]
    fn object_paths_are_valid_dbus_components() {
        assert_eq!(path_component("slopos-files"), "slopos_files");
        assert_eq!(path_component(""), "app");
    }
}
