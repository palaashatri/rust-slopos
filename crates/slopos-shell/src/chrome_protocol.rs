//! Protocol-backed session chrome: menu bar, dock, and notification overlay as
//! layer-shell role surfaces — not ShellWindow paint-rects.
//!
//! Pure geometry / session state; testable on any host (including macOS).
//! Kit paint for menu bar + dock is gated by [`should_paint_kit_chrome`]: when
//! layer-shell chrome is bound, menu/dock pixels live on exclusive Top/Bottom
//! surfaces (`layer_desktop`), not the kit canvas.

/// Layer-shell chrome role for protocol surfaces owned by the shell session.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ChromeRole {
    MenuBar,
    Dock,
    NotificationOverlay,
}

impl ChromeRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MenuBar => "menu-bar",
            Self::Dock => "dock",
            Self::NotificationOverlay => "notification-overlay",
        }
    }

    /// Default layer-shell layer name for this role.
    pub fn default_layer(self) -> &'static str {
        match self {
            Self::MenuBar => "top",
            Self::Dock => "bottom",
            Self::NotificationOverlay => "overlay",
        }
    }
}

/// One protocol chrome surface (layer-shell role), tracked by the session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtocolChromeSurface {
    pub id: String,
    pub role: ChromeRole,
    /// Layer name: `"top"`, `"overlay"`, `"bottom"`, `"background"`.
    pub layer: String,
    pub exclusive_zone: i32,
    pub width: i32,
    pub height: i32,
    pub mapped: bool,
}

/// Session registry of protocol chrome surfaces (menu bar / dock / overlays).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChromeSession {
    surfaces: Vec<ProtocolChromeSurface>,
    output_w: i32,
    output_h: i32,
}

impl ChromeSession {
    /// Bootstrap mapped menu bar (top, full-width) and dock (bottom, full-width)
    /// as protocol surfaces — not window paint-rects.
    pub fn bootstrap_default(output_w: i32, output_h: i32, menu_h: i32, dock_h: i32) -> Self {
        let output_w = output_w.max(0);
        let output_h = output_h.max(0);
        let menu_h = menu_h.max(0);
        let dock_h = dock_h.max(0);

        let menu = ProtocolChromeSurface {
            id: "chrome-menu-bar".into(),
            role: ChromeRole::MenuBar,
            layer: ChromeRole::MenuBar.default_layer().into(),
            exclusive_zone: menu_h,
            width: output_w,
            height: menu_h,
            mapped: true,
        };
        let dock = ProtocolChromeSurface {
            id: "chrome-dock".into(),
            role: ChromeRole::Dock,
            layer: ChromeRole::Dock.default_layer().into(),
            exclusive_zone: dock_h,
            width: output_w,
            height: dock_h,
            mapped: true,
        };

        Self {
            surfaces: vec![menu, dock],
            output_w,
            output_h,
        }
    }

    pub fn surfaces(&self) -> &[ProtocolChromeSurface] {
        &self.surfaces
    }

    pub fn output_size(&self) -> (i32, i32) {
        (self.output_w, self.output_h)
    }

    /// Map a chrome role surface (no-op if role not present).
    pub fn map(&mut self, role: ChromeRole) {
        if let Some(s) = self.surfaces.iter_mut().find(|s| s.role == role) {
            s.mapped = true;
        }
    }

    /// Unmap a chrome role surface (no-op if role not present).
    pub fn unmap(&mut self, role: ChromeRole) {
        if let Some(s) = self.surfaces.iter_mut().find(|s| s.role == role) {
            s.mapped = false;
        }
    }

    /// Whether this role is tracked as protocol chrome in the session.
    pub fn is_protocol_chrome(&self, role: ChromeRole) -> bool {
        self.surfaces.iter().any(|s| s.role == role)
    }

    /// Pure geometry for mapped chrome: menu bar top full-width, dock bottom full-width.
    /// Returns `(role, x, y, w, h)` in output coordinates.
    pub fn layout_rects(&self) -> Vec<(ChromeRole, i32, i32, i32, i32)> {
        let mut out = Vec::new();
        for s in &self.surfaces {
            if !s.mapped {
                continue;
            }
            let (x, y, w, h) = match s.role {
                ChromeRole::MenuBar => (0, 0, s.width, s.height),
                ChromeRole::Dock => {
                    let y = (self.output_h - s.height).max(0);
                    (0, y, s.width, s.height)
                }
                ChromeRole::NotificationOverlay => {
                    // Top-right overlay strip; width from surface, height from surface.
                    let w = s.width.min(self.output_w).max(0);
                    let x = (self.output_w - w).max(0);
                    (x, 0, w, s.height)
                }
            };
            out.push((s.role, x, y, w, h));
        }
        out
    }

    /// Ensure a notification overlay surface exists (unmapped by default until mapped).
    pub fn ensure_notification_overlay(&mut self, width: i32, height: i32) {
        if self.is_protocol_chrome(ChromeRole::NotificationOverlay) {
            if let Some(s) = self
                .surfaces
                .iter_mut()
                .find(|s| s.role == ChromeRole::NotificationOverlay)
            {
                s.width = width.max(0);
                s.height = height.max(0);
            }
            return;
        }
        self.surfaces.push(ProtocolChromeSurface {
            id: "chrome-notification-overlay".into(),
            role: ChromeRole::NotificationOverlay,
            layer: ChromeRole::NotificationOverlay.default_layer().into(),
            exclusive_zone: 0,
            width: width.max(0),
            height: height.max(0),
            mapped: false,
        });
    }
}

/// Whether the shell should still paint menu bar / dock with kit widgets.
///
/// When layer-shell chrome is bound, menu/dock are exclusive protocol surfaces
/// (`layer_desktop`); kit dual-paint on the desktop canvas must stay off.
pub fn should_paint_kit_chrome(layer_shell_bound: bool) -> bool {
    !layer_shell_bound
}

/// Compositor output size from `SLOPOS_COMPOSITOR_WIDTH` / `HEIGHT` (default 1024×768).
pub fn session_output_size() -> (i32, i32) {
    let w = std::env::var("SLOPOS_COMPOSITOR_WIDTH")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1024);
    let h = std::env::var("SLOPOS_COMPOSITOR_HEIGHT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(768);
    (w.max(1), h.max(1))
}

/// Keyboard-only focus ring order for session chrome + desktop content.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChromeFocusTarget {
    MenuBar,
    DesktopIcons,
    Windows,
    Dock,
}

/// Pure focus cycle for keyboard-only navigation (Orca-adjacent path).
pub fn chrome_focus_order() -> &'static [ChromeFocusTarget] {
    &[
        ChromeFocusTarget::MenuBar,
        ChromeFocusTarget::DesktopIcons,
        ChromeFocusTarget::Windows,
        ChromeFocusTarget::Dock,
    ]
}

/// Next focus target after `current` (wraps).
pub fn next_chrome_focus(current: ChromeFocusTarget) -> ChromeFocusTarget {
    let order = chrome_focus_order();
    let i = order.iter().position(|t| *t == current).unwrap_or(0);
    order[(i + 1) % order.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_paint_kit_chrome_off_when_layer_shell_bound() {
        assert!(should_paint_kit_chrome(false));
        assert!(!should_paint_kit_chrome(true));
    }

    #[test]
    fn chrome_focus_cycles() {
        assert_eq!(
            next_chrome_focus(ChromeFocusTarget::MenuBar),
            ChromeFocusTarget::DesktopIcons
        );
        assert_eq!(
            next_chrome_focus(ChromeFocusTarget::Dock),
            ChromeFocusTarget::MenuBar
        );
        assert_eq!(chrome_focus_order().len(), 4);
    }

    #[test]
    fn bootstrap_default_creates_mapped_menu_and_dock() {
        let session = ChromeSession::bootstrap_default(1280, 800, 24, 64);
        assert_eq!(session.surfaces().len(), 2);
        assert!(session.is_protocol_chrome(ChromeRole::MenuBar));
        assert!(session.is_protocol_chrome(ChromeRole::Dock));
        assert!(!session.is_protocol_chrome(ChromeRole::NotificationOverlay));

        let menu = session
            .surfaces()
            .iter()
            .find(|s| s.role == ChromeRole::MenuBar)
            .expect("menu");
        assert!(menu.mapped);
        assert_eq!(menu.layer, "top");
        assert_eq!(menu.exclusive_zone, 24);
        assert_eq!(menu.width, 1280);
        assert_eq!(menu.height, 24);

        let dock = session
            .surfaces()
            .iter()
            .find(|s| s.role == ChromeRole::Dock)
            .expect("dock");
        assert!(dock.mapped);
        assert_eq!(dock.layer, "bottom");
        assert_eq!(dock.exclusive_zone, 64);
        assert_eq!(dock.width, 1280);
        assert_eq!(dock.height, 64);
    }

    #[test]
    fn layout_rects_menu_top_dock_bottom_full_width() {
        let session = ChromeSession::bootstrap_default(1280, 800, 24, 64);
        let rects = session.layout_rects();
        assert_eq!(rects.len(), 2);

        let menu = rects
            .iter()
            .find(|(r, ..)| *r == ChromeRole::MenuBar)
            .copied()
            .expect("menu rect");
        assert_eq!(menu, (ChromeRole::MenuBar, 0, 0, 1280, 24));

        let dock = rects
            .iter()
            .find(|(r, ..)| *r == ChromeRole::Dock)
            .copied()
            .expect("dock rect");
        assert_eq!(dock, (ChromeRole::Dock, 0, 800 - 64, 1280, 64));
    }

    #[test]
    fn layer_assignment_menu_top_dock_bottom_overlay_for_notifications() {
        assert_eq!(ChromeRole::MenuBar.default_layer(), "top");
        assert_eq!(ChromeRole::Dock.default_layer(), "bottom");
        assert_eq!(ChromeRole::NotificationOverlay.default_layer(), "overlay");

        let mut session = ChromeSession::bootstrap_default(1024, 768, 28, 48);
        session.ensure_notification_overlay(320, 120);
        session.map(ChromeRole::NotificationOverlay);

        let overlay = session
            .surfaces()
            .iter()
            .find(|s| s.role == ChromeRole::NotificationOverlay)
            .expect("overlay");
        assert_eq!(overlay.layer, "overlay");
        assert_eq!(overlay.exclusive_zone, 0);
        assert!(overlay.mapped);

        let rects = session.layout_rects();
        let ov = rects
            .iter()
            .find(|(r, ..)| *r == ChromeRole::NotificationOverlay)
            .copied()
            .expect("overlay rect");
        // top-right
        assert_eq!(
            ov,
            (ChromeRole::NotificationOverlay, 1024 - 320, 0, 320, 120)
        );
    }

    #[test]
    fn map_unmap_toggles_mapped_and_layout_rects() {
        let mut session = ChromeSession::bootstrap_default(800, 600, 24, 64);
        assert_eq!(session.layout_rects().len(), 2);

        session.unmap(ChromeRole::MenuBar);
        assert!(
            !session
                .surfaces()
                .iter()
                .find(|s| s.role == ChromeRole::MenuBar)
                .unwrap()
                .mapped
        );
        assert_eq!(session.layout_rects().len(), 1);
        assert!(session
            .layout_rects()
            .iter()
            .all(|(r, ..)| *r == ChromeRole::Dock));

        session.map(ChromeRole::MenuBar);
        assert_eq!(session.layout_rects().len(), 2);
    }
}
