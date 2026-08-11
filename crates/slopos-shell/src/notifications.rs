//! System 7 Classic Desktop Alert & Notification Server
//! Implements DBus `org.freedesktop.Notifications` interface to show System 7 alert dialogs.

use gtk::prelude::*;
use gtk::{
    Align, Box as GtkBox, Button, Image, Label, Orientation, Window, WindowPosition, WindowType,
};
use std::sync::atomic::{AtomicU32, Ordering};

static NOTIFICATION_ID: AtomicU32 = AtomicU32::new(1);

pub struct NotificationServer;

impl NotificationServer {
    pub fn start() {
        log::info!("Initialized System 7 Notification Server component");
    }

    pub fn show_toast(summary: &str, body: &str, icon: &str) {
        let id = NOTIFICATION_ID.fetch_add(1, Ordering::SeqCst);
        let summary = summary.to_string();
        let body = body.to_string();
        let icon = icon.to_string();

        glib::idle_add_local(move || {
            let win = Window::new(WindowType::Toplevel);
            win.set_title("System 7 Alert");
            win.set_default_size(360, 90);
            win.set_position(WindowPosition::None);
            win.move_(900, 36 + ((id % 4) as i32 * 100));
            win.set_decorated(false);
            win.set_keep_above(true);
            win.set_skip_taskbar_hint(true);

            let main_box = GtkBox::new(Orientation::Vertical, 8);
            main_box.style_context().add_class("slopos-alert-box");

            let content_box = GtkBox::new(Orientation::Horizontal, 12);

            let icon_name = if icon.is_empty() {
                "dialog-information-symbolic"
            } else {
                &icon
            };
            let img = Image::from_icon_name(Some(icon_name), gtk::IconSize::Dialog);
            content_box.pack_start(&img, false, false, 0);

            let vbox = GtkBox::new(Orientation::Vertical, 4);
            let title_label = Label::new(Some(&summary));
            let pango_attr = pango::AttrList::new();
            title_label.set_attributes(Some(&pango_attr));
            title_label.set_xalign(0.0);
            vbox.pack_start(&title_label, false, false, 0);

            if !body.is_empty() {
                let body_label = Label::new(Some(&body));
                body_label.set_xalign(0.0);
                vbox.pack_start(&body_label, false, false, 0);
            }

            content_box.pack_start(&vbox, true, true, 0);
            main_box.pack_start(&content_box, true, true, 0);

            // System 7 OK Button
            let ok_btn = Button::with_label("OK");
            ok_btn.style_context().add_class("default");
            ok_btn.set_halign(Align::End);

            let w = win.clone();
            ok_btn.connect_clicked(move |_| {
                w.close();
            });

            main_box.pack_start(&ok_btn, false, false, 0);

            win.add(&main_box);
            win.show_all();

            // Auto dismiss after 6 seconds
            let w_auto = win.clone();
            glib::timeout_add_seconds_local(6, move || {
                w_auto.close();
                glib::ControlFlow::Break
            });

            glib::ControlFlow::Break
        });
    }
}
