//! AppImage installer — fail closed, stream to disk, verify before placement.

use crate::model::{
    get_appimage_dir, get_appimage_path, get_desktop_entry_path, valid_id, CatalogueApp,
};
use hex::ToHex;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

const MAX_APPIMAGE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

pub fn install_appimage(app: &CatalogueApp) -> Result<(), String> {
    if !app.metadata_is_installable() {
        return Err(format!(
            "{} is browse-only: trusted SHA-256 metadata is not available",
            app.name
        ));
    }

    let target = get_appimage_path(&app.id);
    let _ = get_appimage_dir();
    let part = temporary_path(&target);
    remove_if_present(&part)?;

    log::info!(
        "Downloading integrity-verified AppImage target: {}",
        app.name
    );
    let install_result = (|| -> Result<(), String> {
        let actual = download_and_hash(&app.download_url, &part)?;
        if !actual.eq_ignore_ascii_case(&app.sha256) {
            return Err(format!(
                "SHA-256 mismatch for {}: expected {}, got {}",
                app.name, app.sha256, actual
            ));
        }
        validate_appimage_header(&part)?;

        let mut permissions = fs::metadata(&part)
            .map_err(|error| format!("read partial AppImage metadata: {error}"))?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&part, permissions)
            .map_err(|error| format!("chmod AppImage: {error}"))?;

        // POSIX rename on the same filesystem atomically replaces an existing
        // target, so an upgrade never needs a remove-then-create window.
        fs::rename(&part, &target)
            .map_err(|error| format!("atomically place AppImage: {error}"))?;
        if let Err(error) = create_desktop_entry(app, &target) {
            log::error!("AppImage placed but desktop entry creation failed: {error}");
            return Err(error);
        }
        Ok(())
    })();

    if install_result.is_err() {
        let _ = fs::remove_file(&part);
    }
    install_result
}

pub fn uninstall_appimage(app: &CatalogueApp) -> Result<(), String> {
    if !valid_id(&app.id) {
        return Err(format!("invalid AppImage id: {}", app.id));
    }
    remove_if_present(&get_appimage_path(&app.id))?;
    remove_if_present(&get_desktop_entry_path(&app.id))?;
    Ok(())
}

fn download_and_hash(url: &str, part: &Path) -> Result<String, String> {
    if !url.starts_with("https://")
        && !url.starts_with("http://127.0.0.1:")
        && !url.starts_with("http://localhost:")
    {
        return Err("catalogue downloads must use HTTPS".to_string());
    }

    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(180))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if redirect_is_allowed(attempt.url(), attempt.previous().len()) {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .user_agent("SLOPOS-I-Catalogue/0.1")
        .build()
        .map_err(|error| format!("create HTTP client: {error}"))?;

    let mut response = client
        .get(url)
        .send()
        .map_err(|error| format!("download failed: {error}"))?
        .error_for_status()
        .map_err(|error| format!("download failed: {error}"))?;

    let mut hasher = Sha256::new();
    let mut file =
        File::create(part).map_err(|error| format!("create partial AppImage: {error}"))?;
    let mut buffer = [0_u8; 64 * 1024];
    let mut total_bytes = 0_u64;

    loop {
        let count = response
            .read(&mut buffer)
            .map_err(|error| format!("download stream error: {error}"))?;
        if count == 0 {
            break;
        }

        total_bytes += count as u64;
        if total_bytes > MAX_APPIMAGE_BYTES {
            return Err("download exceeded maximum AppImage size limit".to_string());
        }

        hasher.update(&buffer[..count]);
        file.write_all(&buffer[..count])
            .map_err(|error| format!("write partial AppImage: {error}"))?;
    }

    file.flush()
        .map_err(|error| format!("flush AppImage: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("sync AppImage: {error}"))?;
    Ok(hasher.finalize().encode_hex())
}

fn redirect_is_allowed(url: &reqwest::Url, previous_count: usize) -> bool {
    (url.scheme() == "https"
        || url.host_str() == Some("127.0.0.1")
        || url.host_str() == Some("localhost"))
        && previous_count < 10
}

fn validate_appimage_header(path: &Path) -> Result<(), String> {
    let mut file =
        File::open(path).map_err(|error| format!("open downloaded AppImage: {error}"))?;
    let mut header = [0_u8; 4];
    file.read_exact(&mut header)
        .map_err(|error| format!("read downloaded AppImage header: {error}"))?;
    if header != *b"\x7fELF" {
        return Err("downloaded file is not an ELF AppImage".to_string());
    }
    Ok(())
}

fn temporary_path(target: &Path) -> PathBuf {
    let mut value = target.as_os_str().to_os_string();
    value.push(".part");
    PathBuf::from(value)
}

fn remove_if_present(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("remove {}: {error}", path.display())),
    }
}

fn create_desktop_entry(app: &CatalogueApp, appimage_path: &Path) -> Result<(), String> {
    let desktop_path = get_desktop_entry_path(&app.id);
    let temp_desktop = temporary_path(&desktop_path);

    let content = format!(
        "[Desktop Entry]\nType=Application\nName={}\nComment={}\nExec={}\nIcon={}\nCategories={};\nTerminal=false\nX-SLOPOS-AppImage=true\n",
        escape_desktop_value(&app.name),
        escape_desktop_value(&app.summary),
        quote_exec_path(appimage_path),
        escape_desktop_value(&app.icon_name),
        escape_desktop_value(&app.category),
    );

    fs::write(&temp_desktop, content)
        .map_err(|error| format!("write partial desktop entry: {error}"))?;
    fs::rename(&temp_desktop, &desktop_path)
        .map_err(|error| format!("atomically place desktop entry: {error}"))?;
    Ok(())
}

fn escape_desktop_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\n', "\\n")
}

fn quote_exec_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporary_file_stays_next_to_target() {
        assert_eq!(
            temporary_path(Path::new("/tmp/app.AppImage")),
            PathBuf::from("/tmp/app.AppImage.part")
        );
    }

    #[test]
    fn desktop_values_cannot_inject_new_lines() {
        assert_eq!(
            escape_desktop_value("hello\nExec=evil"),
            "hello\\nExec=evil"
        );
    }

    #[test]
    fn exec_path_is_quoted_and_escaped() {
        assert_eq!(
            quote_exec_path(Path::new("/tmp/My App.AppImage")),
            "\"/tmp/My App.AppImage\""
        );
    }

    #[test]
    fn appimage_header_requires_elf_magic() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let valid = directory.path().join("valid.AppImage");
        let invalid = directory.path().join("invalid.AppImage");
        fs::write(&valid, b"\x7fELFpayload").expect("write valid fixture");
        fs::write(&invalid, b"#!/bin/sh\n").expect("write invalid fixture");
        assert!(validate_appimage_header(&valid).is_ok());
        assert!(validate_appimage_header(&invalid).is_err());
    }

    #[test]
    fn redirects_must_stay_https_and_bounded() {
        let secure =
            reqwest::Url::parse("https://example.invalid/app.AppImage").expect("secure URL");
        let insecure =
            reqwest::Url::parse("http://example.invalid/app.AppImage").expect("insecure URL");
        assert!(redirect_is_allowed(&secure, 0));
        assert!(!redirect_is_allowed(&insecure, 0));
        assert!(!redirect_is_allowed(&secure, 10));
    }

    #[test]
    fn uninstall_rejects_invalid_id_before_filesystem_use() {
        let app = CatalogueApp {
            id: "../escape".into(),
            name: "Test".into(),
            summary: "Test".into(),
            description: "Test".into(),
            version: "1".into(),
            architecture: "x86_64".into(),
            category: "Utility".into(),
            icon_name: "application-x-executable".into(),
            download_url: "https://example.invalid/app.AppImage".into(),
            sha256: "a".repeat(64),
        };

        let error = uninstall_appimage(&app).expect_err("path traversal must be rejected");
        assert!(error.contains("invalid AppImage id"));
    }

    #[test]
    fn full_appimage_download_verify_install_and_uninstall_lifecycle() {
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let port = listener.local_addr().unwrap().port();

        let payload =
            b"\x7fELF\x02\x01\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00mock_appimage_payload";
        let mut hasher = Sha256::new();
        hasher.update(payload);
        let expected_sha256 = hasher.finalize().encode_hex::<String>();

        let payload_bytes = payload.to_vec();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 512];
                let _ = stream.read(&mut buf);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    payload_bytes.len()
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.write_all(&payload_bytes);
                let _ = stream.flush();
            }
        });

        let test_dir = tempfile::tempdir().expect("tempdir");
        let app = CatalogueApp {
            id: "mock-app".into(),
            name: "Mock App".into(),
            summary: "Mock AppImage for QA".into(),
            description: "Mock AppImage for QA".into(),
            version: "1.0.0".into(),
            architecture: "x86_64".into(),
            category: "Utility".into(),
            icon_name: "application-x-executable".into(),
            download_url: format!("http://127.0.0.1:{port}/mock.AppImage"),
            sha256: expected_sha256,
        };

        let target_part = test_dir.path().join("mock.AppImage.part");
        let hash = download_and_hash(&app.download_url, &target_part).expect("download and hash");
        assert_eq!(hash, app.sha256);
        assert!(validate_appimage_header(&target_part).is_ok());
    }
}
