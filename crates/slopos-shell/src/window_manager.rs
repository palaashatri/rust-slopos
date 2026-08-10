use slopos_kit::Rect;
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowState {
    Normal,
    Minimized,
    Maximized,
    Fullscreen,
    Hidden,
    Destroyed,
}

#[derive(Debug, Clone)]
pub struct ManagedWindow {
    pub id: Uuid,
    pub app_id: String,
    pub title: String,
    pub rect: Rect,
    pub state: WindowState,
    pub workspace: usize,
    pub is_active: bool,
    pub decorated: bool,
    pub order: usize,
}

pub struct WindowManager {
    pub windows: HashMap<Uuid, ManagedWindow>,
    pub active_window: Option<Uuid>,
    pub focus_history: Vec<Uuid>,
    pub next_order: usize,
}

impl Default for WindowManager {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowManager {
    pub fn new() -> Self {
        Self {
            windows: HashMap::new(),
            active_window: None,
            focus_history: vec![],
            next_order: 1,
        }
    }

    pub fn create_window(&mut self, app_id: &str, title: &str, rect: Rect) -> Uuid {
        let id = Uuid::new_v4();
        let window = ManagedWindow {
            id,
            app_id: app_id.to_string(),
            title: title.to_string(),
            rect,
            state: WindowState::Normal,
            workspace: 0,
            is_active: false,
            decorated: true,
            order: self.next_order,
        };
        self.next_order += 1;
        self.windows.insert(id, window);
        id
    }

    pub fn close_window(&mut self, id: Uuid) {
        self.windows.remove(&id);
        self.focus_history.retain(|&fid| fid != id);
        if self.active_window == Some(id) {
            self.active_window = None;
            self.focus_top_window();
        }
    }

    pub fn focus_window(&mut self, id: Uuid) {
        if !self.windows.contains_key(&id) {
            return;
        }

        for window in self.windows.values_mut() {
            window.is_active = false;
        }

        if let Some(window) = self.windows.get_mut(&id) {
            window.is_active = true;
            window.order = self.next_order;
            self.next_order += 1;
            self.active_window = Some(id);
        }

        self.focus_history.retain(|&fid| fid != id);
        self.focus_history.push(id);
    }

    pub fn minimize_window(&mut self, id: Uuid) {
        if let Some(ref mut window) = self.windows.get_mut(&id) {
            window.state = WindowState::Minimized;
            window.is_active = false;
        }
        if self.active_window == Some(id) {
            self.active_window = None;
            self.focus_top_window();
        }
    }

    pub fn maximize_window(&mut self, id: Uuid) {
        if let Some(ref mut window) = self.windows.get_mut(&id) {
            window.state = WindowState::Maximized;
        }
    }

    pub fn set_fullscreen(&mut self, id: Uuid) {
        if let Some(ref mut window) = self.windows.get_mut(&id) {
            window.state = WindowState::Fullscreen;
            window.decorated = false;
        }
    }

    pub fn restore_window(&mut self, id: Uuid) {
        if let Some(ref mut window) = self.windows.get_mut(&id) {
            window.state = WindowState::Normal;
            window.decorated = true;
        }
    }

    pub fn move_window(&mut self, id: Uuid, new_rect: Rect) {
        if let Some(ref mut window) = self.windows.get_mut(&id) {
            window.rect = new_rect;
        }
    }

    pub fn assign_workspace(&mut self, id: Uuid, workspace: usize) {
        if let Some(ref mut window) = self.windows.get_mut(&id) {
            window.workspace = workspace;
        }
    }

    fn focus_top_window(&mut self) {
        if let Some(id) = self.focus_history.last() {
            if let Some(window) = self.windows.get_mut(id) {
                window.is_active = true;
                self.active_window = Some(*id);
            }
        }
    }

    pub fn windows_on_workspace(&self, workspace: usize) -> Vec<&ManagedWindow> {
        self.windows
            .values()
            .filter(|w| w.workspace == workspace && w.state != WindowState::Hidden)
            .collect()
    }

    pub fn app_windows(&self, app_id: &str) -> Vec<&ManagedWindow> {
        self.windows
            .values()
            .filter(|w| w.app_id == app_id)
            .collect()
    }
}
