use slopos_kit::event::{KeyCode, Modifiers};
use slopos_kit::measure_text_width;
use slopos_kit::menu::{Menu, MenuItem, MenuItemKind};
use slopos_sdk::MenuManifest;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::i18n::{tr, LocalePrefs};

pub struct MenuServer {
    pub menus: Vec<Menu>,
    pub active_app: Option<String>,
    pub status_items: Vec<StatusItem>,
    pub keyboard_shortcuts: Vec<ShortcutBinding>,
    pub app_menus: HashMap<String, Vec<Menu>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusItem {
    pub id: String,
    pub label: String,
    pub icon: Option<String>,
    pub priority: i32,
}

pub struct ShortcutBinding {
    pub key: KeyCode,
    pub modifiers: Modifiers,
    pub action_id: String,
    pub app_id: Option<String>,
}

fn menu_title_advance(title: &str) -> f32 {
    measure_text_width(title) + 15.0
}

impl Default for MenuServer {
    fn default() -> Self {
        Self::new()
    }
}

impl MenuServer {
    pub fn new() -> Self {
        let mut server = Self {
            menus: vec![],
            active_app: None,
            status_items: vec![],
            keyboard_shortcuts: vec![],
            app_menus: HashMap::new(),
        };
        server.setup_default_menus();
        server.refresh_status_items();
        server
    }

    /// Re-label session actions that have i18n keys (locale from LANG / prefs).
    pub fn apply_locale_labels(&mut self, prefs: &LocalePrefs) {
        let loc = &prefs.locale;
        let label_for = |action_id: &str| -> Option<String> {
            match action_id {
                "shell.lock" => Some(tr("menu.lock_screen", loc)),
                "shell.log_out" | "shell.logout" => Some(tr("menu.log_out", loc)),
                "shell.suspend" | "shell.sleep" => Some(tr("menu.suspend", loc)),
                "shell.reboot" | "shell.restart" => Some(tr("menu.reboot", loc)),
                "shell.power_off" | "shell.shutdown" | "shell.poweroff" => {
                    Some(tr("menu.power_off", loc))
                }
                "shell.quit" => Some(tr("menu.quit", loc)),
                "shell.force_quit" => Some(tr("menu.force_quit", loc)),
                "shell.notification_center" => Some(tr("menu.notification_center", loc)),
                "shell.about" => Some(tr("menu.about", loc)),
                _ => None,
            }
        };
        for menu in &mut self.menus {
            for item in &mut menu.items {
                if let Some(label) = label_for(&item.action_id) {
                    item.label = label;
                }
            }
        }
    }

    /// Refresh menu-bar status items for battery, volume, and network (best-effort).
    ///
    /// Safe on hosts without UPower / Pulse / NetworkManager — labels fall back
    /// to placeholders. Call from shell idle `update` (throttled) and after
    /// volume / network connect actions.
    pub fn refresh_status_items(&mut self) {
        use crate::audio::{get_volume, volume_status_label};
        use crate::network_manager::get_network_status;
        use crate::power::battery_info;

        self.status_items.clear();

        let battery = battery_info();
        self.status_items.push(StatusItem {
            id: "battery".to_string(),
            label: battery_status_label(battery.percentage),
            icon: Some("battery".to_string()),
            priority: 10,
        });

        let volume_pct = get_volume().ok();
        self.status_items.push(StatusItem {
            id: "volume".to_string(),
            label: volume_status_label(volume_pct),
            icon: Some("volume".to_string()),
            priority: 15,
        });

        let net = get_network_status();
        let net_label = network_status_label(
            net.available,
            net.primary_connection_name.as_deref(),
            net.state.as_str(),
        );
        self.status_items.push(StatusItem {
            id: "network".to_string(),
            label: net_label,
            icon: Some("network".to_string()),
            priority: 20,
        });
    }

    /// Replace the generated Window-menu Space rows with the latest ordered
    /// compositor projection. App-provided Window items before the generated
    /// rows are preserved; dynamic rows keep index actions so keyboard/menu
    /// dispatch remains compatible with the shell's local mirror.
    pub fn set_workspace_items(&mut self, workspaces: &[(usize, String)]) {
        let Some(window_menu) = self.menus.iter_mut().find(|menu| menu.title == "Window") else {
            return;
        };
        let prefix_end = window_menu
            .items
            .iter()
            .position(|item| item.action_id == "workspace.previous")
            .unwrap_or(window_menu.items.len());
        let mut prefix = window_menu.items[..prefix_end].to_vec();
        if !prefix.is_empty()
            && !matches!(
                prefix.last().map(|item| &item.kind),
                Some(MenuItemKind::Separator)
            )
        {
            prefix.push(MenuItem::separator());
        }
        let mut rebuilt = Menu::new("Window");
        rebuilt.items = prefix;
        append_workspace_items_for(&mut rebuilt, workspaces);
        *window_menu = rebuilt;
    }

    fn setup_default_menus(&mut self) {
        // Locale from LANG (or defaults); settings.conf locale applied at shell startup
        // via `apply_locale_to_system_menu` when conf is loaded.
        let locale = LocalePrefs::parse_from_env_lang(std::env::var("LANG").ok().as_deref());
        let loc = &locale.locale;

        let mut system_menu = Menu::new("SLOPOS");
        system_menu
            .add_action("About SLOPOS-I")
            .with_action("shell.about");
        system_menu.add_separator();
        system_menu
            .add_action("System Settings...")
            .with_action("shell.settings");
        system_menu
            .add_action("Software Catalog...")
            .with_action("shell.software_catalog");
        system_menu
            .add_action("Connect Network…")
            .with_action("shell.network_connect");
        system_menu.add_separator();
        system_menu
            .add_action("Notification Center...")
            .with_action("shell.notification_center");
        system_menu
            .add_action("Clear Notifications")
            .with_action("shell.clear_notifications");
        system_menu.add_separator();
        system_menu
            .add_action(tr("menu.lock_screen", loc))
            .with_action("shell.lock")
            .with_shortcut(
                KeyCode::L,
                Modifiers {
                    shift: false,
                    control: true,
                    alt: false,
                    meta: true,
                },
            );
        system_menu.add_separator();
        system_menu
            .add_action("Force Quit...")
            .with_action("shell.force_quit")
            .with_shortcut(
                KeyCode::Escape,
                Modifiers {
                    shift: false,
                    control: false,
                    alt: true,
                    meta: true,
                },
            );
        system_menu.add_separator();
        system_menu
            .add_action(tr("menu.suspend", loc))
            .with_action("shell.suspend");
        system_menu
            .add_action(tr("menu.reboot", loc))
            .with_action("shell.reboot");
        system_menu
            .add_action(tr("menu.power_off", loc))
            .with_action("shell.power_off");
        system_menu.add_separator();
        system_menu
            .add_action(tr("menu.log_out", loc))
            .with_action("shell.log_out")
            .with_shortcut(
                KeyCode::Q,
                Modifiers {
                    shift: true,
                    control: false,
                    alt: false,
                    meta: true,
                },
            );
        system_menu
            .add_action(tr("menu.quit", loc))
            .with_action("shell.quit")
            .with_shortcut(
                KeyCode::Q,
                Modifiers {
                    shift: false,
                    control: false,
                    alt: false,
                    meta: true,
                },
            );

        let mut file_menu = Menu::new("File");
        file_menu
            .add_action("New")
            .with_action("shell.new_finder_window")
            .with_shortcut(
                KeyCode::N,
                Modifiers {
                    shift: false,
                    control: false,
                    alt: false,
                    meta: true,
                },
            );
        file_menu
            .add_action("Open...")
            .with_action("shell.open_finder")
            .with_shortcut(
                KeyCode::O,
                Modifiers {
                    shift: false,
                    control: false,
                    alt: false,
                    meta: true,
                },
            );
        file_menu
            .add_action("Close Window")
            .with_action("shell.close_finder_window")
            .with_shortcut(
                KeyCode::W,
                Modifiers {
                    shift: false,
                    control: false,
                    alt: false,
                    meta: true,
                },
            );
        // Screenshot and recording remain intentionally absent until the
        // compositor-owned screenshot/portal paths provide a real framebuffer
        // capture and a real PipeWire stream.  Keeping the commands hidden is
        // safer than presenting host-X11 helpers as production desktop media.

        let mut view_menu = Menu::new("View");
        view_menu
            .add_action("Enter Fullscreen")
            .with_action("shell.toggle_fullscreen")
            .with_shortcut(
                KeyCode::F,
                Modifiers {
                    shift: false,
                    control: false,
                    alt: false,
                    meta: true,
                },
            );

        let window_menu = workspace_window_menu();

        let help_menu = Menu::new("Help");
        // Help search is not exposed until an indexed help service exists.

        self.menus = vec![system_menu, file_menu, view_menu, window_menu, help_menu];
    }

    pub fn set_app_menus(&mut self, app_id: &str, menus: Vec<Menu>) {
        self.active_app = Some(app_id.to_string());
        while self.menus.len() > 1 {
            self.menus.pop();
        }
        let menus = ensure_workspace_window_menu(menus);
        for menu in menus {
            self.menus.push(menu);
        }
    }

    pub fn apply_menu_manifest(&mut self, manifest: MenuManifest) {
        let bundle_id = manifest.bundle_id;
        let menus = manifest.menus;
        self.app_menus.insert(bundle_id.clone(), menus.clone());
        if self.active_app.as_deref() == Some(bundle_id.as_str()) {
            self.set_app_menus(&bundle_id, menus);
        }
    }

    pub fn load_menu_manifest<P: AsRef<Path>>(&mut self, path: P) -> std::io::Result<()> {
        let content = fs::read_to_string(path)?;
        let manifest: MenuManifest =
            serde_json::from_str(&content).map_err(std::io::Error::other)?;
        self.apply_menu_manifest(manifest);
        Ok(())
    }

    pub fn load_menu_manifests_from_dir<P: AsRef<Path>>(
        &mut self,
        dir: P,
    ) -> std::io::Result<usize> {
        let dir = dir.as_ref();
        if !dir.exists() {
            return Ok(0);
        }

        let mut loaded = 0;
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            self.load_menu_manifest(&path)?;
            loaded += 1;
        }
        Ok(loaded)
    }

    pub fn reset_to_shell_menus(&mut self) {
        self.active_app = None;
        self.menus.clear();
        self.setup_default_menus();
    }

    pub fn set_active_app_menus(&mut self, app_id: &str) {
        if let Some(menus) = self.app_menus.get(app_id).cloned() {
            self.set_app_menus(app_id, menus);
            return;
        }

        let title = match app_id {
            "com.slopos.finder" => "Finder",
            "com.slopos.textedit" => "TextEdit",
            "com.slopos.terminal" => "Terminal",
            "com.slopos.settings" => "Settings",
            "com.slopos.appstore" => "App Store",
            _ => "Application",
        };

        let mut app_menu = Menu::new(title);
        let about_action = format!("{app_id}.about");
        let hide_action = format!("{app_id}.hide");
        let quit_action = format!("{app_id}.quit");
        app_menu
            .add_action(format!("About {title}"))
            .with_action(&about_action);
        app_menu.add_separator();
        app_menu
            .add_action(format!("Hide {title}"))
            .with_action(&hide_action);
        app_menu.add_separator();
        app_menu
            .add_action(format!("Quit {title}"))
            .with_action(&quit_action);

        let mut file_menu = Menu::new("File");
        file_menu
            .add_action("New Folder")
            .with_action("finder.new_folder")
            .with_shortcut(
                KeyCode::N,
                Modifiers {
                    shift: true,
                    control: false,
                    alt: false,
                    meta: true,
                },
            );
        file_menu
            .add_action("New Window")
            .with_action("shell.new_finder_window")
            .with_shortcut(
                KeyCode::N,
                Modifiers {
                    shift: false,
                    control: false,
                    alt: false,
                    meta: true,
                },
            );
        file_menu.add_separator();
        file_menu
            .add_action("Get Info")
            .with_action("finder.get_info")
            .with_shortcut(
                KeyCode::I,
                Modifiers {
                    shift: false,
                    control: false,
                    alt: false,
                    meta: true,
                },
            );
        file_menu
            .add_action("Move to Trash")
            .with_action("finder.move_to_trash")
            .with_shortcut(
                KeyCode::Delete,
                Modifiers {
                    shift: false,
                    control: false,
                    alt: false,
                    meta: true,
                },
            );
        file_menu.add_separator();
        file_menu
            .add_action("Close Window")
            .with_action("shell.close_finder_window")
            .with_shortcut(
                KeyCode::W,
                Modifiers {
                    shift: false,
                    control: false,
                    alt: false,
                    meta: true,
                },
            );

        let mut edit_menu = Menu::new("Edit");
        edit_menu.add_action("Cut").with_shortcut(
            KeyCode::X,
            Modifiers {
                shift: false,
                control: false,
                alt: false,
                meta: true,
            },
        );
        edit_menu.add_action("Copy").with_shortcut(
            KeyCode::C,
            Modifiers {
                shift: false,
                control: false,
                alt: false,
                meta: true,
            },
        );
        edit_menu.add_action("Paste").with_shortcut(
            KeyCode::V,
            Modifiers {
                shift: false,
                control: false,
                alt: false,
                meta: true,
            },
        );
        edit_menu.add_action("Select All").with_shortcut(
            KeyCode::A,
            Modifiers {
                shift: false,
                control: false,
                alt: false,
                meta: true,
            },
        );

        let mut view_menu = Menu::new("View");
        view_menu
            .add_action("Enter Fullscreen")
            .with_action("shell.toggle_fullscreen")
            .with_shortcut(
                KeyCode::F,
                Modifiers {
                    shift: false,
                    control: false,
                    alt: false,
                    meta: true,
                },
            );

        let mut window_menu = Menu::new("Window");
        window_menu.add_action("Minimize");
        window_menu
            .add_action("Zoom")
            .with_action("shell.zoom_window");

        let mut help_menu = Menu::new("Help");
        help_menu.add_action(format!("{title} Help"));

        let mut menus = vec![app_menu, file_menu, edit_menu, view_menu];
        if app_id == "com.slopos.finder" {
            let mut go_menu = Menu::new("Go");
            go_menu.add_action("Home").with_action("shell.open_home");
            go_menu
                .add_action("Computer")
                .with_action("shell.open_computer");
            menus.push(go_menu);
        }
        menus.push(window_menu);
        menus.push(help_menu);

        self.set_app_menus(app_id, menus);
    }

    pub fn add_status_item(&mut self, item: StatusItem) {
        self.status_items.push(item);
        self.status_items
            .sort_by_key(|i| std::cmp::Reverse(i.priority));
    }

    pub fn register_shortcut(&mut self, binding: ShortcutBinding) {
        self.keyboard_shortcuts.push(binding);
    }

    pub fn lookup_shortcut(&self, key: KeyCode, modifiers: Modifiers) -> Option<&ShortcutBinding> {
        self.keyboard_shortcuts
            .iter()
            .find(|s| s.key == key && s.modifiers == modifiers)
    }

    pub fn action_for_shortcut(&self, key: KeyCode, modifiers: Modifiers) -> Option<String> {
        self.menus
            .iter()
            .find_map(|menu| find_shortcut_action(&menu.items, key, modifiers))
    }

    pub fn render_menu_bar(&self) -> slopos_render::RenderNode {
        let mut children = vec![];
        children.push(slopos_render::RenderNode::Rect {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 24.0,
            color: slopos_render::Color::new(0.9, 0.9, 0.9, 1.0),
            corner_radius: 0.0,
        });

        let mut x = 10.0;
        for menu in &self.menus {
            children.push(slopos_render::RenderNode::Text {
                x,
                y: 16.0,
                text: menu.title.clone(),
                font_size: 13.0,
                color: slopos_render::Color::BLACK,
            });
            x += menu_title_advance(&menu.title);
        }

        slopos_render::RenderNode::Group { children }
    }
}

fn ensure_workspace_window_menu(mut menus: Vec<Menu>) -> Vec<Menu> {
    if let Some(window_menu) = menus.iter_mut().find(|menu| menu.title == "Window") {
        if !window_menu
            .items
            .iter()
            .any(|item| item.action_id == "workspace.next")
        {
            window_menu.add_separator();
            append_workspace_items(window_menu);
        }
    } else {
        menus.push(workspace_window_menu());
    }
    menus
}

fn workspace_window_menu() -> Menu {
    let mut window_menu = Menu::new("Window");
    append_workspace_items(&mut window_menu);
    window_menu
}

fn append_workspace_items(window_menu: &mut Menu) {
    let defaults = (0..8)
        .map(|index| (index, format!("Desktop {}", index + 1)))
        .collect::<Vec<_>>();
    append_workspace_items_for(window_menu, &defaults);
}

fn append_workspace_items_for(window_menu: &mut Menu, workspaces: &[(usize, String)]) {
    window_menu
        .add_action("Previous Workspace")
        .with_action("workspace.previous")
        .with_shortcut(
            KeyCode::ArrowLeft,
            Modifiers {
                shift: false,
                control: true,
                alt: true,
                meta: false,
            },
        );
    window_menu
        .add_action("Next Workspace")
        .with_action("workspace.next")
        .with_shortcut(
            KeyCode::ArrowRight,
            Modifiers {
                shift: false,
                control: true,
                alt: true,
                meta: false,
            },
        );
    window_menu.add_separator();
    for (index, name) in workspaces {
        let key = match *index {
            0 => Some(KeyCode::Key1),
            1 => Some(KeyCode::Key2),
            2 => Some(KeyCode::Key3),
            3 => Some(KeyCode::Key4),
            4 => Some(KeyCode::Key5),
            5 => Some(KeyCode::Key6),
            6 => Some(KeyCode::Key7),
            7 => Some(KeyCode::Key8),
            _ => None,
        };
        let action = format!("workspace.switch.{}", index);
        let item = window_menu.add_action(name.clone()).with_action(&action);
        if let Some(key) = key {
            item.with_shortcut(
                key,
                Modifiers {
                    shift: false,
                    control: true,
                    alt: true,
                    meta: false,
                },
            );
        }
    }
}

/// Pure battery menu-bar status label.
pub fn battery_status_label(percentage: Option<u8>) -> String {
    match percentage {
        Some(pct) => format!("🔋 {pct}%"),
        None => "🔋 —".to_string(),
    }
}

/// Pure network menu-bar status label.
pub fn network_status_label(available: bool, primary_name: Option<&str>, state: &str) -> String {
    if !available {
        return "📶 —".to_string();
    }
    match primary_name {
        Some(name) if !name.is_empty() => format!("📶 {name}"),
        _ => format!("📶 {state}"),
    }
}

/// How often the shell should re-query battery / volume / network for the menu bar.
pub const STATUS_REFRESH_INTERVAL_SECS: u64 = 5;

fn find_shortcut_action(items: &[MenuItem], key: KeyCode, modifiers: Modifiers) -> Option<String> {
    for item in items {
        if !item.enabled {
            continue;
        }
        if item.shortcut == Some((key, modifiers)) && !item.action_id.is_empty() {
            return Some(item.action_id.clone());
        }
        if matches!(item.kind, MenuItemKind::Submenu) {
            if let Some(submenu) = &item.submenu {
                if let Some(action) = find_shortcut_action(&submenu.items, key, modifiers) {
                    return Some(action);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use slopos_kit::menu::Menu;
    use slopos_sdk::MenuManifest;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn menu_server_loads_sdk_menu_manifest() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("slopos-i_menu_manifest_{unique}.json"));

        let mut file_menu = Menu::new("File");
        file_menu.add_action("New").with_action("com.test.app.new");
        let manifest = MenuManifest {
            app_name: "TestApp".to_string(),
            bundle_id: "com.test.app".to_string(),
            menus: vec![file_menu],
            updated_at_millis: 1,
        };
        fs::write(&path, serde_json::to_string(&manifest).unwrap()).unwrap();

        let mut server = MenuServer::new();
        server.load_menu_manifest(&path).unwrap();

        assert_eq!(server.active_app, None);
        assert!(server.app_menus.contains_key("com.test.app"));

        server.set_active_app_menus("com.test.app");

        assert_eq!(server.active_app.as_deref(), Some("com.test.app"));
        assert!(server.menus.iter().any(|menu| menu.title == "File"));
        assert!(server.menus.iter().any(|menu| {
            menu.items
                .iter()
                .any(|item| item.action_id == "com.test.app.new")
        }));

        let window_menu = server
            .menus
            .iter()
            .find(|menu| menu.title == "Window")
            .expect("workspace window menu");
        assert!(window_menu
            .items
            .iter()
            .any(|item| item.action_id == "workspace.next"));
        assert!(window_menu
            .items
            .iter()
            .any(|item| item.action_id == "workspace.switch.0"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn status_labels_are_pure() {
        assert_eq!(battery_status_label(Some(87)), "🔋 87%");
        assert_eq!(battery_status_label(None), "🔋 —");
        assert_eq!(
            network_status_label(true, Some("HomeWiFi"), "Full"),
            "📶 HomeWiFi"
        );
        assert_eq!(network_status_label(true, None, "Limited"), "📶 Limited");
        assert_eq!(network_status_label(false, None, "Unavailable"), "📶 —");
        assert_eq!(STATUS_REFRESH_INTERVAL_SECS, 5);
    }

    #[test]
    fn refresh_status_items_populates_battery_volume_network() {
        let mut server = MenuServer::new();
        server.refresh_status_items();
        let ids: Vec<&str> = server.status_items.iter().map(|s| s.id.as_str()).collect();
        assert!(ids.contains(&"battery"));
        assert!(ids.contains(&"volume"));
        assert!(ids.contains(&"network"));
        // Labels are non-empty placeholders even without host tools.
        for item in &server.status_items {
            assert!(!item.label.is_empty());
        }
    }

    #[test]
    fn render_menu_bar_advances_by_shaped_title_width() {
        let mut server = MenuServer::new();
        server.menus = vec![Menu::new("日本語"), Menu::new("Wiii")];

        let node = server.render_menu_bar();
        let slopos_render::RenderNode::Group { children } = node else {
            panic!("menu bar should render as a group");
        };
        let text_nodes = children
            .iter()
            .filter_map(|child| match child {
                slopos_render::RenderNode::Text { x, text, .. } => Some((*x, text.as_str())),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(text_nodes.len(), 2);
        assert_eq!(text_nodes[0].1, "日本語");
        assert_eq!(text_nodes[1].1, "Wiii");

        let expected = 10.0 + measure_text_width("日本語") + 15.0;
        assert!((text_nodes[1].0 - expected).abs() < 0.01);
    }
}
