//! Session environment: install locations, themes and Openbox configuration.
//!
//! The session never writes to another desktop's configuration. Openbox gets
//! its SLOPOS configuration through `--config-file`, Openbox finds the SLOPOS
//! menu through `XDG_CONFIG_DIRS`, and GTK 2 gets its settings through
//! `GTK2_RC_FILES`. User overrides live under `$XDG_CONFIG_HOME/slopos-i/`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Visual styles the session knows how to apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Appearance {
    Platinum,
    Classic,
    Graphite,
    Oled,
}

impl Appearance {
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "classic" => Self::Classic,
            "graphite" => Self::Graphite,
            "oled" => Self::Oled,
            _ => Self::Platinum,
        }
    }

    fn openbox_config_name(self) -> &'static str {
        match self {
            Self::Platinum => "rc.xml",
            Self::Classic => "rc-classic.xml",
            Self::Graphite => "rc-graphite.xml",
            Self::Oled => "rc-oled.xml",
        }
    }

    fn gtk_theme(self) -> &'static str {
        match self {
            Self::Platinum => "slopos-gtk",
            Self::Classic => "slopos-gtk-classic",
            Self::Graphite => "slopos-gtk-graphite",
            Self::Oled => "slopos-gtk-oled",
        }
    }

    pub fn root_color(self) -> &'static str {
        match self {
            Self::Platinum => "#2B7798",
            Self::Classic => "#808080",
            Self::Graphite => "#25272B",
            Self::Oled => "#000000",
        }
    }
}

pub fn config_home() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
}

fn user_dir() -> Option<PathBuf> {
    config_home().map(|dir| dir.join("slopos-i"))
}

pub fn appearance() -> Appearance {
    user_dir()
        .and_then(|dir| fs::read_to_string(dir.join("appearance")).ok())
        .map(|value| Appearance::parse(&value))
        .unwrap_or(Appearance::Platinum)
}

/// The `share` directory that holds `slopos-i/`, `themes/` and `icons/`.
fn share_dir() -> Option<PathBuf> {
    env::var_os("SLOPOS_SHARE_DIR")
        .map(PathBuf::from)
        .filter(|dir| dir.join("slopos-i").is_dir())
}

pub fn configure() {
    set("XDG_SESSION_TYPE", "x11");
    set("XDG_CURRENT_DESKTOP", "SLOPOS");
    set("XDG_SESSION_DESKTOP", "slopos-i");
    set("DESKTOP_SESSION", "slopos-i");
    set("SLOPOS_SESSION_MANAGED", "1");

    let exe = env::current_exe().ok();
    let bin_dir = exe.as_deref().and_then(Path::parent).map(Path::to_path_buf);
    if let Some(bin) = &bin_dir {
        prepend("PATH", bin, "/usr/local/bin:/usr/bin:/bin");
        // A source session runs from `target/{debug,release}`; its bundled
        // helpers (`slopos-appearance`, `slopos-wallpaper`) live in the
        // checkout's `scripts/` directory, not next to the binaries.
        if let Some(repo) = source_tree_root(bin) {
            let scripts = repo.join("scripts");
            if scripts.is_dir() {
                prepend("PATH", &scripts, "/usr/local/bin:/usr/bin:/bin");
            }
        }
    }

    let share = share_dir()
        .or_else(|| installed_share_dir(bin_dir.as_deref()))
        .or_else(|| bin_dir.as_deref().and_then(stage_source_tree_share));
    if let Some(share) = &share {
        set("SLOPOS_SHARE_DIR", share);
        prepend("XDG_DATA_DIRS", share, "/usr/local/share:/usr/share");
        prepend("XDG_CONFIG_DIRS", &share.join("slopos-i"), "/etc/xdg");
    } else {
        log::warn!("SLOPOS data files were not found; themes and menus will use defaults");
    }

    let appearance = appearance();
    if env::var_os("GTK_THEME").is_none() && theme_installed(appearance.gtk_theme()) {
        set("GTK_THEME", appearance.gtk_theme());
    }
    if let Some(rc) = write_gtk2_rc(appearance) {
        set("GTK2_RC_FILES", &rc);
    }

    // Activated D-Bus services (portals, file managers) must see the same
    // environment as directly started children.
    let _ = Command::new("dbus-update-activation-environment")
        .args([
            "--systemd",
            "DISPLAY",
            "XDG_SESSION_TYPE",
            "XDG_CURRENT_DESKTOP",
            "XDG_SESSION_DESKTOP",
            "DESKTOP_SESSION",
            "XDG_DATA_DIRS",
            "XDG_CONFIG_DIRS",
            "GTK_THEME",
            "GTK2_RC_FILES",
        ])
        .status();
}

/// `<prefix>/bin/slopos-session` → `<prefix>/share` when it is a real install.
fn installed_share_dir(bin_dir: Option<&Path>) -> Option<PathBuf> {
    let share = bin_dir?.parent()?.join("share");
    share.join("slopos-i").is_dir().then_some(share)
}

/// Running from `target/{debug,release}` in a checkout: the repository root.
fn source_tree_root(bin_dir: &Path) -> Option<PathBuf> {
    bin_dir
        .ancestors()
        .find(|dir| dir.join("assets/config/openbox/rc.xml").is_file())
        .map(Path::to_path_buf)
}

/// Running from `target/{debug,release}` in a checkout: expose the checkout's
/// themes, icons and Openbox files through a per-session symlink tree in
/// `$XDG_RUNTIME_DIR`, instead of copying them into the user's home.
fn stage_source_tree_share(bin_dir: &Path) -> Option<PathBuf> {
    let repo = source_tree_root(bin_dir)?;
    let runtime = env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())?;
    let stage = runtime.join("slopos-source-share");
    let links: Vec<(PathBuf, PathBuf)> = vec![
        (
            repo.join("assets/config/openbox"),
            "slopos-i/openbox".into(),
        ),
        (
            repo.join("assets/slopos-logo.png"),
            "slopos-i/slopos-logo.png".into(),
        ),
        (repo.join("assets/wallpapers"), "slopos-i/wallpapers".into()),
        (
            repo.join("themes/platinum/icon-theme"),
            "icons/SLOPOS-Platinum".into(),
        ),
        (
            repo.join("themes/slopos-openbox"),
            "themes/slopos-openbox".into(),
        ),
        (
            repo.join("themes/slopos-openbox-classic"),
            "themes/slopos-openbox-classic".into(),
        ),
        (
            repo.join("themes/slopos-openbox-graphite"),
            "themes/slopos-openbox-graphite".into(),
        ),
        (
            repo.join("themes/slopos-openbox-oled"),
            "themes/slopos-openbox-oled".into(),
        ),
        (
            repo.join("assets/config/gtk-3.0/gtk.css"),
            "themes/slopos-gtk/gtk-3.0/gtk.css".into(),
        ),
        (
            repo.join("assets/config/gtk-3.0/gtk-classic.css"),
            "themes/slopos-gtk-classic/gtk-3.0/gtk.css".into(),
        ),
        (
            repo.join("assets/config/gtk-3.0/gtk-graphite.css"),
            "themes/slopos-gtk-graphite/gtk-3.0/gtk.css".into(),
        ),
        (
            repo.join("assets/config/gtk-3.0/gtk-oled.css"),
            "themes/slopos-gtk-oled/gtk-3.0/gtk.css".into(),
        ),
    ];
    for (source, relative) in links {
        if !source.exists() {
            continue;
        }
        let target = stage.join(relative);
        if let Some(parent) = target.parent() {
            if fs::create_dir_all(parent).is_err() {
                return None;
            }
        }
        let _ = fs::remove_file(&target);
        if let Err(error) = std::os::unix::fs::symlink(&source, &target) {
            log::warn!("Could not link {}: {error}", target.display());
        }
    }
    log::info!(
        "Using SLOPOS data from source tree {} via {}",
        repo.display(),
        stage.display()
    );
    Some(stage)
}

fn theme_installed(name: &str) -> bool {
    data_dirs().any(|dir| dir.join("themes").join(name).join("gtk-3.0").is_dir())
}

fn data_dirs() -> impl Iterator<Item = PathBuf> {
    let dirs = env::var_os("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    env::split_paths(&dirs).collect::<Vec<_>>().into_iter()
}

/// GTK 2 applications read this through `GTK2_RC_FILES`; `~/.gtkrc-2.0` is
/// left alone.
fn write_gtk2_rc(appearance: Appearance) -> Option<PathBuf> {
    let dir = user_dir()?;
    fs::create_dir_all(&dir).ok()?;
    let path = dir.join("gtkrc-2.0");
    let content = format!(
        "# Generated by slopos-session for the {:?} appearance. Edit\n\
         # ~/.config/slopos-i/gtkrc-2.0.local for personal overrides.\n\
         gtk-theme-name = \"{}\"\n\
         gtk-icon-theme-name = \"SLOPOS-Platinum\"\n\
         gtk-font-name = \"Liberation Sans 9\"\n\
         include \"{}\"\n",
        appearance,
        appearance.gtk_theme(),
        dir.join("gtkrc-2.0.local").display()
    );
    let local = dir.join("gtkrc-2.0.local");
    if !local.exists() {
        let _ = fs::write(&local, "# Personal GTK 2 settings for SLOPOS-I.\n");
    }
    fs::write(&path, content).ok()?;
    Some(path)
}

/// Openbox configuration, in priority order: `SLOPOS_OPENBOX_CONFIG`, the
/// user's own `~/.config/slopos-i/openbox/rc.xml`, then the installed file for
/// the selected appearance.
pub fn openbox_config() -> Option<PathBuf> {
    if let Some(path) = env::var_os("SLOPOS_OPENBOX_CONFIG").map(PathBuf::from) {
        if path.is_file() {
            return Some(path);
        }
    }
    if let Some(user) = user_dir().map(|dir| dir.join("openbox/rc.xml")) {
        if user.is_file() {
            return Some(user);
        }
    }
    let name = appearance().openbox_config_name();
    share_dir()
        .map(|share| share.join("slopos-i/openbox").join(name))
        .filter(|path| path.is_file())
        .or_else(|| {
            // An appearance without its own file still gets SLOPOS keybindings.
            share_dir()
                .map(|share| share.join("slopos-i/openbox/rc.xml"))
                .filter(|path| path.is_file())
        })
}

/// Paint the root window in the appearance color, and the saved wallpaper on
/// top when `feh` is available. PCManFM's desktop replaces both when it runs.
pub fn apply_root_background() {
    let color = appearance().root_color();
    if let Some(xsetroot) = slopos_core::process::resolve_program("xsetroot") {
        let _ = Command::new(xsetroot).args(["-solid", color]).status();
    }
    let wallpaper = user_dir()
        .and_then(|dir| fs::read_to_string(dir.join("wallpaper")).ok())
        .map(|line| PathBuf::from(line.trim()))
        .filter(|path| path.is_file());
    if let (Some(path), Some(feh)) = (wallpaper, slopos_core::process::resolve_program("feh")) {
        let _ = Command::new(feh).arg("--bg-fill").arg(path).status();
    }
}

fn set(name: &str, value: impl AsRef<std::ffi::OsStr>) {
    // SAFETY (edition 2021): called before any other thread is started.
    env::set_var(name, value);
}

fn prepend(name: &str, directory: &Path, default: &str) {
    let current = env::var(name).unwrap_or_else(|_| default.to_string());
    let directory = directory.to_string_lossy();
    if current.split(':').any(|entry| entry == directory) {
        return;
    }
    let value = if current.is_empty() {
        directory.into_owned()
    } else {
        format!("{directory}:{current}")
    };
    set(name, value);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appearance_names_are_case_insensitive_with_a_platinum_default() {
        assert_eq!(Appearance::parse(" Graphite\n"), Appearance::Graphite);
        assert_eq!(Appearance::parse("OLED"), Appearance::Oled);
        assert_eq!(Appearance::parse("classic"), Appearance::Classic);
        assert_eq!(Appearance::parse("unknown"), Appearance::Platinum);
        assert_eq!(Appearance::parse(""), Appearance::Platinum);
    }

    #[test]
    fn a_source_checkout_exposes_its_scripts_directory() {
        let manifest = fs::canonicalize(env!("CARGO_MANIFEST_DIR")).unwrap();
        let repo = manifest.parent().unwrap().parent().unwrap();
        let binary = repo.join("target/debug/slopos-session");
        assert_eq!(source_tree_root(&binary).as_deref(), Some(repo));
        assert!(repo.join("scripts/slopos-appearance").is_file());
    }

    #[test]
    fn every_appearance_names_an_openbox_file_that_ships() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/config/openbox");
        for appearance in [
            Appearance::Platinum,
            Appearance::Classic,
            Appearance::Graphite,
            Appearance::Oled,
        ] {
            let file = root.join(appearance.openbox_config_name());
            assert!(file.is_file(), "{} is missing", file.display());
        }
    }
}
