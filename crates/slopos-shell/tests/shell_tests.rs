#[test]
fn pivot_workspace_has_only_current_product_crates() {
    let cargo = include_str!("../../../Cargo.toml");
    assert!(cargo.contains("crates/slopos-session"));
    assert!(cargo.contains("crates/slopos-shell"));
    assert!(cargo.contains("crates/slopos-catalogue"));
    assert!(cargo.contains("crates/slopos-settings"));
    assert!(!cargo.contains("slopos-compositor"));
    assert!(!cargo.contains("smithay"));
}

#[test]
fn launcher_hotkey_targets_existing_shell() {
    let openbox = include_str!("../../../assets/config/openbox/rc.xml");
    assert!(openbox.contains("pkill -USR1 -x slopos-shell"));
    assert!(!openbox.contains("slopos-shell --toggle-launcher"));
}

#[test]
fn shell_does_not_ship_apple_logo_glyph() {
    let topbar = include_str!("../src/topbar.rs");
    assert!(!topbar.contains(''));
}

#[test]
fn appimage_installer_has_no_stub_fallback() {
    let installer = include_str!("../../slopos-catalogue/src/installer.rs");
    assert!(!installer.contains("create_stub_appimage"));
    assert!(installer.contains("metadata_is_installable"));
    assert!(installer.contains("SHA-256 mismatch"));
}
