//! Finding and launching desktop programs.
//!
//! Every shell surface launches applications the same way: resolve the program
//! (sibling SLOPOS binaries first, then `PATH`), start it in its own session so
//! it outlives a shell restart, and reap it on a background thread so a
//! long-running shell never accumulates zombie processes.

use std::env;
use std::ffi::OsStr;
use std::io;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Output, Stdio};
use std::thread;

/// Resolve `program` to an executable path.
///
/// `slopos-*` programs are looked up next to the running executable first, so
/// an installed or `target/` build always finds its own sibling binaries.
/// A name containing a path separator is used as given.
pub fn resolve_program(program: &str) -> Option<PathBuf> {
    resolve_program_in(program, env::current_exe().ok(), env::var_os("PATH"))
}

fn resolve_program_in(
    program: &str,
    current_exe: Option<PathBuf>,
    path_var: Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    if program.is_empty() {
        return None;
    }
    if program.starts_with("slopos-") || program == "start-slopos-i" {
        if let Some(dir) = current_exe.as_deref().and_then(Path::parent) {
            let sibling = dir.join(program);
            if is_executable(&sibling) {
                return Some(sibling);
            }
        }
    }
    let path = Path::new(program);
    if path.components().count() > 1 {
        return is_executable(path).then(|| path.to_path_buf());
    }
    env::split_paths(&path_var?)
        .map(|dir| dir.join(program))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Return the first candidate program that is installed, with its arguments.
pub fn first_available<'a>(
    candidates: &'a [(&'a str, &'a [&'a str])],
) -> Option<(PathBuf, &'a [&'a str])> {
    candidates
        .iter()
        .find_map(|(program, args)| resolve_program(program).map(|path| (path, *args)))
}

/// Start `program` detached from the caller and reap it when it exits.
///
/// The child gets its own session (so closing or restarting the shell does not
/// signal it) and does not inherit the caller's stdin.
pub fn spawn_detached<I, S>(program: &Path, args: I) -> io::Result<u32>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new(program);
    command.args(args).stdin(Stdio::null());
    // SAFETY: setsid is async-signal-safe and touches no Rust state.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command.spawn()?;
    let pid = child.id();
    thread::Builder::new()
        .name(format!("reap-{pid}"))
        .spawn(move || {
            let _ = child.wait();
        })?;
    Ok(pid)
}

/// Resolve and launch a program by name. Returns a user-readable error when it
/// is not installed or cannot start.
pub fn launch(program: &str, args: &[&str]) -> Result<u32, String> {
    let path = resolve_program(program).ok_or_else(|| format!("{program} is not installed"))?;
    spawn_detached(&path, args).map_err(|error| format!("could not start {program}: {error}"))
}

/// A program started with piped stdin and stdout whose reply the caller
/// needs. Unlike [`spawn_detached`] it stays in the caller's session: it is
/// the caller's child, and the caller is responsible for its reply.
///
/// Reaping is guaranteed either way: [`CapturedChild::wait_with_output`]
/// waits for the program, and dropping the value without waiting hands the
/// child to a background thread that waits for it.
#[derive(Debug)]
pub struct CapturedChild {
    child: Option<Child>,
}

impl CapturedChild {
    /// The process id of the running program, 0 once it has been waited for.
    pub fn id(&self) -> u32 {
        self.child.as_ref().map(Child::id).unwrap_or(0)
    }

    /// The program's stdin, once. Dropping it closes the pipe.
    pub fn take_stdin(&mut self) -> Option<ChildStdin> {
        self.child.as_mut().and_then(|child| child.stdin.take())
    }

    /// Wait for the program to exit and collect what it wrote to stdout.
    pub fn wait_with_output(mut self) -> io::Result<Output> {
        match self.child.take() {
            Some(child) => child.wait_with_output(),
            None => Err(io::Error::other("the child was already waited for")),
        }
    }
}

impl Drop for CapturedChild {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // Nobody waits for it any more, so reap it on a thread. If the
            // thread cannot start, the child is dropped unwaited and stays a
            // zombie until this process exits.
            let _ = thread::Builder::new()
                .name(format!("reap-{}", child.id()))
                .spawn(move || {
                    let _ = child.wait();
                });
        }
    }
}

/// Resolve the first installed program in `candidates` and start it with
/// piped stdin and stdout, passing its candidate arguments. It uses the same
/// candidate lists as [`launch`], so callers name programs in preference
/// order and never hard-code one.
///
/// Returns a user-readable error when no candidate is installed or the
/// program cannot start.
pub fn launch_captured(candidates: &[(&str, &[&str])]) -> Result<CapturedChild, String> {
    let (path, args) = first_available(candidates).ok_or_else(|| {
        let names: Vec<&str> = candidates.iter().map(|(program, _)| *program).collect();
        format!("none of these is installed: {}", names.join(", "))
    })?;
    let child = Command::new(&path)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not start {}: {error}", path.display()))?;
    Ok(CapturedChild { child: Some(child) })
}

/// Programs that show a file chooser for the portal, in preference order.
/// Only the SLOPOS chooser: a donor chooser cannot honour the portal's
/// request shape, so no fallback is listed.
pub const FILE_CHOOSERS: &[(&str, &[&str])] = &[("slopos-file-chooser", &[])];

/// Terminal emulators in preference order. `slopos-terminal` is the SLOPOS
/// terminal, found beside the other SLOPOS programs or on `PATH`, so it comes
/// first. The donor emulators follow as fallbacks, and `x-terminal-emulator`
/// honours the Debian alternatives system, so the user's configured choice
/// wins among them.
pub const TERMINALS: &[(&str, &[&str])] = &[
    ("slopos-terminal", &[]),
    ("x-terminal-emulator", &[]),
    ("xdg-terminal-exec", &[]),
    ("xfce4-terminal", &[]),
    ("gnome-terminal", &[]),
    ("konsole", &[]),
    ("lxterminal", &[]),
    ("alacritty", &[]),
    ("kitty", &[]),
    ("xterm", &[]),
];

/// Plain-text editors in preference order.
pub const TEXT_EDITORS: &[(&str, &[&str])] = &[
    ("mousepad", &[]),
    ("gnome-text-editor", &[]),
    ("gedit", &[]),
    ("pluma", &[]),
    ("xed", &[]),
    ("kate", &[]),
    ("featherpad", &[]),
    ("leafpad", &[]),
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn make_executable(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("slopos-core-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn finds_programs_on_path_and_skips_non_executables() {
        let dir = temp_dir("path");
        make_executable(&dir, "tool");
        fs::write(dir.join("data"), "not a program").unwrap();
        let path_var = Some(dir.clone().into_os_string());
        assert_eq!(
            resolve_program_in("tool", None, path_var.clone()),
            Some(dir.join("tool"))
        );
        assert_eq!(resolve_program_in("data", None, path_var.clone()), None);
        assert_eq!(resolve_program_in("missing", None, path_var), None);
    }

    #[test]
    fn slopos_siblings_win_over_path() {
        let bin = temp_dir("bin");
        let other = temp_dir("other");
        make_executable(&bin, "slopos-settings");
        make_executable(&other, "slopos-settings");
        let exe = bin.join("slopos-shell");
        let found = resolve_program_in(
            "slopos-settings",
            Some(exe),
            Some(other.clone().into_os_string()),
        );
        assert_eq!(found, Some(bin.join("slopos-settings")));
    }

    #[test]
    fn slopos_terminal_is_the_first_terminal_and_the_donors_follow_in_order() {
        let programs: Vec<&str> = TERMINALS.iter().map(|(program, _)| *program).collect();
        assert_eq!(
            programs,
            [
                "slopos-terminal",
                "x-terminal-emulator",
                "xdg-terminal-exec",
                "xfce4-terminal",
                "gnome-terminal",
                "konsole",
                "lxterminal",
                "alacritty",
                "kitty",
                "xterm",
            ]
        );
    }

    #[test]
    fn the_first_installed_terminal_wins_and_slopos_terminal_is_preferred() {
        let dir = temp_dir("terminals");
        make_executable(&dir, "xterm");
        let path_var = Some(dir.clone().into_os_string());
        // Only xterm is installed: it is the first candidate that resolves.
        let first = |path: Option<std::ffi::OsString>| {
            TERMINALS
                .iter()
                .find_map(|(program, _)| resolve_program_in(program, None, path.clone()))
        };
        assert_eq!(first(path_var.clone()), Some(dir.join("xterm")));
        // Once slopos-terminal is installed beside it, that one wins.
        make_executable(&dir, "slopos-terminal");
        assert_eq!(first(path_var), Some(dir.join("slopos-terminal")));
    }

    #[test]
    fn empty_names_never_resolve() {
        assert_eq!(resolve_program_in("", None, Some("/bin".into())), None);
    }

    #[test]
    fn detached_children_are_reaped() {
        let pid = spawn_detached(Path::new("/bin/sh"), ["-c", "exit 0"]).unwrap();
        // The reaper thread waits on the child; once it has run, the pid no
        // longer names a zombie of ours.
        for _ in 0..100 {
            let status = fs::read_to_string(format!("/proc/{pid}/status"));
            if status.map(|s| !s.contains("State:\tZ")).unwrap_or(true) {
                let gone = !Path::new(&format!("/proc/{pid}")).exists();
                if gone {
                    return;
                }
            }
            thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("child {pid} was not reaped");
    }

    #[test]
    fn captured_children_pass_stdin_through_and_return_stdout() {
        let args: &[&str] = &["-c", "cat"];
        let candidates: &[(&str, &[&str])] = &[("/bin/sh", args)];
        let mut child = launch_captured(candidates).expect("sh is installed");
        assert_ne!(child.id(), 0);
        let mut stdin = child.take_stdin().expect("piped stdin");
        std::io::Write::write_all(&mut stdin, b"reply").unwrap();
        drop(stdin);
        let output = child.wait_with_output().expect("wait");
        assert!(output.status.success());
        assert_eq!(output.stdout, b"reply");
    }

    #[test]
    fn the_first_installed_candidate_is_used_with_its_arguments() {
        let missing: &[(&str, &[&str])] = &[("/nonexistent/slopos-test-program", &[])];
        let args: &[&str] = &["-c", "printf chosen"];
        let candidates: &[(&str, &[&str])] =
            &[("/nonexistent/slopos-test-program", &[]), ("/bin/sh", args)];
        let child = launch_captured(candidates).expect("the second candidate starts");
        let output = child.wait_with_output().unwrap();
        assert_eq!(output.stdout, b"chosen");
        let error = launch_captured(missing).expect_err("nothing is installed");
        assert!(error.contains("none of these is installed"), "{error}");
        assert!(error.contains("slopos-test-program"), "{error}");
    }

    #[test]
    fn a_dropped_captured_child_is_still_reaped() {
        let args: &[&str] = &["-c", "exit 0"];
        let candidates: &[(&str, &[&str])] = &[("/bin/sh", args)];
        let child = launch_captured(candidates).expect("sh is installed");
        let pid = child.id();
        drop(child);
        for _ in 0..100 {
            if !Path::new(&format!("/proc/{pid}")).exists() {
                return;
            }
            thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("dropped child {pid} was not reaped");
    }

    #[test]
    fn the_file_chooser_is_the_slopos_program_only() {
        let programs: Vec<&str> = FILE_CHOOSERS.iter().map(|(program, _)| *program).collect();
        assert_eq!(programs, ["slopos-file-chooser"]);
    }
}
