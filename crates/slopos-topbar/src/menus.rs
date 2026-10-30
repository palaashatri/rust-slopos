//! What the bar's menus contain, as plain data: titles, toolkit menu items
//! and the command each item runs. Built from what the system can do
//! (programs installed, logind answers), so an item that cannot work is
//! disabled instead of failing when chosen.

use slopos_ui::controls::menu::MenuItem;

/// Everything a top-bar menu item can do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    About,
    ControlPanels,
    Software,
    Appearance(&'static str),
    DesktopPicture,
    Lock,
    SwitchUser,
    Sleep,
    LogOut,
    Restart,
    ShutDown,
    NewFolder,
    NewFilesWindow,
    NewTextDocument,
    OpenTerminal,
    FindApplications,
    EmptyTrash,
    Minimize,
    Zoom,
    CloseWindow,
    /// Item `item` of the focused app's imported menu `menu`.
    Imported(usize, usize),
}

/// What is available on this system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capabilities {
    pub control_panels: bool,
    pub software: bool,
    pub files: bool,
    pub appearance: bool,
    pub text_editor: bool,
    pub terminal: bool,
    pub lock: bool,
    pub switch_user: bool,
    pub sleep: bool,
    pub restart: bool,
    pub shut_down: bool,
}

/// One menu of the bar.
#[derive(Debug, Clone, PartialEq)]
pub struct MenuSpec {
    pub title: String,
    pub items: Vec<(MenuItem, Option<Command>)>,
}

impl MenuSpec {
    fn new(title: &str) -> Self {
        Self {
            title: title.to_owned(),
            items: Vec::new(),
        }
    }

    fn item(mut self, label: &str, command: Command, enabled: bool) -> Self {
        self.items
            .push((MenuItem::new(label).enabled(enabled), Some(command)));
        self
    }

    fn shortcut(mut self, text: &str) -> Self {
        if let Some((item, _)) = self.items.last_mut() {
            item.shortcut = Some(text.to_owned());
        }
        self
    }

    fn separator(mut self) -> Self {
        self.items.push((MenuItem::separator(), None));
        self
    }

    pub fn commands(&self) -> Vec<Option<Command>> {
        self.items.iter().map(|(_, command)| *command).collect()
    }

    pub fn menu_items(&self) -> Vec<MenuItem> {
        self.items.iter().map(|(item, _)| item.clone()).collect()
    }
}

/// The System menu behind the SLOPOS mark.
pub fn system_menu(can: &Capabilities) -> MenuSpec {
    let mut menu = MenuSpec::new("")
        .item("About SLOPOS-I…", Command::About, true)
        .separator()
        .item(
            "Control Panels…",
            Command::ControlPanels,
            can.control_panels,
        )
        .item("Software…", Command::Software, can.software)
        .item(
            "Desktop Picture…",
            Command::DesktopPicture,
            can.control_panels,
        )
        .separator();
    for (label, id) in [
        ("Platinum Light", "platinum"),
        ("Classic Contrast", "classic"),
        ("Graphite Dark", "graphite"),
        ("OLED Dark", "oled"),
    ] {
        menu = menu.item(
            &format!("Appearance: {label}"),
            Command::Appearance(id),
            can.appearance,
        );
    }
    session_items(menu.separator(), can, true)
}

fn session_items(menu: MenuSpec, can: &Capabilities, logout: bool) -> MenuSpec {
    let mut menu = menu
        .item("Lock Screen", Command::Lock, can.lock)
        .item("Switch User…", Command::SwitchUser, can.switch_user)
        .item("Sleep", Command::Sleep, can.sleep)
        .separator();
    if logout {
        menu = menu.item("Log Out…", Command::LogOut, true);
    }
    menu.item("Restart…", Command::Restart, can.restart).item(
        "Shut Down…",
        Command::ShutDown,
        can.shut_down,
    )
}

/// The desktop's own menus, shown when no application window is focused.
pub fn desktop_menus(can: &Capabilities) -> Vec<MenuSpec> {
    vec![
        MenuSpec::new("File")
            .item("New Folder", Command::NewFolder, true)
            .item("New Files Window", Command::NewFilesWindow, can.files)
            .item(
                "New Text Document…",
                Command::NewTextDocument,
                can.text_editor,
            )
            .item("Open Terminal…", Command::OpenTerminal, can.terminal)
            .separator()
            .item("Find Applications…", Command::FindApplications, true)
            .shortcut("Super+Space"),
        MenuSpec::new("View")
            .item(
                "Control Panels…",
                Command::ControlPanels,
                can.control_panels,
            )
            .item("Software…", Command::Software, can.software),
        session_items(
            MenuSpec::new("Special")
                .item("Empty Trash…", Command::EmptyTrash, true)
                .separator(),
            can,
            false,
        ),
        MenuSpec::new("Help").item("About SLOPOS-I…", Command::About, true),
    ]
}

/// For a window that exports no menus: window-manager actions only.
pub fn window_menus() -> Vec<MenuSpec> {
    vec![MenuSpec::new("Window")
        .item("Minimize", Command::Minimize, true)
        .shortcut("Super+M")
        .item("Zoom", Command::Zoom, true)
        .shortcut("Super+F")
        .separator()
        .item("Close Window", Command::CloseWindow, true)
        .shortcut("Super+Q")]
}

/// An imported application menu bar, with commands pointing back at it.
pub fn imported_menus(titles: &[String], menus: &[Vec<MenuItem>]) -> Vec<MenuSpec> {
    titles
        .iter()
        .zip(menus)
        .enumerate()
        .map(|(menu_index, (title, items))| MenuSpec {
            title: title.clone(),
            items: items
                .iter()
                .enumerate()
                .map(|(item_index, item)| {
                    let command =
                        (!item.separator).then_some(Command::Imported(menu_index, item_index));
                    (item.clone(), command)
                })
                .collect(),
        })
        .collect()
}

/// Commands that change or end the session ask first.
pub fn confirmation(command: Command) -> Option<(&'static str, &'static str, &'static str)> {
    match command {
        Command::LogOut => Some((
            "Log Out",
            "Log out of SLOPOS-I now? Unsaved work in open applications may be lost.",
            "Log Out",
        )),
        Command::Restart => Some(("Restart", "Restart this computer now?", "Restart")),
        Command::ShutDown => Some(("Shut Down", "Shut down this computer now?", "Shut Down")),
        Command::EmptyTrash => Some((
            "Empty Trash",
            "Permanently delete all items in the Trash? This cannot be undone.",
            "Empty Trash",
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_actions_are_disabled_not_hidden() {
        let none = Capabilities::default();
        let system = system_menu(&none);
        let find = |label: &str| {
            system
                .items
                .iter()
                .find(|(item, _)| item.label == label)
                .map(|(item, command)| (item.enabled, *command))
                .unwrap()
        };
        assert_eq!(find("Restart…"), (false, Some(Command::Restart)));
        assert_eq!(find("Log Out…"), (true, Some(Command::LogOut)));
        assert!(!find("Control Panels…").0);
        let all = Capabilities {
            control_panels: true,
            restart: true,
            ..Capabilities::default()
        };
        let system = system_menu(&all);
        assert!(system
            .items
            .iter()
            .any(|(item, c)| item.enabled && *c == Some(Command::Restart)));
    }

    #[test]
    fn desktop_window_and_imported_menus() {
        let titles: Vec<String> = desktop_menus(&Capabilities::default())
            .into_iter()
            .map(|menu| menu.title)
            .collect();
        assert_eq!(titles, ["File", "View", "Special", "Help"]);
        let window = window_menus();
        assert_eq!(window[0].title, "Window");
        assert_eq!(window[0].commands()[3], Some(Command::CloseWindow));
        let imported = imported_menus(
            &["File".into()],
            &[vec![
                MenuItem::new("Open"),
                MenuItem::separator(),
                MenuItem::new("Quit"),
            ]],
        );
        assert_eq!(
            imported[0].commands(),
            vec![
                Some(Command::Imported(0, 0)),
                None,
                Some(Command::Imported(0, 2))
            ]
        );
    }

    #[test]
    fn destructive_session_commands_ask_first() {
        for command in [
            Command::LogOut,
            Command::Restart,
            Command::ShutDown,
            Command::EmptyTrash,
        ] {
            assert!(confirmation(command).is_some(), "{command:?}");
        }
        assert!(confirmation(Command::About).is_none());
        assert!(confirmation(Command::Sleep).is_none());
    }
}
