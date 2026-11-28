//! The chooser request and reply, shared by the portal service and the
//! dialog program. Everything here is pure: no D-Bus, no window.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// What the dialog is asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// `OpenFile` (or `SaveFiles`' destination when used through `OpenFolder`).
    Open,
    /// `OpenFile` with `directory = true`.
    OpenFolder,
    /// `SaveFile`.
    Save,
}

/// One rule of a filter, as the portal describes it: a shell-style glob on
/// the file name, or a MIME type (`image/png`, or `image/*` for a family).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Pattern {
    Glob(String),
    Mime(String),
}

impl Pattern {
    /// True when a file called `file_name` matches this rule.
    ///
    /// Globs support `*`, `*.ext` (case-insensitive extension), `*suffix`
    /// and literal names. Other wildcard forms match nothing rather than
    /// guessing. MIME rules use the desktop's name-based type guess.
    pub fn matches(&self, file_name: &str) -> bool {
        match self {
            Self::Glob(glob) => glob_matches(glob, file_name),
            Self::Mime(mime) => mime_matches(mime, &slopos_core::mime::guess_for_name(file_name)),
        }
    }
}

fn glob_matches(glob: &str, name: &str) -> bool {
    if glob == "*" {
        return true;
    }
    if glob.contains(['?', '[']) {
        return false;
    }
    match glob.strip_prefix('*') {
        Some(suffix) if !suffix.contains('*') => {
            name.len() > suffix.len()
                && (name.ends_with(suffix)
                    || (suffix.starts_with('.')
                        && name.to_lowercase().ends_with(&suffix.to_lowercase())))
        }
        Some(_) => false,
        None => !glob.contains('*') && glob == name,
    }
}

fn mime_matches(rule: &str, mime: &str) -> bool {
    match rule.strip_suffix("/*") {
        Some(family) => mime.split_once('/').is_some_and(|(kind, _)| kind == family),
        None => rule == mime,
    }
}

/// A named filter: a file is shown when any of its patterns matches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilterSpec {
    pub name: String,
    pub patterns: Vec<Pattern>,
}

impl FilterSpec {
    pub fn accepts(&self, file_name: &str) -> bool {
        self.patterns
            .iter()
            .any(|pattern| pattern.matches(file_name))
    }
}

/// Everything the dialog needs to show one chooser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChooserSpec {
    pub kind: Kind,
    pub title: String,
    /// The folder to show first. `None` lets the dialog choose (home).
    pub folder: Option<PathBuf>,
    /// The suggested name for a save.
    pub name: Option<String>,
    pub multiple: bool,
    pub filters: Vec<FilterSpec>,
    /// Index into `filters` of the active filter.
    pub current_filter: Option<usize>,
    pub accept_label: Option<String>,
}

impl ChooserSpec {
    pub fn new(kind: Kind, title: &str) -> Self {
        Self {
            kind,
            title: title.to_owned(),
            folder: None,
            name: None,
            multiple: false,
            filters: Vec::new(),
            current_filter: None,
            accept_label: None,
        }
    }

    /// True when a file called `file_name` is shown under the active filter.
    /// With no filters every file is shown.
    pub fn accepts(&self, file_name: &str) -> bool {
        match self.active_filter() {
            Some(filter) => filter.accepts(file_name),
            None => true,
        }
    }

    pub fn active_filter(&self) -> Option<&FilterSpec> {
        self.filters.get(self.current_filter.unwrap_or(0))
    }
}

/// The dialog program's answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ChooserReply {
    Chosen { paths: Vec<PathBuf> },
    Cancelled,
    Failed { message: String },
}

/// The folder a request should start in: the requested folder if it is an
/// existing directory, otherwise `fallback`.
pub fn resolve_folder(requested: Option<&Path>, fallback: &Path) -> PathBuf {
    match requested {
        Some(path) if path.is_absolute() && path.is_dir() => path.to_path_buf(),
        _ => fallback.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_rules_cover_extensions_literals_and_star() {
        assert!(Pattern::Glob("*.png".into()).matches("photo.PNG"));
        assert!(!Pattern::Glob("*.png".into()).matches("photo.jpg"));
        assert!(
            !Pattern::Glob("*.png".into()).matches(".png"),
            "a bare dot name is not an extension"
        );
        assert!(Pattern::Glob("Makefile".into()).matches("Makefile"));
        assert!(!Pattern::Glob("Makefile".into()).matches("makefile"));
        assert!(Pattern::Glob("*".into()).matches("anything"));
        assert!(Pattern::Glob("*.tar.gz".into()).matches("backup.tar.gz"));
        assert!(
            !Pattern::Glob("a?c".into()).matches("abc"),
            "unsupported forms match nothing"
        );
    }

    #[test]
    fn mime_rules_match_exact_types_and_families() {
        assert!(Pattern::Mime("image/png".into()).matches("x.png"));
        assert!(!Pattern::Mime("image/png".into()).matches("x.jpg"));
        assert!(Pattern::Mime("image/*".into()).matches("x.jpg"));
        assert!(!Pattern::Mime("text/*".into()).matches("x.png"));
        assert!(!Pattern::Mime("image/*".into()).matches("no-extension"));
    }

    #[test]
    fn specs_pick_the_active_filter_or_accept_everything() {
        let mut spec = ChooserSpec::new(Kind::Open, "Open");
        assert!(spec.accepts("anything.bin"), "no filters shows every file");
        spec.filters = vec![
            FilterSpec {
                name: "Images".into(),
                patterns: vec![Pattern::Mime("image/*".into())],
            },
            FilterSpec {
                name: "Text".into(),
                patterns: vec![Pattern::Glob("*.txt".into())],
            },
        ];
        assert!(spec.accepts("a.png"));
        assert!(!spec.accepts("a.txt"));
        spec.current_filter = Some(1);
        assert!(spec.accepts("a.txt"));
        assert!(!spec.accepts("a.png"));
    }

    #[test]
    fn requested_folders_must_be_absolute_directories() {
        let home = Path::new("/home/someone");
        assert_eq!(resolve_folder(Some(Path::new("relative")), home), home);
        assert_eq!(
            resolve_folder(Some(Path::new("/no/such/dir/here")), home),
            home
        );
        assert_eq!(resolve_folder(None, home), home);
        assert_eq!(resolve_folder(Some(Path::new("/")), home), Path::new("/"));
    }

    #[test]
    fn specs_and_replies_round_trip_as_json() {
        let mut spec = ChooserSpec::new(Kind::Save, "Save As");
        spec.name = Some("draft.txt".into());
        spec.filters.push(FilterSpec {
            name: "Text".into(),
            patterns: vec![
                Pattern::Glob("*.txt".into()),
                Pattern::Mime("text/*".into()),
            ],
        });
        let json = serde_json::to_string(&spec).unwrap();
        assert_eq!(serde_json::from_str::<ChooserSpec>(&json).unwrap(), spec);
        for reply in [
            ChooserReply::Chosen {
                paths: vec![PathBuf::from("/a b/c")],
            },
            ChooserReply::Cancelled,
            ChooserReply::Failed {
                message: "no file".into(),
            },
        ] {
            let json = serde_json::to_string(&reply).unwrap();
            assert_eq!(serde_json::from_str::<ChooserReply>(&json).unwrap(), reply);
        }
    }
}
