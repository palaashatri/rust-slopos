//! Session-scoped control messages shared by shell clients and the compositor.
//!
//! The shell owns global chrome, but it is not a window manager.  These messages
//! are the small control plane used when a shell action needs the compositor to
//! operate on the focused real client.  The endpoint lives inside the unique
//! session runtime directory created by `slopos-session`; it is never discovered
//! by scanning arbitrary Wayland sockets.

use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

use crate::spaces::{SpaceThumbnailManifest, SpacesControlCommand, SPACE_THUMBNAIL_MANIFEST_FILE};

pub const SESSION_CONTROL_SOCKET: &str = "control.sock";
const APPLICATION_CONTROL_DIR: &str = "app-control";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum WindowPresentationAction {
    ToggleZoom,
    SmartZoom,
    Fill,
    ToggleFullscreen,
    Fullscreen,
    Minimize,
    Restore,
    Close,
}

/// Input events accepted only by the explicitly enabled headless protocol
/// test harness. Coordinates are compositor-space logical pixels and button
/// codes use Linux input-event-codes values (for example, 0x110 for BTN_LEFT).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum HeadlessInputEvent {
    Motion {
        x: i32,
        y: i32,
        time_msec: u32,
    },
    Button {
        button: u32,
        pressed: bool,
        time_msec: u32,
    },
    /// Synthetic gesture events accepted only by the explicitly enabled
    /// headless protocol harness. They exercise compositor policy but never
    /// stand in for physical touchpad evidence.
    GestureSwipeBegin {
        fingers: u32,
        time_msec: u32,
    },
    GestureSwipeUpdate {
        delta_x: i32,
        delta_y: i32,
        time_msec: u32,
    },
    GestureSwipeEnd {
        cancelled: bool,
        time_msec: u32,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SessionControlRequest {
    FocusedWindow {
        action: WindowPresentationAction,
    },
    /// Activate an existing compositor-owned application window from shell
    /// chrome such as the Dock. The compositor remains the sole owner of
    /// focus, stacking, and restore geometry.
    ActivateApplication {
        bundle_id: String,
    },
    /// Activate one of the current compositor's legacy indexed Spaces.
    /// Dynamic shell controls use the stable-ID [`Self::Spaces`] request.
    SwitchWorkspace {
        index: u8,
    },
    /// Apply a dynamic compositor-owned Spaces mutation.  The compositor
    /// publishes the resulting [`crate::SpacesSnapshot`] in the session
    /// runtime directory after a successful command.
    Spaces {
        command: SpacesControlCommand,
    },
    /// Atomically replace the compositor's logical output topology.
    /// The value uses `name:WIDTHxHEIGHT@x,y:sSCALE` entries separated by `;`.
    ReconfigureOutputs {
        layout: String,
    },
    /// Apply the compositor-owned display policy to the running session.
    ///
    /// The values are canonical wire strings (`60hz`, `adaptive`, `srgb`,
    /// `rec2020`, or `scrgb`).  The compositor validates them against the
    /// capabilities of the active backend before mutating any state.
    SetDisplayPolicy {
        policy: DisplayPolicyRequest,
    },
    /// Capture the next compositor-owned framebuffer into an absolute path.
    ///
    /// The compositor validates the destination and schedules a redraw before
    /// invoking its in-process readback path.  This is intentionally a
    /// one-way request; callers observe the atomically committed PNG at the
    /// requested path and must treat a timeout or missing file as failure.
    CaptureScreenshot {
        destination: PathBuf,
    },
    /// Drive the nested/headless compositor's Smithay pointer path for a
    /// deterministic protocol test. Production nested and DRM sessions
    /// explicitly ignore this request.
    HeadlessTestInput {
        event: HeadlessInputEvent,
    },
    FocusedApplicationMenu {
        bundle_id: String,
        action_id: String,
    },
}

/// Name of the atomic compositor-to-shell Spaces projection in the session
/// runtime directory.
pub const SPACES_STATE_FILE: &str = "spaces-state.json";

/// Name of the compositor-authoritative output topology projection.
pub const OUTPUTS_STATE_FILE: &str = "outputs-state.json";

/// Name of the compositor-authoritative display-policy projection.
pub const DISPLAY_POLICY_STATE_FILE: &str = "display-policy-state.json";

/// Typed intent sent by Settings or another session-scoped policy client.
/// Strings are used at this boundary so the bus crate remains independent of
/// the compositor's implementation enums; the compositor performs strict
/// canonical parsing before applying the request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DisplayPolicyRequest {
    pub hdr_requested: bool,
    pub vrr_adaptive: bool,
    pub refresh_rate: String,
    pub color_space: String,
}

/// Authoritative applied display policy and backend capability projection.
///
/// Requested values are retained for diagnostics, while the `*_applied`
/// fields describe the state the compositor actually uses.  A revision is
/// advanced only after an accepted runtime transaction; rejected requests do
/// not change this file.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DisplayPolicySnapshot {
    pub backend: String,
    pub revision: u64,
    pub hdr_requested: bool,
    pub hdr_supported: bool,
    pub hdr_active: bool,
    pub vrr_adaptive: bool,
    pub vrr_supported: bool,
    pub refresh_rate_requested: String,
    pub refresh_rate_applied: String,
    pub color_space_requested: String,
    pub color_space_applied: String,
    pub exact_match: bool,
    pub fallback_reason: Option<String>,
    pub runtime_mutation_supported: bool,
    pub supported_refresh_rates: Vec<String>,
    pub supported_color_spaces: Vec<String>,
}

/// One logical output as published by the compositor for Settings and shell
/// policy consumers.  The geometry is logical; `scale_percent` is the
/// compositor's current uniform buffer scale.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OutputSnapshot {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub scale_percent: u32,
    pub primary: bool,
}

/// Complete, atomically replaced output projection.  `revision` is scoped to
/// the compositor process and increases after every accepted topology change.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OutputsSnapshot {
    pub backend: String,
    pub revision: u64,
    pub outputs: Vec<OutputSnapshot>,
}

/// Return the exact output projection path for the current session.
pub fn session_outputs_state_path() -> Option<PathBuf> {
    std::env::var_os("SLOPOS_SESSION_RUNTIME_DIR")
        .map(PathBuf::from)
        .map(|runtime| runtime.join(OUTPUTS_STATE_FILE))
}

/// Return the exact display-policy projection path for the current session.
pub fn session_display_policy_state_path() -> Option<PathBuf> {
    std::env::var_os("SLOPOS_SESSION_RUNTIME_DIR")
        .map(PathBuf::from)
        .map(|runtime| runtime.join(DISPLAY_POLICY_STATE_FILE))
}

/// Publish one complete display-policy projection atomically.
#[cfg(unix)]
pub fn write_display_policy_snapshot(snapshot: &DisplayPolicySnapshot) -> io::Result<()> {
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    let path = session_display_policy_state_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "SLOPOS_SESSION_RUNTIME_DIR is not set",
        )
    })?;
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "display-policy path has no parent",
        )
    })?;
    fs::create_dir_all(parent)?;
    let bytes = serde_json::to_vec_pretty(snapshot).map_err(io::Error::other)?;
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp = parent.join(format!(".{}.{}.tmp", DISPLAY_POLICY_STATE_FILE, counter));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp)?;
    let result = (|| {
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, &path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(not(unix))]
pub fn write_display_policy_snapshot(_snapshot: &DisplayPolicySnapshot) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "display-policy snapshots require the Unix session runtime",
    ))
}

/// Read the latest compositor display-policy projection.
pub fn read_display_policy_snapshot() -> io::Result<DisplayPolicySnapshot> {
    let path = session_display_policy_state_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "SLOPOS_SESSION_RUNTIME_DIR is not set",
        )
    })?;
    let bytes = std::fs::read(path)?;
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

/// Publish one complete output projection atomically.
#[cfg(unix)]
pub fn write_outputs_snapshot(snapshot: &OutputsSnapshot) -> io::Result<()> {
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    let path = session_outputs_state_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "SLOPOS_SESSION_RUNTIME_DIR is not set",
        )
    })?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "outputs path has no parent"))?;
    fs::create_dir_all(parent)?;
    let bytes = serde_json::to_vec_pretty(snapshot).map_err(io::Error::other)?;
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp = parent.join(format!(".{}.{}.tmp", OUTPUTS_STATE_FILE, counter));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp)?;
    let result = (|| {
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, &path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(not(unix))]
pub fn write_outputs_snapshot(_snapshot: &OutputsSnapshot) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "output snapshots require the Unix session runtime",
    ))
}

/// Read the latest compositor output projection.
pub fn read_outputs_snapshot() -> io::Result<OutputsSnapshot> {
    let path = session_outputs_state_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "SLOPOS_SESSION_RUNTIME_DIR is not set",
        )
    })?;
    let bytes = std::fs::read(path)?;
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

/// Return the exact Spaces snapshot path for the current session.
pub fn session_spaces_state_path() -> Option<PathBuf> {
    std::env::var_os("SLOPOS_SESSION_RUNTIME_DIR")
        .map(PathBuf::from)
        .map(|runtime| runtime.join(SPACES_STATE_FILE))
}

/// Publish one complete Spaces snapshot atomically.  The runtime directory is
/// session-scoped and owned by the session supervisor; readers never observe a
/// partially-written JSON document.
#[cfg(unix)]
pub fn write_spaces_snapshot(snapshot: &crate::SpacesSnapshot) -> io::Result<()> {
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    let path = session_spaces_state_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "SLOPOS_SESSION_RUNTIME_DIR is not set",
        )
    })?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Spaces path has no parent"))?;
    fs::create_dir_all(parent)?;
    let bytes = serde_json::to_vec_pretty(snapshot).map_err(io::Error::other)?;
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp = parent.join(format!(".{}.{}.tmp", SPACES_STATE_FILE, counter));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp)?;
    let result = (|| {
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, &path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(not(unix))]
pub fn write_spaces_snapshot(_snapshot: &crate::SpacesSnapshot) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Spaces snapshots require the Unix session runtime",
    ))
}

/// Read the latest compositor snapshot, if the session has published one.
pub fn read_spaces_snapshot() -> io::Result<crate::SpacesSnapshot> {
    let path = session_spaces_state_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "SLOPOS_SESSION_RUNTIME_DIR is not set",
        )
    })?;
    let bytes = std::fs::read(path)?;
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

/// Publish one complete compositor-owned Space thumbnail manifest atomically.
///
/// The manifest is written after all individual PNGs have been committed. A
/// shell can therefore ignore stale files and only display captures listed by
/// this generation.
#[cfg(unix)]
pub fn write_space_thumbnail_manifest(manifest: &SpaceThumbnailManifest) -> io::Result<()> {
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    let path = crate::session_space_thumbnail_manifest_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "SLOPOS_SESSION_RUNTIME_DIR is not set or unsafe",
        )
    })?;
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "thumbnail manifest path has no parent",
        )
    })?;
    fs::create_dir_all(parent)?;
    let bytes = serde_json::to_vec_pretty(manifest).map_err(io::Error::other)?;
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp = parent.join(format!(
        ".{}.{}.tmp",
        SPACE_THUMBNAIL_MANIFEST_FILE, counter
    ));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp)?;
    let result = (|| {
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, &path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(not(unix))]
pub fn write_space_thumbnail_manifest(_manifest: &SpaceThumbnailManifest) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "thumbnail manifests require the Unix session runtime",
    ))
}

/// Read the latest atomic thumbnail manifest for the current session.
pub fn read_space_thumbnail_manifest() -> io::Result<SpaceThumbnailManifest> {
    let path = crate::session_space_thumbnail_manifest_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "SLOPOS_SESSION_RUNTIME_DIR is not set or unsafe",
        )
    })?;
    #[cfg(unix)]
    let bytes = {
        use std::fs::OpenOptions;
        use std::io::Read;
        use std::os::unix::fs::OpenOptionsExt;

        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&path)?;
        let before = file.metadata()?;
        if !before.file_type().is_file()
            || before.len() == 0
            || before.len() > crate::MAX_SPACE_THUMBNAIL_MANIFEST_BYTES
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "thumbnail manifest is not a bounded regular file",
            ));
        }
        let mut bytes = Vec::new();
        (&mut file)
            .take(crate::MAX_SPACE_THUMBNAIL_MANIFEST_BYTES.saturating_add(1))
            .read_to_end(&mut bytes)?;
        let after = file.metadata()?;
        if before.len() != after.len()
            || bytes.len() as u64 != after.len()
            || bytes.len() as u64 > crate::MAX_SPACE_THUMBNAIL_MANIFEST_BYTES
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "thumbnail manifest changed during read",
            ));
        }
        bytes
    };
    #[cfg(not(unix))]
    let bytes = std::fs::read(path)?;
    if bytes.len() as u64 > crate::MAX_SPACE_THUMBNAIL_MANIFEST_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "thumbnail manifest exceeds the bounded read limit",
        ));
    }
    let manifest =
        serde_json::from_slice::<SpaceThumbnailManifest>(&bytes).map_err(io::Error::other)?;
    if !manifest.is_valid() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "thumbnail manifest contains invalid or duplicate captures",
        ));
    }
    Ok(manifest)
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ApplicationMenuRequest {
    pub bundle_id: String,
    pub action_id: String,
}

pub fn session_control_socket_path() -> Option<PathBuf> {
    std::env::var_os("SLOPOS_SESSION_RUNTIME_DIR")
        .map(PathBuf::from)
        .map(|runtime| runtime.join(SESSION_CONTROL_SOCKET))
}

fn safe_socket_component(value: &str) -> String {
    let component: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if component.is_empty() {
        "application".to_string()
    } else {
        component
    }
}

pub fn application_control_socket_path(bundle_id: &str) -> Option<PathBuf> {
    std::env::var_os("SLOPOS_SESSION_RUNTIME_DIR")
        .map(PathBuf::from)
        .map(|runtime| {
            runtime
                .join(APPLICATION_CONTROL_DIR)
                .join(format!("{}.sock", safe_socket_component(bundle_id)))
        })
}

#[cfg(unix)]
pub fn send_application_menu_action(bundle_id: &str, action_id: &str) -> io::Result<()> {
    use std::os::unix::net::UnixDatagram;

    let path = application_control_socket_path(bundle_id).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "SLOPOS_SESSION_RUNTIME_DIR is not set",
        )
    })?;
    let request = ApplicationMenuRequest {
        bundle_id: bundle_id.to_string(),
        action_id: action_id.to_string(),
    };
    let payload = serde_json::to_vec(&request).map_err(io::Error::other)?;
    UnixDatagram::unbound()?.send_to(&payload, path).map(|_| ())
}

#[cfg(not(unix))]
pub fn send_application_menu_action(_bundle_id: &str, _action_id: &str) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "application menu control requires a Unix-domain socket",
    ))
}

#[cfg(unix)]
pub fn send_session_control(request: &SessionControlRequest) -> io::Result<()> {
    use std::os::unix::net::UnixDatagram;

    let path = session_control_socket_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "SLOPOS_SESSION_RUNTIME_DIR is not set",
        )
    })?;
    let payload = serde_json::to_vec(request).map_err(io::Error::other)?;
    UnixDatagram::unbound()?.send_to(&payload, path).map(|_| ())
}

#[cfg(not(unix))]
pub fn send_session_control(_request: &SessionControlRequest) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "session control requires a Unix-domain socket",
    ))
}

#[cfg(unix)]
pub struct SessionControlListener {
    socket: std::os::unix::net::UnixDatagram,
    path: PathBuf,
    expected_uid: u32,
}

#[cfg(unix)]
impl std::os::fd::AsFd for SessionControlListener {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        self.socket.as_fd()
    }
}

/// The session control socket is a same-UID endpoint.  Linux supplies the
/// sender credential on every datagram when `SO_PASSCRED` is enabled; reject
/// missing credentials as well as a different UID.  The helper is kept
/// platform-neutral so the policy itself remains testable on hosts whose Unix
/// datagram APIs do not expose portable per-datagram credentials.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn peer_uid_is_allowed(peer_uid: Option<u32>, expected_uid: u32) -> bool {
    peer_uid == Some(expected_uid)
}

#[cfg(target_os = "linux")]
fn enable_peer_credentials(socket: &std::os::unix::net::UnixDatagram) -> io::Result<()> {
    use std::mem::size_of;
    use std::os::fd::AsRawFd;

    let enabled: libc::c_int = 1;
    let result = unsafe {
        libc::setsockopt(
            socket.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PASSCRED,
            (&enabled as *const libc::c_int).cast(),
            size_of::<libc::c_int>() as libc::socklen_t,
        )
    };
    if result == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn recv_control_datagram(
    socket: &std::os::unix::net::UnixDatagram,
    buffer: &mut [u8],
    expected_uid: u32,
) -> io::Result<Option<usize>> {
    use std::mem::{size_of, zeroed};
    use std::os::fd::AsRawFd;

    let mut iovec = libc::iovec {
        iov_base: buffer.as_mut_ptr().cast(),
        iov_len: buffer.len(),
    };
    let control_len =
        unsafe { libc::CMSG_SPACE(size_of::<libc::ucred>() as libc::c_uint) } as usize;
    let mut control = vec![0_u8; control_len];
    let mut message: libc::msghdr = unsafe { zeroed() };
    message.msg_iov = &mut iovec;
    message.msg_iovlen = 1;
    message.msg_control = control.as_mut_ptr().cast();
    message.msg_controllen = control.len();

    let received = unsafe { libc::recvmsg(socket.as_raw_fd(), &mut message, libc::MSG_DONTWAIT) };
    if received < 0 {
        return Err(io::Error::last_os_error());
    }

    if message.msg_flags & libc::MSG_CTRUNC != 0 {
        tracing::warn!("discarding session control datagram with truncated credentials");
        return Ok(None);
    }

    let mut peer_uid = None;
    let mut header = unsafe { libc::CMSG_FIRSTHDR(&message) };
    while !header.is_null() {
        let header_ref = unsafe { &*header };
        if header_ref.cmsg_level == libc::SOL_SOCKET
            && header_ref.cmsg_type == libc::SCM_CREDENTIALS
            && header_ref.cmsg_len as usize
                >= unsafe { libc::CMSG_LEN(size_of::<libc::ucred>() as libc::c_uint) } as usize
        {
            let data = unsafe { libc::CMSG_DATA(header) };
            let credentials = unsafe { std::ptr::read_unaligned(data.cast::<libc::ucred>()) };
            peer_uid = Some(credentials.uid as u32);
            break;
        }
        header = unsafe { libc::CMSG_NXTHDR(&message, header) };
    }

    if !peer_uid_is_allowed(peer_uid, expected_uid) {
        tracing::warn!(
            peer_uid = ?peer_uid,
            expected_uid,
            "discarding session control datagram from unauthorized UID"
        );
        return Ok(None);
    }

    Ok(Some(received as usize))
}

#[cfg(all(unix, not(target_os = "linux")))]
fn recv_control_datagram(
    socket: &std::os::unix::net::UnixDatagram,
    buffer: &mut [u8],
    _expected_uid: u32,
) -> io::Result<Option<usize>> {
    // macOS/BSD do not expose a portable per-datagram credential ancillary
    // record for an unconnected Unix datagram.  The socket is still mode 0600;
    // Linux production sessions use the strict SO_PASSCRED path above.  Keep
    // the same receive API for host-side development/tests and cover the
    // authorization predicate with deterministic unit tests.
    socket.recv(buffer).map(Some)
}

#[cfg(unix)]
impl SessionControlListener {
    /// Bind the exact socket owned by this session.  The runtime directory is
    /// already restricted to the session user by `slopos-session`.
    pub fn bind(runtime: &Path) -> io::Result<Self> {
        use std::os::unix::fs::PermissionsExt;
        use std::os::unix::net::UnixDatagram;

        let path = runtime.join(SESSION_CONTROL_SOCKET);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        let socket = UnixDatagram::bind(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        #[cfg(target_os = "linux")]
        enable_peer_credentials(&socket)?;
        socket.set_nonblocking(true)?;
        let expected_uid = unsafe { libc::geteuid() } as u32;
        Ok(Self {
            socket,
            path,
            expected_uid,
        })
    }

    pub fn drain(&self) -> Vec<SessionControlRequest> {
        let mut requests = Vec::new();
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            match recv_control_datagram(&self.socket, &mut buffer, self.expected_uid) {
                Ok(Some(size)) => match serde_json::from_slice(&buffer[..size]) {
                    Ok(request) => requests.push(request),
                    Err(error) => {
                        tracing::warn!(%error, "discarding malformed session control request")
                    }
                },
                Ok(None) => continue,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => {
                    tracing::warn!(%error, "session control socket read failed");
                    break;
                }
            }
        }
        requests
    }
}

#[cfg(unix)]
impl Drop for SessionControlListener {
    fn drop(&mut self) {
        // This is the exact socket created by this listener, never a glob or a
        // host Wayland socket.  Ignore a prior session supervisor cleanup.
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(unix)]
pub struct ApplicationControlListener {
    socket: std::os::unix::net::UnixDatagram,
    path: PathBuf,
}

#[cfg(unix)]
impl ApplicationControlListener {
    pub fn bind(bundle_id: &str) -> io::Result<Self> {
        let runtime = std::env::var_os("SLOPOS_SESSION_RUNTIME_DIR").ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "SLOPOS_SESSION_RUNTIME_DIR is not set",
            )
        })?;
        Self::bind_at(bundle_id, Path::new(&runtime))
    }

    /// Bind an endpoint inside an explicit session runtime directory. The
    /// explicit form keeps tests and launchers from mutating process-global
    /// environment while exercising the application control plane.
    pub fn bind_at(bundle_id: &str, runtime: &Path) -> io::Result<Self> {
        use std::os::unix::net::UnixDatagram;

        let path = runtime
            .join(APPLICATION_CONTROL_DIR)
            .join(format!("{}.sock", safe_socket_component(bundle_id)));
        let directory = path.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "application control path has no parent",
            )
        })?;
        std::fs::create_dir_all(directory)?;
        if path.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AddrInUse,
                format!(
                    "application control endpoint already exists: {}",
                    path.display()
                ),
            ));
        }
        let socket = UnixDatagram::bind(&path)?;
        socket.set_nonblocking(true)?;
        Ok(Self { socket, path })
    }

    pub fn drain(&self) -> Vec<ApplicationMenuRequest> {
        let mut requests = Vec::new();
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            match self.socket.recv(&mut buffer) {
                Ok(size) => match serde_json::from_slice(&buffer[..size]) {
                    Ok(request) => requests.push(request),
                    Err(error) => {
                        tracing::warn!(%error, "discarding malformed application menu request")
                    }
                },
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => {
                    tracing::warn!(%error, "application control socket read failed");
                    break;
                }
            }
        }
        requests
    }

    /// Receive one application-menu request without polling.
    ///
    /// SDK clients normally sleep in their event loop while idle.  A blocking
    /// listener thread can therefore wait on the exact per-application socket
    /// and wake the UI event loop through its proxy, instead of making every
    /// client spin in `ControlFlow::Poll` or relying on an unrelated redraw.
    pub fn recv_blocking(&self) -> io::Result<ApplicationMenuRequest> {
        self.socket.set_nonblocking(false)?;
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            let size = self.socket.recv(&mut buffer)?;
            match serde_json::from_slice(&buffer[..size]) {
                Ok(request) => return Ok(request),
                Err(error) => {
                    tracing::warn!(%error, "discarding malformed application menu request")
                }
            }
        }
    }
}

#[cfg(unix)]
impl Drop for ApplicationControlListener {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(not(unix))]
pub struct ApplicationControlListener;

#[cfg(not(unix))]
impl ApplicationControlListener {
    pub fn bind(_bundle_id: &str) -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "application menu control requires a Unix-domain socket",
        ))
    }

    pub fn drain(&self) -> Vec<ApplicationMenuRequest> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_round_trips_through_json() {
        let request = SessionControlRequest::FocusedWindow {
            action: WindowPresentationAction::ToggleFullscreen,
        };
        let encoded = serde_json::to_vec(&request).unwrap();
        assert_eq!(
            serde_json::from_slice::<SessionControlRequest>(&encoded).unwrap(),
            request
        );
    }

    #[test]
    fn refresh_thumbnails_request_round_trips_through_json() {
        let request = SessionControlRequest::Spaces {
            command: SpacesControlCommand::RefreshThumbnails,
        };
        let encoded = serde_json::to_vec(&request).unwrap();
        assert_eq!(
            serde_json::from_slice::<SessionControlRequest>(&encoded).unwrap(),
            request
        );
    }

    #[test]
    fn active_window_output_move_request_round_trips_through_json() {
        let request = SessionControlRequest::Spaces {
            command: SpacesControlCommand::MoveActiveWindowToOutput {
                output_id: "HDMI-A-1".into(),
            },
        };
        let encoded = serde_json::to_vec(&request).unwrap();
        assert_eq!(
            serde_json::from_slice::<SessionControlRequest>(&encoded).unwrap(),
            request
        );
    }

    #[test]
    fn output_reconfiguration_request_round_trips_through_json() {
        let request = SessionControlRequest::ReconfigureOutputs {
            layout: "LEFT:800x600@0,0:s100;RIGHT:1024x768@800,0:s100".into(),
        };
        let encoded = serde_json::to_vec(&request).unwrap();
        assert_eq!(
            serde_json::from_slice::<SessionControlRequest>(&encoded).unwrap(),
            request
        );
    }

    #[test]
    fn display_policy_request_round_trips_through_json() {
        let request = SessionControlRequest::SetDisplayPolicy {
            policy: DisplayPolicyRequest {
                hdr_requested: false,
                vrr_adaptive: false,
                refresh_rate: "120hz".into(),
                color_space: "srgb".into(),
            },
        };
        let encoded = serde_json::to_vec(&request).unwrap();
        assert_eq!(
            serde_json::from_slice::<SessionControlRequest>(&encoded).unwrap(),
            request
        );
    }

    #[test]
    fn compositor_screenshot_request_round_trips_through_json() {
        let request = SessionControlRequest::CaptureScreenshot {
            destination: PathBuf::from("/tmp/slopos-portal-shot.png"),
        };
        let encoded = serde_json::to_vec(&request).unwrap();
        assert_eq!(
            serde_json::from_slice::<SessionControlRequest>(&encoded).unwrap(),
            request
        );
    }

    #[test]
    fn display_policy_snapshot_round_trips_through_json() {
        let snapshot = DisplayPolicySnapshot {
            backend: "headless".into(),
            revision: 2,
            hdr_requested: false,
            hdr_supported: false,
            hdr_active: false,
            vrr_adaptive: false,
            vrr_supported: false,
            refresh_rate_requested: "120hz".into(),
            refresh_rate_applied: "120hz".into(),
            color_space_requested: "srgb".into(),
            color_space_applied: "srgb".into(),
            exact_match: true,
            fallback_reason: None,
            runtime_mutation_supported: true,
            supported_refresh_rates: vec!["60hz".into(), "120hz".into()],
            supported_color_spaces: vec!["srgb".into()],
        };
        let encoded = serde_json::to_vec(&snapshot).unwrap();
        assert_eq!(
            serde_json::from_slice::<DisplayPolicySnapshot>(&encoded).unwrap(),
            snapshot
        );
    }

    #[test]
    fn output_snapshot_round_trips_through_json() {
        let snapshot = OutputsSnapshot {
            backend: "headless".into(),
            revision: 4,
            outputs: vec![OutputSnapshot {
                name: "LEFT".into(),
                width: 800,
                height: 600,
                x: 0,
                y: 0,
                scale_percent: 100,
                primary: true,
            }],
        };
        let encoded = serde_json::to_vec(&snapshot).unwrap();
        assert_eq!(
            serde_json::from_slice::<OutputsSnapshot>(&encoded).unwrap(),
            snapshot
        );
    }

    #[test]
    fn activate_application_request_round_trips_through_json() {
        let request = SessionControlRequest::ActivateApplication {
            bundle_id: "com.slopos.settings".into(),
        };
        let encoded = serde_json::to_vec(&request).unwrap();
        assert_eq!(
            serde_json::from_slice::<SessionControlRequest>(&encoded).unwrap(),
            request
        );
    }

    #[test]
    fn switch_workspace_request_round_trips_through_json() {
        let request = SessionControlRequest::SwitchWorkspace { index: 3 };
        let encoded = serde_json::to_vec(&request).unwrap();
        assert_eq!(
            serde_json::from_slice::<SessionControlRequest>(&encoded).unwrap(),
            request
        );
    }

    #[test]
    fn headless_test_input_request_round_trips_through_json() {
        let request = SessionControlRequest::HeadlessTestInput {
            event: HeadlessInputEvent::Motion {
                x: 70,
                y: 70,
                time_msec: 100,
            },
        };
        let encoded = serde_json::to_vec(&request).unwrap();
        assert_eq!(
            serde_json::from_slice::<SessionControlRequest>(&encoded).unwrap(),
            request
        );
    }

    #[test]
    fn peer_uid_policy_requires_an_exact_credential_match() {
        assert!(peer_uid_is_allowed(Some(1000), 1000));
        assert!(!peer_uid_is_allowed(Some(1001), 1000));
        assert!(!peer_uid_is_allowed(None, 1000));
    }

    #[cfg(unix)]
    #[test]
    fn session_control_socket_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let runtime = std::env::temp_dir().join(format!(
            "slo-mode-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&runtime).unwrap();
        let listener = SessionControlListener::bind(&runtime).unwrap();
        let mode = std::fs::metadata(runtime.join(SESSION_CONTROL_SOCKET))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
        drop(listener);
        std::fs::remove_dir_all(runtime).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_listener_enables_kernel_peer_credentials() {
        use std::mem::size_of;
        use std::os::fd::AsRawFd;

        let runtime = std::env::temp_dir().join(format!(
            "slo-passcred-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&runtime).unwrap();
        let listener = SessionControlListener::bind(&runtime).unwrap();
        let mut enabled = 0_i32;
        let mut length = size_of::<libc::c_int>() as libc::socklen_t;
        let result = unsafe {
            libc::getsockopt(
                listener.socket.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_PASSCRED,
                (&mut enabled as *mut libc::c_int).cast(),
                &mut length,
            )
        };
        assert_eq!(result, 0);
        assert_eq!(enabled, 1);
        drop(listener);
        std::fs::remove_dir_all(runtime).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn listener_drains_typed_requests() {
        use std::os::unix::net::UnixDatagram;
        // macOS limits Unix-domain socket paths to a small fixed byte budget.
        // Keep the per-process directory name short so this test also works
        // under long `$TMPDIR` paths used by the default macOS test runner.
        let runtime = std::env::temp_dir().join(format!("slo-{}", std::process::id()));
        std::fs::create_dir_all(&runtime).unwrap();
        let listener = SessionControlListener::bind(&runtime).unwrap();
        let sender = UnixDatagram::unbound().unwrap();
        let request = SessionControlRequest::FocusedWindow {
            action: WindowPresentationAction::Minimize,
        };
        sender
            .send_to(
                serde_json::to_vec(&request).unwrap().as_slice(),
                runtime.join(SESSION_CONTROL_SOCKET),
            )
            .unwrap();
        assert_eq!(listener.drain(), vec![request]);
        drop(listener);
        let _ = std::fs::remove_dir(runtime);
    }

    #[cfg(unix)]
    #[test]
    fn listener_drains_switch_workspace_request() {
        use std::os::unix::net::UnixDatagram;

        let runtime = std::env::temp_dir().join(format!("slo-ws-{}", std::process::id()));
        std::fs::create_dir_all(&runtime).unwrap();
        let listener = SessionControlListener::bind(&runtime).unwrap();
        let sender = UnixDatagram::unbound().unwrap();
        let request = SessionControlRequest::SwitchWorkspace { index: 6 };
        sender
            .send_to(
                serde_json::to_vec(&request).unwrap().as_slice(),
                runtime.join(SESSION_CONTROL_SOCKET),
            )
            .unwrap();
        assert_eq!(listener.drain(), vec![request]);
        drop(listener);
        let _ = std::fs::remove_dir(runtime);
    }

    #[cfg(unix)]
    #[test]
    fn application_listener_drains_typed_menu_requests() {
        use std::os::unix::net::UnixDatagram;

        let runtime = std::env::temp_dir().join(format!("slo-app-{}", std::process::id()));
        std::fs::create_dir_all(&runtime).unwrap();
        let bundle_id = "com.slopos.test";
        let listener = ApplicationControlListener::bind_at(bundle_id, &runtime).unwrap();
        let socket_path = runtime
            .join(APPLICATION_CONTROL_DIR)
            .join("com.slopos.test.sock");
        let sender = UnixDatagram::unbound().unwrap();
        let request = ApplicationMenuRequest {
            bundle_id: bundle_id.to_string(),
            action_id: "com.slopos.test.file.open".to_string(),
        };
        sender
            .send_to(
                serde_json::to_vec(&request).unwrap().as_slice(),
                socket_path,
            )
            .unwrap();
        assert_eq!(listener.drain(), vec![request]);
        drop(listener);
        let _ = std::fs::remove_dir(runtime.join(APPLICATION_CONTROL_DIR));
        let _ = std::fs::remove_dir(runtime);
    }
}
