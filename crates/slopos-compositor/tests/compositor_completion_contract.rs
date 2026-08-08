// Copyright (c) 2026 Palaash Atri
// SPDX-License-Identifier: MIT

//! Cross-module compositor completion contracts.
//!
//! These tests intentionally exercise public policy APIs as a consumer would.
//! They protect the state invariants that the nested and DRM backends must
//! share: reversible presentation transitions, deterministic frame pacing,
//! gapless tiling, dynamic Spaces, and deterministic output migration.

use slopos_compositor::frame_timing::{FrameScheduler, RefreshRate};
use slopos_compositor::{
    calculate_presentation_geometry, transition_presentation_state, MultiMonitorPolicy,
    SpaceTarget, SpacesModel, TilePlacement, WindowGeometry, WindowPresentationState,
};
use std::time::{Duration, Instant};

#[test]
fn headless_runtime_gate_builds_the_binary_it_executes() {
    let script = include_str!("../../../scripts/verify-compositor-headless-runtime.sh");
    let build_command =
        "cargo build -p slopos-compositor --bin slopos-compositor --examples --locked";

    assert!(
        script.contains(build_command),
        "headless runtime gate must build the compositor binary before executing target/debug/slopos-compositor"
    );
}

#[test]
fn headless_runtime_gate_exercises_native_clipboard_transfer() {
    let script = include_str!("../../../scripts/verify-compositor-headless-runtime.sh");
    let client = include_str!("../examples/headless_clipboard_client.rs");
    assert!(
        script.contains("headless_clipboard_client"),
        "headless runtime gate must run the native clipboard source/sink client"
    );
    for marker in [
        "SLOPOS_CLIPBOARD_OFFER_VERIFIED",
        "SLOPOS_CLIPBOARD_TRANSFER_VERIFIED",
        "SLOPOS_CLIPBOARD_LARGE_TRANSFER_VERIFIED",
        "SLOPOS_CLIPBOARD_MISSING_MIME_EOF_VERIFIED",
        "SLOPOS_CLIPBOARD_SOURCE_DEATH_CLEARED",
    ] {
        assert!(
            script.contains(marker),
            "headless runtime gate must require clipboard marker {marker}"
        );
    }
    assert!(
        client.contains("wl_data_device::Event::Selection { id: Some(id) }"),
        "clipboard client must treat only a Some(data_offer) event as a live selection"
    );
    assert!(
        client.contains("wl_data_device::Event::Selection { id: None }"),
        "clipboard client must observe the protocol's explicit selection-clear event"
    );
    assert!(
        client.contains("clipboard source disconnect did not emit selection clear"),
        "source-death gate must require an explicit selection-clear event"
    );
}

#[test]
fn headless_runtime_gate_exercises_native_primary_selection_transfer() {
    let script = include_str!("../../../scripts/verify-compositor-headless-runtime.sh");
    let client = include_str!("../examples/headless_clipboard_client.rs");
    assert!(
        script.contains("headless_clipboard_client primary-source"),
        "headless runtime gate must run the native primary-selection source client"
    );
    assert!(
        script.contains("headless_clipboard_client primary-sink"),
        "headless runtime gate must run the native primary-selection sink client"
    );
    for marker in [
        "SLOPOS_PRIMARY_SELECTION_OFFER_VERIFIED",
        "SLOPOS_PRIMARY_SELECTION_TRANSFER_VERIFIED",
        "SLOPOS_PRIMARY_SELECTION_MISSING_MIME_EOF_VERIFIED",
    ] {
        assert!(
            script.contains(marker),
            "headless runtime gate must require primary-selection marker {marker}"
        );
    }
    for mode in ["primary-source", "primary-sink"] {
        assert!(
            client.contains(mode),
            "primary-selection client must expose mode {mode}"
        );
    }
    assert!(
        client.contains("zwp_primary_selection_device_manager_v1"),
        "primary-selection client must use the unstable Wayland primary-selection protocol"
    );
}

#[test]
fn headless_runtime_gate_exercises_native_text_input_and_ime() {
    let script = include_str!("../../../scripts/verify-compositor-headless-runtime.sh");
    let client = include_str!("../examples/headless_text_input_client.rs");
    assert!(
        script.contains("headless_text_input_client ime")
            && script.contains("headless_text_input_client app"),
        "headless runtime gate must run separate native input-method and app clients"
    );
    for marker in [
        "SLOPOS_TEXT_INPUT_APP_ENTER",
        "SLOPOS_TEXT_INPUT_PREEDIT_VERIFIED",
        "SLOPOS_TEXT_INPUT_COMMIT_VERIFIED",
        "SLOPOS_TEXT_INPUT_DELETE_VERIFIED",
        "SLOPOS_IME_ACTIVATE",
        "SLOPOS_TEXT_INPUT_SURROUNDING_VERIFIED",
        "SLOPOS_TEXT_INPUT_CONTENT_TYPE_VERIFIED",
        "SLOPOS_IME_COMMIT_SENT",
        "SLOPOS_IME_DEACTIVATE",
    ] {
        assert!(
            script.contains(marker),
            "headless runtime gate must require text-input marker {marker}"
        );
        assert!(
            client.contains(marker),
            "text-input client must emit text-input marker {marker}"
        );
    }
    assert!(
        client.contains("input_method.commit(state.done_count)"),
        "IME probe must use the server-provided done serial instead of a guessed value"
    );
    assert!(
        script.contains("\"schema\": 12"),
        "adding text-input evidence must bump the runtime evidence schema"
    );
}

#[test]
fn headless_runtime_gate_exercises_clipboard_cancellation_and_target_death() {
    let script = include_str!("../../../scripts/verify-compositor-headless-runtime.sh");
    let client = include_str!("../examples/headless_clipboard_client.rs");
    for command in [
        "headless_clipboard_client source",
        "headless_clipboard_client source-once",
        "headless_clipboard_client sink-abort",
    ] {
        assert!(
            script.contains(command),
            "headless runtime gate must run clipboard failure-path command {command}"
        );
    }
    for marker in [
        "SLOPOS_CLIPBOARD_SOURCE_CANCELLED",
        "SLOPOS_CLIPBOARD_TARGET_DEATH_RECOVERED",
        "SLOPOS_SELECTION_TARGET_DISCONNECTED",
    ] {
        assert!(
            script.contains(marker),
            "headless runtime gate must require clipboard failure-path marker {marker}"
        );
    }
    assert!(
        client.contains("sink-abort"),
        "clipboard client must expose failure-path mode sink-abort"
    );
    assert!(
        client.contains("SLOPOS_CLIPBOARD_SOURCE_CANCELLED"),
        "clipboard source must expose the protocol cancellation event"
    );
}

#[test]
fn headless_runtime_gate_exercises_only_safe_dnd_serial_rejection() {
    let script = include_str!("../../../scripts/verify-compositor-headless-runtime.sh");
    let client = include_str!("../examples/headless_clipboard_client.rs");
    assert!(
        script.contains("headless_clipboard_client dnd-invalid-serial"),
        "headless runtime gate must run the invalid-serial DnD smoke client"
    );
    assert!(
        script.contains("SLOPOS_DND_INVALID_SERIAL_REJECTED"),
        "headless runtime gate must require the invalid-serial DnD marker"
    );
    assert!(
        client.contains("data_device.start_drag(Some(&source), &surface, None, 0)"),
        "headless DnD smoke client must exercise the protocol's invalid serial path"
    );
    assert!(
        client.contains("SLOPOS_DND_INVALID_SERIAL_REJECTED serial=0 events=none"),
        "headless DnD smoke client must report explicit serial rejection"
    );
    assert!(
        client.contains("successful\n//! cross-client DnD"),
        "headless DnD smoke client must document that successful DnD remains unproved"
    );
}

#[test]
fn invalid_serial_dnd_smoke_does_not_advance_window_cascade() {
    let client = include_str!("../examples/headless_clipboard_client.rs");
    let mode = client
        .split("fn run_dnd_invalid_serial")
        .nth(1)
        .expect("invalid-serial DnD mode must exist");
    assert!(
        mode.contains("compositor.create_surface(&queue_handle, ())"),
        "invalid-serial DnD should use an unmapped origin surface"
    );
    assert!(
        !mode.contains("create_toplevel("),
        "invalid-serial DnD must not map a throwaway XDG window"
    );
}

#[test]
fn dnd_runtime_evidence_preserves_bare_and_invalid_serial_markers() {
    let script = include_str!("../../../scripts/verify-compositor-headless-runtime.sh");
    assert!(
        script.contains("grep -Eq \"^$1([[:space:]]|$)\""),
        "DnD evidence matching must accept markers with or without a payload"
    );
    assert!(
        script.contains("cat \"$dnd_source_log\" \"$dnd_target_log\" >>\"$dnd_log\""),
        "positive DnD evidence must preserve the earlier invalid-serial marker"
    );
}

#[test]
fn headless_runtime_gate_requires_cross_client_dnd_lifecycle_evidence() {
    let script = include_str!("../../../scripts/verify-compositor-headless-runtime.sh");
    let compositor = include_str!("../src/main.rs");
    for marker in [
        "SLOPOS_DND_CLIENT_STARTED",
        "SLOPOS_DND_ICON_ATTACHED",
        "SLOPOS_DND_DROPPED",
    ] {
        assert!(
            compositor.contains(marker),
            "compositor must emit DnD lifecycle marker {marker}"
        );
    }
    for marker in [
        "dnd_cross_client_client_started",
        "dnd_cross_client_drag_icon_verified",
        "dnd_cross_client_drop_verified",
    ] {
        assert!(
            script.contains(marker),
            "headless runtime evidence must record DnD field {marker}"
        );
    }
    assert!(
        script.contains("\"schema\": 12"),
        "adding DnD lifecycle evidence must retain the current runtime evidence schema"
    );
}

#[test]
fn xwayland_surfaces_have_a_first_class_scene_lifecycle_contract() {
    let compositor = include_str!("../src/main.rs");
    for marker in [
        "fn x11_surface_scene_origin",
        "fn x11_surface_scene_hit",
        "XWayland surface mapped into scene",
        "XWayland surface rendered",
        "XWayland surface unmapped from scene",
        "XWayland surface destroyed from scene",
    ] {
        assert!(
            compositor.contains(marker),
            "rootless XWayland scene integration must contain behavior marker {marker}"
        );
    }
}

#[test]
fn xwayland_interactive_grabs_use_the_authoritative_x11_scene() {
    let source = include_str!("../src/main.rs");
    for marker in [
        "x11_interactive_grab",
        "update_x11_interactive_grab",
        "x11_window_id",
        "XWayland interactive grab started",
        "XWayland surface geometry changed during grab",
    ] {
        assert!(
            source.contains(marker),
            "XWayland move/resize handling must contain the scene-authoritative marker {marker}"
        );
    }
}

#[test]
fn xwayland_windows_use_the_same_spaces_membership_authority_as_native_windows() {
    let source = include_str!("../src/main.rs");
    for marker in [
        "fn x11_space_window_id",
        "fn ensure_x11_space_membership",
        "fn remove_x11_space_membership",
        "fn x11_window_visible_on_space",
        "fn known_space_window_ids",
        "XWayland window assigned to authoritative Space",
    ] {
        assert!(
            source.contains(marker),
            "rootless XWayland Spaces behavior must use compositor membership marker {marker}"
        );
    }
}

#[test]
fn xwayland_override_redirect_surfaces_do_not_steal_keyboard_focus() {
    let source = include_str!("../src/main.rs");
    for marker in [
        "x11_surface_accepts_keyboard_focus",
        "override-redirect surface kept out of keyboard focus",
    ] {
        assert!(
            source.contains(marker),
            "override-redirect focus policy must be explicit in the compositor scene: {marker}"
        );
    }
}

#[test]
fn xwayland_startup_failure_has_bounded_recovery() {
    let source = include_str!("../src/main.rs");
    for marker in [
        "XWayland startup failed; restarting",
        "XWayland startup recovery budget exhausted",
    ] {
        assert!(
            source.contains(marker),
            "XWayland startup failure must have bounded recovery marker {marker}"
        );
    }
}

#[test]
fn xwayland_runtime_gate_uses_real_client_and_retains_honest_fields() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/verify-xwayland-scene.sh");
    let script = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    for marker in [
        "xmessage",
        "xwininfo -root -tree",
        "xwayland-startup-watchdog",
        "scene_mapping_verified",
        "scene_unmap_destroy_verified",
        "rendering_verified",
        "nested_dri3_available",
    ] {
        assert!(
            script.contains(marker),
            "XWayland runtime gate must retain real-client evidence marker {marker}"
        );
    }
}

#[test]
fn spaces_persistence_recovery_gate_requires_quarantine_and_default_restore() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/verify-spaces-persistence-recovery.sh");
    let script = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    for marker in [
        "invalid_persisted_path_still_present",
        "quarantined=",
        "default_spaces_restored=true",
        "qa_complete=true",
    ] {
        assert!(
            script.contains(marker),
            "Spaces persistence recovery gate must retain marker {marker}"
        );
    }
}

#[test]
fn headless_runtime_gate_exercises_dnd_target_disconnect_cancellation() {
    let script = include_str!("../../../scripts/verify-compositor-headless-runtime.sh");
    let client = include_str!("../examples/headless_dnd_client.rs");
    assert!(
        script.contains("headless_dnd_client target-abort"),
        "headless runtime gate must run a target-disconnect DnD client"
    );
    for marker in [
        "SLOPOS_DND_TARGET_ABORTING",
        "SLOPOS_DND_TARGET_DISCONNECTED",
        "SLOPOS_DND_SOURCE_CANCELLED",
    ] {
        assert!(
            script.contains(marker),
            "target-disconnect DnD gate must require marker {marker}"
        );
        assert!(
            client.contains(marker),
            "target-disconnect DnD client must emit marker {marker}"
        );
    }
    assert!(
        script.contains("dnd_target_disconnect_cancelled_verified")
            && script.contains("dnd_target_disconnect_target_exit_verified"),
        "runtime evidence must retain target-disconnect cancellation fields"
    );
    assert!(
        script.contains("Preserve those markers")
            && !script.contains("combine_dnd_logs() {\n  : >\"$dnd_log\""),
        "target-disconnect evidence must preserve the earlier invalid-serial marker"
    );
    assert!(
        client.contains("Role::TargetAbort") && client.contains("target-abort"),
        "client must expose an explicit target-abort mode"
    );
}

#[test]
fn headless_dnd_motion_points_leave_the_raised_source_buffer() {
    let script = include_str!("../../../scripts/verify-compositor-headless-runtime.sh");
    assert!(
        script
            .contains("send_headless_input '{\"Motion\":{\"x\":400,\"y\":110,\"time_msec\":120}}'"),
        "positive DnD must enter the target-only portion of its committed buffer"
    );
    assert!(
        script
            .contains("send_headless_input '{\"Motion\":{\"x\":410,\"y\":120,\"time_msec\":125}}'"),
        "positive DnD must deliver a second target motion outside the raised source buffer"
    );
}

#[test]
fn headless_dnd_target_allows_the_protocol_leave_after_drop() {
    let client = include_str!("../examples/headless_dnd_client.rs");
    assert!(
        client.contains("target_left_before_drop"),
        "DnD target must distinguish a valid post-drop leave from an early leave"
    );
    assert!(
        client.contains("state.target_left_before_drop || state.target_motion_count"),
        "DnD target validation must reject only a leave before drop"
    );
}

#[test]
fn headless_dnd_source_stays_alive_for_all_requested_mimes() {
    let client = include_str!("../examples/headless_dnd_client.rs");
    assert!(
        client.contains("state.source_drop_performed && state.source_send_count >= 2"),
        "DnD source must keep its data source alive until text and URI sends complete"
    );
}

#[test]
fn presentation_round_trip_preserves_the_original_normal_frame() {
    let normal = WindowGeometry::new(137, 91, 731, 509);
    let work_area = WindowGeometry::new(0, 24, 1600, 876);
    let output_area = WindowGeometry::new(0, 0, 1600, 900);

    let filled = transition_presentation_state(
        WindowPresentationState::Normal,
        normal,
        None,
        WindowPresentationState::Filled,
        work_area,
        output_area,
        None,
        "output-1",
        7,
    );
    assert_eq!(filled.state, WindowPresentationState::Filled);
    assert_eq!(filled.geometry, work_area);
    assert_eq!(
        filled
            .restore_state
            .as_ref()
            .expect("Fill must capture a restore frame")
            .normal_geometry,
        normal
    );

    let fullscreen = transition_presentation_state(
        filled.state,
        filled.geometry,
        filled.restore_state.as_ref(),
        WindowPresentationState::Fullscreen,
        work_area,
        output_area,
        None,
        "output-1",
        7,
    );
    assert_eq!(fullscreen.state, WindowPresentationState::Fullscreen);
    assert_eq!(fullscreen.geometry, output_area);
    assert_eq!(
        fullscreen
            .restore_state
            .as_ref()
            .expect("Fullscreen must retain the original restore frame")
            .normal_geometry,
        normal
    );

    let restored = transition_presentation_state(
        fullscreen.state,
        fullscreen.geometry,
        fullscreen.restore_state.as_ref(),
        WindowPresentationState::Normal,
        work_area,
        output_area,
        None,
        "output-1",
        7,
    );
    assert_eq!(restored.state, WindowPresentationState::Normal);
    assert_eq!(restored.geometry, normal);
    assert!(restored.restore_state.is_none());
    assert_eq!(
        restored
            .restored_from
            .as_ref()
            .expect("restore metadata must be exposed to the backend")
            .normal_geometry,
        normal
    );
}

#[test]
fn restore_after_output_change_clamps_the_saved_frame_into_the_new_work_area() {
    let old_normal = WindowGeometry::new(5000, -200, 2000, 1200);
    let old_work_area = WindowGeometry::new(0, 24, 3840, 2136);
    let old_output_area = WindowGeometry::new(0, 0, 3840, 2160);

    let fullscreen = transition_presentation_state(
        WindowPresentationState::Normal,
        old_normal,
        None,
        WindowPresentationState::Fullscreen,
        old_work_area,
        old_output_area,
        None,
        "DP-1",
        2,
    );

    let laptop_work_area = WindowGeometry::new(0, 24, 1280, 776);
    let laptop_output_area = WindowGeometry::new(0, 0, 1280, 800);
    let restored = transition_presentation_state(
        fullscreen.state,
        fullscreen.geometry,
        fullscreen.restore_state.as_ref(),
        WindowPresentationState::Normal,
        laptop_work_area,
        laptop_output_area,
        None,
        "eDP-1",
        2,
    );

    assert_eq!(restored.geometry, laptop_work_area);
    let metadata = restored
        .restored_from
        .expect("restore metadata must survive output migration");
    assert_eq!(metadata.normal_geometry, old_normal);
    assert_eq!(metadata.output_id, "DP-1");
    assert_eq!(metadata.space_id, 2);
}

#[test]
fn minimize_does_not_destroy_a_preexisting_restore_frame() {
    let normal = WindowGeometry::new(80, 70, 640, 480);
    let work_area = WindowGeometry::new(0, 22, 1280, 778);
    let output_area = WindowGeometry::new(0, 0, 1280, 800);

    let zoomed = transition_presentation_state(
        WindowPresentationState::Normal,
        normal,
        None,
        WindowPresentationState::SmartZoomed,
        work_area,
        output_area,
        Some((900, 650)),
        "output-1",
        1,
    );
    assert_eq!(zoomed.state, WindowPresentationState::SmartZoomed);

    let minimized = transition_presentation_state(
        zoomed.state,
        zoomed.geometry,
        zoomed.restore_state.as_ref(),
        WindowPresentationState::Minimized,
        work_area,
        output_area,
        None,
        "output-1",
        1,
    );
    assert_eq!(minimized.state, WindowPresentationState::Minimized);
    assert_eq!(minimized.geometry, zoomed.geometry);
    assert_eq!(
        minimized
            .restore_state
            .as_ref()
            .expect("minimize must retain the restore record")
            .normal_geometry,
        normal
    );

    let restored = transition_presentation_state(
        minimized.state,
        minimized.geometry,
        minimized.restore_state.as_ref(),
        WindowPresentationState::Normal,
        work_area,
        output_area,
        None,
        "output-1",
        1,
    );
    assert_eq!(restored.geometry, normal);
}

#[test]
fn odd_sized_tiling_partitions_the_work_area_without_gaps() {
    let area = WindowGeometry::new(11, 29, 1001, 701);
    let normal = WindowGeometry::new(100, 100, 500, 400);

    let left = calculate_presentation_geometry(
        area,
        WindowPresentationState::Tiled(TilePlacement::Left),
        None,
        normal,
    );
    let right = calculate_presentation_geometry(
        area,
        WindowPresentationState::Tiled(TilePlacement::Right),
        None,
        normal,
    );

    assert_eq!(left.x, area.x);
    assert_eq!(right.x, left.x + left.width);
    assert_eq!(left.width + right.width, area.width);
    assert_eq!(left.height, area.height);
    assert_eq!(right.height, area.height);

    let top_left = calculate_presentation_geometry(
        area,
        WindowPresentationState::Tiled(TilePlacement::TopLeft),
        None,
        normal,
    );
    let top_right = calculate_presentation_geometry(
        area,
        WindowPresentationState::Tiled(TilePlacement::TopRight),
        None,
        normal,
    );
    let bottom_left = calculate_presentation_geometry(
        area,
        WindowPresentationState::Tiled(TilePlacement::BottomLeft),
        None,
        normal,
    );
    let bottom_right = calculate_presentation_geometry(
        area,
        WindowPresentationState::Tiled(TilePlacement::BottomRight),
        None,
        normal,
    );

    assert_eq!(top_left.width + top_right.width, area.width);
    assert_eq!(bottom_left.width + bottom_right.width, area.width);
    assert_eq!(top_left.height + bottom_left.height, area.height);
    assert_eq!(top_right.height + bottom_right.height, area.height);
    assert_eq!(top_right.x, top_left.x + top_left.width);
    assert_eq!(bottom_right.x, bottom_left.x + bottom_left.width);
    assert_eq!(bottom_left.y, top_left.y + top_left.height);
    assert_eq!(bottom_right.y, top_right.y + top_right.height);
}

#[test]
fn tiling_stays_positive_and_inside_many_small_and_odd_work_areas() {
    let normal = WindowGeometry::new(-100, -100, 10_000, 10_000);
    let placements = [
        TilePlacement::Left,
        TilePlacement::Right,
        TilePlacement::TopLeft,
        TilePlacement::TopRight,
        TilePlacement::BottomLeft,
        TilePlacement::BottomRight,
    ];

    for width in 2..=65 {
        for height in 2..=65 {
            let area = WindowGeometry::new(17, 31, width, height);
            for placement in placements {
                let geometry = calculate_presentation_geometry(
                    area,
                    WindowPresentationState::Tiled(placement),
                    None,
                    normal,
                );
                assert!(geometry.width > 0, "{placement:?} width in {area:?}");
                assert!(geometry.height > 0, "{placement:?} height in {area:?}");
                assert!(geometry.x >= area.x, "{placement:?} x in {area:?}");
                assert!(geometry.y >= area.y, "{placement:?} y in {area:?}");
                assert!(
                    geometry.x + geometry.width <= area.x + area.width,
                    "{placement:?} right edge in {area:?}: {geometry:?}"
                );
                assert!(
                    geometry.y + geometry.height <= area.y + area.height,
                    "{placement:?} bottom edge in {area:?}: {geometry:?}"
                );
            }
        }
    }
}

#[test]
fn fixed_and_adaptive_frame_pacing_do_not_share_deadlines_or_samples() {
    let start = Instant::now();
    let mut scheduler = FrameScheduler::new(RefreshRate::Hz60);
    assert!(scheduler.record_frame_at(start));
    assert!(scheduler.record_frame_at(start + Duration::from_millis(16)));
    assert_eq!(scheduler.sample_count(), 1);
    assert_eq!(
        scheduler.time_until_next_frame_at(start + Duration::from_millis(20)),
        Duration::from_nanos(12_666_666)
    );
    assert!(scheduler
        .time_until_next_frame_at(start + Duration::from_millis(33))
        .is_zero());

    scheduler.set_refresh_rate(RefreshRate::Adaptive);
    assert_eq!(scheduler.sample_count(), 0);
    assert!(!scheduler.record_frame_at(start + Duration::from_millis(21)));
    assert!(scheduler
        .time_until_next_frame_at(start + Duration::from_secs(10))
        .is_zero());

    scheduler.set_refresh_rate(RefreshRate::Hz120);
    assert_eq!(scheduler.sample_count(), 0);
    assert!(scheduler
        .time_until_next_frame_at(start + Duration::from_secs(10))
        .is_zero());
}

#[test]
fn dynamic_spaces_keep_window_membership_valid_during_removal() {
    let mut spaces = SpacesModel::with_initial_name("Personal").unwrap();
    let personal = spaces.active_space();
    let work = spaces.create_space("Work").unwrap();
    let media = spaces.create_space("Media").unwrap();

    spaces
        .assign_window("finder-window", SpaceTarget::Named("Work".into()))
        .unwrap();
    spaces
        .assign_window("music-window", SpaceTarget::All)
        .unwrap();
    spaces.activate_space(work).unwrap();

    assert_eq!(spaces.window_spaces("finder-window"), vec![work]);
    assert_eq!(
        spaces.window_spaces("music-window"),
        vec![personal, work, media]
    );

    let fallback = spaces.remove_space(work).unwrap();
    assert_eq!(spaces.active_space(), fallback);
    assert!(spaces.space(work).is_none());
    assert_eq!(spaces.window_spaces("finder-window"), vec![fallback]);
    assert_eq!(spaces.window_spaces("music-window").len(), 2);
    spaces.validate().unwrap();
}

#[test]
fn application_id_spaces_policy_is_wired_in_both_compositor_backends() {
    let nested = include_str!("../src/main.rs");
    let drm = include_str!("../src/session_drm.rs");
    for source in [nested, drm] {
        assert!(source.contains("SetApplicationPolicy"));
        assert!(source.contains("application_target_from_wire"));
        assert!(source.contains("assign_window_for_application"));
        assert!(source.contains("reapply_application_policy"));
    }
    assert!(nested.contains("fn app_id_changed"));
    assert!(drm.contains("fn app_id_changed"));
}

#[test]
fn repeated_space_removal_never_strands_exclusive_or_all_space_windows() {
    let mut spaces = SpacesModel::with_initial_name("One").unwrap();
    let two = spaces.create_space("Two").unwrap();
    let three = spaces.create_space("Three").unwrap();
    let four = spaces.create_space("Four").unwrap();

    spaces
        .assign_window("exclusive-two", SpaceTarget::Named("Two".into()))
        .unwrap();
    spaces
        .assign_window("exclusive-three", SpaceTarget::Named("Three".into()))
        .unwrap();
    spaces
        .assign_window("everywhere", SpaceTarget::All)
        .unwrap();
    spaces.activate_space(three).unwrap();

    for removed in [three, two, four] {
        spaces.remove_space(removed).unwrap();
        spaces.validate().unwrap();
        for window in ["exclusive-two", "exclusive-three", "everywhere"] {
            assert!(
                !spaces.window_spaces(window).is_empty(),
                "{window} was stranded after removing {removed:?}"
            );
        }
    }
    assert_eq!(spaces.spaces().len(), 1);
    assert_eq!(spaces.window_spaces("everywhere").len(), 1);
}

#[test]
fn independent_display_spaces_migrate_without_changing_identity_or_order() {
    let mut spaces = SpacesModel::with_initial_name("Laptop").unwrap();
    let laptop = spaces.active_space();
    let external = spaces.create_space("External").unwrap();
    let reference_order = spaces.space_ids();

    spaces.set_multi_monitor_policy(MultiMonitorPolicy::IndependentPerDisplay);
    spaces.assign_space_to_output(laptop, "eDP-1").unwrap();
    spaces.assign_space_to_output(external, "DP-1").unwrap();

    assert_eq!(spaces.spaces_for_output("eDP-1").unwrap(), vec![laptop]);
    assert_eq!(spaces.spaces_for_output("DP-1").unwrap(), vec![external]);

    let migrated = spaces.migrate_output("DP-1", Some("HDMI-A-1")).unwrap();
    assert_eq!(migrated, vec![external]);
    assert_eq!(spaces.output_for_space(external), Some("HDMI-A-1"));
    assert_eq!(spaces.space_ids(), reference_order);
    spaces.validate().unwrap();
}

#[test]
fn overview_projection_tracks_reorder_and_active_state() {
    let mut spaces = SpacesModel::with_initial_name("One").unwrap();
    let one = spaces.active_space();
    let two = spaces.create_space("Two").unwrap();
    let three = spaces.create_space("Three").unwrap();

    spaces.reorder_space(three, 0).unwrap();
    spaces.activate_space(two).unwrap();

    let overview = spaces.overview_projection();
    assert_eq!(overview.len(), 3);
    assert_eq!(overview[0].id(), three);
    assert_eq!(overview[0].order(), 0);
    assert_eq!(overview[1].id(), one);
    assert_eq!(overview[2].id(), two);
    assert!(overview[2].is_active());
    assert_eq!(overview.iter().filter(|row| row.is_active()).count(), 1);
}

#[test]
fn three_finger_space_swipe_reducer_has_explicit_commit_contract() {
    let source = include_str!("../src/spaces.rs");
    for marker in [
        "WORKSPACE_SWIPE_MIN_DISTANCE",
        "WORKSPACE_SWIPE_HORIZONTAL_RATIO",
        "WorkspaceSwipeAction::Next",
        "WorkspaceSwipeAction::Previous",
        "self.fingers >= 3",
        "self.delta_x.is_sign_negative()",
        "*self = Self::default()",
    ] {
        assert!(
            source.contains(marker),
            "three-finger swipe policy must contain reducer marker {marker}"
        );
    }
}

#[test]
fn drm_libinput_swipes_forward_and_commit_authoritative_space_cycles() {
    let source = include_str!("../src/session_drm.rs");
    for marker in [
        "InputEvent::GestureSwipeBegin",
        "InputEvent::GestureSwipeUpdate",
        "InputEvent::GestureSwipeEnd",
        "pointer.gesture_swipe_begin",
        "pointer.gesture_swipe_update",
        "pointer.gesture_swipe_end",
        "self.workspace_swipe.update",
        "self.workspace_swipe.end(cancelled)",
        "WorkspaceSwipeAction::Next",
        "WorkspaceSwipeAction::Previous",
        "self.cycle_workspace_next()",
        "self.cycle_workspace_prev()",
        "if !self.locked",
    ] {
        assert!(
            source.contains(marker),
            "DRM gesture handling must contain compositor-authoritative marker {marker}"
        );
    }
}
