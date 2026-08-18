//! Session power and state actions via logind and bounded command fallbacks.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

const LOCK_COMMANDS: &[(&str, &[&str])] = &[
    ("loginctl", &["lock-session"]),
    ("xflock4", &[]),
    ("light-locker-command", &["-l"]),
    ("dm-tool", &["lock"]),
    ("xdg-screensaver", &["lock"]),
    ("slock", &[]),
    ("i3lock", &["-c", "758090"]),
];

const SWITCH_USER_COMMANDS: &[(&str, &[&str])] = &[
    ("dm-tool", &["switch-to-greeter"]),
    ("gdmflexiserver", &[]),
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

pub fn lock_screen() -> bool {
    try_commands(LOCK_COMMANDS)
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

pub fn can_lock_screen() -> bool {
    resolve_first_command(LOCK_COMMANDS).is_some()
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
