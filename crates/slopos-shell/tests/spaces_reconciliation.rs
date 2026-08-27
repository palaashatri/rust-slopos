#[test]
fn openbox_contract_is_floating_x11_with_four_desktops() {
    let rc = include_str!("../../../assets/config/openbox/rc.xml");
    assert!(rc.contains("<name>slopos-openbox</name>"));
    assert!(rc.contains("<number>4</number>"));
    assert!(rc.contains("<top>26</top>"));
    assert!(rc.contains("<bottom>0</bottom>"));
    assert!(!rc.contains("Application Strip"));
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
fn classic_session_has_a_real_desktop_object_manager() {
    let start = include_str!("../../../scripts/start-slopos-i");
    assert!(start.contains("pcmanfm --profile=\"$SLOPOS_DESKTOP_PROFILE\" --desktop"));
    assert!(start.contains("desktop-items-0.conf"));
    assert!(start.contains("desktop_bg=\u{24}desktop_bg"));
    assert!(start.contains("desktop_font=Liberation Sans 10"));
    assert!(start.contains("show_wm_menu=1"));
    assert!(start.contains("[slopos-home.desktop]"));
    assert!(start.contains("[slopos-network.desktop]"));
    assert!(start.contains("[slopos-documents.desktop]"));
    assert!(start.contains("[slopos-trash.desktop]"));
    assert!(start.contains("Name=$name"));
    assert!(start.contains("\"My Home\""));
    assert!(start.contains("\"Network\""));
    assert!(start.contains("\"Documents\""));
    assert!(start.contains("\"Trash\""));
    assert!(start.contains("x=$right_x"));
    assert!(start.contains("pcmanfm --profile=\"$SLOPOS_DESKTOP_PROFILE\" --desktop-off"));
}

#[test]
fn wallpaper_updates_target_the_visible_desktop_surface() {
    let wallpaper = include_str!("../../../scripts/slopos-wallpaper");
    assert!(wallpaper.contains("SLOPOS_DESKTOP_PROFILE"));
    assert!(wallpaper.contains("--set-wallpaper=\"$image_path\""));
    assert!(wallpaper.contains("--wallpaper-mode=\"$pcm_mode\""));
    assert!(wallpaper.contains("pgrep -x pcmanfm"));
}

#[test]
fn docker_qa_does_not_ignore_required_screenshots() {
    let qa = include_str!("../../../scripts/run-docker-qa.sh");
    assert!(qa.contains("rm -f artifacts/qa/screenshots/*.png"));
    assert!(qa.contains("capture_screenshot artifacts/qa/screenshots/clean_desktop_1280x800.png"));
    assert!(!qa.contains("scrot -z artifacts/qa/screenshots/clean_desktop_1280x800.png || true"));
    assert!(qa.contains("Visual acceptance remains a separate human/vision review gate"));
}
