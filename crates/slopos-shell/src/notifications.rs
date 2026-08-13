//! SLOPOS notification service.
//!
//! SLOPOS owns `org.freedesktop.Notifications` when no other daemon already
//! owns it, while retaining the same presenter for shell-local notifications.

use gdk_pixbuf::{InterpType, Pixbuf};
use gtk::prelude::*;
use gtk::{
    Align, Box as GtkBox, Button, Image, Label, Orientation, Window, WindowPosition, WindowType,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;
use zbus::zvariant::OwnedValue;

const NOTIFICATION_PATH: &str = "/org/freedesktop/Notifications";
const NOTIFICATION_INTERFACE: &str = "org.freedesktop.Notifications";
const NOTIFICATION_BUS_NAME: &str = "org.freedesktop.Notifications";
const REASON_EXPIRED: u32 = 1;
const REASON_DISMISSED: u32 = 2;
const REASON_CLOSED: u32 = 3;

static NEXT_NOTIFICATION_ID: AtomicU32 = AtomicU32::new(1);
static UI_SENDER: OnceLock<Sender<UiCommand>> = OnceLock::new();

enum UiCommand {
    Show {
        id: u32,
        summary: String,
        body: String,
        icon: String,
        expire_timeout_ms: i32,
    },
    Close {
        id: u32,
        reason: u32,
    },
    DbusConnection(zbus::blocking::Connection),
}

#[derive(Clone)]
struct FreedesktopNotifications {
    sender: Sender<UiCommand>,
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl FreedesktopNotifications {
    fn get_capabilities(&self) -> Vec<String> {
        vec!["body".to_string()]
    }

    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        _app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        _actions: Vec<String>,
        _hints: HashMap<String, OwnedValue>,
        expire_timeout: i32,
    ) -> u32 {
        let id = if replaces_id == 0 {
            next_notification_id()
        } else {
            replaces_id
        };
        let _ = self.sender.send(UiCommand::Show {
            id,
            summary: summary.to_string(),
            body: body.to_string(),
            icon: app_icon.to_string(),
            expire_timeout_ms: expire_timeout,
        });
        id
    }

    fn close_notification(&self, id: u32) {
        let _ = self.sender.send(UiCommand::Close {
            id,
            reason: REASON_CLOSED,
        });
    }

    #[zbus(out_args("name", "vendor", "version", "spec_version"))]
    fn get_server_information(&self) -> (String, String, String, String) {
        (
            "SLOPOS Notifications".to_string(),
            "SLOPOS-I".to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
            "1.2".to_string(),
        )
    }
}

pub struct NotificationServer;

impl NotificationServer {
    pub fn start() {
        let (sender, receiver) = mpsc::channel();
        if UI_SENDER.set(sender.clone()).is_err() {
            log::warn!("Notification presenter was already initialized");
            return;
        }

        install_ui_pump(receiver);
        spawn_dbus_service(sender);
        log::info!("Initialized SLOPOS notification presenter");
    }

    pub fn show_toast(summary: &str, body: &str, icon: &str) {
        let Some(sender) = UI_SENDER.get() else {
            log::warn!("Notification requested before presenter initialization: {summary}");
            return;
        };
        let _ = sender.send(UiCommand::Show {
            id: next_notification_id(),
            summary: summary.to_string(),
            body: body.to_string(),
            icon: icon.to_string(),
            expire_timeout_ms: 6000,
        });
    }
}

fn spawn_dbus_service(sender: Sender<UiCommand>) {
    thread::spawn(move || {
        let service = FreedesktopNotifications {
            sender: sender.clone(),
        };
        let connection = match zbus::blocking::connection::Builder::session()
            .and_then(|builder| builder.name(NOTIFICATION_BUS_NAME))
            .and_then(|builder| builder.serve_at(NOTIFICATION_PATH, service))
            .and_then(|builder| builder.build())
        {
            Ok(connection) => connection,
            Err(error) => {
                log::warn!(
                    "Could not own {NOTIFICATION_BUS_NAME}; local notifications remain available: {error}"
                );
                return;
            }
        };

        log::info!("SLOPOS owns {NOTIFICATION_BUS_NAME}");
        let _ = sender.send(UiCommand::DbusConnection(connection));
        // zbus runs its internal executor for the lifetime of the connection.
        // Keep this dedicated server thread parked until the process exits.
        loop {
            thread::park();
        }
    });
}

fn install_ui_pump(receiver: Receiver<UiCommand>) {
    let windows: Rc<RefCell<HashMap<u32, Window>>> = Rc::new(RefCell::new(HashMap::new()));
    let dbus_connection: Rc<RefCell<Option<zbus::blocking::Connection>>> =
        Rc::new(RefCell::new(None));

    glib::timeout_add_local(Duration::from_millis(25), move || {
        while let Ok(command) = receiver.try_recv() {
            match command {
                UiCommand::Show {
                    id,
                    summary,
                    body,
                    icon,
                    expire_timeout_ms,
                } => show_window(
                    id,
                    &summary,
                    &body,
                    &icon,
                    expire_timeout_ms,
                    windows.clone(),
                    dbus_connection.clone(),
                ),
                UiCommand::Close { id, reason } => {
                    if let Some(window) = windows.borrow_mut().remove(&id) {
                        window.close();
                        emit_closed(dbus_connection.borrow().as_ref(), id, reason);
                    }
                }
                UiCommand::DbusConnection(connection) => {
                    *dbus_connection.borrow_mut() = Some(connection);
                }
            }
        }
        glib::ControlFlow::Continue
    });
}

#[allow(clippy::too_many_arguments)]
fn show_window(
    id: u32,
    summary: &str,
    body: &str,
    icon: &str,
    expire_timeout_ms: i32,
    windows: Rc<RefCell<HashMap<u32, Window>>>,
    dbus_connection: Rc<RefCell<Option<zbus::blocking::Connection>>>,
) {
    // Replacements update the visual notification without emitting a closed
    // signal for the superseded presentation.
    if let Some(previous) = windows.borrow_mut().remove(&id) {
        previous.close();
    }

    let window = Window::new(WindowType::Toplevel);
    window.set_title(&format!("SLOPOS Notification {id}"));
    let (screen_width, screen_height) = screen_geometry();
    let width = 340;
    let height = 116;
    let stack_index = windows.borrow().len().min(3) as i32;
    let y = 36 + (stack_index * 124);
    window.set_default_size(width, height);
    window.set_position(WindowPosition::None);
    window.move_(
        (screen_width - width - 12).max(0),
        y.min((screen_height - height - 12).max(36)),
    );
    window.set_decorated(false);
    window.set_keep_above(true);
    window.set_skip_taskbar_hint(true);
    window.set_skip_pager_hint(true);

    let main_box = GtkBox::new(Orientation::Vertical, 6);
    main_box.style_context().add_class("slopos-notification");

    let content = GtkBox::new(Orientation::Horizontal, 9);
    if let Some(mark) = icon.is_empty().then(load_slopos_mark).flatten() {
        content.pack_start(&mark, false, false, 0);
    } else {
        let icon_name = if icon.is_empty() {
            "dialog-information-symbolic"
        } else {
            icon
        };
        content.pack_start(
            &Image::from_icon_name(Some(icon_name), gtk::IconSize::Dialog),
            false,
            false,
            0,
        );
    }

    let text = GtkBox::new(Orientation::Vertical, 2);
    let title = Label::new(Some(summary));
    title.style_context().add_class("slopos-notification-title");
    title.set_xalign(0.0);
    title.set_ellipsize(pango::EllipsizeMode::End);
    text.pack_start(&title, false, false, 0);

    if !body.is_empty() {
        let message = Label::new(Some(body));
        message.set_xalign(0.0);
        message.set_line_wrap(true);
        message.set_line_wrap_mode(pango::WrapMode::WordChar);
        message.set_ellipsize(pango::EllipsizeMode::End);
        message.set_lines(3);
        message.set_max_width_chars(43);
        text.pack_start(&message, false, false, 0);
    }
    content.pack_start(&text, true, true, 0);
    main_box.pack_start(&content, true, true, 0);

    let dismiss = Button::with_label("Dismiss");
    dismiss.style_context().add_class("slopos-compact-button");
    dismiss.set_halign(Align::End);
    dismiss.set_tooltip_text(Some("Dismiss this notification"));
    let dismiss_windows = windows.clone();
    let dismiss_connection = dbus_connection.clone();
    let close_target = window.clone();
    dismiss.connect_clicked(move |_| {
        dismiss_windows.borrow_mut().remove(&id);
        close_target.close();
        emit_closed(dismiss_connection.borrow().as_ref(), id, REASON_DISMISSED);
    });
    main_box.pack_start(&dismiss, false, false, 0);

    window.add(&main_box);
    window.show_all();
    windows.borrow_mut().insert(id, window.clone());

    let timeout_ms = normalized_timeout(expire_timeout_ms);
    if timeout_ms > 0 {
        let timeout_windows = windows;
        let timeout_connection = dbus_connection;
        glib::timeout_add_local(Duration::from_millis(timeout_ms), move || {
            if let Some(active) = timeout_windows.borrow_mut().remove(&id) {
                active.close();
                emit_closed(timeout_connection.borrow().as_ref(), id, REASON_EXPIRED);
            }
            glib::ControlFlow::Break
        });
    }
}

fn normalized_timeout(requested_ms: i32) -> u64 {
    match requested_ms {
        value if value < 0 => 6000,
        0 => 0,
        value => value.clamp(1000, 60_000) as u64,
    }
}

fn load_slopos_mark() -> Option<Image> {
    let mut candidates = Vec::new();
    if let Ok(share_dir) = env::var("SLOPOS_SHARE_DIR") {
        candidates.push(PathBuf::from(share_dir).join("slopos-i/slopos-logo.png"));
    }
    candidates.extend([
        PathBuf::from("assets/slopos-logo.png"),
        PathBuf::from("/usr/local/share/slopos-i/slopos-logo.png"),
        PathBuf::from("/usr/share/slopos-i/slopos-logo.png"),
    ]);
    candidates.into_iter().find_map(|path| {
        if !Path::new(&path).is_file() {
            return None;
        }
        let pixbuf = Pixbuf::from_file(&path).ok()?;
        let mark = if pixbuf.width() >= 512 && pixbuf.height() >= 512 {
            let crop = (pixbuf.width().min(pixbuf.height()) / 4).max(1);
            let x = (pixbuf.width() - crop) / 2;
            let y = ((pixbuf.height() * 3) / 10).min(pixbuf.height() - crop);
            pixbuf.new_subpixbuf(x, y, crop, crop)
        } else {
            pixbuf
        };
        let scaled = mark.scale_simple(28, 28, InterpType::Bilinear)?;
        Some(Image::from_pixbuf(Some(&scaled)))
    })
}

fn emit_closed(connection: Option<&zbus::blocking::Connection>, id: u32, reason: u32) {
    let Some(connection) = connection else {
        return;
    };
    if let Err(error) = connection.emit_signal(
        None::<&str>,
        NOTIFICATION_PATH,
        NOTIFICATION_INTERFACE,
        "NotificationClosed",
        &(id, reason),
    ) {
        log::warn!("Failed to emit NotificationClosed for {id}: {error}");
    }
}

fn next_notification_id() -> u32 {
    loop {
        let id = NEXT_NOTIFICATION_ID.fetch_add(1, Ordering::SeqCst);
        if id != 0 {
            return id;
        }
    }
}

fn screen_geometry() -> (i32, i32) {
    let Ok(output) = Command::new("xrandr").arg("--current").output() else {
        return (1280, 800);
    };
    if !output.status.success() {
        return (1280, 800);
    }

    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        let Some(after_current) = line.split("current ").nth(1) else {
            continue;
        };
        let Some(dimensions) = after_current.split(',').next() else {
            continue;
        };
        let mut parts = dimensions.split('x').map(str::trim);
        let (Some(width), Some(height)) = (parts.next(), parts.next()) else {
            continue;
        };
        if let (Ok(width), Ok(height)) = (width.parse::<i32>(), height.parse::<i32>()) {
            return (width, height);
        }
    }
    (1280, 800)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_contract_matches_freedesktop_semantics() {
        assert_eq!(normalized_timeout(-1), 6000);
        assert_eq!(normalized_timeout(0), 0);
        assert_eq!(normalized_timeout(10), 1000);
        assert_eq!(normalized_timeout(2500), 2500);
        assert_eq!(normalized_timeout(120_000), 60_000);
    }
}
