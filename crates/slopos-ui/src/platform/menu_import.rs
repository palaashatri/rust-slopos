//! Read another application's menus from the session bus: the client side
//! of [`super::global_menu`], used by the desktop's top bar.
//!
//! Applications (SLOPOS toolkit apps and GtkApplications alike) export a
//! GMenuModel over `org.gtk.Menus` and its actions over `org.gtk.Actions`.
//! [`Importer`] runs one worker thread that fetches a window's menus
//! (`Start`, following submenu groups), reads every action's enabled and
//! check state (`DescribeAll`) and activates items; a second thread watches
//! `Changed` signals from every exporter. Both only wake the caller, which
//! asks for a refresh when the change came from the window it shows.
//!
//! The menu data handed out ([`Imported`]) is toolkit-owned: titles and
//! [`MenuItem`]s, plus an opaque action reference per item.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use zbus::blocking::Connection;
use zbus::zvariant::{OwnedValue, Value};

use crate::controls::menu::MenuItem;

/// Where a window's menus are exported (from its `_GTK_*` properties).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MenuSource {
    pub bus_name: String,
    pub menu_path: String,
    /// `app.` actions.
    pub app_path: Option<String>,
    /// `win.` actions.
    pub window_path: Option<String>,
}

/// The action an item triggers, kept opaque to callers.
#[derive(Debug, PartialEq)]
pub struct ActionRef {
    /// "app" or "win".
    scope: String,
    name: String,
    target: Option<OwnedValue>,
}

impl Clone for ActionRef {
    fn clone(&self) -> Self {
        Self {
            scope: self.scope.clone(),
            name: self.name.clone(),
            // Targets are plain values (strings, numbers); file descriptors
            // never appear in menu targets, so a failed clone just drops it.
            target: self.target.as_ref().and_then(|t| t.try_clone().ok()),
        }
    }
}

/// One application's menu bar.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Imported {
    pub titles: Vec<String>,
    pub menus: Vec<Vec<MenuItem>>,
    actions: Vec<Vec<Option<ActionRef>>>,
}

impl Imported {
    pub fn is_empty(&self) -> bool {
        self.titles.is_empty()
    }

    /// The action behind menu `menu`, item `item`.
    pub fn action(&self, menu: usize, item: usize) -> Option<&ActionRef> {
        self.actions.get(menu)?.get(item)?.as_ref()
    }
}

/// An exported item: its attributes and links, as `Start` returns them.
pub type RawItem = HashMap<String, OwnedValue>;
/// Menus by (group, menu).
pub type RawMenus = HashMap<(u32, u32), Vec<RawItem>>;

/// An action's state from `DescribeAll`.
#[derive(Debug, PartialEq)]
pub struct ActionInfo {
    pub enabled: bool,
    pub state: Option<OwnedValue>,
}

/// "_Open" → "Open", "Save __As" → "Save _As".
pub fn strip_mnemonic(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut chars = label.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '_' {
            if chars.peek() == Some(&'_') {
                out.push('_');
                chars.next();
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// "<Primary><Shift>n" → "Ctrl+Shift+N"; unknown forms pass through.
pub fn shortcut_text(accel: &str) -> String {
    let mut rest = accel;
    let mut parts: Vec<String> = Vec::new();
    while let Some(stripped) = rest.strip_prefix('<') {
        let Some(end) = stripped.find('>') else {
            break;
        };
        let name = &stripped[..end];
        parts.push(
            match name.to_ascii_lowercase().as_str() {
                "primary" | "control" | "ctrl" => "Ctrl",
                "shift" => "Shift",
                "alt" | "mod1" => "Alt",
                "super" | "mod4" | "meta" => "Super",
                _ => name,
            }
            .to_owned(),
        );
        rest = &stripped[end + 1..];
    }
    let key = match rest {
        "plus" => "+".to_owned(),
        "minus" => "−".to_owned(),
        "equal" => "=".to_owned(),
        "comma" => ",".to_owned(),
        "period" => ".".to_owned(),
        "slash" => "/".to_owned(),
        "BackSpace" => "Backspace".to_owned(),
        "Page_Up" => "PageUp".to_owned(),
        "Page_Down" => "PageDown".to_owned(),
        key if key.chars().count() == 1 => key.to_uppercase(),
        key => key.to_owned(),
    };
    parts.push(key);
    parts.join("+")
}

fn text(item: &RawItem, key: &str) -> Option<String> {
    item.get(key)
        .and_then(|value| <&str>::try_from(&**value).ok().map(str::to_owned))
}

fn link(item: &RawItem, key: &str) -> Option<(u32, u32)> {
    let value = item.get(key)?;
    let Value::Structure(structure) = &**value else {
        return None;
    };
    match structure.fields() {
        [Value::U32(group), Value::U32(menu)] => Some((*group, *menu)),
        _ => None,
    }
}

/// Every (group) a menu set links to, for fetching more groups.
pub fn linked_groups(menus: &RawMenus) -> HashSet<u32> {
    menus
        .values()
        .flatten()
        .flat_map(|item| [link(item, ":submenu"), link(item, ":section")])
        .flatten()
        .map(|(group, _)| group)
        .collect()
}

/// Turn exported menus and action states into a menu bar. The bar is menu
/// (0, 0); each of its items with a `:submenu` becomes a title. Sections are
/// flattened with separators between them; nested submenus are flattened
/// into their parent (the top bar shows one level).
pub fn build(menus: &RawMenus, actions: &HashMap<String, ActionInfo>) -> Imported {
    let mut imported = Imported::default();
    let Some(bar) = menus.get(&(0, 0)) else {
        return imported;
    };
    for entry in bar {
        let Some(submenu) = link(entry, ":submenu") else {
            continue;
        };
        let title = strip_mnemonic(&text(entry, "label").unwrap_or_default());
        let mut items = Vec::new();
        let mut refs = Vec::new();
        flatten(menus, actions, submenu, &mut items, &mut refs, 0);
        // No leading, trailing or doubled separators.
        let mut cleaned: Vec<(MenuItem, Option<ActionRef>)> = Vec::new();
        for pair in items.into_iter().zip(refs) {
            let is_separator = pair.0.separator;
            if is_separator && cleaned.last().is_none_or(|(last, _)| last.separator) {
                continue;
            }
            cleaned.push(pair);
        }
        while cleaned.last().is_some_and(|(last, _)| last.separator) {
            cleaned.pop();
        }
        if title.is_empty() && cleaned.is_empty() {
            continue;
        }
        let (items, refs) = cleaned.into_iter().unzip();
        imported.titles.push(title);
        imported.menus.push(items);
        imported.actions.push(refs);
    }
    imported
}

fn flatten(
    menus: &RawMenus,
    actions: &HashMap<String, ActionInfo>,
    menu: (u32, u32),
    items: &mut Vec<MenuItem>,
    refs: &mut Vec<Option<ActionRef>>,
    depth: usize,
) {
    if depth > 8 {
        return; // A malformed exporter cannot recurse us forever.
    }
    let Some(entries) = menus.get(&menu) else {
        return;
    };
    for entry in entries {
        if let Some(section) = link(entry, ":section") {
            items.push(MenuItem::separator());
            refs.push(None);
            flatten(menus, actions, section, items, refs, depth + 1);
            items.push(MenuItem::separator());
            refs.push(None);
            continue;
        }
        if let Some(submenu) = link(entry, ":submenu") {
            flatten(menus, actions, submenu, items, refs, depth + 1);
            continue;
        }
        let label = strip_mnemonic(&text(entry, "label").unwrap_or_default());
        if label.is_empty() {
            continue;
        }
        let mut item = MenuItem::new(&label);
        if let Some(accel) = text(entry, "accel") {
            item = item.shortcut(&shortcut_text(&accel));
        }
        let action = text(entry, "action").and_then(|full| {
            let (scope, name) = full.split_once('.')?;
            Some(ActionRef {
                scope: scope.to_owned(),
                name: name.to_owned(),
                target: entry.get("target").and_then(|t| t.try_clone().ok()),
            })
        });
        match action
            .as_ref()
            .and_then(|a| actions.get(&format!("{}.{}", a.scope, a.name)))
        {
            Some(info) => {
                item.enabled = info.enabled;
                match (
                    info.state.as_deref(),
                    action.as_ref().and_then(|a| a.target.as_deref()),
                ) {
                    (Some(Value::Bool(on)), None) => item = item.checked(*on),
                    (Some(state), Some(target)) => item = item.checked(state == target),
                    _ => {}
                }
            }
            // An item without a known action cannot do anything.
            None => item.enabled = false,
        }
        items.push(item);
        refs.push(action);
    }
}

/// What the worker reports.
#[derive(Debug)]
pub enum Update {
    /// Menus for `source` (empty when it exports nothing usable).
    Menus(MenuSource, Imported),
    /// An exporter (unique bus name) announced a change.
    Changed(String),
    /// The bus could not be reached or a call failed.
    Failed(MenuSource, String),
}

enum Command {
    Fetch(MenuSource),
    Activate(MenuSource, ActionRef),
}

/// Fetches and activates exported menus off the UI thread.
pub struct Importer {
    commands: Sender<Command>,
    pub updates: Receiver<Update>,
}

impl Importer {
    /// Connect to the session bus; `wake` runs after every update.
    pub fn start(wake: impl Fn() + Send + Sync + 'static) -> Option<Self> {
        let connection = match Connection::session() {
            Ok(connection) => connection,
            Err(error) => {
                log::warn!("menu import unavailable: {error}");
                return None;
            }
        };
        let wake = std::sync::Arc::new(wake);
        let (commands, receiver) = mpsc::channel::<Command>();
        let (sender, updates) = mpsc::channel();
        let worker_connection = connection.clone();
        let worker_sender = sender.clone();
        let worker_wake = wake.clone();
        thread::Builder::new()
            .name("slopos-menu-import".into())
            .spawn(move || {
                for command in receiver {
                    let update = match command {
                        Command::Fetch(source) => match fetch(&worker_connection, &source) {
                            Ok(imported) => Update::Menus(source, imported),
                            Err(error) => Update::Failed(source, error.to_string()),
                        },
                        Command::Activate(source, action) => {
                            match activate(&worker_connection, &source, &action) {
                                Ok(()) => continue,
                                Err(error) => Update::Failed(source, error.to_string()),
                            }
                        }
                    };
                    if worker_sender.send(update).is_err() {
                        break;
                    }
                    worker_wake();
                }
            })
            .ok()?;
        // One watcher for every exporter's Changed signals.
        let watch_wake = wake;
        thread::Builder::new()
            .name("slopos-menu-watch".into())
            .spawn(move || {
                let rules = ["org.gtk.Menus", "org.gtk.Actions"].map(|interface| {
                    zbus::MatchRule::builder()
                        .msg_type(zbus::message::Type::Signal)
                        .interface(interface)
                        .and_then(|rule| rule.member("Changed"))
                        .map(|rule| rule.build())
                });
                let dbus = zbus::blocking::fdo::DBusProxy::new(&connection).ok();
                for rule in rules.iter().flatten() {
                    if let Some(dbus) = &dbus {
                        let _ = dbus.add_match_rule(rule.clone());
                    }
                }
                for message in zbus::blocking::MessageIterator::from(&connection).flatten() {
                    let header = message.header();
                    if header.message_type() != zbus::message::Type::Signal
                        || header.member().map(|m| m.as_str()) != Some("Changed")
                    {
                        continue;
                    }
                    let interface = header.interface().map(|i| i.to_string());
                    if !matches!(
                        interface.as_deref(),
                        Some("org.gtk.Menus" | "org.gtk.Actions")
                    ) {
                        continue;
                    }
                    let Some(sender_name) = header.sender().map(|s| s.to_string()) else {
                        continue;
                    };
                    if sender.send(Update::Changed(sender_name)).is_err() {
                        break;
                    }
                    watch_wake();
                }
            })
            .ok()?;
        Some(Self { commands, updates })
    }

    /// Fetch (or re-fetch) `source`'s menus; the answer arrives as an update.
    pub fn fetch(&self, source: MenuSource) {
        let _ = self.commands.send(Command::Fetch(source));
    }

    /// Run the action behind an item of `imported` (from `source`).
    pub fn activate(&self, source: &MenuSource, imported: &Imported, menu: usize, item: usize) {
        if let Some(action) = imported.action(menu, item) {
            let _ = self
                .commands
                .send(Command::Activate(source.clone(), action.clone()));
        }
    }
}

type StartReply = Vec<(u32, u32, Vec<RawItem>)>;
type Description = (bool, zbus::zvariant::OwnedSignature, Vec<OwnedValue>);

fn fetch(connection: &Connection, source: &MenuSource) -> zbus::Result<Imported> {
    let menus_proxy = zbus::blocking::Proxy::new(
        connection,
        source.bus_name.as_str(),
        source.menu_path.as_str(),
        "org.gtk.Menus",
    )?;
    let mut menus: RawMenus = HashMap::new();
    let mut fetched: HashSet<u32> = HashSet::new();
    let mut wanted: Vec<u32> = vec![0];
    // Submenus usually live in their own groups; follow links until done
    // (bounded, so a hostile exporter cannot keep us busy).
    while let Some(group) = wanted.pop() {
        if !fetched.insert(group) || fetched.len() > 64 {
            continue;
        }
        let reply: StartReply = menus_proxy.call("Start", &(vec![group],))?;
        for (group, menu, items) in reply {
            menus.insert((group, menu), items);
        }
        wanted.extend(linked_groups(&menus).difference(&fetched));
    }
    let _: () = menus_proxy
        .call("End", &(fetched.iter().copied().collect::<Vec<u32>>(),))
        .unwrap_or(());
    let mut actions = HashMap::new();
    for (scope, path) in [("app", &source.app_path), ("win", &source.window_path)] {
        let Some(path) = path else {
            continue;
        };
        let proxy = zbus::blocking::Proxy::new(
            connection,
            source.bus_name.as_str(),
            path.as_str(),
            "org.gtk.Actions",
        )?;
        let described: HashMap<String, Description> = proxy.call("DescribeAll", &())?;
        for (name, (enabled, _, state)) in described {
            actions.insert(
                format!("{scope}.{name}"),
                ActionInfo {
                    enabled,
                    state: state.into_iter().next(),
                },
            );
        }
    }
    Ok(build(&menus, &actions))
}

fn activate(connection: &Connection, source: &MenuSource, action: &ActionRef) -> zbus::Result<()> {
    let path = match action.scope.as_str() {
        "app" => source.app_path.as_deref(),
        "win" => source.window_path.as_deref(),
        _ => None,
    }
    .ok_or_else(|| zbus::Error::Failure(format!("no {} action group", action.scope)))?;
    let proxy = zbus::blocking::Proxy::new(
        connection,
        source.bus_name.as_str(),
        path,
        "org.gtk.Actions",
    )?;
    let parameter: Vec<OwnedValue> = action
        .target
        .as_ref()
        .and_then(|target| target.try_clone().ok())
        .into_iter()
        .collect();
    let platform: HashMap<String, OwnedValue> = HashMap::new();
    proxy.call::<_, _, ()>("Activate", &(action.name.as_str(), parameter, platform))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned(value: Value<'_>) -> OwnedValue {
        OwnedValue::try_from(value).unwrap()
    }

    fn item(pairs: &[(&str, Value<'_>)]) -> RawItem {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), owned(value.try_clone().unwrap())))
            .collect()
    }

    #[test]
    fn labels_and_accelerators_read_naturally() {
        assert_eq!(strip_mnemonic("_File"), "File");
        assert_eq!(strip_mnemonic("Save __As"), "Save _As");
        assert_eq!(shortcut_text("<Primary>o"), "Ctrl+O");
        assert_eq!(shortcut_text("<Control><Shift>n"), "Ctrl+Shift+N");
        assert_eq!(shortcut_text("<Alt>Left"), "Alt+Left");
        assert_eq!(shortcut_text("<Primary>plus"), "Ctrl++");
        assert_eq!(shortcut_text("F5"), "F5");
    }

    #[test]
    fn menus_sections_submenus_and_action_states_become_a_bar() {
        let mut menus: RawMenus = HashMap::new();
        menus.insert(
            (0, 0),
            vec![
                item(&[
                    ("label", Value::from("_File")),
                    (":submenu", Value::from((1u32, 0u32))),
                ]),
                item(&[
                    ("label", Value::from("_View")),
                    (":submenu", Value::from((2u32, 0u32))),
                ]),
            ],
        );
        // File: two sections in group 1, plus a nested submenu.
        menus.insert(
            (1, 0),
            vec![
                item(&[(":section", Value::from((1u32, 1u32)))]),
                item(&[(":section", Value::from((1u32, 2u32)))]),
            ],
        );
        menus.insert(
            (1, 1),
            vec![item(&[
                ("label", Value::from("_Open")),
                ("action", Value::from("app.open")),
                ("accel", Value::from("<Primary>o")),
            ])],
        );
        menus.insert(
            (1, 2),
            vec![
                item(&[
                    ("label", Value::from("Quit")),
                    ("action", Value::from("app.quit")),
                ]),
                item(&[
                    ("label", Value::from("Gone")),
                    ("action", Value::from("app.missing")),
                ]),
            ],
        );
        // View: a check item and a radio pair.
        menus.insert(
            (2, 0),
            vec![
                item(&[
                    ("label", Value::from("Hidden")),
                    ("action", Value::from("win.hidden")),
                ]),
                item(&[
                    ("label", Value::from("List")),
                    ("action", Value::from("win.mode")),
                    ("target", Value::from("list")),
                ]),
                item(&[
                    ("label", Value::from("Icons")),
                    ("action", Value::from("win.mode")),
                    ("target", Value::from("icons")),
                ]),
            ],
        );
        assert_eq!(linked_groups(&menus), HashSet::from([1, 2]));
        let mut actions = HashMap::new();
        let info = |enabled, state: Option<Value<'_>>| ActionInfo {
            enabled,
            state: state.map(owned),
        };
        actions.insert("app.open".into(), info(true, None));
        actions.insert("app.quit".into(), info(false, None));
        actions.insert("win.hidden".into(), info(true, Some(Value::from(true))));
        actions.insert("win.mode".into(), info(true, Some(Value::from("icons"))));
        let bar = build(&menus, &actions);
        assert_eq!(bar.titles, ["File", "View"]);
        let file = &bar.menus[0];
        let labels: Vec<&str> = file.iter().map(|i| i.label.as_str()).collect();
        // Sections are separated once, never at the ends.
        assert_eq!(labels, ["Open", "", "Quit", "Gone"]);
        assert!(file[1].separator);
        assert_eq!(file[0].shortcut.as_deref(), Some("Ctrl+O"));
        assert!(file[0].enabled && !file[2].enabled && !file[3].enabled);
        let view = &bar.menus[1];
        assert!(view[0].checkable && view[0].checked);
        assert!(!view[1].checked && view[2].checked);
        assert!(bar.action(0, 0).is_some() && bar.action(0, 1).is_none());
        assert!(build(&HashMap::new(), &actions).is_empty());
    }

    /// Our importer reads our exporter over a private bus, follows a state
    /// change and activates an item end to end.
    #[test]
    fn round_trip_with_the_toolkit_exporter() {
        use crate::platform::global_menu::{Exporter, Message, Snapshot};
        use std::io::{BufRead, BufReader};
        use std::process::{Command, Stdio};
        use std::time::Duration;

        let Ok(mut daemon) = Command::new("dbus-daemon")
            .args(["--session", "--print-address", "--nofork"])
            .stdout(Stdio::piped())
            .spawn()
        else {
            eprintln!("NOT_RUN: dbus-daemon is not installed");
            return;
        };
        let mut address = String::new();
        BufReader::new(daemon.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        // SAFETY (edition 2021): this is the only test that reads the bus
        // address, and it sets it before any connection is made.
        std::env::set_var("DBUS_SESSION_BUS_ADDRESS", address.trim());

        let snapshot = Snapshot {
            titles: vec!["File".into(), "View".into()],
            menus: vec![
                vec![
                    MenuItem::new("Open").shortcut("Ctrl+O"),
                    MenuItem::separator(),
                    MenuItem::new("Close Window").shortcut("Ctrl+W"),
                ],
                vec![MenuItem::new("Show Hidden Files").checkable()],
            ],
        };
        let (sender, messages) = mpsc::channel();
        let exporter = Exporter::start(
            "slopos-roundtrip",
            snapshot.clone(),
            sender,
            crate::platform::x11::test_proxy(),
        )
        .expect("exporter on the private bus");
        let source = MenuSource {
            bus_name: exporter.unique_name().unwrap(),
            menu_path: exporter.menu_path().to_owned(),
            app_path: Some(exporter.action_path().to_owned()),
            window_path: None,
        };
        let importer = Importer::start(|| {}).expect("importer");
        let wait_menus = |importer: &Importer| loop {
            match importer.updates.recv_timeout(Duration::from_secs(10)) {
                Ok(Update::Menus(_, imported)) => return imported,
                Ok(Update::Failed(_, error)) => panic!("fetch failed: {error}"),
                Ok(Update::Changed(_)) => continue,
                Err(error) => panic!("no menus: {error}"),
            }
        };
        importer.fetch(source.clone());
        let imported = wait_menus(&importer);
        assert_eq!(imported.titles, ["File", "View"]);
        assert_eq!(imported.menus[0][0].shortcut.as_deref(), Some("Ctrl+O"));
        assert!(imported.menus[0][1].separator);
        assert!(imported.menus[1][0].checkable && !imported.menus[1][0].checked);

        // A state change is announced and re-fetched.
        let mut checked = snapshot.clone();
        checked.menus[1][0].checked = true;
        exporter.update(checked);
        loop {
            match importer.updates.recv_timeout(Duration::from_secs(10)) {
                Ok(Update::Changed(name)) if name == source.bus_name => break,
                Ok(_) => continue,
                Err(error) => panic!("no Changed signal: {error}"),
            }
        }
        importer.fetch(source.clone());
        assert!(wait_menus(&importer).menus[1][0].checked);

        // Activating reaches the app as the same menu position.
        importer.activate(&source, &imported, 0, 2);
        assert_eq!(
            messages.recv_timeout(Duration::from_secs(10)).unwrap(),
            Message::Activate { menu: 0, item: 2 }
        );
        drop(exporter);
        let _ = daemon.kill();
    }
}
