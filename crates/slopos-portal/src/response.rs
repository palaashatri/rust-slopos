//! The chooser reply mapped onto the portal's response codes and results.
//!
//! Response codes (xdg-desktop-portal Request.Response): 0 success, 1 the
//! user cancelled, 2 the request ended for another reason.

use std::path::Path;

use crate::spec::ChooserReply;

pub const SUCCESS: u32 = 0;
pub const CANCELLED: u32 = 1;
pub const OTHER: u32 = 2;

/// The portal response code for `reply`.
pub fn response_code(reply: &ChooserReply) -> u32 {
    match reply {
        ChooserReply::Chosen { .. } => SUCCESS,
        ChooserReply::Cancelled => CANCELLED,
        ChooserReply::Failed { .. } => OTHER,
    }
}

/// The `uris` result for chosen paths: one `file://` URI per path. Paths
/// that are not absolute cannot be URIs and are left out.
pub fn uris(paths: &[impl AsRef<Path>]) -> Vec<String> {
    paths
        .iter()
        .filter_map(|path| file_uri(path.as_ref()))
        .collect()
}

/// `file://` URI for an absolute path, with RFC 3986 percent-encoding of
/// every byte outside the unreserved set and `/`.
pub fn file_uri(path: &Path) -> Option<String> {
    use std::os::unix::ffi::OsStrExt;
    if !path.is_absolute() {
        return None;
    }
    let mut uri = String::from("file://");
    for &byte in path.as_os_str().as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                uri.push(byte as char)
            }
            _ => uri.push_str(&format!("%{byte:02X}")),
        }
    }
    Some(uri)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn codes_follow_the_portal_specification() {
        assert_eq!(
            response_code(&ChooserReply::Chosen {
                paths: vec![PathBuf::from("/a")]
            }),
            SUCCESS
        );
        assert_eq!(response_code(&ChooserReply::Cancelled), CANCELLED);
        assert_eq!(
            response_code(&ChooserReply::Failed {
                message: "boom".into()
            }),
            OTHER
        );
    }

    #[test]
    fn uris_percent_encode_everything_but_unreserved_characters() {
        assert_eq!(
            file_uri(Path::new("/home/a b/ünï?#%.txt")).unwrap(),
            "file:///home/a%20b/%C3%BCn%C3%AF%3F%23%25.txt"
        );
        assert_eq!(file_uri(Path::new("/")).unwrap(), "file:///");
        assert_eq!(file_uri(Path::new("relative/path")), None);
    }

    #[test]
    fn only_absolute_paths_become_uris() {
        let uris = uris(&[PathBuf::from("/one"), PathBuf::from("two")]);
        assert_eq!(uris, ["file:///one"]);
    }
}
