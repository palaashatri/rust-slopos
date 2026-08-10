use slopos_bus::{
    ApplicationSpacePolicySnapshot, SpaceClassification, SpaceSnapshot, SpaceTargetWire,
    SpacesDisplayPolicy, SpacesSnapshot,
};
use slopos_shell::workspace_manager::WorkspaceManager;

#[test]
fn shell_reconciles_authoritative_spaces_snapshot() {
    let snapshot = SpacesSnapshot {
        session_epoch: 1,
        revision: 17,
        active_space: 22,
        multi_monitor_policy: SpacesDisplayPolicy::SharedSpan,
        application_policies: Vec::new(),
        spaces: vec![
            SpaceSnapshot {
                id: 11,
                order: 0,
                name: "Personal".to_string(),
                active: false,
                window_count: 1,
                wallpaper: None,
                appearance: None,
                classification: SpaceClassification::Normal,
                output_id: None,
            },
            SpaceSnapshot {
                id: 22,
                order: 1,
                name: "Projects".to_string(),
                active: true,
                window_count: 3,
                wallpaper: Some("wallpapers/projects.png".into()),
                appearance: Some("modern".into()),
                classification: SpaceClassification::Normal,
                output_id: None,
            },
            SpaceSnapshot {
                id: 31,
                order: 2,
                name: "Media".to_string(),
                active: false,
                window_count: 0,
                wallpaper: None,
                appearance: None,
                classification: SpaceClassification::Normal,
                output_id: None,
            },
        ],
    };

    let mut manager = WorkspaceManager::new();
    assert!(manager.apply_snapshot(&snapshot));

    assert_eq!(manager.total, 3);
    assert_eq!(
        manager
            .workspaces
            .iter()
            .map(|workspace| workspace.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Personal", "Projects", "Media"]
    );
    assert_eq!(
        manager
            .active_workspace()
            .expect("authoritative active Space")
            .name,
        "Projects"
    );

    let stale_snapshot = SpacesSnapshot {
        session_epoch: 1,
        revision: 16,
        active_space: 11,
        multi_monitor_policy: SpacesDisplayPolicy::SharedSpan,
        application_policies: Vec::new(),
        spaces: vec![SpaceSnapshot {
            id: 11,
            order: 0,
            name: "Stale state".to_string(),
            active: true,
            window_count: 0,
            wallpaper: None,
            appearance: None,
            classification: SpaceClassification::Normal,
            output_id: None,
        }],
    };
    assert!(!manager.apply_snapshot(&stale_snapshot));

    assert_eq!(manager.total, 3);
    assert_eq!(
        manager
            .active_workspace()
            .expect("latest authoritative Space")
            .name,
        "Projects"
    );
}

#[test]
fn shell_rejects_malformed_authoritative_spaces_snapshot() {
    let mut manager = WorkspaceManager::new();
    let malformed = SpacesSnapshot {
        session_epoch: 1,
        revision: 1,
        active_space: 11,
        multi_monitor_policy: SpacesDisplayPolicy::SharedSpan,
        application_policies: Vec::new(),
        spaces: vec![
            SpaceSnapshot {
                id: 11,
                order: 0,
                name: "Personal".to_string(),
                active: true,
                window_count: 0,
                wallpaper: None,
                appearance: None,
                classification: SpaceClassification::Normal,
                output_id: None,
            },
            SpaceSnapshot {
                id: 11,
                order: 2,
                name: "Duplicate".to_string(),
                active: false,
                window_count: 0,
                wallpaper: None,
                appearance: None,
                classification: SpaceClassification::Normal,
                output_id: None,
            },
        ],
    };

    assert!(!manager.apply_snapshot(&malformed));
    assert_eq!(manager.revision, 0);
    assert_eq!(manager.total, 8);
    assert_eq!(manager.active_id, 1);
}

#[test]
fn shell_cycle_keeps_stable_active_id_in_sync() {
    let mut manager = WorkspaceManager::new();
    manager.next();
    assert_eq!(manager.active, 1);
    assert_eq!(manager.active_id, 2);
    manager.previous();
    assert_eq!(manager.active, 0);
    assert_eq!(manager.active_id, 1);
}

#[test]
fn shell_rejects_duplicate_names_and_control_metadata() {
    let mut manager = WorkspaceManager::new();
    let duplicate_names = SpacesSnapshot {
        session_epoch: 1,
        revision: 1,
        active_space: 11,
        multi_monitor_policy: SpacesDisplayPolicy::SharedSpan,
        application_policies: Vec::new(),
        spaces: vec![
            SpaceSnapshot {
                id: 11,
                order: 0,
                name: "Projects".into(),
                active: true,
                window_count: 0,
                wallpaper: None,
                appearance: None,
                classification: SpaceClassification::Normal,
                output_id: None,
            },
            SpaceSnapshot {
                id: 22,
                order: 1,
                name: "projects".into(),
                active: false,
                window_count: 0,
                wallpaper: Some("wallpaper\nunsafe".into()),
                appearance: None,
                classification: SpaceClassification::Normal,
                output_id: None,
            },
        ],
    };

    assert!(!manager.apply_snapshot(&duplicate_names));
    assert_eq!(manager.revision, 0);
    assert_eq!(manager.total, 8);
}

#[test]
fn shell_accepts_lower_revision_after_compositor_session_epoch_changes() {
    let mut manager = WorkspaceManager::new();
    let first = SpacesSnapshot {
        session_epoch: 10,
        revision: 42,
        active_space: 11,
        multi_monitor_policy: SpacesDisplayPolicy::SharedSpan,
        application_policies: Vec::new(),
        spaces: vec![SpaceSnapshot {
            id: 11,
            order: 0,
            name: "Old session".into(),
            active: true,
            window_count: 0,
            wallpaper: None,
            appearance: None,
            classification: SpaceClassification::Normal,
            output_id: None,
        }],
    };
    assert!(manager.apply_snapshot(&first));

    let restarted = SpacesSnapshot {
        session_epoch: 11,
        revision: 1,
        active_space: 22,
        multi_monitor_policy: SpacesDisplayPolicy::IndependentPerDisplay,
        application_policies: vec![ApplicationSpacePolicySnapshot {
            app_id: "org.example.Editor".into(),
            target: SpaceTargetWire::Id { id: 22 },
        }],
        spaces: vec![SpaceSnapshot {
            id: 22,
            order: 0,
            name: "New session".into(),
            active: true,
            window_count: 2,
            wallpaper: Some("wallpapers/new.png".into()),
            appearance: Some("modern".into()),
            classification: SpaceClassification::Fullscreen,
            output_id: Some("DP-1".into()),
        }],
    };
    assert!(manager.apply_snapshot(&restarted));
    assert_eq!(manager.session_epoch, 11);
    assert_eq!(manager.revision, 1);
    assert_eq!(manager.active_id, 22);
    assert_eq!(
        manager.multi_monitor_policy,
        SpacesDisplayPolicy::IndependentPerDisplay
    );
    assert_eq!(
        manager.workspaces[0].wallpaper.as_deref(),
        Some("wallpapers/new.png")
    );
    assert_eq!(
        manager.workspaces[0].classification,
        SpaceClassification::Fullscreen
    );
    assert_eq!(
        manager.application_policies,
        vec![ApplicationSpacePolicySnapshot {
            app_id: "org.example.Editor".into(),
            target: SpaceTargetWire::Id { id: 22 },
        }]
    );
}

#[test]
fn shell_retains_valid_application_policies_and_rejects_malformed_projection_transactionally() {
    let mut manager = WorkspaceManager::new();
    let baseline = SpacesSnapshot {
        session_epoch: 20,
        revision: 7,
        active_space: 22,
        multi_monitor_policy: SpacesDisplayPolicy::SharedSpan,
        application_policies: vec![
            ApplicationSpacePolicySnapshot {
                app_id: "org.example.Editor".into(),
                target: SpaceTargetWire::Id { id: 22 },
            },
            ApplicationSpacePolicySnapshot {
                app_id: "org.example.Terminal".into(),
                target: SpaceTargetWire::All,
            },
        ],
        spaces: vec![
            SpaceSnapshot {
                id: 11,
                order: 0,
                name: "Personal".into(),
                active: false,
                window_count: 0,
                wallpaper: None,
                appearance: None,
                classification: SpaceClassification::Normal,
                output_id: None,
            },
            SpaceSnapshot {
                id: 22,
                order: 1,
                name: "Projects".into(),
                active: true,
                window_count: 0,
                wallpaper: None,
                appearance: None,
                classification: SpaceClassification::Normal,
                output_id: None,
            },
        ],
    };
    assert!(manager.apply_snapshot(&baseline));
    assert_eq!(manager.application_policies, baseline.application_policies);
    let expected_policies = manager.application_policies.clone();

    let mut stale_unknown_space = baseline.clone();
    stale_unknown_space.revision = 6;
    stale_unknown_space.application_policies[0].target = SpaceTargetWire::Id { id: 99 };
    assert!(!manager.apply_snapshot(&stale_unknown_space));
    assert_eq!(manager.revision, baseline.revision);
    assert_eq!(manager.application_policies, expected_policies);

    let mut duplicate = baseline.clone();
    duplicate.revision = 8;
    duplicate
        .application_policies
        .push(ApplicationSpacePolicySnapshot {
            app_id: "org.example.Editor".into(),
            target: SpaceTargetWire::All,
        });
    assert!(!manager.apply_snapshot(&duplicate));
    assert_eq!(manager.revision, baseline.revision);
    assert_eq!(manager.application_policies, expected_policies);

    let mut invalid_id = baseline.clone();
    invalid_id.revision = 8;
    invalid_id.application_policies[0].app_id = "org.example.\nEditor".into();
    assert!(!manager.apply_snapshot(&invalid_id));
    assert_eq!(manager.application_policies, expected_policies);

    let mut unknown_space = baseline.clone();
    unknown_space.revision = 8;
    unknown_space.application_policies[0].target = SpaceTargetWire::Id { id: 99 };
    assert!(!manager.apply_snapshot(&unknown_space));
    assert_eq!(manager.application_policies, expected_policies);

    let mut current_target = baseline;
    current_target.revision = 8;
    current_target.application_policies[0].target = SpaceTargetWire::Current;
    assert!(!manager.apply_snapshot(&current_target));
    assert_eq!(manager.application_policies, expected_policies);
}
