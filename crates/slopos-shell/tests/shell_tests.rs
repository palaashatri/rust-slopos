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
fn topbar_uses_packaged_slopos_mark_with_fallback() {
    let topbar = include_str!("../src/topbar.rs");
    assert!(topbar.contains("assets/slopos-logo.png"));
    assert!(topbar.contains("new_subpixbuf"));
    assert!(topbar.contains("scale_simple(20, 20, InterpType::Bilinear)"));
    assert!(topbar.contains("set_label(\"S\")"));
}

#[test]
fn catalogue_uses_packaged_fallback_icon() {
    let catalogue = include_str!("../../slopos-catalogue/src/main.rs");
    assert!(catalogue.contains("load_catalogue_icon"));
    assert!(catalogue.contains("software.svg"));
    assert!(catalogue.contains("from_file_at_scale"));
    assert!(catalogue.contains("application-x-executable"));
}

#[test]
fn appimage_installer_has_no_stub_fallback() {
    let installer = include_str!("../../slopos-catalogue/src/installer.rs");
    let model = include_str!("../../slopos-catalogue/src/model.rs");
    assert!(!installer.contains("create_stub_appimage"));
    assert!(installer.contains("metadata_is_installable"));
    assert!(installer.contains("SHA-256 mismatch"));
    assert!(model.contains("eq_ignore_ascii_case(EMPTY_FILE_SHA256)"));
}

#[test]
fn shipping_manifests_are_x11_only_and_complete() {
    let manifests = [
        include_str!("../../../install.sh"),
        include_str!("../../../packaging/arch/PKGBUILD"),
        include_str!("../../../packaging/debian/rules"),
        include_str!("../../../packaging/iso/build-iso.sh"),
    ];
    for manifest in manifests {
        for binary in [
            "slopos-session",
            "slopos-shell",
            "slopos-catalogue",
            "slopos-settings",
        ] {
            assert!(manifest.contains(binary), "missing {binary} in manifest");
        }
        assert!(!manifest.to_ascii_lowercase().contains("wayland"));
        assert!(!manifest.to_ascii_lowercase().contains("smithay"));
        assert!(!manifest.to_ascii_lowercase().contains("wlroots"));
        assert!(!manifest.to_ascii_lowercase().contains("xwayland"));
        assert!(!manifest.contains("slopos-compositor"));
    }

    let package_manifests = [
        include_str!("../../../packaging/arch/PKGBUILD"),
        include_str!("../../../packaging/debian/rules"),
        include_str!("../../../packaging/iso/build-iso.sh"),
    ];
    for manifest in package_manifests {
        for asset in [
            "share/xsessions/slopos-i.desktop",
            "assets/config/openbox/rc.xml",
            "themes/slopos-openbox/openbox-3/themerc",
            "assets/config/gtk-3.0/gtk.css",
            "assets/slopos-logo.png",
        ] {
            assert!(
                manifest.contains(asset),
                "missing {asset} in package manifest"
            );
        }
    }

    let iso = include_str!("../../../packaging/iso/build-iso.sh");
    assert!(iso.contains("assets/config/mimeapps.list"));
    assert!(iso.contains("sub(/\\r$/, \"\")"));
    assert!(iso.contains("systemd-sysusers --root=\"$ROOTFS\""));
    assert!(iso.contains("systemd-tmpfiles --root=\"$ROOTFS\" --create"));
    assert!(iso.contains("passwd --root \"$ROOTFS\" --delete slopos"));
    assert!(iso.contains("u slopos 1000"));
    assert!(iso.contains("file_permissions[\"/usr/local/bin/slopos-session\"]=\"0:0:755\""));
    assert!(iso.contains("chmod 0755"));
    assert!(iso.contains("s|^#autologin-user=.*|autologin-user=slopos|"));
    assert!(iso.contains("s|^#autologin-session=.*|autologin-session=slopos-i|"));
    assert!(include_str!("../../../install.sh").contains("assets/slopos-logo.png"));
}

#[test]
fn dependency_manifests_do_not_reintroduce_removed_display_stack() {
    for manifest in [
        include_str!("../../../packaging/deps/arch.txt"),
        include_str!("../../../packaging/deps/ubuntu.txt"),
        include_str!("../../../packaging/deps/arch-build.txt"),
        include_str!("../../../packaging/deps/ubuntu-build.txt"),
        include_str!("../../../packaging/vm/arch-install.sh"),
    ] {
        let manifest = manifest.to_ascii_lowercase();
        assert!(!manifest.contains("wayland"));
        assert!(!manifest.contains("smithay"));
        assert!(!manifest.contains("wlroots"));
        assert!(!manifest.contains("xwayland"));
        assert!(!manifest.contains("slopos-compositor"));
    }
}

#[test]
fn arch_build_manifest_has_native_gui_dependencies() {
    let manifest = include_str!("../../../packaging/deps/arch-build.txt");
    for package in [
        "gtk3",
        "gdk-pixbuf2",
        "libx11",
        "libxrandr",
        "openssl",
        "dbus",
    ] {
        assert!(manifest.lines().any(|line| line.trim() == package));
    }
}

#[test]
fn custom_prefix_session_resources_are_forwarded() {
    let launcher = include_str!("../../../scripts/start-slopos-i");
    assert!(launcher.contains("SLOPOS_INSTALL_PREFIX"));
    assert!(launcher.contains("SLOPOS_SHARE_DIR"));
    assert!(launcher.contains("$INSTALL_PREFIX/bin/$name"));

    let installer = include_str!("../../../install.sh");
    assert!(installer.contains("XSESSION_DIR=\"${XSESSION_DIR:-/usr/share/xsessions}\""));
    assert!(installer.contains("--session-dir \"$XSESSION_DIR\""));

    let session_files = include_str!("../../../scripts/install-session-files.sh");
    assert!(session_files.contains("install_session_descriptor"));
    assert!(session_files.contains("$PREFIX/bin/slopos-session"));

    let session = include_str!("../../slopos-session/src/main.rs");
    assert!(session.contains("SLOPOS_SHARE_DIR"));
    assert!(session.contains("share/slopos-i/openbox/rc.xml"));

    let topbar = include_str!("../src/topbar.rs");
    assert!(topbar.contains("SLOPOS_SHARE_DIR"));
}

#[test]
fn vm_qa_requires_screenshot_evidence() {
    for qa in [
        include_str!("../../../packaging/vm/qa-vm.sh"),
        include_str!("../../../packaging/vm/qa-live.sh"),
    ] {
        assert!(qa.contains("command -v scrot"));
        assert!(qa.contains("test -s"));
        assert!(!qa.contains("if command -v scrot"));
    }
}

#[test]
fn docker_qa_uses_fresh_visible_windows() {
    let qa = include_str!("../../../scripts/run-docker-qa.sh");
    assert!(qa.contains("dbus-run-session -- ./target/release/slopos-session"));
    assert!(qa.contains("wait_visible_window '^SLOPOS Top Bar$'"));
    assert!(qa.contains("wait_visible_window '^SLOPOS Application Strip$'"));
    assert!(qa.contains("xdotool search --onlyvisible --name \"$pattern\""));
    assert!(qa.contains("--onlyvisible --name '^Software Catalogue$'"));
    assert!(qa.contains("--onlyvisible --name '^System Settings$'"));
    assert!(qa.contains("getwindowpid"));
    assert!(qa.contains("Verify session recovery after child failure"));
    assert!(qa.contains("shell_before"));
    assert!(qa.contains("wm_before"));
    assert!(qa.contains("clean_desktop_1280x800.png"));
}

#[test]
fn target_menu_commands_report_missing_focus() {
    let topbar = include_str!("../src/topbar.rs");
    assert!(topbar.contains("show_target_unavailable"));
    assert!(topbar.contains("This command needs a focused application window."));
    assert!(topbar.contains("register_target_menu_control"));
    assert!(topbar.contains("item.set_sensitive(false)"));
    assert!(topbar.contains("update_target_menu_controls"));
    assert!(topbar.contains("spawn_first_or_message"));
    assert!(topbar.contains("No compatible file manager is installed"));
}
