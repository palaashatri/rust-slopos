//! Native Date & Time Settings Panel.

use crate::providers::availability::command_exists;
use gtk::atk::prelude::AtkObjectExt;
use gtk::prelude::*;
use gtk::{
    Adjustment, Align, Box as GtkBox, Calendar, CheckButton, ComboBoxText, Dialog, DialogFlags,
    Frame, Image, Label, Orientation, ResponseType, SpinButton, Window,
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
    dialog.set_default_size(520, 480);
    set_accessible_name(&dialog, "SLOPOS date and time settings");

    let content = dialog.content_area();
    content.set_spacing(12);
    content.set_margin_start(16);
    content.set_margin_end(16);
    content.set_margin_top(14);
    content.set_margin_bottom(14);

    let now = glib::DateTime::now_local().ok();

    // 1. Header Card with Live Digital Time Readout
    let header_card = GtkBox::new(Orientation::Horizontal, 12);
    header_card.style_context().add_class("slopos-section");
    header_card.set_margin_bottom(4);

    let clock_icon = Image::from_icon_name(
        Some("preferences-system-time-symbolic"),
        gtk::IconSize::Dialog,
    );
    clock_icon.set_valign(Align::Center);
    header_card.pack_start(&clock_icon, false, false, 4);

    let header_text = GtkBox::new(Orientation::Vertical, 2);
    header_text.set_valign(Align::Center);

    let time_text = if let Some(ref dt) = now {
        dt.format("%H:%M:%S")
            .map(|g| g.to_string())
            .unwrap_or_else(|_| "12:00:00".to_string())
    } else {
        "12:00:00".to_string()
    };
    let live_time_label = Label::new(Some(&time_text));
    live_time_label.set_xalign(0.0);
    live_time_label
        .style_context()
        .add_class("slopos-panel-title");
    header_text.pack_start(&live_time_label, false, false, 0);

    let date_text = if let Some(ref dt) = now {
        dt.format("%A, %B %e, %Y (%Z)")
            .map(|g| g.to_string())
            .unwrap_or_else(|_| "".to_string())
    } else {
        String::new()
    };
    let live_date_label = Label::new(Some(&date_text));
    live_date_label.set_xalign(0.0);
    live_date_label
        .style_context()
        .add_class("slopos-panel-subtitle");
    header_text.pack_start(&live_date_label, false, false, 0);

    header_card.pack_start(&header_text, true, true, 0);
    content.pack_start(&header_card, false, false, 0);

    // 2. Automatic NTP Checkbox
    let ntp_box = GtkBox::new(Orientation::Vertical, 2);
    let ntp = CheckButton::with_label("Set date and time automatically (Network Time / NTP)");
    ntp.set_active(true);
    let ntp_sub = Label::new(Some(
        "Keeps system clock accurate using internet time servers (timedatectl).",
    ));
    ntp_sub.set_xalign(0.0);
    ntp_sub.set_margin_start(24);
    ntp_sub.style_context().add_class("slopos-secondary-text");
    ntp_box.pack_start(&ntp, false, false, 0);
    ntp_box.pack_start(&ntp_sub, false, false, 0);
    content.pack_start(&ntp_box, false, false, 0);

    // 3. Manual Adjustment Section
    let manual_frame = Frame::new(Some("Manual Adjustments"));
    manual_frame.style_context().add_class("slopos-section");
    let manual_box = GtkBox::new(Orientation::Horizontal, 14);
    manual_box.set_margin_start(10);
    manual_box.set_margin_end(10);
    manual_box.set_margin_top(8);
    manual_box.set_margin_bottom(8);

    // Left: Interactive Calendar Widget
    let cal_box = GtkBox::new(Orientation::Vertical, 4);
    let cal_label = Label::new(Some("Calendar Date:"));
    cal_label.set_xalign(0.0);
    cal_label.style_context().add_class("slopos-control-title");
    cal_box.pack_start(&cal_label, false, false, 0);

    let calendar = Calendar::new();
    if let Some(ref dt) = now {
        calendar.select_month(dt.month() as u32 - 1, dt.year() as u32);
        calendar.select_day(dt.day_of_month() as u32);
    }
    set_accessible_name(&calendar, "Date calendar");
    cal_box.pack_start(&calendar, true, true, 0);
    manual_box.pack_start(&cal_box, true, true, 0);

    // Right: Time Adjusters
    let time_box = GtkBox::new(Orientation::Vertical, 10);
    time_box.set_valign(Align::Center);

    let time_sec_label = Label::new(Some("Adjust Time (24h):"));
    time_sec_label.set_xalign(0.0);
    time_sec_label
        .style_context()
        .add_class("slopos-control-title");
    time_box.pack_start(&time_sec_label, false, false, 0);

    let cur_hour = now.as_ref().map(|d| d.hour()).unwrap_or(12) as f64;
    let cur_min = now.as_ref().map(|d| d.minute()).unwrap_or(0) as f64;
    let cur_sec = now.as_ref().map(|d| d.seconds()).unwrap_or(0.0);

    // Hour row
    let h_row = GtkBox::new(Orientation::Horizontal, 8);
    let h_lbl = Label::new(Some("Hour:"));
    h_lbl.set_width_chars(6);
    h_lbl.set_xalign(0.0);
    let adj_hour = Adjustment::new(cur_hour, 0.0, 23.0, 1.0, 1.0, 0.0);
    let spin_hour = SpinButton::new(Some(&adj_hour), 1.0, 0);
    spin_hour.set_numeric(true);
    spin_hour.set_width_chars(4);
    set_accessible_name(&spin_hour, "Hour");
    h_row.pack_start(&h_lbl, false, false, 0);
    h_row.pack_start(&spin_hour, true, true, 0);
    time_box.pack_start(&h_row, false, false, 0);

    // Minute row
    let m_row = GtkBox::new(Orientation::Horizontal, 8);
    let m_lbl = Label::new(Some("Minute:"));
    m_lbl.set_width_chars(6);
    m_lbl.set_xalign(0.0);
    let adj_min = Adjustment::new(cur_min, 0.0, 59.0, 1.0, 5.0, 0.0);
    let spin_min = SpinButton::new(Some(&adj_min), 1.0, 0);
    spin_min.set_numeric(true);
    spin_min.set_width_chars(4);
    set_accessible_name(&spin_min, "Minute");
    m_row.pack_start(&m_lbl, false, false, 0);
    m_row.pack_start(&spin_min, true, true, 0);
    time_box.pack_start(&m_row, false, false, 0);

    // Second row
    let s_row = GtkBox::new(Orientation::Horizontal, 8);
    let s_lbl = Label::new(Some("Second:"));
    s_lbl.set_width_chars(6);
    s_lbl.set_xalign(0.0);
    let adj_sec = Adjustment::new(cur_sec, 0.0, 59.0, 1.0, 5.0, 0.0);
    let spin_sec = SpinButton::new(Some(&adj_sec), 1.0, 0);
    spin_sec.set_numeric(true);
    spin_sec.set_width_chars(4);
    set_accessible_name(&spin_sec, "Second");
    s_row.pack_start(&s_lbl, false, false, 0);
    s_row.pack_start(&spin_sec, true, true, 0);
    time_box.pack_start(&s_row, false, false, 0);

    manual_box.pack_start(&time_box, false, false, 4);

    manual_frame.add(&manual_box);
    content.pack_start(&manual_frame, true, true, 0);

    // Connect NTP toggle to manual adjustments sensitivity
    let manual_box_clone = manual_box.clone();
    manual_box.set_sensitive(false);
    ntp.connect_toggled(move |btn| {
        manual_box_clone.set_sensitive(!btn.is_active());
    });

    // 4. Time Zone Row
    let tz_card = GtkBox::new(Orientation::Horizontal, 8);
    tz_card.style_context().add_class("slopos-section");
    let tz_icon = Image::from_icon_name(Some("preferences-system-symbolic"), gtk::IconSize::Menu);
    tz_card.pack_start(&tz_icon, false, false, 0);
    let tz_label = Label::new(Some("Time Zone:"));
    tz_label.style_context().add_class("slopos-control-title");
    tz_card.pack_start(&tz_label, false, false, 0);

    let timezone = ComboBoxText::new();
    for (id, label) in [
        ("UTC", "UTC (Coordinated Universal Time)"),
        ("America/New_York", "America / New York (Eastern)"),
        ("America/Chicago", "America / Chicago (Central)"),
        ("America/Denver", "America / Denver (Mountain)"),
        ("America/Los_Angeles", "America / Los Angeles (Pacific)"),
        ("Europe/London", "Europe / London (GMT/BST)"),
        ("Europe/Paris", "Europe / Paris (CET/CEST)"),
        ("Europe/Berlin", "Europe / Berlin (CET/CEST)"),
        ("Asia/Kolkata", "Asia / Kolkata (IST)"),
        ("Asia/Tokyo", "Asia / Tokyo (JST)"),
        ("Asia/Singapore", "Asia / Singapore (SGT)"),
        ("Australia/Sydney", "Australia / Sydney (AEST/AEDT)"),
    ] {
        timezone.append(Some(id), label);
    }
    timezone.set_active_id(Some("UTC"));
    tz_card.pack_start(&timezone, true, true, 0);
    content.pack_start(&tz_card, false, false, 0);

    dialog.show_all();

    if dialog.run() == ResponseType::Accept {
        let auto_ntp = ntp.is_active();
        if command_exists("timedatectl") {
            let ntp_arg = if auto_ntp { "true" } else { "false" };
            let _ = Command::new("timedatectl")
                .args(["set-ntp", ntp_arg])
                .spawn();

            if let Some(tz) = timezone.active_id() {
                let _ = Command::new("timedatectl")
                    .args(["set-timezone", tz.as_str()])
                    .spawn();
            }

            if !auto_ntp {
                let (year, month, day) = calendar.date();
                let h = spin_hour.value_as_int();
                let m = spin_min.value_as_int();
                let s = spin_sec.value_as_int();
                let date_str =
                    format!("{year:04}-{:02}-{:02} {h:02}:{m:02}:{s:02}", month + 1, day);
                let _ = Command::new("timedatectl")
                    .args(["set-time", &date_str])
                    .spawn();
            }
        }
    }
    unsafe {
        dialog.destroy();
    }
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
