//! Native Date & Time Settings Panel.

use crate::providers::availability::command_exists;
use gtk::atk::prelude::AtkObjectExt;
use gtk::prelude::*;
use gtk::{
    Adjustment, Box as GtkBox, CheckButton, ComboBoxText, Dialog, DialogFlags, Frame, Label,
    Orientation, ResponseType, SpinButton, Window,
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
    dialog.set_default_size(440, 360);
    set_accessible_name(&dialog, "SLOPOS date and time settings");

    let content = dialog.content_area();
    content.set_spacing(10);
    content.set_margin_start(14);
    content.set_margin_end(14);
    content.set_margin_top(12);
    content.set_margin_bottom(12);

    let now = glib::DateTime::now_local().ok();
    let current = if let Some(ref dt) = now {
        dt.format("%A, %B %e, %Y — %H:%M:%S (%Z)")
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

    let ntp = CheckButton::with_label("Set time automatically using network time (NTP)");
    ntp.set_active(true);
    content.pack_start(&ntp, false, false, 0);

    // Manual Date & Time configuration frame
    let manual_frame = Frame::new(Some("Manual Time & Date Adjustments"));
    manual_frame.style_context().add_class("slopos-section");
    let manual_box = GtkBox::new(Orientation::Vertical, 8);
    manual_box.set_margin_start(8);
    manual_box.set_margin_end(8);
    manual_box.set_margin_top(8);
    manual_box.set_margin_bottom(8);

    // Time row: HH : MM : SS
    let time_row = GtkBox::new(Orientation::Horizontal, 6);
    time_row.pack_start(&Label::new(Some("Time (24h):")), false, false, 0);

    let cur_hour = now.as_ref().map(|d| d.hour()).unwrap_or(12) as f64;
    let cur_min = now.as_ref().map(|d| d.minute()).unwrap_or(0) as f64;
    let cur_sec = now.as_ref().map(|d| d.seconds()).unwrap_or(0.0);

    let adj_hour = Adjustment::new(cur_hour, 0.0, 23.0, 1.0, 5.0, 0.0);
    let spin_hour = SpinButton::new(Some(&adj_hour), 1.0, 0);
    spin_hour.set_numeric(true);
    spin_hour.set_width_chars(2);
    set_accessible_name(&spin_hour, "Hour");

    let adj_min = Adjustment::new(cur_min, 0.0, 59.0, 1.0, 5.0, 0.0);
    let spin_min = SpinButton::new(Some(&adj_min), 1.0, 0);
    spin_min.set_numeric(true);
    spin_min.set_width_chars(2);
    set_accessible_name(&spin_min, "Minute");

    let adj_sec = Adjustment::new(cur_sec, 0.0, 59.0, 1.0, 5.0, 0.0);
    let spin_sec = SpinButton::new(Some(&adj_sec), 1.0, 0);
    spin_sec.set_numeric(true);
    spin_sec.set_width_chars(2);
    set_accessible_name(&spin_sec, "Second");

    time_row.pack_start(&spin_hour, false, false, 0);
    time_row.pack_start(&Label::new(Some(":")), false, false, 0);
    time_row.pack_start(&spin_min, false, false, 0);
    time_row.pack_start(&Label::new(Some(":")), false, false, 0);
    time_row.pack_start(&spin_sec, false, false, 0);
    manual_box.pack_start(&time_row, false, false, 0);

    // Date row: YYYY - MM - DD
    let date_row = GtkBox::new(Orientation::Horizontal, 6);
    date_row.pack_start(&Label::new(Some("Date:")), false, false, 0);

    let cur_year = now.as_ref().map(|d| d.year()).unwrap_or(2026) as f64;
    let cur_month = now.as_ref().map(|d| d.month()).unwrap_or(8) as f64;
    let cur_day = now.as_ref().map(|d| d.day_of_month()).unwrap_or(18) as f64;

    let adj_year = Adjustment::new(cur_year, 2000.0, 2099.0, 1.0, 5.0, 0.0);
    let spin_year = SpinButton::new(Some(&adj_year), 1.0, 0);
    spin_year.set_numeric(true);
    spin_year.set_width_chars(4);
    set_accessible_name(&spin_year, "Year");

    let adj_month = Adjustment::new(cur_month, 1.0, 12.0, 1.0, 1.0, 0.0);
    let spin_month = SpinButton::new(Some(&adj_month), 1.0, 0);
    spin_month.set_numeric(true);
    spin_month.set_width_chars(2);
    set_accessible_name(&spin_month, "Month");

    let adj_day = Adjustment::new(cur_day, 1.0, 31.0, 1.0, 5.0, 0.0);
    let spin_day = SpinButton::new(Some(&adj_day), 1.0, 0);
    spin_day.set_numeric(true);
    spin_day.set_width_chars(2);
    set_accessible_name(&spin_day, "Day");

    date_row.pack_start(&Label::new(Some("Year")), false, false, 0);
    date_row.pack_start(&spin_year, false, false, 0);
    date_row.pack_start(&Label::new(Some("Month")), false, false, 0);
    date_row.pack_start(&spin_month, false, false, 0);
    date_row.pack_start(&Label::new(Some("Day")), false, false, 0);
    date_row.pack_start(&spin_day, false, false, 0);
    manual_box.pack_start(&date_row, false, false, 0);

    manual_frame.add(&manual_box);
    content.pack_start(&manual_frame, false, false, 0);

    // Connect NTP checkbox to manual inputs sensitivity
    let manual_box_clone = manual_box.clone();
    manual_box.set_sensitive(false); // Initially NTP is on
    ntp.connect_toggled(move |btn| {
        manual_box_clone.set_sensitive(!btn.is_active());
    });

    let timezone_row = GtkBox::new(Orientation::Horizontal, 8);
    timezone_row.pack_start(&Label::new(Some("Timezone:")), false, false, 0);
    let timezone = ComboBoxText::new();
    for (id, label) in [
        ("UTC", "UTC (Coordinated Universal Time)"),
        ("America/New_York", "America/New York (EST/EDT)"),
        ("America/Chicago", "America/Chicago (CST/CDT)"),
        ("America/Denver", "America/Denver (MST/MDT)"),
        ("America/Los_Angeles", "America/Los Angeles (PST/PDT)"),
        ("Europe/London", "Europe/London (GMT/BST)"),
        ("Europe/Paris", "Europe/Paris (CET/CEST)"),
        ("Europe/Berlin", "Europe/Berlin (CET/CEST)"),
        ("Asia/Kolkata", "Asia/Kolkata (IST)"),
        ("Asia/Tokyo", "Asia/Tokyo (JST)"),
        ("Asia/Singapore", "Asia/Singapore (SGT)"),
        ("Australia/Sydney", "Australia/Sydney (AEST/AEDT)"),
    ] {
        timezone.append(Some(id), label);
    }
    timezone.set_active_id(Some("UTC"));
    timezone_row.pack_start(&timezone, true, true, 0);
    content.pack_start(&timezone_row, false, false, 0);

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
            let is_ntp = ntp.is_active();
            if let Some(id) = timezone.active_id() {
                let _ = Command::new("timedatectl")
                    .args(["set-timezone", id.as_str()])
                    .status();
            }
            if is_ntp {
                let _ = Command::new("timedatectl")
                    .args(["set-ntp", "true"])
                    .status();
            } else {
                let _ = Command::new("timedatectl")
                    .args(["set-ntp", "false"])
                    .status();
                let year = spin_year.value_as_int();
                let month = spin_month.value_as_int();
                let day = spin_day.value_as_int();
                let hour = spin_hour.value_as_int();
                let min = spin_min.value_as_int();
                let sec = spin_sec.value_as_int();
                let time_str = format!("{year:04}-{month:02}-{day:02} {hour:02}:{min:02}:{sec:02}");
                let _ = Command::new("timedatectl")
                    .args(["set-time", &time_str])
                    .status();
            }
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
