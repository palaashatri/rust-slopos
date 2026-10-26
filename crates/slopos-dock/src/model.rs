//! What the Dock shows and what a click does, independent of X11 and pixels.

use slopos_core::desktop_entry::DesktopApp;

/// A top-level window the Dock may represent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub id: u32,
    /// `WM_CLASS` instance and class.
    pub instance: String,
    pub class: String,
    pub title: String,
    pub attention: bool,
}

/// One Dock tile.
#[derive(Debug, Clone)]
pub struct Item {
    /// The application, when one is known (always for pinned items).
    pub app: Option<DesktopApp>,
    pub pinned: bool,
    /// The item's windows, bottom-most first in stacking order.
    pub windows: Vec<Window>,
    /// Shown when there is no desktop entry for a running window.
    pub fallback_name: String,
}

impl Item {
    pub fn name(&self) -> &str {
        self.app
            .as_ref()
            .map(|app| app.name.as_str())
            .unwrap_or(&self.fallback_name)
    }

    pub fn icon(&self) -> &str {
        self.app
            .as_ref()
            .map(|app| app.icon.as_str())
            .filter(|icon| !icon.is_empty())
            .unwrap_or("application-x-executable")
    }

    pub fn attention(&self) -> bool {
        self.windows.iter().any(|window| window.attention)
    }

    pub fn is_active(&self, active: Option<u32>) -> bool {
        active.is_some_and(|active| self.windows.iter().any(|window| window.id == active))
    }
}

/// Pinned applications first, in the user's order, then running
/// applications that are not pinned, in the order their first window opened.
/// `windows` is in stacking order (bottom-most first); `catalog` is every
/// installed application, used to find icons and names for running windows.
pub fn build_items(pins: &[DesktopApp], windows: &[Window], catalog: &[DesktopApp]) -> Vec<Item> {
    let mut items: Vec<Item> = pins
        .iter()
        .map(|app| Item {
            app: Some(app.clone()),
            pinned: true,
            windows: Vec::new(),
            fallback_name: String::new(),
        })
        .collect();
    for window in windows {
        let owner = items.iter_mut().position(|item| match &item.app {
            Some(app) => app.matches_wm_class(&window.instance, &window.class),
            None => item.fallback_name.eq_ignore_ascii_case(&window.class),
        });
        match owner {
            Some(index) => items[index].windows.push(window.clone()),
            None => {
                let app = catalog
                    .iter()
                    .find(|app| app.matches_wm_class(&window.instance, &window.class))
                    .cloned();
                let fallback_name = if window.class.is_empty() {
                    window.title.clone()
                } else {
                    window.class.clone()
                };
                items.push(Item {
                    app,
                    pinned: false,
                    windows: vec![window.clone()],
                    fallback_name,
                });
            }
        }
    }
    items
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Launch(Vec<String>),
    Activate(u32),
    Nothing,
}

/// What a primary click on `item` does: start the app when it has no window;
/// raise its top-most window; when it is already in front, cycle to the
/// app's next window.
pub fn click(item: &Item, active: Option<u32>) -> Action {
    let Some(top) = item.windows.last() else {
        return launch(item);
    };
    match active {
        Some(active) if item.windows.len() > 1 && item.windows.iter().any(|w| w.id == active) => {
            // The bottom-most of the app's windows comes forward next, so
            // repeated clicks visit every window.
            Action::Activate(item.windows[0].id)
        }
        _ => Action::Activate(top.id),
    }
}

/// Middle click: always start another instance.
pub fn launch(item: &Item) -> Action {
    match &item.app {
        Some(app) if !app.argv.is_empty() => Action::Launch(app.argv.clone()),
        _ => Action::Nothing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, program: &str) -> DesktopApp {
        DesktopApp {
            id: id.into(),
            name: id.into(),
            argv: vec![program.into()],
            icon: String::new(),
            comment: String::new(),
            terminal: false,
            startup_wm_class: String::new(),
        }
    }

    fn window(id: u32, class: &str) -> Window {
        Window {
            id,
            instance: class.to_lowercase(),
            class: class.into(),
            title: format!("{class} window"),
            attention: false,
        }
    }

    #[test]
    fn pins_come_first_and_running_windows_join_their_app() {
        let pins = [app("xterm", "xterm"), app("firefox", "firefox")];
        let catalog = [app("org.slopos.Calculator", "slopos-calculator")];
        let windows = [
            window(1, "XTerm"),
            window(2, "Calculator"),
            window(3, "XTerm"),
            window(4, "Gimp"),
        ];
        let items = build_items(&pins, &windows, &catalog);
        let names: Vec<_> = items.iter().map(Item::name).collect();
        assert_eq!(names, ["xterm", "firefox", "org.slopos.Calculator", "Gimp"]);
        assert_eq!(items[0].windows.len(), 2);
        assert!(items[1].windows.is_empty());
        assert!(!items[2].pinned && items[2].app.is_some());
        assert!(items[3].app.is_none());
        assert_eq!(items[3].icon(), "application-x-executable");
    }

    #[test]
    fn clicking_launches_raises_or_cycles() {
        let pins = [app("xterm", "xterm")];
        let empty = build_items(&pins, &[], &[]);
        assert_eq!(click(&empty[0], None), Action::Launch(vec!["xterm".into()]));

        let one = build_items(&pins, &[window(7, "XTerm")], &[]);
        assert_eq!(click(&one[0], None), Action::Activate(7));
        assert_eq!(click(&one[0], Some(7)), Action::Activate(7));

        let two = build_items(&pins, &[window(1, "XTerm"), window(2, "XTerm")], &[]);
        // Not in front: raise the top-most. In front: bring the other forward.
        assert_eq!(click(&two[0], Some(99)), Action::Activate(2));
        assert_eq!(click(&two[0], Some(2)), Action::Activate(1));
        assert_eq!(launch(&two[0]), Action::Launch(vec!["xterm".into()]));
    }

    #[test]
    fn unknown_windows_cannot_be_launched_and_report_attention() {
        let mut urgent = window(5, "Mystery");
        urgent.attention = true;
        let items = build_items(&[], &[urgent], &[]);
        assert!(items[0].attention());
        assert!(items[0].is_active(Some(5)));
        assert_eq!(launch(&items[0]), Action::Nothing);
    }
}
