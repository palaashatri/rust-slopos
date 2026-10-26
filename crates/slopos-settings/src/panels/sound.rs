//! Native Sound & Audio Settings Panel.

use crate::providers::availability::command_exists;
use gtk::atk::prelude::AtkObjectExt;
use gtk::prelude::*;
use gtk::{
    Box as GtkBox, Button, CheckButton, ComboBoxText, Dialog, DialogFlags, Frame, IconSize, Image,
    Label, Orientation, ResponseType, Scale, Window,
};
use slopos_services::audio::{AudioClient, AudioDevice, AudioSnapshot};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    thread,
};

pub fn show_sound_dialog(parent: &Window) {
    let dialog = Dialog::with_buttons(
        Some("Sound & Volume"),
        Some(parent),
        DialogFlags::MODAL | DialogFlags::DESTROY_WITH_PARENT,
        &[
            ("Cancel", ResponseType::Cancel),
            ("Apply", ResponseType::Accept),
        ],
    );
    dialog.set_default_response(ResponseType::Accept);
    dialog.set_default_size(460, 420);
    set_accessible_name(&dialog, "SLOPOS sound and volume settings");

    let content = dialog.content_area();
    content.set_spacing(10);
    content.set_margin_start(14);
    content.set_margin_end(14);
    content.set_margin_top(12);
    content.set_margin_bottom(12);

    let title = Label::new(Some("Sound Preferences"));
    title.set_xalign(0.0);
    title.style_context().add_class("slopos-control-title");
    content.pack_start(&title, false, false, 0);
    let status = Label::new(Some("Reading audio devices…"));
    status.set_xalign(0.0);
    content.pack_start(&status, false, false, 0);

    // Output Section
    let output_frame = Frame::new(Some("Output (Speakers / Headphones)"));
    output_frame.style_context().add_class("slopos-section");
    let output_box = GtkBox::new(Orientation::Vertical, 8);
    output_box.set_margin_start(10);
    output_box.set_margin_end(10);
    output_box.set_margin_top(8);
    output_box.set_margin_bottom(8);

    let device_row = GtkBox::new(Orientation::Horizontal, 8);
    device_row.pack_start(&Label::new(Some("Output Device:")), false, false, 0);
    let device_combo = ComboBoxText::new();
    device_row.pack_start(&device_combo, true, true, 0);
    output_box.pack_start(&device_row, false, false, 0);

    let vol_row = GtkBox::new(Orientation::Horizontal, 8);
    vol_row.pack_start(
        &Image::from_icon_name(Some("audio-volume-high-symbolic"), IconSize::Button),
        false,
        false,
        0,
    );
    vol_row.pack_start(&Label::new(Some("Volume:")), false, false, 0);
    let vol_scale = Scale::with_range(Orientation::Horizontal, 0.0, 150.0, 1.0);
    vol_scale.set_hexpand(true);
    vol_scale.set_draw_value(true);
    vol_scale.set_value_pos(gtk::PositionType::Right);
    vol_row.pack_start(&vol_scale, true, true, 0);
    output_box.pack_start(&vol_row, false, false, 0);

    let mute_check = CheckButton::with_label("Mute output");
    output_box.pack_start(&mute_check, false, false, 0);

    let test_row = GtkBox::new(Orientation::Horizontal, 8);
    let test_btn = Button::with_label("Play Test Sound");
    test_btn.style_context().add_class("slopos-push-btn");
    const TEST_SOUND_PLAYERS: &[(&str, &[&str])] = &[
        ("paplay", &["/usr/share/sounds/freedesktop/stereo/bell.oga"]),
        ("canberra-gtk-play", &["-i", "bell"]),
        ("speaker-test", &["-t", "sine", "-f", "440", "-l", "1"]),
    ];
    match slopos_core::process::first_available(TEST_SOUND_PLAYERS) {
        Some((player, args)) => {
            test_btn.connect_clicked(move |_| {
                if let Err(error) = slopos_core::process::spawn_detached(&player, args) {
                    log::warn!("Could not play the test sound: {error}");
                }
            });
        }
        None => {
            test_btn.set_sensitive(false);
            test_btn.set_tooltip_text(Some(
                "No sound player (paplay, canberra-gtk-play) is installed",
            ));
        }
    }
    test_row.pack_start(&test_btn, false, false, 0);
    output_box.pack_start(&test_row, false, false, 0);

    output_frame.add(&output_box);
    content.pack_start(&output_frame, false, false, 0);

    // Input Section
    let input_frame = Frame::new(Some("Input (Microphone)"));
    input_frame.style_context().add_class("slopos-section");
    let input_box = GtkBox::new(Orientation::Vertical, 8);
    input_box.set_margin_start(10);
    input_box.set_margin_end(10);
    input_box.set_margin_top(8);
    input_box.set_margin_bottom(8);

    let input_vol_row = GtkBox::new(Orientation::Horizontal, 8);
    input_vol_row.pack_start(
        &Image::from_icon_name(Some("audio-input-microphone-symbolic"), IconSize::Button),
        false,
        false,
        0,
    );
    input_vol_row.pack_start(&Label::new(Some("Mic Level:")), false, false, 0);
    let input_device_combo = ComboBoxText::new();
    input_device_combo.set_hexpand(true);
    input_box.pack_start(&input_device_combo, false, false, 0);
    let input_scale = Scale::with_range(Orientation::Horizontal, 0.0, 150.0, 1.0);
    input_scale.set_hexpand(true);
    input_scale.set_draw_value(true);
    input_scale.set_value_pos(gtk::PositionType::Right);
    input_vol_row.pack_start(&input_scale, true, true, 0);
    input_box.pack_start(&input_vol_row, false, false, 0);

    let input_mute_check = CheckButton::with_label("Mute microphone");
    input_box.pack_start(&input_mute_check, false, false, 0);

    input_frame.add(&input_box);
    content.pack_start(&input_frame, false, false, 0);

    if command_exists("pavucontrol") {
        let adv_btn = Button::with_label("Open Advanced Audio Mixer (Pavucontrol)");
        adv_btn.style_context().add_class("slopos-push-btn");
        adv_btn.connect_clicked(|_| {
            if let Err(error) = slopos_core::process::launch("pavucontrol", &[]) {
                log::warn!("{error}");
            }
        });
        content.pack_start(&adv_btn, false, false, 0);
    }

    let output_dirty = Rc::new(Cell::new(false));
    let output_mute_dirty = Rc::new(Cell::new(false));
    let output_device_dirty = Rc::new(Cell::new(false));
    let input_dirty = Rc::new(Cell::new(false));
    let input_mute_dirty = Rc::new(Cell::new(false));
    let input_device_dirty = Rc::new(Cell::new(false));
    let syncing = Rc::new(Cell::new(false));
    let flag = output_dirty.clone();
    let syncing_flag = syncing.clone();
    vol_scale.connect_value_changed(move |_| {
        if !syncing_flag.get() {
            flag.set(true);
        }
    });
    let flag = output_mute_dirty.clone();
    let syncing_flag = syncing.clone();
    mute_check.connect_toggled(move |_| {
        if !syncing_flag.get() {
            flag.set(true);
        }
    });
    let flag = output_device_dirty.clone();
    let syncing_flag = syncing.clone();
    device_combo.connect_changed(move |_| {
        if !syncing_flag.get() {
            flag.set(true);
        }
    });
    let flag = input_dirty.clone();
    let syncing_flag = syncing.clone();
    input_scale.connect_value_changed(move |_| {
        if !syncing_flag.get() {
            flag.set(true);
        }
    });
    let flag = input_mute_dirty.clone();
    let syncing_flag = syncing.clone();
    input_mute_check.connect_toggled(move |_| {
        if !syncing_flag.get() {
            flag.set(true);
        }
    });
    let flag = input_device_dirty.clone();
    let syncing_flag = syncing.clone();
    input_device_combo.connect_changed(move |_| {
        if !syncing_flag.get() {
            flag.set(true);
        }
    });

    device_combo.set_sensitive(false);
    vol_scale.set_sensitive(false);
    mute_check.set_sensitive(false);
    input_device_combo.set_sensitive(false);
    input_scale.set_sensitive(false);
    input_mute_check.set_sensitive(false);
    if let Some(button) = dialog.widget_for_response(ResponseType::Accept) {
        button.set_sensitive(false);
    }

    let latest = Rc::new(RefCell::new(None::<AudioSnapshot>));
    let latest_for_output = latest.clone();
    let volume_for_output = vol_scale.clone();
    let mute_for_output = mute_check.clone();
    let syncing_for_output = syncing.clone();
    device_combo.connect_changed(move |combo| {
        if syncing_for_output.get() {
            return;
        }
        let selected = combo.active_id().map(|id| id.to_string());
        if let Some(device) = latest_for_output.borrow().as_ref().and_then(|snapshot| {
            snapshot
                .outputs
                .iter()
                .find(|item| Some(item.name.as_str()) == selected.as_deref())
        }) {
            volume_for_output.set_value(device_volume(device));
            mute_for_output.set_active(device.muted);
            volume_for_output.set_sensitive(true);
            mute_for_output.set_sensitive(true);
        }
    });
    let latest_for_input = latest.clone();
    let volume_for_input = input_scale.clone();
    let mute_for_input = input_mute_check.clone();
    let syncing_for_input = syncing.clone();
    input_device_combo.connect_changed(move |combo| {
        if syncing_for_input.get() {
            return;
        }
        let selected = combo.active_id().map(|id| id.to_string());
        if let Some(device) = latest_for_input.borrow().as_ref().and_then(|snapshot| {
            snapshot
                .inputs
                .iter()
                .find(|item| Some(item.name.as_str()) == selected.as_deref())
        }) {
            volume_for_input.set_value(device_volume(device));
            mute_for_input.set_active(device.muted);
            volume_for_input.set_sensitive(true);
            mute_for_input.set_sensitive(true);
        }
    });
    #[allow(deprecated)]
    let (snapshot_sender, snapshot_receiver) =
        glib::MainContext::channel::<Result<AudioSnapshot, String>>(glib::Priority::default());
    let output_combo = device_combo.clone();
    let input_combo = input_device_combo.clone();
    let output_volume = vol_scale.clone();
    let input_volume = input_scale.clone();
    let output_mute = mute_check.clone();
    let input_mute = input_mute_check.clone();
    let status_label = status.clone();
    let apply_button = dialog.widget_for_response(ResponseType::Accept);
    let latest_snapshot = latest.clone();
    let syncing_snapshot = syncing.clone();
    let dirty_flags = [
        output_dirty.clone(),
        output_mute_dirty.clone(),
        output_device_dirty.clone(),
        input_dirty.clone(),
        input_mute_dirty.clone(),
        input_device_dirty.clone(),
    ];
    snapshot_receiver.attach(None, move |result| {
        match result {
            Ok(snapshot) => {
                syncing_snapshot.set(true);
                if !dirty_flags[2].get() {
                    fill_devices(
                        &output_combo,
                        &snapshot.outputs,
                        snapshot.default_output.as_deref(),
                    );
                }
                if !dirty_flags[5].get() {
                    fill_devices(
                        &input_combo,
                        &snapshot.inputs,
                        snapshot.default_input.as_deref(),
                    );
                }
                let output_id = output_combo.active_id().map(|id| id.to_string());
                let input_id = input_combo.active_id().map(|id| id.to_string());
                let selected_output = snapshot
                    .outputs
                    .iter()
                    .find(|device| Some(device.name.as_str()) == output_id.as_deref());
                let selected_input = snapshot
                    .inputs
                    .iter()
                    .find(|device| Some(device.name.as_str()) == input_id.as_deref());
                if !dirty_flags[0].get() {
                    if let Some(output) = selected_output {
                        output_volume.set_value(device_volume(output));
                    }
                }
                if !dirty_flags[1].get() {
                    if let Some(output) = selected_output {
                        output_mute.set_active(output.muted);
                    }
                }
                if !dirty_flags[3].get() {
                    if let Some(input) = selected_input {
                        input_volume.set_value(device_volume(input));
                    }
                }
                if !dirty_flags[4].get() {
                    if let Some(input) = selected_input {
                        input_mute.set_active(input.muted);
                    }
                }
                let has_output = !snapshot.outputs.is_empty();
                let has_input = !snapshot.inputs.is_empty();
                output_combo.set_sensitive(has_output);
                output_volume.set_sensitive(selected_output.is_some());
                output_mute.set_sensitive(selected_output.is_some());
                input_combo.set_sensitive(has_input);
                input_volume.set_sensitive(selected_input.is_some());
                input_mute.set_sensitive(selected_input.is_some());
                status_label.set_text(if has_output || has_input {
                    "Audio devices are available. Changes are verified by the audio service."
                } else {
                    "No audio devices are available."
                });
                if let Some(button) = &apply_button {
                    button.set_sensitive(has_output || has_input);
                }
                *latest_snapshot.borrow_mut() = Some(snapshot);
                syncing_snapshot.set(false);
            }
            Err(error) => {
                output_combo.set_sensitive(false);
                output_volume.set_sensitive(false);
                output_mute.set_sensitive(false);
                input_combo.set_sensitive(false);
                input_volume.set_sensitive(false);
                input_mute.set_sensitive(false);
                status_label.set_text(&format!("Audio service unavailable: {error}"));
                if let Some(button) = &apply_button {
                    button.set_sensitive(false);
                }
            }
        }
        glib::ControlFlow::Continue
    });
    let (stop_audio, mut stop_audio_receiver) = tokio::sync::oneshot::channel();
    let audio_client = AudioClient::new();
    let watcher_client = audio_client.clone();
    thread::spawn(move || {
        match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime.block_on(async move {
                let mut events = watcher_client.watch();
                loop {
                    tokio::select! {
                        _ = &mut stop_audio_receiver => break,
                        update = events.next() => match update {
                            Some(update) => {
                                if snapshot_sender.send(update.map_err(|error| error.to_string())).is_err() {
                                    break;
                                }
                            }
                            None => break,
                        }
                    }
                }
                events.close().await;
            }),
            Err(error) => {
                let _ = snapshot_sender.send(Err(error.to_string()));
            }
        }
    });

    let main_loop = glib::MainLoop::new(None, false);
    let applying = Rc::new(Cell::new(false));
    let loop_on_response = main_loop.clone();
    let loop_on_destroy = main_loop.clone();
    dialog.connect_destroy(move |_| loop_on_destroy.quit());
    let loop_on_delete = main_loop.clone();
    let applying_on_delete = applying.clone();
    dialog.connect_delete_event(move |dialog, _| {
        if applying_on_delete.get() {
            return glib::Propagation::Stop;
        }
        dialog.close();
        loop_on_delete.quit();
        glib::Propagation::Stop
    });
    let applying_on_key = applying.clone();
    dialog.connect_key_press_event(move |dialog, event| {
        if event.keyval() == gdk::keys::constants::Escape {
            if !applying_on_key.get() {
                dialog.response(ResponseType::Cancel);
            }
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    let latest_on_response = latest.clone();
    let status_on_response = status.clone();
    let output_combo_on_response = device_combo.clone();
    let input_combo_on_response = input_device_combo.clone();
    let output_volume_on_response = vol_scale.clone();
    let input_volume_on_response = input_scale.clone();
    let output_mute_on_response = mute_check.clone();
    let input_mute_on_response = input_mute_check.clone();
    let dirty_on_response = [
        output_dirty,
        output_mute_dirty,
        output_device_dirty,
        input_dirty,
        input_mute_dirty,
        input_device_dirty,
    ];
    let audio_client_on_response = audio_client.clone();
    let applying_on_response = applying.clone();
    dialog.connect_response(move |dialog, response| {
        if response != ResponseType::Accept {
            if applying_on_response.get() {
                return;
            }
            dialog.close();
            loop_on_response.quit();
            return;
        }
        let Some(previous) = latest_on_response.borrow().clone() else {
            return;
        };
        let output = output_combo_on_response
            .active_id()
            .map(|id| id.to_string());
        let input = input_combo_on_response.active_id().map(|id| id.to_string());
        let output_volume = output_volume_on_response.value().round().clamp(0.0, 150.0) as u32;
        let input_volume = input_volume_on_response.value().round().clamp(0.0, 150.0) as u32;
        let output_muted = output_mute_on_response.is_active();
        let input_muted = input_mute_on_response.is_active();
        let changes = [
            dirty_on_response[0].get(),
            dirty_on_response[1].get(),
            dirty_on_response[2].get(),
            dirty_on_response[3].get(),
            dirty_on_response[4].get(),
            dirty_on_response[5].get(),
        ];
        let audio_client = audio_client_on_response.clone();
        applying_on_response.set(true);
        dialog.set_deletable(false);
        if let Some(button) = dialog.widget_for_response(ResponseType::Accept) {
            button.set_sensitive(false);
        }
        if let Some(button) = dialog.widget_for_response(ResponseType::Cancel) {
            button.set_sensitive(false);
        }
        output_combo_on_response.set_sensitive(false);
        output_volume_on_response.set_sensitive(false);
        output_mute_on_response.set_sensitive(false);
        input_combo_on_response.set_sensitive(false);
        input_volume_on_response.set_sensitive(false);
        input_mute_on_response.set_sensitive(false);
        status_on_response.set_text("Applying changes and verifying audio service state…");
        #[allow(deprecated)]
        let (sender, receiver) =
            glib::MainContext::channel::<Result<(), String>>(glib::Priority::default());
        let dialog_on_result = dialog.clone();
        let status_on_result = status_on_response.clone();
        let applying_on_result = applying_on_response.clone();
        let latest_on_result = latest_on_response.clone();
        let output_combo_on_result = output_combo_on_response.clone();
        let output_volume_on_result = output_volume_on_response.clone();
        let output_mute_on_result = output_mute_on_response.clone();
        let input_combo_on_result = input_combo_on_response.clone();
        let input_volume_on_result = input_volume_on_response.clone();
        let input_mute_on_result = input_mute_on_response.clone();
        receiver.attach(None, move |result| {
            match result {
                Ok(()) => {
                    applying_on_result.set(false);
                    dialog_on_result.response(ResponseType::Close);
                }
                Err(error) => {
                    applying_on_result.set(false);
                    dialog_on_result.set_deletable(true);
                    if let Some(snapshot) = latest_on_result.borrow().as_ref() {
                        let has_output = !snapshot.outputs.is_empty();
                        let has_input = !snapshot.inputs.is_empty();
                        output_combo_on_result.set_sensitive(has_output);
                        output_volume_on_result.set_sensitive(has_output);
                        output_mute_on_result.set_sensitive(has_output);
                        input_combo_on_result.set_sensitive(has_input);
                        input_volume_on_result.set_sensitive(has_input);
                        input_mute_on_result.set_sensitive(has_input);
                    }
                    status_on_result.set_text(&format!("Could not apply sound settings: {error}"));
                    if let Some(button) = dialog_on_result.widget_for_response(ResponseType::Accept)
                    {
                        button.set_sensitive(true);
                    }
                    if let Some(button) = dialog_on_result.widget_for_response(ResponseType::Cancel)
                    {
                        button.set_sensitive(true);
                    }
                }
            }
            glib::ControlFlow::Break
        });
        thread::spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| error.to_string())
                .and_then(|runtime| {
                    runtime
                        .block_on(async move {
                            if let Some(device) = output.as_deref() {
                                if changes[2] && previous.default_output.as_deref() != Some(device)
                                {
                                    audio_client.set_default_output(device).await?;
                                }
                                if changes[0] {
                                    audio_client
                                        .set_output_volume(device, output_volume)
                                        .await?;
                                }
                                if changes[1] {
                                    audio_client.set_output_muted(device, output_muted).await?;
                                }
                            }
                            if let Some(device) = input.as_deref() {
                                if changes[5] && previous.default_input.as_deref() != Some(device) {
                                    audio_client.set_default_input(device).await?;
                                }
                                if changes[3] {
                                    audio_client.set_input_volume(device, input_volume).await?;
                                }
                                if changes[4] {
                                    audio_client.set_input_muted(device, input_muted).await?;
                                }
                            }
                            Ok::<_, slopos_services::audio::AudioError>(())
                        })
                        .map_err(|error| error.to_string())
                });
            let _ = sender.send(result);
        });
    });
    dialog.show_all();
    main_loop.run();
    let _ = stop_audio.send(());
    dialog.close();
}

fn fill_devices(combo: &ComboBoxText, devices: &[AudioDevice], selected: Option<&str>) {
    combo.remove_all();
    for device in devices {
        combo.append(
            Some(&device.name),
            device.description.as_deref().unwrap_or(&device.name),
        );
    }
    if let Some(selected) = selected {
        combo.set_active_id(Some(selected));
    }
}

fn device_volume(device: &AudioDevice) -> f64 {
    if device.volume_millipercent.is_empty() {
        return 0.0;
    }
    let average = device
        .volume_millipercent
        .iter()
        .map(|volume| u64::from(*volume))
        .sum::<u64>()
        / device.volume_millipercent.len() as u64;
    ((average + 500) / 1000).min(150) as f64
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
