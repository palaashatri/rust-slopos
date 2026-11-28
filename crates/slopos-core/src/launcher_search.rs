//! Launcher search: typed result providers, ranking and keyboard-only result
//! navigation for the desktop launcher.
//!
//! This module is portable. It has no GTK, X11 or D-Bus dependency and starts
//! no threads or timers, so nothing here polls. A host builds a candidate
//! snapshot from the providers, feeds query and key events in, and acts on the
//! returned [`Outcome`]. The file snapshot is taken once per open by the host
//! with [`scan_files`], which is bounded in depth and entry count.

use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use crate::desktop_entry::DesktopApp;

/// Most results one query returns. Bounds memory and the list the host draws.
pub const MAX_RESULTS: usize = 50;

/// Most file paths a [`FileProvider`] contributes to one snapshot.
pub const MAX_FILE_CANDIDATES: usize = 2_000;

/// A session action the launcher can offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Lock,
    LogOut,
    Restart,
    ShutDown,
}

impl Action {
    /// Every action, in the order the launcher lists them.
    pub const ALL: [Action; 4] = [
        Action::Lock,
        Action::LogOut,
        Action::Restart,
        Action::ShutDown,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Action::Lock => "Lock Screen",
            Action::LogOut => "Log Out",
            Action::Restart => "Restart",
            Action::ShutDown => "Shut Down",
        }
    }

    /// Actions that end or interrupt the session must be confirmed by the user
    /// before the host runs them, so one stray Enter cannot shut the machine down.
    pub fn requires_confirmation(self) -> bool {
        !matches!(self, Action::Lock)
    }

    fn keywords(self) -> &'static str {
        match self {
            Action::Lock => "lock screen password",
            Action::LogOut => "log out sign out end session",
            Action::Restart => "reboot restart",
            Action::ShutDown => "power off shutdown halt",
        }
    }
}

/// What activating a result does. The host performs the real work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A desktop application, by desktop-file id.
    App { id: String },
    /// A settings panel, by its `slopos-control-panels` argument without the
    /// leading `--`, for example `sound`.
    Panel { argument: &'static str },
    /// A session action.
    Action(Action),
    /// A regular file for the host to open.
    File { path: PathBuf },
}

impl Target {
    pub fn kind(&self) -> ResultKind {
        match self {
            Target::App { .. } => ResultKind::App,
            Target::Panel { .. } => ResultKind::Panel,
            Target::Action(_) => ResultKind::Action,
            Target::File { .. } => ResultKind::File,
        }
    }

    /// Identity used to drop duplicate results for the same thing.
    fn key(&self) -> String {
        match self {
            Target::App { id } => format!("app:{id}"),
            Target::Panel { argument } => format!("panel:{argument}"),
            Target::Action(action) => format!("action:{action:?}"),
            Target::File { path } => format!("file:{}", path.display()),
        }
    }
}

/// The kind of a result. Declaration order is the tie-break order for equal
/// matches: applications, then panels, then actions, then files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ResultKind {
    App,
    Panel,
    Action,
    File,
}

impl ResultKind {
    /// Name announced for a result of this kind.
    pub fn label(self) -> &'static str {
        match self {
            ResultKind::App => "Application",
            ResultKind::Panel => "Settings panel",
            ResultKind::Action => "Action",
            ResultKind::File => "File",
        }
    }
}

/// One searchable result from a provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub title: String,
    pub subtitle: String,
    /// Extra searchable words that are not shown in the title.
    pub keywords: String,
    pub target: Target,
}

impl Candidate {
    pub fn kind(&self) -> ResultKind {
        self.target.kind()
    }
}

/// A source of candidates. Each provider owns one kind of result.
pub trait SearchProvider {
    /// Append this provider's candidates to `out`.
    fn candidates(&self, out: &mut Vec<Candidate>);
}

/// Collect the candidates of several providers into one snapshot.
pub fn collect(providers: &[&dyn SearchProvider]) -> Vec<Candidate> {
    let mut out = Vec::new();
    for provider in providers {
        provider.candidates(&mut out);
    }
    out
}

/// Desktop applications, from `slopos_core::desktop_entry`.
pub struct AppProvider<'a> {
    pub apps: &'a [DesktopApp],
}

impl SearchProvider for AppProvider<'_> {
    fn candidates(&self, out: &mut Vec<Candidate>) {
        for app in self.apps {
            let program = app.argv.first().map(String::as_str).unwrap_or_default();
            out.push(Candidate {
                title: app.name.clone(),
                subtitle: app.comment.clone(),
                keywords: format!("{} {program}", app.comment),
                target: Target::App { id: app.id.clone() },
            });
        }
    }
}

/// The settings panels of `slopos-control-panels`. Titles and arguments are
/// listed here, so the launcher offers exactly the panels the app has.
const PANELS: [(&str, &str, &str); 8] = [
    ("Network", "network", "wifi wireless ethernet internet"),
    ("Sound", "sound", "audio volume speakers output"),
    ("Display", "display", "brightness monitor screen resolution"),
    ("Bluetooth", "bluetooth", "devices pair headphones"),
    ("Date & Time", "datetime", "clock time date timezone"),
    ("Desktop", "desktop", "wallpaper background picture"),
    ("Appearance", "appearance", "theme colors dark light"),
    ("Keyboard & Mouse", "keyboard", "input mouse touchpad keys"),
];

/// Settings panels.
pub struct PanelProvider;

impl SearchProvider for PanelProvider {
    fn candidates(&self, out: &mut Vec<Candidate>) {
        for (title, argument, keywords) in PANELS {
            out.push(Candidate {
                title: title.to_string(),
                subtitle: "Settings".to_string(),
                keywords: keywords.to_string(),
                target: Target::Panel { argument },
            });
        }
    }
}

/// Session actions.
pub struct ActionProvider;

impl SearchProvider for ActionProvider {
    fn candidates(&self, out: &mut Vec<Candidate>) {
        for action in Action::ALL {
            out.push(Candidate {
                title: action.label().to_string(),
                subtitle: String::new(),
                keywords: action.keywords().to_string(),
                target: Target::Action(action),
            });
        }
    }
}

/// Files the host has snapshotted, for example with [`scan_files`].
pub struct FileProvider<'a> {
    pub paths: &'a [PathBuf],
}

impl SearchProvider for FileProvider<'_> {
    fn candidates(&self, out: &mut Vec<Candidate>) {
        for path in self.paths.iter().take(MAX_FILE_CANDIDATES) {
            let title = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            let subtitle = path
                .parent()
                .map(|parent| parent.display().to_string())
                .unwrap_or_default();
            out.push(Candidate {
                title,
                subtitle,
                keywords: String::new(),
                target: Target::File { path: path.clone() },
            });
        }
    }
}

/// List regular files under `root`, breadth first, for a [`FileProvider`].
///
/// `max_depth` counts directory levels including `root`, so `1` lists only the
/// files directly in `root`. Hidden entries and symbolic links are skipped, so
/// the walk never leaves the tree or follows a loop. At most `max_entries`
/// paths are returned. Entries are sorted by name within each directory.
pub fn scan_files(root: &Path, max_depth: usize, max_entries: usize) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::new();
    queue.push_back((root.to_path_buf(), 1));

    while let Some((dir, depth)) = queue.pop_front() {
        if found.len() >= max_entries {
            break;
        }
        let Ok(read) = fs::read_dir(&dir) else {
            continue;
        };
        let mut children: Vec<fs::DirEntry> = read.filter_map(Result::ok).collect();
        children.sort_by_key(|child| child.file_name());

        for child in children {
            if found.len() >= max_entries {
                break;
            }
            if child.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            // `file_type` does not follow links, so a symlink is seen as a link.
            let Ok(file_type) = child.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            let path = child.path();
            if file_type.is_dir() {
                if depth < max_depth {
                    queue.push_back((path, depth + 1));
                }
            } else if file_type.is_file() {
                found.push(path);
            }
        }
    }
    found
}

/// A key the launcher handles. The host maps its keysyms to these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Home,
    End,
    Enter,
    Escape,
}

/// What the host should do after a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing to do beyond redrawing.
    Ignored,
    /// Run this target now.
    Activate(Target),
    /// Run this target only after the user confirms it.
    Confirm(Target),
    /// Close the launcher.
    Dismiss,
}

/// Lower is a better match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Tier {
    Exact,
    Prefix,
    Word,
    Subsequence,
    Detail,
    /// Empty query: every candidate is listed.
    Listed,
}

/// A candidate with its matching text precomputed once.
struct Entry {
    candidate: Candidate,
    title: String,
    detail: String,
}

impl Entry {
    fn new(candidate: Candidate) -> Self {
        let title = normalize(&candidate.title);
        let detail = normalize(&format!("{} {}", candidate.subtitle, candidate.keywords));
        Self {
            candidate,
            title,
            detail,
        }
    }

    fn tier(&self, query: &str) -> Option<Tier> {
        if query.is_empty() {
            return Some(Tier::Listed);
        }
        if self.title == query {
            return Some(Tier::Exact);
        }
        if self.title.starts_with(query) {
            return Some(Tier::Prefix);
        }
        if self.title.contains(query) || self.title.split(' ').any(|word| word.starts_with(query)) {
            return Some(Tier::Word);
        }
        if is_subsequence(query, &self.title) {
            return Some(Tier::Subsequence);
        }
        if self.detail.contains(query) || is_subsequence(query, &self.detail) {
            return Some(Tier::Detail);
        }
        None
    }
}

/// The launcher's search state: query, ranked results and the selection.
pub struct Launcher {
    entries: Vec<Entry>,
    query: String,
    /// Indexes into `entries`, best match first, at most [`MAX_RESULTS`].
    hits: Vec<usize>,
    selected: usize,
}

impl Launcher {
    /// Build a launcher over a snapshot. Candidates with the same target are
    /// kept once, the first one wins. Starts with an empty query.
    pub fn new(candidates: Vec<Candidate>) -> Self {
        let mut seen = HashSet::new();
        let entries: Vec<Entry> = candidates
            .into_iter()
            .filter(|candidate| seen.insert(candidate.target.key()))
            .map(Entry::new)
            .collect();
        let mut launcher = Self {
            entries,
            query: String::new(),
            hits: Vec::new(),
            selected: 0,
        };
        launcher.hits = launcher.rank("");
        launcher
    }

    /// Replace the query, re-rank and select the first result.
    pub fn set_query(&mut self, query: &str) {
        self.query = query.to_string();
        self.hits = self.rank(&normalize(query));
        self.selected = 0;
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    /// Number of results for the current query.
    pub fn result_count(&self) -> usize {
        self.hits.len()
    }

    /// The results in display order.
    pub fn results(&self) -> impl Iterator<Item = &Candidate> + '_ {
        self.hits
            .iter()
            .map(move |&index| &self.entries[index].candidate)
    }

    /// Position of the selected result, or `None` when there are none.
    pub fn selected(&self) -> Option<usize> {
        (!self.hits.is_empty()).then_some(self.selected)
    }

    pub fn selected_candidate(&self) -> Option<&Candidate> {
        self.hits
            .get(self.selected)
            .map(|&index| &self.entries[index].candidate)
    }

    /// Handle one key. Up and Down stop at the ends of the list rather than
    /// wrapping, so the position a screen reader announces stays predictable.
    pub fn handle_key(&mut self, key: Key) -> Outcome {
        let last = self.hits.len().saturating_sub(1);
        match key {
            Key::Up => {
                self.selected = self.selected.saturating_sub(1);
                Outcome::Ignored
            }
            Key::Down => {
                if !self.hits.is_empty() {
                    self.selected = (self.selected + 1).min(last);
                }
                Outcome::Ignored
            }
            Key::Home => {
                self.selected = 0;
                Outcome::Ignored
            }
            Key::End => {
                self.selected = last;
                Outcome::Ignored
            }
            Key::Enter => match self.selected_candidate().map(|c| c.target.clone()) {
                Some(target) => {
                    let confirm =
                        matches!(&target, Target::Action(action) if action.requires_confirmation());
                    if confirm {
                        Outcome::Confirm(target)
                    } else {
                        Outcome::Activate(target)
                    }
                }
                None => Outcome::Ignored,
            },
            Key::Escape => Outcome::Dismiss,
        }
    }

    /// Text for a live region: how many results there are, or why there are none.
    pub fn status_text(&self) -> String {
        match self.hits.len() {
            0 => {
                let query = self.query.trim();
                if query.is_empty() {
                    "Type to search applications, files, settings and actions".to_string()
                } else {
                    format!("No results for \"{query}\"")
                }
            }
            1 => "1 result".to_string(),
            MAX_RESULTS => format!("First {MAX_RESULTS} results"),
            count => format!("{count} results"),
        }
    }

    /// Accessible name for the result at `position`, for example
    /// `Calculator, Application, 1 of 2`. Actions that need confirmation say so.
    pub fn accessible_label(&self, position: usize) -> Option<String> {
        let &index = self.hits.get(position)?;
        let candidate = &self.entries[index].candidate;
        let mut label = format!(
            "{}, {}, {} of {}",
            candidate.title,
            candidate.kind().label(),
            position + 1,
            self.hits.len()
        );
        if let Target::Action(action) = &candidate.target {
            if action.requires_confirmation() {
                label.push_str(", asks for confirmation");
            }
        }
        Some(label)
    }

    fn rank(&self, query: &str) -> Vec<usize> {
        let mut ranked: Vec<(Tier, usize)> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| entry.tier(query).map(|tier| (tier, index)))
            .collect();
        ranked.sort_by(|(left_tier, left), (right_tier, right)| {
            left_tier
                .cmp(right_tier)
                .then_with(|| {
                    self.entries[*left]
                        .candidate
                        .kind()
                        .cmp(&self.entries[*right].candidate.kind())
                })
                .then_with(|| self.entries[*left].title.cmp(&self.entries[*right].title))
                .then_with(|| left.cmp(right))
        });
        ranked
            .into_iter()
            .take(MAX_RESULTS)
            .map(|(_, index)| index)
            .collect()
    }
}

/// Lower case, with runs of whitespace collapsed to single spaces.
fn normalize(value: &str) -> String {
    value
        .split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

/// True when the non-space characters of `query` appear in `text` in order.
fn is_subsequence(query: &str, text: &str) -> bool {
    let mut chars = text.chars();
    query
        .chars()
        .filter(|c| !c.is_whitespace())
        .all(|wanted| chars.any(|c| c == wanted))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, name: &str, comment: &str) -> DesktopApp {
        DesktopApp {
            id: id.to_string(),
            name: name.to_string(),
            argv: vec![id.to_string()],
            icon: String::new(),
            comment: comment.to_string(),
            terminal: false,
            startup_wm_class: String::new(),
            mime_types: Vec::new(),
            exec: id.to_string(),
        }
    }

    fn candidate(title: &str, target: Target) -> Candidate {
        Candidate {
            title: title.to_string(),
            subtitle: String::new(),
            keywords: String::new(),
            target,
        }
    }

    fn titles(launcher: &Launcher) -> Vec<String> {
        launcher.results().map(|c| c.title.clone()).collect()
    }

    fn scratch_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("slopos-launcher-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn prefix_outranks_word_match_which_outranks_subsequence() {
        let mut launcher = Launcher::new(vec![
            candidate("Local Calendar", Target::Panel { argument: "a" }),
            candidate("Spreadsheet Calc", Target::Panel { argument: "b" }),
            candidate(
                "Calculator",
                Target::App {
                    id: "calculator".into(),
                },
            ),
            candidate(
                "Terminal",
                Target::App {
                    id: "terminal".into(),
                },
            ),
        ]);
        launcher.set_query("calc");
        assert_eq!(
            titles(&launcher),
            vec!["Calculator", "Spreadsheet Calc", "Local Calendar"]
        );
    }

    #[test]
    fn exact_title_outranks_longer_prefix_match() {
        let mut launcher = Launcher::new(vec![
            candidate("Files Extra", Target::App { id: "extra".into() }),
            candidate("Files", Target::App { id: "files".into() }),
        ]);
        launcher.set_query("files");
        assert_eq!(titles(&launcher), vec!["Files", "Files Extra"]);
    }

    #[test]
    fn equal_matches_break_ties_by_kind_then_title() {
        let mut launcher = Launcher::new(vec![
            candidate("Sound", Target::Panel { argument: "sound" }),
            candidate(
                "Sound",
                Target::App {
                    id: "sound-app".into(),
                },
            ),
        ]);
        launcher.set_query("sound");
        let kinds: Vec<ResultKind> = launcher.results().map(Candidate::kind).collect();
        assert_eq!(kinds, vec![ResultKind::App, ResultKind::Panel]);
    }

    #[test]
    fn keywords_match_when_the_title_does_not() {
        let mut launcher = Launcher::new(PanelProvider.candidates_vec());
        launcher.set_query("touchpad");
        assert!(titles(&launcher).contains(&"Keyboard & Mouse".to_string()));
        assert_eq!(
            launcher.selected_candidate().unwrap().title,
            "Keyboard & Mouse"
        );
    }

    #[test]
    fn empty_query_lists_in_kind_order_and_is_bounded() {
        let paths: Vec<PathBuf> = (0..MAX_RESULTS + 30)
            .map(|i| PathBuf::from(format!("/home/user/file-{i:03}.txt")))
            .collect();
        let mut all = ActionProvider.candidates_vec();
        all.extend(FileProvider { paths: &paths }.candidates_vec());
        let launcher = Launcher::new(all);
        assert_eq!(launcher.result_count(), MAX_RESULTS);
        assert_eq!(
            launcher.results().next().unwrap().kind(),
            ResultKind::Action
        );
    }

    #[test]
    fn no_match_reports_empty_results_and_status() {
        let mut launcher = Launcher::new(PanelProvider.candidates_vec());
        launcher.set_query("zzqx");
        assert_eq!(launcher.result_count(), 0);
        assert_eq!(launcher.selected(), None);
        assert_eq!(launcher.status_text(), "No results for \"zzqx\"");
        assert_eq!(launcher.handle_key(Key::Enter), Outcome::Ignored);
    }

    #[test]
    fn up_and_down_move_selection_and_stop_at_the_ends() {
        let mut launcher = Launcher::new(vec![
            candidate("Alpha", Target::App { id: "a".into() }),
            candidate("Beta", Target::App { id: "b".into() }),
        ]);
        assert_eq!(launcher.selected(), Some(0));
        assert_eq!(launcher.handle_key(Key::Up), Outcome::Ignored);
        assert_eq!(launcher.selected(), Some(0));
        launcher.handle_key(Key::Down);
        assert_eq!(launcher.selected(), Some(1));
        launcher.handle_key(Key::Down);
        assert_eq!(launcher.selected(), Some(1));
        launcher.handle_key(Key::Home);
        assert_eq!(launcher.selected(), Some(0));
        launcher.handle_key(Key::End);
        assert_eq!(launcher.selected(), Some(1));
    }

    #[test]
    fn changing_the_query_selects_the_first_result() {
        let mut launcher = Launcher::new(vec![
            candidate("Alpha", Target::App { id: "a".into() }),
            candidate("Beta", Target::App { id: "b".into() }),
        ]);
        launcher.handle_key(Key::End);
        launcher.set_query("a");
        assert_eq!(launcher.selected(), Some(0));
        assert_eq!(launcher.query(), "a");
    }

    #[test]
    fn enter_activates_the_selected_target() {
        let mut launcher = Launcher::new(vec![
            candidate("Alpha", Target::App { id: "a".into() }),
            candidate("Beta", Target::App { id: "b".into() }),
        ]);
        launcher.handle_key(Key::Down);
        assert_eq!(
            launcher.handle_key(Key::Enter),
            Outcome::Activate(Target::App { id: "b".into() })
        );
    }

    #[test]
    fn destructive_actions_need_confirmation_and_lock_does_not() {
        let mut launcher = Launcher::new(ActionProvider.candidates_vec());
        launcher.set_query("shut");
        assert_eq!(
            launcher.handle_key(Key::Enter),
            Outcome::Confirm(Target::Action(Action::ShutDown))
        );
        launcher.set_query("lock");
        assert_eq!(
            launcher.handle_key(Key::Enter),
            Outcome::Activate(Target::Action(Action::Lock))
        );
    }

    #[test]
    fn escape_dismisses() {
        let mut launcher = Launcher::new(Vec::new());
        assert_eq!(launcher.handle_key(Key::Escape), Outcome::Dismiss);
    }

    #[test]
    fn accessible_label_names_kind_position_and_confirmation() {
        let mut launcher = Launcher::new(vec![
            candidate(
                "Calculator",
                Target::App {
                    id: "calculator".into(),
                },
            ),
            candidate("Shut Down", Target::Action(Action::ShutDown)),
        ]);
        assert_eq!(
            launcher.accessible_label(0).as_deref(),
            Some("Calculator, Application, 1 of 2")
        );
        assert_eq!(
            launcher.accessible_label(1).as_deref(),
            Some("Shut Down, Action, 2 of 2, asks for confirmation")
        );
        assert_eq!(launcher.accessible_label(2), None);
        launcher.set_query("zzz");
        assert_eq!(launcher.accessible_label(0), None);
    }

    #[test]
    fn status_text_counts_results() {
        let empty = Launcher::new(Vec::new());
        assert!(empty.status_text().starts_with("Type to search"));

        let mut launcher = Launcher::new(PanelProvider.candidates_vec());
        assert_eq!(launcher.status_text(), "8 results");
        launcher.set_query("sound");
        assert_eq!(launcher.status_text(), "1 result");
        launcher.set_query("e");
        assert!(launcher.status_text().ends_with("results"));
    }

    #[test]
    fn duplicate_targets_are_listed_once() {
        let launcher = Launcher::new(vec![
            candidate(
                "Calculator",
                Target::App {
                    id: "calculator".into(),
                },
            ),
            candidate(
                "Calculator (copy)",
                Target::App {
                    id: "calculator".into(),
                },
            ),
        ]);
        assert_eq!(launcher.result_count(), 1);
    }

    #[test]
    fn app_provider_maps_desktop_entries_to_app_targets() {
        let apps = vec![app("calculator", "Calculator", "Do sums")];
        let candidates = AppProvider { apps: &apps }.candidates_vec();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].kind(), ResultKind::App);
        assert_eq!(
            candidates[0].target,
            Target::App {
                id: "calculator".into()
            }
        );
        let mut launcher = Launcher::new(candidates);
        launcher.set_query("sums");
        assert_eq!(titles(&launcher), vec!["Calculator"]);
    }

    #[test]
    fn panel_provider_offers_each_panel_once_with_a_unique_argument() {
        let candidates = PanelProvider.candidates_vec();
        assert_eq!(candidates.len(), PANELS.len());
        let arguments: HashSet<&str> = candidates
            .iter()
            .filter_map(|c| match c.target {
                Target::Panel { argument } => Some(argument),
                _ => None,
            })
            .collect();
        assert_eq!(arguments.len(), PANELS.len());
    }

    #[test]
    fn action_provider_confirms_only_session_ending_actions() {
        let needs: Vec<Action> = Action::ALL
            .into_iter()
            .filter(|action| action.requires_confirmation())
            .collect();
        assert_eq!(
            needs,
            vec![Action::LogOut, Action::Restart, Action::ShutDown]
        );
    }

    #[test]
    fn file_provider_is_bounded_by_the_candidate_limit() {
        let paths: Vec<PathBuf> = (0..(MAX_FILE_CANDIDATES + 10))
            .map(|i| PathBuf::from(format!("/data/f{i}.bin")))
            .collect();
        assert_eq!(
            FileProvider { paths: &paths }.candidates_vec().len(),
            MAX_FILE_CANDIDATES
        );
    }

    #[test]
    fn file_candidates_use_the_file_name_and_parent_directory() {
        let paths = vec![PathBuf::from("/home/user/Documents/report.txt")];
        let mut launcher = Launcher::new(FileProvider { paths: &paths }.candidates_vec());
        launcher.set_query("report");
        let hit = launcher.selected_candidate().unwrap();
        assert_eq!(hit.title, "report.txt");
        assert_eq!(hit.subtitle, "/home/user/Documents");
        assert_eq!(
            hit.target,
            Target::File {
                path: PathBuf::from("/home/user/Documents/report.txt")
            }
        );
    }

    #[test]
    fn scan_files_respects_depth_and_skips_hidden_entries_and_links() {
        let root = scratch_dir("depth");
        fs::create_dir_all(root.join("sub/deep")).unwrap();
        fs::write(root.join("a.txt"), b"a").unwrap();
        fs::write(root.join(".hidden"), b"h").unwrap();
        fs::write(root.join("sub/b.txt"), b"b").unwrap();
        fs::write(root.join("sub/deep/c.txt"), b"c").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(root.join("a.txt"), root.join("link.txt")).unwrap();

        let shallow = scan_files(&root, 1, 100);
        assert_eq!(shallow, vec![root.join("a.txt")]);

        let two_levels = scan_files(&root, 2, 100);
        assert_eq!(two_levels, vec![root.join("a.txt"), root.join("sub/b.txt")]);

        let all = scan_files(&root, 5, 100);
        assert_eq!(
            all,
            vec![
                root.join("a.txt"),
                root.join("sub/b.txt"),
                root.join("sub/deep/c.txt")
            ]
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn scan_files_stops_at_the_entry_limit() {
        let root = scratch_dir("limit");
        for i in 0..5 {
            fs::write(root.join(format!("f{i}.txt")), b"x").unwrap();
        }
        assert_eq!(scan_files(&root, 1, 3).len(), 3);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn scan_files_of_a_missing_root_is_empty() {
        let missing = std::env::temp_dir().join("slopos-launcher-does-not-exist-xyz");
        assert!(scan_files(&missing, 3, 100).is_empty());
    }

    /// Test helper: a provider's candidates as an owned vector.
    trait CandidatesVec {
        fn candidates_vec(&self) -> Vec<Candidate>;
    }

    impl<P: SearchProvider> CandidatesVec for P {
        fn candidates_vec(&self) -> Vec<Candidate> {
            let mut out = Vec::new();
            self.candidates(&mut out);
            out
        }
    }
}
