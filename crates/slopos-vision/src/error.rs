/// Deterministic errors produced by SLOPOS Vision.
#[derive(Debug, thiserror::Error)]
pub enum VisionError {
    #[error("model manifest could not be loaded from {path}: {message}")]
    ManifestLoad { path: String, message: String },

    #[error("model manifest does not contain an entry for: {0}")]
    ManifestEntry(String),

    #[error("model hash verification failed for {path}: expected {expected}, computed {actual}")]
    HashMismatch {
        path: String,
        expected: String,
        actual: String,
    },

    #[error("model file not found: {0}")]
    ModelNotFound(String),

    #[error("model could not be loaded by the inference runtime: {0}")]
    ModelLoad(String),

    #[error("model inference failed: {0}")]
    Inference(String),

    #[error("invalid model output: {0}")]
    InvalidOutput(String),

    #[error("unsupported image format: {0}")]
    UnsupportedFormat(String),

    #[error("image could not be decoded: {0}")]
    Decode(String),

    #[error(
        "encoded image exceeds the maximum allowed size ({max_bytes} bytes); got {actual_bytes} bytes"
    )]
    EncodedImageTooLarge { max_bytes: u64, actual_bytes: u64 },

    #[error("image exceeds the maximum allowed size ({max} pixels); got {pixels} pixels")]
    ImageTooLarge { max: u64, pixels: u64 },

    #[error("no confident subject was found in the image")]
    NoSubject,

    #[error("operation was cancelled")]
    Cancelled,

    #[error("unsupported script or model limitation: {0}")]
    Unsupported(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
