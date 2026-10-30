//! The desktop's notification server (`org.freedesktop.Notifications`,
//! Desktop Notifications Specification 1.2) and the toast queue the bar
//! shows below its right end.
//!
//! The D-Bus methods run on zbus's thread and only queue messages for the
//! bar; the bar owns the toasts, their expiry timers and the
//! NotificationClosed/ActionInvoked signals.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use zbus::blocking::Connection;
use zbus::zvariant::OwnedValue;
use zbus::SignalContext;

pub const NAME: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";
/// Toasts shown at once; older ones wait in the queue.
pub const VISIBLE: usize = 3;
/// Default display time when the sender leaves it to the server.
pub const DEFAULT_TIMEOUT_MS: u32 = 6_000;

/// Why a notification closed (spec values).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    Expired = 1,
    Dismissed = 2,
    Closed = 3,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    pub id: u32,
    pub app: String,
    pub summary: String,
    pub body: String,
    /// (key, label) pairs; "default" is invoked by clicking the toast.
    pub actions: Vec<(String, String)>,
    /// `None` never expires (critical urgency or timeout 0).
    pub timeout_ms: Option<u32>,
}

impl Toast {
    pub fn default_action(&self) -> Option<&str> {
        self.actions
            .iter()
            .find(|(key, _)| key == "default")
            .map(|(key, _)| key.as_str())
    }
}

#[derive(Debug)]
pub enum Message {
    Show(Toast),
    Close(u32),
}

/// Strip the small markup subset some senders use; SLOPOS shows plain text.
pub fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_tag = false;
    for c in text.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

/// Build a toast from a Notify call's arguments.
pub fn toast(
    id: u32,
    app: &str,
    summary: &str,
    body: &str,
    actions: &[String],
    critical: bool,
    expire_timeout: i32,
) -> Toast {
    Toast {
        id,
        app: plain(app),
        summary: plain(summary),
        body: plain(body),
        actions: actions
            .as_chunks::<2>()
            .0
            .iter()
            .map(|[key, label]| (key.clone(), label.clone()))
            .collect(),
        timeout_ms: match (critical, expire_timeout) {
            (true, _) | (_, 0) => None,
            (_, t) if t < 0 => Some(DEFAULT_TIMEOUT_MS),
            (_, t) => Some(t as u32),
        },
    }
}

struct Server {
    sender: Mutex<Sender<Message>>,
    wake: Arc<dyn Fn() + Send + Sync>,
    next_id: AtomicU32,
}

impl Server {
    fn send(&self, message: Message) {
        if let Ok(sender) = self.sender.lock() {
            let _ = sender.send(message);
        }
        (self.wake)();
    }
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Server {
    fn get_capabilities(&self) -> Vec<String> {
        vec!["body".into(), "actions".into(), "persistence".into()]
    }

    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        _app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        expire_timeout: i32,
    ) -> u32 {
        let id = if replaces_id != 0 {
            replaces_id
        } else {
            self.next_id.fetch_add(1, Ordering::Relaxed).max(1)
        };
        let critical = hints
            .get("urgency")
            .and_then(|value| u8::try_from(&**value).ok())
            == Some(2);
        self.send(Message::Show(toast(
            id,
            &app_name,
            &summary,
            &body,
            &actions,
            critical,
            expire_timeout,
        )));
        id
    }

    fn close_notification(&self, id: u32) {
        self.send(Message::Close(id));
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        (
            "SLOPOS".into(),
            "SLOPOS".into(),
            env!("CARGO_PKG_VERSION").into(),
            "1.2".into(),
        )
    }

    #[zbus(signal)]
    async fn notification_closed(
        context: &SignalContext<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn action_invoked(
        context: &SignalContext<'_>,
        id: u32,
        action_key: &str,
    ) -> zbus::Result<()>;
}

/// The running server: owns the bus name until dropped.
pub struct Notifications {
    connection: Connection,
    pub messages: Receiver<Message>,
}

impl Notifications {
    /// Serve the notification name, unless another daemon already does.
    pub fn start(wake: impl Fn() + Send + Sync + 'static) -> Option<Self> {
        let (sender, messages) = mpsc::channel();
        let server = Server {
            sender: Mutex::new(sender),
            wake: Arc::new(wake),
            next_id: AtomicU32::new(1),
        };
        let connection = zbus::blocking::connection::Builder::session()
            .and_then(|builder| builder.name(NAME))
            .and_then(|builder| builder.serve_at(PATH, server))
            .and_then(|builder| builder.build());
        match connection {
            Ok(connection) => Some(Self {
                connection,
                messages,
            }),
            Err(error) => {
                log::info!("not serving notifications: {error}");
                None
            }
        }
    }

    pub fn closed(&self, id: u32, reason: Reason) {
        if let Ok(server) = self.connection.object_server().interface::<_, Server>(PATH) {
            let _ = zbus::block_on(Server::notification_closed(
                server.signal_context(),
                id,
                reason as u32,
            ));
        }
    }

    pub fn invoked(&self, id: u32, action: &str) {
        if let Ok(server) = self.connection.object_server().interface::<_, Server>(PATH) {
            let _ = zbus::block_on(Server::action_invoked(server.signal_context(), id, action));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notify_arguments_become_plain_toasts() {
        let toast = toast(
            4,
            "Mail",
            "<b>New</b> message",
            "From A &amp; B",
            &[
                "default".into(),
                "Open".into(),
                "later".into(),
                "Later".into(),
            ],
            false,
            -1,
        );
        assert_eq!(toast.summary, "New message");
        assert_eq!(toast.body, "From A & B");
        assert_eq!(toast.default_action(), Some("default"));
        assert_eq!(toast.actions.len(), 2);
        assert_eq!(toast.timeout_ms, Some(DEFAULT_TIMEOUT_MS));
        let critical = super::toast(5, "", "Battery low", "", &[], true, 3000);
        assert_eq!(critical.timeout_ms, None);
        assert_eq!(super::toast(6, "", "", "", &[], false, 0).timeout_ms, None);
        assert_eq!(
            super::toast(7, "", "", "", &[], false, 1500).timeout_ms,
            Some(1500)
        );
    }
}
