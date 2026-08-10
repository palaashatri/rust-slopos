//! SLOPOS Vision client — the client half of the `slopos-visiond` interface.
//!
//! Apps (Finder, Preview, and friends) use this crate to submit OCR and
//! subject-segmentation jobs to the background daemon over a local IPC
//! channel, keeping the UI threads responsive.

use serde_json::Error as JsonError;
use slopos_vision_protocol::{
    AcceptedResponse, AssetDataResponse, AssetId, AssetLookupRequest, CapabilityResponse, JobId,
    JobLookupRequest, JobResultResponse, JobStatusResponse, ProbeRequest, ProtocolEnvelope,
    SubmitJobRequest, VisionRequest, VisionResponse, VISION_PROTOCOL_VERSION,
};
use socket2::{Domain, SockAddr, Socket, Type};
use std::{
    env,
    ffi::OsStr,
    io::{self, BufRead, BufReader, Read, Write},
    os::fd::OwnedFd,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    time::Duration,
};
use thiserror::Error;

/// Environment variable that overrides the session-scoped Vision socket.
pub const VISION_SOCKET_ENV: &str = "SLOPOS_VISION_SOCKET";

/// Environment variable used to derive the default Vision socket location.
pub const XDG_RUNTIME_DIR_ENV: &str = "XDG_RUNTIME_DIR";

/// Relative socket location below `$XDG_RUNTIME_DIR`.
pub const DEFAULT_SOCKET_SUFFIX: &str = "slopos-i/vision.sock";

/// Default maximum serialized request size, excluding the line delimiter.
pub const DEFAULT_MAX_REQUEST_BYTES: usize = 4 * 1024 * 1024;

/// Default maximum serialized response size, excluding the line delimiter.
pub const DEFAULT_MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

/// Upper bound for caller-configured request and response frames.
pub const MAX_CONFIGURED_FRAME_BYTES: usize = 64 * 1024 * 1024;

/// Default timeout for establishing the Unix-stream connection.
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

/// Default timeout for writing one request frame.
pub const DEFAULT_WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// Default timeout for reading one response frame.
pub const DEFAULT_READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Resolve the Vision daemon socket from the current process environment.
///
/// `SLOPOS_VISION_SOCKET` takes precedence. In production, an unset socket
/// override requires `XDG_RUNTIME_DIR`; the deterministic temporary fallback
/// is compiled into the unit-test path only.
pub fn socket_path_from_environment() -> Result<PathBuf, VisionClientError> {
    resolve_socket_path(
        env::var_os(VISION_SOCKET_ENV).as_deref(),
        env::var_os(XDG_RUNTIME_DIR_ENV).as_deref(),
        cfg!(test),
    )
}

fn resolve_socket_path(
    explicit_socket: Option<&OsStr>,
    runtime_dir: Option<&OsStr>,
    allow_test_fallback: bool,
) -> Result<PathBuf, VisionClientError> {
    if let Some(socket) = explicit_socket {
        if socket.is_empty() {
            return Err(VisionClientError::InvalidSocketPath(
                VISION_SOCKET_ENV.to_owned(),
            ));
        }
        return Ok(PathBuf::from(socket));
    }

    if let Some(runtime_dir) = runtime_dir {
        if !runtime_dir.is_empty() {
            return Ok(PathBuf::from(runtime_dir).join(DEFAULT_SOCKET_SUFFIX));
        }
    }

    if allow_test_fallback {
        #[cfg(test)]
        {
            return Ok(env::temp_dir().join("slopos-i-vision-client-test.sock"));
        }
        #[cfg(not(test))]
        {
            unreachable!("the test socket fallback is only available to unit tests");
        }
    }

    Err(VisionClientError::MissingRuntimeDirectory)
}

/// Transport and resource limits for a blocking [`VisionClient`].
#[derive(Clone, Debug)]
pub struct VisionClientConfig {
    /// Filesystem path of the daemon's Unix socket.
    pub socket_path: PathBuf,
    /// Maximum compact JSON request size, excluding `\n`.
    pub max_request_bytes: usize,
    /// Maximum compact JSON response size, excluding `\n`.
    pub max_response_bytes: usize,
    /// Deadline for connecting to the daemon.
    pub connect_timeout: Duration,
    /// Socket write timeout for a request frame.
    pub write_timeout: Duration,
    /// Socket read timeout for a response frame.
    pub read_timeout: Duration,
}

impl VisionClientConfig {
    /// Build the default configuration using the environment socket rules.
    pub fn from_environment() -> Result<Self, VisionClientError> {
        Ok(Self::for_socket(socket_path_from_environment()?))
    }

    /// Build a configuration with the standard limits and timeouts for `path`.
    pub fn for_socket(path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: path.into(),
            max_request_bytes: DEFAULT_MAX_REQUEST_BYTES,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            write_timeout: DEFAULT_WRITE_TIMEOUT,
            read_timeout: DEFAULT_READ_TIMEOUT,
        }
    }

    fn validate(&self) -> Result<(), VisionClientError> {
        if self.socket_path.as_os_str().is_empty() {
            return Err(VisionClientError::InvalidConfiguration(
                "socket path must not be empty",
            ));
        }
        validate_frame_limit(self.max_request_bytes, "request")?;
        validate_frame_limit(self.max_response_bytes, "response")?;
        if self.connect_timeout.is_zero() {
            return Err(VisionClientError::InvalidConfiguration(
                "connect timeout must be non-zero",
            ));
        }
        if self.write_timeout.is_zero() {
            return Err(VisionClientError::InvalidConfiguration(
                "write timeout must be non-zero",
            ));
        }
        if self.read_timeout.is_zero() {
            return Err(VisionClientError::InvalidConfiguration(
                "read timeout must be non-zero",
            ));
        }
        Ok(())
    }
}

/// Errors returned by the Vision client transport and typed API.
#[derive(Debug, Error)]
pub enum VisionClientError {
    #[error("Vision socket path is unavailable: set {VISION_SOCKET_ENV} or {XDG_RUNTIME_DIR_ENV}")]
    MissingRuntimeDirectory,

    #[error("invalid Vision socket path from {0}")]
    InvalidSocketPath(String),

    #[error("invalid Vision client configuration: {0}")]
    InvalidConfiguration(&'static str),

    #[error("request JSON is {actual} bytes, exceeding the {limit}-byte limit")]
    RequestTooLarge { actual: usize, limit: usize },

    #[error("response JSON is {actual} bytes, exceeding the {limit}-byte limit")]
    ResponseTooLarge { actual: usize, limit: usize },

    #[error("failed to serialize Vision request: {source}")]
    SerializeRequest {
        #[source]
        source: JsonError,
    },

    #[error("failed to decode Vision response JSON: {source}")]
    DeserializeResponse {
        #[source]
        source: JsonError,
    },

    #[error("Vision response ended before a complete line was received")]
    UnexpectedEof,

    #[error("Vision response is missing its newline delimiter")]
    MissingResponseDelimiter,

    #[error("Vision protocol version mismatch: expected {expected}, received {received}")]
    ProtocolVersionMismatch { expected: u32, received: u32 },

    #[error("Vision daemon returned {error:?}")]
    Remote {
        error: slopos_vision_protocol::VisionError,
    },

    #[error("unexpected Vision response: expected {expected}, received {actual}")]
    UnexpectedResponse {
        expected: &'static str,
        actual: &'static str,
    },

    #[error("timed out while {operation}")]
    Timeout { operation: &'static str },

    #[error("I/O error while {operation}: {source}")]
    Io {
        operation: &'static str,
        #[source]
        source: io::Error,
    },
}

/// A blocking client for the line-delimited local Vision protocol.
///
/// Each call opens one Unix stream, sends exactly one
/// `ProtocolEnvelope<VisionRequest>`, reads exactly one response line, and
/// closes the stream. The client never performs inference; that remains the
/// daemon's responsibility.
#[derive(Clone, Debug)]
pub struct VisionClient {
    config: VisionClientConfig,
}

impl VisionClient {
    /// Construct a client using the environment-based socket selection.
    pub fn new() -> Result<Self, VisionClientError> {
        Self::with_config(VisionClientConfig::from_environment()?)
    }

    /// Construct a client for an explicit socket path with default limits.
    pub fn from_socket(path: impl Into<PathBuf>) -> Result<Self, VisionClientError> {
        Self::with_config(VisionClientConfig::for_socket(path))
    }

    /// Construct a client with explicit transport limits and timeouts.
    pub fn with_config(config: VisionClientConfig) -> Result<Self, VisionClientError> {
        config.validate()?;
        Ok(Self { config })
    }

    /// Return the immutable transport configuration used by this client.
    pub fn config(&self) -> &VisionClientConfig {
        &self.config
    }

    /// Send a raw typed request and return the raw typed response.
    ///
    /// A daemon-side application error is returned as
    /// `Ok(VisionResponse::Error(_))`; the convenience methods below convert
    /// that protocol response into [`VisionClientError::Remote`].
    pub fn request(&self, request: VisionRequest) -> Result<VisionResponse, VisionClientError> {
        let frame = encode_request_frame(request, self.config.max_request_bytes)?;
        let mut stream = self.connect()?;

        stream
            .write_all(&frame)
            .map_err(|source| map_io_error("writing the Vision request", source))?;
        stream
            .flush()
            .map_err(|source| map_io_error("flushing the Vision request", source))?;

        read_response(&mut stream, self.config.max_response_bytes)
    }

    /// Probe daemon capabilities.
    pub fn probe(&self) -> Result<CapabilityResponse, VisionClientError> {
        match self.request(VisionRequest::Probe(ProbeRequest))? {
            VisionResponse::Capabilities(response) => Ok(response),
            response => Err(response_error("capabilities", response)),
        }
    }

    /// Submit an OCR or subject-lifting job.
    pub fn submit(&self, request: SubmitJobRequest) -> Result<AcceptedResponse, VisionClientError> {
        match self.request(VisionRequest::SubmitJob(request))? {
            VisionResponse::Accepted(response) => Ok(response),
            response => Err(response_error("accepted", response)),
        }
    }

    /// Retrieve the current status of a submitted job.
    pub fn get_status(&self, job_id: JobId) -> Result<JobStatusResponse, VisionClientError> {
        match self.request(VisionRequest::GetJobStatus(JobLookupRequest { job_id }))? {
            VisionResponse::JobStatus(response) => Ok(response),
            response => Err(response_error("job status", response)),
        }
    }

    /// Retrieve a completed job's result or failure details.
    pub fn get_result(&self, job_id: JobId) -> Result<JobResultResponse, VisionClientError> {
        match self.request(VisionRequest::GetJobResult(JobLookupRequest { job_id }))? {
            VisionResponse::JobResult(response) => Ok(response),
            response => Err(response_error("job result", response)),
        }
    }

    /// Request cancellation and return the daemon's resulting job status.
    pub fn cancel(&self, job_id: JobId) -> Result<JobStatusResponse, VisionClientError> {
        match self.request(VisionRequest::CancelJob(JobLookupRequest { job_id }))? {
            VisionResponse::JobStatus(response) => Ok(response),
            response => Err(response_error("job status after cancellation", response)),
        }
    }

    /// Retrieve the bytes and metadata for a daemon-managed asset.
    pub fn get_asset(&self, asset_id: AssetId) -> Result<AssetDataResponse, VisionClientError> {
        match self.request(VisionRequest::GetAsset(AssetLookupRequest { asset_id }))? {
            VisionResponse::Asset(response) => Ok(response),
            response => Err(response_error("asset", response)),
        }
    }

    fn connect(&self) -> Result<UnixStream, VisionClientError> {
        let socket = Socket::new(Domain::UNIX, Type::STREAM, None)
            .map_err(|source| map_io_error("creating the Vision socket", source))?;
        let address = SockAddr::unix(Path::new(&self.config.socket_path)).map_err(|source| {
            VisionClientError::InvalidSocketPath(format!(
                "{} ({source})",
                self.config.socket_path.display()
            ))
        })?;

        socket
            .connect_timeout(&address, self.config.connect_timeout)
            .map_err(|source| map_io_error("connecting to the Vision daemon", source))?;

        let owned_fd: OwnedFd = socket.into();
        let stream = UnixStream::from(owned_fd);
        stream
            .set_write_timeout(Some(self.config.write_timeout))
            .map_err(|source| map_io_error("configuring the Vision write timeout", source))?;
        stream
            .set_read_timeout(Some(self.config.read_timeout))
            .map_err(|source| map_io_error("configuring the Vision read timeout", source))?;
        Ok(stream)
    }
}

fn encode_request_frame(
    request: VisionRequest,
    max_bytes: usize,
) -> Result<Vec<u8>, VisionClientError> {
    validate_frame_limit(max_bytes, "request")?;
    let envelope = ProtocolEnvelope::new(request);
    let mut frame = serde_json::to_vec(&envelope)
        .map_err(|source| VisionClientError::SerializeRequest { source })?;
    if frame.len() > max_bytes {
        return Err(VisionClientError::RequestTooLarge {
            actual: frame.len(),
            limit: max_bytes,
        });
    }
    frame.push(b'\n');
    Ok(frame)
}

fn read_response(
    stream: &mut UnixStream,
    max_bytes: usize,
) -> Result<VisionResponse, VisionClientError> {
    validate_frame_limit(max_bytes, "response")?;
    let read_limit = max_bytes
        .checked_add(2)
        .ok_or(VisionClientError::InvalidConfiguration(
            "response size limit is too large",
        ))?;
    let mut reader = BufReader::new(stream);
    let mut frame = Vec::new();
    let bytes_read = reader
        .by_ref()
        .take(read_limit as u64)
        .read_until(b'\n', &mut frame)
        .map_err(|source| map_io_error("reading the Vision response", source))?;
    if bytes_read == 0 {
        return Err(VisionClientError::UnexpectedEof);
    }
    decode_response_frame(&frame, max_bytes)
}

fn decode_response_frame(
    frame: &[u8],
    max_bytes: usize,
) -> Result<VisionResponse, VisionClientError> {
    validate_frame_limit(max_bytes, "response")?;
    if frame.is_empty() {
        return Err(VisionClientError::UnexpectedEof);
    }

    let json = if frame.last() == Some(&b'\n') {
        let without_newline = &frame[..frame.len() - 1];
        if without_newline.last() == Some(&b'\r') {
            &without_newline[..without_newline.len() - 1]
        } else {
            without_newline
        }
    } else {
        return if frame.len() > max_bytes {
            Err(VisionClientError::ResponseTooLarge {
                actual: frame.len(),
                limit: max_bytes,
            })
        } else {
            Err(VisionClientError::MissingResponseDelimiter)
        };
    };

    if json.len() > max_bytes {
        return Err(VisionClientError::ResponseTooLarge {
            actual: json.len(),
            limit: max_bytes,
        });
    }

    let envelope: ProtocolEnvelope<VisionResponse> = serde_json::from_slice(json)
        .map_err(|source| VisionClientError::DeserializeResponse { source })?;
    if envelope.protocol_version != VISION_PROTOCOL_VERSION {
        return Err(VisionClientError::ProtocolVersionMismatch {
            expected: VISION_PROTOCOL_VERSION,
            received: envelope.protocol_version,
        });
    }
    Ok(envelope.payload)
}

fn validate_frame_limit(limit: usize, kind: &'static str) -> Result<(), VisionClientError> {
    if limit == 0 {
        return Err(VisionClientError::InvalidConfiguration(match kind {
            "request" => "request size limit must be non-zero",
            "response" => "response size limit must be non-zero",
            _ => "frame size limit must be non-zero",
        }));
    }
    if limit > MAX_CONFIGURED_FRAME_BYTES {
        return Err(VisionClientError::InvalidConfiguration(match kind {
            "request" => "request size limit exceeds the hard maximum",
            "response" => "response size limit exceeds the hard maximum",
            _ => "frame size limit exceeds the hard maximum",
        }));
    }
    Ok(())
}

fn map_io_error(operation: &'static str, source: io::Error) -> VisionClientError {
    match source.kind() {
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => {
            VisionClientError::Timeout { operation }
        }
        _ => VisionClientError::Io { operation, source },
    }
}

fn response_error(expected: &'static str, response: VisionResponse) -> VisionClientError {
    match response {
        VisionResponse::Error(error) => VisionClientError::Remote { error },
        other => VisionClientError::UnexpectedResponse {
            expected,
            actual: response_kind(&other),
        },
    }
}

fn response_kind(response: &VisionResponse) -> &'static str {
    match response {
        VisionResponse::Accepted(_) => "accepted",
        VisionResponse::JobStatus(_) => "job status",
        VisionResponse::JobResult(_) => "job result",
        VisionResponse::Asset(_) => "asset",
        VisionResponse::Capabilities(_) => "capabilities",
        VisionResponse::Error(_) => "error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slopos_vision_protocol::{
        ErrorCode, ExecutionMode, ModelProvisioning, VisionError, VisionOperation,
    };

    fn capabilities_response() -> VisionResponse {
        VisionResponse::Capabilities(CapabilityResponse {
            execution_mode: ExecutionMode::LocalOnly,
            model_provisioning: ModelProvisioning::ImportedModelPackOnly,
            supported_operations: vec![VisionOperation::ExtractText, VisionOperation::LiftSubject],
        })
    }

    #[test]
    fn explicit_socket_path_takes_precedence() {
        let path = resolve_socket_path(
            Some(OsStr::new("/run/user/1000/custom-vision.sock")),
            Some(OsStr::new("/run/user/1000")),
            false,
        )
        .unwrap();
        assert_eq!(path, PathBuf::from("/run/user/1000/custom-vision.sock"));
    }

    #[test]
    fn runtime_directory_derives_default_socket_path() {
        let path = resolve_socket_path(None, Some(OsStr::new("/run/user/1000")), false).unwrap();
        assert_eq!(path, PathBuf::from("/run/user/1000/slopos-i/vision.sock"));
    }

    #[test]
    fn temporary_fallback_is_only_available_when_enabled_for_tests() {
        assert!(matches!(
            resolve_socket_path(None, None, false),
            Err(VisionClientError::MissingRuntimeDirectory)
        ));
        let fallback = resolve_socket_path(None, None, true).unwrap();
        assert_eq!(
            fallback,
            env::temp_dir().join("slopos-i-vision-client-test.sock")
        );
    }

    #[test]
    fn request_envelope_round_trips_as_line_delimited_json() {
        let request = VisionRequest::Probe(ProbeRequest);
        let frame = encode_request_frame(request.clone(), 1024).unwrap();
        assert_eq!(frame.last(), Some(&b'\n'));

        let envelope: ProtocolEnvelope<VisionRequest> =
            serde_json::from_slice(&frame[..frame.len() - 1]).unwrap();
        assert_eq!(envelope, ProtocolEnvelope::new(request));
    }

    #[test]
    fn response_envelope_round_trips_and_validates_version() {
        let mut frame =
            serde_json::to_vec(&ProtocolEnvelope::new(capabilities_response())).unwrap();
        frame.push(b'\n');
        assert_eq!(
            decode_response_frame(&frame, 1024).unwrap(),
            capabilities_response()
        );

        let wrong_version = ProtocolEnvelope {
            protocol_version: VISION_PROTOCOL_VERSION + 1,
            payload: capabilities_response(),
        };
        let wrong_frame = serde_json::to_vec(&wrong_version).unwrap();
        let mut wrong_frame_with_delimiter = wrong_frame;
        wrong_frame_with_delimiter.push(b'\n');
        assert!(matches!(
            decode_response_frame(&wrong_frame_with_delimiter, 1024),
            Err(VisionClientError::ProtocolVersionMismatch {
                expected: VISION_PROTOCOL_VERSION,
                received
            }) if received == VISION_PROTOCOL_VERSION + 1
        ));
    }

    #[test]
    fn malformed_response_is_reported_as_decode_error() {
        assert!(matches!(
            decode_response_frame(b"not-json\n", 1024),
            Err(VisionClientError::DeserializeResponse { .. })
        ));
        assert!(matches!(
            decode_response_frame(b"{}", 1024),
            Err(VisionClientError::MissingResponseDelimiter)
        ));
    }

    #[test]
    fn oversized_response_is_rejected_before_json_decode() {
        let oversized = vec![b'x'; 9];
        assert!(matches!(
            decode_response_frame(&oversized, 8),
            Err(VisionClientError::ResponseTooLarge {
                actual: 9,
                limit: 8
            })
        ));

        let oversized_with_delimiter = vec![b'x'; 9]
            .into_iter()
            .chain(std::iter::once(b'\n'))
            .collect::<Vec<_>>();
        assert!(matches!(
            decode_response_frame(&oversized_with_delimiter, 8),
            Err(VisionClientError::ResponseTooLarge {
                actual: 9,
                limit: 8
            })
        ));
    }

    #[test]
    fn daemon_error_becomes_a_typed_client_error_for_convenience_methods() {
        let daemon_error = VisionError {
            code: ErrorCode::ModelUnavailable,
            message: "OCR model is not installed".into(),
            operation: Some(VisionOperation::ExtractText),
            retryable: false,
        };
        let error = response_error("capabilities", VisionResponse::Error(daemon_error.clone()));
        assert!(matches!(
            error,
            VisionClientError::Remote { error: actual } if actual == daemon_error
        ));
    }

    #[test]
    fn client_rejects_zero_and_unbounded_limits() {
        let mut config = VisionClientConfig::for_socket("/tmp/vision.sock");
        config.max_response_bytes = 0;
        assert!(matches!(
            VisionClient::with_config(config),
            Err(VisionClientError::InvalidConfiguration(_))
        ));

        let mut config = VisionClientConfig::for_socket("/tmp/vision.sock");
        config.max_request_bytes = MAX_CONFIGURED_FRAME_BYTES + 1;
        assert!(matches!(
            VisionClient::with_config(config),
            Err(VisionClientError::InvalidConfiguration(_))
        ));
    }
}
