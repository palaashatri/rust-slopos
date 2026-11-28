//! Password checks for the lock screen.
//!
//! The check is behind [`PasswordCheck`] so tests use a fake. [`PamCheck`] is
//! the real one: it asks PAM, service `slopos-lock`, whether the password is
//! right for the user. The PAM bindings are the `pam` crate (MIT OR
//! Apache-2.0), which links the system libpam at build time; the build needs
//! the libpam development files (`libpam0g-dev` on Debian and Ubuntu).
//!
//! The password is never logged. The caller keeps it in a `Zeroizing` buffer;
//! the conversation keeps its own zeroed copy until the check ends.

use std::ffi::{CStr, CString};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use pam::{Client, Conversation, PamError, PamReturnCode};
use zeroize::Zeroizing;

use super::state::Verdict;

/// The PAM service the lock uses; `/etc/pam.d/slopos-lock` must exist.
pub const PAM_SERVICE: &str = "slopos-lock";

/// How long one PAM check may take before the lock gives up on it. A stack
/// that blocks (for example a network directory) must not leave the lock in
/// "Checking". The lock stays locked and the attempt is not counted.
pub const PAM_TIMEOUT: Duration = Duration::from_secs(30);

pub trait PasswordCheck: Send {
    fn check(&mut self, user: &str, password: &str) -> Verdict;
}

/// A password typed for `user`, waiting to be checked.
pub struct CheckRequest {
    pub user: String,
    pub password: Zeroizing<String>,
}

/// Check each request in turn until the sender side goes away. `on_verdict`
/// runs on this thread after each check; the thread blocks on the channel
/// between requests, so it costs nothing while the screen is idle.
pub fn run_checks(
    mut checker: Box<dyn PasswordCheck>,
    requests: mpsc::Receiver<CheckRequest>,
    mut on_verdict: impl FnMut(Verdict),
) {
    for request in requests {
        let verdict = checker.check(&request.user, &request.password);
        // `request` (and its password) is dropped, and zeroed, here.
        drop(request);
        on_verdict(verdict);
    }
}

/// Refuses every password. Used when PAM cannot be used.
#[derive(Debug, Default)]
pub struct NoAuthentication;

impl PasswordCheck for NoAuthentication {
    fn check(&mut self, _user: &str, _password: &str) -> Verdict {
        Verdict::Unavailable
    }
}

/// Map a PAM result to a verdict. Wrong credentials and an account that may
/// not log in are rejections; anything else means the check could not run.
pub fn verdict_for_code(code: PamReturnCode) -> Verdict {
    match code {
        PamReturnCode::Success => Verdict::Accepted,
        PamReturnCode::Auth_Err
        | PamReturnCode::User_Unknown
        | PamReturnCode::MaxTries
        | PamReturnCode::Acct_Expired => Verdict::Rejected,
        _ => Verdict::Unavailable,
    }
}

/// Answers PAM's prompts with the user name and the password. Each answer is
/// a fresh copy that libpam frees; this struct zeroes its own copies on drop.
pub struct LoginConversation {
    user: Zeroizing<String>,
    password: Zeroizing<String>,
}

impl LoginConversation {
    pub fn new(user: &str, password: &str) -> Self {
        Self {
            user: Zeroizing::new(user.to_owned()),
            password: Zeroizing::new(password.to_owned()),
        }
    }
}

impl Conversation for LoginConversation {
    fn prompt_echo(&mut self, _msg: &CStr) -> Result<CString, ()> {
        CString::new(self.user.as_str()).map_err(|_| ())
    }

    fn prompt_blind(&mut self, _msg: &CStr) -> Result<CString, ()> {
        CString::new(self.password.as_str()).map_err(|_| ())
    }

    fn info(&mut self, msg: &CStr) {
        log::info!("pam: {}", msg.to_string_lossy());
    }

    fn error(&mut self, msg: &CStr) {
        log::warn!("pam: {}", msg.to_string_lossy());
    }
}

/// One PAM check on the calling thread: authenticate, then the account check.
fn authenticate_once(user: &str, password: &str) -> Verdict {
    let mut client =
        match Client::with_conversation(PAM_SERVICE, LoginConversation::new(user, password)) {
            Ok(client) => client,
            Err(PamError(code)) => {
                log::warn!("PAM check did not start (code {code:?})");
                return Verdict::Unavailable;
            }
        };
    match client.authenticate() {
        Ok(()) => Verdict::Accepted,
        Err(PamError(code)) => {
            let verdict = verdict_for_code(code);
            if verdict == Verdict::Unavailable {
                log::warn!("PAM check did not complete (code {code:?})");
            }
            verdict
        }
    }
}

/// One password check: a user and a password in, a verdict out. [`PamCheck`]
/// runs it on a helper thread under a deadline; tests pass a fake that
/// answers, blocks or panics.
pub type Authenticator = dyn Fn(&str, &str) -> Verdict + Send + Sync;

/// The real check: PAM, with a timeout.
///
/// The check runs on its own thread so that a stack that blocks cannot hold
/// the lock in "Checking". On timeout the thread is left to finish on its own
/// (libpam cannot be interrupted) and only its late answer is discarded; each
/// such thread holds only its own zeroed copies.
pub struct PamCheck {
    authenticate: Arc<Authenticator>,
    timeout: Duration,
}

impl std::fmt::Debug for PamCheck {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PamCheck")
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

impl Default for PamCheck {
    fn default() -> Self {
        Self::new()
    }
}

impl PamCheck {
    /// The production check: PAM, service [`PAM_SERVICE`], timeout [`PAM_TIMEOUT`].
    pub fn new() -> Self {
        Self::with_authenticator(Arc::new(authenticate_once), PAM_TIMEOUT)
    }

    /// A check that asks `authenticate` instead of PAM and gives up after `timeout`.
    pub fn with_authenticator(authenticate: Arc<Authenticator>, timeout: Duration) -> Self {
        Self {
            authenticate,
            timeout,
        }
    }
}

impl PasswordCheck for PamCheck {
    fn check(&mut self, user: &str, password: &str) -> Verdict {
        let user = user.to_owned();
        let password = Zeroizing::new(password.to_owned());
        let authenticate = Arc::clone(&self.authenticate);
        let (answer, received) = mpsc::channel();
        let spawned = thread::Builder::new()
            .name("slopos-pam".to_owned())
            .spawn(move || {
                let verdict = authenticate(user.as_str(), password.as_str());
                let _ = answer.send(verdict);
            });
        if let Err(error) = spawned {
            log::error!("cannot start the PAM check: {error}");
            return Verdict::Unavailable;
        }
        match received.recv_timeout(self.timeout) {
            Ok(verdict) => verdict,
            Err(RecvTimeoutError::Timeout) => {
                log::error!(
                    "PAM did not answer within {:?}; the screen stays locked",
                    self.timeout
                );
                Verdict::Unavailable
            }
            // The helper ended without answering (it panicked): no verdict.
            Err(RecvTimeoutError::Disconnected) => Verdict::Unavailable,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::lock::state::{Input, LockMachine, Phase};

    /// Accepts exactly one password for one user.
    pub struct FixedCheck {
        pub user: String,
        pub password: String,
        pub calls: usize,
    }

    impl PasswordCheck for FixedCheck {
        fn check(&mut self, user: &str, password: &str) -> Verdict {
            self.calls += 1;
            if user == self.user && password == self.password {
                Verdict::Accepted
            } else {
                Verdict::Rejected
            }
        }
    }

    fn fixed(password: &str) -> Box<dyn PasswordCheck> {
        Box::new(FixedCheck {
            user: "alice".into(),
            password: password.into(),
            calls: 0,
        })
    }

    #[test]
    fn fake_check_accepts_the_right_password_and_rejects_a_wrong_one() {
        let mut check = FixedCheck {
            user: "alice".into(),
            password: "correct horse".into(),
            calls: 0,
        };
        assert_eq!(check.check("alice", "correct horse"), Verdict::Accepted);
        assert_eq!(check.check("alice", "wrong"), Verdict::Rejected);
        assert_eq!(check.check("alice", ""), Verdict::Rejected);
        assert_eq!(check.check("bob", "correct horse"), Verdict::Rejected);
        assert_eq!(check.calls, 4);
    }

    #[test]
    fn worker_reports_one_verdict_per_request_then_stops() {
        let (request_sender, requests) = mpsc::channel();
        let (verdict_sender, verdicts) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            run_checks(fixed("open sesame"), requests, |verdict| {
                verdict_sender.send(verdict).unwrap();
            });
        });
        request_sender
            .send(CheckRequest {
                user: "alice".into(),
                password: Zeroizing::new("nope".into()),
            })
            .unwrap();
        request_sender
            .send(CheckRequest {
                user: "alice".into(),
                password: Zeroizing::new("open sesame".into()),
            })
            .unwrap();
        assert_eq!(verdicts.recv().unwrap(), Verdict::Rejected);
        assert_eq!(verdicts.recv().unwrap(), Verdict::Accepted);
        drop(request_sender);
        worker.join().unwrap();
    }

    #[test]
    fn pam_codes_map_to_verdicts() {
        assert_eq!(verdict_for_code(PamReturnCode::Success), Verdict::Accepted);
        assert_eq!(verdict_for_code(PamReturnCode::Auth_Err), Verdict::Rejected);
        assert_eq!(
            verdict_for_code(PamReturnCode::User_Unknown),
            Verdict::Rejected
        );
        assert_eq!(verdict_for_code(PamReturnCode::MaxTries), Verdict::Rejected);
        assert_eq!(
            verdict_for_code(PamReturnCode::Acct_Expired),
            Verdict::Rejected
        );
        // A missing service or a broken stack must not unlock or count as a
        // wrong password.
        assert_eq!(
            verdict_for_code(PamReturnCode::Service_Err),
            Verdict::Unavailable
        );
        assert_eq!(verdict_for_code(PamReturnCode::Abort), Verdict::Unavailable);
    }

    #[test]
    fn without_authentication_every_password_is_unavailable() {
        assert_eq!(
            NoAuthentication.check("alice", "anything"),
            Verdict::Unavailable
        );
    }

    #[test]
    fn the_conversation_answers_the_blind_prompt_with_the_password_and_the_echo_prompt_with_the_user(
    ) {
        let mut conversation = LoginConversation::new("alice", "s3cret");
        let prompt = c"Password: ";
        assert_eq!(
            conversation.prompt_blind(prompt).unwrap().as_bytes(),
            b"s3cret"
        );
        assert_eq!(
            conversation.prompt_echo(prompt).unwrap().as_bytes(),
            b"alice"
        );
        conversation.info(c"Welcome");
        conversation.error(c"Something failed");
    }

    #[test]
    fn a_password_with_a_nul_byte_is_refused_not_truncated() {
        let mut conversation = LoginConversation::new("alice", "bad\0cut");
        assert!(conversation.prompt_blind(c"Password: ").is_err());
    }

    #[test]
    fn a_pam_check_for_a_missing_user_is_never_accepted_and_returns_promptly() {
        let started = std::time::Instant::now();
        let verdict = PamCheck::new().check("slopos-no-such-user", "not the password");
        assert_ne!(verdict, Verdict::Accepted);
        assert!(started.elapsed() < PAM_TIMEOUT);
    }

    /// A fake authenticator that blocks, like a stuck PAM stack, until the
    /// returned sender is used or dropped. It then answers `verdict`.
    fn blocking_fake(verdict: Verdict) -> (Arc<Authenticator>, mpsc::Sender<()>) {
        let (release, gate) = mpsc::channel::<()>();
        let gate = std::sync::Mutex::new(gate);
        let fake: Arc<Authenticator> = Arc::new(move |_user: &str, _password: &str| {
            let _ = gate.lock().unwrap().recv();
            verdict
        });
        (fake, release)
    }

    #[test]
    fn the_fake_authenticator_gets_the_user_and_the_password_and_its_verdict_is_returned() {
        let fake: Arc<Authenticator> = Arc::new(|user: &str, password: &str| {
            if user == "alice" && password == "open sesame" {
                Verdict::Accepted
            } else {
                Verdict::Rejected
            }
        });
        let mut check = PamCheck::with_authenticator(fake, PAM_TIMEOUT);
        assert_eq!(check.check("alice", "open sesame"), Verdict::Accepted);
        assert_eq!(check.check("alice", "nope"), Verdict::Rejected);
        assert_eq!(check.check("bob", "open sesame"), Verdict::Rejected);
    }

    #[test]
    fn a_check_that_never_answers_is_unavailable_at_the_deadline_and_the_lock_stays_locked() {
        let (fake, release) = blocking_fake(Verdict::Accepted);
        let mut check = PamCheck::with_authenticator(fake, Duration::from_millis(50));
        let mut machine = LockMachine::new();
        machine.handle(Input::Lock, Duration::ZERO);
        machine.handle(Input::Submit, Duration::ZERO);

        let started = std::time::Instant::now();
        let verdict = check.check("alice", "anything");
        let waited = started.elapsed();
        // The helper is still blocked: let it finish so the test ends cleanly.
        let _ = release.send(());

        assert_eq!(verdict, Verdict::Unavailable);
        assert!(waited < PAM_TIMEOUT, "the deadline is the one passed in");
        machine.handle(Input::Checked(verdict), Duration::ZERO);
        assert_eq!(machine.phase(), Phase::Locked);
        assert_eq!(machine.failures(), 0, "a timeout is not a wrong password");
    }

    #[test]
    fn a_helper_that_panics_is_unavailable_not_accepted() {
        let fake: Arc<Authenticator> = Arc::new(|_: &str, _: &str| -> Verdict {
            panic!("the stack broke");
        });
        let mut check = PamCheck::with_authenticator(fake, PAM_TIMEOUT);
        assert_eq!(check.check("alice", "pw"), Verdict::Unavailable);
    }

    #[test]
    fn a_late_answer_after_the_deadline_is_not_used() {
        let (fake, release) = blocking_fake(Verdict::Accepted);
        let mut check = PamCheck::with_authenticator(fake, Duration::from_millis(50));
        assert_eq!(check.check("alice", "pw"), Verdict::Unavailable);
        // The stuck check now answers "Accepted". The lock already moved on:
        // the next check is a fresh one and does not see the old answer.
        let _ = release.send(());
        let (fake, second_release) = blocking_fake(Verdict::Rejected);
        check.authenticate = fake;
        let _ = second_release.send(());
        assert_eq!(check.check("alice", "pw"), Verdict::Rejected);
    }
}
