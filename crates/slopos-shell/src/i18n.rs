//! Minimal i18n catalog + locale / RTL policy for the shell.
//!
//! Not gettext-complete: a static message table with locale fallbacks so
//! menus, lock screen, and a11y strings can be localized without a full
//! translation pipeline. Pure — no filesystem catalog loading unless asked.

use std::collections::HashMap;

/// Text direction for layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum TextDirection {
    #[default]
    Ltr,
    Rtl,
}

impl TextDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ltr => "ltr",
            Self::Rtl => "rtl",
        }
    }
}

/// Parsed locale (language + optional region).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocaleId {
    pub language: String,
    pub region: Option<String>,
}

impl LocaleId {
    pub fn new(language: impl Into<String>, region: Option<String>) -> Self {
        Self {
            language: language.into().to_ascii_lowercase(),
            region: region.map(|r| r.to_ascii_uppercase()),
        }
    }

    /// BCP47-ish tag: `en`, `en-US`, `ar`, `he-IL`.
    pub fn tag(&self) -> String {
        match &self.region {
            Some(r) => format!("{}-{}", self.language, r),
            None => self.language.clone(),
        }
    }

    /// Parse `en_US.UTF-8`, `en-US`, `fr`, `C`, `POSIX`.
    pub fn parse(s: &str) -> Self {
        let s = s.trim();
        if s.is_empty() || s.eq_ignore_ascii_case("C") || s.eq_ignore_ascii_case("POSIX") {
            return Self::new("en", Some("US".into()));
        }
        let base = s.split('.').next().unwrap_or(s);
        let base = base.replace('_', "-");
        let mut parts = base.split('-');
        let lang = parts.next().unwrap_or("en");
        let region = parts.next().map(|r| r.to_string());
        Self::new(lang, region)
    }
}

/// Languages that default to RTL script direction.
pub fn is_rtl_language(language: &str) -> bool {
    matches!(
        language.to_ascii_lowercase().as_str(),
        "ar" | "he" | "fa" | "ur" | "yi" | "ps" | "ckb"
    )
}

/// Resolve text direction from locale (overrideable).
pub fn text_direction_for_locale(locale: &LocaleId) -> TextDirection {
    if is_rtl_language(&locale.language) {
        TextDirection::Rtl
    } else {
        TextDirection::Ltr
    }
}

/// Message catalog: key → locale-tag → string.
#[derive(Clone, Debug, Default)]
pub struct MessageCatalog {
    /// map key → (locale_tag → message)
    entries: HashMap<String, HashMap<String, String>>,
    fallback_locale: String,
}

impl MessageCatalog {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            fallback_locale: "en".into(),
        }
    }

    pub fn with_builtin() -> Self {
        let mut cat = Self::new();
        cat.seed_builtin();
        cat
    }

    pub fn insert(&mut self, key: &str, locale: &str, message: impl Into<String>) {
        self.entries
            .entry(key.to_string())
            .or_default()
            .insert(locale.to_ascii_lowercase(), message.into());
    }

    /// Lookup with fallback: full tag → language → fallback locale → key.
    pub fn get(&self, key: &str, locale: &LocaleId) -> String {
        if let Some(map) = self.entries.get(key) {
            let tag = locale.tag().to_ascii_lowercase();
            if let Some(m) = map.get(&tag) {
                return m.clone();
            }
            if let Some(m) = map.get(&locale.language) {
                return m.clone();
            }
            if let Some(m) = map.get(&self.fallback_locale) {
                return m.clone();
            }
            if let Some(m) = map.get("en") {
                return m.clone();
            }
        }
        key.to_string()
    }

    fn seed_builtin(&mut self) {
        // English
        let en = [
            ("menu.lock_screen", "Lock Screen"),
            ("menu.log_out", "Log Out…"),
            ("menu.suspend", "Sleep"),
            ("menu.reboot", "Restart…"),
            ("menu.power_off", "Shut Down…"),
            ("menu.quit", "Quit SLOPOS-I"),
            ("menu.force_quit", "Force Quit…"),
            ("menu.notification_center", "Notification Center"),
            ("menu.about", "About SLOPOS-I"),
            ("lock.prompt", "Enter password to unlock"),
            ("lock.error", "Incorrect password"),
            ("confirm.logout", "Log out of SLOPOS-I?"),
            ("confirm.reboot", "Restart the computer now?"),
            ("confirm.poweroff", "Shut down the computer now?"),
            ("a11y.menu_bar", "Menu Bar"),
            ("a11y.dock", "Dock"),
            ("a11y.desktop", "Desktop"),
            ("a11y.windows", "Windows"),
            ("a11y.action.activate", "Activate"),
            ("a11y.action.press", "Press"),
            ("a11y.action.focus", "Focus"),
            ("a11y.action.click", "Click"),
            ("a11y.notification_center", "Notification Center"),
            ("status.network", "Network"),
            ("status.battery", "Battery"),
            ("workspace.desktop_n", "Desktop {n}"),
            ("portal.inhibit.active", "Session idle inhibit active"),
            (
                "portal.screencast.stub",
                "Screen share uses portal stubs (no live PipeWire stream)",
            ),
            (
                "portal.screencast.socket",
                "PipeWire socket present; streams still protocol stubs",
            ),
        ];
        for (k, v) in en {
            self.insert(k, "en", v);
        }
        // Spanish (sample)
        let es = [
            ("menu.lock_screen", "Bloquear pantalla"),
            ("menu.log_out", "Cerrar sesión…"),
            ("menu.suspend", "Suspender"),
            ("menu.reboot", "Reiniciar…"),
            ("menu.power_off", "Apagar…"),
            ("menu.force_quit", "Forzar salida…"),
            ("menu.notification_center", "Centro de notificaciones"),
            ("lock.prompt", "Introduzca la contraseña para desbloquear"),
            ("lock.error", "Contraseña incorrecta"),
            ("confirm.logout", "¿Cerrar sesión de SLOPOS-I?"),
            ("a11y.menu_bar", "Barra de menús"),
            ("a11y.dock", "Dock"),
            ("a11y.desktop", "Escritorio"),
            ("a11y.windows", "Ventanas"),
            ("a11y.action.activate", "Activar"),
            ("a11y.notification_center", "Centro de notificaciones"),
            ("portal.inhibit.active", "Inhibición de inactividad activa"),
        ];
        for (k, v) in es {
            self.insert(k, "es", v);
        }
        // Arabic (sample RTL)
        let ar = [
            ("menu.lock_screen", "قفل الشاشة"),
            ("menu.log_out", "تسجيل الخروج…"),
            ("menu.suspend", "إيقاف مؤقت"),
            ("menu.reboot", "إعادة التشغيل…"),
            ("menu.power_off", "إيقاف التشغيل…"),
            ("menu.force_quit", "إنهاء إجباري…"),
            ("menu.notification_center", "مركز الإشعارات"),
            ("lock.prompt", "أدخل كلمة المرور لفتح القفل"),
            ("lock.error", "كلمة المرور غير صحيحة"),
            ("confirm.logout", "هل تريد تسجيل الخروج من SLOPOS-I؟"),
            ("a11y.menu_bar", "شريط القوائم"),
            ("a11y.dock", "الرصيف"),
            ("a11y.desktop", "سطح المكتب"),
            ("a11y.windows", "النوافذ"),
            ("a11y.action.activate", "تفعيل"),
            ("a11y.notification_center", "مركز الإشعارات"),
            ("portal.inhibit.active", "منع الخمول نشط"),
        ];
        for (k, v) in ar {
            self.insert(k, "ar", v);
        }
        // French
        let fr = [
            ("menu.lock_screen", "Verrouiller l’écran"),
            ("menu.log_out", "Se déconnecter…"),
            ("menu.suspend", "Veille"),
            ("menu.reboot", "Redémarrer…"),
            ("menu.power_off", "Éteindre…"),
            ("menu.force_quit", "Forcer à quitter…"),
            ("menu.notification_center", "Centre de notifications"),
            ("lock.prompt", "Entrez le mot de passe pour déverrouiller"),
            ("confirm.logout", "Se déconnecter de SLOPOS-I ?"),
            ("a11y.menu_bar", "Barre de menus"),
            ("a11y.dock", "Dock"),
            ("a11y.desktop", "Bureau"),
            ("a11y.windows", "Fenêtres"),
            ("a11y.action.activate", "Activer"),
            ("portal.inhibit.active", "Inhibition d’inactivité active"),
        ];
        for (k, v) in fr {
            self.insert(k, "fr", v);
        }
    }
}

/// Session locale preferences (pure).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalePrefs {
    pub locale: LocaleId,
    pub direction: TextDirection,
    /// Explicit override; when set, wins over locale-derived direction.
    pub force_rtl: Option<bool>,
}

impl Default for LocalePrefs {
    fn default() -> Self {
        Self::from_locale(LocaleId::new("en", Some("US".into())))
    }
}

impl LocalePrefs {
    pub fn from_locale(locale: LocaleId) -> Self {
        let direction = text_direction_for_locale(&locale);
        Self {
            locale,
            direction,
            force_rtl: None,
        }
    }

    /// Parse from env-style `LANG` / settings conf snippet.
    pub fn parse_from_env_lang(lang: Option<&str>) -> Self {
        let locale = LocaleId::parse(lang.unwrap_or("en_US.UTF-8"));
        Self::from_locale(locale)
    }

    pub fn effective_direction(&self) -> TextDirection {
        match self.force_rtl {
            Some(true) => TextDirection::Rtl,
            Some(false) => TextDirection::Ltr,
            None => self.direction,
        }
    }

    /// Parse optional keys from settings.conf: `locale=…`, `force_rtl=true|false`.
    pub fn parse_from_conf(text: &str) -> Self {
        let mut lang: Option<String> = None;
        let mut force_rtl: Option<bool> = None;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            match k.trim() {
                "locale" | "lang" | "language" => lang = Some(v.trim().to_string()),
                "force_rtl" | "rtl" => {
                    force_rtl = Some(matches!(
                        v.trim().to_ascii_lowercase().as_str(),
                        "1" | "true" | "yes" | "on"
                    ));
                }
                _ => {}
            }
        }
        let mut prefs = Self::parse_from_env_lang(lang.as_deref());
        prefs.force_rtl = force_rtl;
        if let Some(f) = force_rtl {
            prefs.direction = if f {
                TextDirection::Rtl
            } else {
                TextDirection::Ltr
            };
        }
        prefs
    }
}

/// Format a simple template: replace `{n}` and `{name}` placeholders.
pub fn format_message(template: &str, n: Option<u32>, name: Option<&str>) -> String {
    let mut s = template.to_string();
    if let Some(n) = n {
        s = s.replace("{n}", &n.to_string());
    }
    if let Some(name) = name {
        s = s.replace("{name}", name);
    }
    s
}

/// Convenience: translate key with builtin catalog.
pub fn tr(key: &str, locale: &LocaleId) -> String {
    MessageCatalog::with_builtin().get(key, locale)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_locale_and_rtl() {
        let l = LocaleId::parse("ar_EG.UTF-8");
        assert_eq!(l.language, "ar");
        assert_eq!(l.region.as_deref(), Some("EG"));
        assert_eq!(text_direction_for_locale(&l), TextDirection::Rtl);
        assert_eq!(
            text_direction_for_locale(&LocaleId::parse("en_US")),
            TextDirection::Ltr
        );
    }

    #[test]
    fn catalog_fallback() {
        let cat = MessageCatalog::with_builtin();
        let en = LocaleId::new("en", None);
        let es = LocaleId::new("es", Some("ES".into()));
        let de = LocaleId::new("de", None);
        assert_eq!(cat.get("menu.lock_screen", &en), "Lock Screen");
        assert_eq!(cat.get("menu.lock_screen", &es), "Bloquear pantalla");
        // German falls back to English
        assert_eq!(cat.get("menu.lock_screen", &de), "Lock Screen");
        assert_eq!(cat.get("missing.key", &en), "missing.key");
    }

    #[test]
    fn conf_parse_force_rtl() {
        let p = LocalePrefs::parse_from_conf("locale=he_IL\nforce_rtl=true\n");
        assert_eq!(p.locale.language, "he");
        assert_eq!(p.effective_direction(), TextDirection::Rtl);
    }

    #[test]
    fn a11y_and_portal_catalog_keys() {
        let cat = MessageCatalog::with_builtin();
        let en = LocaleId::new("en", None);
        assert_eq!(cat.get("a11y.action.activate", &en), "Activate");
        assert_eq!(cat.get("menu.force_quit", &en), "Force Quit…");
        assert!(cat
            .get("portal.screencast.stub", &en)
            .contains("portal stubs"));
        assert!(!cat
            .get("portal.screencast.socket", &en)
            .contains("live stream ready"));
        let es = LocaleId::new("es", None);
        assert_eq!(cat.get("a11y.action.activate", &es), "Activar");
    }

    #[test]
    fn format_workspace() {
        assert_eq!(format_message("Desktop {n}", Some(3), None), "Desktop 3");
    }
}
