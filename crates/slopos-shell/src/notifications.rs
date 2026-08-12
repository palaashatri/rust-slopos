//! SLOPOS local notification presentation.
//!
//! This module currently presents notifications created by SLOPOS components.
//! It does not claim org.freedesktop.Notifications ownership until the D-Bus
//! interface is implemented and tested.

use gtk::prelude::*;
use gtk::{
    Align, Box as GtkBox, Button, Image, Label, Orientation, Window, WindowPosition, WindowType,
};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

static NOTIFICATION_ID: AtomicU32 = AtomicU32::new(1);

pub struct NotificationServer;

impl NotificationServer {
    pub fn start() {
        log::info!("Initialized SLOPOS local notification presenter");
    }

    pub fn show_toast(summary: &str, body: &str, icon: &str) {
        let id = NOTIFICATION_ID.fetch_add(1, Ordering::SeqCst);
        let summary = summary.to_string();
        let body = body.to_string();
        let icon = icon.to_string();

        glib::idle_add_local(move || {
            let window = Window::new(WindowType::Toplevel);
            window.set_title("SLOPOS Notification");
            let (screen_width, screen_height) = screen_geometry();
            let width = 340;
            let height = 116;
            let stack_index = id.saturating_sub(1) % 4;
            let y = 36 + (stack_index as i32 * 124);
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
            let icon_name = if icon.is_empty() {
                "dialog-information-symbolic"
            } else {
                &icon
            };
            content.pack_start(
                &Image::from_icon_name(Some(icon_name), gtk::IconSize::Dialog),
                false,
                false,
                0,
            );

            let text = GtkBox::new(Orientation::Vertical, 2);
            let title = Label::new(Some(&summary));
            title
                .style_context()
                .add_class("slopos-notification-title");
            title.set_xalign(0.0);
            title.set_ellipsize(pango::EllipsizeMode::End);
            text.pack_start(&title, false, false, 0);

            if !body.is_empty() {
                let message = Label::new(Some(&body));
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
            dismiss
                .style_context()
                .add_class("slopos-compact-button");
            dismiss.set_halign(Align::End);
            dismiss.set_tooltip_text(Some("Dismiss this notification"));
            let close_target = window.clone();
            dismiss.connect_clicked(move |_| close_target.close());
            main_box.pack_start(&dismiss, false, false, 0);

            window.add(&main_box);
            window.show_all();

            let timeout_target = window.clone();
            glib::timeout_add_seconds_local(6, move || {
                timeout_target.close();
                glib::ControlFlow::Break
            });
            glib::ControlFlow::Break
        });
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
