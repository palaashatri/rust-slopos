//! Pure accessibility preference helpers for contrast and reduced motion.
//!
//! Preferences are parsed from flat `key=value` lines in `settings.conf`.
//! No I/O, animation engine, or theme palette construction lives here — only
//! pure policy used by [`crate::theme_manager::ThemeManager`] and callers.

use crate::theme_manager::ThemeName;

/// Whether UI motion/animation should run at full duration or be reduced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MotionPreference {
    /// Normal animation durations.
    #[default]
    Full,
    /// Prefer minimal / zero motion (e.g. `prefers-reduced-motion`).
    Reduced,
}

/// Contrast preference for visual presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContrastPreference {
    /// Use the user-selected theme as-is.
    #[default]
    Normal,
    /// Force the high-contrast theme palette.
    High,
}

/// Accessibility preferences derived from settings (pure data).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct A11yPrefs {
    pub motion: MotionPreference,
    pub contrast: ContrastPreference,
}

impl A11yPrefs {
    /// Construct prefs from motion and contrast values.
    pub fn new(motion: MotionPreference, contrast: ContrastPreference) -> Self {
        Self { motion, contrast }
    }

    /// Parse prefs from flat `key=value` settings.conf text.
    ///
    /// Recognized keys (case-sensitive key, case-insensitive bool value):
    /// - `reduced_motion=true|false` — sets [`MotionPreference::Reduced`] when true
    /// - `high_contrast=true|false` — sets [`ContrastPreference::High`] when true
    ///
    /// Legacy aliases also accepted: `reduce_motion`, `increase_contrast`.
    /// Unknown keys and malformed lines are ignored. Defaults are Full / Normal.
    pub fn parse_from_conf(text: &str) -> Self {
        let mut prefs = Self::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim();
            match key {
                "reduced_motion" | "reduce_motion" => {
                    prefs.motion = if parse_conf_bool(value) {
                        MotionPreference::Reduced
                    } else {
                        MotionPreference::Full
                    };
                }
                "high_contrast" | "increase_contrast" => {
                    prefs.contrast = if parse_conf_bool(value) {
                        ContrastPreference::High
                    } else {
                        ContrastPreference::Normal
                    };
                }
                _ => {}
            }
        }
        prefs
    }
}

/// Parse a settings.conf boolean (`true`/`1`/`yes`/`on`, case-insensitive).
fn parse_conf_bool(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "true" | "1" | "yes" | "on"
    )
}

/// Apply contrast prefs to a theme selection.
///
/// When high contrast is requested, returns [`ThemeName::HighContrast`].
/// Otherwise returns `current_theme` unchanged.
pub fn apply_a11y_prefs_to_theme_name(prefs: A11yPrefs, current_theme: ThemeName) -> ThemeName {
    match prefs.contrast {
        ContrastPreference::High => ThemeName::HighContrast,
        ContrastPreference::Normal => current_theme,
    }
}

/// Effective animation duration in milliseconds.
///
/// Returns `0` when reduced motion is preferred; otherwise `base_ms`.
pub fn effective_animation_ms(prefs: A11yPrefs, base_ms: u32) -> u32 {
    match prefs.motion {
        MotionPreference::Reduced => 0,
        MotionPreference::Full => base_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_full_motion_normal_contrast() {
        let prefs = A11yPrefs::default();
        assert_eq!(prefs.motion, MotionPreference::Full);
        assert_eq!(prefs.contrast, ContrastPreference::Normal);
    }

    #[test]
    fn parse_reduced_motion_and_high_contrast() {
        let text = "\
theme=classic
reduced_motion=true
high_contrast=true
";
        let prefs = A11yPrefs::parse_from_conf(text);
        assert_eq!(prefs.motion, MotionPreference::Reduced);
        assert_eq!(prefs.contrast, ContrastPreference::High);
    }

    #[test]
    fn parse_false_keeps_defaults() {
        let text = "reduced_motion=false\nhigh_contrast=false\n";
        let prefs = A11yPrefs::parse_from_conf(text);
        assert_eq!(prefs.motion, MotionPreference::Full);
        assert_eq!(prefs.contrast, ContrastPreference::Normal);
    }

    #[test]
    fn parse_legacy_aliases() {
        let text = "reduce_motion=true\nincrease_contrast=true\n";
        let prefs = A11yPrefs::parse_from_conf(text);
        assert_eq!(prefs.motion, MotionPreference::Reduced);
        assert_eq!(prefs.contrast, ContrastPreference::High);
    }

    #[test]
    fn parse_ignores_comments_blanks_and_unknown() {
        let text = "\
# reduced_motion=true
bold_text=true

theme=dark
high_contrast=1
";
        let prefs = A11yPrefs::parse_from_conf(text);
        assert_eq!(prefs.motion, MotionPreference::Full);
        assert_eq!(prefs.contrast, ContrastPreference::High);
    }

    #[test]
    fn parse_bool_variants() {
        assert_eq!(
            A11yPrefs::parse_from_conf("reduced_motion=YES").motion,
            MotionPreference::Reduced
        );
        assert_eq!(
            A11yPrefs::parse_from_conf("reduced_motion=on").motion,
            MotionPreference::Reduced
        );
        assert_eq!(
            A11yPrefs::parse_from_conf("high_contrast=0").contrast,
            ContrastPreference::Normal
        );
    }

    #[test]
    fn high_contrast_forces_high_contrast_theme() {
        let prefs = A11yPrefs::new(MotionPreference::Full, ContrastPreference::High);
        assert_eq!(
            apply_a11y_prefs_to_theme_name(prefs, ThemeName::Classic),
            ThemeName::HighContrast
        );
        assert_eq!(
            apply_a11y_prefs_to_theme_name(prefs, ThemeName::Dracula),
            ThemeName::HighContrast
        );
    }

    #[test]
    fn normal_contrast_keeps_current_theme() {
        let prefs = A11yPrefs::new(MotionPreference::Reduced, ContrastPreference::Normal);
        assert_eq!(
            apply_a11y_prefs_to_theme_name(prefs, ThemeName::Solarized),
            ThemeName::Solarized
        );
        assert_eq!(
            apply_a11y_prefs_to_theme_name(prefs, ThemeName::Classic),
            ThemeName::Classic
        );
    }

    #[test]
    fn effective_animation_ms_respects_reduced_motion() {
        let full = A11yPrefs::new(MotionPreference::Full, ContrastPreference::Normal);
        let reduced = A11yPrefs::new(MotionPreference::Reduced, ContrastPreference::Normal);
        assert_eq!(effective_animation_ms(full, 250), 250);
        assert_eq!(effective_animation_ms(full, 0), 0);
        assert_eq!(effective_animation_ms(reduced, 250), 0);
        assert_eq!(effective_animation_ms(reduced, 16), 0);
    }

    #[test]
    fn empty_conf_is_default() {
        assert_eq!(A11yPrefs::parse_from_conf(""), A11yPrefs::default());
        assert_eq!(
            A11yPrefs::parse_from_conf("theme=grape\n"),
            A11yPrefs::default()
        );
    }
}
