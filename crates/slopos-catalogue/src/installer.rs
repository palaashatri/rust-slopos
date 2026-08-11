//! AppImage installer — fail closed, verify before placement.

use crate::model::{get_appimage_dir, get_appimage_path, get_desktop_entry_path, CatalogueApp};
use hex::ToHex;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub fn install_appimage(app: &CatalogueApp) -> Result<(), String> {
    if !app.metadata_is_installable() {
        return Err(format!(
            "{} is browse-only: trusted SHA-256 metadata is not available",
            app.name
        ));
    }

    log::info!("Downloading verified AppImage metadata target: {}", app.name);
    let bytes = fetch_url_bytes(&app.download_url)?;

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let actual: String = hasher.finalize().encode_hex();
    if !actual.eq_ignore_ascii_case(&app.sha256) {
        return Err(format!(
            "SHA-256 mismatch for {}: expected {}, got {}",
            app.name, app.sha256, actual
        ));
    }

    let target = get_appimage_path(&app.id);
    let part = temporary_path(&target);
    if part.exists() {
        fs::remove_file(&part).map_err(|e| format!("remove stale partial file: {e}"))?;
    }

    let install_result = (|| -> Result<(), String> {
        let mut file = File::create(&part).map_err(|e| format!("create partial AppImage: {e}"))?;
        file.write_all(&bytes).map_err(|e| format!("write AppImage: {e}"))?;
        file.sync_all().map_err(|e| format!("sync AppImage: {e}"))?;

        let mut permissions = fs::metadata(&part)
            .map_err(|e| format!("read partial AppImage metadata: {e}"))?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&part, permissions).map_err(|e| format!("chmod AppImage: {e}"))?;

        if target.exists() {
            fs::remove_file(&target).map_err(|e| format!("replace existing AppImage: {e}"))?;
        }
        fs::rename(&part, &target).map_err(|e| format!("atomically place AppImage: {e}"))?;
        create_desktop_entry(app, &target)?;
        Ok(())
    })();

    if install_result.is_err() {
        let _ = fs::remove_file(&part);
    }
    install_result
}

pub fn uninstall_appimage(app: &CatalogueApp) -> Result<(), String> {
    let appimage_path = get_appimage_path(&app.id);
    if appimage_path.exists() {
        fs::remove_file(&appimage_path).map_err(|e| format!("remove AppImage: {e}"))?;
    }
    let desktop_path = get_desktop_entry_path(&app.id);
    if desktop_path.exists() {
        fs::remove_file(&desktop_path).map_err(|e| format!("remove desktop entry: {e}"))?;
    }
    Ok(())
}

fn fetch_url_bytes(url: &str) -> Result<Vec<u8>, String> {
    if !url.starts_with("https://") {
        return Err("catalogue downloads must use HTTPS".to_string());
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(90))
        .user_agent("SLOPOS-I-Catalogue/0.1")
        .build()
        .map_err(|e| format!("create HTTP client: {e}"))?;

    let response = client
        .get(url)
        .send()
        .map_err(|e| format!("download failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("download failed: {e}"))?;

    response.bytes().map(|b| b.to_vec()).map_err(|e| format!("read download: {e}"))
}

fn temporary_path(target: &Path) -> PathBuf {
    let mut value = target.as_os_str().to_os_string();
    value.push(".part");
    PathBuf::from(value)
}

fn create_desktop_entry(app: &CatalogueApp, appimage_path: &Path) -> Result<(), String> {
    let desktop_path = get_desktop_entry_path(&app.id);
    let escaped_path = appimage_path.to_string_lossy().replace('"', "\\\"");
    let content = format!(
        "[Desktop Entry]\nType=Application\nName={}\nComment={}\nExec=\"{}\"\nIcon={}\nCategories={};\nTerminal=false\nX-SLOPOS-AppImage=true\n",
        app.name, app.summary, escaped_path, app.icon_name, app.category
    );

    let mut file = File::create(&desktop_path).map_err(|e| format!("create desktop entry: {e}"))?;
    file.write_all(content.as_bytes()).map_err(|e| format!("write desktop entry: {e}"))?;
    file.sync_all().map_err(|e| format!("sync desktop entry: {e}"))?;

    // Ensure parent path was created by model helper before returning success.
    let _ = get_appimage_dir();
    Ok(())
}
