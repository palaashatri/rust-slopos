//! SLOPOS local notification presentation.
//!
//! This module currently presents notifications created by SLOPOS components.
//! It does not claim org.freedesktop.Notifications ownership until the D-Bus
//! interface is implemented and tested.

use gdk::prelude::*;
use gtk::prelude::*;
use gtk::{Align, Box as GtkBox, Button, Image, Label, Orientation, Window, WindowPosition, WindowType};
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
            let win = Window::new(WindowType::Toplevel);
            win.set_title("SLOPOS Notification");
            let (screen_width, _) = gdk::Screen::default()
                .map(|s| (s.width(), s.height()))
                .unwrap_or((1280, 800));
            let width = 330;
            let height = 96;
            win.set_default_size(width, height);
            win.set_position(WindowPosition::None);
            win.move_((screen_width - width - 12).max(0), 36 + ((id % 4) as i32 * 104));
            win.set_decorated(false);
            win.set_keep_above(true);
            win.set_skip_taskbar_hint(true);
            win.set_skip_pager_hint(true);

            let main_box = GtkBox::new(Orientation::Vertical, 6);
            main_box.style_context().add_class("slopos-notification");

            let content = GtkBox::new(Orientation::Horizontal, 9);
            let icon_name = if icon.is_empty() { "dialog-information-symbolic" } else { &icon };
            content.pack_start(&Image::from_icon_name(Some(icon_name), gtk::IconSize::Dialog), false, false, 0);

            let text = GtkBox::new(Orientation::Vertical, 2);
            let title = Label::new(Some(&summary));
            title.style_context().add_class("slopos-notification-title");
            title.set_xalign(0.0);
            text.pack_start(&title, false, false, 0);

            if !body.is_empty() {
                let message = Label::new(Some(&body));
                message.set_xalign(0.0);
                message.set_line_wrap(true);
                message.set_max_width_chars(42);
                text.pack_start(&message, false, false, 0);
            }
            content.pack_start(&text, true, true, 0);
            main_box.pack_start(&content, true, true, 0);

            let dismiss = Button::with_label("Dismiss");
            dismiss.style_context().add_class("slopos-compact-button");
            dismiss.set_halign(Align::End);
            let close_target = win.clone();
            dismiss.connect_clicked(move |_| close_target.close());
            main_box.pack_start(&dismiss, false, false, 0);

            win.add(&main_box);
            win.show_all();

            let timeout_target = win.clone();
            glib::timeout_add_seconds_local(6, move || {
                timeout_target.close();
                glib::ControlFlow::Break
            });
            glib::ControlFlow::Break
        });
    }
}
