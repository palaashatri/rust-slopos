use std::collections::HashMap;

use slopos_compositor::frame_timing::RefreshRate;
use slopos_compositor::hdr::ColorSpace;
use slopos_compositor::{
    cascade_position, layout_outputs_side_by_side, move_to_top, next_cascade_offset,
    outputs_from_env_values, parse_key_value_conf, parse_outputs_spec, selection_bytes_for_mime,
    selection_bytes_for_mime_with_text_fallback, topmost_window_at, total_output_size,
    DisplayPolicy, OutputConfig, WindowGeometry, CASCADE_WRAP, DEFAULT_OUTPUT_H, DEFAULT_OUTPUT_W,
    DEFAULT_WINDOW_H, DEFAULT_WINDOW_W,
};

#[test]
fn window_contains_point_inside() {
    let win = WindowGeometry::new(10, 20, 100, 80);

    assert!(win.contains_f64(60.0, 60.0));
}

#[test]
fn window_contains_top_left_corner() {
    let win = WindowGeometry::new(10, 20, 100, 80);

    assert!(win.contains_f64(10.0, 20.0));
}

#[test]
fn window_rejects_bottom_right_exclusive_boundary() {
    let win = WindowGeometry::new(10, 20, 100, 80);

    assert!(!win.contains_f64(110.0, 100.0));
}

#[test]
fn zero_sized_window_contains_nothing() {
    let win = WindowGeometry::new(5, 5, 0, 0);

    assert!(!win.contains_f64(5.0, 5.0));
    assert!(!win.contains_f64(4.0, 4.0));
    assert!(!win.contains_f64(6.0, 6.0));
}

#[test]
fn topmost_window_at_prefers_later_z_order() {
    let windows = vec![
        WindowGeometry::new(0, 0, 200, 200),
        WindowGeometry::new(50, 50, 200, 200),
        WindowGeometry::new(100, 100, 200, 200),
    ];

    assert_eq!(topmost_window_at(&windows, 125.0, 125.0), Some(2));
    assert_eq!(topmost_window_at(&windows, 75.0, 75.0), Some(1));
    assert_eq!(topmost_window_at(&windows, 25.0, 25.0), Some(0));
    assert_eq!(topmost_window_at(&windows, 500.0, 500.0), None);
}

#[test]
fn move_to_top_moves_target_to_end_and_preserves_other_order() {
    let mut windows = vec![
        WindowGeometry::new(1, 0, 50, 50),
        WindowGeometry::new(2, 0, 50, 50),
        WindowGeometry::new(3, 0, 50, 50),
    ];

    move_to_top(&mut windows, 0);

    assert_eq!(
        windows.iter().map(|win| win.x).collect::<Vec<_>>(),
        vec![2, 3, 1]
    );
}

#[test]
fn cascade_position_uses_classic_offset_and_wraps() {
    assert_eq!(cascade_position(0), (64, 64));
    assert_eq!(cascade_position(32), (96, 96));
    assert_eq!(next_cascade_offset(CASCADE_WRAP - 32), 0);
}

#[test]
fn output_config_uses_defaults_for_missing_or_invalid_values() {
    assert_eq!(
        OutputConfig::from_env_values(None, None),
        OutputConfig::default()
    );
    assert_eq!(
        OutputConfig::from_env_values(Some("wide".into()), Some("-1".into())),
        OutputConfig::default()
    );
}

#[test]
fn output_config_accepts_positive_env_values() {
    assert_eq!(
        OutputConfig::from_env_values(Some("1280".into()), Some("800".into())),
        OutputConfig {
            width: 1280,
            height: 800,
        }
    );
}

#[test]
fn defaults_match_runtime_contract() {
    assert_eq!((DEFAULT_OUTPUT_W, DEFAULT_OUTPUT_H), (1024, 768));
    assert_eq!((DEFAULT_WINDOW_W, DEFAULT_WINDOW_H), (640, 480));
}

#[test]
fn parse_outputs_spec_multi_and_layout() {
    let outs = parse_outputs_spec("1024x768,800x600");
    assert_eq!(outs.len(), 2);
    let laid = layout_outputs_side_by_side(&outs);
    assert_eq!(laid[0].x, 0);
    assert_eq!(laid[1].x, 1024);
    assert_eq!(
        total_output_size(&laid),
        OutputConfig {
            width: 1824,
            height: 768
        }
    );
}

#[test]
fn outputs_from_env_values_single_fallback() {
    let single = outputs_from_env_values(None, Some("1280".into()), Some("800".into()));
    assert_eq!(single.len(), 1);
    assert_eq!(
        single[0],
        OutputConfig {
            width: 1280,
            height: 800
        }
    );
}

#[test]
fn outputs_from_env_values_multi_ignores_width_height() {
    let multi = outputs_from_env_values(
        Some("640x480,320x240".into()),
        Some("1".into()),
        Some("1".into()),
    );
    assert_eq!(multi.len(), 2);
    assert_eq!(multi[0].width, 640);
    assert_eq!(multi[1].height, 240);
}

#[test]
fn display_policy_from_settings_text() {
    let mut policy = DisplayPolicy::default();
    policy.apply_settings_text(
        "# comment\nhdr_requested=true\nvrr_adaptive=false\nrefresh_rate=144hz\ncolor_space=scrgb\n",
    );
    assert!(policy.hdr_requested);
    assert!(!policy.vrr_adaptive);
    assert_eq!(policy.refresh_rate, RefreshRate::Hz144);
    assert_eq!(policy.color_space, ColorSpace::ScRgb);
    assert_eq!(policy.effective_refresh_rate(), RefreshRate::Hz144);
}

#[test]
fn display_policy_env_overrides_and_vrr() {
    let mut policy = DisplayPolicy::default();
    policy.apply_settings_text("refresh_rate=60hz\nvrr_adaptive=true\n");
    assert_eq!(policy.effective_refresh_rate(), RefreshRate::Adaptive);

    let mut env = HashMap::new();
    env.insert("SLOPOS_VRR".into(), "0".into());
    env.insert("SLOPOS_REFRESH".into(), "120".into());
    env.insert("SLOPOS_COLOR_SPACE".into(), "rec2020".into());
    env.insert("SLOPOS_HDR".into(), "yes".into());
    policy.apply_env_map(env);
    assert!(policy.hdr_requested);
    assert!(!policy.vrr_adaptive);
    assert_eq!(policy.refresh_rate, RefreshRate::Hz120);
    assert_eq!(policy.color_space, ColorSpace::Rec2020);
}

#[test]
fn selection_store_lookup_eof_when_missing() {
    let mut store = HashMap::new();
    store.insert("text/plain".into(), b"clip".to_vec());
    assert_eq!(
        selection_bytes_for_mime(&store, "text/plain"),
        Some(b"clip".as_slice())
    );
    assert_eq!(selection_bytes_for_mime(&store, "image/png"), None);
    assert_eq!(
        selection_bytes_for_mime_with_text_fallback(&store, "TEXT"),
        Some(b"clip".as_slice())
    );
}

#[test]
fn parse_key_value_conf_skips_comments() {
    let pairs = parse_key_value_conf("a=1\n#b=2\n\nc = 3\n");
    assert_eq!(
        pairs,
        vec![("a".into(), "1".into()), ("c".into(), "3".into())]
    );
}
