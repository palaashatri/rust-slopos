//! SLOPOS Vision protocol — shared request/response types between the
//! `slopos-visiond` daemon and its clients.
//!
//! The protocol is intentionally small and dependency-light:
//!
//! - JSON/serde friendly so the client and daemon can share one schema.
//! - Local-only by construction: capabilities report local execution and
//!   explicit no-download model provisioning.
//! - Output destinations are represented as daemon-managed asset IDs or inline
//!   bytes, never arbitrary filesystem paths.
//! - Image and path-adjacent metadata can be validated against explicit bounds
//!   before a daemon accepts work.

use serde::{Deserialize, Serialize};

pub const VISION_PROTOCOL_VERSION: u32 = 1;
pub const MAX_ID_LEN: usize = 128;
pub const MAX_FILE_STEM_LEN: usize = 128;
pub const MAX_EXTENSION_LEN: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolEnvelope<T> {
    pub protocol_version: u32,
    pub payload: T,
}

impl<T> ProtocolEnvelope<T> {
    pub fn new(payload: T) -> Self {
        Self {
            protocol_version: VISION_PROTOCOL_VERSION,
            payload,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisionOperation {
    ExtractText,
    LiftSubject,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Rejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    LocalOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelProvisioning {
    PreinstalledOnly,
    ImportedModelPackOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageMediaType {
    Png,
    Jpeg,
    Webp,
    Bmp,
    Tiff,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactRole {
    SourceImage,
    LiftedSubject,
    SubjectMask,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidRequest,
    UnsupportedProtocolVersion,
    InvalidIdentifier,
    InvalidFileLabel,
    EncodedBytesMismatch,
    EncodedBytesExceeded,
    PixelLimitExceeded,
    UnsupportedMediaType,
    MissingAsset,
    ModelUnavailable,
    HashMismatch,
    DecodeFailed,
    InferenceFailed,
    NoSubject,
    Cancelled,
    Internal,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientRequestId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetId(pub String);

impl ClientRequestId {
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_identifier(&self.0, "client_request_id")
    }
}

impl JobId {
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_identifier(&self.0, "job_id")
    }
}

impl AssetId {
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_identifier(&self.0, "asset_id")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PixelSize {
    pub width: u32,
    pub height: u32,
}

impl PixelSize {
    pub fn pixel_count(&self) -> u64 {
        (self.width as u64).saturating_mul(self.height as u64)
    }

    pub fn validate(&self, max_pixels: u64) -> Result<(), ValidationError> {
        if self.width == 0 || self.height == 0 {
            return Err(ValidationError::ZeroDimensions);
        }
        let pixels = self.pixel_count();
        if pixels > max_pixels {
            return Err(ValidationError::PixelLimitExceeded { pixels, max_pixels });
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileLabel {
    pub stem: String,
    pub extension: Option<String>,
}

impl FileLabel {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.stem.is_empty() || self.stem.len() > MAX_FILE_STEM_LEN {
            return Err(ValidationError::InvalidFileLabel("invalid stem length"));
        }
        if !is_safe_label_component(&self.stem) {
            return Err(ValidationError::InvalidFileLabel(
                "stem contains path separator or control characters",
            ));
        }
        if let Some(extension) = &self.extension {
            if extension.is_empty() || extension.len() > MAX_EXTENSION_LEN {
                return Err(ValidationError::InvalidFileLabel(
                    "invalid extension length",
                ));
            }
            if !extension
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
            {
                return Err(ValidationError::InvalidFileLabel(
                    "extension contains unsupported characters",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageMetadata {
    pub media_type: ImageMediaType,
    pub encoded_bytes: u64,
    pub dimensions: PixelSize,
    pub sha256: Option<String>,
    pub label: Option<FileLabel>,
}

impl ImageMetadata {
    pub fn validate(&self, max_encoded_bytes: u64, max_pixels: u64) -> Result<(), ValidationError> {
        if self.encoded_bytes > max_encoded_bytes {
            return Err(ValidationError::EncodedBytesExceeded {
                bytes: self.encoded_bytes,
                max_bytes: max_encoded_bytes,
            });
        }
        self.dimensions.validate(max_pixels)?;
        if let Some(label) = &self.label {
            label.validate()?;
        }
        if let Some(sha256) = &self.sha256 {
            if sha256.len() != 64 || !sha256.chars().all(|ch| ch.is_ascii_hexdigit()) {
                return Err(ValidationError::InvalidSha256);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InlineImage {
    pub metadata: ImageMetadata,
    pub bytes: Vec<u8>,
}

impl InlineImage {
    pub fn validate(&self, max_encoded_bytes: u64, max_pixels: u64) -> Result<(), ValidationError> {
        self.metadata.validate(max_encoded_bytes, max_pixels)?;
        let actual = self.bytes.len() as u64;
        if self.metadata.encoded_bytes != actual {
            return Err(ValidationError::EncodedBytesMismatch {
                declared: self.metadata.encoded_bytes,
                actual,
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredImage {
    pub asset_id: AssetId,
    pub metadata: ImageMetadata,
}

impl StoredImage {
    pub fn validate(&self, max_encoded_bytes: u64, max_pixels: u64) -> Result<(), ValidationError> {
        self.asset_id.validate()?;
        self.metadata.validate(max_encoded_bytes, max_pixels)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum ImageSource {
    Inline(InlineImage),
    Stored(StoredImage),
}

impl ImageSource {
    pub fn validate(&self, max_encoded_bytes: u64, max_pixels: u64) -> Result<(), ValidationError> {
        match self {
            Self::Inline(image) => image.validate(max_encoded_bytes, max_pixels),
            Self::Stored(image) => image.validate(max_encoded_bytes, max_pixels),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactDescriptor {
    pub role: ArtifactRole,
    pub image: StoredImage,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExtractTextOptions {
    pub include_words: bool,
    pub include_line_confidence: bool,
    pub min_confidence: Option<f32>,
}

impl Default for ExtractTextOptions {
    fn default() -> Self {
        Self {
            include_words: true,
            include_line_confidence: true,
            min_confidence: None,
        }
    }
}

impl ExtractTextOptions {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if let Some(min_confidence) = self.min_confidence {
            if !min_confidence.is_finite() || !(0.0..=1.0).contains(&min_confidence) {
                return Err(ValidationError::InvalidConfidence);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiftSubjectOptions {
    pub include_mask: bool,
}

impl Default for LiftSubjectOptions {
    fn default() -> Self {
        Self { include_mask: true }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExtractTextJob {
    pub source: ImageSource,
    pub options: ExtractTextOptions,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiftSubjectJob {
    pub source: ImageSource,
    pub options: LiftSubjectOptions,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum VisionJob {
    ExtractText(ExtractTextJob),
    LiftSubject(LiftSubjectJob),
}

impl VisionJob {
    pub fn operation(&self) -> VisionOperation {
        match self {
            Self::ExtractText(_) => VisionOperation::ExtractText,
            Self::LiftSubject(_) => VisionOperation::LiftSubject,
        }
    }

    pub fn validate(&self, max_encoded_bytes: u64, max_pixels: u64) -> Result<(), ValidationError> {
        match self {
            Self::ExtractText(job) => {
                job.source.validate(max_encoded_bytes, max_pixels)?;
                job.options.validate()?;
            }
            Self::LiftSubject(job) => job.source.validate(max_encoded_bytes, max_pixels)?,
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SubmitJobRequest {
    pub client_request_id: Option<ClientRequestId>,
    pub job: VisionJob,
}

impl SubmitJobRequest {
    pub fn validate(&self, max_encoded_bytes: u64, max_pixels: u64) -> Result<(), ValidationError> {
        if let Some(id) = &self.client_request_id {
            id.validate()?;
        }
        self.job.validate(max_encoded_bytes, max_pixels)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobLookupRequest {
    pub job_id: JobId,
}

impl JobLookupRequest {
    pub fn validate(&self) -> Result<(), ValidationError> {
        self.job_id.validate()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetLookupRequest {
    pub asset_id: AssetId,
}

impl AssetLookupRequest {
    pub fn validate(&self) -> Result<(), ValidationError> {
        self.asset_id.validate()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProbeRequest;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VisionRequest {
    SubmitJob(SubmitJobRequest),
    GetJobStatus(JobLookupRequest),
    GetJobResult(JobLookupRequest),
    CancelJob(JobLookupRequest),
    GetAsset(AssetLookupRequest),
    Probe(ProbeRequest),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcrWord {
    pub text: String,
    pub bounds: PixelRect,
    pub confidence_milli: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcrLine {
    pub text: String,
    pub bounds: PixelRect,
    pub confidence_milli: Option<u16>,
    pub words: Vec<OcrWord>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractTextResult {
    pub full_text: String,
    pub lines: Vec<OcrLine>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiftSubjectResult {
    pub cutout: ArtifactDescriptor,
    pub mask: Option<ArtifactDescriptor>,
    pub opaque_pixel_count: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum VisionResult {
    ExtractText(ExtractTextResult),
    LiftSubject(LiftSubjectResult),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisionError {
    pub code: ErrorCode,
    pub message: String,
    pub operation: Option<VisionOperation>,
    pub retryable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptedResponse {
    pub job_id: JobId,
    pub operation: VisionOperation,
    pub status: JobStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobStatusResponse {
    pub job_id: JobId,
    pub operation: VisionOperation,
    pub status: JobStatus,
    pub error: Option<VisionError>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobResultResponse {
    pub job_id: JobId,
    pub operation: VisionOperation,
    pub status: JobStatus,
    pub result: Option<VisionResult>,
    pub error: Option<VisionError>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetDataResponse {
    pub asset: StoredImage,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityResponse {
    pub execution_mode: ExecutionMode,
    pub model_provisioning: ModelProvisioning,
    pub supported_operations: Vec<VisionOperation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VisionResponse {
    Accepted(AcceptedResponse),
    JobStatus(JobStatusResponse),
    JobResult(JobResultResponse),
    Asset(AssetDataResponse),
    Capabilities(CapabilityResponse),
    Error(VisionError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValidationError {
    InvalidIdentifier(&'static str),
    InvalidFileLabel(&'static str),
    InvalidSha256,
    ZeroDimensions,
    EncodedBytesMismatch { declared: u64, actual: u64 },
    EncodedBytesExceeded { bytes: u64, max_bytes: u64 },
    PixelLimitExceeded { pixels: u64, max_pixels: u64 },
    InvalidConfidence,
}

fn validate_identifier(value: &str, field: &'static str) -> Result<(), ValidationError> {
    if value.is_empty() || value.len() > MAX_ID_LEN {
        return Err(ValidationError::InvalidIdentifier(field));
    }
    if !value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':'))
    {
        return Err(ValidationError::InvalidIdentifier(field));
    }
    Ok(())
}

fn is_safe_label_component(value: &str) -> bool {
    value
        .chars()
        .all(|ch| !ch.is_control() && ch != '/' && ch != '\\' && ch != ':' && ch != '\0')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_metadata() -> ImageMetadata {
        ImageMetadata {
            media_type: ImageMediaType::Png,
            encoded_bytes: 4,
            dimensions: PixelSize {
                width: 2,
                height: 2,
            },
            sha256: Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into()),
            label: Some(FileLabel {
                stem: "sample".into(),
                extension: Some("png".into()),
            }),
        }
    }

    #[test]
    fn submit_request_round_trips_through_json() {
        let request = ProtocolEnvelope::new(VisionRequest::SubmitJob(SubmitJobRequest {
            client_request_id: Some(ClientRequestId("req-123".into())),
            job: VisionJob::ExtractText(ExtractTextJob {
                source: ImageSource::Inline(InlineImage {
                    metadata: sample_metadata(),
                    bytes: vec![1, 2, 3, 4],
                }),
                options: ExtractTextOptions::default(),
            }),
        }));

        let json = serde_json::to_string(&request).unwrap();
        let decoded: ProtocolEnvelope<VisionRequest> = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, request);
    }

    #[test]
    fn inline_image_validation_rejects_mismatched_length() {
        let image = InlineImage {
            metadata: sample_metadata(),
            bytes: vec![1, 2, 3],
        };

        let err = image.validate(1024, 4096).unwrap_err();
        assert_eq!(
            err,
            ValidationError::EncodedBytesMismatch {
                declared: 4,
                actual: 3
            }
        );
    }

    #[test]
    fn file_label_rejects_path_separators() {
        let label = FileLabel {
            stem: "../escape".into(),
            extension: Some("png".into()),
        };

        let err = label.validate().unwrap_err();
        assert_eq!(
            err,
            ValidationError::InvalidFileLabel("stem contains path separator or control characters")
        );
    }

    #[test]
    fn stored_image_validation_checks_asset_id() {
        let image = StoredImage {
            asset_id: AssetId("bad/id".into()),
            metadata: sample_metadata(),
        };

        let err = image.validate(1024, 4096).unwrap_err();
        assert_eq!(err, ValidationError::InvalidIdentifier("asset_id"));
    }

    #[test]
    fn pixel_size_rejects_oversized_images() {
        let size = PixelSize {
            width: 5000,
            height: 5000,
        };

        let err = size.validate(1_000_000).unwrap_err();
        assert_eq!(
            err,
            ValidationError::PixelLimitExceeded {
                pixels: 25_000_000,
                max_pixels: 1_000_000,
            }
        );
    }

    #[test]
    fn capabilities_report_local_only_no_download_semantics() {
        let response = ProtocolEnvelope::new(VisionResponse::Capabilities(CapabilityResponse {
            execution_mode: ExecutionMode::LocalOnly,
            model_provisioning: ModelProvisioning::ImportedModelPackOnly,
            supported_operations: vec![VisionOperation::ExtractText, VisionOperation::LiftSubject],
        }));

        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["payload"]["type"], "capabilities");
        assert_eq!(json["payload"]["execution_mode"], "local_only");
        assert_eq!(
            json["payload"]["model_provisioning"],
            "imported_model_pack_only"
        );
    }

    #[test]
    fn asset_lookup_round_trips_through_json() {
        let request = ProtocolEnvelope::new(VisionRequest::GetAsset(AssetLookupRequest {
            asset_id: AssetId("asset-123".into()),
        }));
        let decoded: ProtocolEnvelope<VisionRequest> =
            serde_json::from_str(&serde_json::to_string(&request).unwrap()).unwrap();
        assert_eq!(decoded, request);
    }

    #[test]
    fn invalid_confidence_is_rejected_by_job_validation() {
        let job = VisionJob::ExtractText(ExtractTextJob {
            source: ImageSource::Inline(InlineImage {
                metadata: sample_metadata(),
                bytes: vec![1, 2, 3, 4],
            }),
            options: ExtractTextOptions {
                include_words: true,
                include_line_confidence: true,
                min_confidence: Some(f32::NAN),
            },
        });
        assert_eq!(
            job.validate(1024, 4096),
            Err(ValidationError::InvalidConfidence)
        );
    }
}
