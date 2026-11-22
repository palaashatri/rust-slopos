//! Which optional software sources the user has enabled.
//!
//! The file is `$XDG_CONFIG_HOME/slopos-i/software.conf` (default
//! `~/.config/slopos-i/software.conf`), one `key=value` per line. Unknown keys
//! and malformed values are ignored, so a damaged file falls back to the
//! defaults for the affected setting only. Every source is off by default.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::xdg::XdgPaths;

/// Path relative to the XDG config home.
pub const RELATIVE_PATH: &str = "slopos-i/software.conf";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SoftwareConfig {
    pub flatpak: bool,
    pub snap: bool,
    /// Only offered on Arch Linux; the flag is kept regardless of the distro.
    pub aur: bool,
}

impl SoftwareConfig {
    /// Parse the file text. Later lines win; keys are case-sensitive.
    pub fn parse(text: &str) -> Self {
        let mut config = Self::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let Some(value) = parse_bool(value.trim()) else {
                continue;
            };
            match key.trim() {
                "flatpak" => config.flatpak = value,
                "snap" => config.snap = value,
                "aur" => config.aur = value,
                _ => {}
            }
        }
        config
    }

    pub fn render(&self) -> String {
        format!(
            "# SLOPOS Software: optional package sources.\n\
             flatpak={}\nsnap={}\naur={}\n",
            self.flatpak, self.snap, self.aur
        )
    }

    /// Read the config. A missing file means the defaults; other read errors
    /// are returned so the caller can say so.
    pub fn load(path: &Path) -> io::Result<Self> {
        match fs::read_to_string(path) {
            Ok(text) => Ok(Self::parse(&text)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error),
        }
    }

    /// Write the config atomically: a temporary file in the same directory,
    /// then a rename over the old file.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        let mut temp = path.as_os_str().to_owned();
        temp.push(".tmp");
        let temp = PathBuf::from(temp);
        {
            let mut file = fs::File::create(&temp)?;
            file.write_all(self.render().as_bytes())?;
            file.sync_all()?;
        }
        fs::rename(&temp, path)
    }
}

/// The default location: `$XDG_CONFIG_HOME/slopos-i/software.conf`.
pub fn default_path() -> io::Result<PathBuf> {
    Ok(XdgPaths::from_environment()?.config.join(RELATIVE_PATH))
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_all_off() {
        assert_eq!(SoftwareConfig::parse(""), SoftwareConfig::default());
        let default = SoftwareConfig::default();
        assert_eq!(SoftwareConfig::parse(&default.render()), default);
    }

    #[test]
    fn parses_flags_and_ignores_junk() {
        let config = SoftwareConfig::parse(
            "# comment\n\
             flatpak = yes\n\
             snap=maybe\n\
             AUR=true\n\
             aur=On\n\
             unknown=true\n\
             no equals sign\n",
        );
        assert!(config.flatpak);
        assert!(!config.snap, "a malformed value keeps the default");
        assert!(
            config.aur,
            "a later valid line wins; keys are case-sensitive"
        );
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = std::env::temp_dir().join(format!("slopos-config-test-{}", std::process::id()));
        let path = dir.join("nested/software.conf");
        assert_eq!(
            SoftwareConfig::load(&path).unwrap(),
            SoftwareConfig::default()
        );
        let config = SoftwareConfig {
            flatpak: true,
            snap: false,
            aur: true,
        };
        config.save(&path).unwrap();
        assert_eq!(SoftwareConfig::load(&path).unwrap(), config);
        assert!(!dir.join("nested/software.conf.tmp").exists());
        fs::remove_dir_all(&dir).unwrap();
    }
}
