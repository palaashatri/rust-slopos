#![allow(dead_code, unused_imports)]

use slopos_kit::button::Button;
use slopos_kit::event::{KeyCode, Modifiers};
use slopos_kit::label::Label;
use slopos_kit::list_view::ListView;
use slopos_kit::progress_bar::ProgressBar;
use slopos_kit::text_field::TextField;
use slopos_kit::window::Window;
use slopos_kit::{
    widget_by_id, AccessibilityNode, Event, EventResult, FocusManager, LayoutConstraint,
    PointerDispatcher, Rect, Size, ThemeContext, Widget, WidgetState,
};
use slopos_sdk::{build_menu, Application};
use std::path::PathBuf;

mod bundle_install;

use bundle_install::{install_signed_archive, parse_signed_catalog, CatalogEntry, TrustStore};

// Featured apps shown when no search is active (catalog stub until Task 3.8).
const FEATURED_APPS: &[&str] = &["Finder", "TextEdit", "Settings", "Terminal"];

// Category definitions: (display name, search keywords)
const CATEGORIES: &[(&str, &[&str])] = &[
    ("ALL", &[]),
    ("SYSTEM", &["settings", "system"]),
    ("DEVELOPMENT", &["terminal", "dev"]),
    ("GAMES", &["game"]),
    ("MEDIA", &["media", "audio", "video"]),
    ("OFFICE", &["text", "edit", "document"]),
    ("NETWORK", &["network", "finder"]),
];

fn main() {
    let _ = tracing_subscriber::fmt::try_init();

    let mut app = Application::new("App Store", "com.slopos.appstore");

    let mut store_menu = build_menu("Store");
    store_menu.add_action("Refresh").with_shortcut(
        KeyCode::R,
        Modifiers {
            shift: false,
            control: false,
            alt: false,
            meta: true,
        },
    );
    store_menu.add_action("Search").with_shortcut(
        KeyCode::F,
        Modifiers {
            shift: false,
            control: false,
            alt: false,
            meta: true,
        },
    );

    let mut edit_menu = build_menu("Edit");
    edit_menu.add_action("Copy").with_shortcut(
        KeyCode::C,
        Modifiers {
            shift: false,
            control: false,
            alt: false,
            meta: true,
        },
    );
    edit_menu.add_action("Paste").with_shortcut(
        KeyCode::V,
        Modifiers {
            shift: false,
            control: false,
            alt: false,
            meta: true,
        },
    );

    let mut window_menu = build_menu("Window");
    window_menu.add_action("Minimize");

    let mut help_menu = build_menu("Help");
    help_menu.add_action("App Store Help");

    app.set_menus(vec![store_menu, edit_menu, window_menu, help_menu]);

    app.on_menu_action(|action, window| {
        let Some(content) = window.content.as_mut() else {
            return;
        };
        let Some(view) = content.as_any_mut().downcast_mut::<AppStoreView>() else {
            return;
        };
        let action = action
            .strip_prefix("com.slopos.appstore.")
            .unwrap_or(action);
        match action {
            "store.refresh" => {
                view.refresh_backend();
            }
            "store.search" => {
                view.focus_widget(view.query.id());
            }
            _ => {}
        }
    });

    let mut window = Window::new("App Store");
    window.set_content(Box::new(AppStoreView::new()));
    app.set_main_window(window);
    app.run();
}

// ── Authenticated catalog and trust-store projection ─────────────────────────

#[derive(Debug, Clone)]
struct CatalogStore {
    entries: Vec<CatalogEntry>,
    source: String,
    trust_store: Option<TrustStore>,
    load_error: Option<String>,
}

impl CatalogStore {
    fn load() -> Self {
        let path = catalog_path();
        let source = path.display().to_string();
        let trust_path = trust_store_path();
        let trust_store = match std::fs::metadata(&trust_path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Self {
                    entries: Vec::new(),
                    source,
                    trust_store: None,
                    load_error: Some(format!("trust store read failed: {error}")),
                };
            }
            Ok(_) => match TrustStore::load(&trust_path) {
                Ok(store) => Some(store),
                Err(error) => {
                    return Self {
                        entries: Vec::new(),
                        source,
                        trust_store: None,
                        load_error: Some(format!("trust store rejected: {error:?}")),
                    };
                }
            },
        };
        let mut load_error = None;
        let entries = match std::fs::read(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => {
                load_error = Some(format!("catalog read failed: {error}"));
                Vec::new()
            }
            Ok(bytes) => {
                let Some(store) = trust_store.as_ref() else {
                    load_error = Some(format!(
                        "signed catalog requires trust store {}",
                        trust_path.display()
                    ));
                    return Self {
                        entries: Vec::new(),
                        source,
                        trust_store: None,
                        load_error,
                    };
                };
                match parse_signed_catalog(&bytes).and_then(|catalog| {
                    catalog.verify(store)?;
                    Ok(catalog.entries)
                }) {
                    Ok(entries) => entries,
                    Err(error) => {
                        load_error = Some(format!("catalog authentication failed: {error:?}"));
                        Vec::new()
                    }
                }
            }
        };
        Self {
            entries,
            source,
            trust_store,
            load_error,
        }
    }

    fn status_text(&self) -> String {
        if let Some(error) = &self.load_error {
            format!("CATALOG UNAVAILABLE - {} ({error})", self.source)
        } else {
            format!(
                "CATALOG - {} ({} trusted apps)",
                self.source,
                self.entries.len()
            )
        }
    }

    fn list_lines(&self) -> Vec<String> {
        if self.load_error.is_some() {
            return Vec::new();
        }
        if self.entries.is_empty() {
            return featured_list();
        }
        self.entries
            .iter()
            .map(|e| format!("[AVAILABLE] {} - {}", e.name, e.version))
            .collect()
    }

    fn search(&self, query: &str) -> Result<Vec<String>, String> {
        let query = query.trim();
        if query.is_empty() {
            return Err("SEARCH NEEDS QUERY".to_string());
        }
        if let Some(error) = &self.load_error {
            return Err(format!("CATALOG UNAVAILABLE: {error}"));
        }
        let q = query.to_ascii_lowercase();
        let results: Vec<String> = self
            .entries
            .iter()
            .filter(|e| {
                e.name.to_ascii_lowercase().contains(&q)
                    || e.bundle_id.to_ascii_lowercase().contains(&q)
            })
            .map(|e| format!("[AVAILABLE] {} - {}", e.name, e.version))
            .collect();
        if results.is_empty() {
            // Fall back to static featured names for empty/missing catalogs.
            let fallback: Vec<String> = FEATURED_APPS
                .iter()
                .filter(|app| app.to_ascii_lowercase().contains(&q))
                .map(|app| format!("[AVAILABLE] {}", app))
                .collect();
            if fallback.is_empty() {
                Ok(vec![format!(
                    "NO RESULTS FOR {}",
                    query.to_ascii_uppercase()
                )])
            } else {
                Ok(fallback)
            }
        } else {
            Ok(results)
        }
    }

    fn entry_for_name(&self, name: &str) -> Option<&CatalogEntry> {
        self.entries
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(name))
    }

    fn app_details(&self, name: &str) -> AppDetails {
        if let Some(entry) = self.entry_for_name(name) {
            return AppDetails {
                name: entry.name.clone(),
                version: entry.version.clone(),
                description: format!(
                    "bundle_id={} publisher={} key_id={}",
                    entry.bundle_id, entry.publisher, entry.key_id
                ),
                state: AppInstallState::Available,
            };
        }
        AppDetails {
            name: name.to_string(),
            version: "0.1.0".to_string(),
            description: "SLOPOS-I application bundle.".to_string(),
            state: AppInstallState::Available,
        }
    }
}

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn default_install_dir() -> PathBuf {
    home_dir().join("Applications")
}

fn catalog_path() -> PathBuf {
    if let Ok(path) = std::env::var("SLOPOS_APPSTORE_CATALOG") {
        return PathBuf::from(path);
    }
    default_install_dir().join("catalog.json")
}

fn trust_store_path() -> PathBuf {
    if let Ok(path) = std::env::var("SLOPOS_APPSTORE_TRUST_STORE") {
        return PathBuf::from(path);
    }
    let config_root = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".config"));
    config_root.join("slopos-i").join("appstore-trust.json")
}

fn resolve_archive_url(url: &str) -> PathBuf {
    if let Some(rest) = url.strip_prefix("file://") {
        PathBuf::from(rest)
    } else {
        PathBuf::from(url)
    }
}

/// Best-effort rescan signal for the shell (Task 3.10 confirms runtime pickup).
fn request_shell_rescan() {
    let dir = default_install_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join(".slopos-rescan"), b"1\n");
}

// ── Domain types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppInstallState {
    Installed,
    Available,
    Unknown,
}

impl AppInstallState {
    fn label(self) -> &'static str {
        match self {
            Self::Installed => "INSTALLED",
            Self::Available => "AVAILABLE",
            Self::Unknown => "UNKNOWN",
        }
    }
}

#[derive(Debug, Clone)]
struct AppDetails {
    name: String,
    version: String,
    description: String,
    state: AppInstallState,
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn package_name_from_result(result: &str) -> Option<String> {
    let result = result
        .strip_prefix("[FEATURED] ")
        .or_else(|| result.strip_prefix("[INSTALLED] "))
        .or_else(|| result.strip_prefix("[AVAILABLE] "))
        .or_else(|| result.strip_prefix("[UNKNOWN] "))
        .unwrap_or(result);
    let first = result.split_whitespace().next()?.trim();
    let name = first
        .rsplit_once('/')
        .map(|(_, name)| name)
        .unwrap_or(first)
        .trim_matches(|c: char| matches!(c, ':' | ',' | ';'));
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn filter_by_category(items: &[String], keywords: &[&str]) -> Vec<String> {
    if keywords.is_empty() {
        return items.to_vec();
    }
    items
        .iter()
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            keywords.iter().any(|kw| lower.contains(*kw))
        })
        .cloned()
        .collect()
}

fn featured_list() -> Vec<String> {
    FEATURED_APPS
        .iter()
        .map(|app| format!("[FEATURED] {}", app))
        .collect()
}

// â”€â”€ UI View â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

struct AppStoreView {
    state: WidgetState,
    heading: Label,
    backend_label: Label,
    query: TextField,
    search_button: Button,
    refresh_button: Button,
    // Category sidebar
    category_list: ListView,
    // Package results
    results: ListView,
    // Detail panel
    detail_name: Label,
    detail_version: Label,
    detail_description: Label,
    detail_state: Label,
    install_button: Button,
    // Progress bar for async install
    progress_bar: ProgressBar,
    progress_label: Label,
    status: Label,
    backend: CatalogStore,
    /// Currently selected category index (matches CATEGORIES slice).
    category_index: usize,
    /// Whether we are in "featured" mode (empty search query).
    featured_mode: bool,
    /// All results before category filter applied (for re-filtering on category change).
    all_results: Vec<String>,
    focus: FocusManager,
    pointer: PointerDispatcher,
}

impl AppStoreView {
    fn new() -> Self {
        let mut query = TextField::new();
        query.set_text("");

        let mut category_list = ListView::new();
        for (name, _) in CATEGORIES {
            category_list.add_item(*name);
        }
        category_list.selected_index = Some(0);

        let backend = CatalogStore::load();

        let mut view = Self {
            state: WidgetState::new(),
            heading: Label::new("SOFTWARE CATALOG"),
            backend_label: Label::new(backend.status_text()),
            query,
            search_button: Button::new("SEARCH"),
            refresh_button: Button::new("REFRESH"),
            category_list,
            results: ListView::new(),
            detail_name: Label::new(""),
            detail_version: Label::new(""),
            detail_description: Label::new(""),
            detail_state: Label::new(""),
            install_button: Button::new("INSTALL"),
            progress_bar: ProgressBar::new(),
            progress_label: Label::new(""),
            status: Label::new("READY"),
            backend,
            category_index: 0,
            featured_mode: true,
            all_results: vec![],
            focus: FocusManager::new(),
            pointer: PointerDispatcher::new(),
        };
        view.load_featured();
        view
    }

    /// Focus `id` through the real focus system (sets `WidgetState.focused`
    /// on exactly that widget, clears it everywhere else in the tree).
    fn focus_widget(&mut self, id: slopos_kit::WidgetId) {
        let mut focus = std::mem::take(&mut self.focus);
        focus.focus(self, id);
        self.focus = focus;
    }

    /// Load featured packages when search query is empty.
    fn load_featured(&mut self) {
        self.featured_mode = true;
        self.all_results = self.backend.list_lines();
        self.apply_category_filter();
        self.status.text = format!("FEATURED - {} APPS", self.results.items.len());
        self.clear_detail();
    }

    fn run_search(&mut self) -> bool {
        let query = self.query.text().trim().to_string();
        if query.is_empty() {
            self.load_featured();
            return true;
        }
        self.featured_mode = false;
        match self.backend.search(&query) {
            Ok(results) => {
                self.all_results = results;
                self.apply_category_filter();
                self.status.text = format!("{} RESULTS", self.results.items.len());
                self.clear_detail();
                true
            }
            Err(err) => {
                self.all_results = vec![];
                self.results.items = vec![err.clone()];
                self.results.selected_index = None;
                self.status.text = err;
                self.clear_detail();
                false
            }
        }
    }

    /// Re-filter `all_results` through the selected category and populate `results`.
    fn apply_category_filter(&mut self) {
        let (_, keywords) = CATEGORIES[self.category_index];
        let filtered = filter_by_category(&self.all_results, keywords);
        self.results.items = filtered;
        self.results.selected_index = (!self.results.items.is_empty()).then_some(0);
    }

    fn refresh_backend(&mut self) -> bool {
        self.backend = CatalogStore::load();
        self.backend_label.text = self.backend.status_text();
        let saved_status = self.status.text.clone();
        let ok = if self.featured_mode {
            self.load_featured();
            true
        } else {
            self.run_search()
        };
        if saved_status.starts_with("INSTALLED") || saved_status.starts_with("INSTALL FAILED") {
            self.status.text = saved_status;
        }
        ok
    }

    fn selected_package(&self) -> Option<String> {
        self.results
            .selected_index
            .and_then(|index| self.results.items.get(index))
            .and_then(|line| package_name_from_result(line))
            .or_else(|| {
                let query = self.query.text().trim();
                (!query.is_empty()).then(|| query.to_string())
            })
    }

    /// Populate the detail panel for the given package name.
    fn show_package_detail(&mut self, package: &str) {
        let details = self.backend.app_details(package);
        self.detail_name.text = format!("APP: {}", details.name.to_ascii_uppercase());
        self.detail_version.text = format!("VERSION: {}", details.version);
        let mut d = details.description.clone();
        d.truncate(120);
        self.detail_description.text = d;
        self.detail_state.text = format!("STATUS: {}", details.state.label());
    }

    fn clear_detail(&mut self) {
        self.detail_name.text = String::new();
        self.detail_version.text = String::new();
        self.detail_description.text = String::new();
        self.detail_state.text = String::new();
    }

    fn start_install_async(&mut self) {
        let Some(package) = self.selected_package() else {
            self.status.text = "SELECT AN APP FIRST".to_string();
            return;
        };
        let Some(entry) = self.backend.entry_for_name(&package).cloned() else {
            self.status.text = format!("NO CATALOG ENTRY FOR {package}");
            self.progress_label.text = self.status.text.clone();
            return;
        };

        let archive = resolve_archive_url(&entry.url);
        let install_dir = default_install_dir();
        self.progress_bar.indeterminate = true;
        self.progress_label.text = format!("Installing {}...", entry.name);
        self.status.text = format!("INSTALLING {}", entry.name);

        let Some(trust_store) = self.backend.trust_store.as_ref() else {
            self.progress_bar.indeterminate = false;
            self.status.text = "INSTALL FAILED: TRUST STORE UNAVAILABLE".to_string();
            self.progress_label.text = self.status.text.clone();
            return;
        };
        match install_signed_archive(&archive, &entry, trust_store, &install_dir) {
            Ok(path) => {
                self.progress_bar.indeterminate = false;
                self.progress_bar.value = 1.0;
                request_shell_rescan();
                self.status.text = format!("INSTALLED {}", path.display());
                self.progress_label.text = self.status.text.clone();
                self.refresh_backend();
            }
            Err(err) => {
                self.progress_bar.indeterminate = false;
                self.status.text = format!("INSTALL FAILED: {err:?}");
                self.progress_label.text = self.status.text.clone();
            }
        }
    }

    /// Drain button activations after an event went through generic
    /// dispatch. Returns whether an action ran.
    fn process_activations(&mut self) -> bool {
        if self.search_button.take_clicked() {
            self.run_search();
            return true;
        }
        if self.refresh_button.take_clicked() {
            self.refresh_backend();
            return true;
        }
        if self.install_button.take_clicked() {
            self.start_install_async();
            return true;
        }
        false
    }

    /// React to a list selection made by a press the dispatcher attributed
    /// to one of the two lists.
    fn react_to_list_press(&mut self, pressed: slopos_kit::WidgetId) {
        if pressed == self.category_list.id() {
            if let Some(idx) = self.category_list.selected_index {
                if idx < CATEGORIES.len() && idx != self.category_index {
                    self.category_index = idx;
                    self.apply_category_filter();
                    self.status.text = format!(
                        "CATEGORY - {} | {} RESULTS",
                        CATEGORIES[idx].0,
                        self.results.items.len()
                    );
                    self.clear_detail();
                }
            }
        } else if pressed == self.results.id() {
            if let Some(package) = self.selected_package() {
                self.status.text = format!("SELECTED - {}", package);
                self.show_package_detail(&package);
            }
        }
    }
}

impl Widget for AppStoreView {
    fn widget_state(&self) -> &WidgetState {
        &self.state
    }

    fn widget_state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        let size = constraint.clamp(Size::new(constraint.max_width, constraint.max_height));
        let rect = Rect::new(self.rect().x, self.rect().y, size.width, size.height);
        self.set_rect(rect);

        let pad = 18.0;
        let content_x = rect.x + pad;
        let content_w = (rect.width - pad * 2.0).max(0.0);
        let mut y = rect.y + 18.0;

        // Heading row
        self.heading
            .set_rect(Rect::new(content_x, y, content_w, 24.0));
        let _ = self
            .heading
            .layout(LayoutConstraint::tight(Size::new(content_w, 24.0)));
        y += 30.0;

        // Backend label
        self.backend_label
            .set_rect(Rect::new(content_x, y, content_w, 20.0));
        let _ = self
            .backend_label
            .layout(LayoutConstraint::tight(Size::new(content_w, 20.0)));
        y += 28.0;

        // Search bar row
        let search_field_w = content_w.min(260.0);
        self.query
            .set_rect(Rect::new(content_x, y, search_field_w, 28.0));
        let _ = self
            .query
            .layout(LayoutConstraint::tight(Size::new(search_field_w, 28.0)));

        let btn_y = y;
        let btn_x = content_x + search_field_w + 12.0;
        self.search_button
            .set_rect(Rect::new(btn_x, btn_y, 88.0, 28.0));
        let _ = self
            .search_button
            .layout(LayoutConstraint::tight(Size::new(88.0, 28.0)));

        self.refresh_button
            .set_rect(Rect::new(btn_x + 96.0, btn_y, 96.0, 28.0));
        let _ = self
            .refresh_button
            .layout(LayoutConstraint::tight(Size::new(96.0, 28.0)));
        y += 44.0;

        // Action buttons row
        let action_w = 94.0;
        self.install_button
            .set_rect(Rect::new(content_x, y, action_w, 28.0));
        let _ = self
            .install_button
            .layout(LayoutConstraint::tight(Size::new(action_w, 28.0)));
        y += 42.0;

        // Progress bar row (always laid out; visible when active)
        let pb_h = 14.0;
        self.progress_bar
            .set_rect(Rect::new(content_x, y, content_w.min(320.0), pb_h));
        let _ = self.progress_bar.layout(LayoutConstraint::tight(Size::new(
            content_w.min(320.0),
            pb_h,
        )));
        self.progress_label.set_rect(Rect::new(
            content_x + content_w.min(320.0) + 10.0,
            y,
            content_w - content_w.min(320.0) - 10.0,
            pb_h,
        ));
        let _ = self
            .progress_label
            .layout(LayoutConstraint::tight(Size::new(
                content_w - content_w.min(320.0) - 10.0,
                pb_h,
            )));
        y += pb_h + 8.0;

        // Main area: category sidebar (left) + package list (center) + detail panel (right)
        let status_h = 26.0;
        let main_h = (rect.height - (y - rect.y) - status_h - pad).max(0.0);

        let cat_w = 110.0;
        let detail_w = 220.0;
        let list_w = (content_w - cat_w - detail_w - 8.0 - 8.0).max(80.0);

        // Category sidebar
        let cat_x = content_x;
        self.category_list
            .set_rect(Rect::new(cat_x, y, cat_w, main_h));
        let _ = self
            .category_list
            .layout(LayoutConstraint::tight(Size::new(cat_w, main_h)));

        // Package results list
        let list_x = cat_x + cat_w + 8.0;
        self.results.set_rect(Rect::new(list_x, y, list_w, main_h));
        let _ = self
            .results
            .layout(LayoutConstraint::tight(Size::new(list_w, main_h)));

        // Detail panel on the right
        let detail_x = list_x + list_w + 8.0;
        let mut dy = y;
        let row_h = 22.0;
        let row_gap = 4.0;

        self.detail_name
            .set_rect(Rect::new(detail_x, dy, detail_w, row_h));
        let _ = self
            .detail_name
            .layout(LayoutConstraint::tight(Size::new(detail_w, row_h)));
        dy += row_h + row_gap;

        self.detail_version
            .set_rect(Rect::new(detail_x, dy, detail_w, row_h));
        let _ = self
            .detail_version
            .layout(LayoutConstraint::tight(Size::new(detail_w, row_h)));
        dy += row_h + row_gap;

        self.detail_state
            .set_rect(Rect::new(detail_x, dy, detail_w, row_h));
        let _ = self
            .detail_state
            .layout(LayoutConstraint::tight(Size::new(detail_w, row_h)));
        dy += row_h + row_gap;

        // Description can be taller
        let desc_h = (row_h * 3.0).min(main_h - (dy - y) - 4.0).max(row_h);
        self.detail_description
            .set_rect(Rect::new(detail_x, dy, detail_w, desc_h));
        let _ = self
            .detail_description
            .layout(LayoutConstraint::tight(Size::new(detail_w, desc_h)));

        y += main_h;

        // Status bar
        self.status
            .set_rect(Rect::new(content_x, y, content_w, status_h));
        let _ = self
            .status
            .layout(LayoutConstraint::tight(Size::new(content_w, status_h)));

        size
    }

    fn draw(&self, theme: &ThemeContext) {
        self.heading.draw(theme);
        self.backend_label.draw(theme);
        self.query.draw(theme);
        self.search_button.draw(theme);
        self.refresh_button.draw(theme);
        self.install_button.draw(theme);
        self.progress_bar.draw(theme);
        self.progress_label.draw(theme);
        self.category_list.draw(theme);
        self.results.draw(theme);
        self.detail_name.draw(theme);
        self.detail_version.draw(theme);
        self.detail_state.draw(theme);
        self.detail_description.draw(theme);
        self.status.draw(theme);
    }

    fn handle_event(&mut self, event: &Event) -> EventResult {
        if let Event::KeyDown { key, modifiers } = event {
            if modifiers.meta && matches!(key, KeyCode::R | KeyCode::F) {
                if matches!(key, KeyCode::R) {
                    self.refresh_backend();
                } else {
                    self.run_search();
                }
                return EventResult::Handled;
            }
        }

        match event {
            // Tab / Shift+Tab walk the focusable widgets (buttons + query).
            Event::KeyDown {
                key: KeyCode::Tab,
                modifiers,
            } => {
                let mut focus = std::mem::take(&mut self.focus);
                if modifiers.shift {
                    focus.focus_prev(self);
                } else {
                    focus.focus_next(self);
                }
                self.focus = focus;
                EventResult::Handled
            }
            // Focused widget first (typing into the query field, Enter/Space
            // on a focused button); an unclaimed Enter runs the search.
            Event::KeyDown { .. } | Event::KeyUp { .. } | Event::Char { .. } => {
                let mut focus = std::mem::take(&mut self.focus);
                let result = focus.dispatch_key(self, event);
                self.focus = focus;
                if matches!(result, EventResult::Handled) {
                    self.process_activations();
                    return EventResult::Handled;
                }
                if matches!(
                    event,
                    Event::KeyDown {
                        key: KeyCode::Enter,
                        ..
                    }
                ) {
                    self.run_search();
                    return EventResult::Handled;
                }
                result
            }
            // Pointer events go through generic rect-checked dispatch with
            // implicit capture; a press on the query field moves focus there.
            Event::MouseDown { .. }
            | Event::MouseUp { .. }
            | Event::MouseMove { .. }
            | Event::DoubleClick { .. }
            | Event::MouseLeave => {
                let mut pointer = std::mem::take(&mut self.pointer);
                let result = pointer.dispatch(self, event);
                let pressed = match event {
                    Event::MouseDown { .. } | Event::DoubleClick { .. } => pointer.captured(),
                    _ => None,
                };
                self.pointer = pointer;
                if let Some(id) = pressed {
                    if widget_by_id(self, id).is_some_and(|w| w.wants_click_focus()) {
                        self.focus_widget(id);
                    }
                }
                if matches!(result, EventResult::Handled) && !self.process_activations() {
                    if let Some(id) = pressed {
                        self.react_to_list_press(id);
                    }
                }
                result
            }
            _ => EventResult::Ignored,
        }
    }

    fn update(&mut self) {
        self.heading.update();
        self.backend_label.update();
        self.query.update();
        self.search_button.update();
        self.refresh_button.update();
        self.install_button.update();
        self.progress_bar.update();
        self.progress_label.update();
        self.category_list.update();
        self.results.update();
        self.detail_name.update();
        self.detail_version.update();
        self.detail_state.update();
        self.detail_description.update();
        self.status.update();
    }

    fn accessibility(&self) -> Option<AccessibilityNode> {
        None
    }

    fn children(&self) -> Vec<&dyn Widget> {
        vec![
            &self.heading,
            &self.backend_label,
            &self.query,
            &self.search_button,
            &self.refresh_button,
            &self.install_button,
            &self.progress_bar,
            &self.progress_label,
            &self.category_list,
            &self.results,
            &self.detail_name,
            &self.detail_version,
            &self.detail_state,
            &self.detail_description,
            &self.status,
        ]
    }

    fn children_mut(&mut self) -> Vec<&mut dyn Widget> {
        vec![
            &mut self.heading,
            &mut self.backend_label,
            &mut self.query,
            &mut self.search_button,
            &mut self.refresh_button,
            &mut self.install_button,
            &mut self.progress_bar,
            &mut self.progress_label,
            &mut self.category_list,
            &mut self.results,
            &mut self.detail_name,
            &mut self.detail_version,
            &mut self.detail_state,
            &mut self.detail_description,
            &mut self.status,
        ]
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// â”€â”€ Tests â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[cfg(test)]
mod tests {
    use super::*;
    use slopos_kit::event::MouseButton;
    use slopos_kit::Point;

    fn click(view: &mut AppStoreView, point: Point) -> EventResult {
        let down = view.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point,
            modifiers: Modifiers::NONE,
        });
        assert!(
            matches!(down, EventResult::Handled),
            "press must land on a widget"
        );
        view.handle_event(&Event::MouseUp {
            button: MouseButton::Left,
            point,
            modifiers: Modifiers::NONE,
        })
    }

    fn rect_center(rect: Rect) -> Point {
        Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
    }

    #[test]
    fn appstore_search_button_click_dispatches_search() {
        let mut view = AppStoreView::new();
        view.layout(LayoutConstraint::tight(Size::new(900.0, 640.0)));
        view.query.set_text("zzz");

        let point = rect_center(view.search_button.rect());
        let result = click(&mut view, point);

        assert!(matches!(result, EventResult::Handled));
        assert!(
            view.results
                .items
                .iter()
                .any(|line| line.contains("NO RESULTS"))
                || view.status.text.contains("0 RESULTS"),
            "search ran with no matches: status={} results={:?}",
            view.status.text,
            view.results.items
        );
    }

    #[test]
    fn appstore_results_click_selects_app_and_fills_detail() {
        let mut view = AppStoreView::new();
        view.layout(LayoutConstraint::tight(Size::new(900.0, 640.0)));
        view.results.items = vec![
            "[AVAILABLE] Finder".to_string(),
            "[AVAILABLE] TextEdit".to_string(),
        ];
        view.results.selected_index = None;

        let rect = view.results.rect();
        let point = Point::new(rect.x + 10.0, rect.y + 3.0 + 18.0 + 9.0);
        let down = view.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point,
            modifiers: Modifiers::NONE,
        });

        assert!(matches!(down, EventResult::Handled));
        assert_eq!(view.results.selected_index, Some(1));
        assert!(
            view.status.text.contains("SELECTED - TextEdit"),
            "{}",
            view.status.text
        );
        assert!(view.detail_name.text.contains("TEXTEDIT"));
    }

    #[test]
    fn appstore_query_click_focuses_field_and_typing_lands_there() {
        let mut view = AppStoreView::new();
        view.layout(LayoutConstraint::tight(Size::new(900.0, 640.0)));

        let rect = view.query.rect();
        let down = view.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point: Point::new(rect.x + 4.0, rect.y + 4.0),
            modifiers: Modifiers::NONE,
        });
        assert!(matches!(down, EventResult::Handled));
        assert!(view.query.widget_state().focused);

        let typed = view.handle_event(&Event::Char { character: 'q' });
        assert!(matches!(typed, EventResult::Handled));
        assert_eq!(view.query.text(), "q");
    }

    #[test]
    fn catalog_store_search_filters_fallback_featured_apps() {
        let backend = CatalogStore {
            entries: vec![],
            source: "empty".into(),
            trust_store: None,
            load_error: None,
        };
        let results = backend.search("text").expect("search ok");
        assert_eq!(results, vec!["[AVAILABLE] TextEdit".to_string()]);
    }

    #[test]
    fn appstore_search_reports_featured_on_startup() {
        let view = AppStoreView::new();
        assert!(view.status.text.contains("FEATURED"));
    }

    #[test]
    fn package_name_extracts_common_result_formats() {
        assert_eq!(
            package_name_from_result("[FEATURED] Finder").as_deref(),
            Some("Finder")
        );
        assert_eq!(
            package_name_from_result("[AVAILABLE] TextEdit").as_deref(),
            Some("TextEdit")
        );
        assert_eq!(
            package_name_from_result("[INSTALLED] doom - game").as_deref(),
            Some("doom")
        );
    }

    #[test]
    fn appstore_install_requires_catalog_entry() {
        let mut view = AppStoreView::new();
        view.layout(LayoutConstraint::tight(Size::new(900.0, 640.0)));
        view.results.items = vec!["[AVAILABLE] NonExistentPackage123".to_string()];
        view.results.selected_index = Some(0);

        let point = rect_center(view.install_button.rect());
        click(&mut view, point);
        assert!(
            view.status.text.contains("NO CATALOG ENTRY")
                || view.status.text.contains("INSTALL FAILED")
                || view.status.text.contains("INSTALLED"),
            "{}",
            view.status.text
        );
    }

    #[test]
    fn appstore_does_not_advertise_unimplemented_transaction_controls() {
        let view = AppStoreView::new();
        let button_labels: Vec<&str> = view
            .children()
            .into_iter()
            .filter_map(|child| child.as_any().downcast_ref::<Button>())
            .map(Button::label)
            .collect();

        assert!(!button_labels
            .iter()
            .any(|label| { matches!(*label, "REMOVE" | "UPDATE" | "CONFIRM") }));
    }

    #[test]
    fn category_filter_all_returns_all_items() {
        let items = vec![
            "curl - transfer tool".to_string(),
            "vim - editor".to_string(),
        ];
        let filtered = filter_by_category(&items, &[]);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn category_filter_keywords_match_subset() {
        let items = vec![
            "curl - network transfer".to_string(),
            "vim - editor".to_string(),
            "wget - network downloader".to_string(),
        ];
        let filtered = filter_by_category(&items, &["network"]);
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|l| l.contains("network")));
    }

    #[test]
    fn featured_list_returns_expected_count() {
        let items = featured_list();
        assert_eq!(items.len(), FEATURED_APPS.len());
        assert!(items[0].contains("[FEATURED]"));
    }

    #[test]
    fn category_index_switches_apply_filter() {
        let mut view = AppStoreView::new();
        view.all_results = vec![
            "[FEATURED] Finder".to_string(),
            "[FEATURED] TextEdit".to_string(),
            "[FEATURED] Terminal".to_string(),
        ];
        view.category_index = 6;
        view.apply_category_filter();
        assert!(!view.results.items.is_empty());
    }
}
