//! The `com.canonical.dbusmenu` layout tree as plain data.
//!
//! A KDE/Canonical AppMenu client exports its menus with
//! `com.canonical.dbusmenu.GetLayout`, which returns a revision and a tree of
//! `(id, properties, children)` nodes. This module turns such a tree into a
//! [`Layout`] the top bar can draw: ids, labels with the `_` mnemonic
//! removed, enabled, visible and checked state, submenus, separators and
//! keyboard shortcuts.
//!
//! The input is a [`RawNode`] tree that the D-Bus layer (phase 2) builds from
//! the wire values; nothing here depends on zbus or X11. The parser treats
//! its input as untrusted: depth, item count and text length are bounded and
//! a violation rejects the whole layout instead of drawing half of it.
//!
//! Reading rules, where the wire format leaves room:
//! - `visible` and `enabled` default to true.
//! - A `toggle-state` that is missing or not 0 or 1 is indeterminate.
//! - A node is a submenu when `children-display` is `"submenu"`, or when it
//!   has children and the property is missing altogether.
//! - Each `shortcut` entry is one chord (a list of modifier names and one
//!   key); an entry that is not a valid chord is skipped.

use std::collections::HashSet;
use std::fmt;

/// Deepest nesting accepted, counting the root's children as depth 1.
pub const MAX_DEPTH: usize = 32;
/// Most items accepted in one layout.
pub const MAX_ITEMS: usize = 10_000;
/// Longest label, icon name or key name kept, in characters.
pub const MAX_TEXT: usize = 1_024;
/// Most shortcuts kept for one item.
pub const MAX_SHORTCUTS: usize = 8;

/// A property value as the D-Bus layer decodes it. Only the types the menu
/// protocol uses are represented; other wire types are dropped by the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Str(String),
    Bool(bool),
    Int(i32),
    /// `a(as)`: the `shortcut` property, one key list per shortcut.
    StrLists(Vec<Vec<String>>),
}

/// One node of the tree as it came off the bus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawNode {
    pub id: i32,
    /// The `a{sv}` properties. The first entry with a given name wins.
    pub properties: Vec<(String, Value)>,
    pub children: Vec<RawNode>,
}

impl RawNode {
    pub fn item(id: i32) -> Self {
        Self {
            id,
            properties: Vec::new(),
            children: Vec::new(),
        }
    }

    pub fn prop(mut self, name: &str, value: Value) -> Self {
        self.properties.push((name.to_owned(), value));
        self
    }

    pub fn child(mut self, node: RawNode) -> Self {
        self.children.push(node);
        self
    }

    fn get(&self, name: &str) -> Option<&Value> {
        self.properties
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value)
    }

    fn text(&self, name: &str) -> Option<&str> {
        match self.get(name) {
            Some(Value::Str(text)) => Some(text.as_str()),
            _ => None,
        }
    }

    fn flag(&self, name: &str, default: bool) -> bool {
        match self.get(name) {
            Some(Value::Bool(flag)) => *flag,
            _ => default,
        }
    }
}

/// A label with its mnemonic. `"_Save _As"` reads as text `"Save As"` with
/// mnemonic `'S'`; `"__"` is a literal underscore.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Label {
    pub text: String,
    /// The character after the first single underscore, if there is one.
    pub mnemonic: Option<char>,
}

impl Label {
    pub fn parse(raw: &str) -> Self {
        let mut text = String::new();
        let mut mnemonic = None;
        let mut chars = raw.chars().peekable();
        while let Some(c) = chars.next() {
            if c != '_' {
                text.push(c);
                continue;
            }
            match chars.peek().copied() {
                // "__" is a literal underscore.
                Some('_') => {
                    chars.next();
                    text.push('_');
                }
                // A single underscore marks the next character; the
                // underscore itself is dropped, and the first mark wins.
                Some(next) => {
                    mnemonic = mnemonic.or(Some(next));
                }
                // A trailing underscore marks nothing.
                None => {}
            }
        }
        Self {
            text: text.chars().take(MAX_TEXT).collect(),
            mnemonic,
        }
    }
}

/// Whether an item is an ordinary item or a separator line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Standard,
    Separator,
}

/// The check state of a togglable item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToggleState {
    Off,
    On,
    Indeterminate,
}

/// A checkable item: a check mark, or a radio item in a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Toggle {
    pub radio: bool,
    pub state: ToggleState,
}

/// One keyboard modifier set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    /// The Super/Logo/Meta key.
    pub logo: bool,
}

/// A shortcut: modifiers and one key, such as Ctrl+Shift+S.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chord {
    pub modifiers: Modifiers,
    /// The key name as the application wrote it, e.g. `"s"` or `"Left"`.
    pub key: String,
}

impl Chord {
    /// Parse one shortcut entry: zero or more modifier names and exactly
    /// one key name. Returns `None` for an entry with no key, two keys, or
    /// an empty key.
    pub fn parse(tokens: &[String]) -> Option<Self> {
        let mut modifiers = Modifiers::default();
        let mut key: Option<&str> = None;
        for token in tokens {
            match token.to_ascii_lowercase().as_str() {
                "control" | "ctrl" => modifiers.ctrl = true,
                "shift" => modifiers.shift = true,
                "alt" => modifiers.alt = true,
                "super" | "meta" | "logo" | "mod4" => modifiers.logo = true,
                _ => {
                    if key.is_some() || token.is_empty() || token.chars().count() > MAX_TEXT {
                        return None;
                    }
                    key = Some(token.as_str());
                }
            }
        }
        key.map(|key| Self {
            modifiers,
            key: key.to_owned(),
        })
    }

    /// The shortcut as menus show it, e.g. `"Ctrl+Shift+S"`.
    pub fn text(&self) -> String {
        let mut parts = Vec::new();
        if self.modifiers.ctrl {
            parts.push("Ctrl".to_owned());
        }
        if self.modifiers.shift {
            parts.push("Shift".to_owned());
        }
        if self.modifiers.alt {
            parts.push("Alt".to_owned());
        }
        if self.modifiers.logo {
            parts.push("Super".to_owned());
        }
        let mut key = self.key.clone();
        if key.chars().count() == 1 {
            key = key.to_uppercase();
        }
        parts.push(key);
        parts.join("+")
    }
}

/// One item of a menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuNode {
    /// The id the application uses in `Event` and `AboutToShow`.
    pub id: i32,
    pub kind: Kind,
    pub label: Label,
    pub enabled: bool,
    pub visible: bool,
    /// `None` for an item that is not checkable.
    pub toggle: Option<Toggle>,
    pub shortcuts: Vec<Chord>,
    pub icon_name: Option<String>,
    /// Whether the item opens `children` as a submenu.
    pub submenu: bool,
    pub children: Vec<MenuNode>,
}

impl MenuNode {
    /// Checked: a check mark or radio dot that is on.
    pub fn checked(&self) -> bool {
        self.toggle
            .is_some_and(|toggle| toggle.state == ToggleState::On)
    }

    pub fn is_separator(&self) -> bool {
        self.kind == Kind::Separator
    }
}

/// A parsed layout: the revision the application gave, and the items under
/// the root (the top-level items of a menu bar, or the entries of a menu).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub revision: u32,
    pub items: Vec<MenuNode>,
}

/// Why a layout was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutError {
    TooDeep,
    TooManyItems,
    DuplicateId(i32),
}

impl fmt::Display for LayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooDeep => write!(f, "menu nests deeper than {MAX_DEPTH} levels"),
            Self::TooManyItems => write!(f, "menu has more than {MAX_ITEMS} items"),
            Self::DuplicateId(id) => write!(f, "menu item id {id} appears twice"),
        }
    }
}

impl std::error::Error for LayoutError {}

impl Layout {
    /// Parse the tree `GetLayout` returned. `root` is the node asked for
    /// (id 0 for the whole menu); its children become `items`.
    pub fn parse(revision: u32, root: &RawNode) -> Result<Self, LayoutError> {
        let mut seen = Seen::default();
        let items = parse_children(&root.children, 1, &mut seen)?;
        Ok(Self { revision, items })
    }

    /// The item with `id`, searching submenus too.
    pub fn find(&self, id: i32) -> Option<&MenuNode> {
        find_in(&self.items, id)
    }
}

#[derive(Default)]
struct Seen {
    ids: HashSet<i32>,
    count: usize,
}

fn find_in(items: &[MenuNode], id: i32) -> Option<&MenuNode> {
    items.iter().find_map(|item| {
        if item.id == id {
            Some(item)
        } else {
            find_in(&item.children, id)
        }
    })
}

fn parse_children(
    nodes: &[RawNode],
    depth: usize,
    seen: &mut Seen,
) -> Result<Vec<MenuNode>, LayoutError> {
    if depth > MAX_DEPTH && !nodes.is_empty() {
        return Err(LayoutError::TooDeep);
    }
    nodes
        .iter()
        .map(|node| {
            seen.count += 1;
            if seen.count > MAX_ITEMS {
                return Err(LayoutError::TooManyItems);
            }
            if !seen.ids.insert(node.id) {
                return Err(LayoutError::DuplicateId(node.id));
            }
            parse_node(node, depth, seen)
        })
        .collect()
}

fn parse_node(node: &RawNode, depth: usize, seen: &mut Seen) -> Result<MenuNode, LayoutError> {
    let kind = if node.text("type") == Some("separator") {
        Kind::Separator
    } else {
        Kind::Standard
    };
    let toggle = match node.text("toggle-type") {
        Some("checkmark") => Some(Toggle {
            radio: false,
            state: toggle_state(node),
        }),
        Some("radio") => Some(Toggle {
            radio: true,
            state: toggle_state(node),
        }),
        _ => None,
    };
    let submenu = match node.text("children-display") {
        Some(display) => display == "submenu",
        None => !node.children.is_empty(),
    };
    Ok(MenuNode {
        id: node.id,
        kind,
        label: Label::parse(node.text("label").unwrap_or("")),
        enabled: node.flag("enabled", true),
        visible: node.flag("visible", true),
        toggle,
        shortcuts: shortcuts(node),
        icon_name: node
            .text("icon-name")
            .filter(|name| !name.is_empty())
            .map(|name| name.chars().take(MAX_TEXT).collect()),
        submenu,
        children: parse_children(&node.children, depth + 1, seen)?,
    })
}

fn toggle_state(node: &RawNode) -> ToggleState {
    match node.get("toggle-state") {
        Some(Value::Int(0)) => ToggleState::Off,
        Some(Value::Int(1)) => ToggleState::On,
        _ => ToggleState::Indeterminate,
    }
}

fn shortcuts(node: &RawNode) -> Vec<Chord> {
    let Some(Value::StrLists(lists)) = node.get("shortcut") else {
        return Vec::new();
    };
    lists
        .iter()
        .filter_map(|tokens| Chord::parse(tokens))
        .take(MAX_SHORTCUTS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> Value {
        Value::Str(value.to_owned())
    }

    fn keys(tokens: &[&str]) -> Vec<String> {
        tokens.iter().map(|token| (*token).to_owned()).collect()
    }

    fn layout_of(children: Vec<RawNode>) -> Result<Layout, LayoutError> {
        let mut root = RawNode::item(0);
        root.children = children;
        Layout::parse(7, &root)
    }

    fn one(node: RawNode) -> MenuNode {
        let layout = layout_of(vec![node]).expect("layout parses");
        layout.items.into_iter().next().expect("one item")
    }

    #[test]
    fn mnemonic_is_removed_and_first_mark_wins() {
        let label = Label::parse("_File");
        assert_eq!(label.text, "File");
        assert_eq!(label.mnemonic, Some('F'));

        let label = Label::parse("Save _As...");
        assert_eq!(label.text, "Save As...");
        assert_eq!(label.mnemonic, Some('A'));

        let label = Label::parse("_A_B");
        assert_eq!(label.text, "AB");
        assert_eq!(label.mnemonic, Some('A'));
    }

    #[test]
    fn double_underscore_is_literal_and_trailing_underscore_marks_nothing() {
        let label = Label::parse("__Double");
        assert_eq!(label.text, "_Double");
        assert_eq!(label.mnemonic, None);

        let label = Label::parse("A__B_C");
        assert_eq!(label.text, "A_BC");
        assert_eq!(label.mnemonic, Some('C'));

        let label = Label::parse("Trailing_");
        assert_eq!(label.text, "Trailing");
        assert_eq!(label.mnemonic, None);
    }

    #[test]
    fn long_labels_are_cut_to_the_limit() {
        let raw: String = "x".repeat(MAX_TEXT + 10);
        assert_eq!(Label::parse(&raw).text.chars().count(), MAX_TEXT);
    }

    #[test]
    fn ids_and_revision_are_kept_and_find_searches_submenus() {
        let inner = RawNode::item(12).prop("label", text("Inner"));
        let outer = RawNode::item(5).prop("label", text("_Outer")).child(inner);
        let layout = layout_of(vec![outer]).unwrap();
        assert_eq!(layout.revision, 7);
        assert_eq!(layout.items[0].id, 5);
        assert_eq!(layout.find(12).unwrap().label.text, "Inner");
        assert!(layout.find(99).is_none());
    }

    #[test]
    fn enabled_and_visible_default_to_true_and_follow_the_properties() {
        let item = one(RawNode::item(1));
        assert!(item.enabled);
        assert!(item.visible);

        let item = one(RawNode::item(1)
            .prop("enabled", Value::Bool(false))
            .prop("visible", Value::Bool(false)));
        assert!(!item.enabled);
        assert!(!item.visible);
    }

    #[test]
    fn checkmark_and_radio_states() {
        let on = one(RawNode::item(1)
            .prop("toggle-type", text("checkmark"))
            .prop("toggle-state", Value::Int(1)));
        assert!(on.checked());
        assert_eq!(
            on.toggle,
            Some(Toggle {
                radio: false,
                state: ToggleState::On
            })
        );

        let off = one(RawNode::item(2)
            .prop("toggle-type", text("checkmark"))
            .prop("toggle-state", Value::Int(0)));
        assert!(!off.checked());
        assert_eq!(off.toggle.unwrap().state, ToggleState::Off);

        let mixed = one(RawNode::item(3)
            .prop("toggle-type", text("checkmark"))
            .prop("toggle-state", Value::Int(-1)));
        assert!(!mixed.checked());
        assert_eq!(mixed.toggle.unwrap().state, ToggleState::Indeterminate);

        // A togglable item with no state reads as indeterminate.
        let missing = one(RawNode::item(4).prop("toggle-type", text("radio")));
        assert_eq!(
            missing.toggle,
            Some(Toggle {
                radio: true,
                state: ToggleState::Indeterminate
            })
        );
        assert!(!missing.checked());
    }

    #[test]
    fn unknown_toggle_type_is_not_checkable() {
        let item = one(RawNode::item(1)
            .prop("toggle-type", text("switch"))
            .prop("toggle-state", Value::Int(1)));
        assert_eq!(item.toggle, None);
        assert!(!item.checked());
    }

    #[test]
    fn separators_are_recognised_and_have_no_label() {
        let line = one(RawNode::item(9).prop("type", text("separator")));
        assert!(line.is_separator());
        assert_eq!(line.kind, Kind::Separator);
        assert_eq!(line.label, Label::default());

        let item = one(RawNode::item(10).prop("type", text("standard")));
        assert_eq!(item.kind, Kind::Standard);
        assert!(!item.is_separator());
    }

    #[test]
    fn submenus_follow_children_display_or_the_children_when_it_is_missing() {
        let explicit = one(RawNode::item(1)
            .prop("children-display", text("submenu"))
            .child(RawNode::item(2).prop("label", text("Child"))));
        assert!(explicit.submenu);
        assert_eq!(explicit.children.len(), 1);
        assert_eq!(explicit.children[0].label.text, "Child");

        let implied = one(RawNode::item(3).child(RawNode::item(4)));
        assert!(implied.submenu);

        // An explicit "not a submenu" wins even with children.
        let declared_flat = one(RawNode::item(5)
            .prop("children-display", text(""))
            .child(RawNode::item(6)));
        assert!(!declared_flat.submenu);

        let leaf = one(RawNode::item(7));
        assert!(!leaf.submenu);
    }

    #[test]
    fn shortcuts_parse_modifiers_and_skip_invalid_entries() {
        let item = one(RawNode::item(1).prop(
            "shortcut",
            Value::StrLists(vec![
                keys(&["Control", "s"]),
                keys(&["Control", "Shift", "S"]),
                keys(&["Super", "Alt", "Left"]),
                keys(&["Control"]),
                keys(&["a", "b"]),
                keys(&[]),
                keys(&["Shift", ""]),
            ]),
        ));
        assert_eq!(item.shortcuts.len(), 3);

        let first = &item.shortcuts[0];
        assert!(first.modifiers.ctrl && !first.modifiers.shift);
        assert_eq!(first.key, "s");
        assert_eq!(first.text(), "Ctrl+S");

        assert_eq!(item.shortcuts[1].text(), "Ctrl+Shift+S");
        assert_eq!(item.shortcuts[2].text(), "Alt+Super+Left");
        assert!(item.shortcuts[2].modifiers.logo && item.shortcuts[2].modifiers.alt);
    }

    #[test]
    fn shortcut_count_is_bounded() {
        let many: Vec<Vec<String>> = (0..MAX_SHORTCUTS * 2).map(|_| keys(&["F1"])).collect();
        let item = one(RawNode::item(1).prop("shortcut", Value::StrLists(many)));
        assert_eq!(item.shortcuts.len(), MAX_SHORTCUTS);
    }

    #[test]
    fn icon_name_is_kept_and_empty_is_dropped() {
        let item = one(RawNode::item(1).prop("icon-name", text("document-save")));
        assert_eq!(item.icon_name.as_deref(), Some("document-save"));
        let item = one(RawNode::item(2).prop("icon-name", text("")));
        assert_eq!(item.icon_name, None);
    }

    #[test]
    fn first_property_with_a_name_wins() {
        let item = one(RawNode::item(1)
            .prop("label", text("First"))
            .prop("label", text("Second")));
        assert_eq!(item.label.text, "First");
    }

    #[test]
    fn wrong_property_types_fall_back_to_defaults() {
        let item = one(RawNode::item(1)
            .prop("label", Value::Int(3))
            .prop("enabled", text("no"))
            .prop("toggle-type", Value::Bool(true)));
        assert_eq!(item.label, Label::default());
        assert!(item.enabled);
        assert_eq!(item.toggle, None);
    }

    #[test]
    fn duplicate_ids_are_refused() {
        let result = layout_of(vec![
            RawNode::item(1),
            RawNode::item(2).child(RawNode::item(1)),
        ]);
        assert_eq!(result, Err(LayoutError::DuplicateId(1)));
    }

    #[test]
    fn nesting_past_the_limit_is_refused() {
        // Depth MAX_DEPTH is allowed; one more level is not.
        fn chain(levels: usize) -> RawNode {
            let mut node = RawNode::item(levels as i32);
            for id in (1..levels).rev() {
                node = RawNode::item(id as i32).child(node);
            }
            node
        }
        let mut root = RawNode::item(0);
        root.children = vec![chain(MAX_DEPTH)];
        assert!(Layout::parse(1, &root).is_ok());

        let mut root = RawNode::item(0);
        root.children = vec![chain(MAX_DEPTH + 1)];
        assert_eq!(Layout::parse(1, &root), Err(LayoutError::TooDeep));
    }

    #[test]
    fn item_count_past_the_limit_is_refused() {
        let items: Vec<RawNode> = (0..MAX_ITEMS as i32).map(RawNode::item).collect();
        assert!(layout_of(items).is_ok());

        let items: Vec<RawNode> = (0..=MAX_ITEMS as i32).map(RawNode::item).collect();
        assert_eq!(layout_of(items), Err(LayoutError::TooManyItems));
    }

    #[test]
    fn errors_are_described() {
        assert!(LayoutError::DuplicateId(4).to_string().contains('4'));
        assert!(LayoutError::TooDeep.to_string().contains("deeper"));
    }
}
