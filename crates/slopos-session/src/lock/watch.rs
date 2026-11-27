//! `slopos-lock --watch`: the long-running part of the lock. It starts the
//! locker when the user has been idle for the timeout, when logind asks for a
//! lock, or before the system sleeps.
//!
//! Nothing polls. The loop blocks on one channel with a deadline: the next
//! idle check, the end of a quiet period, or a message from a signal thread,
//! a locker's exit or a locker's readiness.

use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use x11rb::connection::Connection;
use x11rb::protocol::screensaver::{self, ConnectionExt as _};
use x11rb::rust_connection::RustConnection;

use super::idle::{next_idle_action, IdleAction};
use super::logind::{self, EventSink, LogindEvent};

/// How long a locker has to show its window before sleep goes ahead.
pub const READY_TIMEOUT: Duration = Duration::from_secs(5);
/// How long to wait before trying to start a locker again after a failure.
const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(10);
/// How often to re-read the idle time when the X server could not say.
const IDLE_UNKNOWN_RECHECK: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
pub struct WatchOptions {
    /// Idle time before the automatic lock; `None` turns it off.
    pub idle_timeout: Option<Duration>,
    /// The locker to start (`slopos-lock`).
    pub locker: PathBuf,
}

/// Something the watcher loop reacts to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {
    Logind(LogindEvent),
    /// The locker process ended. `clean` is a normal exit (an unlock);
    /// `lived` is how long it ran.
    LockerExited {
        clean: bool,
        lived: Duration,
    },
    /// The locker shows its window, or it ended, or it did not answer in time.
    LockerReady,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Lock,
    Nothing,
}

/// What a logind signal asks for, given whether a locker already runs.
pub fn decide(event: LogindEvent, locker_running: bool) -> Decision {
    match event {
        LogindEvent::Lock | LogindEvent::PrepareForSleep(true) if !locker_running => Decision::Lock,
        _ => Decision::Nothing,
    }
}

/// The time since the last user input, from the X screensaver extension.
pub struct IdleSource {
    conn: RustConnection,
    root: u32,
}

impl IdleSource {
    pub fn connect() -> Result<Self, String> {
        let (conn, screen_number) =
            x11rb::connect(None).map_err(|error| format!("no X server: {error}"))?;
        let root = conn.setup().roots[screen_number].root;
        conn.screensaver_query_version(1, 1)
            .map_err(|error| error.to_string())?
            .reply()
            .map_err(|error| format!("screensaver extension: {error}"))?;
        Ok(Self { conn, root })
    }

    pub fn idle_for(&self) -> Result<Duration, String> {
        let reply = screensaver::query_info(&self.conn, self.root)
            .map_err(|error| error.to_string())?
            .reply()
            .map_err(|error| error.to_string())?;
        Ok(Duration::from_millis(u64::from(reply.ms_since_user_input)))
    }
}

/// Run until the channel machinery fails. Returns an error only when the
/// watcher cannot run at all.
pub fn run(options: WatchOptions) -> Result<(), String> {
    let (sender, receiver) = mpsc::channel::<Message>();
    let bus = match zbus::blocking::Connection::system() {
        Ok(bus) => Some(bus),
        Err(error) => {
            log::warn!("no system bus; logind requests are not watched: {error}");
            None
        }
    };
    let mut inhibitor = None;
    if let Some(bus) = &bus {
        subscribe(bus, &sender);
        inhibitor = take_inhibitor(bus);
    }
    let idle = match (options.idle_timeout, IdleSource::connect()) {
        (None, _) => None,
        (Some(_), Ok(source)) => Some(source),
        (Some(_), Err(error)) => {
            log::warn!("automatic lock disabled: {error}");
            None
        }
    };
    let mut watcher = Watcher {
        locker: options.locker,
        idle_timeout: options.idle_timeout,
        locker_running: false,
        must_lock: false,
        quiet_until: None,
        sender,
    };
    loop {
        let wait = if watcher.locker_running {
            None
        } else {
            watcher.idle_wait(idle.as_ref())
        };
        let message = match wait {
            None => receiver.recv().ok(),
            Some(wait) => match receiver.recv_timeout(wait) {
                Ok(message) => Some(message),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return Err("watcher channel closed".into()),
            },
        };
        let Some(message) = message else {
            continue;
        };
        match message {
            Message::Logind(event) => {
                if let LogindEvent::PrepareForSleep(false) = event {
                    // Awake again: take the delay inhibitor for the next sleep.
                    if inhibitor.is_none() {
                        inhibitor = bus.as_ref().and_then(take_inhibitor);
                    }
                }
                if decide(event, watcher.locker_running) == Decision::Lock {
                    log::info!("locking the screen for {event:?}");
                    watcher.start_locker();
                }
            }
            Message::LockerExited { clean, lived } => watcher.locker_ended(clean, lived),
            Message::LockerReady => {
                // The screen is covered: suspend may go ahead.
                inhibitor = None;
            }
        }
    }
}

struct Watcher {
    locker: PathBuf,
    idle_timeout: Option<Duration>,
    locker_running: bool,
    /// The session is covered by a lock that has not been unlocked, but no
    /// locker runs: the last one died. Set until a clean unlock.
    must_lock: bool,
    /// No idle lock before this time (after an unlock or a failed start).
    quiet_until: Option<Instant>,
    sender: Sender<Message>,
}

impl Watcher {
    /// The locker ended. A clean end is an unlock. Any other end would leave
    /// the session uncovered, so the lock comes back: at once when the locker
    /// ran for a while, otherwise after a pause, so that a locker which dies
    /// on start is not respawned in a tight loop.
    fn locker_ended(&mut self, clean: bool, lived: Duration) {
        self.locker_running = false;
        if clean {
            self.must_lock = false;
            // An unlock without input (loginctl unlock-session) must not lock
            // again at once: the idle time is still past the timeout.
            self.quiet_until = self.idle_timeout.map(|t| Instant::now() + t);
            return;
        }
        log::error!("the screen locker ended without an unlock; locking again");
        self.must_lock = true;
        if lived >= READY_TIMEOUT {
            self.quiet_until = None;
            self.start_locker();
        } else {
            self.quiet_until = Some(Instant::now() + RETRY_AFTER_FAILURE);
        }
    }

    /// How long the loop may block before it must look at the idle time
    /// again; `None` when the idle lock is off.
    fn idle_wait(&mut self, idle: Option<&IdleSource>) -> Option<Duration> {
        let now = Instant::now();
        if let Some(until) = self.quiet_until {
            if now < until {
                return Some(until - now);
            }
            self.quiet_until = None;
        }
        if self.must_lock {
            // Do not wait for input: the session must not stay open.
            self.start_locker();
            return if self.locker_running {
                None
            } else {
                Some(RETRY_AFTER_FAILURE)
            };
        }
        let (Some(source), Some(timeout)) = (idle, self.idle_timeout) else {
            return None;
        };
        match source.idle_for() {
            Ok(idle_for) => match next_idle_action(idle_for, timeout) {
                IdleAction::Lock => {
                    log::info!("idle for {}s; locking the screen", idle_for.as_secs());
                    self.start_locker();
                    // Running: block until the locker ends. Failed: retry
                    // after the pause that start_locker set.
                    if self.locker_running {
                        None
                    } else {
                        Some(RETRY_AFTER_FAILURE)
                    }
                }
                IdleAction::Wait(wait) => Some(wait),
            },
            Err(error) => {
                log::warn!("cannot read the idle time: {error}");
                Some(IDLE_UNKNOWN_RECHECK)
            }
        }
    }

    fn start_locker(&mut self) {
        match spawn_locker(&self.locker, self.sender.clone()) {
            Ok(()) => self.locker_running = true,
            Err(error) => {
                log::error!("cannot start the screen locker: {error}");
                self.quiet_until = Some(Instant::now() + RETRY_AFTER_FAILURE);
            }
        }
    }
}

fn subscribe(bus: &zbus::blocking::Connection, sender: &Sender<Message>) {
    let to_loop = sender.clone();
    let sink: EventSink = Arc::new(move |event| {
        let _ = to_loop.send(Message::Logind(event));
    });
    if let Err(error) = logind::watch_manager(bus, sink.clone()) {
        log::warn!("cannot watch PrepareForSleep: {error}");
    }
    match logind::session_path(bus) {
        Ok(path) => {
            if let Err(error) = logind::watch_session(bus, path.as_str(), sink) {
                log::warn!("cannot watch the session's Lock signal: {error}");
            }
        }
        Err(error) => log::warn!("no logind session for this process: {error}"),
    }
}

fn take_inhibitor(bus: &zbus::blocking::Connection) -> Option<OwnedFd> {
    match logind::inhibit_sleep(bus) {
        Ok(fd) => Some(fd),
        Err(error) => {
            log::warn!("cannot delay sleep for the lock: {error}");
            None
        }
    }
}

/// Start `locker` with a ready pipe. Its write end is inherited as
/// `--ready-fd N`; the locker writes a byte once it covers the screen, and
/// the pipe reaching end-of-file means it is gone. Two threads report back
/// through `sender`: one when the process ends, one when it is ready or has
/// had [`READY_TIMEOUT`] to become ready.
fn spawn_locker(locker: &Path, sender: Sender<Message>) -> Result<(), String> {
    let (read, write) = pipe()?;
    let started = Instant::now();
    let child = Command::new(locker)
        .arg("--ready-fd")
        .arg(write.as_raw_fd().to_string())
        .spawn();
    // The parent keeps no write end: EOF on the read end means the locker ended.
    drop(write);
    let mut child = child.map_err(|error| format!("{}: {error}", locker.display()))?;

    let exit_sender = sender.clone();
    thread::Builder::new()
        .name("slopos-locker-wait".into())
        .spawn(move || {
            let clean = match child.wait() {
                Ok(status) => {
                    log::info!("screen locker ended ({status})");
                    // A refused start never covered the screen, so it is not
                    // an unlock to undo, and it must not be retried in a loop.
                    status.success() || status.code() == Some(i32::from(super::EXIT_NOT_CONFIGURED))
                }
                Err(error) => {
                    log::error!("waiting for the screen locker failed: {error}");
                    false
                }
            };
            let lived = started.elapsed();
            let _ = exit_sender.send(Message::LockerExited { clean, lived });
        })
        .map_err(|error| error.to_string())?;

    thread::Builder::new()
        .name("slopos-locker-ready".into())
        .spawn(move || {
            wait_readable(read.as_raw_fd(), READY_TIMEOUT);
            drop(read);
            let _ = sender.send(Message::LockerReady);
        })
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// Block until `fd` is readable, hung up, or `timeout` has passed.
fn wait_readable(fd: i32, timeout: Duration) {
    let mut poll = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let millis = libc::c_int::try_from(timeout.as_millis()).unwrap_or(libc::c_int::MAX);
    // SAFETY: one valid pollfd for the duration of the call.
    unsafe { libc::poll(&mut poll, 1, millis) };
}

/// A pipe whose read end is close-on-exec and whose write end is inherited.
fn pipe() -> Result<(OwnedFd, OwnedFd), String> {
    let mut fds = [0 as libc::c_int; 2];
    // SAFETY: pipe2 writes two descriptors into `fds`.
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    // SAFETY: both descriptors were just created and are owned here.
    let (read, write) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
    // SAFETY: clear FD_CLOEXEC on the write end we own, so the locker gets it.
    if unsafe { libc::fcntl(write.as_raw_fd(), libc::F_SETFD, 0) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok((read, write))
}

/// Tell the watcher the locker covers the screen. `fd` is the inherited
/// write end; it is closed afterwards.
pub fn signal_ready(fd: i32) {
    // SAFETY: `fd` came from `--ready-fd`; writing one byte and closing it
    // hands it back to the watcher's pipe.
    unsafe {
        libc::write(fd, b"1".as_ptr() as *const libc::c_void, 1);
        libc::close(fd);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_requests_start_a_locker_only_when_none_runs() {
        assert_eq!(decide(LogindEvent::Lock, false), Decision::Lock);
        assert_eq!(decide(LogindEvent::Lock, true), Decision::Nothing);
        assert_eq!(
            decide(LogindEvent::PrepareForSleep(true), false),
            Decision::Lock
        );
        assert_eq!(
            decide(LogindEvent::PrepareForSleep(true), true),
            Decision::Nothing
        );
    }

    #[test]
    fn waking_up_and_unlocking_never_start_a_locker() {
        assert_eq!(
            decide(LogindEvent::PrepareForSleep(false), false),
            Decision::Nothing
        );
        assert_eq!(decide(LogindEvent::Unlock, false), Decision::Nothing);
    }

    #[test]
    fn a_quiet_period_delays_the_idle_check() {
        let (sender, _receiver) = mpsc::channel();
        let mut watcher = Watcher {
            locker: PathBuf::from("/nonexistent/slopos-lock"),
            idle_timeout: Some(Duration::from_secs(300)),
            locker_running: false,
            must_lock: false,
            quiet_until: Some(Instant::now() + Duration::from_secs(120)),
            sender,
        };
        let wait = watcher.idle_wait(None).expect("waits out the quiet period");
        assert!(wait > Duration::from_secs(100) && wait <= Duration::from_secs(120));
    }

    #[test]
    fn with_no_idle_source_the_loop_blocks_on_messages() {
        let (sender, _receiver) = mpsc::channel();
        let mut watcher = Watcher {
            locker: PathBuf::from("/nonexistent/slopos-lock"),
            idle_timeout: Some(Duration::from_secs(300)),
            locker_running: false,
            must_lock: false,
            quiet_until: None,
            sender,
        };
        assert_eq!(watcher.idle_wait(None), None);
    }

    #[test]
    fn a_failed_locker_start_is_retried_after_a_pause_not_at_once() {
        let (sender, _receiver) = mpsc::channel();
        let mut watcher = Watcher {
            locker: PathBuf::from("/nonexistent/slopos-lock"),
            idle_timeout: Some(Duration::from_secs(300)),
            locker_running: false,
            must_lock: false,
            quiet_until: None,
            sender,
        };
        watcher.start_locker();
        assert!(!watcher.locker_running);
        let wait = watcher.idle_wait(None).expect("paused before retrying");
        assert!(wait > Duration::from_secs(8) && wait <= RETRY_AFTER_FAILURE);
    }

    #[test]
    fn the_ready_pipe_reports_ready_on_a_byte_and_on_hangup() {
        let (read, write) = pipe().unwrap();
        // SAFETY: the write end is ours; a byte is written and the fd closed.
        let raw = write.as_raw_fd();
        std::mem::forget(write);
        signal_ready(raw);
        wait_readable(read.as_raw_fd(), Duration::from_secs(5));

        let (read, write) = pipe().unwrap();
        drop(write);
        // A hangup (locker gone) also counts as readable.
        wait_readable(read.as_raw_fd(), Duration::from_secs(5));
    }

    fn watcher_with(locker: &str) -> (Watcher, mpsc::Receiver<Message>) {
        let (sender, receiver) = mpsc::channel();
        let watcher = Watcher {
            locker: PathBuf::from(locker),
            idle_timeout: Some(Duration::from_secs(300)),
            locker_running: false,
            must_lock: false,
            quiet_until: None,
            sender,
        };
        (watcher, receiver)
    }

    #[test]
    fn a_clean_unlock_ends_the_obligation_to_lock() {
        let (mut watcher, _receiver) = watcher_with("/nonexistent/slopos-lock");
        watcher.must_lock = true;
        watcher.locker_ended(true, Duration::from_secs(60));
        assert!(!watcher.must_lock);
        assert!(!watcher.locker_running);
        // As after any unlock, the idle check waits out the quiet period.
        let wait = watcher.idle_wait(None).expect("quiet period");
        assert!(wait > Duration::from_secs(290));
    }

    #[test]
    fn a_locker_that_dies_young_is_started_again_after_a_pause() {
        let (mut watcher, _receiver) = watcher_with("/nonexistent/slopos-lock");
        watcher.locker_running = true;
        watcher.locker_ended(false, Duration::from_millis(200));
        assert!(watcher.must_lock);
        assert!(!watcher.locker_running, "not respawned at once");
        let wait = watcher.idle_wait(None).expect("paused before retrying");
        assert!(wait > Duration::from_secs(8) && wait <= RETRY_AFTER_FAILURE);
    }

    #[test]
    fn a_locker_that_dies_after_a_while_is_started_again_at_once() {
        // /bin/true ignores its arguments and exits, so the start succeeds.
        let (mut watcher, receiver) = watcher_with("/bin/true");
        watcher.locker_running = true;
        watcher.locker_ended(false, READY_TIMEOUT);
        assert!(watcher.must_lock);
        assert!(watcher.locker_running, "the lock came back at once");
        // The new locker's own clean exit is reported like any other.
        let exited = std::iter::from_fn(|| receiver.recv_timeout(Duration::from_secs(10)).ok())
            .take(2)
            .any(|message| matches!(message, Message::LockerExited { clean: true, .. }));
        assert!(exited, "the replacement locker's exit was reported");
    }

    #[test]
    fn a_failed_relock_is_retried_after_a_pause() {
        let (mut watcher, _receiver) = watcher_with("/nonexistent/slopos-lock");
        watcher.locker_running = true;
        watcher.locker_ended(false, READY_TIMEOUT);
        assert!(watcher.must_lock);
        assert!(!watcher.locker_running);
        // The start failed; the session stays marked as locked and the
        // watcher waits before trying again rather than spinning.
        let wait = watcher.idle_wait(None).expect("paused before retrying");
        assert!(wait > Duration::from_secs(8) && wait <= RETRY_AFTER_FAILURE);
        assert!(watcher.must_lock);
    }
}
