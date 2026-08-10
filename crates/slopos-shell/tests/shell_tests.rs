use slopos_shell::notification_center::NotificationPriority;
use slopos_shell::*;

#[test]
fn session_entry_documents_output_layout_and_backend_selection() {
    let script = include_str!("../../../scripts/start-slopos-i");
    assert!(script.contains("SLOPOS_OUTPUTS_LAYOUT"));
    assert!(script.contains("compositor selection"));
}

#[test]
fn daily_driver_checklist_does_not_require_ripgrep() {
    let script = include_str!("../../../scripts/verify_daily_driver_checklist.sh");
    assert!(!script.contains("rg -q"));
    assert!(script.contains("grep -Eq") || script.contains("grep -Fq"));
}

#[test]
fn test_shell_startup() {
    let shell = SloposI::startup().unwrap();
    assert_eq!(shell.workspace_manager.read().total, 8);
    assert!(server_has_desktop_8());
}

fn server_has_desktop_8() -> bool {
    let server = MenuServer::new();
    server
        .menus
        .iter()
        .flat_map(|m| m.items.iter())
        .any(|item| item.action_id == "workspace.switch.7")
}

#[test]
fn test_menu_server() {
    let server = MenuServer::new();
    assert_eq!(server.menus.len(), 6);
    let window_menu = server
        .menus
        .iter()
        .find(|menu| menu.title == "Window")
        .expect("window menu");
    assert!(window_menu
        .items
        .iter()
        .any(|item| item.action_id == "workspace.next"));
    assert!(window_menu
        .items
        .iter()
        .any(|item| item.action_id == "workspace.switch.0"));
}

#[test]
fn test_notification_center() {
    let mut nc = NotificationCenter::new();
    let id = nc.post(
        "com.test",
        "Alert",
        "This is an alert",
        NotificationPriority::High,
    );
    assert_eq!(nc.visible().len(), 1);
    nc.dismiss(&id);
    assert_eq!(nc.visible().len(), 0);
}

#[test]
fn test_session_manager() {
    let mut sm = SessionManager::new();
    sm.login("testuser");
    assert!(sm.logged_in);
    assert_eq!(sm.username, "testuser");
    sm.lock();
    assert!(sm.locked);
    sm.unlock();
    assert!(!sm.locked);
    sm.session_state
        .insert("active_app".to_string(), "finder".to_string());
    sm.save_state();

    let mut sm2 = SessionManager::new();
    sm2.restore_state();
    assert_eq!(sm2.username, "testuser");
    assert!(sm2.logged_in);
    assert!(!sm2.locked);
    assert!(sm2.restore_windows);
    assert_eq!(
        sm2.session_state.get("active_app").map(|s| s.as_str()),
        Some("finder")
    );
}
