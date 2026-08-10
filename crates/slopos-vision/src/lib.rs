//! SLOPOS Vision — the pure-Rust, local-first vision and image-processing
//! library for SLOPOS-I.
//!
//! This crate owns:
//!
//! * model manifest loading and model hash verification;
//! * OCR preprocessing, inference, and result normalization;
//! * subject segmentation preprocessing, inference, and mask post-processing;
//! * alpha compositing for subject cutouts;
//! * image decoding and encoding abstractions with allocation guards;
//! * model capability reporting;
//! * cancellation checks;
//! * deterministic errors.
//!
//! This crate is intentionally platform-neutral. It knows nothing about
//! Wayland, SLOPOS windows, D-Bus, the Finder, the Preview application, or
//! clipboard implementations.
//!
//! # Models
//!
//! Models are loaded lazily on first use from the configured model directory.
//! The directory must contain a `manifest.toml` (see [`manifest`]) and the
//! model files referenced by it. Every model file is hash-verified against the
//! manifest before it is loaded.
//!
//! # Example
//!
//! ```no_run
//! use std::path::Path;
//! use slopos_vision::{VisionEngine, VisionEngineConfig};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let engine = VisionEngine::load(VisionEngineConfig {
//!     models_dir: Path::new("models/vision").to_path_buf(),
//!     ..Default::default()
//! })?;
//!
//! let image = image::open("photo.png")?;
//! let ocr = engine.extract_text(&image, Default::default())?;
//! println!("{}", ocr.text());
//!
//! let lifted = engine.lift_subject(&image, Default::default())?;
//! lifted.image.save("cutout.png")?;
//! # Ok(())
//! # }
//! ```

pub mod composite;
pub mod decode;
pub mod engine;
pub mod error;
pub mod geometry;
pub mod manifest;
pub mod mask;
pub mod ocr;
pub mod segment;
pub mod types;

pub use engine::{VisionEngine, VisionEngineConfig};
pub use error::VisionError;
pub use types::{
    LiftedSubject, MaskPostProcessOptions, OcrOptions, OcrResult, PixelRect, SegmentationOptions,
    SubjectMask, TextLine, TextWord,
};
