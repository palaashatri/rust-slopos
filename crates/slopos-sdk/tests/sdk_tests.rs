use slopos_sdk::build_menu;
use slopos_sdk::global_menu_mode_enabled;
use slopos_sdk::Application;
use slopos_sdk::MenuManifest;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn test_app_creation() {
    let app = Application::new("TestApp", "com.test.app");
    assert_eq!(app.name, "TestApp");
    assert_eq!(app.bundle_id, "com.test.app");
    assert_eq!(app.initial_size.width, 960.0);
    assert_eq!(app.initial_size.height, 640.0);
    assert!(!app.running);
}

#[test]
fn test_app_initial_size_can_be_configured() {
    let mut app = Application::new("TestApp", "com.test.app");

    app.set_initial_size(slopos_kit::Size::new(1280.0, 800.0));

    assert_eq!(app.initial_size.width, 1280.0);
    assert_eq!(app.initial_size.height, 800.0);
}

#[test]
fn test_global_menu_mode_env_flag() {
    std::env::remove_var("SLOPOS_GLOBAL_MENU");
    assert!(!global_menu_mode_enabled());

    std::env::set_var("SLOPOS_GLOBAL_MENU", "1");
    assert!(global_menu_mode_enabled());

    std::env::set_var("SLOPOS_GLOBAL_MENU", "true");
    assert!(global_menu_mode_enabled());

    std::env::set_var("SLOPOS_GLOBAL_MENU", "0");
    assert!(!global_menu_mode_enabled());

    std::env::remove_var("SLOPOS_GLOBAL_MENU");
}

#[test]
fn test_app_publishes_menu_manifest() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("slopos-i_menu_manifest_{unique}"));
    std::env::set_var("SLOPOS_MENU_MANIFEST_DIR", &dir);

    let mut app = Application::new("TestApp", "com.test.app");
    let mut file_menu = build_menu("File");
    file_menu.add_action("New").with_action("test.new");
    app.set_menus(vec![file_menu]);

    let path = app
        .publish_menu_manifest()
        .expect("menu manifest publish should not fail")
        .expect("apps with menus should publish a manifest");
    let manifest: MenuManifest = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();

    assert_eq!(manifest.app_name, "TestApp");
    assert_eq!(manifest.bundle_id, "com.test.app");
    assert_eq!(manifest.menus[0].title, "TestApp");
    assert_eq!(manifest.menus[1].title, "File");
    assert_eq!(manifest.menus[1].items[0].action_id, "test.new");

    let _ = fs::remove_dir_all(&dir);
    std::env::remove_var("SLOPOS_MENU_MANIFEST_DIR");
}

#[test]
fn test_app_menu_manifest_generates_missing_action_ids() {
    let mut app = Application::new("TextEdit", "com.slopos.textedit");
    let mut file_menu = build_menu("File");
    file_menu.add_action("Save As...");
    file_menu
        .add_action("Explicit")
        .with_action("com.slopos.textedit.file.explicit");
    app.set_menus(vec![file_menu]);

    let manifest = app.menu_manifest();

    assert_eq!(
        manifest.menus[0].items[0].action_id,
        "com.slopos.textedit.textedit.about_textedit"
    );
    assert_eq!(
        manifest.menus[0].items[2].action_id,
        "com.slopos.textedit.textedit.hide_textedit"
    );
    assert_eq!(
        manifest.menus[0].items[4].action_id,
        "com.slopos.textedit.textedit.quit_textedit"
    );
    assert_eq!(
        manifest.menus[1].items[0].action_id,
        "com.slopos.textedit.file.save_as"
    );
    assert_eq!(
        manifest.menus[1].items[1].action_id,
        "com.slopos.textedit.file.explicit"
    );
}

#[test]
fn test_mouse_button_mapping() {
    assert_eq!(
        slopos_sdk::winit_to_retro_mouse_button(winit::event::MouseButton::Left),
        Some(slopos_kit::event::MouseButton::Left)
    );
    assert_eq!(
        slopos_sdk::winit_to_retro_mouse_button(winit::event::MouseButton::Other(42)),
        None
    );
}

#[test]
fn test_scroll_delta_mapping() {
    let line_delta = slopos_sdk::winit_to_retro_scroll_delta(
        winit::event::MouseScrollDelta::LineDelta(1.0, -2.0),
    );
    assert_eq!(line_delta.x, 16.0);
    assert_eq!(line_delta.y, -32.0);

    let pixel_delta = slopos_sdk::winit_to_retro_scroll_delta(
        winit::event::MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition::new(3.5, -4.0)),
    );
    assert_eq!(pixel_delta.x, 3.5);
    assert_eq!(pixel_delta.y, -4.0);
}
