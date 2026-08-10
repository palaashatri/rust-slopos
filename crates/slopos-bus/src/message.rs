use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageKind {
    Command(Command),
    Event(Event),
    Query(Query),
    Response(Response),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Command {
    LaunchApplication {
        bundle_id: String,
    },
    OpenDocument {
        path: String,
        app_id: Option<String>,
    },
    ShowPreferences,
    QuitApplication {
        bundle_id: String,
    },
    SetTheme {
        name: String,
    },
    SwitchWorkspace {
        index: usize,
    },
    ShowNotification {
        title: String,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    WindowFocused { window_id: String, app_id: String },
    WindowClosed { window_id: String },
    ThemeChanged { name: String, is_dark: bool },
    VolumeMounted { path: String, name: String },
    VolumeUnmounted { path: String },
    AppLaunched { bundle_id: String },
    AppTerminated { bundle_id: String },
    WorkspaceChanged { index: usize },
    DisplayChanged { width: u32, height: u32, scale: f32 },
    ScreenLocked,
    ScreenUnlocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Query {
    GetRunningApplications,
    GetTheme,
    GetWorkspaceState,
    GetWindowList,
    GetApplicationInfo { bundle_id: String },
    GetSetting { key: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub success: bool,
    pub data: Option<serde_json::Value>,
    pub error: Option<String>,
}
