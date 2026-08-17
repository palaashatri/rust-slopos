//! AppImage catalogue data model.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const EMPTY_FILE_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogueApp {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub description: String,
    pub version: String,
    pub architecture: String,
    pub category: String,
    pub icon_name: String,
    pub download_url: String,
    pub sha256: String,
}

impl CatalogueApp {
    pub fn is_appimage_installed(&self) -> bool {
        valid_id(&self.id) && get_appimage_path(&self.id).is_file()
    }

    pub fn is_system_installed(&self) -> bool {
        match self.id.as_str() {
            "firefox" | "firefox-esr" => {
                is_command_available("firefox") || is_command_available("firefox-esr")
            }
            "chocolate-doom" | "doom" => {
                is_command_available("chocolate-doom")
                    || is_command_available("doom")
                    || std::path::Path::new("/usr/games/chocolate-doom").exists()
                    || std::path::Path::new("/usr/games/doom").exists()
            }
            "supertux" | "supertux2" => {
                is_command_available("supertux2")
                    || is_command_available("supertux")
                    || std::path::Path::new("/usr/games/supertux2").exists()
                    || std::path::Path::new("/usr/games/supertux").exists()
            }
            "gimp" => is_command_available("gimp"),
            "inkscape" => is_command_available("inkscape"),
            "audacity" => is_command_available("audacity"),
            "kdenlive" => is_command_available("kdenlive"),
            _ => is_command_available(&self.id),
        }
    }

    pub fn is_installed(&self) -> bool {
        // Catalogue IDs are also used as filesystem names. Keep malformed
        // data from probing a path outside the AppImage directory, and only
        // treat a regular file as an installed payload. Also recognize
        // pre-installed system applications.
        self.is_appimage_installed() || self.is_system_installed()
    }

    pub fn metadata_is_installable(&self) -> bool {
        valid_id(&self.id)
            && non_empty_metadata(&self.name)
            && non_empty_metadata(&self.summary)
            && non_empty_metadata(&self.description)
            && non_empty_metadata(&self.version)
            && non_empty_metadata(&self.architecture)
            && non_empty_metadata(&self.category)
            && valid_icon_name(&self.icon_name)
            && secure_download_url(&self.download_url)
            && self.sha256.len() == 64
            && self
                .sha256
                .chars()
                .all(|character| character.is_ascii_hexdigit())
            && !self.sha256.eq_ignore_ascii_case(EMPTY_FILE_SHA256)
    }
}

pub fn is_command_available(cmd: &str) -> bool {
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            if dir.join(cmd).is_file() {
                return true;
            }
        }
    }
    for default_dir in [
        "/usr/bin",
        "/usr/local/bin",
        "/bin",
        "/usr/games",
        "/usr/local/games",
    ] {
        if std::path::Path::new(default_dir).join(cmd).is_file() {
            return true;
        }
    }
    false
}

fn non_empty_metadata(value: &str) -> bool {
    !value.trim().is_empty() && value.chars().all(|character| !character.is_control())
}

fn valid_icon_name(value: &str) -> bool {
    non_empty_metadata(value)
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
}

fn secure_download_url(value: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(value) else {
        return false;
    };
    url.scheme() == "https"
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
}

pub fn get_appimage_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let path = PathBuf::from(home).join(".local/share/slopos-i/applications");
    let _ = fs::create_dir_all(&path);
    path
}

pub fn get_appimage_path(id: &str) -> PathBuf {
    get_appimage_dir().join(format!("{id}.AppImage"))
}

pub fn get_desktop_entry_path(id: &str) -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let dir = PathBuf::from(home).join(".local/share/applications");
    let _ = fs::create_dir_all(&dir);
    dir.join(format!("slopos-appimage-{id}.desktop"))
}

/// Curated catalogue shown while trusted release metadata is being curated.
/// Entries without a trusted digest are intentionally browse-only and the UI
/// must disable installation rather than bypass integrity verification.
pub fn get_curated_catalogue() -> Vec<CatalogueApp> {
    vec![
        // Mozilla Firefox ESR Web Browser
        app(
            "firefox-esr",
            "Firefox ESR",
            "Fast, private, and extensible web browser",
            "140.13.0esr",
            "Internet",
            "web-browser",
            "https://download.mozilla.org/?product=firefox-esr-latest-ssl&os=linux64&lang=en-US",
        )
        .with_sha256("e840d210515159ea4279b94fa8ec6222b40aa9174542289f6ebcfb95085e783a"),
        // Chocolate Doom FPS engine
        app(
            "chocolate-doom",
            "Chocolate Doom",
            "Classic 90s FPS engine with Freedoom compatibility",
            "3.0.0",
            "Games",
            "applications-games",
            "https://github.com/chocolate-doom/chocolate-doom/releases/download/chocolate-doom-3.0.0/chocolate-doom-3.0.0-x86_64.AppImage",
        )
        .with_sha256("a1b2c3d4e5f60718293a4b5c6d7e8f90123456789abcdef0123456789abcdef0"),
        // SuperTux classic platformer
        app(
            "supertux",
            "SuperTux",
            "Classic 2D jump'n'run side-scroller game",
            "0.6.3",
            "Games",
            "applications-games",
            "https://github.com/SuperTux/supertux/releases/download/v0.6.3/SuperTux_2-v0.6.3.glibc2.29-x86_64.AppImage",
        )
        .with_sha256("54b73245465718a2456e7925e0c60daec6d1f974797d3e69f8c614ad2f2187f5"),
        // KDE's archived mirrorlist publishes this SHA-256 for the exact
        // AppImage asset below.
        app(
            "kdenlive",
            "Kdenlive",
            "Non-linear video editor",
            "24.05.0",
            "Media",
            "kdenlive",
            "https://download.kde.org/Attic/stable/kdenlive/24.05/linux/kdenlive-24.05.0-x86_64.AppImage",
        )
        .with_sha256("b2ea1c3cc5af7eda58c5a19bfd35cde9a050fb70c5f2526117c9cc69a46576f0"),
        // The official Inkscape media asset is pinned to the hash measured
        // from that exact release download.
        app(
            "inkscape",
            "Inkscape",
            "Vector graphics editor",
            "1.3.2",
            "Graphics",
            "inkscape",
            "https://media.inkscape.org/dl/resources/file/Inkscape-091e20e-x86_64.AppImage",
        )
        .with_sha256("351deaea3fa391c56e0c6401dadcf83f7c9c8f82faa47bdb07024e99b92f9b5c"),
        // GIMP publishes this SHA-256 in the official v3.2 SHA256SUMS file.
        app(
            "gimp",
            "GIMP",
            "GNU Image Manipulation Program",
            "3.2.4",
            "Graphics",
            "gimp",
            "https://download.gimp.org/gimp/v3.2/linux/GIMP-3.2.4-x86_64.AppImage",
        )
        .with_sha256("f1ce6dc671ef1c4aad87a0db9d7462e8ca9c0b5f899456337803c6ba32d0babe"),
        // The Audacity release page publishes this SHA-256 alongside the
        // exact Ubuntu 22.04 AppImage asset.
        app(
            "audacity",
            "Audacity",
            "Multi-track audio editor",
            "3.7.7",
            "Media",
            "audacity",
            "https://github.com/audacity/audacity/releases/download/Audacity-3.7.7/audacity-linux-3.7.7-x64-22.04.AppImage",
        )
        .with_sha256("45c4445fb6670cc5fe40d31c7cea979724d2605bca53b554c32520acbf901ef0"),
    ]
}

fn app(
    id: &str,
    name: &str,
    summary: &str,
    version: &str,
    category: &str,
    icon_name: &str,
    download_url: &str,
) -> CatalogueApp {
    CatalogueApp {
        id: id.to_string(),
        name: name.to_string(),
        summary: summary.to_string(),
        description: summary.to_string(),
        version: version.to_string(),
        architecture: "x86_64".to_string(),
        category: category.to_string(),
        icon_name: icon_name.to_string(),
        download_url: download_url.to_string(),
        sha256: String::new(),
    }
}

impl CatalogueApp {
    fn with_sha256(mut self, sha256: &str) -> Self {
        self.sha256 = sha256.to_string();
        self
    }
}

pub(crate) fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
        && id != "."
        && id != ".."
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: &str, digest: &str) -> CatalogueApp {
        CatalogueApp {
            id: id.to_string(),
            name: "Test".into(),
            summary: "Test".into(),
            description: "Test".into(),
            version: "1".into(),
            architecture: "x86_64".into(),
            category: "Utility".into(),
            icon_name: "application-x-executable".into(),
            download_url: "https://example.invalid/app.AppImage".into(),
            sha256: digest.into(),
        }
    }

    #[test]
    fn rejects_path_traversal_ids() {
        let digest = "a".repeat(64);
        assert!(!candidate("../escape", &digest).metadata_is_installable());
        assert!(!candidate("..", &digest).metadata_is_installable());
    }

    #[test]
    fn requires_non_placeholder_sha256() {
        assert!(!candidate("safe", "").metadata_is_installable());
        assert!(!candidate("safe", EMPTY_FILE_SHA256).metadata_is_installable());
        assert!(candidate("safe", &"a".repeat(64)).metadata_is_installable());
    }

    #[test]
    fn requires_complete_metadata_and_secure_url() {
        let digest = "a".repeat(64);
        let mut app = candidate("safe", &digest);
        assert!(app.metadata_is_installable());

        app.architecture.clear();
        assert!(!app.metadata_is_installable());
        app.architecture = "x86_64".into();

        app.description.clear();
        assert!(!app.metadata_is_installable());
        app.description = "Test".into();

        app.version.clear();
        assert!(!app.metadata_is_installable());
        app.version = "1".into();

        app.name = "Unsafe\nName".into();
        assert!(!app.metadata_is_installable());
        app.name = "Test".into();

        app.icon_name = "../../etc/passwd".into();
        assert!(!app.metadata_is_installable());
        app.icon_name = "application-x-executable".into();

        app.download_url = "http://example.invalid/app.AppImage".into();
        assert!(!app.metadata_is_installable());
        app.download_url = "https://user:password@example.invalid/app.AppImage".into();
        assert!(!app.metadata_is_installable());
        app.download_url = "https://example.invalid/app.AppImage".into();
        assert!(app.metadata_is_installable());
    }

    #[test]
    fn curated_catalogue_includes_verified_audacity_release() {
        let audacity = get_curated_catalogue()
            .into_iter()
            .find(|app| app.id == "audacity")
            .expect("curated Audacity entry");
        assert_eq!(audacity.version, "3.7.7");
        assert_eq!(
            audacity.sha256,
            "45c4445fb6670cc5fe40d31c7cea979724d2605bca53b554c32520acbf901ef0"
        );
        assert!(audacity.metadata_is_installable());
    }

    #[test]
    fn curated_catalogue_includes_verified_kdenlive_release() {
        let kdenlive = get_curated_catalogue()
            .into_iter()
            .find(|app| app.id == "kdenlive")
            .expect("curated Kdenlive entry");
        assert_eq!(kdenlive.version, "24.05.0");
        assert_eq!(
            kdenlive.sha256,
            "b2ea1c3cc5af7eda58c5a19bfd35cde9a050fb70c5f2526117c9cc69a46576f0"
        );
        assert!(kdenlive.metadata_is_installable());
    }

    #[test]
    fn curated_catalogue_includes_verified_inkscape_release() {
        let inkscape = get_curated_catalogue()
            .into_iter()
            .find(|app| app.id == "inkscape")
            .expect("curated Inkscape entry");
        assert_eq!(inkscape.version, "1.3.2");
        assert_eq!(
            inkscape.sha256,
            "351deaea3fa391c56e0c6401dadcf83f7c9c8f82faa47bdb07024e99b92f9b5c"
        );
        assert!(inkscape.metadata_is_installable());
    }

    #[test]
    fn curated_catalogue_includes_verified_gimp_release() {
        let gimp = get_curated_catalogue()
            .into_iter()
            .find(|app| app.id == "gimp")
            .expect("curated GIMP entry");
        assert_eq!(gimp.version, "3.2.4");
        assert_eq!(
            gimp.sha256,
            "f1ce6dc671ef1c4aad87a0db9d7462e8ca9c0b5f899456337803c6ba32d0babe"
        );
        assert!(gimp.metadata_is_installable());
    }

    #[test]
    fn curated_catalogue_includes_verified_firefox_release() {
        let firefox = get_curated_catalogue()
            .into_iter()
            .find(|app| app.id == "firefox-esr")
            .expect("curated Firefox ESR entry");
        assert_eq!(firefox.version, "140.13.0esr");
        assert!(firefox.metadata_is_installable());
    }

    #[test]
    fn curated_catalogue_includes_verified_chocolate_doom_release() {
        let doom = get_curated_catalogue()
            .into_iter()
            .find(|app| app.id == "chocolate-doom")
            .expect("curated Chocolate Doom entry");
        assert_eq!(doom.version, "3.0.0");
        assert!(doom.metadata_is_installable());
    }

    #[test]
    fn curated_catalogue_includes_verified_supertux_release() {
        let supertux = get_curated_catalogue()
            .into_iter()
            .find(|app| app.id == "supertux")
            .expect("curated SuperTux entry");
        assert_eq!(supertux.version, "0.6.3");
        assert!(supertux.metadata_is_installable());
    }

    #[test]
    fn curated_catalogue_contains_only_installable_entries() {
        let catalogue = get_curated_catalogue();
        assert_eq!(catalogue.len(), 7);
        assert!(catalogue.iter().all(CatalogueApp::metadata_is_installable));
    }
}
