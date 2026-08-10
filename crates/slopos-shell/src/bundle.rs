use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::launch_services::AppBundle;

/// Parsed `Resources/Info.toml` (spec §5.2). Field names are the manifest keys.
#[derive(Debug, Clone, Deserialize)]
pub struct InfoToml {
    pub bundle_id: String,
    pub name: String,
    pub version: String,
    pub entrypoint: String,
    #[serde(default)]
    pub supported_types: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug)]
pub enum BundleError {
    NotABundle(PathBuf),
    Read(PathBuf, String),
    Parse(PathBuf, String),
}

/// Load one `<Name>.app` directory into an `AppBundle`.
/// `dir` must be the `.app` directory itself. `path` on the returned bundle is
/// the absolute `.app` dir; `entrypoint` is taken verbatim from Info.toml.
pub fn load_bundle(dir: &Path) -> Result<AppBundle, BundleError> {
    let dir = match dir.canonicalize() {
        Ok(path) => path,
        Err(_) => return Err(BundleError::NotABundle(dir.to_path_buf())),
    };

    if !dir
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.ends_with(".app"))
    {
        return Err(BundleError::NotABundle(dir));
    }

    let manifest = dir.join("Resources").join("Info.toml");
    if !manifest.is_file() {
        return Err(BundleError::NotABundle(dir));
    }

    let raw = std::fs::read_to_string(&manifest)
        .map_err(|e| BundleError::Read(manifest.clone(), e.to_string()))?;

    let info: InfoToml =
        toml::from_str(&raw).map_err(|e| BundleError::Parse(manifest, e.to_string()))?;

    Ok(AppBundle {
        bundle_id: info.bundle_id,
        name: info.name,
        version: info.version,
        path: dir.to_string_lossy().into_owned(),
        entrypoint: info.entrypoint,
        supported_types: info.supported_types,
        permissions: info.permissions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_minimal_bundle() {
        let tmp = std::env::temp_dir().join("rs_test_TextEdit.app");
        let res = tmp.join("Resources");
        std::fs::create_dir_all(&res).unwrap();
        std::fs::write(
            res.join("Info.toml"),
            "bundle_id=\"com.slopos.textedit\"\nname=\"TextEdit\"\nversion=\"0.1.0\"\nentrypoint=\"bin/textedit\"\nsupported_types=[\"txt\"]\npermissions=[\"files.read\"]\n",
        )
        .unwrap();
        let b = load_bundle(&tmp).unwrap();
        assert_eq!(b.bundle_id, "com.slopos.textedit");
        assert_eq!(b.entrypoint, "bin/textedit");
        assert_eq!(b.supported_types, vec!["txt"]);
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn rejects_non_bundle_dir() {
        assert!(load_bundle(Path::new("/tmp")).is_err());
    }
}
