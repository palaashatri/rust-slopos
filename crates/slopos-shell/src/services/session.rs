//! Session power and state actions via standards-compliant APIs (D-Bus / logind) and verified command fallbacks.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

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

const SUSPEND_COMMANDS: &[(&str, &[&str])] = &[
    ("systemctl", &["suspend"]),
    ("loginctl", &["suspend"]),
    ("pm-suspend", &[]),
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

pub fn suspend_system() -> bool {
    try_commands(SUSPEND_COMMANDS)
}

pub fn reboot_system() -> bool {
    try_commands(REBOOT_COMMANDS)
}

pub fn poweroff_system() -> bool {
    try_commands(POWEROFF_COMMANDS)
}

pub fn can_switch_user() -> bool {
    resolve_first_command(SWITCH_USER_COMMANDS).is_some()
}

pub fn can_suspend() -> bool {
    resolve_first_command(SUSPEND_COMMANDS).is_some()
}

pub fn can_reboot() -> bool {
    resolve_first_command(REBOOT_COMMANDS).is_some()
}

pub fn can_poweroff() -> bool {
    resolve_first_command(POWEROFF_COMMANDS).is_some()
}

fn try_commands(candidates: &[(&str, &[&str])]) -> bool {
    for &(prog, args) in candidates {
        if let Some(path) = resolve_program(prog) {
            if Command::new(path).args(args).spawn().is_ok() {
                return true;
            }
        }
    }
    false
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
    if program.starts_with("slopos-") {
        if let Ok(executable) = env::current_exe() {
            if let Some(parent) = executable.parent() {
                let sibling = parent.join(program);
                if sibling.is_file() {
                    return Some(sibling);
                }
            }
        }
        let local = PathBuf::from("scripts").join(program);
        if local.is_file() {
            return Some(local);
        }
    }

    let path = Path::new(program);
    if path.components().count() > 1 {
        return path.is_file().then(|| path.to_path_buf());
    }

    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths)
            .map(|directory| directory.join(program))
            .find(|candidate| candidate.is_file())
    })
}
