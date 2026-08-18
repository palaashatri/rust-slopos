//! Desktop application discovery and safe `.desktop` Exec parsing.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct DesktopApp {
    pub id: String,
    pub name: String,
    pub argv: Vec<String>,
    pub icon: String,
    pub comment: String,
    pub terminal: bool,
}

pub fn scan_desktop_apps() -> Vec<DesktopApp> {
    // Later directories deliberately override earlier ones so a user's desktop
    // entry wins over the system copy with the same desktop-file id.
    let dirs = application_dirs();
    let mut apps_by_id = HashMap::new();

    for dir in dirs {
        if !dir.exists() {
            continue;
        }
        for entry in WalkDir::new(dir)
            .max_depth(2)
            .into_iter()
            .filter_map(Result::ok)
        {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("desktop") {
                continue;
            }
            if let Some(app) = parse_desktop_file(path) {
                apps_by_id.insert(app.id.clone(), app);
            }
        }
    }

    let mut apps: Vec<_> = apps_by_id.into_values().collect();
    apps.sort_by(app_order);
    apps
}

/// Return applications that match `query`, ranked for the Search palette.
///
/// The matcher deliberately stays independent of GTK so it can be tested with
/// deterministic fixtures.  Name matches always outrank secondary metadata;
/// ties use normalized name, executable identity, and desktop-file ID rather
/// than filesystem/hash-map iteration order.  Semantic duplicates are removed
/// after ranking, so a query never exposes multiple desktop IDs for the same
/// application identity.
pub(crate) fn ranked_app_matches(apps: &[DesktopApp], query: &str) -> Vec<DesktopApp> {
    let query = normalize_text(query);
    let mut ranked = apps
        .iter()
        .filter_map(|app| match_rank(app, &query).map(|rank| (rank, app)))
        .collect::<Vec<_>>();

    ranked.sort_by(|(left_rank, left_app), (right_rank, right_app)| {
        left_rank
            .cmp(right_rank)
            .then_with(|| app_order(left_app, right_app))
    });

    let mut seen = HashSet::new();
    ranked
        .into_iter()
        .filter_map(|(_, app)| {
            let key = semantic_key(app);
            seen.insert(key).then(|| app.clone())
        })
        .collect()
}

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
enum MatchRank {
    ExactName,
    NamePrefix,
    NameTokenOrSubstring,
    NameFuzzy,
    SecondaryFuzzy,
    EmptyQuery,
}

fn match_rank(app: &DesktopApp, query: &str) -> Option<MatchRank> {
    if query.is_empty() {
        return Some(MatchRank::EmptyQuery);
    }

    let name = normalize_text(&app.name);
    if name == query {
        return Some(MatchRank::ExactName);
    }
    if name.starts_with(query) {
        return Some(MatchRank::NamePrefix);
    }
    if name.contains(query)
        || name
            .split_whitespace()
            .any(|token| token.starts_with(query))
    {
        return Some(MatchRank::NameTokenOrSubstring);
    }
    if is_subsequence(query, &name) {
        return Some(MatchRank::NameFuzzy);
    }

    let comment = normalize_text(&app.comment);
    let command = normalize_text(&app.argv.join(" "));
    if is_subsequence(query, &comment) || is_subsequence(query, &command) {
        return Some(MatchRank::SecondaryFuzzy);
    }

    None
}

fn is_subsequence(query: &str, text: &str) -> bool {
    if query.is_empty() {
        return true;
    }

    let mut text_chars = text.chars();
    query
        .chars()
        .all(|query_char| text_chars.any(|text_char| text_char == query_char))
}

fn normalize_text(value: &str) -> String {
    let mut normalized = String::new();
    let mut pending_space = false;
    for character in value.chars() {
        for lowered in character.to_lowercase() {
            if lowered.is_whitespace() {
                pending_space = true;
            } else {
                if pending_space && !normalized.is_empty() {
                    normalized.push(' ');
                }
                normalized.push(lowered);
                pending_space = false;
            }
        }
    }
    normalized
}

fn semantic_key(app: &DesktopApp) -> (String, String) {
    (normalize_text(&app.name), executable_identity(app))
}

fn executable_identity(app: &DesktopApp) -> String {
    app.argv
        .first()
        .map(|program| {
            Path::new(program)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(program)
        })
        .map(normalize_text)
        .unwrap_or_default()
}

fn app_order(left: &DesktopApp, right: &DesktopApp) -> Ordering {
    normalize_text(&left.name)
        .cmp(&normalize_text(&right.name))
        .then_with(|| semantic_key(left).1.cmp(&semantic_key(right).1))
        .then_with(|| normalize_text(&left.id).cmp(&normalize_text(&right.id)))
        .then_with(|| left.id.cmp(&right.id))
        .then_with(|| left.argv.cmp(&right.argv))
        .then_with(|| left.comment.cmp(&right.comment))
        .then_with(|| left.icon.cmp(&right.icon))
        .then_with(|| left.terminal.cmp(&right.terminal))
}

pub(crate) fn application_dirs() -> Vec<PathBuf> {
    application_dirs_from_data_dirs(std::env::var_os("XDG_DATA_DIRS").as_deref())
}

fn application_dirs_from_data_dirs(data_dirs: Option<&OsStr>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    push_unique_dir(&mut dirs, PathBuf::from("/usr/share/applications"));
    push_unique_dir(&mut dirs, PathBuf::from("/usr/local/share/applications"));

    // SLOPOS custom-prefix sessions export XDG_DATA_DIRS so Search sees the
    // installed wrapper desktop entry without hard-coding that prefix.
    if let Some(data_dirs) = data_dirs {
        for data_dir in std::env::split_paths(data_dirs) {
            push_unique_dir(&mut dirs, data_dir.join("applications"));
        }
    }

    push_unique_dir(&mut dirs, dirs_home_applications());
    dirs
}

fn push_unique_dir(dirs: &mut Vec<PathBuf>, directory: PathBuf) {
    if !dirs.iter().any(|existing| existing == &directory) {
        dirs.push(directory);
    }
}

fn parse_desktop_file(path: &Path) -> Option<DesktopApp> {
    let content = fs::read_to_string(path).ok()?;
    let id = path.file_stem()?.to_string_lossy();
    parse_desktop_entry(&content, id.as_ref())
}

fn parse_desktop_entry(content: &str, id: &str) -> Option<DesktopApp> {
    let mut in_desktop_entry = false;
    let mut app_type = String::new();
    let mut name = String::new();
    let mut exec = String::new();
    let mut icon = String::new();
    let mut comment = String::new();
    let mut try_exec = String::new();
    let mut only_show_in = String::new();
    let mut not_show_in = String::new();
    let mut no_display = false;
    let mut hidden = false;
    let mut terminal = false;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line == "[Desktop Entry]" {
            in_desktop_entry = true;
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_desktop_entry = false;
            continue;
        }
        if !in_desktop_entry || line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        match key {
            "Type" if app_type.is_empty() => app_type = value.to_string(),
            "Name" if name.is_empty() => name = value.to_string(),
            "Exec" if exec.is_empty() => exec = value.to_string(),
            "Icon" if icon.is_empty() => icon = value.to_string(),
            "Comment" if comment.is_empty() => comment = value.to_string(),
            "TryExec" if try_exec.is_empty() => try_exec = value.to_string(),
            "OnlyShowIn" if only_show_in.is_empty() => only_show_in = value.to_string(),
            "NotShowIn" if not_show_in.is_empty() => not_show_in = value.to_string(),
            "NoDisplay" => no_display = value.eq_ignore_ascii_case("true"),
            "Hidden" => hidden = value.eq_ignore_ascii_case("true"),
            "Terminal" => terminal = value.eq_ignore_ascii_case("true"),
            _ => {}
        }
    }

    if hidden || no_display || app_type != "Application" || name.is_empty() || exec.is_empty() {
        return None;
    }
    if !desktop_visibility_allows(&only_show_in, &not_show_in) {
        return None;
    }
    if !try_exec.is_empty() && !command_exists(&try_exec) {
        return None;
    }

    let argv = parse_exec_argv(&exec)?;
    if argv.is_empty() || !command_exists(&argv[0]) {
        return None;
    }

    Some(DesktopApp {
        id: id.to_string(),
        name,
        argv,
        icon,
        comment,
        terminal,
    })
}

fn desktop_visibility_allows(only_show_in: &str, not_show_in: &str) -> bool {
    const DESKTOP: &str = "SLOPOS";
    let contains = |value: &str| {
        value
            .split(';')
            .any(|desktop| desktop.eq_ignore_ascii_case(DESKTOP))
    };

    (only_show_in.is_empty() || contains(only_show_in)) && !contains(not_show_in)
}

/// Parse the freedesktop Exec token grammar without invoking a shell.
/// Field codes requiring a selected file/URL are omitted because Search starts
/// applications without an associated document.
fn parse_exec_argv(exec: &str) -> Option<Vec<String>> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut chars = exec.chars().peekable();
    let mut quoted = false;

    while let Some(ch) = chars.next() {
        match ch {
            '"' => quoted = !quoted,
            '\\' => current.push(chars.next()?),
            ' ' | '\t' if !quoted => {
                push_exec_token(&mut args, &mut current)?;
            }
            _ => current.push(ch),
        }
    }

    if quoted {
        return None;
    }
    push_exec_token(&mut args, &mut current)?;
    Some(args)
}

fn push_exec_token(args: &mut Vec<String>, current: &mut String) -> Option<()> {
    if current.is_empty() {
        return Some(());
    }
    let expanded = expand_field_codes(current)?;
    current.clear();
    if !expanded.is_empty() {
        args.push(expanded);
    }
    Some(())
}

fn expand_field_codes(token: &str) -> Option<String> {
    let mut output = String::new();
    let mut chars = token.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            output.push(ch);
            continue;
        }

        match chars.next()? {
            '%' => output.push('%'),
            // No file/URL, icon, display name, or desktop-entry path is
            // supplied by the application palette, so these expand to empty.
            'f' | 'F' | 'u' | 'U' | 'i' | 'c' | 'k' => {}
            _ => return None,
        }
    }
    Some(output)
}

fn command_exists(command: &str) -> bool {
    let path = Path::new(command);
    if path.components().count() > 1 {
        return path.is_file();
    }

    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(command).is_file()))
        .unwrap_or(false)
}

fn dirs_home_applications() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".local/share/applications")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app(id: &str, name: &str, comment: &str, program: &str) -> DesktopApp {
        DesktopApp {
            id: id.to_string(),
            name: name.to_string(),
            argv: vec![program.to_string()],
            icon: String::new(),
            comment: comment.to_string(),
            terminal: false,
        }
    }

    #[test]
    fn exec_parser_preserves_quoted_arguments_and_drops_file_codes() {
        assert_eq!(
            parse_exec_argv(r#"/bin/echo --title "Hello world" %U %%"#),
            Some(vec![
                "/bin/echo".to_string(),
                "--title".to_string(),
                "Hello world".to_string(),
                "%".to_string(),
            ])
        );
    }

    #[test]
    fn exec_parser_rejects_unterminated_quotes() {
        assert_eq!(parse_exec_argv(r#"/bin/echo "broken"#), None);
    }

    #[test]
    fn hidden_non_application_and_foreign_desktop_entries_are_rejected() {
        let hidden =
            "[Desktop Entry]\nType=Application\nName=Hidden\nExec=/bin/true\nHidden=true\n";
        assert!(parse_desktop_entry(hidden, "hidden").is_none());

        let link = "[Desktop Entry]\nType=Link\nName=Docs\nExec=/bin/true\n";
        assert!(parse_desktop_entry(link, "link").is_none());

        let gnome_only = "[Desktop Entry]\nType=Application\nName=GNOME only\nExec=/bin/true\nOnlyShowIn=GNOME;\n";
        assert!(parse_desktop_entry(gnome_only, "gnome-only").is_none());
    }

    #[test]
    fn valid_entry_is_parsed_without_shell_expansion() {
        let entry = r#"[Desktop Entry]
Type=Application
Name=Echo
Comment=Test
Exec=/bin/echo "hello world" %F
Icon=utilities-terminal
Terminal=false
"#;
        let app = parse_desktop_entry(entry, "echo").expect("valid entry");
        assert_eq!(app.name, "Echo");
        assert_eq!(app.argv, vec!["/bin/echo", "hello world"]);
        assert!(!app.terminal);
    }

    #[test]
    fn application_search_includes_custom_xdg_data_directories() {
        let dirs =
            application_dirs_from_data_dirs(Some(OsStr::new("/opt/slopos/share:/usr/share")));
        assert!(dirs.contains(&PathBuf::from("/opt/slopos/share/applications")));
        assert_eq!(
            dirs.iter()
                .filter(|directory| directory.as_path() == Path::new("/usr/share/applications"))
                .count(),
            1
        );
    }

    #[test]
    fn fuzzy_matching_accepts_subsequences() {
        let apps = vec![test_app("xterm", "XTerm", "Terminal", "/usr/bin/xterm")];
        let matches = ranked_app_matches(&apps, "xtrm");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].id, "xterm");
    }

    #[test]
    fn name_ranks_above_secondary_fuzzy_metadata() {
        let apps = vec![
            test_app(
                "secondary",
                "Console",
                "A terminal utility",
                "/usr/bin/console",
            ),
            test_app(
                "token",
                "Xfce Terminal",
                "A shell",
                "/usr/bin/xfce4-terminal",
            ),
            test_app(
                "prefix",
                "Terminal Emulator",
                "A shell",
                "/usr/bin/terminal-emulator",
            ),
            test_app("exact", "Terminal", "A shell", "/usr/bin/terminal"),
        ];

        let matches = ranked_app_matches(&apps, "terminal");
        let ids = matches
            .iter()
            .map(|app| app.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, ["exact", "prefix", "token", "secondary"]);
    }

    #[test]
    fn semantic_duplicates_are_suppressed_but_distinct_executables_remain() {
        let apps = vec![
            test_app("notes-z", "Notes", "Second desktop ID", "/usr/bin/notes"),
            test_app("notes-a", "  notes  ", "First desktop ID", "notes"),
            test_app(
                "notes-pro",
                "Notes",
                "Different program",
                "/usr/bin/notes-pro",
            ),
        ];

        let matches = ranked_app_matches(&apps, "");
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].id, "notes-a");
        assert_eq!(matches[1].id, "notes-pro");
    }

    #[test]
    fn equal_rank_ties_are_deterministic_independent_of_input_order() {
        let first = test_app("editor-a", "Editor", "", "/usr/bin/editor");
        let second = test_app("editor-b", "Editor", "", "/usr/bin/editor-2");
        let forward = ranked_app_matches(&[first.clone(), second.clone()], "edit");
        let reverse = ranked_app_matches(&[second, first], "edit");

        let forward_ids = forward
            .iter()
            .map(|app| app.id.as_str())
            .collect::<Vec<_>>();
        let reverse_ids = reverse
            .iter()
            .map(|app| app.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(forward_ids, reverse_ids);
        assert_eq!(forward_ids, ["editor-a", "editor-b"]);
    }
}
