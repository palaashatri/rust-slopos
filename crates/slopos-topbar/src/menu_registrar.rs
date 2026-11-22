//! The AppMenu registrar (`com.canonical.AppMenu.Registrar`) as a state
//! machine: which X window's menus live at which bus name and object path.
//!
//! A client calls `RegisterWindow(window, path)` on the registrar; the bus
//! name is the caller's, so a registration always belongs to the connection
//! that made it. The bar asks [`Registrar::resolve`] which menus to show for
//! the focused window, and the registrar answers the same way a window's
//! `_KDE_NET_WM_APPMENU_*` properties would.
//!
//! [`Registrar::apply`] is the whole transition function. Each [`Input`] (a
//! D-Bus call, an X server notification or a bus name change) either changes
//! the map and returns the [`Effect`]s the registrar must signal
//! (`WindowRegistered`, `WindowUnregistered`), or is refused. Phase 2 feeds it
//! from zbus and x11rb; nothing here touches a connection.
//!
//! Rules:
//! - A window belongs to one service at a time. A different service that
//!   tries to register the window is [`Refusal::Taken`] until the owner
//!   unregisters, the window is destroyed, or the owner's bus name goes away.
//!   Window ids are per X server, so one client owns one window, and a
//!   second bus name cannot take another application's menus.
//! - The same service registering the same window again is idempotent when
//!   the path is unchanged, and replaces the path otherwise.
//! - A service may only unregister its own windows; other calls are ignored.
//! - A destroyed window leaves the map whoever owned it.
//! - When a bus name vanishes, every window it registered leaves the map, in
//!   window-id order, so a crashed application never leaves stale menus.
//! - The map holds at most [`MAX_WINDOWS`] entries.

use std::collections::BTreeMap;

/// Most windows the registrar holds at once.
pub const MAX_WINDOWS: usize = 1_024;

/// Where a window's menus are: the bus name and object path to query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuRef {
    pub service: String,
    pub path: String,
}

/// Something the registrar is asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// `RegisterWindow(window, path)` from `sender`, the caller's unique name.
    Register {
        sender: String,
        window: u32,
        path: String,
    },
    /// `UnregisterWindow(window)` from `sender`.
    Unregister { sender: String, window: u32 },
    /// The X server reports that `window` was destroyed.
    WindowDestroyed { window: u32 },
    /// The bus reports that `name` no longer has an owner.
    ServiceVanished { name: String },
}

/// A D-Bus signal the registrar must emit because its map changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// `WindowRegistered(window, service, path)`.
    Registered { window: u32, menu: MenuRef },
    /// `WindowUnregistered(window)`.
    Unregistered { window: u32 },
}

/// Why an input was refused. The map is unchanged in every case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// Window 0 is "no window" in X.
    BadWindow,
    /// The sender is not a unique bus name, or the name is malformed.
    BadService,
    /// The object path is not a valid D-Bus object path.
    BadPath,
    /// Another service owns this window's menus.
    Taken,
    /// The registrar already holds [`MAX_WINDOWS`] windows.
    Full,
}

/// The window-to-menu map.
#[derive(Debug, Default)]
pub struct Registrar {
    windows: BTreeMap<u32, MenuRef>,
}

impl Registrar {
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply one input, returning the signals to emit or the refusal.
    pub fn apply(&mut self, input: Input) -> Result<Vec<Effect>, Refusal> {
        match input {
            Input::Register {
                sender,
                window,
                path,
            } => self.register(sender, window, path),
            Input::Unregister { sender, window } => Ok(self.unregister(&sender, window)),
            Input::WindowDestroyed { window } => Ok(self.remove(window).into_iter().collect()),
            Input::ServiceVanished { name } => Ok(self.service_vanished(&name)),
        }
    }

    fn register(
        &mut self,
        sender: String,
        window: u32,
        path: String,
    ) -> Result<Vec<Effect>, Refusal> {
        if window == 0 {
            return Err(Refusal::BadWindow);
        }
        if !is_unique_name(&sender) {
            return Err(Refusal::BadService);
        }
        if !is_object_path(&path) {
            return Err(Refusal::BadPath);
        }
        if let Some(current) = self.windows.get(&window) {
            if current.service != sender {
                return Err(Refusal::Taken);
            }
            if current.path == path {
                return Ok(Vec::new());
            }
        } else if self.windows.len() >= MAX_WINDOWS {
            return Err(Refusal::Full);
        }
        let menu = MenuRef {
            service: sender,
            path,
        };
        self.windows.insert(window, menu.clone());
        Ok(vec![Effect::Registered { window, menu }])
    }

    fn unregister(&mut self, sender: &str, window: u32) -> Vec<Effect> {
        match self.windows.get(&window) {
            Some(current) if current.service == sender => self.remove(window).into_iter().collect(),
            _ => Vec::new(),
        }
    }

    fn remove(&mut self, window: u32) -> Option<Effect> {
        self.windows
            .remove(&window)
            .map(|_| Effect::Unregistered { window })
    }

    fn service_vanished(&mut self, name: &str) -> Vec<Effect> {
        let gone: Vec<u32> = self
            .windows
            .iter()
            .filter(|(_, menu)| menu.service == name)
            .map(|(window, _)| *window)
            .collect();
        gone.into_iter()
            .filter_map(|window| self.remove(window))
            .collect()
    }

    /// The menus registered for `window`, if any.
    pub fn menu(&self, window: u32) -> Option<&MenuRef> {
        self.windows.get(&window)
    }

    /// The menus to show for `window`: the window's own
    /// `_KDE_NET_WM_APPMENU_*` pair when it sets a valid one, which is the
    /// most recent word from the application, and otherwise the registration.
    /// The caller checks that the service still has an owner before use.
    pub fn resolve(&self, window: u32, property: Option<MenuRef>) -> Option<MenuRef> {
        property
            .filter(|menu| is_name(&menu.service) && is_object_path(&menu.path))
            .or_else(|| self.menu(window).cloned())
    }

    pub fn len(&self) -> usize {
        self.windows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }
}

/// A unique bus name (`:1.42`), the only kind a caller's sender can be.
pub fn is_unique_name(name: &str) -> bool {
    name.strip_prefix(':')
        .is_some_and(|rest| is_name_body(rest, true))
        && name.len() <= 255
}

/// A bus name, unique or well-known (`org.example.App`).
pub fn is_name(name: &str) -> bool {
    if name.len() > 255 {
        return false;
    }
    match name.strip_prefix(':') {
        Some(rest) => is_name_body(rest, true),
        None => is_name_body(name, false) && name.contains('.'),
    }
}

/// Elements separated by dots, each `[A-Za-z0-9_-]+`. Well-known names may
/// not start an element with a digit; unique names may.
fn is_name_body(body: &str, unique: bool) -> bool {
    !body.is_empty()
        && body.split('.').all(|element| {
            !element.is_empty()
                && element
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                && (unique || !element.starts_with(|c: char| c.is_ascii_digit()))
        })
}

/// A D-Bus object path: `/` or `/` elements of `[A-Za-z0-9_]+`.
pub fn is_object_path(path: &str) -> bool {
    if path == "/" {
        return true;
    }
    path.strip_prefix('/').is_some_and(|rest| {
        !rest.is_empty()
            && rest.split('/').all(|element| {
                !element.is_empty()
                    && element
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_')
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP: &str = ":1.42";
    const OTHER: &str = ":1.77";
    const PATH: &str = "/com/example/menu";

    fn register(sender: &str, window: u32, path: &str) -> Input {
        Input::Register {
            sender: sender.to_owned(),
            window,
            path: path.to_owned(),
        }
    }

    fn menu(service: &str, path: &str) -> MenuRef {
        MenuRef {
            service: service.to_owned(),
            path: path.to_owned(),
        }
    }

    #[test]
    fn register_then_lookup_and_signal() {
        let mut registrar = Registrar::new();
        let effects = registrar.apply(register(APP, 0x40_0001, PATH)).unwrap();
        assert_eq!(
            effects,
            vec![Effect::Registered {
                window: 0x40_0001,
                menu: menu(APP, PATH),
            }]
        );
        assert_eq!(registrar.menu(0x40_0001), Some(&menu(APP, PATH)));
        assert_eq!(registrar.len(), 1);
        assert!(!registrar.is_empty());
        assert_eq!(registrar.menu(0x40_0002), None);
    }

    #[test]
    fn registering_the_same_menu_again_changes_nothing() {
        let mut registrar = Registrar::new();
        registrar.apply(register(APP, 5, PATH)).unwrap();
        assert_eq!(registrar.apply(register(APP, 5, PATH)), Ok(Vec::new()));
        assert_eq!(registrar.len(), 1);
    }

    #[test]
    fn the_owner_may_move_its_menu_to_another_path() {
        let mut registrar = Registrar::new();
        registrar.apply(register(APP, 5, PATH)).unwrap();
        let effects = registrar.apply(register(APP, 5, "/menus/two")).unwrap();
        assert_eq!(
            effects,
            vec![Effect::Registered {
                window: 5,
                menu: menu(APP, "/menus/two"),
            }]
        );
        assert_eq!(registrar.menu(5), Some(&menu(APP, "/menus/two")));
    }

    #[test]
    fn another_service_cannot_take_a_registered_window() {
        let mut registrar = Registrar::new();
        registrar.apply(register(APP, 5, PATH)).unwrap();
        assert_eq!(
            registrar.apply(register(OTHER, 5, "/evil")),
            Err(Refusal::Taken)
        );
        assert_eq!(registrar.menu(5), Some(&menu(APP, PATH)));
    }

    #[test]
    fn unregister_by_the_owner_removes_and_signals() {
        let mut registrar = Registrar::new();
        registrar.apply(register(APP, 5, PATH)).unwrap();
        let effects = registrar
            .apply(Input::Unregister {
                sender: APP.to_owned(),
                window: 5,
            })
            .unwrap();
        assert_eq!(effects, vec![Effect::Unregistered { window: 5 }]);
        assert_eq!(registrar.menu(5), None);
        assert!(registrar.is_empty());
    }

    #[test]
    fn unregister_by_someone_else_or_of_an_unknown_window_is_ignored() {
        let mut registrar = Registrar::new();
        registrar.apply(register(APP, 5, PATH)).unwrap();

        let effects = registrar
            .apply(Input::Unregister {
                sender: OTHER.to_owned(),
                window: 5,
            })
            .unwrap();
        assert!(effects.is_empty());
        assert_eq!(registrar.menu(5), Some(&menu(APP, PATH)));

        let effects = registrar
            .apply(Input::Unregister {
                sender: APP.to_owned(),
                window: 99,
            })
            .unwrap();
        assert!(effects.is_empty());
    }

    #[test]
    fn a_destroyed_window_leaves_the_map_whoever_owned_it() {
        let mut registrar = Registrar::new();
        registrar.apply(register(APP, 5, PATH)).unwrap();
        assert_eq!(
            registrar
                .apply(Input::WindowDestroyed { window: 5 })
                .unwrap(),
            vec![Effect::Unregistered { window: 5 }]
        );
        assert_eq!(registrar.menu(5), None);
        // A second report of the same destruction does nothing.
        assert!(registrar
            .apply(Input::WindowDestroyed { window: 5 })
            .unwrap()
            .is_empty());
    }

    #[test]
    fn a_vanished_service_takes_all_its_windows_and_no_others() {
        let mut registrar = Registrar::new();
        registrar.apply(register(APP, 30, PATH)).unwrap();
        registrar.apply(register(OTHER, 20, "/other")).unwrap();
        registrar.apply(register(APP, 10, PATH)).unwrap();

        let effects = registrar
            .apply(Input::ServiceVanished {
                name: APP.to_owned(),
            })
            .unwrap();
        assert_eq!(
            effects,
            vec![
                Effect::Unregistered { window: 10 },
                Effect::Unregistered { window: 30 },
            ]
        );
        assert_eq!(registrar.menu(10), None);
        assert_eq!(registrar.menu(30), None);
        assert_eq!(registrar.menu(20), Some(&menu(OTHER, "/other")));
    }

    #[test]
    fn a_vanished_service_with_no_windows_signals_nothing() {
        let mut registrar = Registrar::new();
        assert!(registrar
            .apply(Input::ServiceVanished {
                name: APP.to_owned()
            })
            .unwrap()
            .is_empty());
    }

    #[test]
    fn after_the_owner_dies_another_service_may_register_the_window() {
        let mut registrar = Registrar::new();
        registrar.apply(register(APP, 5, PATH)).unwrap();
        registrar
            .apply(Input::ServiceVanished {
                name: APP.to_owned(),
            })
            .unwrap();
        assert!(registrar.apply(register(OTHER, 5, "/fresh")).is_ok());
        assert_eq!(registrar.menu(5), Some(&menu(OTHER, "/fresh")));
    }

    #[test]
    fn invalid_registrations_are_refused_and_change_nothing() {
        let mut registrar = Registrar::new();
        assert_eq!(
            registrar.apply(register(APP, 0, PATH)),
            Err(Refusal::BadWindow)
        );
        assert_eq!(
            registrar.apply(register("org.example.Named", 5, PATH)),
            Err(Refusal::BadService)
        );
        assert_eq!(
            registrar.apply(register("", 5, PATH)),
            Err(Refusal::BadService)
        );
        assert_eq!(registrar.apply(register(APP, 5, "")), Err(Refusal::BadPath));
        assert_eq!(
            registrar.apply(register(APP, 5, "menu/no-slash")),
            Err(Refusal::BadPath)
        );
        assert_eq!(
            registrar.apply(register(APP, 5, "/trailing/")),
            Err(Refusal::BadPath)
        );
        assert!(registrar.is_empty());
    }

    #[test]
    fn the_map_is_bounded_but_an_existing_window_can_still_be_updated() {
        let mut registrar = Registrar::new();
        for window in 1..=MAX_WINDOWS as u32 {
            registrar.apply(register(APP, window, PATH)).unwrap();
        }
        assert_eq!(registrar.len(), MAX_WINDOWS);
        assert_eq!(
            registrar.apply(register(APP, MAX_WINDOWS as u32 + 1, PATH)),
            Err(Refusal::Full)
        );
        // Re-registering a window already held is not growth.
        assert_eq!(registrar.apply(register(APP, 1, PATH)), Ok(Vec::new()));
        assert_eq!(
            registrar
                .apply(register(APP, 1, "/moved"))
                .map(|effects| effects.len()),
            Ok(1)
        );
        // Dropping one makes room again.
        registrar
            .apply(Input::WindowDestroyed { window: 2 })
            .unwrap();
        assert!(registrar
            .apply(register(APP, MAX_WINDOWS as u32 + 1, PATH))
            .is_ok());
    }

    #[test]
    fn resolve_prefers_a_valid_property_then_the_registration() {
        let mut registrar = Registrar::new();
        registrar.apply(register(APP, 5, PATH)).unwrap();

        let property = menu(":1.99", "/window/props");
        assert_eq!(
            registrar.resolve(5, Some(property.clone())),
            Some(property.clone())
        );
        assert_eq!(registrar.resolve(5, None), Some(menu(APP, PATH)));
        // A malformed property is ignored in favour of the registration.
        assert_eq!(
            registrar.resolve(5, Some(menu("not a name", "/x"))),
            Some(menu(APP, PATH))
        );
        assert_eq!(
            registrar.resolve(5, Some(menu(":1.99", "relative"))),
            Some(menu(APP, PATH))
        );
        assert_eq!(registrar.resolve(9, None), None);
        // A property alone is enough for a window that never registered.
        assert_eq!(registrar.resolve(9, Some(property.clone())), Some(property));
    }

    #[test]
    fn unique_names_are_recognised() {
        assert!(is_unique_name(":1.42"));
        assert!(is_unique_name(":1.0-x_y"));
        assert!(!is_unique_name("1.42"));
        assert!(!is_unique_name(":"));
        assert!(!is_unique_name(":1..2"));
        assert!(!is_unique_name("org.example.App"));
        assert!(!is_unique_name(&format!(":{}", "a".repeat(300))));
    }

    #[test]
    fn bus_names_are_recognised() {
        assert!(is_name("org.example.App"));
        assert!(is_name("com.canonical.dbusmenu"));
        assert!(is_name(":1.42"));
        assert!(is_name("org.slopos.GlobalMenu"));
        assert!(!is_name("nodots"));
        assert!(!is_name("org.1example.App"));
        assert!(!is_name("org..App"));
        assert!(!is_name("org.example App"));
        assert!(!is_name(""));
    }

    #[test]
    fn object_paths_are_recognised() {
        assert!(is_object_path("/"));
        assert!(is_object_path("/com/canonical/menu/1"));
        assert!(is_object_path("/menu_bar"));
        assert!(!is_object_path(""));
        assert!(!is_object_path("menu"));
        assert!(!is_object_path("/a//b"));
        assert!(!is_object_path("/a/"));
        assert!(!is_object_path("/a-b"));
        assert!(!is_object_path("/a.b"));
    }
}
