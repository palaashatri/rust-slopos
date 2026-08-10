//! Native SLOPOS visual tokens inspired by the supplied Classic Macintosh UI
//! Kit.  These are measurements and relationships, not copied artwork or
//! proprietary font/asset dependencies.

use crate::theme::ThemeToken;
use slopos_render::Color;

// Renderer-facing equivalents of the semantic palette.  Keeping these in the
// kit lets immediate-mode presenters consume the same values as the widgets
// without importing a presenter-specific canvas type.
pub const CLASSIC_PAPER_RGBA: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
pub const CLASSIC_INK_RGBA: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
pub const CLASSIC_FACE_RGBA: [f32; 4] = [0.9372549, 0.9372549, 0.9372549, 1.0];
pub const CLASSIC_FACE_ALT_RGBA: [f32; 4] = [0.854902, 0.854902, 0.854902, 1.0];
pub const CLASSIC_MID_LIGHT_RGBA: [f32; 4] = [0.64705884, 0.64705884, 0.64705884, 1.0];
pub const CLASSIC_MID_RGBA: [f32; 4] = [0.5254902, 0.5254902, 0.5254902, 1.0];
pub const CLASSIC_DARK_GRAY_RGBA: [f32; 4] = [0.4, 0.4, 0.4, 1.0];
pub const CLASSIC_LAVENDER_RGBA: [f32; 4] = [0.854902, 0.854902, 0.9882353, 1.0];
pub const CLASSIC_LAVENDER_DARK_RGBA: [f32; 4] = [0.5294118, 0.5294118, 0.6901961, 1.0];
pub const CLASSIC_SELECTION_RGBA: [f32; 4] = [0.39, 0.59, 0.86, 1.0];
pub const CLASSIC_DESKTOP_RGBA: [f32; 4] = [0.59607846, 0.59607846, 0.5803922, 1.0];

/// Semantic colors for the restrained, high-contrast Classic appearance.
///
/// The palette deliberately contains roles rather than widget-specific paint
/// instructions.  A renderer or theme adapter can use these roles without
/// taking ownership of window state or backend policy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClassicPalette {
    pub paper: Color,
    pub ink: Color,
    pub desktop_background: Color,
    pub window_background: Color,
    pub window_border: Color,
    pub window_shadow: Color,
    pub window_title: Color,
    pub window_title_inactive: Color,
    pub menu_background: Color,
    pub menu_highlight: Color,
    pub menu_text: Color,
    pub button_background: Color,
    pub button_highlight: Color,
    pub button_shadow: Color,
    pub button_text: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub selection_background: Color,
    pub selection_text: Color,
    pub focus_ring: Color,
    pub disabled_text: Color,
    pub separator: Color,
}

impl ClassicPalette {
    /// Return the canonical SLOPOS Classic palette.
    pub const fn classic() -> Self {
        let paper = Color::WHITE;
        let ink = Color::BLACK;
        let face = Color::new(
            CLASSIC_FACE_RGBA[0],
            CLASSIC_FACE_RGBA[1],
            CLASSIC_FACE_RGBA[2],
            CLASSIC_FACE_RGBA[3],
        );
        let face_alt = Color::new(
            CLASSIC_FACE_ALT_RGBA[0],
            CLASSIC_FACE_ALT_RGBA[1],
            CLASSIC_FACE_ALT_RGBA[2],
            CLASSIC_FACE_ALT_RGBA[3],
        );
        let mid_gray = Color::new(
            CLASSIC_MID_RGBA[0],
            CLASSIC_MID_RGBA[1],
            CLASSIC_MID_RGBA[2],
            CLASSIC_MID_RGBA[3],
        );
        let dark_gray = Color::new(
            CLASSIC_DARK_GRAY_RGBA[0],
            CLASSIC_DARK_GRAY_RGBA[1],
            CLASSIC_DARK_GRAY_RGBA[2],
            CLASSIC_DARK_GRAY_RGBA[3],
        );
        let selection = Color::new(
            CLASSIC_SELECTION_RGBA[0],
            CLASSIC_SELECTION_RGBA[1],
            CLASSIC_SELECTION_RGBA[2],
            CLASSIC_SELECTION_RGBA[3],
        );

        Self {
            paper,
            ink,
            desktop_background: Color::new(
                CLASSIC_DESKTOP_RGBA[0],
                CLASSIC_DESKTOP_RGBA[1],
                CLASSIC_DESKTOP_RGBA[2],
                CLASSIC_DESKTOP_RGBA[3],
            ),
            window_background: paper,
            window_border: ink,
            window_shadow: dark_gray,
            window_title: face_alt,
            // Inactive title text still needs to remain legible against the
            // platinum title face; the lighter mid-gray token was too faint.
            window_title_inactive: dark_gray,
            menu_background: paper,
            menu_highlight: ink,
            menu_text: ink,
            button_background: face,
            button_highlight: paper,
            button_shadow: dark_gray,
            button_text: ink,
            text_primary: ink,
            text_secondary: dark_gray,
            selection_background: selection,
            selection_text: paper,
            focus_ring: selection,
            disabled_text: mid_gray,
            separator: dark_gray,
        }
    }

    /// Resolve a kit theme token to its Classic semantic color.
    pub const fn color(&self, token: ThemeToken) -> Color {
        match token {
            ThemeToken::WindowBackground => self.window_background,
            ThemeToken::WindowBorder => self.window_border,
            ThemeToken::WindowTitle => self.window_title,
            ThemeToken::WindowTitleInactive => self.window_title_inactive,
            ThemeToken::MenuBackground => self.menu_background,
            ThemeToken::MenuHighlight => self.menu_highlight,
            ThemeToken::MenuText => self.menu_text,
            ThemeToken::ButtonBackground => self.button_background,
            ThemeToken::ButtonHighlight => self.button_highlight,
            ThemeToken::ButtonShadow => self.button_shadow,
            ThemeToken::ButtonText => self.button_text,
            ThemeToken::TextPrimary => self.text_primary,
            ThemeToken::TextSecondary => self.text_secondary,
            ThemeToken::SelectionBackground => self.selection_background,
            ThemeToken::SelectionText => self.selection_text,
            ThemeToken::ScrollBar => self.button_shadow,
            ThemeToken::ScrollBarHover => self.button_highlight,
            ThemeToken::ToolbarBackground => self.window_background,
            ThemeToken::ToolbarBorder => self.window_border,
            ThemeToken::DialogBackground => self.window_background,
            ThemeToken::DialogBorder => self.window_border,
            ThemeToken::ProgressBarFill => self.selection_background,
            ThemeToken::ProgressBarTrack => self.button_shadow,
            ThemeToken::SliderTrack => self.button_shadow,
            ThemeToken::SliderThumb => self.button_highlight,
            ThemeToken::FocusRing => self.focus_ring,
            ThemeToken::StatusBarBackground => self.window_background,
            ThemeToken::IconBackground => self.paper,
            ThemeToken::DesktopBackground => self.desktop_background,
            ThemeToken::DockBackground => self.window_background,
            ThemeToken::DockHighlight => self.menu_highlight,
            ThemeToken::NotificationBackground => self.window_background,
            ThemeToken::NotificationBorder => self.window_border,
            ThemeToken::Separator => self.separator,
            ThemeToken::DisabledText => self.disabled_text,
            ThemeToken::LinkText => self.selection_background,
        }
    }
}

impl Default for ClassicPalette {
    fn default() -> Self {
        Self::classic()
    }
}

/// Canonical layout metrics for the Classic appearance.
///
/// Values are expressed in logical pixels.  The 19 px menu bar and 16 px
/// menu rhythm follow the supplied kit; the window values retain the existing
/// SLOPOS chrome proportions so current clients do not change behavior merely
/// by consuming this API.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClassicMetrics {
    pub menu_bar_height: f32,
    pub menu_bar_leading: f32,
    pub menu_title_horizontal_padding: f32,
    pub menu_item_gap: f32,
    pub menu_item_height: f32,
    pub menu_dropdown_vertical_padding: f32,
    pub menu_label_inset: f32,
    pub menu_shortcut_inset: f32,
    pub menu_shadow_offset: f32,
    pub window_border_width: f32,
    pub window_title_bar_height: f32,
    pub window_info_bar_height: f32,
    pub window_scrollbar_size: f32,
    pub window_shadow_offset: f32,
    pub window_control_size: f32,
    pub window_control_top: f32,
    pub window_control_inset: f32,
    pub window_resize_grip_size: f32,
}

impl ClassicMetrics {
    /// Return the canonical SLOPOS Classic metrics.
    pub const fn classic() -> Self {
        Self {
            menu_bar_height: 19.0,
            menu_bar_leading: 6.0,
            menu_title_horizontal_padding: 18.0,
            menu_item_gap: 0.0,
            menu_item_height: 16.0,
            menu_dropdown_vertical_padding: 4.0,
            menu_label_inset: 16.0,
            menu_shortcut_inset: 5.0,
            menu_shadow_offset: 3.0,
            window_border_width: 1.0,
            window_title_bar_height: 18.0,
            window_info_bar_height: 20.0,
            window_scrollbar_size: 15.0,
            window_shadow_offset: 1.0,
            window_control_size: 13.0,
            window_control_top: 5.0,
            window_control_inset: 11.0,
            window_resize_grip_size: 18.0,
        }
    }
}

impl Default for ClassicMetrics {
    fn default() -> Self {
        Self::classic()
    }
}

/// The canonical palette and metrics are intentionally copyable value APIs.
pub const CLASSIC_PALETTE: ClassicPalette = ClassicPalette::classic();
pub const CLASSIC_METRICS: ClassicMetrics = ClassicMetrics::classic();

/// Full-width global menu strip from the reference kit.
pub const MENU_BAR_HEIGHT_PX: u32 = 19;
pub const MENU_BAR_HEIGHT: f32 = CLASSIC_METRICS.menu_bar_height;
pub const MENU_BAR_LEADING: f32 = CLASSIC_METRICS.menu_bar_leading;
pub const MENU_TITLE_HORIZONTAL_PADDING: f32 = CLASSIC_METRICS.menu_title_horizontal_padding;
pub const MENU_ITEM_GAP: f32 = CLASSIC_METRICS.menu_item_gap;

/// Menu rows use a compact 16 px rhythm with a 1–2 px outer frame.
pub const MENU_ITEM_HEIGHT: f32 = CLASSIC_METRICS.menu_item_height;
pub const MENU_DROPDOWN_VERTICAL_PADDING: f32 = CLASSIC_METRICS.menu_dropdown_vertical_padding;
pub const MENU_LABEL_INSET: f32 = CLASSIC_METRICS.menu_label_inset;
pub const MENU_SHORTCUT_INSET: f32 = CLASSIC_METRICS.menu_shortcut_inset;
pub const MENU_SHADOW_OFFSET: f32 = CLASSIC_METRICS.menu_shadow_offset;

/// Document-window proportions from the reference kit, adapted for SLOPOS
/// controls and native application content.
pub const WINDOW_TITLE_BAR_HEIGHT: f32 = CLASSIC_METRICS.window_title_bar_height;
pub const WINDOW_INFO_BAR_HEIGHT: f32 = CLASSIC_METRICS.window_info_bar_height;
pub const WINDOW_SCROLLBAR_SIZE: f32 = CLASSIC_METRICS.window_scrollbar_size;
pub const WINDOW_SHADOW_OFFSET: f32 = CLASSIC_METRICS.window_shadow_offset;
pub const WINDOW_CONTROL_SIZE: f32 = CLASSIC_METRICS.window_control_size;
pub const WINDOW_CONTROL_TOP: f32 = CLASSIC_METRICS.window_control_top;
pub const WINDOW_CONTROL_INSET: f32 = CLASSIC_METRICS.window_control_inset;
pub const WINDOW_RESIZE_GRIP_SIZE: f32 = CLASSIC_METRICS.window_resize_grip_size;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_reference_rhythm_is_compact() {
        assert_eq!(MENU_BAR_HEIGHT_PX, 19);
        assert_eq!(MENU_ITEM_HEIGHT, 16.0);
        assert_eq!(WINDOW_SCROLLBAR_SIZE, 15.0);
        const { assert!(MENU_BAR_HEIGHT < 24.0) };
        assert_eq!(WINDOW_TITLE_BAR_HEIGHT, 18.0);
    }

    #[test]
    fn classic_palette_resolves_semantic_tokens() {
        let palette = CLASSIC_PALETTE;

        assert_eq!(palette.color(ThemeToken::MenuText), palette.ink);
        assert_eq!(
            palette.color(ThemeToken::WindowBackground),
            palette.window_background
        );
        assert_eq!(palette.color(ThemeToken::SelectionText), palette.paper);
        assert_eq!(
            palette.color(ThemeToken::DesktopBackground),
            palette.desktop_background
        );
        assert_eq!(
            palette.color(ThemeToken::WindowTitleInactive),
            palette.window_title_inactive
        );
        assert_eq!(palette.window_title_inactive, palette.window_shadow);
    }

    #[test]
    fn legacy_constants_are_backed_by_classic_metrics() {
        let metrics = CLASSIC_METRICS;

        assert_eq!(MENU_BAR_HEIGHT, metrics.menu_bar_height);
        assert_eq!(MENU_LABEL_INSET, metrics.menu_label_inset);
        assert_eq!(WINDOW_CONTROL_TOP, metrics.window_control_top);
        assert_eq!(WINDOW_RESIZE_GRIP_SIZE, metrics.window_resize_grip_size);
    }
}
