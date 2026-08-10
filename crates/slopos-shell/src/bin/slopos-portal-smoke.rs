//! Runtime probe for the standard SLOPOS-I portal frontend.
//!
//! This intentionally proves only session-bus registration and introspection.
//! Portal backends that need a chooser, launcher, permission decision, or live
//! PipeWire graph remain fail-closed until those services are connected.

#[cfg(target_os = "linux")]
fn main() {
    use slopos_shell::portal::{
        PORTAL_BUS_NAME, PORTAL_FILECHOOSER_INTERFACE, PORTAL_OPENURI_INTERFACE, PORTAL_PATH,
        PORTAL_REQUEST_INTERFACE, PORTAL_SCREENCAST_INTERFACE, PORTAL_SCREENSHOT_INTERFACE,
        PORTAL_SETTINGS_INTERFACE,
    };
    use slopos_shell::portal_dbus::try_register_portal_session_bus;
    use zbus::blocking::{Connection, Proxy};

    let interfaces = [
        PORTAL_SCREENSHOT_INTERFACE,
        PORTAL_SETTINGS_INTERFACE,
        PORTAL_FILECHOOSER_INTERFACE,
        PORTAL_OPENURI_INTERFACE,
        PORTAL_SCREENCAST_INTERFACE,
    ];

    if !try_register_portal_session_bus() {
        eprintln!("portal frontend did not acquire the standard session-bus name");
        std::process::exit(2);
    }

    let connection = match Connection::session() {
        Ok(connection) => connection,
        Err(error) => {
            eprintln!("cannot connect to the session bus: {error}");
            std::process::exit(1);
        }
    };

    let introspect = match Proxy::new(
        &connection,
        PORTAL_BUS_NAME,
        PORTAL_PATH,
        "org.freedesktop.DBus.Introspectable",
    ) {
        Ok(proxy) => proxy,
        Err(error) => {
            eprintln!("cannot create portal introspection proxy: {error}");
            std::process::exit(1);
        }
    };
    let xml: String = match introspect.call("Introspect", &()) {
        Ok(xml) => xml,
        Err(error) => {
            eprintln!("portal introspection failed: {error}");
            std::process::exit(1);
        }
    };
    let missing = interfaces
        .iter()
        .copied()
        .filter(|interface| !xml.contains(&format!("name=\"{interface}\"")))
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        eprintln!("portal introspection omitted interfaces: {missing:?}");
        std::process::exit(1);
    }

    let dbus = match Proxy::new(
        &connection,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    ) {
        Ok(proxy) => proxy,
        Err(error) => {
            eprintln!("cannot create D-Bus owner proxy: {error}");
            std::process::exit(1);
        }
    };
    let owner: String = match dbus.call("GetNameOwner", &(PORTAL_BUS_NAME,)) {
        Ok(owner) => owner,
        Err(error) => {
            eprintln!("standard portal name has no session-bus owner: {error}");
            std::process::exit(1);
        }
    };

    println!(
        "{}",
        serde_json::json!({
            "status": "passed",
            "bus_name": PORTAL_BUS_NAME,
            "object_path": PORTAL_PATH,
            "owner": owner,
            "interfaces": interfaces,
            "request_interface": PORTAL_REQUEST_INTERFACE,
            "request_lifecycle": "dynamic_request_object_paths",
            "backend_scope": "frontend_registration_only",
            "live_pipewire": false,
            "permission_backend": false,
        })
    );
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("slopos-portal-smoke: Linux is required");
    std::process::exit(2);
}
