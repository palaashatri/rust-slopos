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
        assert!(
            !cargo.contains(obsolete),
            "obsolete scope returned: {obsolete}"
        );
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
    assert!(topbar.contains("Platinum Light"));
    assert!(topbar.contains("Graphite Dark"));
    assert!(topbar.contains("window.set_type_hint(gdk::WindowTypeHint::Dock)"));
    assert!(topbar.contains("window.set_accept_focus(false)"));
}

#[test]
fn fallback_menu_does_not_fake_application_edit_or_desktop_hotkeys() {
    let topbar = include_str!("../src/topbar.rs");
    assert!(!topbar.contains("xdotool"));
    for label in [
        "Cut (Ctrl+X)",
        "Copy (Ctrl+C)",
        "Paste (Ctrl+V)",
        "Select All (Ctrl+A)",
        "Refresh Desktop (F5)",
        "Clean Up Desktop",
    ] {
        assert!(!topbar.contains(label), "fake fallback command: {label}");
    }
}

#[test]
fn sleep_menu_uses_the_typed_power_provider_without_command_fallback() {
    let topbar = include_str!("../src/topbar.rs");
    let session = include_str!("../src/services/session.rs");
    let monitor = include_str!("../src/services/monitor.rs");
    assert!(topbar.contains("services::power::request_suspend"));
    assert!(!topbar.contains("session::can_suspend()"));
    assert!(topbar.contains("status.suspend_capability"));
    assert!(monitor.contains("suspend_capability"));
    assert!(!session.contains("pub fn suspend_system"));
    assert!(!session.contains("SUSPEND_CAPABILITY"));
}

#[test]
fn gtk_gmenu_bridge_uses_the_native_export_protocol() {
    let bridge = include_str!("../src/menu/gmenu.rs");
    for property in [
        "_GTK_UNIQUE_BUS_NAME",
        "_GTK_MENUBAR_OBJECT_PATH",
        "_GTK_APP_MENU_OBJECT_PATH",
        "_GTK_APPLICATION_OBJECT_PATH",
        "_GTK_WINDOW_OBJECT_PATH",
    ] {
        assert!(
            bridge.contains(property),
            "missing GTK export property {property}"
        );
    }
    assert!(bridge.contains("gio::DBusMenuModel::get"));
    assert!(bridge.contains("gio::DBusActionGroup::get"));
    assert!(bridge.contains("gtk::MenuBar::from_model"));
    assert!(bridge.contains("insert_action_group(\"app\""));
    assert!(bridge.contains("insert_action_group(\"win\""));
    assert!(!bridge.contains("com.canonical.dbusmenu"));
}

#[test]
fn shell_registers_only_the_single_global_menu_bridge() {
    let entry = include_str!("../src/main.rs");
    let menu_module = include_str!("../src/menu/mod.rs");
    assert!(!entry.contains("pub mod gmenu;"));
    assert!(entry.contains("pub mod menu;"));
    assert!(menu_module.contains("pub mod gmenu;"));
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
        assert!(
            root.join(relative).is_file(),
            "missing SLOPOS icon {relative}"
        );
    }
    let index = include_str!("../../../themes/platinum/icon-theme/index.theme");
    assert!(index.contains("Name=SLOPOS Platinum"));
    assert!(index.contains("Context=Places"));
    assert!(index.contains("Context=MimeTypes"));
    assert!(index.contains("Context=Actions"));
}

#[test]
fn appearances_are_complete_runtime_themes() {
    let light = include_str!("../../../assets/config/gtk-3.0/gtk.css");
    let classic = include_str!("../../../assets/config/gtk-3.0/gtk-classic.css");
    let dark = include_str!("../../../assets/config/gtk-3.0/gtk-graphite.css");
    let oled = include_str!("../../../assets/config/gtk-3.0/gtk-oled.css");
    let light_wm = include_str!("../../../themes/slopos-openbox/openbox-3/themerc");
    let classic_wm = include_str!("../../../themes/slopos-openbox-classic/openbox-3/themerc");
    let dark_wm = include_str!("../../../themes/slopos-openbox-graphite/openbox-3/themerc");
    let oled_wm = include_str!("../../../themes/slopos-openbox-oled/openbox-3/themerc");
    let classic_rc = include_str!("../../../assets/config/openbox/rc-classic.xml");
    let dark_rc = include_str!("../../../assets/config/openbox/rc-graphite.xml");
    let oled_rc = include_str!("../../../assets/config/openbox/rc-oled.xml");
    let switcher = include_str!("../../../scripts/slopos-appearance");

    for css in [light, classic, dark, oled] {
        for selector in [
            ".slopos-topbar",
            "menubar > menuitem",
            "menu menuitem",
            "button",
            "entry",
            "scrollbar",
        ] {
            assert!(
                css.contains(selector),
                "appearance misses selector {selector}"
            );
        }
    }
    assert!(light_wm.contains("window.active.title.bg"));
    assert!(classic_wm.contains("window.active.title.bg"));
    assert!(dark_wm.contains("window.active.title.bg"));
    assert!(oled_wm.contains("window.active.title.bg"));
    assert!(classic_rc.contains("slopos-openbox-classic"));
    assert!(dark_rc.contains("slopos-openbox-graphite"));
    assert!(oled_rc.contains("slopos-openbox-oled"));
    assert!(switcher.contains("classic|platinum|graphite|oled|status"));
    assert!(switcher.contains("gtk-theme-name = $gtk_theme"));
    assert!(switcher.contains("legacy_gtk2"));
    assert!(switcher.contains("gtkrc-2.0.legacy"));
    assert!(switcher.contains("cp -f \"$legacy_gtk2\" \"$legacy_copy\""));
    assert!(switcher.contains("SLOPOS_SESSION_MANAGED"));
    assert!(switcher.contains("gtk-shell-shows-menubar = 1"));
    assert!(switcher.contains("pkill -TERM -x slopos-shell"));
    assert!(switcher.contains("pkill -TERM -x openbox"));
}

#[test]
fn settings_is_a_compact_control_panel_and_appearance_is_built_in() {
    let settings = include_str!("../../slopos-settings/src/main.rs");
    let settings_theme = include_str!("../../slopos-settings/src/theme.rs");
    let appearance = include_str!("../../slopos-settings/src/panels/appearance.rs");
    assert!(settings.contains("window.set_title(\"System Settings\")"));
    assert!(settings.contains("Label::new(Some(\"Control Panels\"))"));
    assert!(settings.contains("adaptive_window_size"));
    assert!(settings.contains("(index % 5) as i32"));
    assert!(settings.contains("(index / 5) as i32"));
    assert!(settings.contains("slopos-icon-grid"));
    assert!(settings.contains("slopos-control-panel-icon"));
    assert!(settings_theme.contains("#000080"));
    assert!(settings_theme.contains("button.slopos-control-panel-icon"));
    assert!(settings.contains("title: \"Appearance\""));
    assert!(settings.contains("built_in: BuiltInPanel::Appearance"));
    assert!(appearance.contains("Platinum Light"));
    assert!(appearance.contains("Graphite Dark"));
    assert!(appearance.contains("slopos-appearance"));
    assert!(settings.contains("button.set_sensitive(false)"));
}

#[test]
fn settings_keeps_five_builtin_panels_and_delegates_four_external_panels() {
    let settings = include_str!("../../slopos-settings/src/main.rs");
    for built_in in [
        "BuiltInPanel::Sound",
        "BuiltInPanel::Network",
        "BuiltInPanel::Appearance",
        "BuiltInPanel::Desktop",
        "BuiltInPanel::DateTime",
    ] {
        assert!(
            settings.contains(built_in),
            "missing built-in Settings panel {built_in}"
        );
    }

    let combined = format!(
        "{settings}\n{}",
        include_str!("../../slopos-settings/src/panels/desktop.rs")
    );
    for utility in [
        "arandr",
        "blueman-manager",
        "xfce4-power-manager-settings",
        "lxinput",
    ] {
        assert!(
            combined.contains(utility),
            "missing Settings delegate {utility}"
        );
    }

    let runner = include_str!("../../../scripts/run-settings-service-qa.sh");
    let probe = include_str!("../../../scripts/qa-settings-services.py");
    assert!(runner.contains("SETTINGS_UNAVAILABLE_CONTROLS_DISABLED=4"));
    assert!(runner.contains("SETTINGS_DELEGATED_CONTROLS=4"));
    assert!(runner.contains("SETTINGS_BUILTIN_CONTROLS_ENABLED=5"));
    assert!(probe.contains("BUILT_INS = ["));
    assert!(probe.contains("SETTINGS_BUILTIN_CONTROLS_ENABLED=5"));
    assert!(probe.contains("SETTINGS_BUILTIN_APPEARANCE_ENABLED=1"));
}

#[test]
fn sound_settings_uses_the_shared_audio_provider_without_fabricated_devices() {
    let sound = include_str!("../../slopos-settings/src/panels/sound.rs");
    let manifest = include_str!("../../slopos-settings/Cargo.toml");
    assert!(manifest.contains("slopos-services ="));
    for provider_api in [
        "AudioClient::new()",
        "watcher_client.watch()",
        "audio_client.clone()",
        "set_output_volume(",
        "set_output_muted(",
        "set_input_volume(",
        "set_input_muted(",
    ] {
        assert!(
            sound.contains(provider_api),
            "missing shared audio API {provider_api}"
        );
    }
    assert!(sound.contains("stop_audio.send(())"));
    for fabricated_state in [
        "Built-in Analog Stereo Speakers",
        "Headphones / Line Out",
        "Digital Output (HDMI / DisplayPort)",
        "set-sink-volume",
        "set-sink-mute",
        "amixer",
    ] {
        assert!(
            !sound.contains(fabricated_state),
            "found bypass/fake state {fabricated_state}"
        );
    }
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
    let main = include_str!("../../slopos-session/src/main.rs");
    let environment = include_str!("../../slopos-session/src/environment.rs");
    assert!(environment.contains("pub fn appearance()"));
    assert!(environment.contains("rc-graphite.xml"));
    assert!(environment.contains("pub fn openbox_config()"));
    // The session must not write other desktops' configuration files. It does
    // read ~/.gtkrc-2.0 to include legacy GTK 2 settings in its managed config,
    // so check that the legacy lookup stays read-only.
    let legacy = environment
        .split("fn legacy_gtk2_rc()")
        .nth(1)
        .and_then(|rest| rest.split("\n}").next())
        .expect("legacy_gtk2_rc must exist");
    assert!(legacy.contains(".is_file()"));
    for writer in [
        "fs::write",
        "OpenOptions",
        "File::create",
        "create_dir",
        "set_len",
    ] {
        assert!(
            !legacy.contains(writer),
            "the session must not write ~/.gtkrc-2.0 ({writer})"
        );
    }
    assert!(!environment.contains("\"openbox\").join(\"rc.xml\")"));
    for source in [main, environment] {
        for obsolete in ["wayland", "smithay", "wlroots", "xwayland"] {
            assert!(!source.to_ascii_lowercase().contains(obsolete));
        }
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
        "capture 02-pcmanfm-slopos-icons",
        "capture 03-real-gtk-global-menu",
        "capture 04-settings-available",
        "capture 05-graphite-desktop",
        "capture 06-graphite-settings",
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
    assert!(qa.contains("grep -q 'App (local)'"));
}

#[test]
fn shell_owned_surfaces_keep_accessibility_names() {
    let launcher = include_str!("../src/launcher.rs");
    let topbar = include_str!("../src/topbar.rs");
    let settings = include_str!("../../slopos-settings/src/main.rs");
    let catalogue = include_str!("../../slopos-catalogue/src/main.rs");
    for (source, name) in [
        (launcher, "SLOPOS application search"),
        (topbar, "SLOPOS top menu bar"),
        (topbar, "Focused application global menu"),
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
    // Native SLOPOS apps are the defaults; the upstream apps stay listed as
    // fallbacks rather than being forked or removed.
    assert!(mimeapps.contains("image/png=slopos-image-viewer.desktop;ristretto.desktop"));
    assert!(mimeapps.contains("video/mp4=slopos-media.desktop;mpv.desktop"));
    assert!(mimeapps.contains("inode/directory=slopos-files.desktop;pcmanfm.desktop"));
}

#[test]
fn x11_resolution_and_accessibility_acceptance_remain_release_gates() {
    let resolution = include_str!("../../../scripts/run-resolution-qa.sh");
    let atspi = include_str!("../../../scripts/run-atspi-qa.sh");
    assert!(resolution.contains("SLOPOS_RESOLUTION"));
    assert!(resolution.contains("SLOPOS_SCALE"));
    assert!(resolution.contains("RESOLUTION_QA_STATUS_0"));
    assert!(atspi.contains("at-spi-bus-launcher --launch-immediately"));
    assert!(atspi.contains("python3 scripts/qa-atspi.py --extended"));
    assert!(atspi.contains("AT_SPI_SCREEN_READER_ORCA_STATUS_0"));
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
fn shell_battery_status_uses_the_shared_power_provider_without_sysfs_fallbacks() {
    let monitor = include_str!("../src/services/monitor.rs");
    let entry = include_str!("../src/main.rs");
    let provider = include_str!("../../slopos-services/src/power.rs");
    assert!(monitor.contains("PowerClient::connect()"));
    assert!(monitor.contains("client.watch()"));
    assert!(monitor.contains("PowerEvent::Changed(snapshot)"));
    assert!(provider.contains("org.freedesktop.UPower"));
    assert!(provider.contains("CanSuspend"));
    assert!(monitor.contains("PowerEvent::LowBattery"));
    assert!(monitor.contains("PowerEvent::CriticalBattery"));
    assert!(entry.contains("NotificationServer::show_toast"));
    assert!(!monitor.contains("query_battery_state"));
    assert!(!monitor.contains("/sys/class/power_supply"));
}

#[test]
fn network_settings_do_not_invent_connections_or_expose_inert_controls() {
    let panel = include_str!("../../slopos-settings/src/panels/network.rs");
    for fabricated_state in [
        "SLOPOS-Fast-5G",
        "Home-Network-Guest",
        "CoffeeShop_Free_WiFi",
        "192.168.1.100",
        "1000 Mbps Full Duplex",
        "Switch::new()",
        "Button::with_label(\"Connect\")",
    ] {
        assert!(
            !panel.contains(fabricated_state),
            "Network Settings fabricates or implies state: {fabricated_state}"
        );
    }
    // State comes from the NetworkManager provider, never from literals.
    assert!(panel.contains("NetworkManagerClient::connect"));
    assert!(panel.contains("Update::Unavailable"));
    // A failed worker spawn reports it instead of leaving the dialog loading.
    assert!(panel.contains("let failure_updates = updates.clone();"));
    assert!(panel.contains("failure_updates.send(Update::Unavailable"));
    assert!(!panel.contains("let _ = Command::new"));
    // Closing the dialog detaches the GLib source that updates its widgets.
    assert!(panel.contains("let update_source = update_receiver.attach"));
    assert!(panel.contains("update_source.remove();"));
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
        for obsolete in [
            "smithay",
            "wlroots",
            "xwayland",
            "slopos-compositor",
            "slopos-vision",
        ] {
            assert!(
                !lower.contains(obsolete),
                "shipping source contains {obsolete}"
            );
        }
    }
}

#[test]
fn event_driven_architecture_replaces_periodic_subprocess_polling() {
    let shell_main = include_str!("../src/main.rs");
    let x11_events = include_str!("../src/x11/events.rs");
    let monitor = include_str!("../src/services/monitor.rs");
    let audio = include_str!("../src/services/audio.rs");
    let topbar = include_str!("../src/topbar.rs");

    assert!(shell_main.contains("AppIndex::start"));
    assert!(shell_main.contains("SystemMonitor::start"));
    assert!(shell_main.contains("X11EventBus::start"));

    // Ensure edge detection uses native X11 Enter/Leave notify window rather than 50ms pointer polling
    assert!(x11_events.contains("EventMask::ENTER_WINDOW"));
    assert!(x11_events.contains("EventMask::LEAVE_WINDOW"));
    assert!(x11_events.contains("Event::EnterNotify"));
    assert!(x11_events.contains("Event::LeaveNotify"));
    assert!(!x11_events.contains("query_pointer"));
    // The event thread blocks on the X server instead of polling.
    assert!(x11_events.contains("wait_for_event"));
    assert!(!x11_events.contains("poll_for_event"));
    assert!(!shell_main.contains("timeout_add_local"));

    // Ensure shell audio state and controls use the shared audio provider.
    assert!(monitor.contains("audio::watch_audio_changes"));
    assert!(monitor.contains("network::watch"));
    let monitor_runtime = monitor.split("#[cfg(test)]").next().unwrap_or(monitor);
    assert!(!monitor_runtime.contains("recv_timeout"));
    assert!(!monitor.contains("pactl"));
    assert!(audio.contains("AudioClient"));
    assert!(topbar.contains("audio::adjust_default_output_volume"));
    assert!(topbar.contains("audio::toggle_default_output_mute"));
    assert!(!topbar.contains("wpctl"));
    assert!(!topbar.contains("pactl"));
}

#[test]
fn multi_monitor_and_coordinate_mapping_supports_hidpi_and_non_zero_origins() {
    let monitors = include_str!("../src/x11/monitors.rs");
    let topbar = include_str!("../src/topbar.rs");

    assert!(monitors.contains("pub fn gdk_x"));
    assert!(monitors.contains("pub fn gdk_y"));
    assert!(monitors.contains("pub fn gdk_width"));
    assert!(monitors.contains("pub fn gdk_height"));
    assert!(monitors.contains("pub fn root_left"));
    assert!(monitors.contains("pub fn root_bottom"));

    assert!(topbar.contains("primary.gdk_x()"));
    assert!(topbar.contains("primary.gdk_width()"));
}
