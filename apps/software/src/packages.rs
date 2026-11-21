//! Install commands for the native, Flatpak, Snap and AUR sources.
//!
//! Each command is built by a pure function that returns argv, so the
//! arguments can be tested without running anything. No shell is involved,
//! and a package name is accepted only if it cannot be read as an option.
//!
//! Who asks for authority:
//! - native (apt, dnf, pacman) and Snap run through `pkexec`, because they
//!   change the whole system and need root;
//! - Flatpak runs as the user and lets flatpak itself ask through polkit for
//!   a system install;
//! - AUR helpers refuse to run as root, so they run as the user and call
//!   sudo themselves.

use crate::run::{self, Cancel, Stream};
use crate::sources::{Family, Source};

/// Longest package name accepted.
const MAX_NAME: usize = 255;

/// Accept a name a source reports: an ASCII letter or digit first, then
/// letters, digits and `+ . _ -`. A leading dash can never reach the tool.
pub fn validate_name(name: &str) -> Result<&str, String> {
    let first_ok = name
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric());
    let rest_ok = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '_' | '-'));
    if first_ok && rest_ok && name.len() <= MAX_NAME {
        Ok(name)
    } else {
        Err(format!(
            "“{name}” is not a package name that can be installed."
        ))
    }
}

/// Tools an install needs besides the tab's own search tool.
pub fn extra_tools(source: Source) -> &'static [&'static str] {
    match source {
        Source::Native | Source::Snap => &["pkexec"],
        Source::Flatpak | Source::Aur | Source::Slopos => &[],
    }
}

/// The install command for `name` from `source`. `family` is the native
/// package manager; `program` is the AUR helper (yay or paru) for AUR.
pub fn install_argv(
    source: Source,
    family: Option<Family>,
    program: &str,
    name: &str,
) -> Result<Vec<String>, String> {
    let name = validate_name(name)?;
    let argv: Vec<&str> = match source {
        Source::Native => match family {
            Some(Family::Apt) => vec![
                "pkexec",
                "env",
                "DEBIAN_FRONTEND=noninteractive",
                "apt-get",
                "install",
                "-y",
                name,
            ],
            Some(Family::Dnf) => vec!["pkexec", "dnf", "install", "-y", name],
            // Installing one package without a full sync is a partial
            // upgrade on Arch; the user should run a full upgrade first.
            Some(Family::Pacman) => {
                vec!["pkexec", "pacman", "-S", "--needed", "--noconfirm", name]
            }
            None => return Err("unavailable: no native package manager".into()),
        },
        Source::Flatpak => vec!["flatpak", "install", "--noninteractive", "-y", name],
        Source::Snap => vec!["pkexec", "snap", "install", name],
        Source::Aur => match program {
            "yay" | "paru" => vec![program, "-S", "--needed", "--noconfirm", name],
            other => return Err(format!("unavailable: {other} is not an AUR helper")),
        },
        Source::Slopos => return Err("the SLOPOS catalogue installs its own signed entries".into()),
    };
    Ok(argv.into_iter().map(str::to_owned).collect())
}

/// Run an install command, passing each non-empty output line to `on_line`.
///
/// A failure carries the program, its exit status and the last line it wrote
/// to stderr (or to stdout when stderr was silent), as the tool printed it.
pub fn run_install(
    argv: &[String],
    cancel: &Cancel,
    on_line: &mut dyn FnMut(&str),
) -> Result<(), String> {
    let mut last_error = String::new();
    let mut last_line = String::new();
    let status = run::run(argv, &[], cancel, &mut |stream: Stream, line: &str| {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        if stream == Stream::Err {
            last_error = line.to_owned();
        }
        last_line = line.to_owned();
        on_line(line);
    })?;
    // The exit status is the last line the install shows, success or not.
    let program = &argv[0];
    let outcome = match status.code() {
        Some(code) => format!("{program} exited with status {code}"),
        None => format!("{program} was stopped by a signal"),
    };
    on_line(&outcome);
    if status.success() {
        return Ok(());
    }
    let reason = if last_error.is_empty() {
        last_line
    } else {
        last_error
    };
    Err(if reason.is_empty() {
        outcome
    } else {
        format!("{outcome}: {reason}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(source: Source, family: Option<Family>, program: &str, name: &str) -> Vec<String> {
        install_argv(source, family, program, name).unwrap()
    }

    fn words(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| (*part).to_owned()).collect()
    }

    #[test]
    fn apt_installs_through_pkexec_without_prompts() {
        assert_eq!(
            argv(Source::Native, Some(Family::Apt), "apt-cache", "zstd"),
            words(&[
                "pkexec",
                "env",
                "DEBIAN_FRONTEND=noninteractive",
                "apt-get",
                "install",
                "-y",
                "zstd",
            ])
        );
    }

    #[test]
    fn dnf_installs_through_pkexec() {
        assert_eq!(
            argv(Source::Native, Some(Family::Dnf), "dnf", "firefox"),
            words(&["pkexec", "dnf", "install", "-y", "firefox"])
        );
    }

    #[test]
    fn pacman_installs_through_pkexec_and_skips_what_is_there() {
        assert_eq!(
            argv(Source::Native, Some(Family::Pacman), "pacman", "firefox"),
            words(&[
                "pkexec",
                "pacman",
                "-S",
                "--needed",
                "--noconfirm",
                "firefox"
            ])
        );
    }

    #[test]
    fn flatpak_installs_as_the_user_without_pkexec() {
        assert_eq!(
            argv(Source::Flatpak, None, "flatpak", "org.mozilla.firefox"),
            words(&[
                "flatpak",
                "install",
                "--noninteractive",
                "-y",
                "org.mozilla.firefox"
            ])
        );
    }

    #[test]
    fn snap_installs_through_pkexec() {
        assert_eq!(
            argv(Source::Snap, None, "snap", "vlc"),
            words(&["pkexec", "snap", "install", "vlc"])
        );
    }

    #[test]
    fn aur_uses_whichever_helper_is_present_and_never_pkexec() {
        assert_eq!(
            argv(Source::Aur, None, "yay", "zen-browser-bin"),
            words(&["yay", "-S", "--needed", "--noconfirm", "zen-browser-bin"])
        );
        assert_eq!(
            argv(Source::Aur, None, "paru", "librewolf-bin"),
            words(&["paru", "-S", "--needed", "--noconfirm", "librewolf-bin"])
        );
        assert_eq!(
            install_argv(Source::Aur, None, "sudo", "x").unwrap_err(),
            "unavailable: sudo is not an AUR helper"
        );
    }

    #[test]
    fn the_catalogue_never_installs_through_a_package_manager() {
        assert!(install_argv(Source::Slopos, None, "", "zstd").is_err());
        assert!(install_argv(Source::Native, None, "", "zstd").is_err());
    }

    #[test]
    fn names_that_could_be_options_or_shell_text_are_refused() {
        for bad in [
            "",
            "-rf",
            "--help",
            "a;rm -rf /",
            "$(id)",
            "a b",
            "é",
            "x/../y",
        ] {
            assert!(
                install_argv(Source::Flatpak, None, "flatpak", bad).is_err(),
                "{bad:?}"
            );
        }
        assert!(install_argv(Source::Snap, None, "snap", &"a".repeat(256)).is_err());
        assert_eq!(
            validate_name("g++-14_x.y+z").unwrap(),
            "g++-14_x.y+z",
            "the characters real package names use are allowed"
        );
    }

    #[test]
    fn only_native_and_snap_need_pkexec() {
        assert_eq!(extra_tools(Source::Native), &["pkexec"]);
        assert_eq!(extra_tools(Source::Snap), &["pkexec"]);
        assert!(extra_tools(Source::Flatpak).is_empty());
        assert!(extra_tools(Source::Aur).is_empty());
    }

    #[test]
    fn a_failing_install_reports_the_tools_own_last_error_line() {
        let cancel = Cancel::default();
        let mut lines = Vec::new();
        let failed = run_install(
            &words(&[
                "sh",
                "-c",
                "echo fetching; echo 'E: Unable to locate zstd' >&2; exit 100",
            ]),
            &cancel,
            &mut |line| lines.push(line.to_owned()),
        );
        assert_eq!(
            failed.unwrap_err(),
            "sh exited with status 100: E: Unable to locate zstd"
        );
        // The two pipes are read separately, so their lines may interleave;
        // the exit status comes last and is always shown.
        assert_eq!(
            lines.last().map(String::as_str),
            Some("sh exited with status 100")
        );
        lines.sort();
        assert_eq!(
            lines,
            [
                "E: Unable to locate zstd",
                "fetching",
                "sh exited with status 100"
            ]
        );
        assert_eq!(
            run_install(&words(&["sh", "-c", "exit 1"]), &cancel, &mut |_| {}).unwrap_err(),
            "sh exited with status 1"
        );
    }

    #[test]
    fn a_successful_install_is_ok_and_a_cancelled_one_says_so() {
        let cancel = Cancel::default();
        let mut lines = Vec::new();
        assert!(run_install(&words(&["true"]), &cancel, &mut |line| lines
            .push(line.to_owned()))
        .is_ok());
        assert_eq!(
            lines,
            ["true exited with status 0"],
            "success shows the status too"
        );
        cancel.cancel();
        assert_eq!(
            run_install(&words(&["true"]), &cancel, &mut |_| {}),
            Err(run::CANCELLED.to_owned())
        );
    }
}
