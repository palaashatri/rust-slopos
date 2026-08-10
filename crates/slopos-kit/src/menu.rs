use crate::event::{KeyCode, Modifiers};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MenuItemKind {
    Action,
    Submenu,
    Separator,
    Checkbox,
    Radio,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MenuItem {
    pub label: String,
    pub kind: MenuItemKind,
    pub enabled: bool,
    pub checked: bool,
    pub shortcut: Option<(KeyCode, Modifiers)>,
    pub submenu: Option<Menu>,
    pub action_id: String,
}

impl MenuItem {
    pub fn action<S: Into<String>>(label: S) -> Self {
        Self {
            label: label.into(),
            kind: MenuItemKind::Action,
            enabled: true,
            checked: false,
            shortcut: None,
            submenu: None,
            action_id: String::new(),
        }
    }

    pub fn separator() -> Self {
        Self {
            label: String::new(),
            kind: MenuItemKind::Separator,
            enabled: false,
            checked: false,
            shortcut: None,
            submenu: None,
            action_id: String::new(),
        }
    }

    pub fn submenu<S: Into<String>>(label: S, menu: Menu) -> Self {
        Self {
            label: label.into(),
            kind: MenuItemKind::Submenu,
            enabled: true,
            checked: false,
            shortcut: None,
            submenu: Some(menu),
            action_id: String::new(),
        }
    }

    pub fn with_shortcut(&mut self, key: KeyCode, mods: Modifiers) -> &mut Self {
        self.shortcut = Some((key, mods));
        self
    }

    pub fn with_action(&mut self, id: &str) -> &mut Self {
        self.action_id = id.to_string();
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Menu {
    pub title: String,
    pub items: Vec<MenuItem>,
}

impl Menu {
    pub fn new<S: Into<String>>(title: S) -> Self {
        Self {
            title: title.into(),
            items: vec![],
        }
    }

    pub fn add(&mut self, item: MenuItem) {
        self.items.push(item);
    }

    pub fn add_action<S: Into<String>>(&mut self, label: S) -> &mut MenuItem {
        let index = self.items.len();
        self.items.push(MenuItem::action(label));
        &mut self.items[index]
    }

    pub fn add_separator(&mut self) {
        self.items.push(MenuItem::separator());
    }

    pub fn add_submenu<S: Into<String>>(&mut self, label: S, menu: Menu) {
        self.items.push(MenuItem::submenu(label, menu));
    }
}
