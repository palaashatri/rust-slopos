//! Password checks for the lock screen.
//!
//! The check is behind [`PasswordCheck`] so tests use a fake. [`PamCheck`] is
//! the real one: it asks PAM, service `slopos-lock`, whether the password is
//! right for the user. libpam is loaded at run time with `dlopen`, so the
//! binary builds and starts on machines without the PAM development files.
//! Without PAM, [`NoAuthentication`] refuses every password: the screen stays
//! locked and says so.
//!
//! The password is never logged. The caller keeps it in a `Zeroizing` buffer;
//! the PAM conversation copies it into memory that libpam frees after use.

use std::ffi::{c_char, c_int, c_void, CString};
use std::ptr;
use std::sync::mpsc::Receiver;

use zeroize::{Zeroize, Zeroizing};

use super::state::Verdict;

/// The PAM service the lock uses; `/etc/pam.d/slopos-lock` must exist.
pub const PAM_SERVICE: &str = "slopos-lock";

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
    requests: Receiver<CheckRequest>,
    mut on_verdict: impl FnMut(Verdict),
) {
    for request in requests {
        let verdict = checker.check(&request.user, &request.password);
        // `request` (and its password) is dropped, and zeroed, here.
        drop(request);
        on_verdict(verdict);
    }
}

/// Refuses every password. Used when PAM cannot be loaded.
#[derive(Debug, Default)]
pub struct NoAuthentication;

impl PasswordCheck for NoAuthentication {
    fn check(&mut self, _user: &str, _password: &str) -> Verdict {
        Verdict::Unavailable
    }
}

// Constants from <security/_pam_types.h> and <security/pam_appl.h>.
const PAM_SUCCESS: c_int = 0;
const PAM_BUF_ERR: c_int = 5;
const PAM_AUTH_ERR: c_int = 7;
const PAM_USER_UNKNOWN: c_int = 10;
const PAM_MAXTRIES: c_int = 11;
const PAM_ACCT_EXPIRED: c_int = 13;
const PAM_CONV_ERR: c_int = 19;
const PAM_PROMPT_ECHO_OFF: c_int = 1;
const PAM_PROMPT_ECHO_ON: c_int = 2;
const PAM_ERROR_MSG: c_int = 3;
const PAM_TEXT_INFO: c_int = 4;
const PAM_MAX_NUM_MSG: c_int = 32;

#[repr(C)]
pub struct PamMessage {
    msg_style: c_int,
    msg: *const c_char,
}

#[repr(C)]
pub struct PamResponse {
    resp: *mut c_char,
    resp_retcode: c_int,
}

type ConvFn = unsafe extern "C" fn(
    c_int,
    *mut *const PamMessage,
    *mut *mut PamResponse,
    *mut c_void,
) -> c_int;

#[repr(C)]
pub struct PamConv {
    conv: Option<ConvFn>,
    appdata_ptr: *mut c_void,
}

type StartFn =
    unsafe extern "C" fn(*const c_char, *const c_char, *const PamConv, *mut *mut c_void) -> c_int;
type StepFn = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;
type EndFn = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;

/// Answers PAM's prompts. The password is the `appdata` (a `Vec<u8>`), and
/// each answer is a `calloc`ed copy, because libpam frees the answers itself.
///
/// # Safety
/// Called by libpam with the arrays it documents: `count` messages, and
/// `appdata` pointing at the live `Vec<u8>` of [`PamCheck::check`].
unsafe extern "C" fn conversation(
    count: c_int,
    messages: *mut *const PamMessage,
    responses: *mut *mut PamResponse,
    appdata: *mut c_void,
) -> c_int {
    if count <= 0
        || count > PAM_MAX_NUM_MSG
        || messages.is_null()
        || responses.is_null()
        || appdata.is_null()
    {
        return PAM_CONV_ERR;
    }
    let secret = &*(appdata as *const Vec<u8>);
    let count_usize = count as usize;
    let array = libc::calloc(count_usize, std::mem::size_of::<PamResponse>()) as *mut PamResponse;
    if array.is_null() {
        return PAM_BUF_ERR;
    }
    for index in 0..count_usize {
        let message = *messages.add(index);
        if message.is_null() {
            free_responses(array, index);
            return PAM_CONV_ERR;
        }
        let answer = &mut *array.add(index);
        match (*message).msg_style {
            PAM_PROMPT_ECHO_OFF => {
                let copy = libc::calloc(secret.len() + 1, 1) as *mut c_char;
                if copy.is_null() {
                    free_responses(array, index);
                    return PAM_BUF_ERR;
                }
                ptr::copy_nonoverlapping(secret.as_ptr(), copy as *mut u8, secret.len());
                answer.resp = copy;
            }
            PAM_PROMPT_ECHO_ON => {
                // The lock has no text to offer for a visible prompt.
                answer.resp = libc::calloc(1, 1) as *mut c_char;
                if answer.resp.is_null() {
                    free_responses(array, index);
                    return PAM_BUF_ERR;
                }
            }
            PAM_ERROR_MSG | PAM_TEXT_INFO => {}
            _ => {
                free_responses(array, index);
                return PAM_CONV_ERR;
            }
        }
    }
    *responses = array;
    PAM_SUCCESS
}

/// Free the first `filled` answers and the array.
///
/// # Safety
/// `array` is a `calloc`ed array of at least `filled` entries from
/// [`conversation`].
unsafe fn free_responses(array: *mut PamResponse, filled: usize) {
    for index in 0..filled {
        let answer = &*array.add(index);
        if !answer.resp.is_null() {
            libc::free(answer.resp as *mut c_void);
        }
    }
    libc::free(array as *mut c_void);
}

/// Map a PAM status to a verdict. Wrong credentials and an account that may
/// not log in are rejections; anything else means the check could not run.
pub fn verdict_for_status(status: c_int) -> Verdict {
    match status {
        PAM_SUCCESS => Verdict::Accepted,
        PAM_AUTH_ERR | PAM_USER_UNKNOWN | PAM_MAXTRIES | PAM_ACCT_EXPIRED => Verdict::Rejected,
        _ => Verdict::Unavailable,
    }
}

/// libpam, loaded at run time.
pub struct PamCheck {
    start: StartFn,
    authenticate: StepFn,
    acct_mgmt: StepFn,
    end: EndFn,
}

// SAFETY: the struct holds only function pointers into a library that stays
// loaded for the life of the process; the worker thread is its only user.
unsafe impl Send for PamCheck {}

impl PamCheck {
    /// Load `libpam.so.0` and its entry points.
    pub fn open() -> Result<Self, String> {
        // SAFETY: plain dlopen/dlsym calls with NUL-terminated names; the
        // symbol types match the PAM API they name.
        unsafe {
            let library = libc::dlopen(c"libpam.so.0".as_ptr(), libc::RTLD_NOW);
            if library.is_null() {
                return Err("libpam.so.0 cannot be loaded".to_owned());
            }
            Ok(Self {
                start: symbol(library, c"pam_start")?,
                authenticate: symbol(library, c"pam_authenticate")?,
                acct_mgmt: symbol(library, c"pam_acct_mgmt")?,
                end: symbol(library, c"pam_end")?,
            })
        }
    }
}

/// # Safety
/// `library` must be a handle from `dlopen`; `F` must be the function type
/// the named symbol has.
unsafe fn symbol<F: Copy>(library: *mut c_void, name: &std::ffi::CStr) -> Result<F, String> {
    let address = libc::dlsym(library, name.as_ptr());
    if address.is_null() {
        return Err(format!("libpam has no {}", name.to_string_lossy()));
    }
    Ok(std::mem::transmute_copy::<*mut c_void, F>(&address))
}

impl PasswordCheck for PamCheck {
    fn check(&mut self, user: &str, password: &str) -> Verdict {
        let (Ok(user), Ok(service)) = (CString::new(user), CString::new(PAM_SERVICE)) else {
            return Verdict::Unavailable;
        };
        let mut secret: Vec<u8> = password.as_bytes().to_vec();
        let conv = PamConv {
            conv: Some(conversation),
            appdata_ptr: &mut secret as *mut Vec<u8> as *mut c_void,
        };
        let mut handle: *mut c_void = ptr::null_mut();
        // SAFETY: every pointer passed is valid for the call; `handle` is
        // ended exactly when `pam_start` succeeded.
        let status = unsafe {
            let mut status = (self.start)(service.as_ptr(), user.as_ptr(), &conv, &mut handle);
            if status == PAM_SUCCESS {
                status = (self.authenticate)(handle, 0);
                if status == PAM_SUCCESS {
                    status = (self.acct_mgmt)(handle, 0);
                }
                (self.end)(handle, status);
            }
            status
        };
        secret.zeroize();
        let verdict = verdict_for_status(status);
        if verdict == Verdict::Unavailable {
            log::warn!("PAM check did not complete (status {status})");
        }
        verdict
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::mpsc;

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
    fn pam_statuses_map_to_verdicts() {
        assert_eq!(verdict_for_status(PAM_SUCCESS), Verdict::Accepted);
        assert_eq!(verdict_for_status(PAM_AUTH_ERR), Verdict::Rejected);
        assert_eq!(verdict_for_status(PAM_USER_UNKNOWN), Verdict::Rejected);
        assert_eq!(verdict_for_status(PAM_MAXTRIES), Verdict::Rejected);
        assert_eq!(verdict_for_status(PAM_ACCT_EXPIRED), Verdict::Rejected);
        // A missing service or a broken stack must not unlock or count as a
        // wrong password.
        assert_eq!(verdict_for_status(3), Verdict::Unavailable);
        assert_eq!(verdict_for_status(9), Verdict::Unavailable);
    }

    #[test]
    fn without_authentication_every_password_is_unavailable() {
        assert_eq!(
            NoAuthentication.check("alice", "anything"),
            Verdict::Unavailable
        );
    }

    /// Drive the conversation callback the way libpam does and check that
    /// the echo-off prompt gets a copy of the password, in memory libpam
    /// may free.
    #[test]
    fn conversation_answers_the_echo_off_prompt_with_the_password() {
        let prompt = PamMessage {
            msg_style: PAM_PROMPT_ECHO_OFF,
            msg: c"Password: ".as_ptr(),
        };
        let mut message_list: [*const PamMessage; 1] = [&prompt];
        let mut responses: *mut PamResponse = ptr::null_mut();
        let mut secret: Vec<u8> = b"s3cret".to_vec();
        // SAFETY: one message, `secret` lives across the call, and the
        // answers are freed below exactly as libpam would free them.
        unsafe {
            let status = conversation(
                1,
                message_list.as_mut_ptr(),
                &mut responses,
                &mut secret as *mut Vec<u8> as *mut c_void,
            );
            assert_eq!(status, PAM_SUCCESS);
            let answer = &*responses;
            assert_eq!(std::ffi::CStr::from_ptr(answer.resp).to_bytes(), b"s3cret");
            free_responses(responses, 1);
        }
    }

    #[test]
    fn conversation_refuses_unknown_prompt_styles_and_bad_counts() {
        let prompt = PamMessage {
            msg_style: 99,
            msg: c"?".as_ptr(),
        };
        let mut message_list: [*const PamMessage; 1] = [&prompt];
        let mut responses: *mut PamResponse = ptr::null_mut();
        let mut secret: Vec<u8> = b"x".to_vec();
        let appdata = &mut secret as *mut Vec<u8> as *mut c_void;
        // SAFETY: the arguments are valid; a refused conversation returns
        // without writing `responses`.
        unsafe {
            assert_eq!(
                conversation(1, message_list.as_mut_ptr(), &mut responses, appdata),
                PAM_CONV_ERR
            );
            assert!(responses.is_null());
            assert_eq!(
                conversation(0, message_list.as_mut_ptr(), &mut responses, appdata),
                PAM_CONV_ERR
            );
            assert_eq!(
                conversation(
                    1,
                    message_list.as_mut_ptr(),
                    &mut responses,
                    ptr::null_mut()
                ),
                PAM_CONV_ERR
            );
        }
    }
}
