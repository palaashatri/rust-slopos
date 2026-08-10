use crate::Color;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThemeToken {
    WindowBackground,
    WindowBorder,
    WindowTitle,
    /// Title text used when a window is not the active/focused surface.
    WindowTitleInactive,
    MenuBackground,
    MenuHighlight,
    MenuText,
    ButtonBackground,
    ButtonHighlight,
    ButtonShadow,
    ButtonText,
    TextPrimary,
    TextSecondary,
    SelectionBackground,
    SelectionText,
    ScrollBar,
    ScrollBarHover,
    ToolbarBackground,
    ToolbarBorder,
    DialogBackground,
    DialogBorder,
    ProgressBarFill,
    ProgressBarTrack,
    SliderTrack,
    SliderThumb,
    FocusRing,
    StatusBarBackground,
    IconBackground,
    DesktopBackground,
    DockBackground,
    DockHighlight,
    NotificationBackground,
    NotificationBorder,
    Separator,
    DisabledText,
    LinkText,
}

#[derive(Debug, Clone)]
pub struct ThemeValue {
    pub color: Color,
    pub dark: Option<Color>,
    pub hdr: Option<Color>,
}

impl ThemeValue {
    pub fn new(color: Color) -> Self {
        Self {
            color,
            dark: None,
            hdr: None,
        }
    }

    pub fn with_dark(mut self, dark: Color) -> Self {
        self.dark = Some(dark);
        self
    }

    pub fn resolve(&self, is_dark: bool, is_hdr: bool) -> Color {
        if is_hdr {
            self.hdr.unwrap_or(if is_dark {
                self.dark.unwrap_or(self.color)
            } else {
                self.color
            })
        } else if is_dark {
            self.dark.unwrap_or(self.color)
        } else {
            self.color
        }
    }
}

#[derive(Debug, Clone)]
pub struct ThemePalette {
    pub name: String,
    pub is_dark: bool,
    pub tokens: HashMap<ThemeToken, ThemeValue>,
}

impl ThemePalette {
    pub fn get(&self, token: ThemeToken) -> Color {
        self.tokens
            .get(&token)
            .map(|v| v.resolve(self.is_dark, false))
            .unwrap_or(Color::BLACK)
    }
}

pub struct ThemeContext {
    pub current: ThemePalette,
    pub scale: f32,
    pub is_hdr: bool,
}

impl ThemeContext {
    pub fn new(palette: ThemePalette) -> Self {
        Self {
            current: palette,
            scale: 1.0,
            is_hdr: false,
        }
    }

    pub fn color(&self, token: ThemeToken) -> Color {
        self.current.get(token)
    }

    pub fn scaled(&self, value: f32) -> f32 {
        value * self.scale
    }
}
