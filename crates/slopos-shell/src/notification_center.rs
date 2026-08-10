#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationPriority {
    Low,
    Normal,
    High,
    Critical,
}

#[derive(Debug, Clone)]
pub struct Notification {
    pub id: String,
    pub app_id: String,
    pub title: String,
    pub message: String,
    pub icon: Option<String>,
    pub priority: NotificationPriority,
    pub timestamp: std::time::Instant,
    pub dismissed: bool,
}

pub struct NotificationCenter {
    pub notifications: Vec<Notification>,
    pub max_visible: usize,
}

impl Default for NotificationCenter {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationCenter {
    pub fn new() -> Self {
        Self {
            notifications: vec![],
            max_visible: 5,
        }
    }

    pub fn post(
        &mut self,
        app_id: &str,
        title: &str,
        message: &str,
        priority: NotificationPriority,
    ) -> String {
        let id = format!("notif-{}", self.notifications.len());
        self.notifications.push(Notification {
            id: id.clone(),
            app_id: app_id.to_string(),
            title: title.to_string(),
            message: message.to_string(),
            icon: None,
            priority,
            timestamp: std::time::Instant::now(),
            dismissed: false,
        });
        id
    }

    pub fn dismiss(&mut self, id: &str) {
        if let Some(notif) = self.notifications.iter_mut().find(|n| n.id == id) {
            notif.dismissed = true;
        }
    }

    pub fn dismiss_all(&mut self) {
        for notif in &mut self.notifications {
            notif.dismissed = true;
        }
    }

    pub fn visible(&self) -> Vec<&Notification> {
        self.notifications
            .iter()
            .filter(|n| !n.dismissed)
            .take(self.max_visible)
            .collect()
    }

    pub fn clear_expired(&mut self, max_age: std::time::Duration) {
        let now = std::time::Instant::now();
        self.notifications
            .retain(|n| n.dismissed || now.duration_since(n.timestamp) < max_age);
    }

    pub fn render_notifications(&self) -> slopos_render::RenderNode {
        let mut children = vec![];
        let mut y = 30.0;

        for notif in self.visible() {
            children.push(slopos_render::RenderNode::Rect {
                x: 1600.0,
                y,
                width: 300.0,
                height: 80.0,
                color: slopos_render::Color::new(0.95, 0.95, 0.95, 0.9),
                corner_radius: 6.0,
            });
            children.push(slopos_render::RenderNode::Text {
                x: 1610.0,
                y: y + 20.0,
                text: notif.title.clone(),
                font_size: 13.0,
                color: slopos_render::Color::BLACK,
            });
            children.push(slopos_render::RenderNode::Text {
                x: 1610.0,
                y: y + 45.0,
                text: notif.message.clone(),
                font_size: 11.0,
                color: slopos_render::Color::new(0.3, 0.3, 0.3, 1.0),
            });
            y += 90.0;
        }

        slopos_render::RenderNode::Group { children }
    }
}
