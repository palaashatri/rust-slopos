//! Model manifest parsing and model file verification.
//!
//! The manifest lives at `models/vision/manifest.toml` and describes every
//! model file the vision subsystem can load: purpose, architecture, SHA-256
//! digest, expected size, input shape, normalization, output interpretation,
//! licenses, and attribution. The daemon never ships or downloads models
//! implicitly; a model file is only loaded after it has been verified against
//! this manifest.

use crate::error::VisionError;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

/// Normalization constants applied to model input pixels.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Normalization {
    pub scale: f32,
    pub mean: Vec<f32>,
    pub std: Vec<f32>,
}

/// A single model (or model-support data file) entry in the manifest.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ModelEntry {
    pub id: String,
    pub version: String,
    /// One of: `text_detection`, `text_recognition`, `text_dictionary`,
    /// `subject_segmentation`.
    pub purpose: String,
    pub architecture: String,
    /// File name relative to the model directory.
    pub file: String,
    /// Lowercase hex SHA-256 of the file contents.
    pub sha256: String,
    pub size: u64,
    /// ONNX input shape; `-1` marks a dynamic dimension.
    pub input_shape: Vec<i32>,
    pub normalization: Option<Normalization>,
    pub output_interpretation: String,
    pub source_url: String,
    pub model_license: String,
    pub weight_license: String,
    pub attribution: String,
    pub redistribution: String,
}

impl ModelEntry {
    /// Absolute path of this entry's file under the model directory.
    pub fn path(&self, models_dir: &Path) -> PathBuf {
        models_dir.join(&self.file)
    }

    fn validate_file_path(&self) -> Result<(), VisionError> {
        let path = Path::new(&self.file);
        if self.file.is_empty()
            || path.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
            || path
                .components()
                .any(|component| matches!(component, Component::CurDir))
        {
            return Err(VisionError::ManifestLoad {
                path: self.file.clone(),
                message: "model file must be a non-empty relative path without . or .. components"
                    .to_string(),
            });
        }
        Ok(())
    }
}

/// The full parsed manifest.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ModelManifest {
    #[serde(default)]
    pub models: Vec<ModelEntry>,
}

impl ModelManifest {
    /// Find an entry by its `id`.
    pub fn by_id(&self, id: &str) -> Option<&ModelEntry> {
        self.models.iter().find(|m| m.id == id)
    }

    /// Find all entries with the given purpose.
    pub fn by_purpose<'a>(&'a self, purpose: &'a str) -> impl Iterator<Item = &'a ModelEntry> + 'a {
        self.models.iter().filter(move |m| m.purpose == purpose)
    }

    fn validate(&self) -> Result<(), VisionError> {
        for entry in &self.models {
            entry.validate_file_path()?;
        }
        for (index, entry) in self.models.iter().enumerate() {
            if self
                .models
                .iter()
                .take(index)
                .any(|previous| previous.id == entry.id)
            {
                return Err(VisionError::ManifestLoad {
                    path: "manifest.toml".to_string(),
                    message: format!("duplicate model id: {}", entry.id),
                });
            }
        }
        Ok(())
    }

    /// Compute the SHA-256 of `data` as lowercase hex.
    pub fn sha256_hex(data: &[u8]) -> String {
        hex::encode(Sha256::digest(data))
    }
}

/// Load and parse `manifest.toml` from `models_dir`.
pub fn load_manifest(models_dir: &Path) -> Result<ModelManifest, VisionError> {
    let path = models_dir.join("manifest.toml");
    let text = fs::read_to_string(&path).map_err(|err| VisionError::ManifestLoad {
        path: path.display().to_string(),
        message: err.to_string(),
    })?;
    let manifest: ModelManifest =
        toml::from_str(&text).map_err(|err| VisionError::ManifestLoad {
            path: path.display().to_string(),
            message: err.to_string(),
        })?;
    manifest.validate()?;
    Ok(manifest)
}

/// Streaming SHA-256 of a file, as lowercase hex.
pub fn file_sha256(path: &Path) -> Result<String, VisionError> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// The status of a model file relative to its manifest entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelStatus {
    /// File exists with the expected size and digest.
    Installed,
    /// File does not exist.
    Missing,
    /// File exists but its digest (or size) does not match the manifest.
    HashMismatch,
}

/// Check whether the file for `entry` is present and matches its manifest
/// digest. This reads the whole file, so it is intended for install-time
/// validation and diagnostics, not per-job calls.
pub fn verify_model(models_dir: &Path, entry: &ModelEntry) -> Result<ModelStatus, VisionError> {
    let path = entry.path(models_dir);
    let meta = match fs::metadata(&path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(ModelStatus::Missing),
        Err(err) => return Err(VisionError::Io(err)),
    };
    if meta.len() != entry.size {
        return Ok(ModelStatus::HashMismatch);
    }
    let actual = file_sha256(&path)?;
    if actual == entry.sha256 {
        Ok(ModelStatus::Installed)
    } else {
        Ok(ModelStatus::HashMismatch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_manifest(dir: &std::path::Path, content: &str) -> PathBuf {
        let path = dir.join("manifest.toml");
        fs::write(&path, content).unwrap();
        path
    }

    fn sample_entry(dir: &std::path::Path) -> (ModelEntry, PathBuf) {
        let data = b"fake model bytes";
        let file = std::path::Path::new("model.onnx");
        let path = dir.join(file);
        fs::write(&path, data).unwrap();
        let entry = ModelEntry {
            id: "test-model".into(),
            version: "1.0.0".into(),
            purpose: "subject_segmentation".into(),
            architecture: "test".into(),
            file: file.display().to_string(),
            sha256: ModelManifest::sha256_hex(data),
            size: data.len() as u64,
            input_shape: vec![1, 3, -1, -1],
            normalization: None,
            output_interpretation: "single-channel probability".into(),
            source_url: "https://example.invalid/model.onnx".into(),
            model_license: "Apache-2.0".into(),
            weight_license: "Apache-2.0".into(),
            attribution: "test".into(),
            redistribution: "allowed".into(),
        };
        (entry, path)
    }

    #[test]
    fn manifest_round_trip() {
        let dir = tempdir().unwrap();
        let toml = r#"
            [[models]]
            id = "ocr-det"
            version = "1.0.0"
            purpose = "text_detection"
            architecture = "DBNet"
            file = "det.onnx"
            sha256 = "abc123"
            size = 1024
            input_shape = [1, 3, -1, -1]
            output_interpretation = "probability map"
            source_url = "https://example.invalid/det.onnx"
            model_license = "Apache-2.0"
            weight_license = "Apache-2.0"
            attribution = "test"
            redistribution = "allowed"
        "#;
        write_manifest(dir.path(), toml);
        let manifest = load_manifest(dir.path()).unwrap();
        assert_eq!(manifest.models.len(), 1);
        assert_eq!(manifest.by_id("ocr-det").unwrap().architecture, "DBNet");
        assert_eq!(
            manifest.by_id("ocr-det").unwrap().input_shape,
            vec![1, 3, -1, -1]
        );
        assert_eq!(manifest.by_purpose("text_detection").count(), 1);
    }

    #[test]
    fn missing_manifest_is_manifest_error() {
        let dir = tempdir().unwrap();
        match load_manifest(dir.path()) {
            Err(VisionError::ManifestLoad { .. }) => {}
            other => panic!("expected ManifestLoad error, got {other:?}"),
        }
    }

    #[test]
    fn hash_verification_succeeds() {
        let dir = tempdir().unwrap();
        let (entry, _) = sample_entry(dir.path());
        let status = verify_model(dir.path(), &entry).unwrap();
        assert_eq!(status, ModelStatus::Installed);
    }

    #[test]
    fn hash_verification_detects_corruption() {
        let dir = tempdir().unwrap();
        let (entry, path) = sample_entry(dir.path());
        fs::write(&path, b"tampered").unwrap();
        let status = verify_model(dir.path(), &entry).unwrap();
        assert_eq!(status, ModelStatus::HashMismatch);
    }

    #[test]
    fn hash_verification_reports_missing() {
        let dir = tempdir().unwrap();
        let (entry, _) = sample_entry(dir.path());
        fs::remove_file(entry.path(dir.path())).unwrap();
        let status = verify_model(dir.path(), &entry).unwrap();
        assert_eq!(status, ModelStatus::Missing);
    }

    #[test]
    fn file_sha256_matches_reference() {
        let dir = tempdir().unwrap();
        let data = b"hello world";
        let path = dir.path().join("data.bin");
        fs::write(&path, data).unwrap();
        assert_eq!(
            file_sha256(&path).unwrap(),
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn manifest_rejects_model_path_escape() {
        let dir = tempdir().unwrap();
        write_manifest(
            dir.path(),
            r#"
                [[models]]
                id = "escape"
                version = "1.0.0"
                purpose = "subject_segmentation"
                architecture = "test"
                file = "../outside.onnx"
                sha256 = "0000000000000000000000000000000000000000000000000000000000000000"
                size = 1
                input_shape = [1, 3, -1, -1]
                output_interpretation = "test"
                source_url = "https://example.invalid/model.onnx"
                model_license = "MIT"
                weight_license = "MIT"
                attribution = "test"
                redistribution = "allowed"
            "#,
        );

        let error = load_manifest(dir.path()).unwrap_err();
        assert!(matches!(error, VisionError::ManifestLoad { .. }));
    }

    #[test]
    fn manifest_rejects_duplicate_model_ids() {
        let dir = tempdir().unwrap();
        write_manifest(
            dir.path(),
            r#"
                [[models]]
                id = "duplicate"
                version = "1.0.0"
                purpose = "subject_segmentation"
                architecture = "test"
                file = "one.onnx"
                sha256 = "0000000000000000000000000000000000000000000000000000000000000000"
                size = 1
                input_shape = [1, 3, -1, -1]
                output_interpretation = "test"
                source_url = "https://example.invalid/one.onnx"
                model_license = "MIT"
                weight_license = "MIT"
                attribution = "test"
                redistribution = "allowed"

                [[models]]
                id = "duplicate"
                version = "1.0.0"
                purpose = "subject_segmentation"
                architecture = "test"
                file = "two.onnx"
                sha256 = "0000000000000000000000000000000000000000000000000000000000000000"
                size = 1
                input_shape = [1, 3, -1, -1]
                output_interpretation = "test"
                source_url = "https://example.invalid/two.onnx"
                model_license = "MIT"
                weight_license = "MIT"
                attribution = "test"
                redistribution = "allowed"
            "#,
        );

        let error = load_manifest(dir.path()).unwrap_err();
        assert!(matches!(error, VisionError::ManifestLoad { .. }));
    }
}
