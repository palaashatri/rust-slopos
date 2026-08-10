//! Global search/launcher (Spotlight-like) for SLOPOS-I.
//!
//! Provides a system-wide search overlay for apps, files, and settings.
//! Invoked by Super+Space; displays results as user types.

use crate::launch_services::AppBundle;
use std::path::PathBuf;

/// Result from a search query — could be an app, file, or settings entry.
#[derive(Debug, Clone)]
pub enum SearchResult {
    /// An installed app.
    App(AppBundle),
    /// A file path.
    File {
        path: PathBuf,
        /// MIME type hint.
        mime_type: Option<String>,
    },
    /// A settings entry (category + title).
    Setting { category: String, title: String },
}

impl SearchResult {
    /// Display name for UI rendering.
    pub fn display_name(&self) -> String {
        match self {
            SearchResult::App(app) => app.name.clone(),
            SearchResult::File { path, .. } => path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("File")
                .to_string(),
            SearchResult::Setting { title, .. } => title.clone(),
        }
    }

    /// Detailed description for UI rendering (optional).
    pub fn description(&self) -> Option<String> {
        match self {
            SearchResult::File { path, .. } => Some(path.display().to_string()),
            SearchResult::Setting { category, .. } => Some(category.clone()),
            SearchResult::App(_) => None,
        }
    }
}

/// Search backend — queries apps, files, and settings.
pub struct SearchBackend {
    /// Hardcoded settings entries (category, title).
    settings: Vec<(String, String)>,
}

impl SearchBackend {
    /// Create a new search backend with default settings entries.
    pub fn new() -> Self {
        let settings = vec![
            ("Display".to_string(), "Resolution...".to_string()),
            ("Display".to_string(), "Brightness...".to_string()),
            ("Sound".to_string(), "Volume...".to_string()),
            ("Keyboard".to_string(), "Shortcuts...".to_string()),
            ("Network".to_string(), "WiFi Settings...".to_string()),
        ];
        Self { settings }
    }

    /// Search settings by name (case-insensitive substring match).
    fn search_settings(&self, query: &str) -> Vec<SearchResult> {
        let query_lower = query.to_lowercase();
        self.settings
            .iter()
            .filter(|(_, title)| title.to_lowercase().contains(&query_lower))
            .map(|(cat, title)| SearchResult::Setting {
                category: cat.clone(),
                title: title.clone(),
            })
            .collect()
    }

    /// Search for apps by name (case-insensitive substring match).
    /// This would be called with results from `launch_services::scan_applications()`.
    pub fn search_apps(&self, query: &str, apps: &[AppBundle]) -> Vec<SearchResult> {
        let query_lower = query.to_lowercase();
        apps.iter()
            .filter(|app| app.name.to_lowercase().contains(&query_lower))
            .map(|app| SearchResult::App(app.clone()))
            .collect()
    }

    /// Perform a combined search across all scope types (apps, files, settings).
    /// Returns results in priority order (apps first, then files, then settings).
    pub fn search(&self, query: &str, apps: &[AppBundle]) -> Vec<SearchResult> {
        let mut results = Vec::new();

        // 1. Search apps
        results.extend(self.search_apps(query, apps));

        // 2. Search settings (simple hardcoded list for now)
        results.extend(self.search_settings(query));

        // File search would be async; not included here yet.

        results
    }
}

impl Default for SearchBackend {
    fn default() -> Self {
        Self::new()
    }
}

/// State of the Spotlight search overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpotlightState {
    /// Overlay is hidden.
    Hidden,
    /// Overlay is visible, waiting for user input.
    Visible,
    /// A search is in progress (file search in background).
    Searching,
}

/// Spotlight search overlay manager.
pub struct Spotlight {
    state: SpotlightState,
    query: String,
    backend: SearchBackend,
}

impl Spotlight {
    /// Create a new Spotlight instance.
    pub fn new() -> Self {
        Self {
            state: SpotlightState::Hidden,
            query: String::new(),
            backend: SearchBackend::new(),
        }
    }

    /// Show the overlay (invoked on Super+Space).
    pub fn show(&mut self) {
        self.state = SpotlightState::Visible;
        self.query.clear();
    }

    /// Hide the overlay.
    pub fn hide(&mut self) {
        self.state = SpotlightState::Hidden;
        self.query.clear();
    }

    /// Check if the overlay is visible.
    pub fn is_visible(&self) -> bool {
        self.state != SpotlightState::Hidden
    }

    /// Update the search query (append a character).
    pub fn append_char(&mut self, ch: char) {
        if self.state != SpotlightState::Hidden {
            self.query.push(ch);
        }
    }

    /// Delete the last character from the query.
    pub fn backspace(&mut self) {
        self.query.pop();
    }

    /// Get the current query string.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Perform the current search with the given app list.
    /// Returns results in priority order.
    pub fn search_results(&self, apps: &[AppBundle]) -> Vec<SearchResult> {
        if self.query.is_empty() {
            // Show featured apps when query is empty.
            apps.iter()
                .filter(|app| {
                    ["Finder", "Settings", "TextEdit", "Terminal"].contains(&app.name.as_str())
                })
                .map(|app| SearchResult::App(app.clone()))
                .collect()
        } else {
            self.backend.search(&self.query, apps)
        }
    }
}

impl Default for Spotlight {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_apps_by_name() {
        let backend = SearchBackend::new();
        let apps = vec![
            AppBundle {
                bundle_id: "com.slopos.finder".to_string(),
                name: "Finder".to_string(),
                version: "0.1.0".to_string(),
                path: "/Applications/Finder.app".to_string(),
                entrypoint: "bin/finder".to_string(),
                supported_types: vec![],
                permissions: vec![],
            },
            AppBundle {
                bundle_id: "com.slopos.terminal".to_string(),
                name: "Terminal".to_string(),
                version: "0.1.0".to_string(),
                path: "/Applications/Terminal.app".to_string(),
                entrypoint: "bin/terminal".to_string(),
                supported_types: vec![],
                permissions: vec![],
            },
        ];

        let results = backend.search_apps("find", &apps);
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], SearchResult::App(app) if app.name == "Finder"));
    }

    #[test]
    fn search_settings_by_name() {
        let backend = SearchBackend::new();
        let results = backend.search_settings("volume");
        assert!(!results.is_empty());
        assert!(results
            .iter()
            .any(|r| matches!(r, SearchResult::Setting { title, .. } if title.contains("Volume"))));
    }

    #[test]
    fn spotlight_visibility() {
        let mut spotlight = Spotlight::new();
        assert!(!spotlight.is_visible());

        spotlight.show();
        assert!(spotlight.is_visible());

        spotlight.hide();
        assert!(!spotlight.is_visible());
    }

    #[test]
    fn spotlight_query_input() {
        let mut spotlight = Spotlight::new();
        spotlight.show();

        spotlight.append_char('f');
        spotlight.append_char('i');
        spotlight.append_char('n');

        assert_eq!(spotlight.query(), "fin");

        spotlight.backspace();
        assert_eq!(spotlight.query(), "fi");
    }
}
