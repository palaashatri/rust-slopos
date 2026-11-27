//! The polkit authentication helper: `polkit-agent-helper-1`, the setuid
//! program that runs PAM for the agent. The agent starts it with the username
//! and the cookie polkit gave it, relays PAM's prompts to the user, and writes
//! back the password. Verification happens inside the helper and polkit; this
//! module never checks a password itself.
//!
//! Protocol (as shipped by polkit; checked against the helper's source
//! strings, not yet against a running polkitd, so it is NOT_RUN):
//! - stdin, first line: the cookie;
//! - stdout lines: `PAM_PROMPT_ECHO_OFF <text>` / `PAM_PROMPT_ECHO_ON <text>`
//!   (answer with one line on stdin), `PAM_ERROR_MSG <text>`,
//!   `PAM_TEXT_INFO <text>`, then `SUCCESS` or `FAILURE`.
//!
//! On `SUCCESS` the helper has already answered the Authority; the agent
//! then calls `AuthenticationAgentResponse2` itself (see `agent.rs`).

use std::{
    ffi::CStr,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{ChildStdin, ChildStdout, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

use zeroize::Zeroizing;

/// Where the helper is installed, in preference order (Debian/Ubuntu, then
/// Fedora/Arch).
pub const HELPER_CANDIDATES: &[&str] = &[
    "/usr/lib/polkit-1/polkit-agent-helper-1",
    "/usr/libexec/polkit-agent-helper-1",
];

/// One parsed line of helper output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelperLine {
    /// PAM asks for an answer; the text is the prompt.
    Prompt(String),
    /// PAM error text, shown to the user (for example "Sorry, try again.").
    Error(String),
    /// PAM informational text.
    Info(String),
    Success,
    Failure,
    /// Anything else; ignored so a newer helper cannot break the agent.
    Unknown,
}

pub fn parse_line(line: &str) -> HelperLine {
    let line = line.trim_end_matches(['\r', '\n']);
    let (token, text) = match line.split_once(' ') {
        Some((token, text)) => (token, text),
        None => (line, ""),
    };
    match token {
        "PAM_PROMPT_ECHO_OFF" | "PAM_PROMPT_ECHO_ON" => HelperLine::Prompt(text.to_owned()),
        "PAM_ERROR_MSG" => HelperLine::Error(text.to_owned()),
        "PAM_TEXT_INFO" => HelperLine::Info(text.to_owned()),
        "SUCCESS" => HelperLine::Success,
        "FAILURE" => HelperLine::Failure,
        _ => HelperLine::Unknown,
    }
}

/// The user's side of a PAM conversation, implemented by the agent with the
/// dialog. A secret never outlives the call that receives it.
pub trait Conversation {
    /// Ask for a secret answer. `None` means the user cancelled.
    fn secret(&mut self, prompt: &str) -> Option<Zeroizing<String>>;
    fn error(&mut self, text: &str);
    fn info(&mut self, text: &str);
}

/// Lets another thread stop a running helper. Cancelling kills the helper
/// process (if one is running) and makes every later attach fail, so a cancel
/// that arrives before the helper starts still wins.
#[derive(Debug, Default)]
pub struct CancelToken {
    state: Mutex<CancelState>,
    cancelled: AtomicBool,
}

#[derive(Debug, Default)]
struct CancelState {
    pid: Option<u32>,
}

impl CancelToken {
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn cancel(&self) {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        self.cancelled.store(true, Ordering::SeqCst);
        if let Some(pid) = state.pid {
            // SAFETY: plain kill(2) on a pid this process spawned and has not
            // yet reaped; `state` is locked, so the pid cannot be detached.
            unsafe {
                libc::kill(pid as libc::pid_t, libc::SIGKILL);
            }
        }
    }

    /// Record the helper's pid. Returns false when already cancelled.
    fn attach(&self, pid: u32) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.pid = Some(pid);
        !self.is_cancelled()
    }

    fn detach(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.pid = None;
    }
}

/// How one helper run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelperOutcome {
    /// The helper printed `SUCCESS`.
    Authenticated,
    /// The helper printed `FAILURE` after the user had answered a prompt.
    WrongPassword,
    /// Any other failure, with the helper's last message.
    Failed(String),
    /// The user or polkit cancelled.
    Cancelled,
    /// The helper could not be started.
    Unavailable(String),
}

/// Runs one authentication. Implemented by [`PamHelper`]; tests substitute
/// fakes.
pub trait Helper: Send + Sync + 'static {
    fn authenticate(
        &self,
        username: &str,
        cookie: &str,
        conversation: &mut dyn Conversation,
        cancel: &CancelToken,
    ) -> HelperOutcome;
}

/// The real helper binary at `path`.
#[derive(Debug, Clone)]
pub struct PamHelper {
    path: PathBuf,
}

impl PamHelper {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The first installed helper from [`HELPER_CANDIDATES`], if any.
    pub fn installed() -> Option<Self> {
        HELPER_CANDIDATES
            .iter()
            .map(Path::new)
            .find(|path| path.is_file())
            .map(Self::new)
    }
}

impl Helper for PamHelper {
    fn authenticate(
        &self,
        username: &str,
        cookie: &str,
        conversation: &mut dyn Conversation,
        cancel: &CancelToken,
    ) -> HelperOutcome {
        let mut child = match Command::new(&self.path)
            .arg(username)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => return HelperOutcome::Unavailable(error.to_string()),
        };
        if !cancel.attach(child.id()) {
            let _ = child.kill();
            let _ = child.wait();
            return HelperOutcome::Cancelled;
        }
        let mut stdin = child.stdin.take().expect("stdin was piped");
        let stdout = child.stdout.take().expect("stdout was piped");
        let outcome = drive(&mut stdin, stdout, cookie, conversation);
        cancel.detach();
        drop(stdin);
        if matches!(outcome, HelperOutcome::Cancelled) || cancel.is_cancelled() {
            let _ = child.kill();
        }
        let _ = child.wait();
        if cancel.is_cancelled() {
            return HelperOutcome::Cancelled;
        }
        outcome
    }
}

/// Speak the helper protocol over an open pipe pair. Pure I/O, so tests can
/// drive it with a shell script.
fn drive(
    stdin: &mut ChildStdin,
    stdout: ChildStdout,
    cookie: &str,
    conversation: &mut dyn Conversation,
) -> HelperOutcome {
    if writeln!(stdin, "{cookie}")
        .and_then(|()| stdin.flush())
        .is_err()
    {
        return HelperOutcome::Failed("the helper did not accept the cookie".into());
    }
    let mut asked_for_secret = false;
    let mut last_message = String::from("the helper exited without an answer");
    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        match parse_line(&line) {
            HelperLine::Prompt(prompt) => match conversation.secret(&prompt) {
                None => return HelperOutcome::Cancelled,
                Some(password) => {
                    asked_for_secret = true;
                    // The line buffer is wiped as soon as it is written.
                    let answer = Zeroizing::new(format!("{}\n", password.as_str()));
                    if stdin
                        .write_all(answer.as_bytes())
                        .and_then(|()| stdin.flush())
                        .is_err()
                    {
                        return HelperOutcome::Failed("the helper stopped reading".into());
                    }
                }
            },
            HelperLine::Error(text) => {
                conversation.error(&text);
                last_message = text;
            }
            HelperLine::Info(text) => conversation.info(&text),
            HelperLine::Success => return HelperOutcome::Authenticated,
            HelperLine::Failure => {
                return if asked_for_secret {
                    HelperOutcome::WrongPassword
                } else {
                    HelperOutcome::Failed(last_message)
                };
            }
            HelperLine::Unknown => {}
        }
    }
    HelperOutcome::Failed(last_message)
}

/// The account name for `uid`, from the password database.
pub fn username_for_uid(uid: u32) -> Option<String> {
    let mut buffer = vec![0 as libc::c_char; 16 * 1024];
    // SAFETY: getpwuid_r writes into `passwd` and `buffer`, both owned here
    // and sized correctly; the returned name is copied out before `buffer`
    // is dropped.
    unsafe {
        let mut passwd: libc::passwd = std::mem::zeroed();
        let mut result: *mut libc::passwd = std::ptr::null_mut();
        let status = libc::getpwuid_r(
            uid,
            &mut passwd,
            buffer.as_mut_ptr(),
            buffer.len(),
            &mut result,
        );
        if status != 0 || result.is_null() {
            return None;
        }
        CStr::from_ptr(passwd.pw_name)
            .to_str()
            .ok()
            .map(str::to_owned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::VecDeque, os::unix::fs::PermissionsExt, time::Instant};

    #[derive(Default)]
    struct Script {
        answers: VecDeque<Option<String>>,
        prompts: Vec<String>,
        errors: Vec<String>,
    }

    impl Conversation for Script {
        fn secret(&mut self, prompt: &str) -> Option<Zeroizing<String>> {
            self.prompts.push(prompt.to_owned());
            self.answers.pop_front().flatten().map(Zeroizing::new)
        }
        fn error(&mut self, text: &str) {
            self.errors.push(text.to_owned());
        }
        fn info(&mut self, _text: &str) {}
    }

    /// A helper stand-in: reads the cookie, prompts, reads the password and
    /// answers. The password it accepts is `correct horse`.
    fn fake_helper(dir: &Path, body: &str) -> PamHelper {
        let path = dir.join("polkit-agent-helper-1");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        PamHelper::new(path)
    }

    const CHECKING_HELPER: &str = r#"
read cookie
[ -n "$cookie" ] || exit 2
printf 'PAM_PROMPT_ECHO_OFF Password: \n'
read pw
if [ "$pw" = "correct horse" ]; then echo SUCCESS; exit 0; fi
echo "PAM_ERROR_MSG Sorry, try again."
echo FAILURE
exit 1
"#;

    #[test]
    fn parses_every_protocol_line_and_ignores_unknown_ones() {
        assert_eq!(
            parse_line("PAM_PROMPT_ECHO_OFF Password: "),
            HelperLine::Prompt("Password: ".into())
        );
        assert_eq!(
            parse_line("PAM_PROMPT_ECHO_ON Username:\r\n"),
            HelperLine::Prompt("Username:".into())
        );
        assert_eq!(
            parse_line("PAM_ERROR_MSG Sorry, try again."),
            HelperLine::Error("Sorry, try again.".into())
        );
        assert_eq!(
            parse_line("PAM_TEXT_INFO hi"),
            HelperLine::Info("hi".into())
        );
        assert_eq!(parse_line("SUCCESS"), HelperLine::Success);
        assert_eq!(parse_line("FAILURE"), HelperLine::Failure);
        assert_eq!(parse_line("SOMETHING_NEW 1 2"), HelperLine::Unknown);
        assert_eq!(parse_line(""), HelperLine::Unknown);
    }

    #[test]
    fn cancel_before_attach_refuses_to_start_the_helper() {
        let token = CancelToken::default();
        token.cancel();
        assert!(!token.attach(1));
    }

    #[test]
    fn correct_password_authenticates_through_the_helper() {
        let dir = tempfile::tempdir().unwrap();
        let helper = fake_helper(dir.path(), CHECKING_HELPER);
        let mut conversation = Script {
            answers: VecDeque::from([Some("correct horse".to_owned())]),
            ..Script::default()
        };
        let outcome = helper.authenticate(
            "alice",
            "cookie",
            &mut conversation,
            &CancelToken::default(),
        );
        assert_eq!(outcome, HelperOutcome::Authenticated);
        assert_eq!(conversation.prompts, ["Password: "]);
    }

    #[test]
    fn wrong_password_is_reported_as_wrong_password_with_pam_text() {
        let dir = tempfile::tempdir().unwrap();
        let helper = fake_helper(dir.path(), CHECKING_HELPER);
        let mut conversation = Script {
            answers: VecDeque::from([Some("guess".to_owned())]),
            ..Script::default()
        };
        let outcome = helper.authenticate(
            "alice",
            "cookie",
            &mut conversation,
            &CancelToken::default(),
        );
        assert_eq!(outcome, HelperOutcome::WrongPassword);
        assert_eq!(conversation.errors, ["Sorry, try again."]);
    }

    #[test]
    fn failure_before_any_password_is_a_generic_failure() {
        let dir = tempfile::tempdir().unwrap();
        let helper = fake_helper(dir.path(), "read cookie\necho FAILURE\nexit 1");
        let mut conversation = Script::default();
        let outcome = helper.authenticate(
            "alice",
            "cookie",
            &mut conversation,
            &CancelToken::default(),
        );
        assert!(matches!(outcome, HelperOutcome::Failed(_)), "{outcome:?}");
    }

    #[test]
    fn cancelling_at_the_prompt_stops_the_helper() {
        let dir = tempfile::tempdir().unwrap();
        let helper = fake_helper(dir.path(), CHECKING_HELPER);
        let mut conversation = Script {
            answers: VecDeque::from([None]),
            ..Script::default()
        };
        let outcome = helper.authenticate(
            "alice",
            "cookie",
            &mut conversation,
            &CancelToken::default(),
        );
        assert_eq!(outcome, HelperOutcome::Cancelled);
    }

    #[test]
    fn cancel_from_another_thread_kills_a_helper_that_is_still_running() {
        let dir = tempfile::tempdir().unwrap();
        // After the answer the helper blocks; only a kill ends it.
        let helper = fake_helper(
            dir.path(),
            "read cookie\nprintf 'PAM_PROMPT_ECHO_OFF Password: \\n'\nread pw\nexec sleep 30",
        );
        let token = std::sync::Arc::new(CancelToken::default());
        let canceller = {
            let token = std::sync::Arc::clone(&token);
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(300));
                token.cancel();
            })
        };
        let mut conversation = Script {
            answers: VecDeque::from([Some("anything".to_owned())]),
            ..Script::default()
        };
        let started = Instant::now();
        let outcome = helper.authenticate("alice", "cookie", &mut conversation, &token);
        canceller.join().unwrap();
        assert_eq!(outcome, HelperOutcome::Cancelled);
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
    }

    #[test]
    fn missing_helper_is_unavailable_not_a_panic() {
        let helper = PamHelper::new("/nonexistent/polkit-agent-helper-1");
        let mut conversation = Script::default();
        let outcome = helper.authenticate(
            "alice",
            "cookie",
            &mut conversation,
            &CancelToken::default(),
        );
        assert!(matches!(outcome, HelperOutcome::Unavailable(_)));
    }

    #[test]
    fn root_has_a_username() {
        assert_eq!(username_for_uid(0).as_deref(), Some("root"));
        assert_eq!(username_for_uid(u32::MAX - 1), None);
    }
}
