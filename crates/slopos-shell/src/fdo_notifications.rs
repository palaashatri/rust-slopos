//! FreeDesktop Notifications (org.freedesktop.Notifications) — pure state + optional D-Bus.
//!
//! Spec: https://specifications.freedesktop.org/notification-spec/
//!
//! Host tests exercise only [`NotificationServerState`] and pure helpers (no D-Bus).
//! On Linux, [`try_register_session_bus`] best-effort claims the well-known name and
//! forwards `Notify` into an optional [`crate::NotificationCenter`].

use crate::notification_center::{NotificationCenter, NotificationPriority};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// FreeDesktop urgency hint values (`hints["urgency"]` as BYTE).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum Urgency {
    Low = 0,
    #[default]
    Normal = 1,
    Critical = 2,
}

impl Urgency {
    /// Parse FreeDesktop urgency byte (unknown values → Normal).
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Low,
            2 => Self::Critical,
            _ => Self::Normal,
        }
    }

    /// Parse notify-send style urgency (`low` / `normal` / `critical`, case-insensitive).
    ///
    /// Also accepts FreeDesktop numeric strings `"0"`, `"1"`, `"2"`.
    pub fn parse(s: &str) -> Option<Self> {
        let t = s.trim();
        match t.to_ascii_lowercase().as_str() {
            "low" | "0" => Some(Self::Low),
            "normal" | "1" => Some(Self::Normal),
            "critical" | "2" => Some(Self::Critical),
            _ => None,
        }
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::Critical => "critical",
        }
    }

    /// Map to in-process shell banner priority.
    pub fn to_priority(self) -> NotificationPriority {
        match self {
            Self::Low => NotificationPriority::Low,
            Self::Normal => NotificationPriority::Normal,
            Self::Critical => NotificationPriority::Critical,
        }
    }
}

/// Pure notification payload (FreeDesktop Notify fields we care about).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationPayload {
    pub app_name: String,
    pub summary: String,
    pub body: String,
    pub replaces_id: u32,
    pub urgency: Urgency,
    pub app_icon: String,
    /// Milliseconds; FreeDesktop uses `-1` for default, `0` for never expire.
    pub expire_timeout_ms: i32,
}

impl NotificationPayload {
    pub fn new(
        app_name: impl Into<String>,
        summary: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            app_name: app_name.into(),
            summary: summary.into(),
            body: body.into(),
            replaces_id: 0,
            urgency: Urgency::Normal,
            app_icon: String::new(),
            expire_timeout_ms: -1,
        }
    }

    pub fn with_replaces_id(mut self, id: u32) -> Self {
        self.replaces_id = id;
        self
    }

    pub fn with_urgency(mut self, urgency: Urgency) -> Self {
        self.urgency = urgency;
        self
    }

    pub fn with_app_icon(mut self, icon: impl Into<String>) -> Self {
        self.app_icon = icon.into();
        self
    }

    pub fn with_expire_timeout_ms(mut self, ms: i32) -> Self {
        self.expire_timeout_ms = ms;
        self
    }
}

/// Stored server-side notification (payload + assigned FreeDesktop id).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredNotification {
    pub id: u32,
    pub payload: NotificationPayload,
}

/// Capabilities advertised by this daemon (honest subset).
pub fn default_capabilities() -> Vec<&'static str> {
    vec!["body", "body-markup", "icon-static"]
}

/// Server identity returned by GetServerInformation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerInformation {
    pub name: String,
    pub vendor: String,
    pub version: String,
    pub spec_version: String,
}

impl Default for ServerInformation {
    fn default() -> Self {
        Self {
            name: "SLOPOS-I".to_string(),
            vendor: "SLOPOS-I".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            spec_version: "1.2".to_string(),
        }
    }
}

/// Pure FreeDesktop-compatible notification server state (no D-Bus).
///
/// ID allocation is monotonic `u32`, never zero (0 is FreeDesktop “no id”).
#[derive(Debug, Clone)]
pub struct NotificationServerState {
    next_id: u32,
    notifications: HashMap<u32, NotificationPayload>,
    server_info: ServerInformation,
}

impl Default for NotificationServerState {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationServerState {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            notifications: HashMap::new(),
            server_info: ServerInformation::default(),
        }
    }

    pub fn with_server_info(mut self, info: ServerInformation) -> Self {
        self.server_info = info;
        self
    }

    /// Allocate a new FreeDesktop notification id (never 0).
    pub fn allocate_id(&mut self) -> u32 {
        // Guarantee progress even if the map is dense; skip 0 forever.
        for _ in 0..u32::MAX {
            let id = self.next_id;
            self.next_id = match self.next_id.checked_add(1) {
                Some(n) if n != 0 => n,
                _ => 1,
            };
            if id == 0 {
                continue;
            }
            if !self.notifications.contains_key(&id) {
                return id;
            }
        }
        // Pathological full map: still return a non-zero id (overwrite path via notify).
        1
    }

    /// FreeDesktop `Notify` — insert or replace; returns the notification id.
    pub fn notify(&mut self, payload: NotificationPayload) -> u32 {
        let id =
            if payload.replaces_id != 0 && self.notifications.contains_key(&payload.replaces_id) {
                payload.replaces_id
            } else {
                self.allocate_id()
            };
        self.notifications.insert(id, payload);
        id
    }

    /// Convenience: construct payload fields then [`notify`](Self::notify).
    #[allow(clippy::too_many_arguments)]
    pub fn notify_fields(
        &mut self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        urgency: Urgency,
        expire_timeout_ms: i32,
    ) -> u32 {
        self.notify(
            NotificationPayload::new(app_name, summary, body)
                .with_replaces_id(replaces_id)
                .with_app_icon(app_icon)
                .with_urgency(urgency)
                .with_expire_timeout_ms(expire_timeout_ms),
        )
    }

    /// FreeDesktop `CloseNotification` — returns true if the id was present.
    pub fn close(&mut self, id: u32) -> bool {
        self.notifications.remove(&id).is_some()
    }

    pub fn get(&self, id: u32) -> Option<&NotificationPayload> {
        self.notifications.get(&id)
    }

    pub fn active_ids(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self.notifications.keys().copied().collect();
        ids.sort_unstable();
        ids
    }

    pub fn len(&self) -> usize {
        self.notifications.len()
    }

    pub fn is_empty(&self) -> bool {
        self.notifications.is_empty()
    }

    pub fn get_capabilities(&self) -> Vec<&'static str> {
        default_capabilities()
    }

    pub fn get_server_information(&self) -> ServerInformation {
        self.server_info.clone()
    }
}

/// Parse urgency from FreeDesktop hints map-like key/value pairs.
///
/// Looks for key `"urgency"` with a single-byte / integer / string value encoding.
pub fn urgency_from_hint_bytes(raw: Option<u8>) -> Urgency {
    raw.map(Urgency::from_u8).unwrap_or_default()
}

/// Minimal notify-send style CLI fragment parser for tests and tooling.
///
/// Recognizes `-u` / `--urgency` followed by a value; remaining non-flag tokens
/// are treated as `summary` then `body`. Does not shell-escape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotifySendStyle {
    pub urgency: Urgency,
    pub summary: String,
    pub body: String,
    pub app_name: String,
}

impl NotifySendStyle {
    pub fn parse(args: &[&str]) -> Self {
        let mut urgency = Urgency::Normal;
        let mut app_name = String::new();
        let mut positional: Vec<&str> = Vec::new();
        let mut i = 0;
        while i < args.len() {
            let arg = args[i];
            if arg == "-u" || arg == "--urgency" {
                if let Some(v) = args.get(i + 1) {
                    if let Some(u) = Urgency::parse(v) {
                        urgency = u;
                    }
                    i += 2;
                    continue;
                }
            } else if arg == "-a" || arg == "--app-name" {
                if let Some(v) = args.get(i + 1) {
                    app_name = (*v).to_string();
                    i += 2;
                    continue;
                }
            } else if arg.starts_with('-') {
                // Skip unknown single flags; skip flag+value pairs for common notify-send opts.
                if matches!(
                    arg,
                    "-i" | "--icon" | "-c" | "--category" | "-t" | "--expire-time"
                ) && args.get(i + 1).is_some()
                {
                    i += 2;
                    continue;
                }
                i += 1;
                continue;
            } else {
                positional.push(arg);
            }
            i += 1;
        }
        let summary = positional.first().copied().unwrap_or("").to_string();
        let body = positional.get(1).copied().unwrap_or("").to_string();
        Self {
            urgency,
            summary,
            body,
            app_name,
        }
    }

    pub fn into_payload(self) -> NotificationPayload {
        NotificationPayload::new(self.app_name, self.summary, self.body).with_urgency(self.urgency)
    }
}

/// Daemon that owns FreeDesktop state and optionally mirrors into [`NotificationCenter`].
pub struct NotificationDaemon {
    state: NotificationServerState,
    center: Option<Arc<RwLock<NotificationCenter>>>,
    /// FreeDesktop id → NotificationCenter string id.
    center_ids: HashMap<u32, String>,
}

impl NotificationDaemon {
    pub fn new() -> Self {
        Self {
            state: NotificationServerState::new(),
            center: None,
            center_ids: HashMap::new(),
        }
    }

    pub fn with_notification_center(center: Arc<RwLock<NotificationCenter>>) -> Self {
        Self {
            state: NotificationServerState::new(),
            center: Some(center),
            center_ids: HashMap::new(),
        }
    }

    pub fn state(&self) -> &NotificationServerState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut NotificationServerState {
        &mut self.state
    }

    /// FreeDesktop Notify → server state + optional NotificationCenter banner.
    pub fn notify(&mut self, payload: NotificationPayload) -> u32 {
        let replaces = payload.replaces_id;
        let app_name = payload.app_name.clone();
        let summary = payload.summary.clone();
        let body = payload.body.clone();
        let priority = payload.urgency.to_priority();

        let id = self.state.notify(payload);

        if let Some(center) = &self.center {
            // Replacing: dismiss previous center banner if we tracked it.
            if replaces != 0 {
                if let Some(old_center_id) = self.center_ids.remove(&replaces) {
                    center.write().dismiss(&old_center_id);
                }
            }
            // If we replaced in-place under the same FreeDesktop id, clear any prior mapping.
            if let Some(old_center_id) = self.center_ids.remove(&id) {
                center.write().dismiss(&old_center_id);
            }
            let app_id = if app_name.is_empty() {
                "org.freedesktop.Notifications"
            } else {
                app_name.as_str()
            };
            let center_id = center.write().post(app_id, &summary, &body, priority);
            self.center_ids.insert(id, center_id);
        }

        id
    }

    #[allow(clippy::too_many_arguments)]
    pub fn notify_fields(
        &mut self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        urgency: Urgency,
        expire_timeout_ms: i32,
    ) -> u32 {
        self.notify(
            NotificationPayload::new(app_name, summary, body)
                .with_replaces_id(replaces_id)
                .with_app_icon(app_icon)
                .with_urgency(urgency)
                .with_expire_timeout_ms(expire_timeout_ms),
        )
    }

    pub fn close(&mut self, id: u32) -> bool {
        let closed = self.state.close(id);
        if let Some(center_id) = self.center_ids.remove(&id) {
            if let Some(center) = &self.center {
                center.write().dismiss(&center_id);
            }
        }
        closed
    }

    pub fn get_capabilities(&self) -> Vec<&'static str> {
        self.state.get_capabilities()
    }

    pub fn get_server_information(&self) -> ServerInformation {
        self.state.get_server_information()
    }
}

impl Default for NotificationDaemon {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Optional FreeDesktop D-Bus export (Linux only)
// ---------------------------------------------------------------------------

/// Well-known bus name for the notifications daemon.
pub const FDO_NOTIFICATIONS_BUS_NAME: &str = "org.freedesktop.Notifications";
/// Object path for the notifications daemon.
pub const FDO_NOTIFICATIONS_PATH: &str = "/org/freedesktop/Notifications";
/// Interface name.
pub const FDO_NOTIFICATIONS_INTERFACE: &str = "org.freedesktop.Notifications";

/// Best-effort register `org.freedesktop.Notifications` on the session bus.
///
/// On non-Linux hosts, or when the session bus is missing / name is taken,
/// logs and returns `Ok(false)` — never fails shell startup.
pub fn try_register_session_bus(center: Arc<RwLock<NotificationCenter>>) -> bool {
    #[cfg(target_os = "linux")]
    {
        match linux::register(center) {
            Ok(()) => {
                tracing::info!(
                    bus = FDO_NOTIFICATIONS_BUS_NAME,
                    path = FDO_NOTIFICATIONS_PATH,
                    "FreeDesktop Notifications daemon registered on session bus"
                );
                true
            }
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    "FreeDesktop Notifications registration skipped"
                );
                false
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = center;
        tracing::debug!("FreeDesktop Notifications registration skipped (non-Linux host)");
        false
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use parking_lot::Mutex;
    use std::sync::Mutex as StdMutex;
    use zbus::blocking::connection::Builder as ConnectionBuilder;
    use zbus::blocking::Connection;
    use zbus::interface;
    use zbus::zvariant::OwnedValue;

    /// Keeps the session bus connection alive for process lifetime.
    static REGISTRATION: StdMutex<Option<FdoRegistration>> = StdMutex::new(None);

    struct FdoRegistration {
        _connection: Connection,
    }

    struct FdoNotificationsIface {
        daemon: Arc<Mutex<NotificationDaemon>>,
    }

    #[interface(name = "org.freedesktop.Notifications")]
    impl FdoNotificationsIface {
        /// FreeDesktop Notifications.Notify
        #[allow(clippy::too_many_arguments)]
        fn notify(
            &self,
            app_name: &str,
            replaces_id: u32,
            app_icon: &str,
            summary: &str,
            body: &str,
            _actions: Vec<String>,
            hints: HashMap<String, OwnedValue>,
            expire_timeout: i32,
        ) -> u32 {
            let urgency = extract_urgency(&hints);
            self.daemon.lock().notify_fields(
                app_name,
                replaces_id,
                app_icon,
                summary,
                body,
                urgency,
                expire_timeout,
            )
        }

        fn close_notification(&self, id: u32) {
            let _ = self.daemon.lock().close(id);
        }

        fn get_capabilities(&self) -> Vec<String> {
            self.daemon
                .lock()
                .get_capabilities()
                .into_iter()
                .map(|s| s.to_string())
                .collect()
        }

        fn get_server_information(&self) -> (String, String, String, String) {
            let info = self.daemon.lock().get_server_information();
            (info.name, info.vendor, info.version, info.spec_version)
        }
    }

    fn extract_urgency(hints: &HashMap<String, OwnedValue>) -> Urgency {
        let Some(value) = hints.get("urgency") else {
            return Urgency::Normal;
        };
        if let Ok(v) = u8::try_from(value) {
            return Urgency::from_u8(v);
        }
        if let Ok(v) = i32::try_from(value) {
            if (0..=255).contains(&v) {
                return Urgency::from_u8(v as u8);
            }
        }
        if let Ok(s) = <&str>::try_from(value) {
            return Urgency::parse(s).unwrap_or(Urgency::Normal);
        }
        Urgency::Normal
    }

    pub(super) fn register(
        center: Arc<RwLock<NotificationCenter>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // If already registered in this process, do not claim the name again.
        if let Ok(guard) = REGISTRATION.lock() {
            if guard.is_some() {
                return Ok(());
            }
        }

        let daemon = Arc::new(Mutex::new(NotificationDaemon::with_notification_center(
            center,
        )));
        let iface = FdoNotificationsIface { daemon };

        let conn = ConnectionBuilder::session()?
            .name(FDO_NOTIFICATIONS_BUS_NAME)?
            .serve_at(FDO_NOTIFICATIONS_PATH, iface)?
            .build()?;

        if let Ok(mut guard) = REGISTRATION.lock() {
            *guard = Some(FdoRegistration { _connection: conn });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_allocation_is_monotonic_nonzero() {
        let mut state = NotificationServerState::new();
        let a = state.allocate_id();
        let b = state.allocate_id();
        let c = state.allocate_id();
        assert_eq!((a, b, c), (1, 2, 3));
        assert_ne!(a, 0);
    }

    #[test]
    fn notify_assigns_ids_and_stores_payload() {
        let mut state = NotificationServerState::new();
        let id = state.notify(NotificationPayload::new("app", "Hello", "World"));
        assert_eq!(id, 1);
        let stored = state.get(id).expect("stored");
        assert_eq!(stored.app_name, "app");
        assert_eq!(stored.summary, "Hello");
        assert_eq!(stored.body, "World");
        assert_eq!(stored.urgency, Urgency::Normal);
        assert_eq!(state.len(), 1);
    }

    #[test]
    fn notify_replaces_existing_id() {
        let mut state = NotificationServerState::new();
        let id = state.notify(NotificationPayload::new("app", "One", "A"));
        let id2 = state.notify(
            NotificationPayload::new("app", "Two", "B")
                .with_replaces_id(id)
                .with_urgency(Urgency::Critical),
        );
        assert_eq!(id, id2);
        assert_eq!(state.len(), 1);
        let stored = state.get(id).unwrap();
        assert_eq!(stored.summary, "Two");
        assert_eq!(stored.urgency, Urgency::Critical);
    }

    #[test]
    fn notify_unknown_replaces_id_allocates_new() {
        let mut state = NotificationServerState::new();
        let id = state.notify(NotificationPayload::new("app", "X", "Y").with_replaces_id(99));
        assert_eq!(id, 1);
        assert!(state.get(99).is_none());
    }

    #[test]
    fn close_removes_notification() {
        let mut state = NotificationServerState::new();
        let id = state.notify(NotificationPayload::new("a", "s", "b"));
        assert!(state.close(id));
        assert!(!state.close(id));
        assert!(state.is_empty());
    }

    #[test]
    fn urgency_parse_and_byte_roundtrip() {
        assert_eq!(Urgency::parse("low"), Some(Urgency::Low));
        assert_eq!(Urgency::parse("NORMAL"), Some(Urgency::Normal));
        assert_eq!(Urgency::parse("critical"), Some(Urgency::Critical));
        assert_eq!(Urgency::parse("0"), Some(Urgency::Low));
        assert_eq!(Urgency::parse("2"), Some(Urgency::Critical));
        assert_eq!(Urgency::parse("nope"), None);
        assert_eq!(Urgency::from_u8(0), Urgency::Low);
        assert_eq!(Urgency::from_u8(1), Urgency::Normal);
        assert_eq!(Urgency::from_u8(2), Urgency::Critical);
        assert_eq!(Urgency::from_u8(9), Urgency::Normal);
        assert_eq!(Urgency::Critical.as_u8(), 2);
        assert_eq!(Urgency::Low.to_priority(), NotificationPriority::Low);
        assert_eq!(
            Urgency::Critical.to_priority(),
            NotificationPriority::Critical
        );
    }

    #[test]
    fn urgency_from_hint_bytes_defaults() {
        assert_eq!(urgency_from_hint_bytes(None), Urgency::Normal);
        assert_eq!(urgency_from_hint_bytes(Some(2)), Urgency::Critical);
    }

    #[test]
    fn payload_builder_sets_fields() {
        let p = NotificationPayload::new("mail", "New mail", "Inbox")
            .with_replaces_id(3)
            .with_urgency(Urgency::Low)
            .with_app_icon("mail")
            .with_expire_timeout_ms(5000);
        assert_eq!(p.app_name, "mail");
        assert_eq!(p.replaces_id, 3);
        assert_eq!(p.urgency, Urgency::Low);
        assert_eq!(p.app_icon, "mail");
        assert_eq!(p.expire_timeout_ms, 5000);
    }

    #[test]
    fn get_capabilities_and_server_info() {
        let state = NotificationServerState::new();
        let caps = state.get_capabilities();
        assert!(caps.contains(&"body"));
        let info = state.get_server_information();
        assert_eq!(info.name, "SLOPOS-I");
        assert_eq!(info.spec_version, "1.2");
    }

    #[test]
    fn notify_send_style_parses_urgency_and_text() {
        let parsed = NotifySendStyle::parse(&["-u", "critical", "Battery", "Low power"]);
        assert_eq!(parsed.urgency, Urgency::Critical);
        assert_eq!(parsed.summary, "Battery");
        assert_eq!(parsed.body, "Low power");
        let payload = parsed.into_payload();
        assert_eq!(payload.urgency, Urgency::Critical);
        assert_eq!(payload.summary, "Battery");
    }

    #[test]
    fn notify_send_style_app_name_and_icon_skip() {
        let parsed = NotifySendStyle::parse(&[
            "-a",
            "Terminal",
            "-i",
            "utilities-terminal",
            "--urgency",
            "low",
            "Done",
            "Build finished",
        ]);
        assert_eq!(parsed.app_name, "Terminal");
        assert_eq!(parsed.urgency, Urgency::Low);
        assert_eq!(parsed.summary, "Done");
        assert_eq!(parsed.body, "Build finished");
    }

    #[test]
    fn daemon_mirrors_notify_and_close_to_center() {
        let center = Arc::new(RwLock::new(NotificationCenter::new()));
        let mut daemon = NotificationDaemon::with_notification_center(center.clone());
        let id = daemon.notify(
            NotificationPayload::new("com.test", "Title", "Body").with_urgency(Urgency::Critical),
        );
        assert_eq!(id, 1);
        assert_eq!(center.read().visible().len(), 1);
        assert_eq!(center.read().visible()[0].title, "Title");
        assert_eq!(
            center.read().visible()[0].priority,
            NotificationPriority::Critical
        );

        assert!(daemon.close(id));
        assert!(center.read().visible().is_empty());
    }

    #[test]
    fn daemon_replace_dismisses_prior_center_banner() {
        let center = Arc::new(RwLock::new(NotificationCenter::new()));
        let mut daemon = NotificationDaemon::with_notification_center(center.clone());
        let id = daemon.notify(NotificationPayload::new("app", "First", "A"));
        let id2 =
            daemon.notify(NotificationPayload::new("app", "Second", "B").with_replaces_id(id));
        assert_eq!(id, id2);
        let center_guard = center.read();
        let visible = center_guard.visible();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].title, "Second");
    }
}
