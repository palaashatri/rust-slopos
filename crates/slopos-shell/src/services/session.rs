//! Session power and state actions via standards-compliant APIs (D-Bus / logind) and verified command fallbacks.

use std::path::PathBuf;
use std::sync::OnceLock;

/// Verified concrete locker binaries that directly lock X11 displays.
const CONCRETE_LOCKER_COMMANDS: &[(&str, &[&str])] = &[
    ("light-locker-command", &["-l"]),
    ("i3lock", &["-c", "758090"]),
    ("slock", &[]),
    ("xscreensaver-command", &["-lock"]),
    ("xlock", &[]),
    ("xflock4", &[]),
];

const SWITCH_USER_COMMANDS: &[(&str, &[&str])] = &[
    ("gdmflexiserver", &[]),
    ("dm-tool", &["switch-to-greeter"]),
    ("kdmctl", &["reserve"]),
];

const REBOOT_COMMANDS: &[(&str, &[&str])] = &[
    ("systemctl", &["reboot"]),
    ("loginctl", &["reboot"]),
    ("reboot", &[]),
];

const POWEROFF_COMMANDS: &[(&str, &[&str])] = &[
    ("systemctl", &["poweroff"]),
    ("loginctl", &["poweroff"]),
    ("poweroff", &[]),
];

/// Lock the session using freedesktop/GNOME screensaver D-Bus APIs or verified system lockers.
/// Returns true if an active locking mechanism was successfully invoked.
pub fn lock_screen() -> bool {
    // 1. Try freedesktop screensaver D-Bus interface
    if lock_via_dbus(
        "org.freedesktop.ScreenSaver",
        "/org/freedesktop/ScreenSaver",
        "org.freedesktop.ScreenSaver",
        "Lock",
    ) {
        return true;
    }

    // 2. Try GNOME screensaver D-Bus interface
    if lock_via_dbus(
        "org.gnome.ScreenSaver",
        "/org/gnome/ScreenSaver",
        "org.gnome.ScreenSaver",
        "Lock",
    ) {
        return true;
    }

    // 3. Try verified standalone locker binaries
    if try_commands(CONCRETE_LOCKER_COMMANDS) {
        return true;
    }

    false
}

/// Determine whether a functional screen locker is actually present and callable.
/// Does not report true on generic logind presence alone if no locker daemon or binary exists.
pub fn can_lock_screen() -> bool {
    // Check if session bus has an active ScreenSaver provider
    if has_dbus_screensaver() {
        return true;
    }

    // Check if any verified concrete locker binary is installed in PATH
    resolve_first_command(CONCRETE_LOCKER_COMMANDS).is_some()
}

fn has_dbus_screensaver() -> bool {
    let Ok(connection) = zbus::blocking::Connection::session() else {
        return false;
    };

    let dbus_proxy = match zbus::blocking::fdo::DBusProxy::new(&connection) {
        Ok(proxy) => proxy,
        Err(_) => return false,
    };

    for service in &["org.freedesktop.ScreenSaver", "org.gnome.ScreenSaver"] {
        if let Ok(service_name) = (*service).try_into() {
            if let Ok(has_owner) = dbus_proxy.name_has_owner(service_name) {
                if has_owner {
                    return true;
                }
            }
        }
    }

    false
}

fn lock_via_dbus(service: &str, path: &str, interface: &str, method: &str) -> bool {
    let Ok(connection) = zbus::blocking::Connection::session() else {
        return false;
    };

    let proxy = match zbus::blocking::Proxy::new(&connection, service, path, interface) {
        Ok(proxy) => proxy,
        Err(_) => return false,
    };

    let result: zbus::Result<()> = proxy.call(method, &());
    result.is_ok()
}

pub fn switch_user() -> bool {
    try_commands(SWITCH_USER_COMMANDS)
}

/// Reboot through logind (which handles polkit authorization and inhibitors),
/// falling back to the classic commands.
pub fn reboot_system() -> Result<(), String> {
    logind_call("Reboot").or_else(|error| {
        if run_commands(REBOOT_COMMANDS) {
            Ok(())
        } else {
            Err(error)
        }
    })
}

pub fn poweroff_system() -> Result<(), String> {
    logind_call("PowerOff").or_else(|error| {
        if run_commands(POWEROFF_COMMANDS) {
            Ok(())
        } else {
            Err(error)
        }
    })
}

fn logind_proxy(
    connection: &zbus::blocking::Connection,
) -> zbus::Result<zbus::blocking::Proxy<'_>> {
    zbus::blocking::Proxy::new(
        connection,
        "org.freedesktop.login1",
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
    )
}

fn logind_call(method: &str) -> Result<(), String> {
    let connection = zbus::blocking::Connection::system()
        .map_err(|error| format!("the system bus is unavailable: {error}"))?;
    let proxy = logind_proxy(&connection).map_err(|error| error.to_string())?;
    // `true` lets polkit ask for a password when one is required.
    proxy
        .call::<_, _, ()>(method, &(true,))
        .map_err(|error| format!("logind refused {method}: {error}"))
}

/// `Can*` answers are "yes", "challenge" (needs authentication), "no" or "na".
fn logind_can(method: &str) -> bool {
    let Ok(connection) = zbus::blocking::Connection::system() else {
        return false;
    };
    let Ok(proxy) = logind_proxy(&connection) else {
        return false;
    };
    proxy
        .call::<_, _, String>(method, &())
        .map(|answer| matches!(answer.as_str(), "yes" | "challenge"))
        .unwrap_or(false)
}

pub fn can_switch_user() -> bool {
    resolve_first_command(SWITCH_USER_COMMANDS).is_some()
}

// Menus are rebuilt on focus changes, so ask logind once per shell lifetime.
pub fn can_reboot() -> bool {
    static CAN: OnceLock<bool> = OnceLock::new();
    *CAN.get_or_init(|| logind_can("CanReboot") || resolve_first_command(REBOOT_COMMANDS).is_some())
}

pub fn can_poweroff() -> bool {
    static CAN: OnceLock<bool> = OnceLock::new();
    *CAN.get_or_init(|| {
        logind_can("CanPowerOff") || resolve_first_command(POWEROFF_COMMANDS).is_some()
    })
}

fn try_commands(candidates: &[(&str, &[&str])]) -> bool {
    candidates.iter().any(|&(program, args)| {
        resolve_program(program)
            .map(|path| slopos_core::process::spawn_detached(&path, args).is_ok())
            .unwrap_or(false)
    })
}

/// Run fallback commands to completion and succeed only when one exits 0.
/// A helper that starts but fails (`systemctl` without permission, say) must
/// not hide the original logind error.
fn run_commands(candidates: &[(&str, &[&str])]) -> bool {
    candidates.iter().any(|&(program, args)| {
        resolve_program(program)
            .and_then(|path| std::process::Command::new(path).args(args).status().ok())
            .is_some_and(|status| status.success())
    })
}

pub fn resolve_first_command(
    candidates: &[(&'static str, &'static [&'static str])],
) -> Option<(&'static str, &'static [&'static str])> {
    for &(program, args) in candidates {
        if resolve_program(program).is_some() {
            return Some((program, args));
        }
    }
    None
}

pub fn resolve_program(program: &str) -> Option<PathBuf> {
    slopos_core::process::resolve_program(program)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fallback_only_counts_when_it_exits_successfully() {
        assert!(!run_commands(&[("false", &[])]));
        assert!(run_commands(&[("false", &[]), ("true", &[])]));
        assert!(!run_commands(&[("definitely-not-a-command-3f5a", &[])]));
    }
}
