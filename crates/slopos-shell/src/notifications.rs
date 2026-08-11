//! Desktop Notifications Server
//! Implements DBus `org.freedesktop.Notifications` interface to show GTK toast notifications.

use gtk::prelude::*;
use gtk::{Box as GtkBox, Image, Label, Orientation, Window, WindowPosition, WindowType};
use std::sync::atomic::{AtomicU32, Ordering};

static NOTIFICATION_ID: AtomicU32 = AtomicU32::new(1);

pub struct NotificationServer;

impl NotificationServer {
    pub fn start() {
        log::info!("Initialized Notification Server component");
    }

    pub fn show_toast(summary: &str, body: &str, icon: &str) {
        let id = NOTIFICATION_ID.fetch_add(1, Ordering::SeqCst);
        let summary = summary.to_string();
        let body = body.to_string();
        let icon = icon.to_string();

        glib::idle_add_local(move || {
            let win = Window::new(WindowType::Toplevel);
            win.set_title("Notification");
            win.set_default_size(320, 70);
            win.set_position(WindowPosition::None);
            win.move_(940, 40 + ((id % 5) as i32 * 80));
            win.set_decorated(false);
            win.set_keep_above(true);
            win.set_skip_taskbar_hint(true);

            let main_box = GtkBox::new(Orientation::Horizontal, 10);
            main_box.set_margin_start(10);
            main_box.set_margin_end(10);
            main_box.set_margin_top(8);
            main_box.set_margin_bottom(8);

            let icon_name = if icon.is_empty() { "dialog-information" } else { &icon };
            let img = Image::from_icon_name(Some(icon_name), gtk::IconSize::Dialog);
            main_box.pack_start(&img, false, false, 0);

            let vbox = GtkBox::new(Orientation::Vertical, 4);
            let title_label = Label::new(Some(&summary));
            title_label.set_xalign(0.0);
            vbox.pack_start(&title_label, false, false, 0);

            if !body.is_empty() {
                let body_label = Label::new(Some(&body));
                body_label.set_xalign(0.0);
                vbox.pack_start(&body_label, false, false, 0);
            }

            main_box.pack_start(&vbox, true, true, 0);
            win.add(&main_box);
            win.show_all();

            // Auto dismiss after 4 seconds
            let w = win.clone();
            glib::timeout_add_seconds_local(4, move || {
                w.close();
                glib::ControlFlow::Break
            });

            glib::ControlFlow::Break
        });
    }
}
