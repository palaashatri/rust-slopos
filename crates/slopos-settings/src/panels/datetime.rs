//! Native Date & Time Settings Panel.

use crate::providers::availability::command_exists;
use gtk::atk::prelude::AtkObjectExt;
use gtk::prelude::*;
use gtk::{
    Box as GtkBox, CheckButton, ComboBoxText, Dialog, DialogFlags, Label, Orientation,
    ResponseType, Window,
};
use std::process::Command;

pub fn show_datetime_dialog(parent: &Window) {
    let dialog = Dialog::with_buttons(
        Some("Date & Time"),
        Some(parent),
        DialogFlags::MODAL | DialogFlags::DESTROY_WITH_PARENT,
        &[
            ("Cancel", ResponseType::Cancel),
            ("Apply", ResponseType::Accept),
        ],
    );
    dialog.set_default_response(ResponseType::Accept);
    set_accessible_name(&dialog, "SLOPOS date and time settings");

    let content = dialog.content_area();
    content.set_spacing(10);
    content.set_margin_start(14);
    content.set_margin_end(14);
    content.set_margin_top(12);
    content.set_margin_bottom(12);

    let current = if let Ok(now) = glib::DateTime::now_local() {
        now.format("%A, %B %e, %Y — %H:%M:%S (%Z)")
            .map(|g| g.to_string())
            .unwrap_or_else(|_| "Current system time unavailable".to_string())
    } else {
        "Current system time unavailable".to_string()
    };
    let current_label = Label::new(Some(current.trim()));
    current_label.set_xalign(0.0);
    current_label
        .style_context()
        .add_class("slopos-control-title");
    content.pack_start(&current_label, false, false, 0);

    let timezone_row = GtkBox::new(Orientation::Horizontal, 8);
    timezone_row.pack_start(&Label::new(Some("Timezone:")), false, false, 0);
    let timezone = ComboBoxText::new();
    for (id, label) in [
        ("UTC", "UTC"),
        ("America/New_York", "America/New York"),
        ("America/Chicago", "America/Chicago"),
        ("America/Denver", "America/Denver"),
        ("America/Los_Angeles", "America/Los Angeles"),
        ("Europe/London", "Europe/London"),
        ("Europe/Paris", "Europe/Paris"),
        ("Europe/Berlin", "Europe/Berlin"),
        ("Asia/Kolkata", "Asia/Kolkata"),
        ("Asia/Tokyo", "Asia/Tokyo"),
        ("Australia/Sydney", "Australia/Sydney"),
    ] {
        timezone.append(Some(id), label);
    }
    timezone.set_active_id(Some("UTC"));
    timezone_row.pack_start(&timezone, true, true, 0);
    content.pack_start(&timezone_row, false, false, 0);

    let ntp = CheckButton::with_label("Set time automatically using network time");
    ntp.set_active(true);
    content.pack_start(&ntp, false, false, 0);

    let note = Label::new(Some(
        "Changing system time may require administrator authorization from your Linux distribution.",
    ));
    note.set_xalign(0.0);
    note.set_line_wrap(true);
    note.style_context().add_class("slopos-secondary-text");
    content.pack_start(&note, false, false, 0);

    dialog.show_all();
    if dialog.run() == ResponseType::Accept {
        if !command_exists("timedatectl") {
            log::warn!("timedatectl is unavailable; date/time changes were not applied");
        } else {
            if let Some(id) = timezone.active_id() {
                let _ = Command::new("timedatectl")
                    .args(["set-timezone", id.as_str()])
                    .status();
            }
            let _ = Command::new("timedatectl")
                .args(["set-ntp", if ntp.is_active() { "true" } else { "false" }])
                .status();
        }
    }
    dialog.close();
}

fn set_accessible_name<W>(widget: &W, name: &str)
where
    W: IsA<gtk::Widget>,
{
    let Some(accessible) = widget.accessible() else {
        return;
    };
    let Ok(accessible) = accessible.downcast::<gtk::atk::Object>() else {
        return;
    };
    accessible.set_name(name);
}
