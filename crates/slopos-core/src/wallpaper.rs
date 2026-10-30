//! The desktop wallpaper setting.
//!
//! The choice lives in `$XDG_CONFIG_HOME/slopos-i/wallpaper` (one absolute
//! path) and `wallpaper_mode` (`fill`, `tile` or `center`), the files the
//! older `slopos-wallpaper` helper also used. Bundled pictures are found in
//! `slopos-i/wallpapers` under each `$XDG_DATA_DIRS` entry; the session puts
//! the SLOPOS share directory first there.

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The bundled wallpapers, as (file name, display name, mode). They are
/// fine repeating patterns, so they tile at 1:1 instead of being scaled.
pub const BUNDLED: [(&str, &str, &str); 5] = [
    ("03_slate_blue.png", "Slate Blue", "tile"),
    ("02_platinum_cool_slate.png", "Platinum Slate", "tile"),
    ("01_classic_system_gray.png", "Classic Gray", "tile"),
    ("04_retro_teal_grid.png", "Teal Grid", "tile"),
    ("05_oled_pure_dark.png", "OLED Dark", "tile"),
];

/// The picture used before the user chooses one.
pub const DEFAULT: &str = "03_slate_blue.png";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wallpaper {
    pub path: PathBuf,
    /// `fill`, `tile` or `center`.
    pub mode: String,
}

/// `$XDG_CONFIG_HOME/slopos-i`, or `~/.config/slopos-i`.
pub fn config_dir() -> Option<PathBuf> {
    crate::xdg::XdgPaths::from_environment()
        .ok()
        .map(|paths| paths.config.join("slopos-i"))
}

/// The saved choice in `config`, if its picture still exists.
pub fn load(config: &Path) -> Option<Wallpaper> {
    let path = PathBuf::from(fs::read_to_string(config.join("wallpaper")).ok()?.trim());
    if !path.is_absolute() || !path.is_file() {
        return None;
    }
    let mode = fs::read_to_string(config.join("wallpaper_mode"))
        .map(|mode| mode.trim().to_owned())
        .ok()
        .filter(|mode| matches!(mode.as_str(), "fill" | "tile" | "center"))
        .unwrap_or_else(|| "fill".to_owned());
    Some(Wallpaper { path, mode })
}

/// Save `wallpaper` into `config`, replacing each file atomically.
pub fn save(config: &Path, wallpaper: &Wallpaper) -> io::Result<()> {
    if !wallpaper.path.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the wallpaper path must be absolute",
        ));
    }
    fs::create_dir_all(config)?;
    for (name, value) in [
        ("wallpaper", wallpaper.path.to_string_lossy().into_owned()),
        ("wallpaper_mode", wallpaper.mode.clone()),
    ] {
        let temporary = config.join(format!(".{name}.tmp"));
        fs::write(&temporary, format!("{value}\n"))?;
        fs::rename(&temporary, config.join(name))?;
    }
    Ok(())
}

/// The `slopos-i/wallpapers` directories under `$XDG_DATA_DIRS`.
pub fn bundled_dirs() -> Vec<PathBuf> {
    let dirs = env::var_os("XDG_DATA_DIRS")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    env::split_paths(&dirs)
        .map(|dir| dir.join("slopos-i/wallpapers"))
        .filter(|dir| dir.is_dir())
        .collect()
}

/// Find bundled picture `name` in `dirs`.
pub fn find_bundled(dirs: &[PathBuf], name: &str) -> Option<PathBuf> {
    dirs.iter()
        .map(|dir| dir.join(name))
        .find(|path| path.is_file())
}

/// The saved wallpaper, or the default bundled one.
pub fn current(config: Option<&Path>, dirs: &[PathBuf]) -> Option<Wallpaper> {
    config.and_then(load).or_else(|| {
        let mode = BUNDLED
            .iter()
            .find(|(name, _, _)| *name == DEFAULT)
            .map_or("tile", |entry| entry.2);
        find_bundled(dirs, DEFAULT).map(|path| Wallpaper {
            path,
            mode: mode.to_owned(),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("slopos-wallpaper-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn saved_choice_round_trips_and_falls_back_to_the_default() {
        let root = scratch("roundtrip");
        let config = root.join("config");
        let share = root.join("share");
        fs::create_dir_all(&share).unwrap();
        fs::write(share.join(DEFAULT), b"png").unwrap();
        let dirs = vec![share.clone()];
        assert_eq!(load(&config), None);
        assert_eq!(
            current(Some(&config), &dirs),
            Some(Wallpaper {
                path: share.join(DEFAULT),
                mode: "tile".into()
            })
        );
        let picture = root.join("photo.png");
        fs::write(&picture, b"png").unwrap();
        let choice = Wallpaper {
            path: picture.clone(),
            mode: "fill".into(),
        };
        save(&config, &choice).unwrap();
        assert_eq!(load(&config), Some(choice.clone()));
        assert_eq!(current(Some(&config), &dirs), Some(choice));
        // A deleted picture or a bad mode never reaches the desktop.
        fs::write(config.join("wallpaper_mode"), "spin\n").unwrap();
        assert_eq!(load(&config).unwrap().mode, "fill");
        fs::remove_file(&picture).unwrap();
        assert_eq!(load(&config), None);
        assert!(save(
            &config,
            &Wallpaper {
                path: "relative.png".into(),
                mode: "fill".into()
            }
        )
        .is_err());
        assert_eq!(find_bundled(&dirs, "missing.png"), None);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn bundled_list_matches_the_shipped_assets() {
        let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/wallpapers");
        for (name, _, mode) in BUNDLED {
            assert!(assets.join(name).is_file(), "{name} is not shipped");
            assert_eq!(mode, "tile");
        }
        assert!(BUNDLED.iter().any(|entry| entry.0 == DEFAULT));
    }
}
