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
    assert!(topbar.contains("load_slopos_mark_sized(20)"));
    assert!(topbar.contains("scale_simple(size, size, InterpType::Bilinear)"));
    assert!(topbar.contains("set_label(\"S\")"));
}

#[test]
fn platinum_dialogs_use_packaged_identity_mark() {
    let topbar = include_str!("../src/topbar.rs");
    assert!(topbar.contains("alert.pack_start(&mark, false, false, 0)"));
    assert!(topbar.contains("style_context().add_class(\"slopos-alert-box\")"));
    assert!(topbar.contains("load_slopos_mark_sized(40)"));
    assert!(topbar.contains("fn load_slopos_mark_sized(size: i32)"));
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
fn installed_vm_source_scan_only_checks_shipping_files() {
    let qa = include_str!("../../../packaging/vm/qa-vm.sh");
    assert!(qa.contains("shipping_files=("));
    assert!(qa.contains("scripts/start-slopos-i"));
    assert!(qa.contains("packaging/vm/arch-install.sh"));
    assert!(qa.contains("obsolete display-stack reference remains in shipping file"));
    assert!(!qa.contains("grep -RIEq"));
    assert!(!qa.contains("--exclude='qa-vm.sh'"));
}

#[test]
fn installed_vm_harness_pins_source_and_collects_status() {
    let installer = include_str!("../../../packaging/vm/arch-install.sh");
    assert!(installer.contains("REPO_COMMIT=\"${REPO_COMMIT:-}\""));
    assert!(installer.contains("REPO_COMMIT must be a full 40-character commit SHA"));
    assert!(installer.contains("git -C ~/slopos-i fetch --depth 1 origin"));
    assert!(installer.contains("git -C ~/slopos-i checkout --detach"));
    assert!(installer.contains("Pinned source commit: $REPO_COMMIT"));

    let provision = include_str!("../../../packaging/vm/provision.ps1");
    assert!(provision.contains("[string]$RepoCommit = \"\""));
    assert!(provision.contains("REPO_COMMIT=$RepoCommit"));
    assert!(provision.contains("qa-installed.ps1"));
    assert!(provision.contains("-ExpectedCommit $RepoCommit"));
    assert!(provision.contains("Stop-Process -Id $http.Id -Force"));

    let qa = include_str!("../../../packaging/vm/qa-installed.ps1");
    assert!(qa.contains("[string]$ExpectedCommit = \"\""));
    assert!(qa.contains("does not match expected"));
    assert!(qa.contains("function Invoke-SshCapture"));
    assert!(qa.contains("$ErrorActionPreference = \"Continue\""));
    assert!(qa.contains("LogLevel=ERROR"));
    assert!(qa.contains("git -C /home/$SshUser/slopos-i rev-parse HEAD"));
    assert!(qa.contains("packaging/vm/qa-vm.sh"));
    assert!(qa.contains("screenshotpng"));
    assert!(qa.contains("INSTALLED_VM_QA_STATUS_0"));
    assert!(qa.contains("status.json"));
}

#[test]
fn vm_recreate_checks_state_before_poweroff() {
    let create_vm = include_str!("../../../packaging/vm/create-vm.ps1");
    assert!(create_vm.contains("showvminfo $VmName --machinereadable"));
    assert!(create_vm.contains("$vmState -in @('running', 'paused', 'stuck')"));
    assert!(create_vm.contains("controlvm $VmName poweroff"));
}

#[test]
fn docker_qa_uses_fresh_visible_windows() {
    let qa = include_str!("../../../scripts/run-docker-qa.sh");
    assert!(qa.contains("dbus-run-session -- bash -c"));
    assert!(qa.contains("DBUS_SESSION_BUS_ADDRESS"));
    assert!(qa.contains("notify-send"));
    assert!(qa.contains("wait_visible_window '^SLOPOS Top Bar$'"));
    assert!(qa.contains("wait_visible_window '^SLOPOS Application Strip$'"));
    assert!(qa.contains("capture_screenshot()"));
    assert!(qa.contains("xdotool getdisplaygeometry"));
    assert!(qa.contains("Keep pointer-driven tooltips out of canonical evidence"));
    assert!(qa.contains("xdotool search --onlyvisible --name \"$pattern\""));
    assert!(qa.contains("--onlyvisible --name '^Software Catalogue$'"));
    assert!(qa.contains("--onlyvisible --name '^System Settings$'"));
    assert!(qa.contains("notify-send -t 60000"));
    assert!(qa.contains("Close only the fresh visible"));
    assert!(qa.contains("! xdotool search --onlyvisible --name '^SLOPOS Notification [0-9]+$'"));
    assert!(qa.contains("getwindowpid"));
    assert!(qa.contains("Verify session recovery after child failure"));
    assert!(qa.contains("shell_before"));
    assert!(qa.contains("wm_before"));
    assert!(qa.contains("close_visible_windows_by_class"));
    assert!(qa.contains("xdotool windowmove --sync"));
    assert!(qa.contains("xdotool windowsize"));
    assert!(qa.contains("xdotool key --clearmodifiers alt+Tab"));
    assert!(qa.contains("ACTIVE_BEFORE"));
    assert!(qa.contains("ACTIVE_AFTER"));
    assert!(qa.contains("clean_desktop_1280x800.png"));
    for scene in [
        "menu_open_1280x800.png",
        "search_open_1280x800.png",
        "notification_1280x800.png",
        "modal_about_1280x800.png",
        "file_manager_1280x800.png",
        "terminal_1280x800.png",
    ] {
        assert!(qa.contains(scene), "missing canonical scene {scene}");
    }
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

#[test]
fn image_controls_have_accessible_names_and_focus_feedback() {
    let dock = include_str!("../src/dock.rs");
    let topbar = include_str!("../src/topbar.rs");
    let notifications = include_str!("../src/notifications.rs");
    let css = include_str!("../../../assets/config/gtk-3.0/gtk.css");
    assert!(dock.contains("set_accessible_name(&button, tooltip)"));
    assert!(topbar.contains("set_accessible_name(&system_button, \"SLOPOS menu\")"));
    assert!(topbar.contains("set_accessible_name(&search_button"));
    assert!(notifications.contains("icon.is_empty().then(load_slopos_mark)"));
    assert!(notifications.contains("SLOPOS_SHARE_DIR"));
    assert!(notifications.contains("slopos-logo.png"));
    assert!(css.contains("button:focus"));
    assert!(css.contains("@slopos_highlight"));
}

#[test]
fn launcher_prefers_packaged_role_icons_with_upstream_fallbacks() {
    let launcher = include_str!("../src/launcher.rs");
    let css = include_str!("../../../assets/config/gtk-3.0/gtk.css");
    assert!(launcher.contains("fn role_icon_file(app: &DesktopApp)"));
    assert!(launcher.contains("load_launcher_icon(app)"));
    assert!(launcher.contains("Pixbuf::from_file_at_scale(&path, 32, 32, true)"));
    for icon in [
        "folder.svg",
        "terminal.svg",
        "textedit.svg",
        "browser.svg",
        "desktop.svg",
        "software.svg",
        "settings.svg",
    ] {
        assert!(launcher.contains(icon), "missing launcher role icon {icon}");
    }
    assert!(launcher.contains("Image::from_icon_name(Some(icon_name), IconSize::Dnd)"));
    assert!(css.contains(".slopos-result-icon"));
    assert!(css.contains("min-width: 32px"));
}

#[test]
fn atspi_acceptance_covers_named_surfaces_and_focus() {
    let launcher = include_str!("../src/launcher.rs");
    let topbar = include_str!("../src/topbar.rs");
    let dock = include_str!("../src/dock.rs");
    let settings = include_str!("../../slopos-settings/src/main.rs");
    let catalogue = include_str!("../../slopos-catalogue/src/main.rs");
    let runner = include_str!("../../../scripts/run-atspi-qa.sh");
    let probe = include_str!("../../../scripts/qa-atspi.py");
    let ci = include_str!("../../../.github/workflows/ci.yml");

    for (source, name) in [
        (launcher, "SLOPOS application search"),
        (launcher, "Application search field"),
        (topbar, "SLOPOS top bar"),
        (dock, "SLOPOS application strip"),
        (settings, "SLOPOS system settings"),
        (catalogue, "SLOPOS software catalogue"),
    ] {
        assert!(source.contains(name), "missing AT-SPI name {name}");
    }
    assert!(runner.contains("GTK_MODULES=gail:atk-bridge"));
    assert!(runner.contains("at-spi-bus-launcher --launch-immediately"));
    assert!(runner.contains("gsettings set org.gnome.desktop.interface toolkit-accessibility true"));
    assert!(runner.contains("qa-atspi.py"));
    assert!(ci.contains("x11-atspi-acceptance"));
    assert!(ci.contains("sudo -E env \"PATH=$PATH\" bash scripts/run-atspi-qa.sh"));
    assert!(probe.contains("Atspi.get_desktop(0)"));
    assert!(probe.contains("Atspi.StateType.FOCUSED"));
    assert!(probe.contains("EXPECTED_NAMES"));
    assert!(probe.contains("AT_SPI_EXPECTED_NAMES="));
    assert!(probe.contains("run_extended_checks"));
    assert!(probe.contains("AT_SPI_UTF8_TEXT="));
    assert!(probe.contains("AT_SPI_EXTENDED_STATUS_0"));
    assert!(probe.contains("xdotool"));
    assert!(probe.contains("shift+Tab"));
    assert!(probe.contains("AT_SPI_STATUS_0"));
    assert!(runner.contains("SLOPOS_ATSPI_SCREEN"));
    assert!(runner.contains("SLOPOS_ATSPI_SCALE"));
    assert!(runner.contains("SLOPOS_ATSPI_LOCALE"));
    assert!(runner.contains("--extended"));
    assert!(runner.contains("GDK_SCALE"));
}

#[test]
fn settings_hub_uses_compact_platinum_controls() {
    let settings = include_str!("../../slopos-settings/src/main.rs");
    let css = include_str!("../../../assets/config/gtk-3.0/gtk.css");
    assert!(settings.contains("window.set_default_size(640, 460)"));
    assert!(settings.contains("IconSize::LargeToolbar"));
    assert!(settings.contains("icon.set_pixel_size(32)"));
    assert!(settings.contains("button.set_vexpand(false)"));
    assert!(css.contains(".slopos-control-icon"));
    assert!(css.contains("min-height: 64px"));
}

#[test]
fn settings_hub_uses_packaged_platinum_icons_with_fallbacks() {
    let settings = include_str!("../../slopos-settings/src/main.rs");
    assert!(settings.contains("load_control_icon(panel.icon_file, panel.fallback_icon)"));
    assert!(settings.contains("Pixbuf::from_file_at_scale(&path, 32, 32, true)"));
    for icon in [
        "display.svg",
        "sound.svg",
        "network.svg",
        "bluetooth.svg",
        "power.svg",
        "appearance.svg",
        "desktop.svg",
        "keyboard.svg",
    ] {
        let path = format!("themes/platinum/icons/{icon}");
        let repo_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(&path);
        assert!(repo_path.is_file(), "missing packaged icon {path}");
        assert!(
            settings.contains(icon),
            "settings does not reference {icon}"
        );
    }
}

#[test]
fn upstream_gtk_menubars_keep_platinum_spacing() {
    let css = include_str!("../../../assets/config/gtk-3.0/gtk.css");
    assert!(css.contains("menubar > menuitem"));
    assert!(css.contains("padding: 2px 7px"));
    assert!(css.contains("menubar > menuitem:hover"));
    assert!(css.contains("menubar > menuitem:focus"));
}

#[test]
fn browser_integration_is_upstream_and_no_fork() {
    let launcher = include_str!("../../../scripts/start-slopos-browser");
    let installer = include_str!("../../../scripts/install-browser-theme.sh");
    let dock = include_str!("../src/dock.rs");
    let chromium = include_str!("../../../packaging/browser/chromium/manifest.json");
    let firefox = include_str!("../../../packaging/browser/firefox/manifest.json");
    let firefox_css = include_str!("../../../packaging/browser/firefox/userChrome.css");
    let browser_docs = include_str!("../../../packaging/browser/README.md");

    assert!(launcher.contains("GTK_THEME"));
    assert!(launcher.contains("export XDG_SESSION_TYPE=\"x11\""));
    assert!(launcher.contains("unset WAYLAND_DISPLAY"));
    assert!(launcher.contains("export GDK_BACKEND=\"x11\""));
    assert!(launcher.contains("export MOZ_ENABLE_WAYLAND=\"0\""));
    assert!(launcher.contains("firefox"));
    assert!(launcher.contains("chromium"));
    assert!(launcher.contains("google-chrome"));
    assert!(launcher.contains("--ozone-platform=x11"));
    assert!(launcher.contains("--load-extension"));
    assert!(installer.contains("PROFILE_DIR must be an absolute path"));
    assert!(installer.contains("slopos-backup"));
    assert!(dock.contains("program: \"start-slopos-browser\""));
    assert!(chromium.contains("\"manifest_version\": 3"));
    assert!(chromium.contains("\"frame\": [117, 128, 144]"));
    assert!(chromium.contains("\"toolbar_button_icon\": [0, 0, 0]"));
    assert!(chromium.contains("\"omnibox_background\": [255, 255, 255]"));
    assert!(firefox.contains("\"theme\""));
    assert!(firefox.contains("\"toolbar_field_border_focus\": \"#000080\""));
    assert!(firefox_css.contains("#nav-bar .toolbarbutton-1"));
    assert!(firefox_css.contains("#sidebar-box"));
    assert!(firefox_css.contains("menupopup > menuitem[_moz-menuactive=\"true\"]"));
    assert!(browser_docs.contains("does not fork or patch Firefox, Chromium or Chrome"));

    for manifest in [
        include_str!("../../../install.sh"),
        include_str!("../../../packaging/arch/PKGBUILD"),
        include_str!("../../../packaging/debian/rules"),
        include_str!("../../../packaging/iso/build-iso.sh"),
    ] {
        assert!(manifest.contains("start-slopos-browser"));
        assert!(manifest.contains("packaging/browser") || manifest.contains("browser/chromium"));
    }
}

#[test]
fn upstream_app_and_game_qa_covers_five_roles_with_audio() {
    let qa = include_str!("../../../scripts/run-arch-app-qa.sh");
    for role in [
        "file-manager",
        "terminal",
        "text-editor",
        "image-viewer",
        "browser",
    ] {
        assert!(qa.contains(role), "missing upstream role {role}");
    }
    assert!(qa.contains("supertux"));
    assert!(qa.contains("SLOPOS_BROWSER_QA_MARKER"));
    assert!(qa.contains("SLOPOS Browser QA"));
    assert!(qa.contains("browser-firefox.png"));
    assert!(qa.contains("install-browser-theme.sh firefox"));
    assert!(qa.contains("SLOPOS_BROWSER_THEME_DIR=/usr/share/slopos-i/browser/chromium"));
    assert!(qa.contains("--profile \"$FIREFOX_PROFILE\""));
    assert!(qa.contains("browser-dom.html"));
    assert!(qa.contains("getwindowname"));
    assert!(qa.contains("world1/frosted_fields.stl"));
    assert!(qa.contains("xdotool keydown --window"));
    assert!(qa.contains("xdotool key --window \"$GAME_WINDOW\" space"));
    assert!(qa.contains("kill -0 \"$GAME_PID\""));
    assert!(qa.contains("xdotool key --window \"$GAME_WINDOW\" Escape"));
    assert!(qa.contains("xdotool key --window \"$GAME_WINDOW\" q"));
    assert!(qa.contains("unrecoverable error"));
    assert!(qa.contains("pactl list sink-inputs"));
    assert!(qa.contains("test -s artifacts/qa/app-matrix/sink-inputs.txt"));
    assert!(qa.contains("command -v parec"));
    assert!(qa.contains("slopos_null.monitor"));
    assert!(qa.contains("game-audio.raw"));
    assert!(qa.contains("GAME_AUDIO_NONZERO_BYTES"));
    assert!(qa.contains("PulseAudio monitor capture is empty or silent"));
    assert!(qa.contains("SLOPOS-I Arch upstream application/browser/game evidence PASS"));

    for manifest in [
        include_str!("../../../packaging/deps/arch.txt"),
        include_str!("../../../packaging/deps/ubuntu.txt"),
        include_str!("../../../packaging/iso/packages.x86_64"),
        include_str!("../../../packaging/vm/arch-install.sh"),
    ] {
        assert!(manifest
            .split_whitespace()
            .any(|token| token.trim_matches('\\') == "supertux"));
    }
}

#[test]
fn retained_resolution_qa_covers_scale_matrix() {
    let qa = include_str!("../../../scripts/run-resolution-qa.sh");
    assert!(qa.contains("SLOPOS_RESOLUTION"));
    assert!(qa.contains("SLOPOS_SCALE"));
    assert!(qa.contains("GDK_SCALE"));
    assert!(qa.contains("1366x768"));
    assert!(qa.contains("RESOLUTION_QA_STATUS_0"));
    assert!(qa.contains("identify -format '%wx%h'"));
    assert!(qa.contains("capture_screenshot"));
    assert!(qa.contains("xdotool getdisplaygeometry"));
}
