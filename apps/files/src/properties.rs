//! Properties for one entry (FILES-P1 item 2): read through `symlink_metadata`,
//! so a symlink describes the link itself and is never followed. Folder
//! contents are not counted; that would be a recursive walk.
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Folder,
    File,
    /// The link itself; `target` is what it points at, unresolved.
    Link {
        target: Option<PathBuf>,
    },
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Properties {
    pub name: String,
    pub location: PathBuf,
    pub kind: Kind,
    /// Byte size of files and links; `None` for folders and special files.
    pub size: Option<u64>,
    pub modified: Option<SystemTime>,
    /// Permission bits (`st_mode & 0o777`).
    pub mode: u32,
}

/// Why the properties of an entry could not be read, in words a user can act on.
pub fn explain(error: &io::Error, name: &str) -> String {
    let reason = match error.kind() {
        io::ErrorKind::NotFound => "it no longer exists".to_owned(),
        io::ErrorKind::PermissionDenied => "permission denied".to_owned(),
        _ => error.to_string(),
    };
    format!("Can't read the properties of “{name}”: {reason}.")
}

/// Read the properties of `path` without following a final symlink.
pub fn read(path: &Path) -> Result<Properties, String> {
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let meta = fs::symlink_metadata(path).map_err(|error| explain(&error, &name))?;
    let file_type = meta.file_type();
    let (kind, size) = if file_type.is_symlink() {
        let target = fs::read_link(path).ok();
        (Kind::Link { target }, Some(meta.len()))
    } else if file_type.is_dir() {
        (Kind::Folder, None)
    } else if file_type.is_file() {
        (Kind::File, Some(meta.len()))
    } else {
        (Kind::Other, None)
    };
    let location = path.parent().map_or_else(PathBuf::new, Path::to_path_buf);
    Ok(Properties {
        name,
        location,
        kind,
        size,
        modified: meta.modified().ok(),
        mode: std::os::unix::fs::PermissionsExt::mode(&meta.permissions()) & 0o777,
    })
}

/// `rwxr-x---` for permission bits `mode`.
pub fn permission_text(mode: u32) -> String {
    let mut text = String::with_capacity(9);
    for shift in [6, 3, 0] {
        let bits = (mode >> shift) & 0o7;
        text.push(if bits & 0o4 != 0 { 'r' } else { '-' });
        text.push(if bits & 0o2 != 0 { 'w' } else { '-' });
        text.push(if bits & 0o1 != 0 { 'x' } else { '-' });
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_a_file_and_a_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("notes.txt");
        fs::write(&file, b"hello").unwrap();
        fs::create_dir(tmp.path().join("docs")).unwrap();
        let props = read(&file).unwrap();
        assert_eq!(props.name, "notes.txt");
        assert_eq!(props.kind, Kind::File);
        assert_eq!(props.size, Some(5));
        assert_eq!(props.location, tmp.path());
        assert!(props.modified.is_some());
        let folder = read(&tmp.path().join("docs")).unwrap();
        assert_eq!(folder.kind, Kind::Folder);
        assert_eq!(folder.size, None, "folder contents are not counted");
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_describes_the_link_and_never_its_target() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("real");
        fs::write(&target, b"0123456789").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        let link = tmp.path().join("alias");
        symlink(&target, &link).unwrap();
        let props = read(&link).unwrap();
        assert_eq!(
            props.kind,
            Kind::Link {
                target: Some(target.clone())
            }
        );
        // The link's own size, not the 10-byte target's, and its own mode.
        assert_ne!(props.size, Some(10));
        assert_ne!(props.mode, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn permission_bits_are_reported() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("p");
        fs::write(&file, b"").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o640)).unwrap();
        let props = read(&file).unwrap();
        assert_eq!(props.mode, 0o640);
        assert_eq!(permission_text(props.mode), "rw-r-----");
    }

    #[test]
    fn a_missing_entry_gets_a_clear_error_not_a_guess() {
        let tmp = tempfile::tempdir().unwrap();
        let error = read(&tmp.path().join("gone.txt")).unwrap_err();
        assert_eq!(
            error,
            "Can't read the properties of “gone.txt”: it no longer exists."
        );
    }

    #[test]
    fn permission_errors_are_named_as_such() {
        let error = io::Error::from(io::ErrorKind::PermissionDenied);
        assert!(explain(&error, "secret").ends_with("permission denied."));
    }

    #[test]
    fn permission_text_covers_all_nine_bits() {
        assert_eq!(permission_text(0o755), "rwxr-xr-x");
        assert_eq!(permission_text(0o000), "---------");
        assert_eq!(permission_text(0o421), "r---w---x");
    }
}
