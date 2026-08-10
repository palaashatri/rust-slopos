//! Spawn first-party SLOPOS-I binaries as Wayland clients of this compositor.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// Return true only for an unqualified first-party executable name.
///
/// Client launch requests are compositor-internal today, but treating the name
/// as data rather than a path prevents a future control-plane caller from
/// turning `spawn_client` into an arbitrary executable launcher.
pub fn is_valid_client_binary_name(bin: &str) -> bool {
    !bin.is_empty()
        && bin != "."
        && bin != ".."
        && bin
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.metadata()
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

/// Resolve an executable using explicit inputs.
///
/// Keeping this helper independent of process-global environment variables
/// makes launch policy deterministic and lets tests cover non-executable and
/// missing candidates without racing other tests.
fn resolve_client_bin_from(
    bin: &str,
    home: Option<&Path>,
    path_value: Option<&OsStr>,
    system_bin_dir: &Path,
) -> Option<PathBuf> {
    if !is_valid_client_binary_name(bin) {
        return None;
    }

    if let Some(home) = home {
        let candidate = home.join("slopos-i/target/release").join(bin);
        if is_executable_file(&candidate) {
            return Some(candidate);
        }
    }

    if let Some(path_value) = path_value {
        for dir in std::env::split_paths(path_value) {
            if dir.as_os_str().is_empty() {
                // Never interpret an empty PATH component as the current
                // directory for compositor-owned privileged launch policy.
                continue;
            }
            let candidate = dir.join(bin);
            if is_executable_file(&candidate) {
                return Some(candidate);
            }
        }
    }

    let system = system_bin_dir.join(bin);
    is_executable_file(&system).then_some(system)
}

/// Resolve a first-party binary from `~/slopos-i/target/release`, `PATH`, or
/// `/usr/local/bin`.
///
/// This compatibility wrapper preserves the historical return type. New launch
/// code should use [`resolve_client_bin_checked`] so a missing binary cannot be
/// passed back to `Command` for another implicit PATH lookup.
pub fn resolve_client_bin(bin: &str) -> PathBuf {
    resolve_client_bin_checked(bin).unwrap_or_else(|| PathBuf::from(bin))
}

/// Resolve an existing executable first-party binary without an implicit final
/// PATH lookup.
pub fn resolve_client_bin_checked(bin: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let path_value = std::env::var_os("PATH");
    resolve_client_bin_from(
        bin,
        home.as_deref(),
        path_value.as_deref(),
        Path::new("/usr/local/bin"),
    )
}

#[cfg(unix)]
fn ensure_private_directory(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::create_dir_all(path)?;
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("{} exists but is not a directory", path.display()),
        ));
    }
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn ensure_private_directory(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)
}

/// Spawn `bin` as a Wayland client on `wayland_socket_name`.
pub fn spawn_client(wayland_socket_name: &str, bin: &str) {
    if !is_valid_client_binary_name(bin) {
        tracing::warn!(bin, "refusing invalid first-party client binary name");
        return;
    }
    if !wayland_socket_name.starts_with("wayland-")
        || wayland_socket_name["wayland-".len()..].is_empty()
        || !wayland_socket_name["wayland-".len()..]
            .bytes()
            .all(|byte| byte.is_ascii_digit())
    {
        tracing::warn!(
            socket = wayland_socket_name,
            bin,
            "refusing invalid private Wayland socket name"
        );
        return;
    }

    let Some(path) = resolve_client_bin_checked(bin) else {
        tracing::warn!(
            bin,
            "first-party client executable was not found or is not executable"
        );
        return;
    };

    let mut cmd = std::process::Command::new(&path);
    cmd.env("WAYLAND_DISPLAY", wayland_socket_name)
        .env("SLOPOS_CLIENT_WAYLAND_DISPLAY", wayland_socket_name)
        .env("WINIT_UNIX_BACKEND", "wayland")
        .env("SLOPOS_GLOBAL_MENU", "1")
        // A first-party client must never silently escape to the host X server
        // or inherit the nested compositor's host-display control variable.
        .env_remove("DISPLAY")
        .env_remove("SLOPOS_HOST_WAYLAND_DISPLAY");

    if let Ok(width) = std::env::var("SLOPOS_COMPOSITOR_WIDTH") {
        cmd.env("SLOPOS_COMPOSITOR_WIDTH", width);
    }
    if let Ok(height) = std::env::var("SLOPOS_COMPOSITOR_HEIGHT") {
        cmd.env("SLOPOS_COMPOSITOR_HEIGHT", height);
    }
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") {
        cmd.env("XDG_RUNTIME_DIR", &runtime);
        let menu_dir = PathBuf::from(&runtime).join("slopos-i").join("menus");
        match ensure_private_directory(&menu_dir) {
            Ok(()) => {
                cmd.env("SLOPOS_MENU_MANIFEST_DIR", menu_dir);
            }
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    path = %menu_dir.display(),
                    bin,
                    "could not create private menu manifest directory"
                );
            }
        }
    }

    if bin == "slopos-lock" {
        if let Ok(password) = std::env::var("SLOPOS_LOCK_PASSWORD") {
            cmd.env("SLOPOS_LOCK_PASSWORD", password);
        }
    } else {
        cmd.env_remove("SLOPOS_LOCK_PASSWORD");
    }

    match cmd.spawn() {
        Ok(child) => {
            tracing::info!(bin, pid = child.id(), path = %path.display(), "spawned client");
        }
        Err(error) => {
            tracing::warn!(error = %error, bin, path = %path.display(), "spawn_client failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

    struct TempTree(PathBuf);

    impl TempTree {
        fn new(label: &str) -> Self {
            let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "slopos-client-spawn-{label}-{}-{sequence}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn make_candidate(path: &Path, executable: bool) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = if executable { 0o700 } else { 0o600 };
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
        }
        #[cfg(not(unix))]
        let _ = executable;
    }

    #[test]
    fn binary_names_cannot_escape_the_first_party_search_roots() {
        for invalid in [
            "",
            ".",
            "..",
            "../shell",
            "bin/shell",
            "bin\\shell",
            "shell name",
        ] {
            assert!(
                !is_valid_client_binary_name(invalid),
                "accepted {invalid:?}"
            );
        }
        for valid in ["slopos-shell", "slopos_lock", "app.v2"] {
            assert!(is_valid_client_binary_name(valid), "rejected {valid:?}");
        }
    }

    #[test]
    fn resolver_prefers_executable_release_binary() {
        let tree = TempTree::new("home");
        let home_binary = tree
            .path()
            .join("home/slopos-i/target/release/slopos-shell");
        let path_binary = tree.path().join("path/slopos-shell");
        make_candidate(&home_binary, true);
        make_candidate(&path_binary, true);

        let resolved = resolve_client_bin_from(
            "slopos-shell",
            Some(&tree.path().join("home")),
            Some(tree.path().join("path").as_os_str()),
            &tree.path().join("system"),
        );
        assert_eq!(resolved.as_deref(), Some(home_binary.as_path()));
    }

    #[cfg(unix)]
    #[test]
    fn resolver_skips_non_executable_candidates() {
        let tree = TempTree::new("permissions");
        let home_binary = tree
            .path()
            .join("home/slopos-i/target/release/slopos-shell");
        let path_binary = tree.path().join("path/slopos-shell");
        make_candidate(&home_binary, false);
        make_candidate(&path_binary, true);

        let resolved = resolve_client_bin_from(
            "slopos-shell",
            Some(&tree.path().join("home")),
            Some(tree.path().join("path").as_os_str()),
            &tree.path().join("system"),
        );
        assert_eq!(resolved.as_deref(), Some(path_binary.as_path()));
    }

    #[test]
    fn resolver_returns_none_when_no_candidate_exists() {
        let tree = TempTree::new("missing");
        assert_eq!(
            resolve_client_bin_from(
                "slopos-shell",
                Some(&tree.path().join("home")),
                Some(tree.path().join("path").as_os_str()),
                &tree.path().join("system"),
            ),
            None
        );
    }

    #[cfg(unix)]
    #[test]
    fn private_directory_permissions_are_repaired() {
        use std::os::unix::fs::PermissionsExt;

        let tree = TempTree::new("private-dir");
        let directory = tree.path().join("menus");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o755)).unwrap();

        ensure_private_directory(&directory).unwrap();
        let mode = std::fs::metadata(directory).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
    }
}
