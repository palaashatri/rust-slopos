//! Semantic colour roles (Design Bible §5.2: components read roles, never
//! primitives). The values are the provisional Platinum appearance; a theme
//! dataset can replace them without touching any control.

use crate::foundation::color::Color;

const fn rgb(red: u8, green: u8, blue: u8) -> Color {
    Color {
        red,
        green,
        blue,
        alpha: 0xff,
    }
}

/// The semantic roles controls paint with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// Window and panel background.
    pub surface: Color,
    /// Background of editable or list content (fields, lists, menus).
    pub content: Color,
    /// Normal text and outlines.
    pub ink: Color,
    /// Text and outlines of disabled controls.
    pub ink_disabled: Color,
    /// Secondary text such as placeholders, sizes and dates.
    pub ink_secondary: Color,
    /// Control borders.
    pub border: Color,
    /// Bevel light and shadow edges.
    pub bevel_light: Color,
    pub bevel_shadow: Color,
    /// Selected rows and text while the window is active.
    pub selection: Color,
    pub selection_ink: Color,
    /// Selected rows when the control does not have keyboard focus.
    pub selection_inactive: Color,
    /// Focus indication.
    pub focus: Color,
    /// Highlighted menu item.
    pub menu_highlight: Color,
    pub menu_highlight_ink: Color,
    /// Destructive or error emphasis.
    pub danger: Color,
    /// Scrollbar track and thumb.
    pub track: Color,
    pub thumb: Color,
    /// Dimming behind a modal dialog.
    pub scrim: Color,
}

/// The default Platinum appearance.
pub const PLATINUM: Palette = Palette {
    surface: rgb(0xdd, 0xdd, 0xdd),
    content: rgb(0xff, 0xff, 0xff),
    ink: rgb(0x00, 0x00, 0x00),
    ink_disabled: rgb(0x80, 0x80, 0x80),
    ink_secondary: rgb(0x55, 0x55, 0x55),
    border: rgb(0x00, 0x00, 0x00),
    bevel_light: rgb(0xff, 0xff, 0xff),
    bevel_shadow: rgb(0x88, 0x88, 0x88),
    selection: rgb(0xcc, 0xcc, 0xff),
    selection_ink: rgb(0x00, 0x00, 0x00),
    selection_inactive: rgb(0xdd, 0xdd, 0xdd),
    focus: rgb(0x00, 0x00, 0x00),
    menu_highlight: rgb(0x33, 0x33, 0x99),
    menu_highlight_ink: rgb(0xff, 0xff, 0xff),
    danger: rgb(0xbb, 0x00, 0x00),
    track: rgb(0xee, 0xee, 0xee),
    thumb: rgb(0xbb, 0xbb, 0xdd),
    scrim: Color {
        red: 0,
        green: 0,
        blue: 0,
        alpha: 0x55,
    },
};

/// The palette for the current process. Only Platinum exists today.
pub fn palette() -> &'static Palette {
    &PLATINUM
}
