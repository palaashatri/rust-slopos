use slopos_kit::button::Button;
use slopos_kit::event::{KeyCode, Modifiers, MouseButton};
use slopos_kit::icon_view::{IconItem, IconView};
use slopos_kit::layout::Layout;
use slopos_kit::status_bar::{StatusBar, StatusBarAlignment};
use slopos_kit::toolbar::Toolbar;
use slopos_kit::tree_view::{TreeNode, TreeView};
use slopos_kit::window::Window;
use slopos_kit::{
    AccessibilityNode, AccessibilityRole, Event, EventResult, FocusManager, LayoutConstraint,
    PointerDispatcher, Rect, Size, ThemeContext, Visibility, Widget, WidgetState,
};
use slopos_sdk::{build_menu, Application};
use std::path::PathBuf;

mod file_ops;

fn main() {
    let _ = tracing_subscriber::fmt::try_init();

    let mut app = Application::new("Finder", "com.slopos.finder");

    let mut file_menu = build_menu("File");
    file_menu.add_action("New Folder").with_shortcut(
        KeyCode::N,
        Modifiers {
            shift: true,
            control: false,
            alt: false,
            meta: true,
        },
    );
    file_menu.add_separator();
    file_menu.add_action("Get Info").with_shortcut(
        KeyCode::I,
        Modifiers {
            shift: false,
            control: false,
            alt: false,
            meta: true,
        },
    );
    file_menu.add_separator();
    file_menu.add_action("Move to Trash").with_shortcut(
        KeyCode::Backspace,
        Modifiers {
            shift: false,
            control: false,
            alt: false,
            meta: true,
        },
    );
    let mut view_menu = build_menu("View");
    view_menu.add_action("Show Status Bar");
    view_menu.add_action("Show Sidebar");

    let mut go_menu = build_menu("Go");
    go_menu.add_action("Back");
    go_menu.add_action("Forward");
    go_menu.add_separator();
    go_menu.add_action("Enclosing Folder");
    go_menu.add_action("Home");

    let mut window_menu = build_menu("Window");
    window_menu.add_action("Minimize");
    window_menu.add_action("Zoom");

    app.set_menus(vec![file_menu, view_menu, go_menu, window_menu]);

    // The global menu is rendered by slopos-shell, but application commands
    // stay in Finder. The SDK delivers the namespaced action over the
    // session-private application endpoint; this closure is the application
    // boundary, not a second shell window model.
    app.on_menu_action(|action, window| {
        let Some(content) = window.content.as_mut() else {
            return;
        };
        let Some(view) = content.as_any_mut().downcast_mut::<FinderView>() else {
            return;
        };
        let action = action.strip_prefix("com.slopos.finder.").unwrap_or(action);
        match action {
            "file.new_folder" => {
                view.create_new_folder();
            }
            "file.get_info" => {
                view.show_selected_info();
            }
            "file.move_to_trash" => {
                view.move_selected_to_trash();
            }
            "go.back" => {
                view.go_back();
            }
            "go.forward" => {
                view.go_forward();
            }
            "go.enclosing_folder" => {
                view.go_to_parent();
            }
            "go.home" => {
                if let Some(home) = std::env::var_os("HOME") {
                    view.navigate_to_path(PathBuf::from(home));
                }
            }
            "go.desktop" | "go.documents" | "go.downloads" => {
                if let Some(home) = std::env::var_os("HOME") {
                    let folder = match action {
                        "go.desktop" => "Desktop",
                        "go.documents" => "Documents",
                        _ => "Downloads",
                    };
                    view.navigate_to_path(PathBuf::from(home).join(folder));
                }
            }
            "view.show_sidebar" => view.toggle_sidebar(),
            "view.show_status_bar" => view.toggle_status_bar(),
            _ => {}
        }
    });

    let finderview = FinderView::new();
    let mut window = Window::new("Finder");
    window.layout = Layout::vertical(0.0);
    window.set_content(Box::new(finderview));
    app.set_main_window(window);
    app.run();
}

pub struct FinderView {
    state: WidgetState,
    current_path: PathBuf,
    toolbar: Toolbar,
    sidebar: TreeView,
    file_grid: IconView,
    status_bar: StatusBar,
    sidebar_visible: bool,
    status_bar_visible: bool,
    last_selected_path: Option<Vec<usize>>,
    back_stack: Vec<PathBuf>,
    forward_stack: Vec<PathBuf>,
    info_text: Option<String>,
    drag_source_path: Option<PathBuf>,
    focus: FocusManager,
    pointer: PointerDispatcher,
}

impl Default for FinderView {
    fn default() -> Self {
        Self::new()
    }
}

impl FinderView {
    pub fn new() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let current_path = PathBuf::from(home);

        let mut sidebar = TreeView::new();
        let mut favorites = TreeNode::new("Favorites");
        favorites.children.push(TreeNode::new("SLOPOS Share"));
        favorites.children.push(TreeNode::new("Recents"));
        favorites.children.push(TreeNode::new("Applications"));
        favorites.children.push(TreeNode::new("Desktop"));
        favorites.children.push(TreeNode::new("Documents"));
        favorites.children.push(TreeNode::new("Downloads"));
        favorites.expanded = true;

        let mut locations = TreeNode::new("Locations");
        locations.children.push(TreeNode::new("SLOPOS-I"));
        locations.children.push(TreeNode::new("Network"));
        locations.expanded = true;

        sidebar.roots = vec![favorites, locations];

        let mut file_grid = IconView::new();
        file_grid.icon_size = 64.0;
        file_grid.spacing = 12.0;

        let mut toolbar = Toolbar::new();
        toolbar.add(Box::new(Button::new("BACK")));
        toolbar.add(Box::new(Button::new("FWD")));
        toolbar.add(Box::new(Button::new("UP")));
        toolbar.add(Box::new(Button::new("NEW FOLDER")));
        toolbar.add(Box::new(Button::new("DUP")));
        toolbar.add(Box::new(Button::new("TRASH")));
        toolbar.add(Box::new(Button::new("INFO")));

        let mut view = FinderView {
            state: WidgetState::new(),
            current_path,
            toolbar,
            sidebar,
            file_grid,
            status_bar: StatusBar::new(),
            sidebar_visible: true,
            status_bar_visible: true,
            last_selected_path: None,
            back_stack: Vec::new(),
            forward_stack: Vec::new(),
            info_text: None,
            drag_source_path: None,
            focus: FocusManager::new(),
            pointer: PointerDispatcher::new(),
        };
        view.reload_directory();
        view
    }

    pub fn reload_directory(&mut self) {
        self.info_text = None;
        self.file_grid.items.clear();
        if let Ok(mut entries) = file_ops::list_directory(&self.current_path) {
            entries.sort_by(|left, right| {
                right
                    .is_dir
                    .cmp(&left.is_dir)
                    .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            });

            for entry in entries {
                self.file_grid.items.push(IconItem {
                    label: entry.name,
                    icon: Some(if entry.is_dir { "folder" } else { "document" }.to_string()),
                    selected: false,
                    rect: Rect::ZERO,
                });
            }
        }
        self.refresh_status_bar();
    }

    fn toggle_sidebar(&mut self) {
        self.sidebar_visible = !self.sidebar_visible;
        self.sidebar.set_visibility(if self.sidebar_visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
        let _ = self.layout(LayoutConstraint::tight(Size::new(
            self.rect().width,
            self.rect().height,
        )));
    }

    fn toggle_status_bar(&mut self) {
        self.status_bar_visible = !self.status_bar_visible;
        self.status_bar.set_visibility(if self.status_bar_visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
        let _ = self.layout(LayoutConstraint::tight(Size::new(
            self.rect().width,
            self.rect().height,
        )));
    }

    fn refresh_status_bar(&mut self) {
        self.status_bar.items.clear();
        let selected = self
            .file_grid
            .items
            .iter()
            .filter(|item| item.selected)
            .count();
        let item_count = self.file_grid.items.len();
        let count_text = if selected > 0 {
            format!("{selected} of {item_count} selected")
        } else {
            format!("{item_count} items")
        };
        let summary = self.info_text.as_deref().unwrap_or(&count_text).to_string();
        self.status_bar
            .add_item(&summary, StatusBarAlignment::Left, 360.0);
        self.status_bar.add_item(
            &self.current_path.display().to_string(),
            StatusBarAlignment::Left,
            520.0,
        );
    }

    fn selected_item(&self) -> Option<IconItem> {
        self.file_grid
            .items
            .iter()
            .find(|item| item.selected)
            .cloned()
    }

    fn selected_path(&self) -> Option<PathBuf> {
        self.selected_item()
            .map(|item| self.current_path.join(item.label))
    }

    fn item_at_point(&self, point: slopos_kit::Point) -> Option<IconItem> {
        self.file_grid
            .items
            .iter()
            .find(|item| item.rect.contains(point))
            .cloned()
    }

    fn start_drag_at(&mut self, point: slopos_kit::Point) -> bool {
        let Some(item) = self.item_at_point(point) else {
            self.drag_source_path = None;
            return false;
        };

        let path = self.current_path.join(item.label);
        self.drag_source_path = Some(path.clone());
        self.info_text = Some(format!(
            "DRAGGING - {}",
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string())
        ));
        self.refresh_status_bar();
        true
    }

    fn finish_drag_at(&mut self, point: slopos_kit::Point) -> bool {
        let Some(source) = self.drag_source_path.take() else {
            return false;
        };

        let Some(target_item) = self.item_at_point(point) else {
            self.info_text = Some("MOVE CANCELLED".to_string());
            self.refresh_status_bar();
            return false;
        };

        if target_item.icon.as_deref() != Some("folder") {
            self.info_text = Some("MOVE CANCELLED - DROP ON A FOLDER".to_string());
            self.refresh_status_bar();
            return false;
        }

        let target_dir = self.current_path.join(&target_item.label);
        if source == target_dir || source.starts_with(&target_dir) {
            self.info_text = Some("MOVE CANCELLED - INVALID TARGET".to_string());
            self.refresh_status_bar();
            return false;
        }

        let Some(file_name) = source.file_name() else {
            self.info_text = Some("MOVE FAILED - INVALID SOURCE".to_string());
            self.refresh_status_bar();
            return false;
        };
        let destination = target_dir.join(file_name);
        if destination.exists() {
            self.info_text = Some("MOVE CANCELLED - NAME ALREADY EXISTS".to_string());
            self.refresh_status_bar();
            return false;
        }

        let moved_name = file_name.to_string_lossy().into_owned();
        match file_ops::move_file(&source, &destination) {
            Ok(()) => {
                self.reload_directory();
                self.info_text = Some(format!("MOVED - {moved_name} TO {}", target_item.label));
                self.refresh_status_bar();
                true
            }
            Err(err) => {
                self.info_text = Some(format!("MOVE FAILED - {err}"));
                self.refresh_status_bar();
                false
            }
        }
    }

    fn set_current_path(&mut self, path: PathBuf) {
        if path.is_dir() {
            self.current_path = path;
            self.reload_directory();
        }
    }

    fn navigate_to_path(&mut self, path: PathBuf) -> bool {
        if !path.is_dir() || path == self.current_path {
            return false;
        }

        self.back_stack.push(self.current_path.clone());
        self.forward_stack.clear();
        self.set_current_path(path);
        true
    }

    fn enter_folder_named(&mut self, folder: &str) -> bool {
        let path = self.current_path.join(folder);
        if path.is_dir() {
            self.navigate_to_path(path)
        } else {
            false
        }
    }

    fn go_to_parent(&mut self) -> bool {
        let Some(parent) = self.current_path.parent().map(PathBuf::from) else {
            return false;
        };
        self.navigate_to_path(parent)
    }

    fn go_back(&mut self) -> bool {
        let Some(previous) = self.back_stack.pop() else {
            return false;
        };
        self.forward_stack.push(self.current_path.clone());
        self.set_current_path(previous);
        true
    }

    fn go_forward(&mut self) -> bool {
        let Some(next) = self.forward_stack.pop() else {
            return false;
        };
        self.back_stack.push(self.current_path.clone());
        self.set_current_path(next);
        true
    }

    /// Index of the toolbar button activated since last asked (via real
    /// press/release or keyboard activation), drained through the widgets'
    /// own `take_clicked()` rather than any geometry math here.
    fn take_toolbar_click(&mut self) -> Option<usize> {
        self.toolbar.items.iter_mut().position(|item| {
            item.as_any_mut()
                .downcast_mut::<Button>()
                .is_some_and(|button| button.take_clicked())
        })
    }

    fn run_toolbar_action(&mut self, index: usize) {
        match index {
            0 => {
                self.go_back();
            }
            1 => {
                self.go_forward();
            }
            2 => {
                self.go_to_parent();
            }
            3 => {
                self.create_new_folder();
            }
            4 => {
                self.duplicate_selected();
            }
            5 => {
                self.move_selected_to_trash();
            }
            6 => {
                self.show_selected_info();
            }
            _ => {}
        }
    }

    /// Drain widget activations after an event went through generic
    /// dispatch: toolbar buttons record clicks, the icon grid records
    /// double-click activation.
    fn process_activations(&mut self) {
        if let Some(index) = self.take_toolbar_click() {
            self.run_toolbar_action(index);
            return;
        }
        if let Some(index) = self.file_grid.take_activated() {
            let folder = self
                .file_grid
                .items
                .get(index)
                .filter(|item| item.icon.as_deref() == Some("folder"))
                .map(|item| item.label.clone());
            if let Some(folder) = folder {
                self.enter_folder_named(&folder);
            }
        }
    }

    /// App-level keyboard accelerators; run only after the focused widget
    /// declined the key.
    fn handle_app_key(&mut self, event: &Event) -> EventResult {
        let Event::KeyDown { key, modifiers } = event else {
            return EventResult::Ignored;
        };
        if modifiers.meta {
            match key {
                KeyCode::ArrowUp => {
                    if self.go_to_parent() {
                        return EventResult::Handled;
                    }
                }
                KeyCode::LeftBracket => {
                    if self.go_back() {
                        return EventResult::Handled;
                    }
                }
                KeyCode::RightBracket => {
                    if self.go_forward() {
                        return EventResult::Handled;
                    }
                }
                KeyCode::N if modifiers.shift => {
                    self.create_new_folder();
                    return EventResult::Handled;
                }
                KeyCode::Backspace => {
                    self.move_selected_to_trash();
                    return EventResult::Handled;
                }
                KeyCode::D => {
                    self.duplicate_selected();
                    return EventResult::Handled;
                }
                KeyCode::I => {
                    self.show_selected_info();
                    return EventResult::Handled;
                }
                _ => {}
            }
        } else if *key == KeyCode::Enter {
            if let Some(item) = self.selected_item() {
                if item.icon == Some("folder".to_string()) && self.enter_folder_named(&item.label) {
                    return EventResult::Handled;
                }
            }
        }
        EventResult::Ignored
    }

    fn create_new_folder(&mut self) -> bool {
        let mut candidate = self.current_path.join("New Folder");
        for index in 2.. {
            if !candidate.exists() {
                break;
            }
            candidate = self.current_path.join(format!("New Folder {index}"));
        }
        if file_ops::create_directory(&candidate).is_ok() {
            self.reload_directory();
            true
        } else {
            false
        }
    }

    fn duplicate_selected(&mut self) -> bool {
        let Some(path) = self.selected_path() else {
            return false;
        };
        if file_ops::duplicate_file(&path).is_ok() {
            self.reload_directory();
            true
        } else {
            false
        }
    }

    fn move_selected_to_trash(&mut self) -> bool {
        let Some(path) = self.selected_path() else {
            return false;
        };
        if file_ops::delete_file(&path).is_ok() {
            self.reload_directory();
            true
        } else {
            false
        }
    }

    fn show_selected_info(&mut self) -> bool {
        let Some(path) = self.selected_path() else {
            self.info_text = Some("INFO - NO SELECTION".to_string());
            self.refresh_status_bar();
            return false;
        };

        match file_ops::get_file_info(&path) {
            Ok(info) => {
                let kind = if info.is_dir { "FOLDER" } else { "FILE" };
                let size = if info.is_dir {
                    "FOLDER".to_string()
                } else {
                    format!("{} BYTES", info.size)
                };
                self.info_text = Some(format!("INFO - {kind} - {} - {size}", info.name));
                self.refresh_status_bar();
                true
            }
            Err(err) => {
                self.info_text = Some(format!("INFO FAILED - {err}"));
                self.refresh_status_bar();
                false
            }
        }
    }

    fn sync_sidebar_selection(&mut self) {
        let sidebar_selected = self.sidebar.selected_path.clone();
        if sidebar_selected == self.last_selected_path {
            return;
        }

        self.last_selected_path = sidebar_selected.clone();
        let Some(selected) = sidebar_selected else {
            return;
        };

        let home = PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string()));
        let path = match selected.as_slice() {
            [0, 3] => home.join("Desktop"),
            [0, 4] => home.join("Documents"),
            [0, 5] => home.join("Downloads"),
            [1, 0] => PathBuf::from("/"),
            _ => home,
        };
        self.navigate_to_path(path);
    }
}

impl Widget for FinderView {
    fn widget_state(&self) -> &WidgetState {
        &self.state
    }

    fn widget_state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        let size = constraint.clamp(Size::new(constraint.max_width, constraint.max_height));
        let r = Rect::new(self.rect().x, self.rect().y, size.width, size.height);
        self.set_rect(r);

        let toolbar_h = 32.0;
        let status_h = if self.status_bar_visible { 24.0 } else { 0.0 };
        let content_y = r.y + toolbar_h;
        let content_h = (r.height - toolbar_h - status_h).max(0.0);
        let sidebar_w = if self.sidebar_visible {
            (r.width * 0.25).clamp(150.0, 220.0).min(r.width)
        } else {
            0.0
        };
        let grid_w = (r.width - sidebar_w).max(0.0);

        self.toolbar
            .set_rect(Rect::new(r.x, r.y, r.width, toolbar_h));
        let _ = self
            .toolbar
            .layout(LayoutConstraint::tight(Size::new(r.width, toolbar_h)));

        self.sidebar
            .set_rect(Rect::new(r.x, content_y, sidebar_w, content_h));
        let _ = self
            .sidebar
            .layout(LayoutConstraint::tight(Size::new(sidebar_w, content_h)));

        self.file_grid
            .set_rect(Rect::new(r.x + sidebar_w, content_y, grid_w, content_h));
        let _ = self
            .file_grid
            .layout(LayoutConstraint::tight(Size::new(grid_w, content_h)));

        self.status_bar.set_rect(Rect::new(
            r.x,
            r.y + toolbar_h + content_h,
            r.width,
            status_h,
        ));
        let _ = self
            .status_bar
            .layout(LayoutConstraint::tight(Size::new(r.width, status_h)));

        size
    }

    fn draw(&self, theme: &ThemeContext) {
        self.toolbar.draw(theme);
        self.sidebar.draw(theme);
        self.file_grid.draw(theme);
        self.status_bar.draw(theme);
    }

    fn handle_event(&mut self, event: &Event) -> EventResult {
        match event {
            // Tab / Shift+Tab walk the focusable widgets (toolbar buttons).
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
            // Focused widget first (Enter/Space activate a focused toolbar
            // button); app accelerators only if the key was declined.
            Event::KeyDown { .. } | Event::KeyUp { .. } | Event::Char { .. } => {
                let mut focus = std::mem::take(&mut self.focus);
                let result = focus.dispatch_key(self, event);
                self.focus = focus;
                if matches!(result, EventResult::Handled) {
                    self.process_activations();
                    return EventResult::Handled;
                }
                self.handle_app_key(event)
            }
            // The icon-grid drag protocol stays app-level for now: nothing
            // in the SDK loop synthesizes Drag* events yet, and the drop
            // target is an *item*, not a widget.
            Event::DragStart { point } => {
                if self.start_drag_at(*point) {
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
            Event::DragEnd { point } | Event::Drop { point } => {
                if self.finish_drag_at(*point) {
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
            // Mouse history buttons are chords on the whole window, not
            // positional clicks.
            Event::MouseDown {
                button: MouseButton::Back,
                ..
            } => {
                if self.go_back() {
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
            Event::MouseDown {
                button: MouseButton::Forward,
                ..
            } => {
                if self.go_forward() {
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
            // Everything positional goes through generic rect-checked
            // dispatch with implicit capture; no hand-rolled hit-testing.
            Event::MouseDown { .. }
            | Event::MouseUp { .. }
            | Event::MouseMove { .. }
            | Event::DoubleClick { .. }
            | Event::MouseLeave => {
                let previous_selection = self.selected_path();
                let mut pointer = std::mem::take(&mut self.pointer);
                let result = pointer.dispatch(self, event);
                self.pointer = pointer;
                if matches!(result, EventResult::Handled) {
                    self.process_activations();
                    if self.selected_path() != previous_selection {
                        self.info_text = None;
                        self.refresh_status_bar();
                    }
                }
                result
            }
            _ => EventResult::Ignored,
        }
    }

    fn update(&mut self) {
        self.toolbar.update();
        self.sidebar.update();
        self.file_grid.update();
        self.sync_sidebar_selection();
    }

    fn accessibility(&self) -> Option<AccessibilityNode> {
        Some(AccessibilityNode::new(AccessibilityRole::Window, "Finder"))
    }

    fn children(&self) -> Vec<&dyn Widget> {
        vec![
            &self.toolbar,
            &self.sidebar,
            &self.file_grid,
            &self.status_bar,
        ]
    }

    fn children_mut(&mut self) -> Vec<&mut dyn Widget> {
        vec![
            &mut self.toolbar,
            &mut self.sidebar,
            &mut self.file_grid,
            &mut self.status_bar,
        ]
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn temp_finder_root() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("slopos-i_finder_view_{unique}_{sequence}"));
        let _ = fs::remove_dir_all(&root);
        root
    }

    fn rect_center(rect: Rect) -> slopos_kit::Point {
        slopos_kit::Point::new(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5)
    }

    #[test]
    fn reload_directory_sorts_folders_first_and_updates_status() {
        let root = temp_finder_root();
        fs::create_dir_all(root.join("Folder")).unwrap();
        fs::write(root.join("note.txt"), "hello").unwrap();

        let mut view = FinderView::new();
        view.set_current_path(root.clone());

        assert_eq!(view.file_grid.items[0].label, "Folder");
        assert_eq!(view.status_bar.items[0].text, "2 items");
        assert_eq!(view.status_bar.items[1].text, root.display().to_string());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn finder_enters_folder_and_returns_to_parent() {
        let root = temp_finder_root();
        let child = root.join("Child");
        fs::create_dir_all(&child).unwrap();

        let mut view = FinderView::new();
        view.set_current_path(root.clone());

        assert!(view.enter_folder_named("Child"));
        assert_eq!(view.current_path, child);
        assert!(view.go_to_parent());
        assert_eq!(view.current_path, root);

        fs::remove_dir_all(view.current_path).unwrap();
    }

    #[test]
    fn finder_navigation_history_tracks_back_and_forward() {
        let root = temp_finder_root();
        let child = root.join("Child");
        let grandchild = child.join("Grandchild");
        fs::create_dir_all(&grandchild).unwrap();

        let mut view = FinderView::new();
        view.set_current_path(root.clone());

        assert!(view.enter_folder_named("Child"));
        assert_eq!(view.current_path, child);
        assert_eq!(view.back_stack, vec![root.clone()]);
        assert!(view.enter_folder_named("Grandchild"));
        assert_eq!(view.current_path, grandchild);
        assert_eq!(view.back_stack, vec![root.clone(), child.clone()]);

        assert!(view.go_back());
        assert_eq!(view.current_path, child);
        assert_eq!(view.forward_stack, vec![grandchild.clone()]);
        assert!(view.go_forward());
        assert_eq!(view.current_path, grandchild);
        assert!(view.forward_stack.is_empty());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn finder_keyboard_shortcuts_drive_navigation_history() {
        let root = temp_finder_root();
        let child = root.join("Child");
        fs::create_dir_all(&child).unwrap();

        let mut view = FinderView::new();
        view.set_current_path(root.clone());
        assert!(view.enter_folder_named("Child"));

        let handled = view.handle_event(&Event::KeyDown {
            key: KeyCode::LeftBracket,
            modifiers: Modifiers {
                meta: true,
                ..Modifiers::NONE
            },
        });
        assert!(matches!(handled, EventResult::Handled));
        assert_eq!(view.current_path, root);

        let handled = view.handle_event(&Event::KeyDown {
            key: KeyCode::RightBracket,
            modifiers: Modifiers {
                meta: true,
                ..Modifiers::NONE
            },
        });
        assert!(matches!(handled, EventResult::Handled));
        assert_eq!(view.current_path, child);

        fs::remove_dir_all(root).unwrap();
    }

    fn click_toolbar_button(view: &mut FinderView, index: usize) -> EventResult {
        let rect = view.toolbar.items[index].rect();
        let point = slopos_kit::Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
        let down = view.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point,
            modifiers: Modifiers::NONE,
        });
        assert!(
            matches!(down, EventResult::Handled),
            "press must land on the button"
        );
        view.handle_event(&Event::MouseUp {
            button: MouseButton::Left,
            point,
            modifiers: Modifiers::NONE,
        })
    }

    #[test]
    fn finder_toolbar_buttons_drive_navigation_history() {
        let root = temp_finder_root();
        let child = root.join("Child");
        fs::create_dir_all(&child).unwrap();

        let mut view = FinderView::new();
        view.set_current_path(root.clone());
        view.layout(LayoutConstraint::tight(Size::new(960.0, 640.0)));
        assert!(view.enter_folder_named("Child"));

        let handled = click_toolbar_button(&mut view, 0);
        assert!(matches!(handled, EventResult::Handled));
        assert_eq!(view.current_path, root);

        let handled = click_toolbar_button(&mut view, 1);
        assert!(matches!(handled, EventResult::Handled));
        assert_eq!(view.current_path, child);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn finder_toolbar_new_folder_creates_directory() {
        let root = temp_finder_root();
        fs::create_dir_all(&root).unwrap();

        let mut view = FinderView::new();
        view.set_current_path(root.clone());
        view.layout(LayoutConstraint::tight(Size::new(960.0, 640.0)));

        let handled = click_toolbar_button(&mut view, 3);

        assert!(matches!(handled, EventResult::Handled));
        assert!(root.join("New Folder").is_dir());
        assert!(view
            .file_grid
            .items
            .iter()
            .any(|item| item.label == "New Folder"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn finder_toolbar_duplicate_copies_selected_file() {
        let root = temp_finder_root();
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("note.txt"), "hello").unwrap();

        let mut view = FinderView::new();
        view.set_current_path(root.clone());
        view.layout(LayoutConstraint::tight(Size::new(960.0, 640.0)));
        let note = view
            .file_grid
            .items
            .iter_mut()
            .find(|item| item.label == "note.txt")
            .expect("note is listed");
        note.selected = true;

        let handled = click_toolbar_button(&mut view, 4);

        assert!(matches!(handled, EventResult::Handled));
        assert_eq!(
            fs::read_to_string(root.join("note copy.txt")).unwrap(),
            "hello"
        );
        assert!(view
            .file_grid
            .items
            .iter()
            .any(|item| item.label == "note copy.txt"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn finder_get_info_reports_selected_file_metadata() {
        let root = temp_finder_root();
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("note.txt"), "hello").unwrap();

        let mut view = FinderView::new();
        view.set_current_path(root.clone());
        view.layout(LayoutConstraint::tight(Size::new(960.0, 640.0)));
        let note = view
            .file_grid
            .items
            .iter_mut()
            .find(|item| item.label == "note.txt")
            .expect("note is listed");
        note.selected = true;

        let handled = view.handle_event(&Event::KeyDown {
            key: KeyCode::I,
            modifiers: Modifiers {
                meta: true,
                ..Modifiers::NONE
            },
        });

        assert!(matches!(handled, EventResult::Handled));
        assert_eq!(
            view.status_bar.items[0].text,
            "INFO - FILE - note.txt - 5 BYTES"
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn finder_toolbar_info_uses_same_metadata_status() {
        let root = temp_finder_root();
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(root.join("Folder")).unwrap();

        let mut view = FinderView::new();
        view.set_current_path(root.clone());
        view.layout(LayoutConstraint::tight(Size::new(960.0, 640.0)));
        let folder = view
            .file_grid
            .items
            .iter_mut()
            .find(|item| item.label == "Folder")
            .expect("folder is listed");
        folder.selected = true;

        let handled = click_toolbar_button(&mut view, 6);

        assert!(matches!(handled, EventResult::Handled));
        assert_eq!(
            view.status_bar.items[0].text,
            "INFO - FOLDER - Folder - FOLDER"
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn finder_dragging_file_onto_folder_moves_it() {
        let root = temp_finder_root();
        fs::create_dir_all(root.join("Folder")).unwrap();
        fs::write(root.join("note.txt"), "hello").unwrap();

        let mut view = FinderView::new();
        view.set_current_path(root.clone());
        view.layout(LayoutConstraint::tight(Size::new(960.0, 640.0)));

        let note_rect = view
            .file_grid
            .items
            .iter()
            .find(|item| item.label == "note.txt")
            .expect("note is listed")
            .rect;
        let folder_rect = view
            .file_grid
            .items
            .iter()
            .find(|item| item.label == "Folder")
            .expect("folder is listed")
            .rect;

        assert!(matches!(
            view.handle_event(&Event::DragStart {
                point: rect_center(note_rect)
            }),
            EventResult::Handled
        ));
        assert!(matches!(
            view.handle_event(&Event::DragEnd {
                point: rect_center(folder_rect)
            }),
            EventResult::Handled
        ));

        assert!(!root.join("note.txt").exists());
        assert_eq!(
            fs::read_to_string(root.join("Folder").join("note.txt")).unwrap(),
            "hello"
        );
        assert_eq!(view.status_bar.items[0].text, "MOVED - note.txt TO Folder");
        assert!(!view
            .file_grid
            .items
            .iter()
            .any(|item| item.label == "note.txt"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn finder_dragging_to_non_folder_cancels_move() {
        let root = temp_finder_root();
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("note.txt"), "hello").unwrap();
        fs::write(root.join("target.txt"), "target").unwrap();

        let mut view = FinderView::new();
        view.set_current_path(root.clone());
        view.layout(LayoutConstraint::tight(Size::new(960.0, 640.0)));

        let note_rect = view
            .file_grid
            .items
            .iter()
            .find(|item| item.label == "note.txt")
            .expect("note is listed")
            .rect;
        let target_rect = view
            .file_grid
            .items
            .iter()
            .find(|item| item.label == "target.txt")
            .expect("target is listed")
            .rect;

        assert!(matches!(
            view.handle_event(&Event::DragStart {
                point: rect_center(note_rect)
            }),
            EventResult::Handled
        ));
        assert!(matches!(
            view.handle_event(&Event::DragEnd {
                point: rect_center(target_rect)
            }),
            EventResult::Ignored
        ));

        assert_eq!(fs::read_to_string(root.join("note.txt")).unwrap(), "hello");
        assert_eq!(
            view.status_bar.items[0].text,
            "MOVE CANCELLED - DROP ON A FOLDER"
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn finder_click_selects_item_through_generic_dispatch() {
        let root = temp_finder_root();
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("note.txt"), "hello").unwrap();

        let mut view = FinderView::new();
        view.set_current_path(root.clone());
        view.layout(LayoutConstraint::tight(Size::new(960.0, 640.0)));

        let note_rect = view
            .file_grid
            .items
            .iter()
            .find(|item| item.label == "note.txt")
            .expect("note is listed")
            .rect;
        let handled = view.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point: rect_center(note_rect),
            modifiers: Modifiers::NONE,
        });

        assert!(matches!(handled, EventResult::Handled));
        assert!(view
            .selected_item()
            .is_some_and(|item| item.label == "note.txt"));
        assert_eq!(view.status_bar.items[0].text, "1 of 1 selected");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn finder_double_click_enters_folder_through_dispatch() {
        let root = temp_finder_root();
        fs::create_dir_all(root.join("Child")).unwrap();

        let mut view = FinderView::new();
        view.set_current_path(root.clone());
        view.layout(LayoutConstraint::tight(Size::new(960.0, 640.0)));

        let folder_rect = view
            .file_grid
            .items
            .iter()
            .find(|item| item.label == "Child")
            .expect("folder is listed")
            .rect;
        let handled = view.handle_event(&Event::DoubleClick {
            button: MouseButton::Left,
            point: rect_center(folder_rect),
            modifiers: Modifiers::NONE,
        });

        assert!(matches!(handled, EventResult::Handled));
        assert_eq!(view.current_path, root.join("Child"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn finder_tab_then_enter_activates_focused_toolbar_button() {
        let root = temp_finder_root();
        fs::create_dir_all(&root).unwrap();

        let mut view = FinderView::new();
        view.set_current_path(root.clone());
        view.layout(LayoutConstraint::tight(Size::new(960.0, 640.0)));

        // Tab x4 lands on the fourth toolbar button: NEW FOLDER.
        for _ in 0..4 {
            assert!(matches!(
                view.handle_event(&Event::KeyDown {
                    key: KeyCode::Tab,
                    modifiers: Modifiers::NONE,
                }),
                EventResult::Handled
            ));
        }
        assert!(view.toolbar.items[3].widget_state().focused);

        let handled = view.handle_event(&Event::KeyDown {
            key: KeyCode::Enter,
            modifiers: Modifiers::NONE,
        });
        assert!(matches!(handled, EventResult::Handled));
        assert!(root.join("New Folder").is_dir());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn finder_status_bar_sits_below_content() {
        let mut view = FinderView::new();

        view.layout(LayoutConstraint::tight(Size::new(960.0, 640.0)));

        assert_eq!(view.status_bar.rect().y, 616.0);
    }

    #[test]
    fn finder_view_menu_toggles_sidebar_and_status_bar_geometry() {
        let mut view = FinderView::new();
        view.layout(LayoutConstraint::tight(Size::new(960.0, 640.0)));
        assert!(view.sidebar_visible);
        assert_eq!(view.file_grid.rect().x, 220.0);

        view.toggle_sidebar();
        assert!(!view.sidebar_visible);
        assert_eq!(view.sidebar.visibility(), Visibility::Hidden);
        assert_eq!(view.file_grid.rect().x, 0.0);

        view.toggle_status_bar();
        assert!(!view.status_bar_visible);
        assert_eq!(view.status_bar.visibility(), Visibility::Hidden);
        assert_eq!(view.file_grid.rect().height, 608.0);
    }
}
