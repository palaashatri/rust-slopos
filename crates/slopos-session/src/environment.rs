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

/// GTK 2 applications read this through `GTK2_RC_FILES`. The generated file
/// includes the user's `~/.gtkrc-2.0` (or the copy that `slopos-appearance`
/// keeps before replacing it) so settings that predate SLOPOS keep working,
/// then `gtkrc-2.0.local` so per-SLOPOS overrides still win.
fn write_gtk2_rc(appearance: Appearance) -> Option<PathBuf> {
    let dir = user_dir()?;
    fs::create_dir_all(&dir).ok()?;
    let path = dir.join("gtkrc-2.0");
    let local = dir.join("gtkrc-2.0.local");
    if !local.exists() {
        let _ = fs::write(&local, "# Personal GTK 2 settings for SLOPOS-I.\n");
    }
    let legacy = legacy_gtk2_rc_or_copy(legacy_gtk2_rc(), &dir);
    let content = gtk2_rc_content(appearance, &local, legacy.as_deref());
    fs::write(&path, content).ok()?;
    Some(path)
}

fn legacy_gtk2_rc() -> Option<PathBuf> {
    let path = env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".gtkrc-2.0"))
        .filter(|path| path.is_file())?;
    (!is_slopos_gtk2_rc(&path)).then_some(path)
}

/// `slopos-appearance` replaces `~/.gtkrc-2.0` outside a managed session, so
/// it first keeps the original here. A live user file still wins over the copy.
fn legacy_gtk2_rc_or_copy(legacy: Option<PathBuf>, dir: &Path) -> Option<PathBuf> {
    legacy.or_else(|| {
        let copy = dir.join("gtkrc-2.0.legacy");
        copy.is_file().then_some(copy)
    })
}

/// Whether this is a file SLOPOS generated itself. Including such a file from
/// the managed configuration would recurse, so it is left out.
fn is_slopos_gtk2_rc(path: &Path) -> bool {
    let Ok(content) = fs::read_to_string(path) else {
        return false;
    };
    let first = content.lines().next().unwrap_or_default();
    first == "# SLOPOS-I GTK2 Runtime Configuration"
        || first.starts_with("# Generated by slopos-session")
}

fn gtk2_rc_content(appearance: Appearance, local: &Path, legacy: Option<&Path>) -> String {
    let mut content = format!(
        "# Generated by slopos-session for the {:?} appearance. Edit\n\
         # ~/.config/slopos-i/gtkrc-2.0.local for personal overrides.\n\
         gtk-theme-name = \"{}\"\n\
         gtk-icon-theme-name = \"SLOPOS-Platinum\"\n\
         gtk-font-name = \"Liberation Sans 9\"\n",
        appearance,
        appearance.gtk_theme(),
    );
    if let Some(legacy) = legacy {
        content.push_str(&format!("include \"{}\"\n", legacy.display()));
    }
    content.push_str(&format!("include \"{}\"\n", local.display()));
    content
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

/// Paint the saved wallpaper (or, for Platinum, the bundled default) on the
/// root window with the SLOPOS toolkit; with no picture, paint the
/// appearance color. Nothing external (feh, xsetroot, PCManFM) is involved.
pub fn apply_root_background() {
    let backdrop = parse_color(appearance().root_color());
    let config = user_dir();
    // A chosen picture always shows; before any choice, only Platinum gets
    // the bundled blue field and the other appearances show their color.
    let wallpaper = match appearance() {
        Appearance::Platinum => slopos_core::wallpaper::current(
            config.as_deref(),
            &slopos_core::wallpaper::bundled_dirs(),
        ),
        _ => config.as_deref().and_then(slopos_core::wallpaper::load),
    };
    let result = slopos_ui::platform::x11::set_root_background(|width, height| {
        let picture =
            wallpaper.as_ref().and_then(|wallpaper| {
                match slopos_ui::image::Image::load(&wallpaper.path, 512) {
                    Ok(image) => Some((image, wallpaper.mode.as_str())),
                    Err(error) => {
                        log::warn!("wallpaper unavailable: {error}");
                        None
                    }
                }
            });
        let (image, mode) = match picture {
            Some(picture) => picture,
            None => (
                slopos_ui::image::Image::from_rgba(
                    1,
                    1,
                    vec![backdrop.red, backdrop.green, backdrop.blue, 255],
                )
                .ok()?,
                "tile",
            ),
        };
        image
            .compose_background(
                width,
                height,
                slopos_ui::image::BackgroundMode::parse(mode),
                backdrop,
            )
            .ok()
    });
    if let Err(error) = result {
        log::warn!("could not paint the desktop background: {error}");
    }
}

/// "#RRGGBB" to a color; anything else is black.
fn parse_color(text: &str) -> slopos_ui::foundation::color::Color {
    let channel = |range: std::ops::Range<usize>| {
        text.strip_prefix('#')
            .filter(|hex| hex.len() == 6)
            .and_then(|hex| u8::from_str_radix(hex.get(range)?, 16).ok())
            .unwrap_or(0)
    };
    slopos_ui::foundation::color::Color {
        red: channel(0..2),
        green: channel(2..4),
        blue: channel(4..6),
        alpha: 255,
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
    fn gtk2_rc_keeps_legacy_settings_below_slopos_overrides() {
        let local = Path::new("/home/u/.config/slopos-i/gtkrc-2.0.local");
        let legacy = Path::new("/home/u/.gtkrc-2.0");
        let content = gtk2_rc_content(Appearance::Classic, local, Some(legacy));
        let legacy_at = content.find("include \"/home/u/.gtkrc-2.0\"").unwrap();
        let local_at = content
            .find("include \"/home/u/.config/slopos-i/gtkrc-2.0.local\"")
            .unwrap();
        assert!(legacy_at < local_at);
        assert!(content.contains("gtk-theme-name = \"slopos-gtk-classic\""));
    }

    #[test]
    fn slopos_generated_gtk2_files_are_not_included_again() {
        let dir = std::env::temp_dir().join(format!(
            "slopos-gtk2-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let generated = dir.join("gtkrc-2.0");
        fs::write(
            &generated,
            "# Generated by slopos-session for the Classic appearance.\n",
        )
        .unwrap();
        assert!(is_slopos_gtk2_rc(&generated));
        fs::write(&generated, "# SLOPOS-I GTK2 Runtime Configuration\n").unwrap();
        assert!(is_slopos_gtk2_rc(&generated));
        fs::write(&generated, "gtk-theme-name = \"Adwaita\"\n").unwrap();
        assert!(!is_slopos_gtk2_rc(&generated));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_replaced_legacy_gtk2_file_is_included_from_its_saved_copy() {
        let dir = std::env::temp_dir().join(format!(
            "slopos-gtk2-copy-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let copy = dir.join("gtkrc-2.0.legacy");
        fs::write(&copy, "gtk-font-name = \"Legacy Sans 11\"\n").unwrap();
        // ~/.gtkrc-2.0 was replaced, so the session falls back to the copy.
        assert_eq!(legacy_gtk2_rc_or_copy(None, &dir), Some(copy.clone()));

        // A live user file still wins over the saved copy.
        let live = dir.join("gtkrc-2.0");
        fs::write(&live, "gtk-font-name = \"Live Sans 11\"\n").unwrap();
        assert_eq!(legacy_gtk2_rc_or_copy(Some(live.clone()), &dir), Some(live));
        let _ = fs::remove_dir_all(&dir);
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
