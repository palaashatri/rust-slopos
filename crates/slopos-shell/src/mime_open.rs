//! Pure MIME / FreeDesktop `.desktop` open helpers.
//!
//! No process execution — builds argv plans and registry lookups only.
//! Field-code expansion follows the FreeDesktop Desktop Entry Spec Exec keys
//! for the minimal set: `%f` `%F` `%u` `%U` `%i` `%c` `%%`.
//!
//! Live spawn of an [`OpenPlan`] lives in [`crate::session_clients::spawn_open_plan`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A registered application that can open one or more MIME types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopAppEntry {
    pub id: String,
    pub name: String,
    pub exec: String,
    pub mimetypes: Vec<String>,
}

/// Planned argv for opening a path with a chosen app (not executed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenPlan {
    pub app_id: String,
    pub argv: Vec<String>,
}

/// In-memory registry of desktop app entries and default MIME handlers.
#[derive(Debug, Default, Clone)]
pub struct MimeOpenRegistry {
    apps: HashMap<String, DesktopAppEntry>,
    /// Explicit default app id per MIME pattern (`text/plain`, `text/*`, …).
    defaults: HashMap<String, String>,
}

impl MimeOpenRegistry {
    pub fn new() -> Self {
        Self {
            apps: HashMap::new(),
            defaults: HashMap::new(),
        }
    }

    /// Register (or replace) an app entry. First registration for each of its
    /// MIME types becomes the default if none is set yet.
    pub fn register(&mut self, entry: DesktopAppEntry) {
        for mime in &entry.mimetypes {
            self.defaults
                .entry(mime.clone())
                .or_insert_with(|| entry.id.clone());
        }
        self.apps.insert(entry.id.clone(), entry);
    }

    /// App ids that claim `mime` (exact or `type/*` wildcard).
    pub fn find_handlers(&self, mime: &str) -> Vec<String> {
        let mut ids: Vec<String> = self
            .apps
            .values()
            .filter(|app| app.mimetypes.iter().any(|p| mime_matches(p, mime)))
            .map(|app| app.id.clone())
            .collect();
        ids.sort();
        ids.dedup();
        ids
    }

    /// Default handler for `mime`, preferring exact then `type/*` defaults,
    /// then the first registered handler.
    pub fn pick_default(&self, mime: &str) -> Option<String> {
        if let Some(id) = self.defaults.get(mime) {
            return Some(id.clone());
        }
        if let Some((major, _)) = mime.split_once('/') {
            let wild = format!("{major}/*");
            if let Some(id) = self.defaults.get(&wild) {
                return Some(id.clone());
            }
        }
        self.find_handlers(mime).into_iter().next()
    }

    pub fn get(&self, id: &str) -> Option<&DesktopAppEntry> {
        self.apps.get(id)
    }
}

/// Whether a MIME pattern (possibly `type/*`) matches a concrete MIME type.
fn mime_matches(pattern: &str, mime: &str) -> bool {
    if pattern == mime {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix("/*") {
        if let Some((major, rest)) = mime.split_once('/') {
            return major == prefix && !rest.is_empty();
        }
    }
    false
}

/// Parse a FreeDesktop `Exec=` value into argv, expanding field codes with
/// `path_arg` as the single file/URL payload.
///
/// Supported codes:
/// - `%f` / `%F` → local file path (`path_arg`)
/// - `%u` / `%U` → URI form of `path_arg` (`file://…` when not already a URI)
/// - `%i` → dropped (no Icon key available in this pure helper)
/// - `%c` → dropped (Name is not passed into this pure expander)
/// - `%%` → literal `%`
///
/// If none of `%f`/`%F`/`%u`/`%U` appear, `path_arg` is appended so open plans
/// still deliver the path to the binary.
pub fn parse_desktop_exec(exec_line: &str, path_arg: &str) -> Vec<String> {
    let tokens = tokenize_exec(exec_line);
    let mut out = Vec::new();
    let mut used_file_code = false;

    for token in tokens {
        match expand_token(&token, path_arg, &mut used_file_code) {
            ExpandResult::Drop => {}
            ExpandResult::One(s) => out.push(s),
            ExpandResult::Many(parts) => out.extend(parts),
        }
    }

    if !used_file_code && !path_arg.is_empty() {
        out.push(path_arg.to_string());
    }

    out
}

enum ExpandResult {
    Drop,
    One(String),
    #[allow(dead_code)]
    Many(Vec<String>),
}

fn expand_token(token: &str, path_arg: &str, used_file_code: &mut bool) -> ExpandResult {
    // Bare field codes as whole arguments.
    match token {
        "%f" | "%F" => {
            *used_file_code = true;
            return ExpandResult::One(path_arg.to_string());
        }
        "%u" | "%U" => {
            *used_file_code = true;
            return ExpandResult::One(path_as_uri(path_arg));
        }
        "%i" | "%c" => return ExpandResult::Drop,
        "%%" => return ExpandResult::One("%".to_string()),
        _ => {}
    }

    // Embedded field codes inside a larger argument (e.g. `--file=%f`, `100%%`).
    let mut result = String::with_capacity(token.len());
    let mut chars = token.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            result.push(c);
            continue;
        }
        match chars.next() {
            Some('%') => result.push('%'),
            Some('f') | Some('F') => {
                *used_file_code = true;
                result.push_str(path_arg);
            }
            Some('u') | Some('U') => {
                *used_file_code = true;
                result.push_str(&path_as_uri(path_arg));
            }
            Some('i') | Some('c') => {
                // No icon/name available: expand to empty substring.
            }
            Some(other) => {
                // Unknown field code: keep literally as `%X` (spec: reserved).
                result.push('%');
                result.push(other);
            }
            None => result.push('%'),
        }
    }
    ExpandResult::One(result)
}

/// Convert a path to a URI for `%u` / `%U`. Paths that already look like a
/// URI (contain `://`) are returned unchanged; otherwise `file://` is prefixed.
fn path_as_uri(path_arg: &str) -> String {
    if path_arg.contains("://") {
        path_arg.to_string()
    } else {
        format!("file://{path_arg}")
    }
}

/// Minimal FreeDesktop-style Exec tokenization: split on unquoted whitespace,
/// honor double quotes and backslash escapes inside quotes.
fn tokenize_exec(exec_line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = exec_line.chars().peekable();

    while let Some(c) = chars.next() {
        if in_quotes {
            match c {
                '"' => in_quotes = false,
                '\\' => {
                    if let Some(next) = chars.next() {
                        current.push(next);
                    }
                }
                _ => current.push(c),
            }
            continue;
        }
        match c {
            '"' => in_quotes = true,
            c if c.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            '\\' => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            _ => current.push(c),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Map a filesystem path to a MIME type via a small extension table.
///
/// - Directories → `inode/directory`
/// - `.txt` / `.text` / `.md` → `text/plain`
/// - `.png` → `image/png`
/// - `.pdf` → `application/pdf`
/// - otherwise → `application/octet-stream`
pub fn mime_from_path(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    if path.is_dir() {
        return "inode/directory".to_string();
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    match ext.as_deref() {
        Some("txt") | Some("text") | Some("md") => "text/plain".to_string(),
        Some("png") => "image/png".to_string(),
        Some("pdf") => "application/pdf".to_string(),
        _ => "application/octet-stream".to_string(),
    }
}

/// Seed built-in SLOPOS-I handlers: TextEdit for `text/*`, Finder for directories.
pub fn seed_slopos_defaults(registry: &mut MimeOpenRegistry) {
    registry.register(DesktopAppEntry {
        id: "com.slopos.textedit".to_string(),
        name: "TextEdit".to_string(),
        exec: "retro-textedit %f".to_string(),
        mimetypes: vec!["text/*".to_string(), "text/plain".to_string()],
    });
    registry.register(DesktopAppEntry {
        id: "com.slopos.finder".to_string(),
        name: "Finder".to_string(),
        exec: "retro-finder %f".to_string(),
        mimetypes: vec!["inode/directory".to_string()],
    });
}

/// Build an open plan for `path` using the registry default handler.
pub fn open_plan(registry: &MimeOpenRegistry, path: impl AsRef<Path>) -> Result<OpenPlan, String> {
    let path = path.as_ref();
    let path_str = path.to_string_lossy();
    if path_str.is_empty() {
        return Err("path must be non-empty".to_string());
    }

    let mime = mime_from_path(path);
    let app_id = registry
        .pick_default(&mime)
        .ok_or_else(|| format!("no handler for mime type '{mime}'"))?;

    let entry = registry
        .get(&app_id)
        .ok_or_else(|| format!("registered default app '{app_id}' is missing"))?;

    let argv = parse_desktop_exec(&entry.exec, path_str.as_ref());
    if argv.is_empty() {
        return Err(format!("empty Exec for app '{app_id}'"));
    }

    Ok(OpenPlan { app_id, argv })
}

/// First-party binary name for a SLOPOS-I app id (spawn-ready, not Exec= name).
pub fn first_party_binary_for_app_id(app_id: &str) -> Option<&'static str> {
    match app_id {
        "com.slopos.textedit" => Some("textedit"),
        "com.slopos.finder" => Some("finder"),
        "com.slopos.settings" => Some("settings"),
        "com.slopos.terminal" => Some("terminal"),
        "com.slopos.appstore" => Some("appstore"),
        _ => None,
    }
}

/// Pure spawn argv for an [`OpenPlan`].
///
/// Remaps known SLOPOS-I app ids to on-disk first-party binary names
/// (`retro-textedit` Exec → `textedit`). Unknown apps keep `plan.argv` as-is.
pub fn spawn_argv(plan: &OpenPlan) -> Vec<String> {
    let mut argv = plan.argv.clone();
    if argv.is_empty() {
        return argv;
    }
    if let Some(bin) = first_party_binary_for_app_id(&plan.app_id) {
        argv[0] = bin.to_string();
    }
    argv
}

/// Pure: filesystem path from a `file:` URI.
///
/// Accepts `file:///abs`, `file://localhost/abs`, and `file:/abs`.
/// Minimal percent-decoding for path segments (`%20` → space).
pub fn path_from_file_uri(uri: &str) -> Result<PathBuf, String> {
    let uri = uri.trim();
    if uri.is_empty() {
        return Err("empty URI".to_string());
    }
    let rest = uri
        .strip_prefix("file:")
        .or_else(|| uri.strip_prefix("FILE:"))
        .ok_or_else(|| "not a file URI".to_string())?;

    // file:///path  |  file://localhost/path  |  file:/path  |  file://path
    let path_part = if let Some(after) = rest.strip_prefix("//") {
        // authority + path
        if after.starts_with('/') {
            // file:///tmp/x → after = "/tmp/x"
            after
        } else if let Some(slash) = after.find('/') {
            let authority = &after[..slash];
            if authority.is_empty() || authority.eq_ignore_ascii_case("localhost") {
                &after[slash..]
            } else {
                return Err(format!("unsupported file URI authority: {authority}"));
            }
        } else {
            return Err("file URI missing path".to_string());
        }
    } else if rest.starts_with('/') {
        // file:/tmp/x
        rest
    } else {
        return Err("file URI missing path".to_string());
    };

    let decoded = percent_decode_path(path_part);
    if decoded.is_empty() {
        return Err("file URI path is empty".to_string());
    }
    Ok(PathBuf::from(decoded))
}

/// Pure open plan for a validated (or raw) `file://` URI using the registry.
pub fn open_plan_for_file_uri(registry: &MimeOpenRegistry, uri: &str) -> Result<OpenPlan, String> {
    let path = path_from_file_uri(uri)?;
    open_plan(registry, path)
}

fn percent_decode_path(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let h1 = bytes[i + 1];
            let h2 = bytes[i + 2];
            if let (Some(a), Some(b)) = (from_hex(h1), from_hex(h2)) {
                out.push((a << 4) | b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn from_hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // ── parse_desktop_exec: %f / %F ─────────────────────────────────────────

    #[test]
    fn parse_exec_percent_f_replaces_with_path() {
        let argv = parse_desktop_exec("editor %f", "/tmp/note.txt");
        assert_eq!(argv, vec!["editor", "/tmp/note.txt"]);
    }

    #[test]
    fn parse_exec_percent_f_list_replaces_with_path() {
        let argv = parse_desktop_exec("editor %F", "/tmp/a.txt");
        assert_eq!(argv, vec!["editor", "/tmp/a.txt"]);
    }

    #[test]
    fn parse_exec_embedded_percent_f() {
        let argv = parse_desktop_exec("viewer --file=%f", "/docs/x.pdf");
        assert_eq!(argv, vec!["viewer", "--file=/docs/x.pdf"]);
    }

    #[test]
    fn parse_exec_percent_f_with_quoted_binary() {
        let argv = parse_desktop_exec(r#""/opt/My App/bin" %f"#, "/tmp/f");
        assert_eq!(argv, vec!["/opt/My App/bin", "/tmp/f"]);
    }

    // ── parse_desktop_exec: %u / %U ─────────────────────────────────────────

    #[test]
    fn parse_exec_percent_u_file_path_becomes_file_uri() {
        let argv = parse_desktop_exec("browser %u", "/tmp/page.html");
        assert_eq!(argv, vec!["browser", "file:///tmp/page.html"]);
    }

    #[test]
    fn parse_exec_percent_u_list_same_as_single_uri() {
        let argv = parse_desktop_exec("browser %U", "/tmp/page.html");
        assert_eq!(argv, vec!["browser", "file:///tmp/page.html"]);
    }

    #[test]
    fn parse_exec_percent_u_preserves_existing_uri() {
        let argv = parse_desktop_exec("browser %u", "https://example.com/a");
        assert_eq!(argv, vec!["browser", "https://example.com/a"]);
    }

    #[test]
    fn parse_exec_embedded_percent_u() {
        let argv = parse_desktop_exec("open --url=%u", "/tmp/x");
        assert_eq!(argv, vec!["open", "--url=file:///tmp/x"]);
    }

    // ── parse_desktop_exec: %% ──────────────────────────────────────────────

    #[test]
    fn parse_exec_double_percent_is_literal_percent() {
        let argv = parse_desktop_exec("tool 100%%", "/tmp/x");
        // No file field code → path appended.
        assert_eq!(argv, vec!["tool", "100%", "/tmp/x"]);
    }

    #[test]
    fn parse_exec_bare_double_percent_token() {
        let argv = parse_desktop_exec("tool %% %f", "/p");
        assert_eq!(argv, vec!["tool", "%", "/p"]);
    }

    #[test]
    fn parse_exec_percent_percent_with_file_code() {
        let argv = parse_desktop_exec("tool --pct=50%% %f", "/file");
        assert_eq!(argv, vec!["tool", "--pct=50%", "/file"]);
    }

    // ── parse_desktop_exec: %i / %c ─────────────────────────────────────────

    #[test]
    fn parse_exec_percent_i_and_c_dropped() {
        let argv = parse_desktop_exec("app %i %c %f", "/tmp/x");
        assert_eq!(argv, vec!["app", "/tmp/x"]);
    }

    // ── parse_desktop_exec: no field codes ──────────────────────────────────

    #[test]
    fn parse_exec_appends_path_when_no_file_code() {
        let argv = parse_desktop_exec("retro-textedit", "/tmp/a.txt");
        assert_eq!(argv, vec!["retro-textedit", "/tmp/a.txt"]);
    }

    #[test]
    fn parse_exec_empty_path_no_append() {
        let argv = parse_desktop_exec("tool", "");
        assert_eq!(argv, vec!["tool"]);
    }

    // ── mime_from_path ──────────────────────────────────────────────────────

    #[test]
    fn mime_map_txt_is_text_plain() {
        assert_eq!(mime_from_path("/tmp/readme.txt"), "text/plain");
        assert_eq!(mime_from_path("/tmp/README.TXT"), "text/plain");
        assert_eq!(mime_from_path("notes.text"), "text/plain");
        assert_eq!(mime_from_path("doc.md"), "text/plain");
    }

    #[test]
    fn mime_map_png_is_image_png() {
        assert_eq!(mime_from_path("/img/a.png"), "image/png");
        assert_eq!(mime_from_path("B.PNG"), "image/png");
    }

    #[test]
    fn mime_map_pdf_is_application_pdf() {
        assert_eq!(mime_from_path("/docs/spec.pdf"), "application/pdf");
        assert_eq!(mime_from_path("X.PDF"), "application/pdf");
    }

    #[test]
    fn mime_map_unknown_is_octet_stream() {
        assert_eq!(mime_from_path("/tmp/blob.bin"), "application/octet-stream");
        assert_eq!(mime_from_path("/tmp/noext"), "application/octet-stream");
        assert_eq!(mime_from_path("archive.xyz"), "application/octet-stream");
    }

    #[test]
    fn mime_map_directory_is_inode_directory() {
        // std::env::temp_dir() is always a real directory on supported platforms.
        let dir = std::env::temp_dir();
        assert!(dir.is_dir(), "temp_dir should exist");
        assert_eq!(mime_from_path(&dir), "inode/directory");
    }

    // ── registry / open_plan ────────────────────────────────────────────────

    #[test]
    fn seed_defaults_textedit_handles_text() {
        let mut reg = MimeOpenRegistry::new();
        seed_slopos_defaults(&mut reg);

        let handlers = reg.find_handlers("text/plain");
        assert!(handlers.contains(&"com.slopos.textedit".to_string()));
        assert_eq!(
            reg.pick_default("text/plain").as_deref(),
            Some("com.slopos.textedit")
        );
        assert_eq!(
            reg.pick_default("text/html").as_deref(),
            Some("com.slopos.textedit")
        );
    }

    #[test]
    fn seed_defaults_finder_handles_directory() {
        let mut reg = MimeOpenRegistry::new();
        seed_slopos_defaults(&mut reg);
        assert_eq!(
            reg.pick_default("inode/directory").as_deref(),
            Some("com.slopos.finder")
        );
    }

    #[test]
    fn open_plan_text_file_uses_textedit_argv() {
        let mut reg = MimeOpenRegistry::new();
        seed_slopos_defaults(&mut reg);
        let plan = open_plan(&reg, PathBuf::from("/tmp/hello.txt")).unwrap();
        assert_eq!(plan.app_id, "com.slopos.textedit");
        assert_eq!(plan.argv, vec!["retro-textedit", "/tmp/hello.txt"]);
    }

    #[test]
    fn open_plan_spawn_argv_text_file_is_first_party_binary() {
        let mut reg = MimeOpenRegistry::new();
        seed_slopos_defaults(&mut reg);
        let plan = open_plan(&reg, PathBuf::from("/tmp/hello.txt")).unwrap();
        // Pure spawn argv: Exec name remapped to on-disk binary (no process).
        assert_eq!(spawn_argv(&plan), vec!["textedit", "/tmp/hello.txt"]);
        assert_eq!(
            first_party_binary_for_app_id(&plan.app_id),
            Some("textedit")
        );
    }

    #[test]
    fn open_plan_spawn_argv_directory_is_finder() {
        let mut reg = MimeOpenRegistry::new();
        seed_slopos_defaults(&mut reg);
        let dir = std::env::temp_dir();
        let plan = open_plan(&reg, &dir).unwrap();
        let argv = spawn_argv(&plan);
        assert_eq!(argv[0], "finder");
        assert_eq!(argv[1], dir.to_string_lossy().as_ref());
    }

    #[test]
    fn open_plan_directory_uses_finder() {
        let mut reg = MimeOpenRegistry::new();
        seed_slopos_defaults(&mut reg);
        let dir = std::env::temp_dir();
        let plan = open_plan(&reg, &dir).unwrap();
        assert_eq!(plan.app_id, "com.slopos.finder");
        assert_eq!(plan.argv[0], "retro-finder");
        assert_eq!(plan.argv[1], dir.to_string_lossy().as_ref());
    }

    #[test]
    fn open_plan_errors_without_handler() {
        let reg = MimeOpenRegistry::new();
        let err = open_plan(&reg, PathBuf::from("/tmp/x.bin")).unwrap_err();
        assert!(err.contains("no handler"), "{err}");
    }

    #[test]
    fn path_from_file_uri_triple_slash() {
        assert_eq!(
            path_from_file_uri("file:///tmp/doc.txt").unwrap(),
            PathBuf::from("/tmp/doc.txt")
        );
    }

    #[test]
    fn path_from_file_uri_localhost_and_percent() {
        assert_eq!(
            path_from_file_uri("file://localhost/tmp/hello%20world.txt").unwrap(),
            PathBuf::from("/tmp/hello world.txt")
        );
        assert_eq!(
            path_from_file_uri("file:/tmp/a.txt").unwrap(),
            PathBuf::from("/tmp/a.txt")
        );
    }

    #[test]
    fn path_from_file_uri_rejects_non_file() {
        assert!(path_from_file_uri("https://example.com/x").is_err());
        assert!(path_from_file_uri("").is_err());
    }

    #[test]
    fn open_plan_for_file_uri_text_uses_textedit_spawn_argv() {
        let mut reg = MimeOpenRegistry::new();
        seed_slopos_defaults(&mut reg);
        let plan = open_plan_for_file_uri(&reg, "file:///tmp/notes.md").unwrap();
        assert_eq!(plan.app_id, "com.slopos.textedit");
        assert_eq!(spawn_argv(&plan), vec!["textedit", "/tmp/notes.md"]);
    }

    #[test]
    fn find_handlers_empty_when_unregistered() {
        let reg = MimeOpenRegistry::new();
        assert!(reg.find_handlers("image/png").is_empty());
        assert!(reg.pick_default("image/png").is_none());
    }

    #[test]
    fn register_sets_first_default() {
        let mut reg = MimeOpenRegistry::new();
        reg.register(DesktopAppEntry {
            id: "app.a".into(),
            name: "A".into(),
            exec: "a %f".into(),
            mimetypes: vec!["image/png".into()],
        });
        reg.register(DesktopAppEntry {
            id: "app.b".into(),
            name: "B".into(),
            exec: "b %f".into(),
            mimetypes: vec!["image/png".into()],
        });
        assert_eq!(reg.pick_default("image/png").as_deref(), Some("app.a"));
        let handlers = reg.find_handlers("image/png");
        assert_eq!(handlers, vec!["app.a", "app.b"]);
    }
}
