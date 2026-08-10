//! Accessible actions for shell chrome (AT-SPI Action interface pure map).
//!
//! Maps focus targets and common widgets to named actions an AT can invoke.
//! Not a full Orca stack — provides stable action names + invoke plans.
//! Live path: kit `DoAction` → pending queue → shell `update()` drains via
//! [`resolve_pending_invoke`] / [`primary_invoke_for_chrome`].

use crate::chrome_protocol::ChromeFocusTarget;
use crate::session_actions::SessionAction;

/// Named accessible action (AT-SPI Action.name style).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccessibleAction {
    pub name: String,
    pub description: String,
    /// Shell menu / session action id when applicable.
    pub invoke_id: String,
}

/// Pure: actions available for a chrome focus region.
pub fn actions_for_chrome(target: ChromeFocusTarget) -> Vec<AccessibleAction> {
    match target {
        ChromeFocusTarget::MenuBar => vec![
            AccessibleAction {
                name: "click".into(),
                description: "Open the focused menu".into(),
                invoke_id: "chrome.menu.activate".into(),
            },
            AccessibleAction {
                name: "show-menu".into(),
                description: "Show the Retro system menu".into(),
                invoke_id: "chrome.menu.system".into(),
            },
        ],
        ChromeFocusTarget::Dock => vec![
            AccessibleAction {
                name: "click".into(),
                description: "Launch or focus the dock item".into(),
                invoke_id: "chrome.dock.activate".into(),
            },
            AccessibleAction {
                name: "show-menu".into(),
                description: "Show dock item context menu".into(),
                invoke_id: "chrome.dock.menu".into(),
            },
        ],
        ChromeFocusTarget::DesktopIcons => vec![
            AccessibleAction {
                name: "click".into(),
                description: "Open the selected desktop icon".into(),
                invoke_id: "chrome.desktop.open".into(),
            },
            AccessibleAction {
                name: "show-menu".into(),
                description: "Show desktop context menu".into(),
                invoke_id: "chrome.desktop.menu".into(),
            },
        ],
        ChromeFocusTarget::Windows => vec![
            AccessibleAction {
                name: "activate".into(),
                description: "Raise and focus the window".into(),
                invoke_id: "chrome.window.activate".into(),
            },
            AccessibleAction {
                name: "close".into(),
                description: "Close the focused window".into(),
                invoke_id: "chrome.window.close".into(),
            },
            AccessibleAction {
                name: "minimize".into(),
                description: "Minimize the focused window".into(),
                invoke_id: "chrome.window.minimize".into(),
            },
        ],
    }
}

/// Session-level accessible actions always available from the shell root.
pub fn session_root_actions() -> Vec<AccessibleAction> {
    vec![
        AccessibleAction {
            name: "lock-screen".into(),
            description: "Lock the session".into(),
            invoke_id: "shell.lock".into(),
        },
        AccessibleAction {
            name: "log-out".into(),
            description: "Log out of the session".into(),
            invoke_id: "shell.log_out".into(),
        },
        AccessibleAction {
            name: "notification-center".into(),
            description: "Open the notification center".into(),
            invoke_id: "shell.notification_center".into(),
        },
        AccessibleAction {
            name: "force-quit".into(),
            description: "Open Force Quit".into(),
            invoke_id: "shell.force_quit".into(),
        },
        AccessibleAction {
            name: "next-workspace".into(),
            description: "Switch to the next workspace".into(),
            invoke_id: "workspace.next".into(),
        },
        AccessibleAction {
            name: "previous-workspace".into(),
            description: "Switch to the previous workspace".into(),
            invoke_id: "workspace.previous".into(),
        },
    ]
}

/// Pure classification of an a11y invoke_id for shell dispatch / tests.
///
/// Distinguishes live handlers from log-only stubs so Orca DoAction coverage
/// is auditable without spinning up a full desktop.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum A11yDispatchTarget {
    /// Open the system/SLOPOS menu via kit MenuBar `open_menu_at` / `open_first_menu`.
    ChromeMenuActivate,
    /// Launch or focus first dock item.
    ChromeDockActivate,
    /// Dock item context menu — opens a status shell window listing dock items.
    ChromeDockMenu,
    /// Open selected desktop icon.
    ChromeDesktopOpen,
    /// Desktop context menu — opens a status shell window listing desktop icons.
    ChromeDesktopMenu,
    /// Focus next non-minimized window on the active workspace.
    ChromeWindowActivateNext,
    /// Close the active window.
    ChromeWindowClose,
    /// Minimize the active window.
    ChromeWindowMinimize,
    /// Route through [`crate::ShellDesktop`] menu/session handlers (live).
    ///
    /// Includes `shell.lock`, `shell.log_out`, `shell.notification_center`,
    /// `shell.force_quit`, `workspace.next` / `workspace.previous`, and other
    /// `shell.*` / `workspace.*` / `finder.*` action ids.
    MenuAction(&'static str),
    /// Dynamic menu action id (owned) — same live path as [`Self::MenuAction`].
    MenuActionOwned(String),
    /// Unrecognized invoke id.
    Unknown,
}

impl A11yDispatchTarget {
    /// Whether dispatch runs a real shell side effect (not just a debug log).
    pub fn is_live(&self) -> bool {
        match self {
            Self::Unknown => false,
            Self::ChromeMenuActivate
            | Self::ChromeDockActivate
            | Self::ChromeDockMenu
            | Self::ChromeDesktopOpen
            | Self::ChromeDesktopMenu
            | Self::ChromeWindowActivateNext
            | Self::ChromeWindowClose
            | Self::ChromeWindowMinimize
            | Self::MenuAction(_)
            | Self::MenuActionOwned(_) => true,
        }
    }

    /// Stable invoke id string when known.
    pub fn invoke_id(&self) -> Option<&str> {
        match self {
            Self::ChromeMenuActivate => Some("chrome.menu.activate"),
            Self::ChromeDockActivate => Some("chrome.dock.activate"),
            Self::ChromeDockMenu => Some("chrome.dock.menu"),
            Self::ChromeDesktopOpen => Some("chrome.desktop.open"),
            Self::ChromeDesktopMenu => Some("chrome.desktop.menu"),
            Self::ChromeWindowActivateNext => Some("chrome.window.activate"),
            Self::ChromeWindowClose => Some("chrome.window.close"),
            Self::ChromeWindowMinimize => Some("chrome.window.minimize"),
            Self::MenuAction(id) => Some(id),
            Self::MenuActionOwned(id) => Some(id.as_str()),
            Self::Unknown => None,
        }
    }
}

/// Pure: map invoke_id → dispatch target (no I/O).
pub fn classify_a11y_invoke(invoke_id: &str) -> A11yDispatchTarget {
    match invoke_id {
        "chrome.menu.activate" | "chrome.menu.system" => A11yDispatchTarget::ChromeMenuActivate,
        "chrome.dock.activate" => A11yDispatchTarget::ChromeDockActivate,
        "chrome.dock.menu" => A11yDispatchTarget::ChromeDockMenu,
        "chrome.desktop.open" => A11yDispatchTarget::ChromeDesktopOpen,
        "chrome.desktop.menu" => A11yDispatchTarget::ChromeDesktopMenu,
        "chrome.window.activate" => A11yDispatchTarget::ChromeWindowActivateNext,
        "chrome.window.close" => A11yDispatchTarget::ChromeWindowClose,
        "chrome.window.minimize" => A11yDispatchTarget::ChromeWindowMinimize,
        // Core daily-driver session actions (static for tests / Orca audit).
        "shell.lock" => A11yDispatchTarget::MenuAction("shell.lock"),
        "shell.log_out" | "shell.logout" => A11yDispatchTarget::MenuAction("shell.log_out"),
        "shell.notification_center" => A11yDispatchTarget::MenuAction("shell.notification_center"),
        "shell.force_quit" => A11yDispatchTarget::MenuAction("shell.force_quit"),
        "workspace.next" => A11yDispatchTarget::MenuAction("workspace.next"),
        "workspace.previous" => A11yDispatchTarget::MenuAction("workspace.previous"),
        other
            if other.starts_with("shell.")
                || other.starts_with("workspace.")
                || other.starts_with("finder.")
                || other.starts_with("com.slopos.") =>
        {
            A11yDispatchTarget::MenuActionOwned(other.to_string())
        }
        _ => A11yDispatchTarget::Unknown,
    }
}

/// True when Orca DoAction on this invoke_id reaches a live shell handler.
pub fn a11y_invoke_is_live(invoke_id: &str) -> bool {
    classify_a11y_invoke(invoke_id).is_live()
}

/// Map an accessible action invoke_id to a session action when applicable.
pub fn session_action_for_invoke(invoke_id: &str) -> Option<SessionAction> {
    SessionAction::from_menu_action(invoke_id)
}

/// AT-SPI Action interface summary for a path (tests / D-Bus serialize).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionInterfaceSummary {
    pub path: String,
    pub n_actions: i32,
    pub names: Vec<String>,
    pub descriptions: Vec<String>,
}

pub fn summarize_actions(path: &str, actions: &[AccessibleAction]) -> ActionInterfaceSummary {
    ActionInterfaceSummary {
        path: path.to_string(),
        n_actions: actions.len() as i32,
        names: actions.iter().map(|a| a.name.clone()).collect(),
        descriptions: actions.iter().map(|a| a.description.clone()).collect(),
    }
}

/// DoInvoke plan: which shell handler to call (pure).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvokePlan {
    pub invoke_id: String,
    pub valid: bool,
}

pub fn plan_invoke(actions: &[AccessibleAction], index: i32) -> InvokePlan {
    if index < 0 {
        return InvokePlan {
            invoke_id: String::new(),
            valid: false,
        };
    }
    match actions.get(index as usize) {
        Some(a) => InvokePlan {
            invoke_id: a.invoke_id.clone(),
            valid: true,
        },
        None => InvokePlan {
            invoke_id: String::new(),
            valid: false,
        },
    }
}

/// Primary (index 0) invoke for the focused chrome region — used by keyboard Activate.
pub fn primary_invoke_for_chrome(target: ChromeFocusTarget) -> InvokePlan {
    plan_invoke(&actions_for_chrome(target), 0)
}

/// Map an AT-SPI accessible path to a chrome focus target when it is a chrome region root.
///
/// Indices match [`slopos_kit::shell_chrome_accessibility_tree`]:
/// menu bar `0`, desktop `1`, dock `2`, window frame `3`.
pub fn chrome_target_for_atspi_path(path: &str) -> Option<ChromeFocusTarget> {
    let path = path.trim_end_matches('/');
    // Nested children (…/cN) are not the chrome root itself.
    if path.contains("/c") {
        return None;
    }
    match path {
        "/org/a11y/atspi/accessible/0" => Some(ChromeFocusTarget::MenuBar),
        "/org/a11y/atspi/accessible/1" => Some(ChromeFocusTarget::DesktopIcons),
        "/org/a11y/atspi/accessible/2" => Some(ChromeFocusTarget::Dock),
        "/org/a11y/atspi/accessible/3" => Some(ChromeFocusTarget::Windows),
        _ => None,
    }
}

/// Map a known accessible object name (menu item label / a11y Name) to a shell invoke id.
pub fn invoke_id_for_object_name(name: &str) -> Option<&'static str> {
    let n = name.trim();
    // English structural tree labels (register-time shell chrome tree).
    match n {
        "Lock Screen" | "lock-screen" => Some("shell.lock"),
        "Log Out…" | "Log Out..." | "log-out" => Some("shell.log_out"),
        "Sleep" | "Suspend" => Some("shell.suspend"),
        "Restart…" | "Restart..." | "Reboot" => Some("shell.reboot"),
        "Shut Down…" | "Shut Down..." | "Power Off" => Some("shell.power_off"),
        "Force Quit..." | "Force Quit…" => Some("shell.force_quit"),
        "Notification Center..." | "Notification Center…" => Some("shell.notification_center"),
        "System Settings..." | "System Settings…" => Some("shell.settings"),
        "About SLOPOS-I" => Some("shell.about"),
        "Quit SLOPOS-I" => Some("shell.quit"),
        "Next Workspace" | "next-workspace" => Some("workspace.next"),
        "Previous Workspace" | "previous-workspace" => Some("workspace.previous"),
        _ => None,
    }
}

/// Resolve a kit pending DoAction into a shell invoke plan (pure).
///
/// Priority:
/// 1. Known object Name → menu/session invoke id (nested menu items, buttons)
/// 2. Chrome region path + action index → [`actions_for_chrome`]
/// 3. Application root → session root actions by index
/// 4. Focus-only on chrome root still yields chrome primary when index is Focus
pub fn resolve_pending_invoke(
    path: &str,
    object_name: &str,
    action_index: i32,
    action_name: &str,
) -> InvokePlan {
    if let Some(id) = invoke_id_for_object_name(object_name) {
        return InvokePlan {
            invoke_id: id.into(),
            valid: true,
        };
    }

    if let Some(target) = chrome_target_for_atspi_path(path) {
        let actions = actions_for_chrome(target);
        // Kit exposes Focus as the only action on MenuBar/Dock/Desktop; map Focus
        // and Activate-style indices onto shell chrome invoke plans by index, or
        // primary on Focus (index 0 for kit Focus-only roles).
        if action_name.eq_ignore_ascii_case("Focus") || action_name.eq_ignore_ascii_case("Activate")
        {
            // Prefer primary shell action for region activation.
            let plan = primary_invoke_for_chrome(target);
            if plan.valid {
                return plan;
            }
        }
        return plan_invoke(&actions, action_index);
    }

    if path == "/org/a11y/atspi/accessible/root" || path.ends_with("/root") {
        return plan_invoke(&session_root_actions(), action_index);
    }

    InvokePlan {
        invoke_id: String::new(),
        valid: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chrome_and_session_actions() {
        let dock = actions_for_chrome(ChromeFocusTarget::Dock);
        assert!(dock.iter().any(|a| a.name == "click"));
        let root = session_root_actions();
        assert!(root.iter().any(|a| a.invoke_id == "shell.lock"));
        assert_eq!(
            session_action_for_invoke("shell.lock"),
            Some(SessionAction::Lock)
        );
    }

    #[test]
    fn invoke_plan_bounds() {
        let a = actions_for_chrome(ChromeFocusTarget::Windows);
        let p = plan_invoke(&a, 0);
        assert!(p.valid);
        assert!(!plan_invoke(&a, 99).valid);
        let s = summarize_actions("/org/a11y/atspi/accessible/3", &a);
        assert_eq!(s.n_actions, a.len() as i32);
    }

    #[test]
    fn resolve_pending_lock_by_name() {
        let p = resolve_pending_invoke(
            "/org/a11y/atspi/accessible/0/c0",
            "Lock Screen",
            0,
            "Activate",
        );
        assert!(p.valid);
        assert_eq!(p.invoke_id, "shell.lock");
    }

    #[test]
    fn resolve_chrome_path_primary() {
        let p = resolve_pending_invoke("/org/a11y/atspi/accessible/2", "Dock", 0, "Focus");
        assert!(p.valid);
        assert_eq!(p.invoke_id, "chrome.dock.activate");
        assert_eq!(
            chrome_target_for_atspi_path("/org/a11y/atspi/accessible/0"),
            Some(ChromeFocusTarget::MenuBar)
        );
        assert_eq!(
            primary_invoke_for_chrome(ChromeFocusTarget::MenuBar).invoke_id,
            "chrome.menu.activate"
        );
    }

    #[test]
    fn classify_daily_driver_dispatch_paths() {
        assert_eq!(
            classify_a11y_invoke("shell.lock"),
            A11yDispatchTarget::MenuAction("shell.lock")
        );
        assert_eq!(
            classify_a11y_invoke("shell.log_out"),
            A11yDispatchTarget::MenuAction("shell.log_out")
        );
        assert_eq!(
            classify_a11y_invoke("shell.logout"),
            A11yDispatchTarget::MenuAction("shell.log_out")
        );
        assert_eq!(
            classify_a11y_invoke("shell.notification_center"),
            A11yDispatchTarget::MenuAction("shell.notification_center")
        );
        assert_eq!(
            classify_a11y_invoke("shell.force_quit"),
            A11yDispatchTarget::MenuAction("shell.force_quit")
        );
        assert_eq!(
            classify_a11y_invoke("chrome.window.close"),
            A11yDispatchTarget::ChromeWindowClose
        );
        assert_eq!(
            classify_a11y_invoke("chrome.window.activate"),
            A11yDispatchTarget::ChromeWindowActivateNext
        );
        assert_eq!(
            classify_a11y_invoke("workspace.next"),
            A11yDispatchTarget::MenuAction("workspace.next")
        );
        assert_eq!(
            classify_a11y_invoke("workspace.previous"),
            A11yDispatchTarget::MenuAction("workspace.previous")
        );

        // Live vs log-only audit for Orca.
        assert!(a11y_invoke_is_live("shell.lock"));
        assert!(a11y_invoke_is_live("shell.log_out"));
        assert!(a11y_invoke_is_live("shell.notification_center"));
        assert!(a11y_invoke_is_live("shell.force_quit"));
        assert!(a11y_invoke_is_live("chrome.window.close"));
        assert!(a11y_invoke_is_live("chrome.window.activate"));
        assert!(a11y_invoke_is_live("workspace.next"));
        assert!(a11y_invoke_is_live("chrome.dock.activate"));
        assert!(a11y_invoke_is_live("chrome.desktop.open"));
        assert!(a11y_invoke_is_live("chrome.menu.activate"));
        assert!(a11y_invoke_is_live("chrome.menu.system"));
        assert!(a11y_invoke_is_live("chrome.dock.menu"));
        assert!(a11y_invoke_is_live("chrome.desktop.menu"));
        assert!(!a11y_invoke_is_live("not.a.real.action"));
    }

    #[test]
    fn primary_window_chrome_is_activate_next() {
        let plan = primary_invoke_for_chrome(ChromeFocusTarget::Windows);
        assert!(plan.valid);
        assert_eq!(plan.invoke_id, "chrome.window.activate");
        assert_eq!(
            classify_a11y_invoke(&plan.invoke_id),
            A11yDispatchTarget::ChromeWindowActivateNext
        );
    }

    #[test]
    fn session_root_includes_workspace_cycle() {
        let root = session_root_actions();
        assert!(root.iter().any(|a| a.invoke_id == "workspace.next"));
        assert!(root.iter().any(|a| a.invoke_id == "workspace.previous"));
        assert_eq!(
            invoke_id_for_object_name("Next Workspace"),
            Some("workspace.next")
        );
        assert_eq!(
            invoke_id_for_object_name("Previous Workspace"),
            Some("workspace.previous")
        );
    }
}
