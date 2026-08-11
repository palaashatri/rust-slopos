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
    pub fn is_installed(&self) -> bool {
        get_appimage_path(&self.id).exists()
    }

    pub fn metadata_is_installable(&self) -> bool {
        self.download_url.starts_with("https://")
            && self.sha256.len() == 64
            && self.sha256.chars().all(|c| c.is_ascii_hexdigit())
            && !self.sha256.eq_ignore_ascii_case(EMPTY_FILE_SHA256)
    }
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

/// Seed catalogue shown while trusted release metadata is being curated.
/// Entries with an empty digest are intentionally browse-only and the UI must
/// disable installation rather than bypass integrity verification.
pub fn get_curated_catalogue() -> Vec<CatalogueApp> {
    vec![
        app("kdenlive", "Kdenlive", "Non-linear video editor", "24.05.0", "Media", "kdenlive",
            "https://files.kde.org/kdenlive/release/kdenlive-24.05.0-x86_64.AppImage"),
        app("inkscape", "Inkscape", "Vector graphics editor", "1.3.2", "Graphics", "inkscape",
            "https://inkscape.org/gallery/item/44621/Inkscape-e7c6843-x86_64.AppImage"),
        app("gimp", "GIMP", "GNU Image Manipulation Program", "2.10.38", "Graphics", "gimp",
            "https://download.gimp.org/gimp/v2.10/appimage/GIMP_AppImage-git-2.10.38-x86_64.AppImage"),
        app("obs-studio", "OBS Studio", "Screen recording and live streaming", "30.1.2", "Media", "com.obsproject.Studio",
            "https://github.com/obsproject/obs-studio/releases/download/30.1.2/OBS-Studio-30.1.2-x86_64.AppImage"),
        app("audacity", "Audacity", "Multi-track audio editor", "3.5.1", "Media", "audacity",
            "https://github.com/audacity/audacity/releases/download/Audacity-3.5.1/audacity-linux-3.5.1-x86_64.AppImage"),
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
