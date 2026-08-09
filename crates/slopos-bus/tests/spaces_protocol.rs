use slopos_bus::{
    ApplicationSpacePolicySnapshot, SessionControlListener, SessionControlRequest,
    SpaceClassification, SpaceSnapshot, SpaceTargetWire, SpacesControlCommand, SpacesDisplayPolicy,
    SpacesSnapshot,
};
use std::sync::Mutex;

static SESSION_RUNTIME_ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn spaces_control_commands_round_trip_through_session_control_json() {
    let commands = vec![
        SpacesControlCommand::Select { id: 11 },
        SpacesControlCommand::Create {
            name: "Projects".to_string(),
        },
        SpacesControlCommand::Rename {
            id: 11,
            name: "Personal".to_string(),
        },
        SpacesControlCommand::Reorder { id: 11, order: 2 },
        SpacesControlCommand::Remove { id: 11 },
        SpacesControlCommand::MoveWindow {
            window_id: "window-7".to_string(),
            target: SpaceTargetWire::Current,
        },
        SpacesControlCommand::MoveActiveWindow {
            target: SpaceTargetWire::Id { id: 22 },
        },
        SpacesControlCommand::MoveActiveWindowToOutput {
            output_id: "HDMI-A-1".to_string(),
        },
        SpacesControlCommand::SetWallpaper {
            id: 11,
            wallpaper: Some("wallpapers/work.png".to_string()),
        },
        SpacesControlCommand::SetAppearance {
            id: 11,
            appearance: Some("graphite".to_string()),
        },
        SpacesControlCommand::SetClassification {
            id: 11,
            classification: SpaceClassification::Fullscreen,
        },
        SpacesControlCommand::SetMultiMonitorPolicy {
            policy: SpacesDisplayPolicy::IndependentPerDisplay,
        },
        SpacesControlCommand::AssignOutput {
            id: 11,
            output_id: Some("DP-1".to_string()),
        },
        SpacesControlCommand::SetApplicationPolicy {
            app_id: "org.example.Editor".to_string(),
            target: SpaceTargetWire::Id { id: 22 },
        },
    ];

    for command in commands {
        let request = SessionControlRequest::Spaces { command };
        let encoded = serde_json::to_vec(&request).expect("encode Spaces request");
        let decoded = serde_json::from_slice::<SessionControlRequest>(&encoded)
            .expect("decode Spaces request");
        assert_eq!(decoded, request);
    }
}

#[test]
fn move_window_wire_shape_is_explicitly_tagged() {
    let request = SessionControlRequest::Spaces {
        command: SpacesControlCommand::MoveWindow {
            window_id: "window-7".to_string(),
            target: SpaceTargetWire::Id { id: 22 },
        },
    };
    let encoded = serde_json::to_value(&request).expect("encode move request");
    assert_eq!(
        encoded,
        serde_json::json!({
            "Spaces": {
                "command": {
                    "command": "move_window",
                    "window_id": "window-7",
                    "target": {"id": {"id": 22}}
                }
            }
        })
    );
    serde_json::from_value::<SessionControlRequest>(encoded).expect("decode move request");
}

#[test]
fn move_active_window_wire_shape_is_explicitly_tagged() {
    let request = SessionControlRequest::Spaces {
        command: SpacesControlCommand::MoveActiveWindow {
            target: SpaceTargetWire::Id { id: 22 },
        },
    };
    let encoded = serde_json::to_value(&request).expect("encode active-window move request");
    assert_eq!(
        encoded,
        serde_json::json!({
            "Spaces": {
                "command": {
                    "command": "move_active_window",
                    "target": {"id": {"id": 22}}
                }
            }
        })
    );
    serde_json::from_value::<SessionControlRequest>(encoded)
        .expect("decode active-window move request");
}

#[test]
fn move_active_window_rejects_missing_or_invalid_target() {
    let missing_target = serde_json::json!({
        "Spaces": {
            "command": {
                "command": "move_active_window"
            }
        }
    });
    assert!(serde_json::from_value::<SessionControlRequest>(missing_target).is_err());

    let invalid_target = serde_json::json!({
        "Spaces": {
            "command": {
                "command": "move_active_window",
                "target": {"unknown": {}}
            }
        }
    });
    assert!(serde_json::from_value::<SessionControlRequest>(invalid_target).is_err());
}

#[test]
fn move_active_window_to_output_wire_shape_is_explicitly_tagged() {
    let request = SessionControlRequest::Spaces {
        command: SpacesControlCommand::MoveActiveWindowToOutput {
            output_id: "HDMI-A-1".to_string(),
        },
    };
    let encoded = serde_json::to_value(&request).expect("encode output move request");
    assert_eq!(
        encoded,
        serde_json::json!({
            "Spaces": {
                "command": {
                    "command": "move_active_window_to_output",
                    "output_id": "HDMI-A-1"
                }
            }
        })
    );
    serde_json::from_value::<SessionControlRequest>(encoded).expect("decode output move request");
}

#[test]
fn move_active_window_to_output_rejects_missing_identifier() {
    let missing_output = serde_json::json!({
        "Spaces": {
            "command": {
                "command": "move_active_window_to_output"
            }
        }
    });
    assert!(serde_json::from_value::<SessionControlRequest>(missing_output).is_err());
}

#[test]
fn spaces_snapshot_round_trip_preserves_revision_order_active_and_counts() {
    let snapshot = SpacesSnapshot {
        session_epoch: 101,
        revision: 9,
        active_space: 22,
        multi_monitor_policy: SpacesDisplayPolicy::IndependentPerDisplay,
        application_policies: vec![ApplicationSpacePolicySnapshot {
            app_id: "org.example.Editor".to_string(),
            target: SpaceTargetWire::Id { id: 22 },
        }],
        spaces: vec![
            SpaceSnapshot {
                id: 11,
                order: 0,
                name: "Personal".to_string(),
                active: false,
                window_count: 2,
                wallpaper: Some("wallpapers/personal.png".into()),
                appearance: Some("classic".into()),
                classification: SpaceClassification::Normal,
                output_id: Some("DP-1".into()),
            },
            SpaceSnapshot {
                id: 22,
                order: 1,
                name: "Projects".to_string(),
                active: true,
                window_count: 4,
                wallpaper: None,
                appearance: Some("graphite".into()),
                classification: SpaceClassification::Fullscreen,
                output_id: Some("DP-2".into()),
            },
        ],
    };

    let encoded = serde_json::to_vec(&snapshot).expect("encode Spaces snapshot");
    let decoded =
        serde_json::from_slice::<SpacesSnapshot>(&encoded).expect("decode Spaces snapshot");
    assert_eq!(decoded, snapshot);
}

#[test]
fn legacy_spaces_snapshot_defaults_new_authority_fields() {
    let decoded: SpacesSnapshot = serde_json::from_str(
        r#"{
            "revision": 3,
            "active_space": 11,
            "spaces": [{
                "id": 11,
                "order": 0,
                "name": "Desktop 1",
                "active": true,
                "window_count": 0
            }]
        }"#,
    )
    .expect("legacy snapshot remains readable");

    assert_eq!(decoded.session_epoch, 0);
    assert_eq!(
        decoded.multi_monitor_policy,
        SpacesDisplayPolicy::SharedSpan
    );
    assert_eq!(
        decoded.spaces[0].classification,
        SpaceClassification::Normal
    );
    assert_eq!(decoded.spaces[0].wallpaper, None);
    assert_eq!(decoded.spaces[0].output_id, None);
}

#[cfg(unix)]
#[test]
fn spaces_snapshot_writer_replaces_runtime_file_atomically() {
    let _guard = SESSION_RUNTIME_ENV_LOCK.lock().expect("runtime env lock");
    let runtime = std::path::PathBuf::from(format!(
        "/tmp/slsp-write-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock after epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&runtime).expect("create runtime");
    let previous = std::env::var_os("SLOPOS_SESSION_RUNTIME_DIR");
    std::env::set_var("SLOPOS_SESSION_RUNTIME_DIR", &runtime);
    let snapshot = SpacesSnapshot {
        session_epoch: 7,
        revision: 3,
        active_space: 22,
        multi_monitor_policy: SpacesDisplayPolicy::SharedSpan,
        application_policies: Vec::new(),
        spaces: vec![SpaceSnapshot {
            id: 22,
            order: 0,
            name: "Projects".into(),
            active: true,
            window_count: 1,
            wallpaper: None,
            appearance: None,
            classification: SpaceClassification::Normal,
            output_id: None,
        }],
    };
    slopos_bus::write_spaces_snapshot(&snapshot).expect("write Spaces snapshot");
    assert_eq!(
        slopos_bus::read_spaces_snapshot().expect("read Spaces snapshot"),
        snapshot
    );
    assert!(!std::fs::read_dir(&runtime)
        .expect("read runtime")
        .filter_map(Result::ok)
        .any(|entry| entry.file_name().to_string_lossy().contains(".tmp")));
    if let Some(previous) = previous {
        std::env::set_var("SLOPOS_SESSION_RUNTIME_DIR", previous);
    } else {
        std::env::remove_var("SLOPOS_SESSION_RUNTIME_DIR");
    }
    let _ = std::fs::remove_dir_all(runtime);
}

#[cfg(unix)]
#[test]
fn session_control_listener_drains_a_typed_spaces_command() {
    use std::os::unix::net::UnixDatagram;

    let runtime = std::path::PathBuf::from(format!(
        "/tmp/slsp-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock after epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&runtime).expect("create test runtime");
    let listener = SessionControlListener::bind(&runtime).expect("bind session control socket");
    let sender = UnixDatagram::unbound().expect("create datagram sender");
    let request = SessionControlRequest::Spaces {
        command: SpacesControlCommand::Reorder { id: 22, order: 0 },
    };
    sender
        .send_to(
            &serde_json::to_vec(&request).expect("encode request"),
            runtime.join(slopos_bus::SESSION_CONTROL_SOCKET),
        )
        .expect("send Spaces request");

    assert_eq!(listener.drain(), vec![request]);
    drop(listener);
    let _ = std::fs::remove_dir(runtime);
}
