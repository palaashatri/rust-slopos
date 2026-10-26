//! Ranked application search for the launcher. Discovery and `.desktop`
//! parsing live in `slopos_core::desktop_entry`.

use std::cmp::Ordering;
use std::collections::HashSet;
use std::path::Path;

pub use slopos_core::desktop_entry::{application_dirs, scan_desktop_apps, DesktopApp};

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
            startup_wm_class: String::new(),
        }
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
