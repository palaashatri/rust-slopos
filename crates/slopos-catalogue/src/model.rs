//! AppImage Catalogue Data Models

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

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
}

pub fn get_appimage_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let path = PathBuf::from(home).join(".local/share/slopos-i/applications");
    let _ = fs::create_dir_all(&path);
    path
}

pub fn get_appimage_path(id: &str) -> PathBuf {
    get_appimage_dir().join(format!("{}.AppImage", id))
}

pub fn get_desktop_entry_path(id: &str) -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let dir = PathBuf::from(home).join(".local/share/applications");
    let _ = fs::create_dir_all(&dir);
    dir.join(format!("slopos-appimage-{}.desktop", id))
}

/// Returns curated default AppImage catalogue list
pub fn get_curated_catalogue() -> Vec<CatalogueApp> {
    vec![
        CatalogueApp {
            id: "kdenlive".to_string(),
            name: "Kdenlive".to_string(),
            summary: "Non-linear video editor".to_string(),
            description: "Full-featured open source video editor for creators and video production.".to_string(),
            version: "24.05.0".to_string(),
            architecture: "x86_64".to_string(),
            category: "Media".to_string(),
            icon_name: "kdenlive".to_string(),
            download_url: "https://files.kde.org/kdenlive/release/kdenlive-24.05.0-x86_64.AppImage".to_string(),
            sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
        },
        CatalogueApp {
            id: "inkscape".to_string(),
            name: "Inkscape".to_string(),
            summary: "Vector graphics editor".to_string(),
            description: "Professional vector graphics editor for illustration, logo design, and SVG editing.".to_string(),
            version: "1.3.2".to_string(),
            architecture: "x86_64".to_string(),
            category: "Graphics".to_string(),
            icon_name: "inkscape".to_string(),
            download_url: "https://inkscape.org/gallery/item/44621/Inkscape-e7c6843-x86_64.AppImage".to_string(),
            sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
        },
        CatalogueApp {
            id: "gimp".to_string(),
            name: "GIMP".to_string(),
            summary: "GNU Image Manipulation Program".to_string(),
            description: "Advanced image editor for photo retouching, image composition, and graphic design.".to_string(),
            version: "2.10.38".to_string(),
            architecture: "x86_64".to_string(),
            category: "Graphics".to_string(),
            icon_name: "gimp".to_string(),
            download_url: "https://download.gimp.org/gimp/v2.10/appimage/GIMP_AppImage-git-2.10.38-x86_64.AppImage".to_string(),
            sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
        },
        CatalogueApp {
            id: "obs-studio".to_string(),
            name: "OBS Studio".to_string(),
            summary: "Screen recording and live streaming".to_string(),
            description: "Free and open source software for video recording and live streaming.".to_string(),
            version: "30.1.2".to_string(),
            architecture: "x86_64".to_string(),
            category: "Media".to_string(),
            icon_name: "com.obsproject.Studio".to_string(),
            download_url: "https://github.com/obsproject/obs-studio/releases/download/30.1.2/OBS-Studio-30.1.2-x86_64.AppImage".to_string(),
            sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
        },
        CatalogueApp {
            id: "audacity".to_string(),
            name: "Audacity".to_string(),
            summary: "Multi-track audio editor".to_string(),
            description: "Easy-to-use, multi-track audio recorder and editor for Linux.".to_string(),
            version: "3.5.1".to_string(),
            architecture: "x86_64".to_string(),
            category: "Media".to_string(),
            icon_name: "audacity".to_string(),
            download_url: "https://github.com/audacity/audacity/releases/download/Audacity-3.5.1/audacity-linux-3.5.1-x86_64.AppImage".to_string(),
            sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
        },
    ]
}
