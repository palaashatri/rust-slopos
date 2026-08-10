use slopos_kit::button::Button;
use slopos_kit::clipboard::Clipboard;
use slopos_kit::event::{KeyCode, Modifiers};
use slopos_kit::label::Label;
use slopos_kit::text_field::TextField;
use slopos_kit::toolbar::Toolbar;
use slopos_kit::window::Window;
use slopos_kit::{
    widget_by_id, AccessibilityNode, AccessibilityRole, Event, EventResult, FocusManager,
    LayoutConstraint, PointerDispatcher, Rect, Size, ThemeContext, Visibility, Widget, WidgetState,
};
use slopos_sdk::{build_menu, Application};
use std::path::{Path, PathBuf};

mod save;

/// Returns the default file path: $TEXTEDIT_FILE env var or /tmp/slopos-i-textedit.txt.
fn default_file_path() -> PathBuf {
    std::env::var("TEXTEDIT_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp/slopos-i-textedit.txt"))
}

fn main() {
    let _ = tracing_subscriber::fmt::try_init();

    let mut app = Application::new("TextEdit", "com.slopos.textedit");

    let mut file_menu = build_menu("File");
    file_menu.add_action("New").with_shortcut(
        KeyCode::N,
        Modifiers {
            shift: false,
            control: false,
            alt: false,
            meta: true,
        },
    );
    file_menu.add_action("Open...").with_shortcut(
        KeyCode::O,
        Modifiers {
            shift: false,
            control: false,
            alt: false,
            meta: true,
        },
    );
    file_menu.add_separator();
    file_menu.add_action("Save").with_shortcut(
        KeyCode::S,
        Modifiers {
            shift: false,
            control: false,
            alt: false,
            meta: true,
        },
    );
    file_menu.add_action("Save As...").with_shortcut(
        KeyCode::S,
        Modifiers {
            shift: true,
            control: false,
            alt: false,
            meta: true,
        },
    );

    let mut edit_menu = build_menu("Edit");
    edit_menu.add_action("Undo").with_shortcut(
        KeyCode::Z,
        Modifiers {
            shift: false,
            control: false,
            alt: false,
            meta: true,
        },
    );
    edit_menu.add_action("Redo").with_shortcut(
        KeyCode::Z,
        Modifiers {
            shift: true,
            control: false,
            alt: false,
            meta: true,
        },
    );
    edit_menu.add_separator();
    edit_menu.add_action("Cut").with_shortcut(
        KeyCode::X,
        Modifiers {
            shift: false,
            control: false,
            alt: false,
            meta: true,
        },
    );
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
    edit_menu.add_action("Select All").with_shortcut(
        KeyCode::A,
        Modifiers {
            shift: false,
            control: false,
            alt: false,
            meta: true,
        },
    );
    edit_menu.add_separator();
    edit_menu.add_action("Find").with_shortcut(
        KeyCode::F,
        Modifiers {
            shift: false,
            control: false,
            alt: false,
            meta: true,
        },
    );

    let mut window_menu = build_menu("Window");
    window_menu.add_action("Minimize");
    window_menu.add_action("Zoom");

    app.set_menus(vec![file_menu, edit_menu, window_menu]);

    // Keep document operations in TextEdit while the shell owns only the
    // global menu presentation and compositor-owned window operations.
    app.on_menu_action(|action, window| {
        let Some(content) = window.content.as_mut() else {
            return;
        };
        let Some(view) = content.as_any_mut().downcast_mut::<TextEditView>() else {
            return;
        };
        let action = action
            .strip_prefix("com.slopos.textedit.")
            .unwrap_or(action);
        match action {
            "file.new" => {
                view.new_document();
            }
            "file.open" => {
                view.open_document();
            }
            "file.save" => {
                view.save_document();
            }
            "file.save_as" => {
                view.save_as_from_path_field();
            }
            "edit.undo" => {
                view.undo();
            }
            "edit.redo" => {
                view.redo();
            }
            "edit.cut" => {
                view.cut_document();
            }
            "edit.copy" => {
                view.copy_document();
            }
            "edit.paste" => {
                view.paste_document();
            }
            "edit.select_all" => {
                view.select_all_document();
            }
            "edit.find" => {
                view.toggle_find();
            }
            _ => {}
        }
    });

    let document_path = std::env::args_os().nth(1).map(PathBuf::from);
    let view = TextEditView::open(document_path);
    let title = view.window_title();

    let mut window = Window::new(title);
    window.has_toolbar = true;
    window.set_content(Box::new(view));
    app.set_main_window(window);
    app.run();
}

struct TextEditView {
    state: WidgetState,
    toolbar: Toolbar,
    path_label: Label,
    path_field: TextField,
    find_label: Label,
    find_field: TextField,
    editor: TextField,
    status: Label,
    document_path: Option<PathBuf>,
    saved_text: String,
    dirty: bool,
    last_error: Option<String>,
    /// Transient notification that overrides the error/state display for one render cycle.
    notification: Option<String>,
    undo_stack: Vec<String>,
    redo_stack: Vec<String>,
    /// Whether the find bar row is currently visible.
    find_visible: bool,
    /// Last search string used for find-next.
    last_find_query: String,
    focus: FocusManager,
    pointer: PointerDispatcher,
}

impl TextEditView {
    fn open(document_path: Option<PathBuf>) -> Self {
        let (text, saved_text, error, recovered) = match document_path.as_deref() {
            Some(path) => match save::open_document(path) {
                Ok(document) => (document.text, document.saved_text, None, document.recovered),
                Err(err) => (
                    String::new(),
                    String::new(),
                    Some(format!("Could not open: {err}")),
                    false,
                ),
            },
            None => {
                let text = "Untitled Document\n\nWelcome to TextEdit. Start typing...".to_string();
                (text.clone(), text, None, false)
            }
        };

        let mut toolbar = Toolbar::new();
        toolbar.add(Box::new(Button::new("NEW")));
        toolbar.add(Box::new(Button::new("OPEN")));
        toolbar.add(Box::new(Button::new("SAVE")));
        toolbar.add(Box::new(Button::new("SAVE AS")));
        toolbar.add(Box::new(Button::new("UNDO")));
        toolbar.add(Box::new(Button::new("REDO")));
        toolbar.add(Box::new(Button::new("FIND")));
        toolbar.add(Box::new(Button::new("COPY")));
        toolbar.add(Box::new(Button::new("PASTE")));

        let mut path_field = TextField::new().with_placeholder("Document path");
        path_field.set_expands_horizontally(true);
        if let Some(path) = document_path.as_deref() {
            path_field.set_text(path.display().to_string());
        }

        let find_field = TextField::new().with_placeholder("Search…");

        let mut editor = TextField::new();
        editor.set_multiline(true);
        editor.set_text(text.clone());

        let mut view = Self {
            state: WidgetState::new(),
            toolbar,
            path_label: Label::new("PATH"),
            path_field,
            find_label: Label::new("FIND"),
            find_field,
            editor,
            status: Label::new(""),
            document_path,
            saved_text,
            dirty: recovered,
            last_error: error,
            notification: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            find_visible: false,
            last_find_query: String::new(),
            focus: FocusManager::new(),
            pointer: PointerDispatcher::new(),
        };
        view.apply_find_visibility();
        let editor_id = view.editor.id();
        view.focus_widget(editor_id);
        view.refresh_status();
        if recovered {
            if let Some(path) = view.document_path.as_deref() {
                view.notify(format!("Recovered unsaved changes from {}", path.display()));
            }
        }
        view
    }

    /// Focus `id` through the real focus system (sets `WidgetState.focused`
    /// on exactly that widget, clears it everywhere else in the tree).
    fn focus_widget(&mut self, id: slopos_kit::WidgetId) {
        let mut focus = std::mem::take(&mut self.focus);
        focus.focus(self, id);
        self.focus = focus;
    }

    /// Keep the find row's widget visibility in sync with `find_visible`, so
    /// hit-testing and the tab order skip it while it is closed.
    fn apply_find_visibility(&mut self) {
        let visibility = if self.find_visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        self.find_label.widget_state_mut().visibility = visibility;
        self.find_field.widget_state_mut().visibility = visibility;
    }

    fn window_title(&self) -> String {
        let name = self
            .document_path
            .as_deref()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string());
        if self.dirty {
            format!("{name} - Edited - TextEdit")
        } else {
            format!("{name} - TextEdit")
        }
    }

    // ----- Status bar helpers -----

    fn word_count(&self) -> usize {
        self.editor
            .text()
            .split_whitespace()
            .filter(|w| !w.is_empty())
            .count()
    }

    fn current_line(&self) -> usize {
        let cursor = self.editor.cursor_position();
        let text = self.editor.text();
        // Count newlines before the cursor (1-based line number).
        text[..cursor.min(text.len())]
            .chars()
            .filter(|&c| c == '\n')
            .count()
            + 1
    }

    fn refresh_status(&mut self) {
        if let Some(note) = self.notification.take() {
            self.status.text = note;
            return;
        }

        let path = self
            .document_path
            .as_deref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "No file".to_string());
        let state = if self.dirty { "Edited" } else { "Saved" };
        let words = self.word_count();
        let line = self.current_line();

        let error_part = self
            .last_error
            .as_deref()
            .map(|e| format!(" | {e}"))
            .unwrap_or_default();

        self.status.text = format!("{state} | {path} | Ln {line} | {words}w{error_part}");
    }

    /// Show a notification in the status bar; it will be displayed once then cleared.
    fn notify(&mut self, msg: impl Into<String>) {
        self.notification = Some(msg.into());
        self.refresh_status();
    }

    fn sync_path_field(&mut self) {
        if let Some(path) = self.document_path.as_deref() {
            self.path_field.set_text(path.display().to_string());
        } else {
            self.path_field.set_text("");
        }
    }

    fn mark_dirty_from_editor(&mut self) {
        self.dirty = self.editor.text() != self.saved_text;
        self.last_error = None;
        self.refresh_status();
    }

    fn push_undo_snapshot(&mut self) {
        let current = self.editor.text().to_string();
        if self.undo_stack.last() != Some(&current) {
            self.undo_stack.push(current);
            // Cap at 50 entries.
            if self.undo_stack.len() > 50 {
                self.undo_stack.remove(0);
            }
        }
        self.redo_stack.clear();
    }

    fn replace_editor_text(&mut self, text: String) {
        self.editor.set_text(text);
        self.mark_dirty_from_editor();
    }

    fn undo(&mut self) -> bool {
        let Some(previous) = self.undo_stack.pop() else {
            self.last_error = Some("Nothing to undo".to_string());
            self.refresh_status();
            return false;
        };
        self.redo_stack.push(self.editor.text().to_string());
        self.replace_editor_text(previous);
        true
    }

    fn redo(&mut self) -> bool {
        let Some(next) = self.redo_stack.pop() else {
            self.last_error = Some("Nothing to redo".to_string());
            self.refresh_status();
            return false;
        };
        self.undo_stack.push(self.editor.text().to_string());
        self.replace_editor_text(next);
        true
    }

    fn copy_document(&mut self) -> bool {
        // Preserve the useful whole-document fallback when there is no active
        // selection, while making Cmd-C operate on the selected UTF-8 range
        // when one exists.
        let text = self
            .editor
            .selected_text()
            .unwrap_or_else(|| self.editor.text());
        Clipboard::copy(text);
        self.last_error = None;
        self.refresh_status();
        true
    }

    fn cut_document(&mut self) -> bool {
        if let Some(selected) = self.editor.selected_text().map(str::to_owned) {
            self.push_undo_snapshot();
            Clipboard::copy(&selected);
            self.editor.replace_selection("");
            self.mark_dirty_from_editor();
            return true;
        }

        if self.editor.text().is_empty() {
            return self.copy_document();
        }
        self.push_undo_snapshot();
        Clipboard::copy(self.editor.text());
        self.editor.set_text("");
        self.mark_dirty_from_editor();
        true
    }

    fn paste_document(&mut self) -> bool {
        let pasted = Clipboard::paste();
        if pasted.is_empty() {
            self.last_error = Some("Clipboard empty".to_string());
            self.refresh_status();
            return false;
        }
        self.push_undo_snapshot();
        // TextField replaces a selection or inserts at the current caret;
        // this avoids the old append-at-end behavior.
        self.editor.replace_selection(&pasted);
        self.mark_dirty_from_editor();
        true
    }

    fn select_all_document(&mut self) -> bool {
        self.editor.select_all();
        self.last_error = None;
        self.refresh_status();
        true
    }

    fn new_document(&mut self) -> bool {
        self.push_undo_snapshot();
        self.document_path = None;
        self.sync_path_field();
        self.saved_text.clear();
        self.editor.set_text("");
        self.dirty = false;
        self.last_error = None;
        self.redo_stack.clear();
        self.refresh_status();
        true
    }

    fn path_from_field_or_default(&mut self) -> PathBuf {
        let typed = self.path_field.text().trim().to_string();
        if typed.is_empty() {
            default_file_path()
        } else {
            PathBuf::from(typed)
        }
    }

    fn open_path(&mut self, path: PathBuf) -> bool {
        match save::open_document(&path) {
            Ok(document) => {
                self.push_undo_snapshot();
                self.document_path = Some(path.clone());
                self.sync_path_field();
                self.editor.set_text(document.text);
                self.saved_text = document.saved_text;
                self.dirty = document.recovered;
                self.last_error = None;
                self.redo_stack.clear();
                if document.recovered {
                    self.notify(format!("Recovered unsaved changes from {}", path.display()));
                } else {
                    self.notify(format!("Opened {}", path.display()));
                }
                true
            }
            Err(err) => {
                self.last_error = Some(format!("Could not open: {err}"));
                self.refresh_status();
                false
            }
        }
    }

    /// Cmd+O: open from path field, falling back to TEXTEDIT_FILE / /tmp default.
    fn open_document(&mut self) -> bool {
        let path = self.path_from_field_or_default();
        self.open_path(path)
    }

    fn save_document(&mut self) -> bool {
        // Use the set document path, or fall back to TEXTEDIT_FILE / /tmp default.
        let path = self.document_path.clone().unwrap_or_else(default_file_path);
        let text = self.editor.text().to_string();

        match save::save_document(&path, &text) {
            Ok(()) => {
                self.document_path = Some(path.clone());
                self.sync_path_field();
                self.saved_text = text;
                self.dirty = false;
                self.last_error = None;
                self.notify(format!("Saved to {}", path.display()));
                true
            }
            Err(err) => {
                self.last_error = Some(format!("Could not save: {err}"));
                self.refresh_status();
                false
            }
        }
    }

    fn save_as_from_path_field(&mut self) -> bool {
        let path = self.path_from_field_or_default();
        let text = self.editor.text().to_string();
        match save::save_document(&path, &text) {
            Ok(()) => {
                self.document_path = Some(path.clone());
                self.sync_path_field();
                self.saved_text = text;
                self.dirty = false;
                self.last_error = None;
                self.notify(format!("Saved to {}", path.display()));
                true
            }
            Err(err) => {
                self.last_error = Some(format!("Could not save as: {err}"));
                self.refresh_status();
                false
            }
        }
    }

    // ----- Find -----

    fn toggle_find(&mut self) {
        self.find_visible = !self.find_visible;
        self.apply_find_visibility();
        let target = if self.find_visible {
            self.find_field.id()
        } else {
            self.editor.id()
        };
        self.focus_widget(target);
        self.refresh_status();
    }

    /// Execute a find for the text currently in the find field.
    /// Moves the editor cursor to the first match after the current cursor position
    /// (wraps around). Returns true if a match was found.
    fn execute_find(&mut self) -> bool {
        let query = self.find_field.text().to_string();
        if query.is_empty() {
            self.last_error = Some("Enter a search term".to_string());
            self.refresh_status();
            return false;
        }
        self.last_find_query = query.clone();

        let text = self.editor.text().to_string();
        let cursor = self.editor.cursor_position();

        // Search from after current cursor first, then wrap around.
        let found = if cursor < text.len() {
            text[cursor..].find(&query).map(|offset| cursor + offset)
        } else {
            None
        };
        let found = found.or_else(|| text.find(&query));

        match found {
            Some(pos) => {
                // Select the match and leave the caret at its end so the next
                // edit replaces it rather than silently appending elsewhere.
                self.editor.set_selection(pos, pos + query.len());
                self.notify(format!("Found \"{}\" at byte {}", query, pos));
                true
            }
            None => {
                self.last_error = Some(format!("Not found: {query}"));
                self.refresh_status();
                false
            }
        }
    }

    /// Index of the toolbar button activated since last asked, drained
    /// through the widgets' own `take_clicked()`.
    fn take_toolbar_click(&mut self) -> Option<usize> {
        self.toolbar.items.iter_mut().position(|item| {
            item.as_any_mut()
                .downcast_mut::<Button>()
                .is_some_and(|button| button.take_clicked())
        })
    }

    /// Drain widget activations after an event went through generic
    /// dispatch. Returns whether a toolbar action ran.
    fn process_activations(&mut self) -> bool {
        let Some(index) = self.take_toolbar_click() else {
            return false;
        };
        match index {
            0 => {
                self.new_document();
            }
            1 => {
                self.open_document();
            }
            2 => {
                self.save_document();
            }
            3 => {
                self.save_as_from_path_field();
            }
            4 => {
                self.undo();
            }
            5 => {
                self.redo();
            }
            6 => {
                self.toggle_find();
            }
            7 => {
                self.copy_document();
            }
            8 => {
                self.paste_document();
            }
            _ => return false,
        }
        true
    }
}

impl Widget for TextEditView {
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

        let toolbar_h = 32.0;
        let path_h = 30.0;
        let find_h = if self.find_visible { 30.0 } else { 0.0 };
        let status_h = 24.0;
        let editor_h = (rect.height - toolbar_h - path_h - find_h - status_h).max(0.0);

        self.toolbar
            .set_rect(Rect::new(rect.x, rect.y, rect.width, toolbar_h));
        let _ = self
            .toolbar
            .layout(LayoutConstraint::tight(Size::new(rect.width, toolbar_h)));

        // PATH row
        self.path_label.set_rect(Rect::new(
            rect.x + 8.0,
            rect.y + toolbar_h + 4.0,
            46.0,
            22.0,
        ));
        let _ = self
            .path_label
            .layout(LayoutConstraint::tight(Size::new(46.0, 22.0)));

        let path_field_x = rect.x + 58.0;
        let path_field_w = (rect.width - 66.0).max(0.0);
        self.path_field.set_rect(Rect::new(
            path_field_x,
            rect.y + toolbar_h + 2.0,
            path_field_w,
            26.0,
        ));
        let _ = self
            .path_field
            .layout(LayoutConstraint::tight(Size::new(path_field_w, 26.0)));

        // FIND row (conditionally visible)
        let find_row_y = rect.y + toolbar_h + path_h;
        self.find_label
            .set_rect(Rect::new(rect.x + 8.0, find_row_y + 4.0, 46.0, 22.0));
        let _ = self
            .find_label
            .layout(LayoutConstraint::tight(Size::new(46.0, 22.0)));

        let find_field_w = (rect.width - 66.0).max(0.0);
        self.find_field.set_rect(Rect::new(
            rect.x + 58.0,
            find_row_y + 2.0,
            find_field_w,
            26.0,
        ));
        let _ = self
            .find_field
            .layout(LayoutConstraint::tight(Size::new(find_field_w, 26.0)));

        // EDITOR
        self.editor.set_rect(Rect::new(
            rect.x,
            rect.y + toolbar_h + path_h + find_h,
            rect.width,
            editor_h,
        ));
        let _ = self
            .editor
            .layout(LayoutConstraint::tight(Size::new(rect.width, editor_h)));

        // STATUS
        self.status.set_rect(Rect::new(
            rect.x,
            rect.y + toolbar_h + path_h + find_h + editor_h,
            rect.width,
            status_h,
        ));
        let _ = self
            .status
            .layout(LayoutConstraint::tight(Size::new(rect.width, status_h)));

        size
    }

    fn draw(&self, theme: &ThemeContext) {
        self.toolbar.draw(theme);
        self.path_label.draw(theme);
        self.path_field.draw(theme);
        if self.find_visible {
            self.find_label.draw(theme);
            self.find_field.draw(theme);
        }
        self.editor.draw(theme);
        self.status.draw(theme);
    }

    fn handle_event(&mut self, event: &Event) -> EventResult {
        // Global keyboard shortcuts.
        if let Event::KeyDown { key, modifiers } = event {
            if modifiers.meta {
                match key {
                    KeyCode::N => {
                        self.new_document();
                        return EventResult::Handled;
                    }
                    KeyCode::S => {
                        if modifiers.shift {
                            self.save_as_from_path_field();
                        } else {
                            self.save_document();
                        }
                        return EventResult::Handled;
                    }
                    KeyCode::O => {
                        self.open_document();
                        return EventResult::Handled;
                    }
                    KeyCode::F => {
                        self.toggle_find();
                        return EventResult::Handled;
                    }
                    KeyCode::Z if modifiers.shift => {
                        self.redo();
                        return EventResult::Handled;
                    }
                    KeyCode::Z => {
                        self.undo();
                        return EventResult::Handled;
                    }
                    KeyCode::X => {
                        self.cut_document();
                        return EventResult::Handled;
                    }
                    KeyCode::C => {
                        self.copy_document();
                        return EventResult::Handled;
                    }
                    KeyCode::V => {
                        self.paste_document();
                        return EventResult::Handled;
                    }
                    KeyCode::A => {
                        self.select_all_document();
                        return EventResult::Handled;
                    }
                    _ => {}
                }
            }

            // Enter in the find field executes the search.
            if *key == KeyCode::Enter && self.find_field.widget_state().focused {
                self.execute_find();
                return EventResult::Handled;
            }

            // Escape closes the find bar.
            if *key == KeyCode::Escape && self.find_field.widget_state().focused {
                self.find_visible = false;
                self.apply_find_visibility();
                let editor_id = self.editor.id();
                self.focus_widget(editor_id);
                self.refresh_status();
                return EventResult::Handled;
            }
        }

        match event {
            // Tab / Shift+Tab walk the focusable widgets (toolbar buttons,
            // path field, find field when visible, editor).
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
            // Keys go to the focused widget; the editor's text changes feed
            // the undo stack and dirty tracking.
            Event::KeyDown { .. } | Event::KeyUp { .. } | Event::Char { .. } => {
                let before_edit = self.editor.text().to_string();
                let mut focus = std::mem::take(&mut self.focus);
                let result = focus.dispatch_key(self, event);
                self.focus = focus;
                if matches!(result, EventResult::Handled) {
                    if self.process_activations() {
                        return EventResult::Handled;
                    }
                    if self.editor.text() != before_edit {
                        if self.undo_stack.last() != Some(&before_edit) {
                            self.undo_stack.push(before_edit);
                            // Cap at 50 entries.
                            if self.undo_stack.len() > 50 {
                                self.undo_stack.remove(0);
                            }
                        }
                        self.redo_stack.clear();
                    }
                    self.mark_dirty_from_editor();
                }
                result
            }
            // Pointer events go through generic rect-checked dispatch with
            // implicit capture; a press that lands on a focusable widget
            // (the text fields) moves focus there.
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
                    // No toolbar action ran: a field click may have moved the
                    // caret, so recompute the Ln display.
                    self.refresh_status();
                }
                result
            }
            _ => EventResult::Ignored,
        }
    }

    fn update(&mut self) {
        self.toolbar.update();
        self.path_label.update();
        self.path_field.update();
        self.find_label.update();
        self.find_field.update();
        self.editor.update();
        self.status.update();
    }

    fn accessibility(&self) -> Option<AccessibilityNode> {
        Some(AccessibilityNode::new(
            AccessibilityRole::Window,
            "TextEdit",
        ))
    }

    fn children(&self) -> Vec<&dyn Widget> {
        vec![
            &self.toolbar,
            &self.path_label,
            &self.path_field,
            &self.find_label,
            &self.find_field,
            &self.editor,
            &self.status,
        ]
    }

    fn children_mut(&mut self) -> Vec<&mut dyn Widget> {
        vec![
            &mut self.toolbar,
            &mut self.path_label,
            &mut self.path_field,
            &mut self.find_label,
            &mut self.find_field,
            &mut self.editor,
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

#[cfg(test)]
mod tests {
    use super::*;
    use slopos_kit::event::MouseButton;
    use slopos_kit::Point;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);
    static CLIPBOARD_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn temp_textedit_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir()
            .join(format!("slopos-i_textedit_{unique}_{sequence}"))
            .join(name)
    }

    fn click_toolbar_button(view: &mut TextEditView, index: usize) -> EventResult {
        let rect = view.toolbar.items[index].rect();
        let point = Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
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
    fn textedit_opens_existing_document_and_tracks_dirty_state() {
        let path = temp_textedit_path("note.txt");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "hello").unwrap();

        let mut view = TextEditView::open(Some(path.clone()));
        assert_eq!(view.editor.text(), "hello");
        assert!(!view.dirty);
        assert!(view.status.text.contains("Saved"));

        let result = view.handle_event(&Event::Char { character: '!' });

        assert!(matches!(result, EventResult::Handled));
        assert_eq!(view.editor.text(), "hello!");
        assert!(view.dirty);
        assert!(view.status.text.contains("Edited"));

        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn textedit_save_writes_document_and_clears_dirty_state() {
        let path = temp_textedit_path("note.txt");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "hello").unwrap();

        let mut view = TextEditView::open(Some(path.clone()));
        view.layout(LayoutConstraint::tight(Size::new(640.0, 420.0)));
        let _ = view.handle_event(&Event::Char { character: '!' });

        // Toolbar index 2 = SAVE
        let result = click_toolbar_button(&mut view, 2);

        assert!(matches!(result, EventResult::Handled));
        assert_eq!(fs::read_to_string(&path).unwrap(), "hello!");
        assert!(!view.dirty);
        // After save the notification replaces the normal status line.
        assert!(view.status.text.contains("Saved to"));

        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn textedit_open_toolbar_loads_path_field_document() {
        let path = temp_textedit_path("open.txt");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "opened from path").unwrap();

        let mut view = TextEditView::open(None);
        view.layout(LayoutConstraint::tight(Size::new(700.0, 460.0)));
        view.path_field.set_text(path.display().to_string());

        // Toolbar index 1 = OPEN
        let result = click_toolbar_button(&mut view, 1);

        assert!(matches!(result, EventResult::Handled));
        assert_eq!(view.editor.text(), "opened from path");
        assert_eq!(view.document_path.as_deref(), Some(path.as_path()));
        assert!(!view.dirty);

        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn textedit_save_as_toolbar_writes_path_field_document() {
        let path = temp_textedit_path("saved-as.txt");
        fs::create_dir_all(path.parent().unwrap()).unwrap();

        let mut view = TextEditView::open(None);
        view.layout(LayoutConstraint::tight(Size::new(700.0, 460.0)));
        view.editor.set_text("save as body");
        view.mark_dirty_from_editor();
        view.path_field.set_text(path.display().to_string());

        // Toolbar index 3 = SAVE AS
        let result = click_toolbar_button(&mut view, 3);

        assert!(matches!(result, EventResult::Handled));
        assert_eq!(fs::read_to_string(&path).unwrap(), "save as body");
        assert_eq!(view.document_path.as_deref(), Some(path.as_path()));
        assert!(!view.dirty);

        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn textedit_shift_cmd_s_runs_save_as() {
        let path = temp_textedit_path("shortcut-save-as.txt");
        fs::create_dir_all(path.parent().unwrap()).unwrap();

        let mut view = TextEditView::open(None);
        view.editor.set_text("shortcut body");
        view.mark_dirty_from_editor();
        view.path_field.set_text(path.display().to_string());

        let result = view.handle_event(&Event::KeyDown {
            key: KeyCode::S,
            modifiers: Modifiers {
                shift: true,
                control: false,
                alt: false,
                meta: true,
            },
        });

        assert!(matches!(result, EventResult::Handled));
        assert_eq!(fs::read_to_string(&path).unwrap(), "shortcut body");
        assert_eq!(view.document_path.as_deref(), Some(path.as_path()));

        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    /// Serializes tests that read or mutate the process-global TEXTEDIT_FILE
    /// env var; without it, parallel test threads race on default_file_path().
    static DEFAULT_FILE_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn textedit_cmd_s_without_path_saves_to_default() {
        let _env = DEFAULT_FILE_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let default_path = default_file_path();
        let mut view = TextEditView::open(None);
        // Clear initial text so we can check what gets written.
        view.editor.set_text("default save test");
        view.mark_dirty_from_editor();
        // No document_path set, no path in path field.
        view.path_field.set_text("");
        view.document_path = None;

        let result = view.handle_event(&Event::KeyDown {
            key: KeyCode::S,
            modifiers: Modifiers {
                shift: false,
                control: false,
                alt: false,
                meta: true,
            },
        });

        assert!(matches!(result, EventResult::Handled));
        assert_eq!(
            fs::read_to_string(&default_path).unwrap(),
            "default save test"
        );
        assert!(view.status.text.contains("Saved to"));

        // Cleanup.
        let _ = fs::remove_file(default_path);
    }

    #[test]
    fn textedit_path_field_accepts_typed_path_when_focused() {
        let mut view = TextEditView::open(None);
        view.layout(LayoutConstraint::tight(Size::new(700.0, 460.0)));
        let rect = view.path_field.rect();

        let focus = view.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point: Point::new(rect.x + 4.0, rect.y + 4.0),
            modifiers: Modifiers::NONE,
        });
        let typed = view.handle_event(&Event::Char { character: 'x' });

        assert!(matches!(focus, EventResult::Handled));
        assert!(matches!(typed, EventResult::Handled));
        assert_eq!(view.path_field.text(), "x");
        assert!(!view.editor.text().ends_with('x'));
    }

    #[test]
    fn textedit_toolbar_click_does_not_steal_editor_focus() {
        let mut view = TextEditView::open(None);
        view.layout(LayoutConstraint::tight(Size::new(700.0, 460.0)));
        assert!(view.editor.widget_state().focused);

        // Click UNDO (index 4; clipboard-neutral, tests share one global
        // clipboard): the action runs but focus must stay in the editor so
        // the user can keep typing.
        let result = click_toolbar_button(&mut view, 4);
        assert!(matches!(result, EventResult::Handled));
        assert!(view.editor.widget_state().focused);

        let before = view.editor.text().to_string();
        let typed = view.handle_event(&Event::Char { character: 'z' });
        assert!(matches!(typed, EventResult::Handled));
        assert_eq!(view.editor.text(), format!("{before}z"));
    }

    #[test]
    fn textedit_hidden_find_field_is_skipped_by_tab_and_clicks() {
        let mut view = TextEditView::open(None);
        view.layout(LayoutConstraint::tight(Size::new(700.0, 460.0)));
        assert!(!view.find_visible);
        assert!(
            !view.find_field.focusable(),
            "hidden find field must not join tab order"
        );

        // Tab through every focusable widget; the find field must never
        // become focused while hidden.
        for _ in 0..12 {
            let _ = view.handle_event(&Event::KeyDown {
                key: KeyCode::Tab,
                modifiers: Modifiers::NONE,
            });
            assert!(!view.find_field.widget_state().focused);
        }

        // Once the find bar opens it takes focus and accepts input.
        view.toggle_find();
        assert!(view.find_field.widget_state().focused);
        let typed = view.handle_event(&Event::Char { character: 'q' });
        assert!(matches!(typed, EventResult::Handled));
        assert_eq!(view.find_field.text(), "q");
        assert_eq!(
            view.editor.text(),
            view.saved_text,
            "editor must not receive the keystroke"
        );
    }

    #[test]
    fn textedit_new_document_clears_text_and_path() {
        let path = temp_textedit_path("note.txt");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "hello").unwrap();

        let mut view = TextEditView::open(Some(path.clone()));
        view.layout(LayoutConstraint::tight(Size::new(640.0, 420.0)));

        // Toolbar index 0 = NEW
        let result = click_toolbar_button(&mut view, 0);

        assert!(matches!(result, EventResult::Handled));
        assert_eq!(view.editor.text(), "");
        assert!(view.document_path.is_none());
        assert_eq!(view.path_field.text(), "");
        assert!(!view.dirty);

        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn textedit_undo_and_redo_restore_editor_text() {
        let mut view = TextEditView::open(None);
        view.editor.set_text("hello");
        view.saved_text = "hello".to_string();
        view.dirty = false;

        let _ = view.handle_event(&Event::Char { character: '!' });
        assert_eq!(view.editor.text(), "hello!");

        let undo = view.handle_event(&Event::KeyDown {
            key: KeyCode::Z,
            modifiers: Modifiers {
                shift: false,
                control: false,
                alt: false,
                meta: true,
            },
        });
        assert!(matches!(undo, EventResult::Handled));
        assert_eq!(view.editor.text(), "hello");
        assert!(!view.dirty);

        let redo = view.handle_event(&Event::KeyDown {
            key: KeyCode::Z,
            modifiers: Modifiers {
                shift: true,
                control: false,
                alt: false,
                meta: true,
            },
        });
        assert!(matches!(redo, EventResult::Handled));
        assert_eq!(view.editor.text(), "hello!");
        assert!(view.dirty);
    }

    #[test]
    fn textedit_undo_stack_capped_at_50() {
        let mut view = TextEditView::open(None);
        view.editor.set_text("");
        view.saved_text = String::new();

        // Type 60 characters, each pushing a snapshot.
        for _ in 0..60 {
            view.handle_event(&Event::Char { character: 'a' });
        }
        assert!(
            view.undo_stack.len() <= 50,
            "undo stack exceeded 50 entries"
        );
    }

    #[test]
    fn textedit_copy_cut_and_paste_use_clipboard() {
        let _clipboard = CLIPBOARD_TEST_LOCK.lock().unwrap();
        Clipboard::clear();
        let mut view = TextEditView::open(None);
        view.editor.set_text("clip");
        view.saved_text.clear();
        view.dirty = true;

        let copy = view.handle_event(&Event::KeyDown {
            key: KeyCode::C,
            modifiers: Modifiers {
                shift: false,
                control: false,
                alt: false,
                meta: true,
            },
        });
        assert!(matches!(copy, EventResult::Handled));
        assert_eq!(Clipboard::paste(), "clip");

        let cut = view.handle_event(&Event::KeyDown {
            key: KeyCode::X,
            modifiers: Modifiers {
                shift: false,
                control: false,
                alt: false,
                meta: true,
            },
        });
        assert!(matches!(cut, EventResult::Handled));
        assert_eq!(view.editor.text(), "");
        assert_eq!(Clipboard::paste(), "clip");

        let paste = view.handle_event(&Event::KeyDown {
            key: KeyCode::V,
            modifiers: Modifiers {
                shift: false,
                control: false,
                alt: false,
                meta: true,
            },
        });
        assert!(matches!(paste, EventResult::Handled));
        assert_eq!(view.editor.text(), "clip");
    }

    #[test]
    fn textedit_selection_clipboard_replaces_only_selected_unicode_range() {
        let _clipboard = CLIPBOARD_TEST_LOCK.lock().unwrap();
        Clipboard::clear();
        let mut view = TextEditView::open(None);
        view.editor.set_text("aé🙂z");
        view.saved_text = view.editor.text().to_string();
        view.dirty = false;
        // Select `é🙂` by UTF-8 byte range (1..7), leaving the surrounding
        // characters untouched.
        view.editor.set_selection(1, 7);

        let copy = view.handle_event(&Event::KeyDown {
            key: KeyCode::C,
            modifiers: Modifiers {
                meta: true,
                ..Modifiers::NONE
            },
        });
        assert!(matches!(copy, EventResult::Handled));
        assert_eq!(Clipboard::paste(), "é🙂");
        assert_eq!(view.editor.text(), "aé🙂z");

        let cut = view.handle_event(&Event::KeyDown {
            key: KeyCode::X,
            modifiers: Modifiers {
                meta: true,
                ..Modifiers::NONE
            },
        });
        assert!(matches!(cut, EventResult::Handled));
        assert_eq!(Clipboard::paste(), "é🙂");
        assert_eq!(view.editor.text(), "az");
        assert_eq!(view.editor.cursor_position(), 1);
        assert!(view.dirty);

        Clipboard::copy("XY");
        let paste = view.handle_event(&Event::KeyDown {
            key: KeyCode::V,
            modifiers: Modifiers {
                meta: true,
                ..Modifiers::NONE
            },
        });
        assert!(matches!(paste, EventResult::Handled));
        assert_eq!(view.editor.text(), "aXYz");
        assert_eq!(view.editor.cursor_position(), 3);
    }

    #[test]
    fn textedit_cmd_a_selects_without_overwriting_clipboard() {
        let _clipboard = CLIPBOARD_TEST_LOCK.lock().unwrap();
        Clipboard::copy("keep me");
        let mut view = TextEditView::open(None);
        view.editor.set_text("select me");

        let selected = view.handle_event(&Event::KeyDown {
            key: KeyCode::A,
            modifiers: Modifiers {
                meta: true,
                ..Modifiers::NONE
            },
        });
        assert!(matches!(selected, EventResult::Handled));
        assert_eq!(view.editor.selected_text(), Some("select me"));
        assert_eq!(Clipboard::paste(), "keep me");
    }

    #[test]
    fn textedit_word_count_counts_whitespace_separated_tokens() {
        let mut view = TextEditView::open(None);
        view.editor.set_text("hello world\nthis is a test");
        assert_eq!(view.word_count(), 6);

        view.editor.set_text("   ");
        assert_eq!(view.word_count(), 0);

        view.editor.set_text("");
        assert_eq!(view.word_count(), 0);
    }

    #[test]
    fn textedit_line_number_tracks_newlines_before_cursor() {
        let mut view = TextEditView::open(None);
        // set_text moves cursor to end.
        view.editor.set_text("line1\nline2\nline3");
        // Cursor at end of line3 => line 3.
        assert_eq!(view.current_line(), 3);

        // Move cursor to start of text.
        view.editor.set_cursor_position(0);
        assert_eq!(view.current_line(), 1);

        // Move cursor to start of line2 (after "line1\n" = 6 bytes).
        view.editor.set_cursor_position(6);
        assert_eq!(view.current_line(), 2);
    }

    #[test]
    fn textedit_find_moves_cursor_to_first_match() {
        let mut view = TextEditView::open(None);
        view.editor.set_text("foo bar foo baz");
        view.editor.set_cursor_position(0);

        view.find_field.set_text("foo");
        let found = view.execute_find();

        assert!(found);
        // Cursor should be at end of first "foo" (byte 3).
        assert_eq!(view.editor.cursor_position(), 3);
    }

    #[test]
    fn textedit_find_wraps_around_from_end_of_document() {
        let mut view = TextEditView::open(None);
        view.editor.set_text("foo bar foo baz");
        // Start cursor at "foo baz" section (after second foo, byte 11).
        view.editor.set_cursor_position(11);

        view.find_field.set_text("foo");
        let found = view.execute_find();

        assert!(found);
        // Should wrap to first "foo" at byte 0, cursor placed at byte 3.
        assert_eq!(view.editor.cursor_position(), 3);
    }

    #[test]
    fn textedit_find_not_found_sets_error() {
        let mut view = TextEditView::open(None);
        view.editor.set_text("hello world");
        view.editor.set_cursor_position(0);

        view.find_field.set_text("zzz");
        let found = view.execute_find();

        assert!(!found);
        assert!(view.last_error.is_some());
    }

    #[test]
    fn textedit_status_bar_shows_word_count_and_line() {
        let mut view = TextEditView::open(None);
        view.editor.set_text("one two three\nfour");
        view.last_error = None;
        view.notification = None;
        view.refresh_status();

        assert!(
            view.status.text.contains("4w"),
            "expected '4w' in status: {}",
            view.status.text
        );
        assert!(
            view.status.text.contains("Ln 2"),
            "expected 'Ln 2' in status: {}",
            view.status.text
        );
    }

    #[test]
    fn textedit_cmd_o_without_path_field_uses_default() {
        // Use an isolated temp path via TEXTEDIT_FILE to avoid collisions with the
        // save-to-default test which also uses default_file_path().
        let _env = DEFAULT_FILE_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let path = temp_textedit_path("cmd-o-default.txt");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "default open content").unwrap();

        // Set TEXTEDIT_FILE so default_file_path() returns our temp file.
        std::env::set_var("TEXTEDIT_FILE", path.display().to_string());

        let mut view = TextEditView::open(None);
        view.path_field.set_text(""); // empty path field
        view.document_path = None;

        let result = view.handle_event(&Event::KeyDown {
            key: KeyCode::O,
            modifiers: Modifiers {
                shift: false,
                control: false,
                alt: false,
                meta: true,
            },
        });

        std::env::remove_var("TEXTEDIT_FILE");

        assert!(matches!(result, EventResult::Handled));
        assert_eq!(view.editor.text(), "default open content");

        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
