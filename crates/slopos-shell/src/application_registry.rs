use std::collections::HashMap;
use uuid::Uuid;

pub struct ApplicationRegistry {
    pub apps: HashMap<String, RunningApp>,
}

pub struct RunningApp {
    pub bundle_id: String,
    pub pid: Option<u32>,
    pub windows: Vec<Uuid>,
    pub focused: bool,
    pub launch_time: std::time::Instant,
}

impl Default for ApplicationRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ApplicationRegistry {
    pub fn new() -> Self {
        Self {
            apps: HashMap::new(),
        }
    }

    pub fn register(&mut self, bundle_id: &str) {
        self.apps.insert(
            bundle_id.to_string(),
            RunningApp {
                bundle_id: bundle_id.to_string(),
                pid: None,
                windows: vec![],
                focused: false,
                launch_time: std::time::Instant::now(),
            },
        );
    }

    pub fn unregister(&mut self, bundle_id: &str) {
        self.apps.remove(bundle_id);
    }

    pub fn running_apps(&self) -> Vec<&RunningApp> {
        self.apps.values().collect()
    }

    pub fn is_running(&self, bundle_id: &str) -> bool {
        self.apps.contains_key(bundle_id)
    }

    pub fn focused_app(&self) -> Option<&RunningApp> {
        self.apps.values().find(|a| a.focused)
    }

    pub fn set_focused(&mut self, bundle_id: &str) {
        for app in self.apps.values_mut() {
            app.focused = app.bundle_id == bundle_id;
        }
    }
}
