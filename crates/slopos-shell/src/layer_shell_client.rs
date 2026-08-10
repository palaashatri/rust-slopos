//! Pure helpers mapping [`ChromeSession`] → layer-shell create requests.
//!
//! Live mapping of chrome pixels is owned by [`crate::layer_desktop`] (Phase 3).
//! The old gray-placeholder Wayland bind path was deleted — it raced Top layers
//! over the real desktop and never carried kit pixels.

use crate::chrome_protocol::{ChromeRole, ChromeSession};

/// One layer-shell surface the shell intends to map (pure request description).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayerShellChromeRequest {
    pub namespace: String,
    pub role: ChromeRole,
    /// `"background" | "bottom" | "top" | "overlay"`
    pub layer: String,
    pub width: u32,
    pub height: u32,
    pub exclusive_zone: i32,
    pub anchor_top: bool,
    pub anchor_bottom: bool,
    pub anchor_left: bool,
    pub anchor_right: bool,
}

/// Convert protocol chrome session into layer-shell create requests (pure).
///
/// Only **mapped** surfaces are included. Geometry matches exclusive zones /
/// anchors for top menu bar and bottom dock.
pub fn chrome_to_layer_shell_requests(session: &ChromeSession) -> Vec<LayerShellChromeRequest> {
    session
        .surfaces()
        .iter()
        .filter(|s| s.mapped)
        .map(|s| {
            let (anchor_top, anchor_bottom) = match s.role {
                ChromeRole::MenuBar | ChromeRole::NotificationOverlay => (true, false),
                ChromeRole::Dock => (false, true),
            };
            LayerShellChromeRequest {
                namespace: s.role.as_str().into(),
                role: s.role,
                layer: s.layer.clone(),
                width: s.width.max(0) as u32,
                height: s.height.max(0) as u32,
                exclusive_zone: s.exclusive_zone,
                anchor_top,
                anchor_bottom,
                anchor_left: true,
                anchor_right: true,
            }
        })
        .collect()
}

/// Result of a live layer-shell bind attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayerShellBindResult {
    pub mapped_namespaces: Vec<String>,
    pub wayland_display: String,
    pub layer_shell_global: bool,
}

/// Pure success description used by tests.
pub fn layer_shell_bind_summary(
    display: &str,
    requests: &[LayerShellChromeRequest],
    global_ok: bool,
) -> LayerShellBindResult {
    LayerShellBindResult {
        mapped_namespaces: requests.iter().map(|r| r.namespace.clone()).collect(),
        wayland_display: display.to_string(),
        layer_shell_global: global_ok,
    }
}

/// Deprecated: gray PoC bind removed. Real chrome is mapped by `layer_desktop`.
///
/// Always returns `None`. Kept so call sites and tests stay compile-safe.
pub fn try_map_layer_shell_chrome(_session: &ChromeSession) -> Option<LayerShellBindResult> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chrome_protocol::ChromeSession;

    #[test]
    fn chrome_to_layer_requests_menu_and_dock() {
        let session = ChromeSession::bootstrap_default(1280, 800, 24, 64);
        let reqs = chrome_to_layer_shell_requests(&session);
        assert_eq!(reqs.len(), 2);
        let menu = reqs.iter().find(|r| r.namespace == "menu-bar").unwrap();
        assert!(menu.anchor_top);
        assert!(!menu.anchor_bottom);
        assert_eq!(menu.height, 24);
        assert_eq!(menu.layer, "top");
        let dock = reqs.iter().find(|r| r.namespace == "dock").unwrap();
        assert!(dock.anchor_bottom);
        assert_eq!(dock.height, 64);
        assert_eq!(dock.exclusive_zone, 64);
        assert_eq!(dock.layer, "bottom");
    }

    #[test]
    fn unmapped_surfaces_omitted() {
        let mut session = ChromeSession::bootstrap_default(800, 600, 20, 40);
        session.unmap(ChromeRole::Dock);
        let reqs = chrome_to_layer_shell_requests(&session);
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].role, ChromeRole::MenuBar);
    }

    #[test]
    fn bind_summary_records_namespaces() {
        let session = ChromeSession::bootstrap_default(100, 100, 10, 10);
        let reqs = chrome_to_layer_shell_requests(&session);
        let summary = layer_shell_bind_summary("wayland-0", &reqs, true);
        assert!(summary.layer_shell_global);
        assert!(summary.mapped_namespaces.contains(&"menu-bar".into()));
        assert!(summary.mapped_namespaces.contains(&"dock".into()));
        assert_eq!(summary.wayland_display, "wayland-0");
    }

    #[test]
    fn try_map_is_retired_noop() {
        let session = ChromeSession::bootstrap_default(640, 480, 22, 48);
        assert!(try_map_layer_shell_chrome(&session).is_none());
    }
}
