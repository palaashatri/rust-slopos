//! SLOPOS application Search palette.

use crate::app_finder::{ranked_app_matches, DesktopApp};
use crate::app_index::AppIndexUpdate;
use gdk_pixbuf::Pixbuf;
use gtk::atk::prelude::AtkObjectExt;
use gtk::prelude::*;
use gtk::{
    Box as GtkBox, Entry, IconSize, Image, Label, ListBox, ListBoxRow, Orientation, PolicyType,
    ScrolledWindow, SelectionMode, Window, WindowPosition, WindowType,
};
use std::cell::RefCell;
use std::env;
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Debug, Default, Clone)]
pub struct AppIndexState {
    pub apps: Arc<Vec<DesktopApp>>,
    pub last_seq: u64,
}

impl AppIndexState {
    pub fn update(&mut self, update: AppIndexUpdate) -> bool {
        if update.seq > self.last_seq {
            self.last_seq = update.seq;
            self.apps = update.apps;
            true
        } else {
            false
        }
    }
}

pub struct Launcher {
    window: Window,
    search_entry: Entry,
    list_box: ListBox,
    status_label: Label,
    state: RefCell<AppIndexState>,
}

impl Launcher {
    pub fn new() -> Rc<Self> {
        let window = Window::new(WindowType::Toplevel);
        window.set_title("SLOPOS Search");
        window.set_app_paintable(true);
        if let Some(screen) = gtk::prelude::GtkWindowExt::screen(&window) {
            if let Some(visual) = screen.rgba_visual() {
                window.set_visual(Some(&visual));
            }
        }
        let (screen_width, screen_height) = screen_geometry();
        let (window_width, window_height) = adaptive_window_size(screen_width, screen_height);
        window.set_default_size(560, 450);
        if (window_width, window_height) != (560, 450) {
            window.set_default_size(window_width, window_height);
        }
        window.set_position(WindowPosition::Center);
        window.set_decorated(false);
        window.set_keep_above(true);
        window.set_skip_taskbar_hint(true);
        window.style_context().add_class("slopos-launcher-window");
        window.connect_draw(|_, cr| {
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
            cr.set_operator(gtk::cairo::Operator::Source);
            let _ = cr.paint();
            glib::Propagation::Proceed
        });
        set_accessible_name(&window, "SLOPOS application search");

        let main_box = GtkBox::new(Orientation::Vertical, 6);
        main_box.style_context().add_class("slopos-launcher");

        let title = Label::new(Some("Find Applications"));
        title.style_context().add_class("slopos-dialog-title");
        title.set_xalign(0.0);
        main_box.pack_start(&title, false, false, 0);

        let search_entry = Entry::new();
        search_entry.set_placeholder_text(Some("Type an application name…"));
        search_entry.set_icon_from_icon_name(
            gtk::EntryIconPosition::Primary,
            Some("system-search-symbolic"),
        );
        search_entry
            .style_context()
            .add_class("slopos-search-entry");
        search_entry.set_tooltip_text(Some("Search installed desktop applications"));
        set_accessible_name(&search_entry, "Application search field");
        main_box.pack_start(&search_entry, false, false, 0);

        let scroll = ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
        scroll.set_policy(PolicyType::Never, PolicyType::Automatic);
        scroll.set_overlay_scrolling(false);
        scroll.set_min_content_height(280);
        let scroll_height = (window_height - 170).max(280);
        if scroll_height != 280 {
            scroll.set_min_content_height(scroll_height);
        }
        scroll.style_context().add_class("slopos-list-frame");

        let list_box = ListBox::new();
        list_box.set_selection_mode(SelectionMode::Single);
        list_box.style_context().add_class("slopos-search-results");
        set_accessible_name(&list_box, "Application search results");
        scroll.add(&list_box);
        main_box.pack_start(&scroll, true, true, 0);

        let status_label = Label::new(Some(""));
        status_label.style_context().add_class("slopos-statusbar");
        status_label.set_xalign(0.0);
        set_accessible_name(&status_label, "Search result status");
        main_box.pack_start(&status_label, false, false, 0);

        window.add(&main_box);

        let launcher = Rc::new(Self {
            window,
            search_entry,
            list_box,
            status_label,
            state: RefCell::new(AppIndexState::default()),
        });
        launcher.setup_events();
        launcher
    }

    fn setup_events(self: &Rc<Self>) {
        let launcher = self.clone();
        self.search_entry.connect_changed(move |entry| {
            launcher.filter_apps(&entry.text().to_lowercase());
        });

        let launcher = self.clone();
        self.search_entry
            .connect_activate(move |_| launcher.launch_selected_or_first());

        self.window.connect_focus_out_event(|window, _| {
            window.hide();
            glib::Propagation::Proceed
        });

        let launcher = self.clone();
        self.window.connect_key_press_event(move |window, event| {
            match event.keyval() {
                gdk::keys::constants::Escape => {
                    window.hide();
                    return glib::Propagation::Stop;
                }
                gdk::keys::constants::Down => {
                    launcher.move_selection(1);
                    return glib::Propagation::Stop;
                }
                gdk::keys::constants::Up => {
                    launcher.move_selection(-1);
                    return glib::Propagation::Stop;
                }
                _ => {}
            }
            glib::Propagation::Proceed
        });
    }

    pub fn update_index(&self, update: AppIndexUpdate) {
        let updated = self.state.borrow_mut().update(update);
        if updated && self.window.is_visible() {
            let query = self.search_entry.text().to_lowercase();
            self.filter_apps(&query);
        }
    }

    pub fn toggle(&self) {
        if self.window.is_visible() {
            self.window.hide();
        } else {
            self.show();
        }
    }

    pub fn show(&self) {
        self.search_entry.set_text("");
        self.filter_apps("");
        self.window.show_all();
        self.window.present();
        self.search_entry.grab_focus();
    }

    fn filter_apps(&self, query: &str) {
        filter_apps_internal(
            &self.list_box,
            &self.status_label,
            &self.window,
            &self.state.borrow().apps,
            query,
        );
    }

    fn launch_selected_or_first(&self) {
        if let Some(row) = self.list_box.selected_row().or_else(|| self.first_row()) {
            row.activate();
        }
    }

    fn first_row(&self) -> Option<ListBoxRow> {
        self.list_box
            .children()
            .first()
            .and_then(|widget| widget.clone().downcast::<ListBoxRow>().ok())
    }

    fn move_selection(&self, direction: isize) {
        let rows: Vec<ListBoxRow> = self
            .list_box
            .children()
            .into_iter()
            .filter_map(|widget| widget.downcast::<ListBoxRow>().ok())
            .collect();
        if rows.is_empty() {
            return;
        }

        let current = self
            .list_box
            .selected_row()
            .and_then(|selected| rows.iter().position(|row| row == &selected))
            .unwrap_or(0) as isize;
        let next = (current + direction).clamp(0, rows.len() as isize - 1) as usize;
        self.list_box.select_row(Some(&rows[next]));
    }
}

fn filter_apps_internal(
    list_box: &ListBox,
    status_label: &Label,
    window: &Window,
    all_apps: &[DesktopApp],
    query: &str,
) {
    for child in list_box.children() {
        list_box.remove(&child);
    }

    let matches = ranked_app_matches(all_apps, query);
    let count = matches.len();
    for app in matches.iter() {
        let row = ListBoxRow::new();
        row.style_context().add_class("slopos-list-row");
        let accessible_name = if app.comment.is_empty() {
            app.name.clone()
        } else {
            format!("{} — {}", app.name, app.comment)
        };
        set_accessible_name(&row, &accessible_name);
        row.set_tooltip_text(Some(&accessible_name));
        let hbox = GtkBox::new(Orientation::Horizontal, 9);
        hbox.set_margin_start(7);
        hbox.set_margin_end(7);
        hbox.set_margin_top(4);
        hbox.set_margin_bottom(4);

        let icon = load_launcher_icon(app);
        icon.style_context().add_class("slopos-result-icon");
        hbox.pack_start(&icon, false, false, 0);

        let labels = GtkBox::new(Orientation::Vertical, 1);
        let title = Label::new(Some(&app.name));
        title.set_xalign(0.0);
        title.set_single_line_mode(true);
        title.set_ellipsize(pango::EllipsizeMode::End);
        title.set_hexpand(true);
        title.style_context().add_class("slopos-result-title");
        labels.pack_start(&title, false, false, 0);
        if !app.comment.is_empty() {
            let description = Label::new(Some(&app.comment));
            description.set_xalign(0.0);
            description.set_single_line_mode(true);
            description.set_ellipsize(pango::EllipsizeMode::End);
            description.set_hexpand(true);
            description
                .style_context()
                .add_class("slopos-secondary-text");
            labels.pack_start(&description, false, false, 0);
        }
        hbox.pack_start(&labels, true, true, 0);
        row.add(&hbox);

        let app = app.clone();
        let window_c = window.clone();
        row.connect_activate(move |_| {
            if let Err(error) = spawn_app(&app) {
                log::warn!("Failed to launch {}: {error}", app.name);
            }
            window_c.hide();
        });
        list_box.add(&row);
    }

    status_label.set_text(&format!("{count} matching applications"));
    list_box.show_all();

    if let Some(first) = list_box
        .children()
        .first()
        .and_then(|widget| widget.clone().downcast::<ListBoxRow>().ok())
    {
        list_box.select_row(Some(&first));
    }
}

fn load_launcher_icon(app: &DesktopApp) -> Image {
    if let Some(file_name) = role_icon_file(app) {
        let mut candidates = Vec::new();
        if let Ok(share_dir) = env::var("SLOPOS_SHARE_DIR") {
            candidates.push(
                PathBuf::from(share_dir)
                    .join("slopos-i/themes/platinum/icons")
                    .join(file_name),
            );
        }
        candidates.extend([
            PathBuf::from("themes/platinum/icons").join(file_name),
            PathBuf::from("/usr/local/share/slopos-i/themes/platinum/icons").join(file_name),
            PathBuf::from("/usr/share/slopos-i/themes/platinum/icons").join(file_name),
        ]);
        for path in candidates {
            if path.is_file() {
                if let Ok(pixbuf) = Pixbuf::from_file_at_scale(&path, 32, 32, true) {
                    return Image::from_pixbuf(Some(&pixbuf));
                }
            }
        }
    }

    let icon_name = if app.icon.is_empty() {
        "application-x-executable"
    } else {
        &app.icon
    };
    Image::from_icon_name(Some(icon_name), IconSize::Dnd)
}

fn role_icon_file(app: &DesktopApp) -> Option<&'static str> {
    let command = app.argv.first().map(String::as_str).unwrap_or_default();
    let haystack = format!("{} {} {}", app.id, app.name, command).to_ascii_lowercase();

    if haystack.contains("file") || haystack.contains("pcmanfm") || haystack.contains("thunar") {
        return Some("folder.svg");
    }
    if haystack.contains("term") || haystack.contains("xterm") {
        return Some("terminal.svg");
    }
    if haystack.contains("text") || haystack.contains("gedit") || haystack.contains("mousepad") {
        return Some("textedit.svg");
    }
    if haystack.contains("browser") || haystack.contains("firefox") || haystack.contains("chrome") {
        return Some("browser.svg");
    }
    if haystack.contains("game") || haystack.contains("doom") || haystack.contains("supertux") {
        return Some("game.svg");
    }
    if haystack.contains("settings") || haystack.contains("control") {
        return Some("settings.svg");
    }
    if haystack.contains("software")
        || haystack.contains("package")
        || haystack.contains("catalogue")
    {
        return Some("software.svg");
    }

    None
}

fn screen_geometry() -> (i32, i32) {
    if let Some(display) = gdk::Display::default() {
        if let Some(monitor) = display.primary_monitor().or_else(|| display.monitor(0)) {
            let geom = monitor.geometry();
            return (geom.width(), geom.height());
        }
    }
    (1280, 800)
}

fn adaptive_window_size(screen_width: i32, screen_height: i32) -> (i32, i32) {
    let width = (screen_width * 55 / 100).clamp(560, 920);
    let height = (screen_height * 55 / 100).clamp(450, 760);
    (width, height)
}

fn spawn_app(app: &DesktopApp) -> std::io::Result<()> {
    if app.argv.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Application argv is empty",
        ));
    }

    let program = &app.argv[0];
    let args = &app.argv[1..];

    if app.terminal {
        let terminals = ["xfce4-terminal", "xterm"];
        for term in terminals {
            if let Some(term_path) = which(term) {
                let mut cmd = Command::new(term_path);
                cmd.arg("-e");
                cmd.arg(program);
                for arg in args {
                    cmd.arg(arg);
                }
                return cmd.spawn().map(|_| ());
            }
        }
    }

    if is_browser_command(program) {
        if let Some(wrapper) = which("start-slopos-browser") {
            let mut cmd = Command::new(wrapper);
            cmd.args(args);
            return cmd.spawn().map(|_| ());
        }
    }

    let mut cmd = Command::new(program);
    cmd.args(args);
    cmd.spawn().map(|_| ())
}

fn is_browser_command(program: &str) -> bool {
    let lower = program.to_ascii_lowercase();
    lower.ends_with("firefox")
        || lower.ends_with("firefox-esr")
        || lower.ends_with("chromium")
        || lower.ends_with("chromium-browser")
        || lower.ends_with("chrome")
        || lower.ends_with("google-chrome")
        || lower.ends_with("brave")
        || lower.ends_with("brave-browser")
        || lower.ends_with("epiphany")
}

fn which(program: &str) -> Option<PathBuf> {
    if program.starts_with('/') {
        let path = PathBuf::from(program);
        if path.is_file() {
            return Some(path);
        }
        return None;
    }

    if let Ok(path_var) = env::var("PATH") {
        for dir in env::split_paths(&path_var) {
            let candidate = dir.join(program);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn set_accessible_name<W: IsA<gtk::Widget>>(widget: &W, name: &str) {
    if let Some(accessible) = widget.accessible() {
        accessible.set_name(name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launcher_keeps_compact_canonical_size_and_scales_large_surfaces() {
        assert_eq!(adaptive_window_size(1280, 800), (704, 450));
        assert_eq!(adaptive_window_size(1024, 768), (563, 450));
        assert_eq!(adaptive_window_size(1920, 1080), (920, 594));
        assert_eq!(adaptive_window_size(3840, 2160), (920, 760));
    }

    #[test]
    fn search_recognizes_upstream_browser_commands_for_wrapper_routing() {
        assert!(is_browser_command("firefox"));
        assert!(is_browser_command("/usr/bin/firefox-esr"));
        assert!(is_browser_command("/usr/local/bin/chromium"));
        assert!(is_browser_command("google-chrome"));
        assert!(is_browser_command("/opt/brave.com/brave/brave-browser"));
        assert!(!is_browser_command("thunar"));
        assert!(!is_browser_command("xfce4-terminal"));
    }

    #[test]
    fn update_index_ignores_stale_or_out_of_order_sequences() {
        let mut state = AppIndexState::default();

        let app1 = DesktopApp {
            id: "app1.desktop".to_string(),
            name: "App One".to_string(),
            argv: vec!["app1".to_string()],
            icon: String::new(),
            comment: String::new(),
            terminal: false,
        };
        let app2 = DesktopApp {
            id: "app2.desktop".to_string(),
            name: "App Two".to_string(),
            argv: vec!["app2".to_string()],
            icon: String::new(),
            comment: String::new(),
            terminal: false,
        };

        // Apply sequence 2
        let applied2 = state.update(AppIndexUpdate {
            seq: 2,
            apps: Arc::new(vec![app2.clone()]),
        });
        assert!(applied2);
        assert_eq!(state.apps.len(), 1);
        assert_eq!(state.apps[0].name, "App Two");

        // Stale sequence 1 should be ignored
        let applied1 = state.update(AppIndexUpdate {
            seq: 1,
            apps: Arc::new(vec![app1.clone()]),
        });
        assert!(!applied1);
        assert_eq!(state.apps.len(), 1);
        assert_eq!(state.apps[0].name, "App Two");

        // Fresh sequence 3 is applied
        let applied3 = state.update(AppIndexUpdate {
            seq: 3,
            apps: Arc::new(vec![app1.clone(), app2.clone()]),
        });
        assert!(applied3);
        assert_eq!(state.apps.len(), 2);
    }
}
