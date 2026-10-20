//! Native Date & Time Settings Panel.

use gtk::atk::prelude::AtkObjectExt;
use gtk::prelude::*;
use gtk::{
    Adjustment, Align, Box as GtkBox, Calendar, CheckButton, ComboBoxText, Dialog, DialogFlags,
    Frame, Image, Label, Orientation, ResponseType, SpinButton, Window,
};
use slopos_services::timedate::{TimedateClient, TimedateSnapshot};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    thread,
};

pub fn show_datetime_dialog(parent: &Window) {
    let dialog = Dialog::with_buttons(
        Some("Date & Time"),
        Some(parent),
        DialogFlags::MODAL,
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

    let status = Label::new(Some("Reading date and time settings…"));
    status.set_xalign(0.0);
    content.pack_start(&status, false, false, 0);

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

    let live_time_label = Label::new(Some("Loading system time…"));
    live_time_label.set_xalign(0.0);
    live_time_label
        .style_context()
        .add_class("slopos-panel-title");
    header_text.pack_start(&live_time_label, false, false, 0);

    let live_date_label = Label::new(None);
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
    ntp.set_sensitive(false);
    let ntp_sub = Label::new(Some("Uses the system time synchronization service."));
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

    let cur_hour = 0.0;
    let cur_min = 0.0;
    let cur_sec = 0.0;

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
    timezone.set_sensitive(false);
    tz_card.pack_start(&timezone, true, true, 0);
    content.pack_start(&tz_card, false, false, 0);

    let latest = Rc::new(RefCell::new(None::<TimedateSnapshot>));
    let syncing = Rc::new(Cell::new(false));
    let ntp_dirty = Rc::new(Cell::new(false));
    let timezone_dirty = Rc::new(Cell::new(false));
    let time_dirty = Rc::new(Cell::new(false));
    let applying = Rc::new(Cell::new(false));
    let form_locked = Rc::new(Cell::new(false));
    let dirty = ntp_dirty.clone();
    let syncing_ntp = syncing.clone();
    ntp.connect_toggled(move |_| {
        if !syncing_ntp.get() {
            dirty.set(true);
        }
    });
    let dirty = timezone_dirty.clone();
    let syncing_timezone = syncing.clone();
    timezone.connect_changed(move |_| {
        if !syncing_timezone.get() {
            dirty.set(true);
        }
    });
    for widget in [&spin_hour, &spin_min, &spin_sec] {
        let dirty = time_dirty.clone();
        let syncing_time = syncing.clone();
        widget.connect_value_changed(move |_| {
            if !syncing_time.get() {
                dirty.set(true);
            }
        });
    }
    let dirty = time_dirty.clone();
    let syncing_calendar = syncing.clone();
    calendar.connect_day_selected(move |_| {
        if !syncing_calendar.get() {
            dirty.set(true);
        }
    });

    #[allow(deprecated)]
    let (load_sender, load_receiver) = glib::MainContext::channel::<
        Result<(TimedateSnapshot, Vec<String>), String>,
    >(glib::Priority::default());
    #[allow(deprecated)]
    let (watch_sender, watch_receiver) =
        glib::MainContext::channel::<Result<TimedateSnapshot, String>>(glib::Priority::default());
    let ntp_for_load = ntp.clone();
    let timezone_for_load = timezone.clone();
    let manual_for_load = manual_box.clone();
    let status_for_load = status.clone();
    let apply_for_load = dialog.widget_for_response(ResponseType::Accept);
    let latest_for_load = latest.clone();
    let syncing_for_load = syncing.clone();
    let calendar_for_load = calendar.clone();
    let hour_for_load = spin_hour.clone();
    let minute_for_load = spin_min.clone();
    let second_for_load = spin_sec.clone();
    let live_time_for_load = live_time_label.clone();
    let live_date_for_load = live_date_label.clone();
    load_receiver.attach(None, move |result| {
        match result {
            Ok((snapshot, zones)) => {
                syncing_for_load.set(true);
                if let Some(datetime) = system_datetime(&snapshot) {
                    calendar_for_load
                        .select_month(datetime.month() as u32 - 1, datetime.year() as u32);
                    calendar_for_load.select_day(datetime.day_of_month() as u32);
                    hour_for_load.set_value(datetime.hour() as f64);
                    minute_for_load.set_value(datetime.minute() as f64);
                    second_for_load.set_value(datetime.second() as f64);
                    live_time_for_load.set_text(
                        &datetime
                            .format("%H:%M:%S")
                            .map(|text| text.to_string())
                            .unwrap_or_else(|_| "System time unavailable".into()),
                    );
                    live_date_for_load.set_text(
                        &datetime
                            .format("%A, %B %e, %Y (%Z)")
                            .map(|text| text.to_string())
                            .unwrap_or_default(),
                    );
                } else {
                    live_time_for_load.set_text("System time unavailable");
                    live_date_for_load.set_text("");
                }
                timezone_for_load.remove_all();
                for zone in zones {
                    timezone_for_load.append(Some(&zone), &zone);
                }
                timezone_for_load.set_active_id(Some(&snapshot.timezone));
                ntp_for_load.set_active(snapshot.ntp_enabled);
                syncing_for_load.set(false);
                ntp_for_load.set_sensitive(snapshot.ntp_available);
                manual_for_load.set_sensitive(!snapshot.ntp_enabled);
                timezone_for_load.set_sensitive(true);
                status_for_load.set_text(&format!("Current time zone: {}", snapshot.timezone));
                if let Some(button) = &apply_for_load {
                    button.set_sensitive(true);
                }
                *latest_for_load.borrow_mut() = Some(snapshot);
            }
            Err(error) => {
                status_for_load.set_text(&format!("Date and time service unavailable: {error}"));
                ntp_for_load.set_sensitive(false);
                timezone_for_load.set_sensitive(false);
                manual_for_load.set_sensitive(false);
                if let Some(button) = &apply_for_load {
                    button.set_sensitive(false);
                }
            }
        }
        glib::ControlFlow::Break
    });

    let latest_for_watch = latest.clone();
    let syncing_for_watch = syncing.clone();
    let ntp_dirty_for_watch = ntp_dirty.clone();
    let timezone_dirty_for_watch = timezone_dirty.clone();
    let time_dirty_for_watch = time_dirty.clone();
    let form_locked_for_watch = form_locked.clone();
    let ntp_for_watch = ntp.clone();
    let timezone_for_watch = timezone.clone();
    let manual_for_watch = manual_box.clone();
    let calendar_for_watch = calendar.clone();
    let hour_for_watch = spin_hour.clone();
    let minute_for_watch = spin_min.clone();
    let second_for_watch = spin_sec.clone();
    let live_time_for_watch = live_time_label.clone();
    let live_date_for_watch = live_date_label.clone();
    let status_for_watch = status.clone();
    let apply_for_watch = dialog.widget_for_response(ResponseType::Accept);
    let watch_source = Rc::new(RefCell::new(Some(watch_receiver.attach(
        None,
        move |result| {
            if latest_for_watch.borrow().is_none() {
                return glib::ControlFlow::Continue;
            }
            match result {
                Ok(snapshot) => {
                    let previous = latest_for_watch.borrow().clone();
                    syncing_for_watch.set(true);
                    if !timezone_dirty_for_watch.get() {
                        timezone_for_watch.set_active_id(Some(&snapshot.timezone));
                    }
                    if !ntp_dirty_for_watch.get() {
                        ntp_for_watch.set_active(snapshot.ntp_enabled);
                    }
                    if !time_dirty_for_watch.get() {
                        if let Some(datetime) = system_datetime(&snapshot) {
                            calendar_for_watch
                                .select_month(datetime.month() as u32 - 1, datetime.year() as u32);
                            calendar_for_watch.select_day(datetime.day_of_month() as u32);
                            hour_for_watch.set_value(datetime.hour() as f64);
                            minute_for_watch.set_value(datetime.minute() as f64);
                            second_for_watch.set_value(datetime.second() as f64);
                        }
                    }
                    if let Some(datetime) = system_datetime(&snapshot) {
                        live_time_for_watch.set_text(
                            &datetime
                                .format("%H:%M:%S")
                                .map(|text| text.to_string())
                                .unwrap_or_else(|_| "System time unavailable".into()),
                        );
                        live_date_for_watch.set_text(
                            &datetime
                                .format("%A, %B %e, %Y (%Z)")
                                .map(|text| text.to_string())
                                .unwrap_or_default(),
                        );
                    } else {
                        live_time_for_watch.set_text("System time unavailable");
                        live_date_for_watch.set_text("");
                    }
                    syncing_for_watch.set(false);
                    let locked = form_locked_for_watch.get();
                    let ntp_enabled = if ntp_dirty_for_watch.get() {
                        ntp_for_watch.is_active()
                    } else {
                        snapshot.ntp_enabled
                    };
                    ntp_for_watch.set_sensitive(snapshot.ntp_available && !locked);
                    manual_for_watch.set_sensitive(!ntp_enabled && !locked);
                    timezone_for_watch.set_sensitive(!locked);
                    if let Some(button) = &apply_for_watch {
                        button.set_sensitive(!locked);
                    }
                    if !locked
                        && previous
                            .as_ref()
                            .is_some_and(|previous| previous.timezone != snapshot.timezone)
                    {
                        status_for_watch.set_text(&format!(
                            "System time zone changed to {}.",
                            snapshot.timezone
                        ));
                    }
                    *latest_for_watch.borrow_mut() = Some(snapshot);
                }
                Err(error) => {
                    status_for_watch
                        .set_text(&format!("Date and time service unavailable: {error}"));
                    ntp_for_watch.set_sensitive(false);
                    timezone_for_watch.set_sensitive(false);
                    manual_for_watch.set_sensitive(false);
                    if let Some(button) = &apply_for_watch {
                        button.set_sensitive(false);
                    }
                }
            }
            glib::ControlFlow::Continue
        },
    ))));

    if let Some(button) = dialog.widget_for_response(ResponseType::Accept) {
        button.set_sensitive(false);
    }
    thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                let _ = load_sender.send(Err(error.to_string()));
                return;
            }
        };
        let startup = runtime.block_on(async {
            let client = TimedateClient::connect()
                .await
                .map_err(|error| error.to_string())?;
            let snapshot = client.snapshot().await.map_err(|error| error.to_string())?;
            let zones = client
                .list_timezones()
                .await
                .map_err(|error| error.to_string())?;
            Ok::<_, String>((client, snapshot, zones))
        });
        match startup {
            Ok((client, snapshot, zones)) => {
                if load_sender.send(Ok((snapshot, zones))).is_err() {
                    return;
                }
                runtime.block_on(async move {
                    let mut updates = client.watch();
                    while let Some(update) = updates.next().await {
                        if watch_sender
                            .send(update.map_err(|error| error.to_string()))
                            .is_err()
                        {
                            break;
                        }
                    }
                });
            }
            Err(error) => {
                let _ = load_sender.send(Err(error));
            }
        }
    });

    let main_loop = glib::MainLoop::new(None, false);
    let quit_on_destroy = main_loop.clone();
    let watch_source_on_destroy = watch_source.clone();
    dialog.connect_destroy(move |_| {
        if let Some(source_id) = watch_source_on_destroy.borrow_mut().take() {
            if let Some(source) = glib::MainContext::default().find_source_by_id(&source_id) {
                source.destroy();
            }
        }
        quit_on_destroy.quit();
    });
    let quit_on_response = main_loop.clone();
    let applying_response = applying.clone();
    let status_response = status.clone();
    let latest_response = latest.clone();
    let ntp_response = ntp.clone();
    let timezone_response = timezone.clone();
    let calendar_response = calendar.clone();
    let hour_response = spin_hour.clone();
    let minute_response = spin_min.clone();
    let second_response = spin_sec.clone();
    let dirty_ntp_response = ntp_dirty.clone();
    let dirty_timezone_response = timezone_dirty.clone();
    let dirty_time_response = time_dirty.clone();
    let form_locked_response = form_locked.clone();
    let ntp_for_response = ntp.clone();
    let timezone_for_response = timezone.clone();
    let manual_for_response = manual_box.clone();
    let dialog_for_response = dialog.clone();
    dialog.connect_response(move |dialog, response| {
        if applying_response.get() {
            return;
        }
        if response != ResponseType::Accept {
            dialog.close();
            quit_on_response.quit();
            return;
        }
        let Some(previous) = latest_response.borrow().clone() else { return; };
        let selected_timezone = timezone_response
            .active_id()
            .map(|zone| zone.to_string())
            .unwrap_or_else(|| previous.timezone.clone());
        let ntp_enabled = ntp_response.is_active();
        let timezone_edited = dirty_timezone_response.get();
        let ntp_edited = dirty_ntp_response.get();
        let manual_time = if dirty_time_response.get() && !ntp_enabled {
            let (year, month, day) = calendar_response.date();
            Some(ManualTimeInput {
                year,
                month: month + 1,
                day,
                hour: hour_response.value_as_int(),
                minute: minute_response.value_as_int(),
                second: second_response.value(),
            })
        } else {
            None
        };
        if let Some(time) = manual_time {
            if local_time_unix_usec(
                Some("UTC"),
                time.year,
                time.month,
                time.day,
                time.hour,
                time.minute,
                time.second,
            )
            .is_none()
            {
                status_response.set_text("The selected local date and time is invalid.");
                return;
            }
        }
        applying_response.set(true);
        form_locked_response.set(true);
        dialog.set_deletable(false);
        ntp_for_response.set_sensitive(false);
        timezone_for_response.set_sensitive(false);
        manual_for_response.set_sensitive(false);
        if let Some(button) = dialog.widget_for_response(ResponseType::Accept) { button.set_sensitive(false); }
        if let Some(button) = dialog.widget_for_response(ResponseType::Cancel) { button.set_sensitive(false); }
        status_response.set_text("Applying date and time settings…");
        #[allow(deprecated)]
        let (sender, receiver) = glib::MainContext::channel::<Result<TimedateSnapshot, String>>(glib::Priority::default());
        let status_result = status_response.clone();
        let latest_result = latest_response.clone();
        let applying_result = applying_response.clone();
        let dialog_result = dialog_for_response.clone();
        receiver.attach(None, move |result| {
            applying_result.set(false);
            dialog_result.set_deletable(true);
            match result {
                Ok(snapshot) => {
                    status_result.set_text("Date and time settings applied and verified.");
                    *latest_result.borrow_mut() = Some(snapshot);
                    dialog_result.response(ResponseType::Close);
                }
                Err(error) => {
                    status_result.set_text(&format!("Could not apply settings: {error}. Close and reopen this panel to refresh system state."));
                    if let Some(button) = dialog_result.widget_for_response(ResponseType::Cancel) { button.set_sensitive(true); }
                }
            }
            glib::ControlFlow::Break
        });
        thread::spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread().enable_all().build()
            .map_err(|error| error.to_string())
            .and_then(|runtime| runtime.block_on(async move {
                let client = TimedateClient::connect().await.map_err(|error| error.to_string())?;
                let current = client.snapshot().await.map_err(|error| error.to_string())?;
                let time_usec = match manual_time {
                    Some(time) => Some(
                        manual_time_unix_usec_for_timezone(
                            &current.timezone,
                            &selected_timezone,
                            timezone_edited,
                            time,
                        )
                        .ok_or_else(|| "The selected local date and time is invalid.".to_owned())?,
                    ),
                    None => None,
                };
                let effective_ntp = if ntp_edited { ntp_enabled } else { current.ntp_enabled };
                if time_usec.is_some() && effective_ntp {
                    return Err("Automatic time synchronization became enabled.".to_owned());
                }
                if timezone_edited && selected_timezone != current.timezone {
                    client.set_timezone(&selected_timezone).await.map_err(|error| error.to_string())?;
                }
                if ntp_edited && ntp_enabled != current.ntp_enabled {
                    client.set_ntp(ntp_enabled).await.map_err(|error| error.to_string())?;
                }
                    if let Some(time_usec) = time_usec {
                        client.set_time_usec(time_usec).await.map_err(|error| error.to_string())?;
                    }
                    client.snapshot().await.map_err(|error| error.to_string())
                }));
            let _ = sender.send(result);
        });
    });
    dialog.show_all();
    main_loop.run();
    dialog.close();
}

fn system_datetime(snapshot: &TimedateSnapshot) -> Option<glib::DateTime> {
    let timestamp = i64::try_from(snapshot.time_unix_usec / 1_000_000).ok()?;
    let timezone = glib::TimeZone::new(Some(&snapshot.timezone));
    glib::DateTime::from_unix_utc(timestamp)
        .ok()?
        .to_timezone(&timezone)
        .ok()
}

#[derive(Clone, Copy)]
struct ManualTimeInput {
    year: u32,
    month: u32,
    day: u32,
    hour: i32,
    minute: i32,
    second: f64,
}

fn manual_time_unix_usec_for_timezone(
    current_timezone: &str,
    selected_timezone: &str,
    timezone_edited: bool,
    time: ManualTimeInput,
) -> Option<u64> {
    let timezone = if timezone_edited {
        selected_timezone
    } else {
        current_timezone
    };
    local_time_unix_usec(
        Some(timezone),
        time.year,
        time.month,
        time.day,
        time.hour,
        time.minute,
        time.second,
    )
}

fn local_time_unix_usec(
    timezone_identifier: Option<&str>,
    year: u32,
    month: u32,
    day: u32,
    hour: i32,
    minute: i32,
    second: f64,
) -> Option<u64> {
    let timezone = glib::TimeZone::new(timezone_identifier);
    let datetime = glib::DateTime::new(
        &timezone,
        i32::try_from(year).ok()?,
        i32::try_from(month).ok()?,
        i32::try_from(day).ok()?,
        hour,
        minute,
        second,
    )
    .ok()?;
    u64::try_from(datetime.to_unix())
        .ok()?
        .checked_mul(1_000_000)
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

#[cfg(test)]
mod tests {
    use super::{
        local_time_unix_usec, manual_time_unix_usec_for_timezone, system_datetime, ManualTimeInput,
    };
    use slopos_services::timedate::TimedateSnapshot;

    #[test]
    fn manual_clock_uses_selected_timezone_and_rejects_invalid_dates() {
        assert_eq!(
            local_time_unix_usec(Some("UTC"), 1970, 1, 1, 0, 0, 0.0),
            Some(0)
        );
        assert_eq!(
            local_time_unix_usec(Some("Asia/Kolkata"), 1970, 1, 2, 0, 0, 0.0),
            Some(66_600_000_000)
        );
        assert_eq!(
            local_time_unix_usec(Some("UTC"), 2024, 2, 30, 0, 0, 0.0),
            None
        );
    }

    #[test]
    fn live_time_uses_the_current_snapshot_timezone() {
        let snapshot = TimedateSnapshot {
            revision: 1,
            timezone: "Asia/Kolkata".into(),
            ntp_available: true,
            ntp_enabled: true,
            ntp_synchronized: true,
            time_unix_usec: 0,
        };
        let datetime = system_datetime(&snapshot).unwrap();
        assert_eq!(datetime.hour(), 5);
        assert_eq!(datetime.minute(), 30);
    }

    #[test]
    fn manual_time_uses_fresh_system_timezone_unless_timezone_was_edited() {
        assert_eq!(
            manual_time_unix_usec_for_timezone(
                "Asia/Kolkata",
                "UTC",
                false,
                ManualTimeInput {
                    year: 1970,
                    month: 1,
                    day: 2,
                    hour: 0,
                    minute: 0,
                    second: 0.0,
                },
            ),
            Some(66_600_000_000)
        );
        assert_eq!(
            manual_time_unix_usec_for_timezone(
                "Asia/Kolkata",
                "UTC",
                true,
                ManualTimeInput {
                    year: 1970,
                    month: 1,
                    day: 2,
                    hour: 0,
                    minute: 0,
                    second: 0.0,
                },
            ),
            Some(86_400_000_000)
        );
    }

    #[test]
    fn manual_time_conversion_runs_on_a_worker_thread() {
        let result = std::thread::spawn(|| {
            manual_time_unix_usec_for_timezone(
                "Asia/Kolkata",
                "UTC",
                false,
                ManualTimeInput {
                    year: 1970,
                    month: 1,
                    day: 2,
                    hour: 0,
                    minute: 0,
                    second: 0.0,
                },
            )
        })
        .join()
        .unwrap();
        assert_eq!(result, Some(66_600_000_000));
    }
}
