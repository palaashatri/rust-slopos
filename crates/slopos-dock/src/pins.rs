//! Pinned applications: `$XDG_CONFIG_HOME/slopos-i/dock-pins`, one
//! desktop-file id per line. Without that file the Dock pins one installed
//! application per role, so it never shows apps that are not installed.

use std::fs;
use std::io;
use std::path::PathBuf;

use slopos_core::desktop_entry::DesktopApp;
use slopos_core::xdg::XdgPaths;

/// Candidate desktop ids per default role, in preference order.
const DEFAULT_ROLES: &[&[&str]] = &[
    // Bundled SLOPOS roles name only the native slopos-ui application.
    &["slopos-files"],
    &["slopos-browser", "firefox", "firefox-esr", "chromium"],
    &[
        "xfce4-terminal",
        "org.gnome.Terminal",
        "org.gnome.Console",
        "lxterminal",
        "org.kde.konsole",
        "xterm",
        "debian-xterm",
    ],
    &[
        "mousepad",
        "org.gnome.TextEditor",
        "gedit",
        "pluma",
        "org.kde.kate",
    ],
    &["slopos-calculator"],
    &["slopos-media"],
    &["slopos-catalogue"],
    &["slopos-control-panels"],
];

pub fn path() -> Option<PathBuf> {
    XdgPaths::from_environment()
        .ok()
        .map(|paths| paths.config.join("slopos-i/dock-pins"))
}

/// Parse a pins file: ids, ignoring blank lines, `#` comments and duplicates.
pub fn parse(contents: &str) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for line in contents.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let id = line.strip_suffix(".desktop").unwrap_or(line).to_owned();
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

/// The pinned applications, resolved against `catalog`. Ids that are no
/// longer installed are skipped, not shown as broken tiles.
pub fn load(catalog: &[DesktopApp]) -> Vec<DesktopApp> {
    let saved = path().and_then(|path| fs::read_to_string(path).ok());
    let ids = match saved {
        Some(contents) => parse(&contents),
        None => defaults(catalog),
    };
    resolve(&ids, catalog)
}

pub fn defaults(catalog: &[DesktopApp]) -> Vec<String> {
    DEFAULT_ROLES
        .iter()
        .filter_map(|role| {
            role.iter()
                .find(|id| catalog.iter().any(|app| app.id == **id))
                .map(|id| (*id).to_owned())
        })
        .collect()
}

pub fn resolve(ids: &[String], catalog: &[DesktopApp]) -> Vec<DesktopApp> {
    ids.iter()
        .filter_map(|id| catalog.iter().find(|app| &app.id == id).cloned())
        .collect()
}

pub fn save(pins: &[DesktopApp]) -> io::Result<()> {
    let path = path().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no config dir"))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut contents = String::from("# Dock pins, one desktop-file id per line.\n");
    for app in pins {
        contents.push_str(&app.id);
        contents.push('\n');
    }
    // Write then rename, so a crash never leaves a half-written file.
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, contents)?;
    fs::rename(temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str) -> DesktopApp {
        DesktopApp {
            id: id.into(),
            name: id.into(),
            argv: vec![id.into()],
            icon: String::new(),
            comment: String::new(),
            terminal: false,
            startup_wm_class: String::new(),
            mime_types: Vec::new(),
            exec: String::new(),
        }
    }

    #[test]
    fn pins_file_ignores_comments_duplicates_and_suffixes() {
        let ids = parse("# mine\nfirefox.desktop\n\n  xterm \nfirefox\n");
        assert_eq!(ids, ["firefox", "xterm"]);
    }

    #[test]
    fn defaults_pick_one_installed_app_per_role() {
        let catalog = [
            app("thunar"),
            app("nemo"),
            app("xterm"),
            app("slopos-calculator"),
        ];
        // Donor file managers never stand in for the native Files.
        assert_eq!(defaults(&catalog), ["xterm", "slopos-calculator"]);
        let catalog = [
            app("thunar"),
            app("slopos-files"),
            app("xterm"),
            app("slopos-media"),
            app("slopos-calculator"),
        ];
        assert_eq!(
            defaults(&catalog),
            ["slopos-files", "xterm", "slopos-calculator", "slopos-media"]
        );
    }

    #[test]
    fn uninstalled_pins_are_skipped() {
        let catalog = [app("xterm")];
        let pins = resolve(&["gone".into(), "xterm".into()], &catalog);
        assert_eq!(pins.len(), 1);
        assert_eq!(pins[0].id, "xterm");
    }
}
