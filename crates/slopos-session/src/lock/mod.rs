//! The screen lock: the lock state machine, the idle timer, password checks
//! through PAM, logind integration, the lock surface and the watcher that
//! starts a lock on idle, suspend or request.

pub mod auth;
pub mod clock;
pub mod idle;
pub mod logind;
pub mod screen;
pub mod state;
pub mod watch;

use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

/// The PAM service the password check uses (see `auth::PAM_SERVICE`).
pub const PAM_SERVICE_FILE: &str = "/etc/pam.d/slopos-lock";

/// Exit status of `slopos-lock` when it refuses to lock because
/// [`PAM_SERVICE_FILE`] is missing. The watcher treats it as a clean end: the
/// screen was never covered, so there is nothing to lock again.
pub const EXIT_NOT_CONFIGURED: u8 = 3;

/// Whether the password service is installed. Without it no password can be
/// checked (libpam would fall back to the `other` stack), so the lock must not
/// cover the screen: only logind could unlock it.
pub fn password_service_installed(path: &Path) -> bool {
    path.is_file()
}

/// The lock's single-instance file: one lock per user session.
pub fn instance_lock_path() -> Result<PathBuf, String> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join("slopos-lock.lock"))
        .ok_or_else(|| "XDG_RUNTIME_DIR is not set; the screen lock cannot run".to_owned())
}

/// Take the single-instance lock without waiting. `Ok(None)` means another
/// lock already runs. The returned file must stay open while locked.
pub fn acquire_instance(path: &std::path::Path) -> io::Result<Option<File>> {
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(path)?;
    // SAFETY: flock(2) on a descriptor this function owns.
    let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if result == 0 {
        Ok(Some(file))
    } else {
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::EWOULDBLOCK) {
            Ok(None)
        } else {
            Err(error)
        }
    }
}

/// The name of the user running the session, from the passwd database.
pub fn current_user() -> Option<String> {
    // SAFETY: getpwuid_r with a buffer that lives for the call; the name is
    // copied out before the buffer goes away.
    unsafe {
        let mut buffer = vec![0 as libc::c_char; 4096];
        let mut entry: libc::passwd = std::mem::zeroed();
        let mut result: *mut libc::passwd = std::ptr::null_mut();
        let status = libc::getpwuid_r(
            libc::getuid(),
            &mut entry,
            buffer.as_mut_ptr(),
            buffer.len(),
            &mut result,
        );
        if status != 0 || result.is_null() || entry.pw_name.is_null() {
            return None;
        }
        Some(
            std::ffi::CStr::from_ptr(entry.pw_name)
                .to_string_lossy()
                .into_owned(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_password_service_is_not_installed() {
        let missing = std::env::temp_dir().join("slopos-lock-no-such-pam-service");
        assert!(!password_service_installed(&missing));
    }

    #[test]
    fn a_present_password_service_file_counts_as_installed() {
        let file =
            std::env::temp_dir().join(format!("slopos-lock-pam-service-{}", std::process::id()));
        std::fs::write(&file, "auth required pam_deny.so\n").unwrap();
        let installed = password_service_installed(&file);
        std::fs::remove_file(&file).unwrap();
        assert!(installed);
    }

    #[test]
    fn the_password_service_is_the_one_auth_uses() {
        assert_eq!(
            PAM_SERVICE_FILE,
            format!("/etc/pam.d/{}", auth::PAM_SERVICE)
        );
    }
}
