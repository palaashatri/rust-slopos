//! Keyboard-only navigation helpers for session chrome and window focus.
//!
//! Pure policy used by ShellDesktop event handling (Tab / Escape / Enter).

use crate::chrome_protocol::{next_chrome_focus, ChromeFocusTarget};

/// Keyboard shortcut intents for a11y / keyboard-only users.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyboardNavIntent {
    /// Advance chrome focus region (Tab).
    NextChromeRegion,
    /// Reverse chrome focus (Shift+Tab).
    PrevChromeRegion,
    /// Activate focused chrome / default action (Enter / Space).
    Activate,
    /// Dismiss overlays / clear transient UI (Escape).
    Dismiss,
    /// Cycle windows (Meta+Tab) — handled separately in shell.
    NextWindow,
    /// Lock screen (Meta+L / Ctrl+Meta+L).
    LockScreen,
    /// Log out (Meta+Shift+Q).
    LogOut,
    /// Next workspace (Meta+]).
    NextWorkspace,
    /// Previous workspace (Meta+[).
    PrevWorkspace,
}

/// Pure: map key + modifiers to a nav intent (no OS deps).
///
/// `shift` / `meta` / `control` / `alt` are plain bools so tests stay simple.
pub fn keyboard_nav_intent(
    key: &str,
    shift: bool,
    meta: bool,
    control: bool,
    alt: bool,
) -> Option<KeyboardNavIntent> {
    let key = key.trim().to_ascii_lowercase();
    if meta || control || alt {
        if meta && key == "tab" {
            return Some(KeyboardNavIntent::NextWindow);
        }
        // Meta+L or Ctrl+Meta+L → lock
        if meta && key == "l" {
            return Some(KeyboardNavIntent::LockScreen);
        }
        // Meta+Shift+Q → log out
        if meta && shift && key == "q" {
            return Some(KeyboardNavIntent::LogOut);
        }
        // Meta+] / Meta+[ → workspaces
        if meta && (key == "]" || key == "bracketright") {
            return Some(KeyboardNavIntent::NextWorkspace);
        }
        if meta && (key == "[" || key == "bracketleft") {
            return Some(KeyboardNavIntent::PrevWorkspace);
        }
        return None;
    }
    match key.as_str() {
        "tab" if shift => Some(KeyboardNavIntent::PrevChromeRegion),
        "tab" => Some(KeyboardNavIntent::NextChromeRegion),
        "escape" | "esc" => Some(KeyboardNavIntent::Dismiss),
        "enter" | "return" | "space" => Some(KeyboardNavIntent::Activate),
        _ => None,
    }
}

/// Pure: apply chrome region intent to current focus.
pub fn apply_chrome_nav(
    current: ChromeFocusTarget,
    intent: KeyboardNavIntent,
) -> ChromeFocusTarget {
    match intent {
        KeyboardNavIntent::NextChromeRegion => next_chrome_focus(current),
        KeyboardNavIntent::PrevChromeRegion => prev_chrome_focus(current),
        _ => current,
    }
}

fn prev_chrome_focus(current: ChromeFocusTarget) -> ChromeFocusTarget {
    use ChromeFocusTarget::*;
    // Reverse of next_chrome_focus: MenuBar ← DesktopIcons ← Windows ← Dock ← MenuBar
    match current {
        MenuBar => Dock,
        DesktopIcons => MenuBar,
        Windows => DesktopIcons,
        Dock => Windows,
    }
}

/// Whether Escape should close a transient UI (status / force-quit / about).
pub fn is_dismissable_window_title(title: &str) -> bool {
    matches!(title, "Force Quit" | "About SLOPOS-I" | "Help" | "Get Info")
        || title.starts_with("Status:")
        || title.starts_with("Dispatch:")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_and_shift_tab_intents() {
        assert_eq!(
            keyboard_nav_intent("Tab", false, false, false, false),
            Some(KeyboardNavIntent::NextChromeRegion)
        );
        assert_eq!(
            keyboard_nav_intent("tab", true, false, false, false),
            Some(KeyboardNavIntent::PrevChromeRegion)
        );
        assert_eq!(
            keyboard_nav_intent("Tab", false, true, false, false),
            Some(KeyboardNavIntent::NextWindow)
        );
    }

    #[test]
    fn escape_and_activate() {
        assert_eq!(
            keyboard_nav_intent("Escape", false, false, false, false),
            Some(KeyboardNavIntent::Dismiss)
        );
        assert_eq!(
            keyboard_nav_intent("Enter", false, false, false, false),
            Some(KeyboardNavIntent::Activate)
        );
        assert_eq!(
            keyboard_nav_intent("Space", false, false, false, false),
            Some(KeyboardNavIntent::Activate)
        );
    }

    #[test]
    fn lock_logout_workspace_intents() {
        assert_eq!(
            keyboard_nav_intent("l", false, true, false, false),
            Some(KeyboardNavIntent::LockScreen)
        );
        assert_eq!(
            keyboard_nav_intent("q", true, true, false, false),
            Some(KeyboardNavIntent::LogOut)
        );
        assert_eq!(
            keyboard_nav_intent("]", false, true, false, false),
            Some(KeyboardNavIntent::NextWorkspace)
        );
        assert_eq!(
            keyboard_nav_intent("[", false, true, false, false),
            Some(KeyboardNavIntent::PrevWorkspace)
        );
    }

    #[test]
    fn apply_chrome_nav_cycles_both_ways() {
        let mut f = ChromeFocusTarget::MenuBar;
        f = apply_chrome_nav(f, KeyboardNavIntent::NextChromeRegion);
        assert_eq!(f, ChromeFocusTarget::DesktopIcons);
        f = apply_chrome_nav(f, KeyboardNavIntent::PrevChromeRegion);
        assert_eq!(f, ChromeFocusTarget::MenuBar);
        f = apply_chrome_nav(f, KeyboardNavIntent::PrevChromeRegion);
        assert_eq!(f, ChromeFocusTarget::Dock);
    }

    #[test]
    fn dismissable_titles() {
        assert!(is_dismissable_window_title("Force Quit"));
        assert!(is_dismissable_window_title("Status: done"));
        assert!(!is_dismissable_window_title("Finder"));
    }
}
