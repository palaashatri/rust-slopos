//! Content types and default applications (freedesktop shared-mime-info
//! and the MIME Applications Associations spec).
//!
//! Types come from file-name globs in shared-mime-info's `globs2` files
//! (highest weight, then longest pattern wins), falling back to a small
//! built-in table when the database is absent. Content sniffing is not done.
//! The default application for a type comes from `mimeapps.list`
//! `[Default Applications]` in the XDG config then data directories, then
//! from any installed `.desktop` entry that lists the type.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::desktop_entry::{find_desktop_app, scan_desktop_apps, DesktopApp};

pub const DIRECTORY: &str = "inode/directory";
pub const UNKNOWN: &str = "application/octet-stream";

#[derive(Debug, Clone, PartialEq, Eq)]
struct Glob {
    weight: u32,
    mime: String,
    pattern: String,
}

/// Parse one shared-mime-info `globs2` file (`weight:type:glob[:flags]`).
fn parse_globs2(content: &str) -> Vec<Glob> {
    content
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| {
            let mut parts = line.splitn(4, ':');
            let weight = parts.next()?.parse().ok()?;
            let mime = parts.next()?.to_owned();
            let pattern = parts.next()?.to_owned();
            (!mime.is_empty() && !pattern.is_empty()).then_some(Glob {
                weight,
                mime,
                pattern,
            })
        })
        .collect()
}

/// `*.ext`-style and literal globs; other wildcards are compared literally
/// after the leading `*`, which covers the shared-mime-info database.
fn glob_matches(pattern: &str, name: &str) -> bool {
    match pattern.strip_prefix('*') {
        Some(suffix) if !suffix.contains(['*', '?', '[']) => {
            name.len() >= suffix.len()
                && name[name.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
        }
        Some(_) => false,
        None => pattern == name,
    }
}

fn data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    match std::env::var_os("XDG_DATA_HOME") {
        Some(home) if !home.is_empty() => dirs.push(PathBuf::from(home)),
        _ => {
            if let Some(home) = std::env::var_os("HOME") {
                dirs.push(PathBuf::from(home).join(".local/share"));
            }
        }
    }
    let system = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    dirs.extend(system.split(':').map(PathBuf::from));
    dirs
}

/// Globs indexed for speed: simple `*.ext` patterns by lower-case
/// extension, everything else in a short list.
struct Database {
    by_extension: HashMap<String, Glob>,
    others: Vec<Glob>,
}

impl Database {
    fn new(globs: Vec<Glob>) -> Self {
        let mut by_extension: HashMap<String, Glob> = HashMap::new();
        let mut others = Vec::new();
        for glob in globs {
            let simple = glob
                .pattern
                .strip_prefix("*.")
                .filter(|ext| !ext.contains(['*', '?', '[', '.']))
                .map(str::to_lowercase);
            match simple {
                Some(ext) => {
                    let keep = by_extension
                        .get(&ext)
                        .is_none_or(|old| glob.weight > old.weight);
                    if keep {
                        by_extension.insert(ext, glob);
                    }
                }
                None => others.push(glob),
            }
        }
        Self {
            by_extension,
            others,
        }
    }

    fn guess(&self, name: &str) -> String {
        let extension = name
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_lowercase())
            .and_then(|ext| self.by_extension.get(&ext));
        let other = self
            .others
            .iter()
            .filter(|glob| glob_matches(&glob.pattern, name))
            .max_by_key(|glob| (glob.weight, glob.pattern.len()));
        let best = match (extension, other) {
            (Some(a), Some(b)) => {
                if (b.weight, b.pattern.len()) > (a.weight, a.pattern.len()) {
                    Some(b)
                } else {
                    Some(a)
                }
            }
            (a, b) => a.or(b),
        };
        match best {
            Some(glob) => glob.mime.clone(),
            None => built_in(name),
        }
    }
}

fn database() -> &'static Database {
    static DATABASE: OnceLock<Database> = OnceLock::new();
    DATABASE.get_or_init(|| {
        let mut globs = Vec::new();
        for dir in data_dirs() {
            if let Ok(content) = fs::read_to_string(dir.join("mime/globs2")) {
                globs.extend(parse_globs2(&content));
            }
        }
        Database::new(globs)
    })
}

const BUILT_IN: &[(&str, &str)] = &[
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("bmp", "image/bmp"),
    ("webp", "image/webp"),
    ("svg", "image/svg+xml"),
    ("tif", "image/tiff"),
    ("tiff", "image/tiff"),
    ("txt", "text/plain"),
    ("md", "text/markdown"),
    ("html", "text/html"),
    ("pdf", "application/pdf"),
    ("mp3", "audio/mpeg"),
    ("ogg", "audio/ogg"),
    ("flac", "audio/flac"),
    ("wav", "audio/x-wav"),
    ("mp4", "video/mp4"),
    ("mkv", "video/x-matroska"),
    ("webm", "video/webm"),
    ("srt", "application/x-subrip"),
    ("zip", "application/zip"),
    ("appimage", "application/vnd.appimage"),
];

#[cfg(test)]
fn guess_with(globs: &[Glob], name: &str) -> String {
    Database::new(globs.to_vec()).guess(name)
}

fn built_in(name: &str) -> String {
    let extension = Path::new(name)
        .extension()
        .map(|ext| ext.to_string_lossy().to_lowercase());
    extension
        .and_then(|ext| {
            BUILT_IN
                .iter()
                .find(|(known, _)| *known == ext)
                .map(|(_, mime)| (*mime).to_owned())
        })
        .unwrap_or_else(|| UNKNOWN.to_owned())
}

/// The content type of a file name (no file access).
pub fn guess_for_name(name: &str) -> String {
    database().guess(name)
}

/// The content type of `path`: directories are `inode/directory`, other
/// files are typed by name.
pub fn guess(path: &Path) -> String {
    if fs::metadata(path).is_ok_and(|meta| meta.is_dir()) {
        return DIRECTORY.to_owned();
    }
    path.file_name()
        .map(|name| guess_for_name(&name.to_string_lossy()))
        .unwrap_or_else(|| UNKNOWN.to_owned())
}

/// A short human description of a type, for Kind columns.
pub fn describe(mime: &str) -> String {
    match mime {
        DIRECTORY => "Folder".into(),
        UNKNOWN => "Document".into(),
        "application/pdf" => "PDF document".into(),
        "application/zip" => "Zip archive".into(),
        "application/vnd.appimage" => "AppImage application".into(),
        _ => {
            let (major, minor) = mime.split_once('/').unwrap_or((mime, ""));
            let minor = minor
                .trim_start_matches("x-")
                .trim_start_matches("vnd.")
                .split(['+', '.'])
                .next()
                .unwrap_or(minor)
                .to_uppercase();
            match major {
                "image" => format!("{minor} image"),
                "audio" => format!("{minor} audio"),
                "video" => format!("{minor} video"),
                "text" if minor == "PLAIN" => "Plain text".into(),
                "text" => format!("{minor} text"),
                _ => format!("{minor} document"),
            }
        }
    }
}

/// Parse the `[Default Applications]` group of a `mimeapps.list`.
fn parse_defaults(content: &str) -> HashMap<String, Vec<String>> {
    let mut defaults = HashMap::new();
    let mut in_group = false;
    for line in content.lines().map(str::trim) {
        if line.starts_with('[') {
            in_group = line == "[Default Applications]";
            continue;
        }
        if !in_group {
            continue;
        }
        if let Some((mime, apps)) = line.split_once('=') {
            let apps = apps
                .split(';')
                .map(|app| app.trim().trim_end_matches(".desktop").to_owned())
                .filter(|app| !app.is_empty())
                .collect();
            defaults.entry(mime.trim().to_owned()).or_insert(apps);
        }
    }
    defaults
}

fn mimeapps_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    if let Some(config) = config_home {
        files.push(config.join("slopos-mimeapps.list"));
        files.push(config.join("mimeapps.list"));
    }
    let config_dirs = std::env::var("XDG_CONFIG_DIRS").unwrap_or_else(|_| "/etc/xdg".into());
    files.extend(
        config_dirs
            .split(':')
            .map(|dir| PathBuf::from(dir).join("mimeapps.list")),
    );
    for dir in data_dirs() {
        files.push(dir.join("applications/slopos-mimeapps.list"));
        files.push(dir.join("applications/mimeapps.list"));
    }
    files
}

/// Default-app candidates for `mime` in preference order, from the given
/// `mimeapps.list` contents and installed apps.
fn candidates(mime: &str, lists: &[String], installed: &[DesktopApp]) -> Vec<String> {
    let mut ids = Vec::new();
    for list in lists {
        if let Some(apps) = parse_defaults(list).get(mime) {
            ids.extend(apps.iter().cloned());
        }
    }
    // SLOPOS's own apps first among the entries that merely declare a type.
    let mut declared: Vec<&DesktopApp> = installed
        .iter()
        .filter(|app| app.mime_types.iter().any(|known| known == mime))
        .collect();
    declared.sort_by_key(|app| !app.id.starts_with("slopos-"));
    ids.extend(declared.into_iter().map(|app| app.id.clone()));
    let mut seen = std::collections::HashSet::new();
    ids.retain(|id| seen.insert(id.clone()));
    ids
}

/// The installed application that should open `mime`, if any. Text
/// subtypes fall back to `text/plain`.
pub fn default_app(mime: &str) -> Option<DesktopApp> {
    let lists: Vec<String> = mimeapps_files()
        .iter()
        .filter_map(|path| fs::read_to_string(path).ok())
        .collect();
    let installed = scan_desktop_apps();
    let mut types = vec![mime.to_owned()];
    if mime.starts_with("text/") && mime != "text/plain" {
        types.push("text/plain".into());
    }
    for kind in types {
        for id in candidates(&kind, &lists, &installed) {
            if let Some(app) = installed
                .iter()
                .find(|app| app.id == id)
                .cloned()
                .or_else(|| find_desktop_app(&id))
            {
                return Some(app);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, mime: &[&str]) -> DesktopApp {
        DesktopApp {
            id: id.into(),
            name: id.into(),
            argv: vec!["true".into()],
            icon: String::new(),
            comment: String::new(),
            terminal: false,
            startup_wm_class: String::new(),
            mime_types: mime.iter().map(|m| (*m).into()).collect(),
            exec: "true %F".into(),
        }
    }

    #[test]
    fn globs_prefer_weight_then_length_and_ignore_case() {
        let globs = parse_globs2(
            "# comment\n50:text/plain:*.txt\n50:application/x-compressed-tar:*.tar.gz\n\
             50:application/gzip:*.gz\n80:text/x-makefile:Makefile\nbad line\n",
        );
        assert_eq!(globs.len(), 4);
        assert_eq!(guess_with(&globs, "NOTES.TXT"), "text/plain");
        assert_eq!(
            guess_with(&globs, "a.tar.gz"),
            "application/x-compressed-tar"
        );
        assert_eq!(guess_with(&globs, "Makefile"), "text/x-makefile");
        assert_eq!(guess_with(&globs, "photo.JPG"), "image/jpeg");
        assert_eq!(guess_with(&globs, "no-extension"), UNKNOWN);
    }

    #[test]
    fn system_database_types_common_files() {
        // Uses /usr/share/mime when installed, else the built-in table.
        assert_eq!(guess_for_name("x.png"), "image/png");
        assert_eq!(guess_for_name("x.jpeg"), "image/jpeg");
        assert_eq!(guess(Path::new("/")), DIRECTORY);
    }

    #[test]
    fn mimeapps_defaults_win_over_declared_types() {
        let lists = vec![
            "[Added Associations]\nimage/png=other.desktop\n[Default Applications]\nimage/png=viewer.desktop;fallback.desktop\n".to_owned(),
            "[Default Applications]\nimage/png=late.desktop\n".to_owned(),
        ];
        let installed = vec![
            app("gimp", &["image/png"]),
            app("slopos-image-viewer", &["image/png"]),
        ];
        assert_eq!(
            candidates("image/png", &lists, &installed),
            ["viewer", "fallback", "late", "slopos-image-viewer", "gimp"]
        );
        assert_eq!(
            candidates("video/mp4", &lists, &installed),
            Vec::<String>::new()
        );
    }

    #[test]
    fn descriptions_are_readable() {
        assert_eq!(describe("image/png"), "PNG image");
        assert_eq!(describe(DIRECTORY), "Folder");
        assert_eq!(describe("text/plain"), "Plain text");
        assert_eq!(describe("image/svg+xml"), "SVG image");
        assert_eq!(describe("video/x-matroska"), "MATROSKA video");
    }

    #[test]
    fn exec_expansion_places_files_without_a_shell() {
        let mut viewer = app("v", &[]);
        viewer.exec = "viewer --open %f --flag".into();
        let a = Path::new("/tmp/a b.png");
        let b = Path::new("/tmp/c;rm -rf.png");
        assert_eq!(
            viewer.command_for(&[a, b]).unwrap(),
            ["viewer", "--open", "/tmp/a b.png", "--flag"]
        );
        viewer.exec = "\"my editor\" %U".into();
        assert_eq!(
            viewer.command_for(&[a, b]).unwrap(),
            ["my editor", "/tmp/a b.png", "/tmp/c;rm -rf.png"]
        );
        viewer.exec = "player".into();
        assert_eq!(
            viewer.command_for(&[a]).unwrap(),
            ["player", "/tmp/a b.png"]
        );
        viewer.exec = "bad \"quote".into();
        assert_eq!(viewer.command_for(&[a]), None);
    }
}
