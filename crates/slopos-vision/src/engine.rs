//! The [`VisionEngine`]: a single entry point for OCR and subject
//! segmentation with lazy, manifest-verified model loading.

use crate::composite;
use crate::decode::{
    decode_image_limited, read_image_limited, DEFAULT_MAX_ENCODED_INPUT_BYTES,
    DEFAULT_MAX_SOURCE_PIXELS,
};
use crate::error::VisionError;
use crate::manifest::{self, ModelEntry, ModelManifest, ModelStatus};
use crate::ocr::OcrEngine;
use crate::segment::SegmentEngine;
use crate::types::{
    LiftedSubject, OcrOptions, OcrResult, PixelRect, SegmentationOptions, SubjectMask,
};
use image::DynamicImage;
use parking_lot::Mutex;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Default model directory (relative to the working directory).
pub const DEFAULT_MODELS_DIR: &str = "models/vision";

/// Static configuration for a [`VisionEngine`].
#[derive(Debug, Clone, PartialEq)]
pub struct VisionEngineConfig {
    /// Directory containing `manifest.toml` and the model files it references.
    pub models_dir: PathBuf,
    /// Maximum encoded image size accepted before reading the file into memory.
    pub max_encoded_input_bytes: u64,
    /// Maximum source-image pixel count for any single job.
    pub max_source_pixels: u64,
}

impl Default for VisionEngineConfig {
    fn default() -> Self {
        Self {
            models_dir: PathBuf::from(DEFAULT_MODELS_DIR),
            max_encoded_input_bytes: DEFAULT_MAX_ENCODED_INPUT_BYTES,
            max_source_pixels: DEFAULT_MAX_SOURCE_PIXELS,
        }
    }
}

/// The manifest entry ids used by this engine.
pub const MODEL_DET: &str = "ppocr-text-det";
pub const MODEL_REC: &str = "ppocr-text-rec";
pub const MODEL_KEYS: &str = "ppocr-keys";
pub const MODEL_SEG: &str = "u2netp";

/// A model-capability report for diagnostics and UI.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ModelCapability {
    pub id: String,
    pub purpose: String,
    pub architecture: String,
    pub version: String,
    pub file: String,
    pub size: u64,
    pub model_license: String,
    pub weight_license: String,
    pub attribution: String,
    pub redistribution: String,
}

/// Vision engine: OCR and subject segmentation with lazy, manifest-verified
/// model loading.
///
/// Instances are cheap to construct; models load on first use and are cached.
/// The engine is `Send + Sync` and safe to share across threads.
pub struct VisionEngine {
    config: VisionEngineConfig,
    manifest: ModelManifest,
    ocr: Mutex<Option<Arc<OcrEngine>>>,
    segment: Mutex<Option<Arc<SegmentEngine>>>,
}

impl VisionEngine {
    /// Load the manifest and validate the configuration. Model files are not
    /// loaded (or hash-checked) until their first use.
    pub fn load(config: VisionEngineConfig) -> Result<Self, VisionError> {
        let manifest = manifest::load_manifest(&config.models_dir)?;
        Ok(Self {
            config,
            manifest,
            ocr: Mutex::new(None),
            segment: Mutex::new(None),
        })
    }

    pub fn config(&self) -> &VisionEngineConfig {
        &self.config
    }

    fn entry(&self, id: &str) -> Result<&ModelEntry, VisionError> {
        self.manifest
            .by_id(id)
            .ok_or_else(|| VisionError::ManifestEntry(id.to_string()))
    }

    /// Path of a manifest entry's file, hash-verified as installed.
    fn verified_path(&self, id: &str) -> Result<PathBuf, VisionError> {
        let entry = self.entry(id)?;
        let path = entry.path(&self.config.models_dir);
        match manifest::verify_model(&self.config.models_dir, entry)? {
            ModelStatus::Installed => Ok(path),
            ModelStatus::Missing => Err(VisionError::ModelNotFound(path.display().to_string())),
            ModelStatus::HashMismatch => {
                let actual = match fs::metadata(&path) {
                    Ok(_) => manifest::file_sha256(&path).unwrap_or_else(|_| "unreadable".into()),
                    Err(_) => "missing".into(),
                };
                Err(VisionError::HashMismatch {
                    path: path.display().to_string(),
                    expected: entry.sha256.clone(),
                    actual,
                })
            }
        }
    }

    fn ocr(&self) -> Result<Arc<OcrEngine>, VisionError> {
        let mut guard = self.ocr.lock();
        if let Some(engine) = guard.as_ref() {
            return Ok(engine.clone());
        }
        let det = self.verified_path(MODEL_DET)?;
        let rec = self.verified_path(MODEL_REC)?;
        let keys = self.verified_path(MODEL_KEYS)?;
        let engine = Arc::new(OcrEngine::load(&det, &rec, &keys)?);
        *guard = Some(engine.clone());
        Ok(engine)
    }

    fn segment(&self) -> Result<Arc<SegmentEngine>, VisionError> {
        let mut guard = self.segment.lock();
        if let Some(engine) = guard.as_ref() {
            return Ok(engine.clone());
        }
        let path = self.verified_path(MODEL_SEG)?;
        let engine = Arc::new(SegmentEngine::load(&path)?);
        *guard = Some(engine.clone());
        Ok(engine)
    }

    fn check_source_pixels(&self, image: &DynamicImage) -> Result<(), VisionError> {
        let pixels = (image.width() as u64).saturating_mul(image.height() as u64);
        if pixels > self.config.max_source_pixels {
            return Err(VisionError::ImageTooLarge {
                max: self.config.max_source_pixels,
                pixels,
            });
        }
        Ok(())
    }

    /// Decode an image file, guarding against decompression bombs.
    pub fn decode_image(&self, path: &Path) -> Result<DynamicImage, VisionError> {
        read_image_limited(
            path,
            self.config.max_encoded_input_bytes,
            self.config.max_source_pixels,
        )
    }

    /// Decode raw image bytes, guarding against decompression bombs.
    pub fn decode_image_bytes(&self, data: &[u8]) -> Result<DynamicImage, VisionError> {
        decode_image_limited(data, self.config.max_source_pixels)
    }

    /// Run OCR over `image`.
    pub fn extract_text(
        &self,
        image: &DynamicImage,
        options: OcrOptions,
    ) -> Result<OcrResult, VisionError> {
        if !options.min_confidence.is_finite() || !(0.0..=1.0).contains(&options.min_confidence) {
            return Err(VisionError::Unsupported(
                "OCR minimum confidence must be finite and between 0 and 1".to_string(),
            ));
        }
        self.check_source_pixels(image)?;
        self.ocr()?.extract_text(image, &options)
    }

    /// Segment the main subject of `image`, returning a source-resolution mask.
    pub fn segment_subject(
        &self,
        image: &DynamicImage,
        options: SegmentationOptions,
    ) -> Result<SubjectMask, VisionError> {
        validate_segmentation_options(&options)?;
        self.check_source_pixels(image)?;
        self.segment()?.segment(image, &options)
    }

    /// Segment and cut out the main subject on a transparent background.
    pub fn lift_subject(
        &self,
        image: &DynamicImage,
        options: SegmentationOptions,
    ) -> Result<LiftedSubject, VisionError> {
        let mask = self.segment_subject(image, options)?;
        if !mask.alpha.iter().any(|&a| a > 0) {
            return Err(VisionError::NoSubject);
        }
        let rgba = image.to_rgba8();
        let cutout = composite::composite_subject(&rgba, &mask)?;
        let source_bounds = PixelRect::new(0, 0, rgba.width(), rgba.height());
        Ok(LiftedSubject {
            image: cutout,
            mask,
            source_bounds,
        })
    }

    /// Capability report for every model in the manifest.
    pub fn capabilities(&self) -> Vec<ModelCapability> {
        self.manifest
            .models
            .iter()
            .map(|m| ModelCapability {
                id: m.id.clone(),
                purpose: m.purpose.clone(),
                architecture: m.architecture.clone(),
                version: m.version.clone(),
                file: m.file.clone(),
                size: m.size,
                model_license: m.model_license.clone(),
                weight_license: m.weight_license.clone(),
                attribution: m.attribution.clone(),
                redistribution: m.redistribution.clone(),
            })
            .collect()
    }

    /// Per-model install status (`Installed` / `Missing` / `HashMismatch`).
    ///
    /// Verification errors are retained in the result instead of being
    /// silently dropped, so callers can distinguish a broken model path from
    /// a model that is simply not installed.
    pub fn model_status(&self) -> Vec<(String, Result<ModelStatus, VisionError>)> {
        self.manifest
            .models
            .iter()
            .map(|m| {
                (
                    m.id.clone(),
                    manifest::verify_model(&self.config.models_dir, m),
                )
            })
            .collect()
    }
}

fn validate_segmentation_options(options: &SegmentationOptions) -> Result<(), VisionError> {
    let mask = &options.mask_post;
    if !mask.threshold.is_finite() || !(0.0..=1.0).contains(&mask.threshold) {
        return Err(VisionError::Unsupported(
            "segmentation threshold must be finite and between 0 and 1".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn default_config_points_at_models_dir() {
        let cfg = VisionEngineConfig::default();
        assert_eq!(cfg.models_dir, PathBuf::from("models/vision"));
        assert_eq!(cfg.max_encoded_input_bytes, DEFAULT_MAX_ENCODED_INPUT_BYTES);
        assert_eq!(cfg.max_source_pixels, DEFAULT_MAX_SOURCE_PIXELS);
    }

    #[test]
    fn missing_manifest_errors_on_load() {
        let dir = tempdir().unwrap();
        let result = VisionEngine::load(VisionEngineConfig {
            models_dir: dir.path().to_path_buf(),
            ..Default::default()
        });
        assert!(matches!(result, Err(VisionError::ManifestLoad { .. })));
    }

    #[test]
    fn capabilities_reflect_manifest() {
        let dir = tempdir().unwrap();
        let toml = r#"
            [[models]]
            id = "u2netp"
            version = "1.0.0"
            purpose = "subject_segmentation"
            architecture = "U2Netp"
            file = "u2netp.onnx"
            sha256 = "0000000000000000000000000000000000000000000000000000000000000000"
            size = 1
            input_shape = [1, 3, 320, 320]
            output_interpretation = "probability"
            source_url = "https://example.invalid/u2netp.onnx"
            model_license = "Apache-2.0"
            weight_license = "Apache-2.0"
            attribution = "test"
            redistribution = "allowed"
        "#;
        std::fs::write(dir.path().join("manifest.toml"), toml).unwrap();
        let engine = VisionEngine::load(VisionEngineConfig {
            models_dir: dir.path().to_path_buf(),
            ..Default::default()
        })
        .unwrap();
        let caps = engine.capabilities();
        assert_eq!(caps.len(), 1);
        assert_eq!(caps[0].id, "u2netp");
        let status = engine.model_status();
        assert_eq!(status.len(), 1);
        assert_eq!(status[0].0, "u2netp");
        assert!(matches!(status[0].1, Ok(ModelStatus::Missing)));
    }

    #[test]
    fn decode_image_rejects_encoded_input_before_reading_it() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("manifest.toml"), "models = []").unwrap();
        let image_path = dir.path().join("input.bin");
        std::fs::write(&image_path, [0_u8; 4]).unwrap();
        let engine = VisionEngine::load(VisionEngineConfig {
            models_dir: dir.path().to_path_buf(),
            max_encoded_input_bytes: 3,
            ..Default::default()
        })
        .unwrap();

        let error = engine.decode_image(&image_path).unwrap_err();
        assert!(matches!(
            error,
            VisionError::EncodedImageTooLarge {
                max_bytes: 3,
                actual_bytes: 4
            }
        ));
    }

    #[test]
    fn model_status_preserves_verification_errors() {
        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join("model.onnx")).unwrap();
        let directory_size = std::fs::metadata(dir.path().join("model.onnx"))
            .unwrap()
            .len();
        let manifest = format!(
            r#"
                [[models]]
                id = "directory-model"
                version = "1.0.0"
                purpose = "subject_segmentation"
                architecture = "test"
                file = "model.onnx"
                sha256 = "{}"
                size = {}
                input_shape = [1, 3, -1, -1]
                output_interpretation = "test"
                source_url = "https://example.invalid/model.onnx"
                model_license = "MIT"
                weight_license = "MIT"
                attribution = "test"
                redistribution = "allowed"
            "#,
            "0".repeat(64),
            directory_size
        );
        std::fs::write(dir.path().join("manifest.toml"), manifest).unwrap();
        let engine = VisionEngine::load(VisionEngineConfig {
            models_dir: dir.path().to_path_buf(),
            ..Default::default()
        })
        .unwrap();

        let status = engine.model_status();
        assert!(matches!(status[0].1, Err(VisionError::Io(_))));
    }

    #[test]
    fn invalid_ocr_confidence_is_rejected_before_model_load() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("manifest.toml"), "models = []").unwrap();
        let engine = VisionEngine::load(VisionEngineConfig {
            models_dir: dir.path().to_path_buf(),
            ..Default::default()
        })
        .unwrap();
        let error = engine
            .extract_text(
                &DynamicImage::new_rgb8(1, 1),
                OcrOptions {
                    min_confidence: f32::NAN,
                    ..Default::default()
                },
            )
            .unwrap_err();
        assert!(matches!(error, VisionError::Unsupported(_)));
    }

    #[test]
    fn invalid_segmentation_threshold_is_rejected_before_model_load() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("manifest.toml"), "models = []").unwrap();
        let engine = VisionEngine::load(VisionEngineConfig {
            models_dir: dir.path().to_path_buf(),
            ..Default::default()
        })
        .unwrap();
        let error = engine
            .segment_subject(
                &DynamicImage::new_rgb8(1, 1),
                SegmentationOptions {
                    mask_post: crate::types::MaskPostProcessOptions {
                        threshold: 2.0,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .unwrap_err();
        assert!(matches!(error, VisionError::Unsupported(_)));
    }
}
