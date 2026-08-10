use slopos_kit::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    NotRunning,
    Running,
    Focused,
    AttentionRequired,
}

#[derive(Debug, Clone)]
pub struct DockItem {
    pub app_id: String,
    pub label: String,
    pub icon: Option<String>,
    pub state: AppState,
    pub is_trash: bool,
    pub is_folder: bool,
    pub position: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockPosition {
    Bottom,
    Left,
    Right,
}

pub struct Dock {
    pub items: Vec<DockItem>,
    pub position: DockPosition,
    pub auto_hide: bool,
    pub magnification: bool,
    pub icon_size: f32,
    pub running_apps: Vec<String>,
    pub trash_items: usize,
}

impl Default for Dock {
    fn default() -> Self {
        Self::new()
    }
}

impl Dock {
    pub fn new() -> Self {
        let mut dock = Self {
            items: vec![],
            position: DockPosition::Bottom,
            auto_hide: false,
            magnification: false,
            icon_size: 48.0,
            running_apps: vec![],
            trash_items: 0,
        };
        dock.setup_default_items();
        dock
    }

    fn setup_default_items(&mut self) {
        self.items.push(DockItem {
            app_id: "com.slopos.finder".into(),
            label: "Finder".into(),
            icon: Some("finder".into()),
            state: AppState::Focused,
            is_trash: false,
            is_folder: false,
            position: Rect::ZERO,
        });
        self.items.push(DockItem {
            app_id: "com.slopos.settings".into(),
            label: "Settings".into(),
            icon: Some("settings".into()),
            state: AppState::NotRunning,
            is_trash: false,
            is_folder: false,
            position: Rect::ZERO,
        });
        self.items.push(DockItem {
            app_id: "com.slopos.textedit".into(),
            label: "TextEdit".into(),
            icon: Some("textedit".into()),
            state: AppState::NotRunning,
            is_trash: false,
            is_folder: false,
            position: Rect::ZERO,
        });
        self.items.push(DockItem {
            app_id: "com.slopos.terminal".into(),
            label: "Terminal".into(),
            icon: Some("terminal".into()),
            state: AppState::NotRunning,
            is_trash: false,
            is_folder: false,
            position: Rect::ZERO,
        });
        self.items.push(DockItem {
            app_id: "com.slopos.appstore".into(),
            label: "App Store".into(),
            icon: Some("appstore".into()),
            state: AppState::NotRunning,
            is_trash: false,
            is_folder: false,
            position: Rect::ZERO,
        });
    }

    pub fn add_item(&mut self, app_id: &str, label: &str) {
        self.items.push(DockItem {
            app_id: app_id.to_string(),
            label: label.to_string(),
            icon: None,
            state: AppState::NotRunning,
            is_trash: false,
            is_folder: false,
            position: Rect::ZERO,
        });
    }

    pub fn set_app_state(&mut self, app_id: &str, state: AppState) {
        if let Some(item) = self.items.iter_mut().find(|i| i.app_id == app_id) {
            item.state = state;
        }
        if (state == AppState::Running || state == AppState::Focused)
            && !self.running_apps.contains(&app_id.to_string())
        {
            self.running_apps.push(app_id.to_string());
        }
    }

    pub fn launch_app(&mut self, index: usize) -> Option<String> {
        self.items.get(index).map(|item| item.app_id.clone())
    }

    pub fn layout_dock(&mut self, screen_width: f32, screen_height: f32) {
        let padding = 8.0;
        let item_spacing = 4.0;
        let dock_height = self.icon_size + padding * 2.0;
        let total_width = self.items.len() as f32 * (self.icon_size + item_spacing) + padding * 2.0;
        let start_x = (screen_width - total_width) / 2.0;
        let y = screen_height - dock_height;

        for (i, item) in self.items.iter_mut().enumerate() {
            item.position = Rect::new(
                start_x + padding + i as f32 * (self.icon_size + item_spacing),
                y + padding,
                self.icon_size,
                self.icon_size,
            );
        }
    }

    pub fn render_dock(&self) -> slopos_render::RenderNode {
        let mut children = vec![];
        if let (Some(first), Some(last)) = (self.items.first(), self.items.last()) {
            let padding = 8.0;
            let bg_x = first.position.x - padding;
            let bg_y = first.position.y - padding;
            let bg_w = last.position.x + last.position.width + padding - bg_x;
            let bg_h = self.icon_size + padding * 2.0;

            children.push(slopos_render::RenderNode::Rect {
                x: bg_x,
                y: bg_y,
                width: bg_w,
                height: bg_h,
                color: slopos_render::Color::new(0.8, 0.8, 0.8, 0.9),
                corner_radius: 8.0,
            });
        }

        for item in &self.items {
            children.push(slopos_render::RenderNode::Rect {
                x: item.position.x,
                y: item.position.y,
                width: item.position.width,
                height: item.position.height,
                color: match item.state {
                    AppState::Focused => slopos_render::Color::new(0.6, 0.6, 0.6, 1.0),
                    AppState::Running => slopos_render::Color::new(0.7, 0.7, 0.7, 1.0),
                    AppState::AttentionRequired => slopos_render::Color::new(0.9, 0.3, 0.3, 1.0),
                    AppState::NotRunning => slopos_render::Color::new(0.9, 0.9, 0.9, 1.0),
                },
                corner_radius: 4.0,
            });
            children.push(slopos_render::RenderNode::Text {
                x: item.position.x + 2.0,
                y: item.position.y + item.position.height / 2.0 + 4.0,
                text: item.label.clone(),
                font_size: 11.0,
                color: slopos_render::Color::BLACK,
            });
        }

        slopos_render::RenderNode::Group { children }
    }
}
