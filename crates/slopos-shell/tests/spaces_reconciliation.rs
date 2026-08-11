#[test]
fn openbox_contract_is_floating_x11_with_four_desktops() {
    let rc = include_str!("../../../assets/config/openbox/rc.xml");
    assert!(rc.contains("<name>slopos-openbox</name>"));
    assert!(rc.contains("<number>4</number>"));
    assert!(rc.contains("<top>26</top>"));
    assert!(rc.contains("<bottom>60</bottom>"));
    assert!(rc.contains("ToggleMaximize"));
}

#[test]
fn install_script_is_x11_only() {
    let installer = include_str!("../../../install.sh");
    assert!(installer.contains("slopos-session"));
    assert!(installer.contains("slopos-shell"));
    assert!(installer.contains("slopos-catalogue"));
    assert!(installer.contains("slopos-settings"));
    assert!(!installer.contains("slopos-compositor"));
    assert!(!installer.contains("share/wayland-sessions"));
}

#[test]
fn docker_qa_does_not_ignore_required_screenshots() {
    let qa = include_str!("../../../scripts/run-docker-qa.sh");
    assert!(!qa.contains("scrot -z artifacts/qa/screenshots/clean_desktop_1280x800.png || true"));
    assert!(qa.contains("Visual acceptance remains a separate human/vision review gate"));
}
