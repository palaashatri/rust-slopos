use std::path::Path;

#[test]
fn pivot_workspace_contains_only_the_current_x11_product_crates() {
    let cargo = include_str!("../../../Cargo.toml").to_ascii_lowercase();
    for member in [
        "crates/slopos-session",
        "crates/slopos-shell",
        "crates/slopos-catalogue",
        "crates/slopos-settings",
    ] {
        assert!(cargo.contains(member), "missing workspace member {member}");
    }
    for obsolete in [
        "slopos-compositor",
        "slopos-vision",
        "smithay",
        "wlroots",
        "xwayland",
    ] {
        assert!(!cargo.contains(obsolete), "obsolete scope returned: {obsolete}");
    }
}

#[test]
fn search_hotkey_targets_the_existing_shell_instance() {
    for openbox in [
        include_str!("../../../assets/config/openbox/rc.xml"),
        include_str!("../../../assets/config/openbox/rc-graphite.xml"),
    ] {
        assert!(openbox.contains("pkill -USR1 -x slopos-shell"));
        assert!(!openbox.contains("slopos-shell --toggle-launcher"));
    }
    let shell = include_str!("../src/main.rs");
    assert!(shell.contains("SIGUSR1"));
    assert!(shell.contains("Refusing to start a second SLOPOS shell"));
}

#[test]
fn topbar_is_a_real_global_menu_host_not_the_old_placeholder() {
    let topbar = include_str!("../src/topbar.rs");
    assert!(topbar.contains("slopos-global-menu-host"));
    assert!(topbar.contains("gmenu::detect"));
    assert!(topbar.contains("gmenu::build_menu_bar"));
    assert!(topbar.contains("Imported GTK global menubar"));
    assert!(!topbar.contains("App (local)"));
    assert!(!topbar.contains("N/A"));
    assert!(!topbar.contains(''));
    assert!(topbar.contains("Control Panels…"));
    assert!(topbar.contains("Software…"));
    assert!(topbar.contains("Platinum (Light)"));
    assert!(topbar.contains("Graphite (Dark)"));
    assert!(topbar.contains("window.set_type_hint(gdk::WindowTypeHint::Dock)"));
    assert!(topbar.contains("window.set_accept_focus(false)"));
}

#[test]
fn gtk_gmenu_bridge_uses_the_native_export_protocol() {
    let bridge = include_str!("../src/gmenu.rs");
    for property in [
        "_GTK_UNIQUE_BUS_NAME",
        "_GTK_MENUBAR_OBJECT_PATH",
        "_GTK_APP_MENU_OBJECT_PATH",
        "_GTK_APPLICATION_OBJECT_PATH",
        "_GTK_WINDOW_OBJECT_PATH",
    ] {
        assert!(bridge.contains(property), "missing GTK export property {property}");
    }
    assert!(bridge.contains("gio::DBusMenuModel::get"));
    assert!(bridge.contains("gio::DBusActionGroup::get"));
    assert!(bridge.contains("gtk::MenuBar::from_model"));
    assert!(bridge.contains("insert_action_group(\"app\""));
    assert!(bridge.contains("insert_action_group(\"win\""));
    assert!(!bridge.contains("com.canonical.dbusmenu"));
}

#[test]
fn gtk_is_configured_for_a_shell_owned_global_menubar() {
    let settings = include_str!("../../../assets/config/gtk-3.0/settings.ini");
    assert!(settings.contains("gtk-icon-theme-name = SLOPOS-Platinum"));
    assert!(settings.contains("gtk-shell-shows-menubar = 1"));
    assert!(settings.contains("gtk-shell-shows-app-menu = 0"));
}

#[test]
fn slopos_freedesktop_icon_theme_covers_core_file_manager_vocabulary() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("themes/platinum/icon-theme");
    assert!(root.join("index.theme").is_file());
    for relative in [
        "scalable/places/folder.svg",
        "scalable/places/user-home.svg",
        "scalable/places/user-desktop.svg",
        "scalable/places/computer.svg",
        "scalable/mimetypes/text-x-generic.svg",
        "scalable/devices/drive-harddisk.svg",
        "scalable/status/user-trash.svg",
        "scalable/actions/go-previous.svg",
        "scalable/actions/go-next.svg",
        "scalable/actions/go-up.svg",
        "scalable/actions/go-home.svg",
        "scalable/actions/view-refresh.svg",
        "scalable/actions/edit-find.svg",
    ] {
        assert!(root.join(relative).is_file(), "missing SLOPOS icon {relative}");
    }
    let index = include_str!("../../../themes/platinum/icon-theme/index.theme");
    assert!(index.contains("Name=SLOPOS Platinum"));
    assert!(index.contains("Context=Places"));
    assert!(index.contains("Context=MimeTypes"));
    assert!(index.contains("Context=Actions"));
}

#[test]
fn platinum_and_graphite_are_complete_runtime_appearances() {
    let light = include_str!("../../../assets/config/gtk-3.0/gtk.css");
    let dark = include_str!("../../../assets/config/gtk-3.0/gtk-graphite.css");
    let light_wm = include_str!("../../../themes/slopos-openbox/openbox-3/themerc");
    let dark_wm = include_str!("../../../themes/slopos-openbox-graphite/openbox-3/themerc");
    let dark_rc = include_str!("../../../assets/config/openbox/rc-graphite.xml");
    let switcher = include_str!("../../../scripts/slopos-appearance");

    for css in [light, dark] {
        for selector in [
            ".slopos-topbar",
            ".slopos-dock-container",
            ".slopos-launcher",
            ".slopos-notification",
            ".slopos-control-panel",
            "menubar > menuitem",
            "menu menuitem",
            "button",
            "entry",
            "scrollbar",
        ] {
            assert!(css.contains(selector), "appearance misses selector {selector}");
        }
    }
    assert!(light_wm.contains("window.active.title.bg"));
    assert!(dark_wm.contains("window.active.title.bg"));
    assert!(dark_rc.contains("slopos-openbox-graphite"));
    assert!(switcher.contains("platinum|graphite|status"));
    assert!(switcher.contains("gtk-theme-name = $gtk_theme"));
    assert!(switcher.contains("gtk-shell-shows-menubar = 1"));
    assert!(switcher.contains("pkill -TERM -x slopos-shell"));
    assert!(switcher.contains("pkill -TERM -x openbox"));
}

#[test]
fn settings_is_a_compact_control_panel_and_appearance_is_built_in() {
    let settings = include_str!("../../slopos-settings/src/main.rs");
    assert!(settings.contains("Label::new(Some(\"Control Panels\"))"));
    assert!(settings.contains("(640, 390)"));
    assert!(settings.contains("(index % 4) as i32"));
    assert!(settings.contains("(index / 4) as i32"));
    assert!(settings.contains("title: \"Appearance\""));
    assert!(settings.contains("built_in: true"));
    assert!(settings.contains("Platinum — classic light"));
    assert!(settings.contains("Graphite — dark"));
    assert!(settings.contains("appearance_helper"));
    assert!(settings.contains("slopos-appearance"));
    assert!(settings.contains("button.set_sensitive(false)"));
}

#[test]
fn settings_delegates_the_seven_external_system_panels() {
    let settings = include_str!("../../slopos-settings/src/main.rs");
    for utility in [
        "arandr",
        "pavucontrol",
        "nm-connection-editor",
        "blueman-manager",
        "xfce4-power-manager-settings",
        "pcmanfm",
        "lxinput",
    ] {
        assert!(settings.contains(utility), "missing Settings delegate {utility}");
    }
    let runner = include_str!("../../../scripts/run-settings-service-qa.sh");
    let probe = include_str!("../../../scripts/qa-settings-services.py");
    assert!(runner.contains("SETTINGS_UNAVAILABLE_CONTROLS_DISABLED=7"));
    assert!(runner.contains("SETTINGS_DELEGATED_CONTROLS=7"));
    assert!(probe.contains("BUILT_IN = \"Appearance settings\""));
    assert!(probe.contains("SETTINGS_BUILTIN_APPEARANCE_ENABLED=1"));
}

#[test]
fn shipping_dependency_manifests_include_real_settings_backends() {
    for manifest in [
        include_str!("../../../packaging/deps/arch.txt"),
        include_str!("../../../packaging/deps/ubuntu.txt"),
        include_str!("../../../packaging/iso/packages.x86_64"),
        include_str!("../../../packaging/vm/arch-install.sh"),
    ] {
        for required in [
            "pavucontrol",
            "network-manager",
            "blueman",
            "xfce4-power-manager",
            "xfce4-settings",
            "lxappearance",
            "arandr",
        ] {
            assert!(
                manifest.contains(required),
                "shipping dependency set misses {required}"
            );
        }
    }
}

#[test]
fn install_and_native_packages_ship_icons_and_both_appearances() {
    for manifest in [
        include_str!("../../../install.sh"),
        include_str!("../../../packaging/arch/PKGBUILD"),
        include_str!("../../../packaging/debian/rules"),
    ] {
        for required in [
            "slopos-appearance",
            "SLOPOS-Platinum",
            "gtk-graphite.css",
            "slopos-openbox-graphite",
            "rc-graphite.xml",
        ] {
            assert!(manifest.contains(required), "package misses {required}");
        }
    }
}

#[test]
fn session_reloads_appearance_without_reintroducing_display_stack_scope() {
    let session = include_str!("../../slopos-session/src/main.rs");
    assert!(session.contains("fn appearance()"));
    assert!(session.contains("#25272B"));
    assert!(session.contains("#758090"));
    assert!(session.contains("rc-graphite.xml"));
    assert!(session.contains("resolve_openbox_config"));
    for obsolete in ["wayland", "smithay", "wlroots", "xwayland"] {
        assert!(!session.to_ascii_lowercase().contains(obsolete));
    }
}

#[test]
fn appimage_catalogue_remains_fail_closed() {
    let installer = include_str!("../../slopos-catalogue/src/installer.rs");
    let model = include_str!("../../slopos-catalogue/src/model.rs");
    let catalogue = include_str!("../../slopos-catalogue/src/main.rs");
    assert!(!installer.contains("create_stub_appimage"));
    assert!(installer.contains("metadata_is_installable"));
    assert!(installer.contains("SHA-256 mismatch"));
    assert!(model.contains("eq_ignore_ascii_case(EMPTY_FILE_SHA256)"));
    assert!(catalogue.contains("Curated AppImages with pinned integrity metadata"));
    assert!(catalogue.contains("current_appearance"));
    assert!(catalogue.contains("gtk-graphite.css"));
}

#[test]
fn ui_ux_acceptance_proves_the_user_reported_gaps() {
    let qa = include_str!("../../../scripts/run-ui-ux-qa.sh");
    for proof in [
        "02-pcmanfm-slopos-icons.png",
        "03-real-gtk-global-menu.png",
        "04-settings-available.png",
        "05-graphite-desktop.png",
        "06-graphite-settings.png",
        "gtk-icon-theme-name",
        "gtk-shell-shows-menubar",
        "Imported GTK global menubar",
        "Required Settings delegate is missing",
        "slopos-appearance graphite",
        "slopos-appearance platinum",
        "UI/UX QA PASS",
    ] {
        assert!(qa.contains(proof), "UI/UX harness misses proof {proof}");
    }
    assert!(!qa.contains("App (local) placeholder leaked") || qa.contains("grep -q 'App (local)'"));
}

#[test]
fn shell_owned_surfaces_keep_accessibility_names() {
    let launcher = include_str!("../src/launcher.rs");
    let topbar = include_str!("../src/topbar.rs");
    let dock = include_str!("../src/dock.rs");
    let settings = include_str!("../../slopos-settings/src/main.rs");
    let catalogue = include_str!("../../slopos-catalogue/src/main.rs");
    for (source, name) in [
        (launcher, "SLOPOS application search"),
        (topbar, "SLOPOS top menu bar"),
        (topbar, "Focused application global menu"),
        (dock, "SLOPOS application strip"),
        (settings, "SLOPOS system settings"),
        (catalogue, "SLOPOS software catalogue"),
    ] {
        assert!(source.contains(name), "missing accessible name {name}");
    }
}

#[test]
fn platinum_controls_have_dense_classic_interaction_states() {
    let css = include_str!("../../../assets/config/gtk-3.0/gtk.css");
    for required in [
        "border-radius: 0",
        "button:active",
        "button:focus",
        "button:disabled",
        "menu menuitem:hover",
        "menubar > menuitem:hover",
        "check:checked",
        "radio:checked",
        "entry:focus",
        "row:selected",
        ".slopos-dock-container",
        ".slopos-alert-box",
        ".slopos-control-panel",
        "tooltip",
    ] {
        assert!(css.contains(required), "Platinum CSS misses {required}");
    }
}

#[test]
fn browser_and_upstream_apps_remain_integrated_not_forked() {
    let browser = include_str!("../../../scripts/start-slopos-browser");
    let mimeapps = include_str!("../../../assets/config/mimeapps.list");
    assert!(browser.contains("firefox"));
    assert!(browser.contains("chromium"));
    assert!(browser.contains("GDK_BACKEND=\"x11\""));
    assert!(mimeapps.contains("text/html=slopos-browser.desktop"));
    assert!(mimeapps.contains("image/png=ristretto.desktop"));
}

#[test]
fn x11_resolution_and_accessibility_acceptance_remain_release_gates() {
    let resolution = include_str!("../../../scripts/run-resolution-qa.sh");
    let atspi = include_str!("../../../scripts/run-atspi-qa.sh");
    assert!(resolution.contains("SLOPOS_RESOLUTION"));
    assert!(resolution.contains("SLOPOS_SCALE"));
    assert!(resolution.contains("RESOLUTION_QA_STATUS_0"));
    assert!(atspi.contains("at-spi-bus-launcher --launch-immediately"));
    assert!(atspi.contains("AT_SPI_STATUS_0"));
}

#[test]
fn custom_prefix_installation_forwards_all_desktop_resources() {
    let installer = include_str!("../../../install.sh");
    let launcher = include_str!("../../../scripts/start-slopos-i");
    assert!(installer.contains("XSESSION_DIR"));
    assert!(installer.contains("must be an absolute path"));
    assert!(installer.contains("SLOPOS-Platinum"));
    assert!(launcher.contains("SLOPOS_INSTALL_PREFIX"));
    assert!(launcher.contains("SLOPOS_SHARE_DIR"));
    assert!(launcher.contains("XDG_DATA_DIRS"));
    assert!(launcher.contains("SLOPOS_APPEARANCE"));
}

#[test]
fn shipping_sources_do_not_reintroduce_wayland_or_vision() {
    for source in [
        include_str!("../../../install.sh"),
        include_str!("../../../packaging/arch/PKGBUILD"),
        include_str!("../../../packaging/debian/rules"),
        include_str!("../../../packaging/vm/arch-install.sh"),
    ] {
        let lower = source.to_ascii_lowercase();
        for obsolete in ["smithay", "wlroots", "xwayland", "slopos-compositor", "slopos-vision"] {
            assert!(!lower.contains(obsolete), "shipping source contains {obsolete}");
        }
    }
}
