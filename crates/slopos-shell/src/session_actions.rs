//! Session power / logout actions (pure plan + optional logind).
//!
//! Pure helpers decide which action to take; Linux may invoke logind / systemctl later.

/// High-level session action requested by the shell UI.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionAction {
    Logout,
    Lock,
    Suspend,
    Reboot,
    PowerOff,
}

impl SessionAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Logout => "logout",
            Self::Lock => "lock",
            Self::Suspend => "suspend",
            Self::Reboot => "reboot",
            Self::PowerOff => "poweroff",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "logout" | "log-out" | "exit" | "log_out" => Some(Self::Logout),
            "lock" | "lock-screen" | "lock_screen" => Some(Self::Lock),
            "suspend" | "sleep" => Some(Self::Suspend),
            "reboot" | "restart" => Some(Self::Reboot),
            "poweroff" | "power-off" | "shutdown" | "power_off" => Some(Self::PowerOff),
            _ => None,
        }
    }

    /// Map shell menu action ids (`shell.lock`, `shell.log_out`, …).
    pub fn from_menu_action(action: &str) -> Option<Self> {
        match action {
            "shell.lock" => Some(Self::Lock),
            "shell.log_out" | "shell.logout" => Some(Self::Logout),
            "shell.suspend" | "shell.sleep" => Some(Self::Suspend),
            "shell.reboot" | "shell.restart" => Some(Self::Reboot),
            "shell.power_off" | "shell.shutdown" | "shell.poweroff" => Some(Self::PowerOff),
            // Quit is process exit; treat as logout plan when requested via session API.
            _ => None,
        }
    }
}

/// Concrete execution plan (no side effects).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionActionPlan {
    /// In-process lock (shell handles).
    ShellLock,
    /// Exit shell process (session end).
    ShellExit { code: i32 },
    /// logind / systemctl argv (caller may spawn).
    SystemCommand { argv: Vec<String> },
    /// logind D-Bus method plan (bus name / path / interface / member).
    LogindMethod {
        method: &'static str,
        /// Interactive flag for logind Reboot/PowerOff/Suspend.
        interactive: bool,
    },
}

/// How to prefer executing power actions.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum PowerBackend {
    /// `systemctl suspend|reboot|poweroff`
    #[default]
    Systemctl,
    /// org.freedesktop.login1 Manager methods
    Logind,
}

/// Pure: map a session action to an execution plan.
pub fn plan_session_action(action: SessionAction) -> SessionActionPlan {
    plan_session_action_with(action, PowerBackend::Systemctl, false)
}

/// Pure: map action with backend preference.
pub fn plan_session_action_with(
    action: SessionAction,
    backend: PowerBackend,
    interactive: bool,
) -> SessionActionPlan {
    match action {
        SessionAction::Lock => SessionActionPlan::ShellLock,
        SessionAction::Logout => SessionActionPlan::ShellExit { code: 0 },
        SessionAction::Suspend => match backend {
            PowerBackend::Systemctl => SessionActionPlan::SystemCommand {
                argv: vec!["systemctl".into(), "suspend".into()],
            },
            PowerBackend::Logind => SessionActionPlan::LogindMethod {
                method: "Suspend",
                interactive,
            },
        },
        SessionAction::Reboot => match backend {
            PowerBackend::Systemctl => SessionActionPlan::SystemCommand {
                argv: vec!["systemctl".into(), "reboot".into()],
            },
            PowerBackend::Logind => SessionActionPlan::LogindMethod {
                method: "Reboot",
                interactive,
            },
        },
        SessionAction::PowerOff => match backend {
            PowerBackend::Systemctl => SessionActionPlan::SystemCommand {
                argv: vec!["systemctl".into(), "poweroff".into()],
            },
            PowerBackend::Logind => SessionActionPlan::LogindMethod {
                method: "PowerOff",
                interactive,
            },
        },
    }
}

/// Whether the plan requires elevated system privileges / polkit.
pub fn plan_requires_privileges(plan: &SessionActionPlan) -> bool {
    matches!(
        plan,
        SessionActionPlan::SystemCommand { .. } | SessionActionPlan::LogindMethod { .. }
    )
}

/// Confirm prompt text for destructive actions (pure English; i18n via catalog keys).
pub fn confirm_prompt(action: SessionAction) -> Option<&'static str> {
    match action {
        SessionAction::Reboot => Some("Restart the computer now?"),
        SessionAction::PowerOff => Some("Shut down the computer now?"),
        SessionAction::Logout => Some("Log out of SLOPOS-I?"),
        _ => None,
    }
}

/// i18n catalog key for confirm prompts.
pub fn confirm_prompt_i18n_key(action: SessionAction) -> Option<&'static str> {
    match action {
        SessionAction::Reboot => Some("confirm.reboot"),
        SessionAction::PowerOff => Some("confirm.poweroff"),
        SessionAction::Logout => Some("confirm.logout"),
        _ => None,
    }
}

/// Human-readable one-line description of a plan (logs / UI).
pub fn describe_plan(plan: &SessionActionPlan) -> String {
    match plan {
        SessionActionPlan::ShellLock => "shell: lock screen".into(),
        SessionActionPlan::ShellExit { code } => format!("shell: exit({code})"),
        SessionActionPlan::SystemCommand { argv } => format!("exec: {}", argv.join(" ")),
        SessionActionPlan::LogindMethod {
            method,
            interactive,
        } => format!("logind: {method}(interactive={interactive})"),
    }
}

/// Whether the action should show a confirmation dialog before execute.
pub fn requires_confirmation(action: SessionAction) -> bool {
    confirm_prompt(action).is_some()
}

/// Apply shell-local side of a plan to session flags (pure state transition).
///
/// Returns `true` if the shell should mark itself locked / exiting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShellSessionDelta {
    pub lock: bool,
    pub request_exit: bool,
    pub exit_code: i32,
}

pub fn shell_delta_for_plan(plan: &SessionActionPlan) -> ShellSessionDelta {
    match plan {
        SessionActionPlan::ShellLock => ShellSessionDelta {
            lock: true,
            request_exit: false,
            exit_code: 0,
        },
        SessionActionPlan::ShellExit { code } => ShellSessionDelta {
            lock: false,
            request_exit: true,
            exit_code: *code,
        },
        SessionActionPlan::SystemCommand { .. } | SessionActionPlan::LogindMethod { .. } => {
            ShellSessionDelta {
                lock: false,
                request_exit: false,
                exit_code: 0,
            }
        }
    }
}

/// logind well-known bus constants (for D-Bus clients).
pub const LOGIND_BUS: &str = "org.freedesktop.login1";
pub const LOGIND_PATH: &str = "/org/freedesktop/login1";
pub const LOGIND_MANAGER_IFACE: &str = "org.freedesktop.login1.Manager";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_plan_lock_logout() {
        assert_eq!(SessionAction::parse("lock"), Some(SessionAction::Lock));
        assert_eq!(
            plan_session_action(SessionAction::Lock),
            SessionActionPlan::ShellLock
        );
        assert_eq!(
            plan_session_action(SessionAction::Logout),
            SessionActionPlan::ShellExit { code: 0 }
        );
        assert_eq!(
            SessionAction::from_menu_action("shell.log_out"),
            Some(SessionAction::Logout)
        );
    }

    #[test]
    fn power_actions_use_systemctl() {
        let p = plan_session_action(SessionAction::Reboot);
        assert!(plan_requires_privileges(&p));
        match p {
            SessionActionPlan::SystemCommand { argv } => {
                assert_eq!(argv, vec!["systemctl", "reboot"]);
            }
            _ => panic!("expected system command"),
        }
        assert!(confirm_prompt(SessionAction::PowerOff).is_some());
        assert!(confirm_prompt(SessionAction::Lock).is_none());
        assert!(requires_confirmation(SessionAction::Logout));
    }

    #[test]
    fn logind_backend() {
        let p = plan_session_action_with(SessionAction::Suspend, PowerBackend::Logind, true);
        match p {
            SessionActionPlan::LogindMethod {
                method,
                interactive,
            } => {
                assert_eq!(method, "Suspend");
                assert!(interactive);
            }
            _ => panic!("expected logind"),
        }
        assert!(describe_plan(&p).contains("Suspend"));
    }

    #[test]
    fn shell_delta() {
        let d = shell_delta_for_plan(&SessionActionPlan::ShellLock);
        assert!(d.lock && !d.request_exit);
        let d = shell_delta_for_plan(&SessionActionPlan::ShellExit { code: 0 });
        assert!(d.request_exit);
    }
}
