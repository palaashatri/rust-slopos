//! SLOPOS-I session supervisor.
//!
//! Copyright (c) 2026 Palaash Atri
//! SPDX-License-Identifier: MIT
//!
//! This process is the stable parent for the compositor and shell.  It keeps
//! host and private Wayland sockets separate, waits for compositor readiness,
//! and tears down the entire client process group if either critical process
//! exits.

use std::env;
use std::fs;
use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const SESSION_ROOT: &str = "slopos-i";
const READINESS_FILE: &str = "readiness";
const CLIENT_DISPLAY_FILE: &str = "client-wayland-display";
const TOKEN_FILE: &str = "token";
const VISION_SOCKET_ENV: &str = "SLOPOS_VISION_SOCKET";
const VISION_MODELS_ENV: &str = "SLOPOS_VISION_MODELS_DIR";
const VISION_ARTIFACT_ENV: &str = "SLOPOS_VISION_ARTIFACT_DIR";
const DEFAULT_STARTUP_TIMEOUT: Duration = Duration::from_secs(12);
const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);

static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);
static SESSION_NONCE_COUNTER: AtomicU64 = AtomicU64::new(0);

extern "C" fn handle_shutdown_signal(_signal: libc::c_int) {
    SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
}

fn install_signal_handlers() -> Result<(), String> {
    // The handler only flips an AtomicBool, which is async-signal-safe. The
    // supervisor performs all process-group teardown back in its normal loop.
    for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
        let previous = unsafe {
            libc::signal(
                signal,
                handle_shutdown_signal as *const () as libc::sighandler_t,
            )
        };
        if previous == libc::SIG_ERR {
            return Err(format!("cannot install signal handler for signal {signal}"));
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Backend {
    Drm,
    Nested,
    Headless,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PrivateDisplay {
    socket_name: String,
    output_width: Option<u32>,
    output_height: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct OwnedProcessGroup {
    id: libc::pid_t,
}

impl OwnedProcessGroup {
    fn for_child_pid(pid: u32) -> Result<Self, String> {
        let id = libc::pid_t::try_from(pid)
            .map_err(|_| format!("child PID {pid} does not fit in pid_t"))?;
        if id <= 1 {
            return Err(format!("refusing to own unsafe process-group id {id}"));
        }
        Ok(Self { id })
    }

    fn signal_target(self) -> libc::pid_t {
        -self.id
    }
}

struct OwnedChild {
    child: Child,
    process_group: OwnedProcessGroup,
    role: &'static str,
}

impl OwnedChild {
    fn spawn(command: &mut Command, role: &'static str) -> Result<Self, String> {
        let child = command
            .spawn()
            .map_err(|error| format!("cannot start {role}: {error}"))?;
        let process_group = match OwnedProcessGroup::for_child_pid(child.id()) {
            Ok(group) => group,
            Err(error) => {
                let mut child = Self {
                    process_group: OwnedProcessGroup { id: 0 },
                    child,
                    role,
                };
                let _ = child.child.kill();
                let _ = child.child.wait();
                return Err(error);
            }
        };
        eprintln!(
            "[slopos-session] started {} pid={} pgid={}",
            role,
            child.id(),
            process_group.id
        );
        Ok(Self {
            child,
            process_group,
            role,
        })
    }

    fn id(&self) -> u32 {
        self.child.id()
    }

    fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LifecycleState {
    Starting,
    Running,
    Stopping,
    Succeeded,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LifecycleEvent {
    CompositorReady,
    ShutdownRequested,
    CompositorExited,
    ShellExited { success: bool },
    StartupTimeout,
    StartupFailed,
    ChildrenReaped,
}

impl LifecycleState {
    fn transition(self, event: LifecycleEvent) -> Self {
        match (self, event) {
            (Self::Starting, LifecycleEvent::CompositorReady) => Self::Running,
            (Self::Starting, LifecycleEvent::ShutdownRequested) => Self::Stopping,
            (Self::Starting, LifecycleEvent::CompositorExited)
            | (Self::Starting, LifecycleEvent::StartupTimeout)
            | (Self::Starting, LifecycleEvent::StartupFailed) => Self::Failed,
            (Self::Running, LifecycleEvent::ShutdownRequested)
            | (Self::Running, LifecycleEvent::ShellExited { success: true }) => Self::Stopping,
            (Self::Running, LifecycleEvent::CompositorExited)
            | (Self::Running, LifecycleEvent::ShellExited { success: false }) => Self::Failed,
            (Self::Stopping, LifecycleEvent::ChildrenReaped) => Self::Succeeded,
            (Self::Stopping, LifecycleEvent::CompositorExited) => Self::Stopping,
            (state, _) => state,
        }
    }
}

struct VisionService {
    child: OwnedChild,
    socket_path: PathBuf,
}

impl Backend {
    fn cli_value(self) -> &'static str {
        match self {
            Self::Drm => "drm",
            Self::Nested => "nested",
            Self::Headless => "headless",
        }
    }
}

fn default_backend_for_host(display: Option<&str>, _wayland_display: Option<&str>) -> Backend {
    // The nested implementation is Smithay's X11 backend. A host Wayland
    // socket is not an X11 transport and must never select this path.
    if display.is_some_and(|value| !value.is_empty()) {
        Backend::Nested
    } else {
        Backend::Drm
    }
}

fn validate_backend_transport(backend: Backend, display: Option<&str>) -> Result<(), String> {
    if backend == Backend::Nested && display.is_none_or(|value| value.is_empty()) {
        return Err(
            "nested backend requires a non-empty DISPLAY (nested transport is X11-only); use --backend drm or --backend headless"
                .to_owned(),
        );
    }
    Ok(())
}

fn parse_backend() -> Result<Backend, String> {
    let mut value: Option<String> = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--backend" {
            value = args.next();
        } else if let Some(v) = arg.strip_prefix("--backend=") {
            value = Some(v.to_owned());
        } else if arg == "--help" || arg == "-h" {
            println!("Usage: slopos-session [--backend drm|nested|x11|headless]");
            std::process::exit(0);
        } else {
            return Err(format!("unknown argument: {arg}"));
        }
    }

    match value.as_deref() {
        Some("drm") => Ok(Backend::Drm),
        Some("nested") | Some("x11") | Some("winit") => Ok(Backend::Nested),
        Some("headless") => Ok(Backend::Headless),
        Some(other) => Err(format!("unsupported backend '{other}'")),
        None => Ok(default_backend_for_host(
            env::var("DISPLAY").ok().as_deref(),
            env::var("WAYLAND_DISPLAY").ok().as_deref(),
        )),
    }
}

fn runtime_dir() -> Result<PathBuf, String> {
    let path = env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", unsafe { libc::geteuid() })));
    validate_runtime_dir(&path)?;
    Ok(path)
}

fn validate_runtime_dir(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect XDG_RUNTIME_DIR {}: {error}", path.display()))?;
    if !metadata.file_type().is_dir() {
        return Err(format!(
            "XDG_RUNTIME_DIR {} is not a directory",
            path.display()
        ));
    }

    let effective_uid = unsafe { libc::geteuid() };
    if metadata.uid() != effective_uid {
        return Err(format!(
            "XDG_RUNTIME_DIR {} is owned by uid {}, expected {}",
            path.display(),
            metadata.uid(),
            effective_uid
        ));
    }

    let mode = metadata.mode() & 0o777;
    if mode != 0o700 {
        return Err(format!(
            "XDG_RUNTIME_DIR {} must have mode 0700, found {:o}",
            path.display(),
            mode
        ));
    }
    Ok(())
}

struct SessionRuntime {
    path: PathBuf,
    identity: Option<DirectoryIdentity>,
    /// Open descriptor for an owned directory. Keeping the original directory
    /// inode referenced prevents the filesystem from immediately reusing that
    /// inode after an attacker or test replaces the pathname.
    identity_handle: Option<fs::File>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DirectoryIdentity {
    device: u64,
    inode: u64,
}

fn directory_identity_from_metadata(
    metadata: &fs::Metadata,
    display_path: &Path,
) -> Result<DirectoryIdentity, String> {
    if !metadata.file_type().is_dir() {
        return Err(format!(
            "runtime path {} is not a directory",
            display_path.display()
        ));
    }
    Ok(DirectoryIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

fn directory_identity(path: &Path) -> Result<DirectoryIdentity, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        format!(
            "cannot inspect runtime directory {}: {error}",
            path.display()
        )
    })?;
    directory_identity_from_metadata(&metadata, path)
}

impl SessionRuntime {
    fn owned(path: PathBuf) -> Result<Self, String> {
        let identity_handle = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&path)
            .map_err(|error| {
                format!(
                    "cannot hold runtime directory {} open: {error}",
                    path.display()
                )
            })?;
        let identity = directory_identity_from_metadata(
            &identity_handle.metadata().map_err(|error| {
                format!(
                    "cannot inspect held runtime directory {}: {error}",
                    path.display()
                )
            })?,
            &path,
        )?;
        Ok(Self {
            path,
            identity: Some(identity),
            identity_handle: Some(identity_handle),
        })
    }

    #[cfg(test)]
    fn unowned(path: PathBuf) -> Self {
        Self {
            path,
            identity: None,
            identity_handle: None,
        }
    }

    fn still_owns_path(&self) -> bool {
        let (Some(expected), Some(handle)) = (self.identity, self.identity_handle.as_ref()) else {
            return false;
        };
        let held_identity = handle
            .metadata()
            .ok()
            .and_then(|metadata| directory_identity_from_metadata(&metadata, &self.path).ok());
        held_identity == Some(expected) && directory_identity(&self.path).ok() == Some(expected)
    }
}

impl Drop for SessionRuntime {
    fn drop(&mut self) {
        // This directory was created by this supervisor. Re-check its inode
        // before recursive cleanup so a replaced path, symlink, or borrowed
        // test handle can never make the supervisor remove another directory.
        if self.still_owns_path() {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn private_dir(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.file_type().is_dir() {
                return Err(format!(
                    "runtime path {} is not a directory",
                    path.display()
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(path).map_err(|error| {
                format!(
                    "cannot create runtime directory {}: {error}",
                    path.display()
                )
            })?;
        }
        Err(error) => {
            return Err(format!(
                "cannot inspect runtime directory {}: {error}",
                path.display()
            ));
        }
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|e| format!("cannot restrict runtime directory {}: {e}", path.display()))
}

fn session_nonce() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = SESSION_NONCE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:x}-{:x}-{counter:x}", std::process::id())
}

fn write_private_file(path: &Path, contents: &str) -> Result<(), String> {
    fs::write(path, contents).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("cannot restrict {}: {e}", path.display()))
}

fn create_session_runtime(base: &Path) -> Result<(SessionRuntime, String), String> {
    let root = base.join(SESSION_ROOT);
    private_dir(&root)?;

    for attempt in 0..8u8 {
        let nonce = session_nonce();
        let suffix = if attempt == 0 {
            String::new()
        } else {
            format!("-{attempt}")
        };
        let path = root.join(format!("session-{nonce}{suffix}"));
        match fs::create_dir(&path) {
            Ok(()) => {
                let runtime = SessionRuntime::owned(path.clone())?;
                if let Err(error) = private_dir(&path) {
                    drop(runtime);
                    return Err(error);
                }
                let logs = path.join("logs");
                if let Err(error) = private_dir(&logs) {
                    drop(runtime);
                    return Err(error);
                }
                let token = format!("{nonce}-{}", session_nonce());
                if let Err(error) = write_private_file(&path.join(TOKEN_FILE), &token) {
                    drop(runtime);
                    return Err(error);
                }
                return Ok((runtime, token));
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "cannot create private session directory {}: {error}",
                    path.display()
                ));
            }
        }
    }

    Err("could not allocate a unique SLOPOS-I session directory".to_string())
}

fn sibling_or_path(name: &str) -> Result<PathBuf, String> {
    if let Ok(exe) = env::current_exe() {
        if let Some(parent) = exe.parent() {
            let candidate = parent.join(name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    let path = env::var_os("PATH").unwrap_or_default();
    for dir in env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    let repo_candidates = [
        PathBuf::from(format!("target/release/{name}")),
        PathBuf::from(format!("target/debug/{name}")),
    ];
    for candidate in repo_candidates {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    Err(format!("required binary '{name}' was not found"))
}

fn configured_vision_models_dir_from_path(path: &Path) -> Result<PathBuf, String> {
    let path = fs::canonicalize(path)
        .map_err(|error| format!("cannot access the configured Vision model directory: {error}"))?;
    if !path.is_dir() {
        return Err("configured Vision model path is not a directory".to_string());
    }
    if !path.join("manifest.toml").is_file() {
        return Err(format!(
            "configured Vision model directory {} has no manifest.toml",
            path.display()
        ));
    }
    Ok(path)
}

fn configured_vision_models_dir() -> Result<PathBuf, String> {
    let value =
        env::var_os(VISION_MODELS_ENV).ok_or_else(|| format!("{VISION_MODELS_ENV} is not set"))?;
    if value.is_empty() {
        return Err(format!("{VISION_MODELS_ENV} is empty"));
    }
    configured_vision_models_dir_from_path(&PathBuf::from(value))
}

fn maybe_spawn_visiond(runtime: &Path, token: &str) -> Option<VisionService> {
    let visiond_bin = match sibling_or_path("slopos-visiond") {
        Ok(path) => path,
        Err(error) => {
            eprintln!("[slopos-session] Vision daemon not started: {error}");
            return None;
        }
    };
    let models_dir = match configured_vision_models_dir() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("[slopos-session] Vision daemon not started: {error}");
            return None;
        }
    };

    let socket_path = runtime.join("vision.sock");
    let artifact_dir = runtime.join("vision-artifacts");
    let mut command = child_command(&visiond_bin);
    command
        .env("XDG_RUNTIME_DIR", runtime)
        .env("SLOPOS_SESSION_RUNTIME_DIR", runtime)
        .env("SLOPOS_SESSION_TOKEN", token)
        .env(VISION_SOCKET_ENV, &socket_path)
        .env(VISION_MODELS_ENV, &models_dir)
        .env(VISION_ARTIFACT_ENV, &artifact_dir)
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("SLOPOS_HOST_WAYLAND_DISPLAY");

    match OwnedChild::spawn(&mut command, "slopos-visiond") {
        Ok(child) => {
            eprintln!(
                "[slopos-session] visiond pid={} socket={} models={}",
                child.id(),
                socket_path.display(),
                models_dir.display()
            );
            Some(VisionService { child, socket_path })
        }
        Err(error) => {
            eprintln!(
                "[slopos-session] Vision daemon not started from {}: {error}",
                visiond_bin.display()
            );
            None
        }
    }
}

fn apply_vision_client_environment(command: &mut Command, service: Option<&VisionService>) {
    if let Some(service) = service {
        command.env(VISION_SOCKET_ENV, &service.socket_path);
    } else {
        // Do not leak a caller-provided socket into a session that did not
        // start its own Vision service.
        command.env_remove(VISION_SOCKET_ENV);
    }
}

fn read_timeout_from_env() -> Duration {
    env::var("SLOPOS_COMPOSITOR_WAIT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_secs)
        .filter(|d| !d.is_zero())
        .unwrap_or(DEFAULT_STARTUP_TIMEOUT)
}

#[derive(Debug, Eq, PartialEq)]
struct Readiness {
    socket_name: String,
    pid: u32,
    token: String,
    output_width: Option<u32>,
    output_height: Option<u32>,
}

fn valid_socket_name(socket_name: &str) -> bool {
    let suffix = socket_name.strip_prefix("wayland-");
    suffix.is_some_and(|suffix| {
        !suffix.is_empty() && suffix.chars().all(|character| character.is_ascii_digit())
    })
}

fn parse_readiness(value: &str) -> Result<Readiness, String> {
    let mut lines = value.lines();
    let socket_name = lines
        .next()
        .map(str::trim)
        .filter(|value| valid_socket_name(value))
        .ok_or_else(|| "readiness has an invalid private Wayland socket name".to_string())?
        .to_owned();
    let mut pid = None;
    let mut token = None;
    let mut output_width = None;
    let mut output_height = None;

    for line in lines {
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| "readiness contains a malformed key/value line".to_string())?;
        let key = key.trim();
        let value = value.trim();
        match key {
            "pid" if pid.is_none() => {
                pid = Some(
                    value
                        .parse::<u32>()
                        .map_err(|_| "readiness contains an invalid compositor PID".to_string())?,
                );
            }
            "token" if token.is_none() => {
                if value.is_empty() {
                    return Err("readiness contains an empty session token".to_string());
                }
                token = Some(value.to_owned());
            }
            "width" if output_width.is_none() => {
                output_width = Some(
                    value
                        .parse::<u32>()
                        .map_err(|_| "readiness contains an invalid output width".to_string())?,
                );
            }
            "height" if output_height.is_none() => {
                output_height = Some(
                    value
                        .parse::<u32>()
                        .map_err(|_| "readiness contains an invalid output height".to_string())?,
                );
            }
            "pid" | "token" | "width" | "height" => {
                return Err(format!("readiness contains a duplicate {key} field"));
            }
            _ => return Err(format!("readiness contains an unknown field '{key}'")),
        }
    }

    let pid = pid.ok_or_else(|| "readiness does not identify the compositor PID".to_string())?;
    let token = token.ok_or_else(|| "readiness does not identify the session token".to_string())?;
    let output_width = output_width.filter(|value| *value > 0);
    let output_height = output_height.filter(|value| *value > 0);

    Ok(Readiness {
        socket_name,
        pid,
        token,
        output_width,
        output_height,
    })
}

fn validate_readiness(
    value: &str,
    expected_pid: u32,
    expected_token: &str,
    client_display: &str,
) -> Result<PrivateDisplay, String> {
    let readiness = parse_readiness(value)?;
    if readiness.pid != expected_pid {
        return Err(format!(
            "readiness belongs to compositor pid {}, expected {}",
            readiness.pid, expected_pid
        ));
    }
    if readiness.token != expected_token {
        return Err("readiness session token does not match this session".to_string());
    }
    if client_display.trim() != readiness.socket_name {
        return Err("client Wayland display does not match readiness".to_string());
    }
    Ok(PrivateDisplay {
        socket_name: readiness.socket_name,
        output_width: readiness.output_width,
        output_height: readiness.output_height,
    })
}

fn read_private_file(path: &Path) -> Result<Option<String>, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "cannot inspect session file {}: {error}",
                path.display()
            ));
        }
    };
    if !metadata.file_type().is_file() {
        return Err(format!(
            "session file {} is not a regular file",
            path.display()
        ));
    }
    fs::read_to_string(path)
        .map(Some)
        .map_err(|error| format!("cannot read session file {}: {error}", path.display()))
}

fn wait_for_private_socket(
    child: &mut OwnedChild,
    session_runtime: &SessionRuntime,
    token: &str,
    timeout: Duration,
) -> Result<PrivateDisplay, String> {
    if !session_runtime.still_owns_path() {
        return Err("session runtime ownership was lost before compositor startup".to_string());
    }
    let runtime = &session_runtime.path;
    let token_path = runtime.join(TOKEN_FILE);
    match read_private_file(&token_path)? {
        Some(value) if value.trim() == token => {}
        Some(_) => return Err("session token file does not match this session".to_string()),
        None => {
            return Err(format!(
                "session token file {} is missing",
                token_path.display()
            ))
        }
    }

    let readiness = runtime.join(READINESS_FILE);
    let client_display = runtime.join(CLIENT_DISPLAY_FILE);
    let deadline = Instant::now() + timeout;
    let mut last_handshake_error = None;
    loop {
        if SHUTDOWN_REQUESTED.load(Ordering::SeqCst) {
            return Err("shutdown requested during compositor startup".to_string());
        }
        if !session_runtime.still_owns_path() {
            return Err("session runtime ownership was lost during compositor startup".to_string());
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("cannot inspect compositor process: {error}"))?
        {
            return Err(format!("slopos-compositor exited during startup: {status}"));
        }

        if Instant::now() >= deadline {
            let detail = last_handshake_error
                .as_deref()
                .map(|error| format!("; last handshake error: {error}"))
                .unwrap_or_default();
            return Err(format!(
                "slopos-compositor readiness timed out after {}s{}",
                timeout.as_secs(),
                detail
            ));
        }

        if let Some(readiness_value) = read_private_file(&readiness)? {
            if let Some(client_display_value) = read_private_file(&client_display)? {
                match validate_readiness(&readiness_value, child.id(), token, &client_display_value)
                {
                    Ok(private_display) => {
                        let socket_path = runtime.join(&private_display.socket_name);
                        match fs::symlink_metadata(&socket_path) {
                            Ok(metadata) if metadata.file_type().is_socket() => {
                                return Ok(private_display);
                            }
                            Ok(_) => {
                                last_handshake_error = Some(format!(
                                    "private socket {} is not a Unix socket",
                                    socket_path.display()
                                ));
                            }
                            Err(error) => {
                                last_handshake_error = Some(format!(
                                    "private socket {} is unavailable: {error}",
                                    socket_path.display()
                                ));
                            }
                        }
                    }
                    Err(error) => last_handshake_error = Some(error),
                }
            }
        }

        let remaining = deadline.saturating_duration_since(Instant::now());
        thread::sleep(Duration::from_millis(50).min(remaining));
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProcessGroupSignal {
    Sent,
    Missing,
}

fn signal_owned_group(
    group: OwnedProcessGroup,
    signal: libc::c_int,
) -> Result<ProcessGroupSignal, String> {
    if group.id <= 1 {
        return Err(format!(
            "refusing to signal unsafe process-group id {}",
            group.id
        ));
    }
    let result = unsafe { libc::kill(group.signal_target(), signal) };
    if result == 0 {
        return Ok(ProcessGroupSignal::Sent);
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        Ok(ProcessGroupSignal::Missing)
    } else {
        Err(format!(
            "cannot send signal {signal} to owned process group {}: {error}",
            group.id
        ))
    }
}

fn reap_owned_group(child: &mut OwnedChild) -> Result<ExitStatus, String> {
    reap_owned_group_with_grace(child, SHUTDOWN_GRACE)
}

fn reap_owned_group_with_grace(
    child: &mut OwnedChild,
    grace: Duration,
) -> Result<ExitStatus, String> {
    if let Some(status) = child
        .try_wait()
        .map_err(|error| format!("cannot inspect {}: {error}", child.role))?
    {
        return Ok(status);
    }

    let term_result = signal_owned_group(child.process_group, libc::SIGTERM);
    let deadline = Instant::now() + grace;
    while Instant::now() < deadline {
        match child
            .try_wait()
            .map_err(|error| format!("cannot reap {}: {error}", child.role))?
        {
            Some(status) => return Ok(status),
            None => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                thread::sleep(Duration::from_millis(25).min(remaining));
            }
        }
    }

    let kill_result = signal_owned_group(child.process_group, libc::SIGKILL);
    if matches!(term_result, Err(_) | Ok(ProcessGroupSignal::Missing))
        || matches!(kill_result, Err(_) | Ok(ProcessGroupSignal::Missing))
    {
        // The direct child PID is still owned by this supervisor. It is a
        // safe last resort when a process-group signal found no group or was
        // rejected by the kernel; never broaden this to process-name scans.
        let _ = child.child.kill();
    }
    child
        .child
        .wait()
        .map_err(|error| format!("cannot reap {} after SIGKILL: {error}", child.role))
}

fn terminate_group(child: &mut OwnedChild) {
    match reap_owned_group(child) {
        Ok(status) => eprintln!(
            "[slopos-session] reaped {} pid={} pgid={} status={status}",
            child.role,
            child.id(),
            child.process_group.id
        ),
        Err(error) => eprintln!("[slopos-session] failed to reap {}: {error}", child.role),
    }
}

fn child_command(path: &Path) -> Command {
    let mut command = Command::new(path);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    // A dedicated process group lets the supervisor terminate the shell and
    // every application it launches without globbing process names.
    command.process_group(0);
    command
}

fn stop_visiond(visiond: &mut Option<VisionService>) {
    if let Some(mut service) = visiond.take() {
        terminate_group(&mut service.child);
    }
}

fn startup_failure_event(error: &str) -> LifecycleEvent {
    if error.contains("timed out") {
        LifecycleEvent::StartupTimeout
    } else if error.contains("compositor exited") {
        LifecycleEvent::CompositorExited
    } else {
        LifecycleEvent::StartupFailed
    }
}

fn run() -> Result<(), String> {
    install_signal_handlers()?;
    let backend = parse_backend()?;
    validate_backend_transport(backend, env::var("DISPLAY").ok().as_deref())?;
    let base_runtime = runtime_dir()?;
    let (session_runtime, token) = create_session_runtime(&base_runtime)?;
    let runtime = &session_runtime.path;
    let mut lifecycle = LifecycleState::Starting;

    let compositor_bin = sibling_or_path("slopos-compositor")?;
    let shell_bin = sibling_or_path("slopos-shell")?;
    let mut visiond = maybe_spawn_visiond(runtime, &token);

    let mut compositor_cmd = child_command(&compositor_bin);
    compositor_cmd.arg("--backend").arg(backend.cli_value());
    compositor_cmd.env("XDG_RUNTIME_DIR", runtime);
    compositor_cmd
        .env("SLOPOS_SESSION_RUNTIME_DIR", runtime)
        .env("SLOPOS_SESSION_TOKEN", &token)
        // There is no nested-Wayland transport. Do not pass a host Wayland
        // name that could be mistaken for a connection owned by SLOPOS.
        .env_remove("SLOPOS_HOST_WAYLAND_DISPLAY");
    apply_vision_client_environment(&mut compositor_cmd, visiond.as_ref());
    // The compositor is the only SLOPOS process allowed to inherit the host
    // X11 display in nested mode. Its clients are launched later with an
    // explicit private Wayland socket.
    let mut compositor = match OwnedChild::spawn(&mut compositor_cmd, "slopos-compositor") {
        Ok(child) => child,
        Err(error) => {
            if let Some(service) = visiond.as_mut() {
                terminate_group(&mut service.child);
            }
            return Err(format!(
                "cannot start {}: {error}",
                compositor_bin.display()
            ));
        }
    };

    let private_display = match wait_for_private_socket(
        &mut compositor,
        &session_runtime,
        &token,
        read_timeout_from_env(),
    ) {
        Ok(socket) => socket,
        Err(error) => {
            terminate_group(&mut compositor);
            stop_visiond(&mut visiond);
            lifecycle = lifecycle.transition(startup_failure_event(&error));
            debug_assert_eq!(lifecycle, LifecycleState::Failed);
            return Err(error);
        }
    };
    lifecycle = lifecycle.transition(LifecycleEvent::CompositorReady);

    eprintln!(
        "[slopos-session] compositor pid={} pgid={} backend={} client_socket={}",
        compositor.id(),
        compositor.process_group.id,
        backend.cli_value(),
        private_display.socket_name
    );

    let mut shell_cmd = child_command(&shell_bin);
    shell_cmd
        .env("XDG_RUNTIME_DIR", runtime)
        .env("SLOPOS_SESSION_RUNTIME_DIR", runtime)
        .env("WAYLAND_DISPLAY", &private_display.socket_name)
        .env(
            "SLOPOS_CLIENT_WAYLAND_DISPLAY",
            &private_display.socket_name,
        )
        // Linux production shell chrome is always layer-shell; keep this
        // explicit in the child environment for diagnostics and direct
        // consumers of the session contract.
        .env("SLOPOS_LAYER_SHELL_CHROME", "1")
        .env(
            "SLOPOS_ACTIVE_TOPLEVEL_FILE",
            runtime.join("active-toplevel"),
        )
        .env_remove("SLOPOS_HOST_WAYLAND_DISPLAY");
    apply_vision_client_environment(&mut shell_cmd, visiond.as_ref());
    if let (Some(width), Some(height)) =
        (private_display.output_width, private_display.output_height)
    {
        shell_cmd
            .env("SLOPOS_COMPOSITOR_WIDTH", width.to_string())
            .env("SLOPOS_COMPOSITOR_HEIGHT", height.to_string());
    }
    if env::var_os("SLOPOS_KEEP_DISPLAY").is_none() {
        shell_cmd.env_remove("DISPLAY");
    }
    let mut shell = match OwnedChild::spawn(&mut shell_cmd, "slopos-shell") {
        Ok(child) => child,
        Err(error) => {
            terminate_group(&mut compositor);
            stop_visiond(&mut visiond);
            return Err(format!("cannot start {}: {error}", shell_bin.display()));
        }
    };

    debug_assert_eq!(lifecycle, LifecycleState::Running);
    eprintln!(
        "[slopos-session] shell pid={} pgid={}",
        shell.id(),
        shell.process_group.id
    );

    let result = loop {
        if SHUTDOWN_REQUESTED.load(Ordering::SeqCst) {
            eprintln!("[slopos-session] shutdown signal received; stopping session");
            lifecycle = lifecycle.transition(LifecycleEvent::ShutdownRequested);
            terminate_group(&mut shell);
            terminate_group(&mut compositor);
            stop_visiond(&mut visiond);
            lifecycle = lifecycle.transition(LifecycleEvent::ChildrenReaped);
            break if lifecycle == LifecycleState::Succeeded {
                Ok(())
            } else {
                Err("session shutdown did not reach a clean terminal state".to_string())
            };
        }
        let compositor_status = match compositor.try_wait() {
            Ok(status) => status,
            Err(error) => {
                terminate_group(&mut shell);
                stop_visiond(&mut visiond);
                break Err(format!("cannot wait for compositor: {error}"));
            }
        };
        if let Some(status) = compositor_status {
            terminate_group(&mut shell);
            stop_visiond(&mut visiond);
            break Err(format!("slopos-compositor exited: {status}"));
        }
        let visiond_status = match visiond.as_mut() {
            Some(service) => match service.child.try_wait() {
                Ok(status) => status,
                Err(error) => {
                    terminate_group(&mut shell);
                    terminate_group(&mut compositor);
                    stop_visiond(&mut visiond);
                    break Err(format!("cannot wait for slopos-visiond: {error}"));
                }
            },
            None => None,
        };
        if let Some(status) = visiond_status {
            eprintln!(
                "[slopos-session] slopos-visiond exited: {status}; Vision is unavailable for this session"
            );
            visiond = None;
        }
        let shell_status = match shell.try_wait() {
            Ok(status) => status,
            Err(error) => {
                terminate_group(&mut compositor);
                stop_visiond(&mut visiond);
                break Err(format!("cannot wait for shell: {error}"));
            }
        };
        if let Some(status) = shell_status {
            lifecycle = lifecycle.transition(LifecycleEvent::ShellExited {
                success: status.success(),
            });
            terminate_group(&mut compositor);
            stop_visiond(&mut visiond);
            lifecycle = lifecycle.transition(LifecycleEvent::ChildrenReaped);
            if lifecycle == LifecycleState::Succeeded {
                break Ok(());
            }
            break Err(format!("slopos-shell exited: {status}"));
        }
        thread::sleep(Duration::from_millis(100));
    };

    stop_visiond(&mut visiond);
    result
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("slopos-session: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_backend_requires_x11_display_for_nested() {
        assert_eq!(default_backend_for_host(Some(":99"), None), Backend::Nested);
        assert_eq!(
            default_backend_for_host(None, Some("wayland-0")),
            Backend::Drm,
            "a Wayland-only host must not select the X11 nested backend"
        );
        assert_eq!(default_backend_for_host(Some(""), None), Backend::Drm);
        assert_eq!(default_backend_for_host(None, None), Backend::Drm);
    }

    #[test]
    fn explicit_nested_backend_fails_without_x11_display() {
        let error = validate_backend_transport(Backend::Nested, None).unwrap_err();
        assert!(error.contains("DISPLAY"));
        assert!(validate_backend_transport(Backend::Nested, Some(":99")).is_ok());
        assert!(validate_backend_transport(Backend::Drm, None).is_ok());
        assert!(validate_backend_transport(Backend::Headless, None).is_ok());
    }

    #[test]
    fn runtime_directory_must_be_private_owned_and_not_a_symlink() {
        let path = env::temp_dir().join(format!("slopos-session-runtime-base-{}", session_nonce()));
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();

        assert!(validate_runtime_dir(&path).is_ok());

        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        let error = validate_runtime_dir(&path).unwrap_err();
        assert!(error.contains("mode 0700"));

        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        let link = path.with_extension("link");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(validate_runtime_dir(&link).is_err());

        fs::remove_file(link).unwrap();
        fs::remove_dir(path).unwrap();
    }

    #[test]
    fn runtime_is_nonce_scoped_private_and_cleans_its_owned_directory() {
        let base = env::temp_dir().join(format!("slopos-session-runtime-{}", session_nonce()));
        fs::create_dir(&base).unwrap();
        let (runtime, token) = create_session_runtime(&base).unwrap();
        let path = runtime.path.clone();

        assert_eq!(path.parent().unwrap().file_name().unwrap(), SESSION_ROOT);
        assert!(path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("session-"));
        assert!(runtime.identity.is_some());
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(fs::read_to_string(path.join(TOKEN_FILE)).unwrap(), token);

        drop(runtime);
        assert!(!path.exists());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn distinct_runtime_allocations_have_distinct_nonce_paths_and_tokens() {
        let base = env::temp_dir().join(format!("slopos-session-distinct-{}", session_nonce()));
        fs::create_dir(&base).unwrap();
        let (first, first_token) = create_session_runtime(&base).unwrap();
        let (second, second_token) = create_session_runtime(&base).unwrap();

        assert_ne!(first.path, second.path);
        assert_ne!(first_token, second_token);
        assert_ne!(
            first.path.file_name().unwrap(),
            second.path.file_name().unwrap()
        );

        drop(first);
        drop(second);
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn cleanup_skips_a_directory_replaced_after_creation() {
        let base = env::temp_dir().join(format!("slopos-session-replaced-{}", session_nonce()));
        fs::create_dir(&base).unwrap();
        let (runtime, _) = create_session_runtime(&base).unwrap();
        let path = runtime.path.clone();

        let original_identity = runtime.identity.expect("owned identity");
        fs::remove_dir_all(&path).unwrap();
        fs::create_dir(&path).unwrap();
        fs::write(path.join("foreign"), b"keep").unwrap();
        let replacement_identity = directory_identity(&path).unwrap();
        assert_ne!(
            replacement_identity, original_identity,
            "the held directory descriptor must prevent immediate inode reuse"
        );
        drop(runtime);

        assert_eq!(fs::read(path.join("foreign")).unwrap(), b"keep");
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn dropping_an_unowned_runtime_path_does_not_remove_it() {
        let path = env::temp_dir().join(format!("slopos-session-unowned-{}", session_nonce()));
        fs::create_dir(&path).unwrap();
        fs::write(path.join("keep"), b"not ours").unwrap();

        let runtime = SessionRuntime::unowned(path.clone());
        drop(runtime);

        assert!(path.is_dir());
        assert_eq!(fs::read(path.join("keep")).unwrap(), b"not ours");
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn readiness_binds_socket_to_compositor_pid_token_and_client_display() {
        let payload = "wayland-7\npid=42\ntoken=session-token\nwidth=1280\nheight=800\n";
        let expected = PrivateDisplay {
            socket_name: "wayland-7".to_string(),
            output_width: Some(1280),
            output_height: Some(800),
        };

        assert_eq!(
            validate_readiness(payload, 42, "session-token", "wayland-7\n").unwrap(),
            expected
        );
        assert!(validate_readiness(payload, 43, "session-token", "wayland-7").is_err());
        assert!(validate_readiness(payload, 42, "other-token", "wayland-7").is_err());
        assert!(validate_readiness(payload, 42, "session-token", "wayland-8").is_err());
    }

    #[test]
    fn readiness_parser_rejects_unsafe_or_ambiguous_payloads() {
        for payload in [
            "wayland-/tmp/socket\npid=42\ntoken=token\n",
            "wayland-7\ntoken=token\n",
            "wayland-7\npid=42\npid=43\ntoken=token\n",
            "wayland-7\npid=42\ntoken=token\nunknown=value\n",
        ] {
            assert!(
                parse_readiness(payload).is_err(),
                "accepted payload: {payload:?}"
            );
        }
    }

    #[test]
    fn owned_process_group_can_only_target_a_valid_child_group() {
        let group = OwnedProcessGroup::for_child_pid(42).unwrap();
        assert_eq!(group.id, 42);
        assert_eq!(group.signal_target(), -42);
        assert!(OwnedProcessGroup::for_child_pid(0).is_err());
        assert!(OwnedProcessGroup::for_child_pid(1).is_err());
    }

    #[test]
    fn missing_process_group_falls_back_to_direct_child_kill() {
        let mut command = child_command(Path::new("/bin/sleep"));
        command.arg("1");
        let mut child = OwnedChild::spawn(&mut command, "test-child").unwrap();
        child.process_group = OwnedProcessGroup {
            id: i32::MAX as libc::pid_t,
        };

        assert_eq!(
            signal_owned_group(child.process_group, libc::SIGTERM).unwrap(),
            ProcessGroupSignal::Missing
        );
        let started = Instant::now();
        let status = reap_owned_group_with_grace(&mut child, Duration::ZERO).unwrap();

        assert!(!status.success());
        assert!(
            started.elapsed() < Duration::from_millis(750),
            "direct child fallback took too long: {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn lifecycle_transitions_keep_timeout_and_compositor_death_failed() {
        assert_eq!(
            LifecycleState::Starting.transition(LifecycleEvent::StartupTimeout),
            LifecycleState::Failed
        );
        assert_eq!(
            LifecycleState::Starting.transition(LifecycleEvent::CompositorExited),
            LifecycleState::Failed
        );
        assert_eq!(
            LifecycleState::Starting
                .transition(LifecycleEvent::CompositorReady)
                .transition(LifecycleEvent::CompositorExited),
            LifecycleState::Failed
        );
        assert_eq!(
            LifecycleState::Running
                .transition(LifecycleEvent::ShutdownRequested)
                .transition(LifecycleEvent::ChildrenReaped),
            LifecycleState::Succeeded
        );
        assert_eq!(
            LifecycleState::Running.transition(LifecycleEvent::ShellExited { success: false }),
            LifecycleState::Failed
        );
    }

    #[test]
    fn vision_models_directory_requires_a_manifest() {
        let path = env::temp_dir().join(format!("slopos-session-vision-{}", session_nonce()));
        fs::create_dir(&path).unwrap();
        assert!(configured_vision_models_dir_from_path(&path).is_err());

        fs::write(path.join("manifest.toml"), b"models = []").unwrap();
        assert_eq!(
            configured_vision_models_dir_from_path(&path).unwrap(),
            fs::canonicalize(&path).unwrap()
        );

        fs::remove_dir_all(path).unwrap();
    }
}
