//! `slopos-lock`: lock the screen now, or run the watcher that locks it on
//! idle, before suspend and on logind requests.
//!
//! - `slopos-lock`: cover the screen and wait for the password (or logind's
//!   Unlock). Exits when unlocked.
//! - `slopos-lock --watch`: started by the session; runs until the session
//!   ends.
//! - `slopos-lock --ready-fd N`: internal, used by the watcher.

use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use slopos_session::lock::{self as lock, idle, screen, watch};

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args: Vec<String> = env::args().skip(1).collect();
    let result = match args.as_slice() {
        [] => lock_now(None),
        [flag, fd] if flag == "--ready-fd" => match fd.parse::<i32>() {
            Ok(fd) => lock_now(Some(fd)),
            Err(_) => return usage(2),
        },
        [flag] if flag == "--watch" => watch::run(watch_options()).map_err(LockNowError::Failed),
        [flag] if flag == "--version" => {
            println!("slopos-lock {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        [flag] if flag == "--help" || flag == "-h" => return usage(0),
        _ => return usage(2),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(LockNowError::NotConfigured) => ExitCode::from(lock::EXIT_NOT_CONFIGURED),
        Err(LockNowError::Failed(error)) => {
            eprintln!("slopos-lock: {error}");
            ExitCode::FAILURE
        }
    }
}

enum LockNowError {
    NotConfigured,
    Failed(String),
}

/// Lock the screen, unless the password service is missing: then nothing
/// could unlock it, so the screen is left alone and the reason is said.
fn lock_now(ready_fd: Option<i32>) -> Result<(), LockNowError> {
    if !lock::password_service_installed(Path::new(lock::PAM_SERVICE_FILE)) {
        let message = format!(
            "not locking: {} is not installed, so no password could unlock the screen",
            lock::PAM_SERVICE_FILE
        );
        log::error!("{message}");
        eprintln!("slopos-lock: {message}");
        return Err(LockNowError::NotConfigured);
    }
    screen::run(ready_fd).map_err(LockNowError::Failed)
}

fn usage(code: u8) -> ExitCode {
    let text = "Usage: slopos-lock [--watch | --version]\n\n\
                Without arguments, lock the screen now.\n\
                --watch     lock on idle, before suspend and on logind requests\n\
                            (the session starts this)\n";
    if code == 0 {
        print!("{text}");
    } else {
        eprint!("{text}");
    }
    ExitCode::from(code)
}

/// The idle timeout comes from `slopos-i/lock-idle-seconds` in the user's
/// configuration directory: whole seconds, `0` to turn it off.
fn watch_options() -> watch::WatchOptions {
    let timeout = match idle_setting_file() {
        Some(path) => match std::fs::read_to_string(&path) {
            Ok(text) => match idle::parse_idle_seconds(&text) {
                Some(setting) => setting,
                None => {
                    log::warn!(
                        "{} is not a number of seconds; using the default",
                        path.display()
                    );
                    Some(idle::DEFAULT_IDLE_TIMEOUT)
                }
            },
            Err(_) => Some(idle::DEFAULT_IDLE_TIMEOUT),
        },
        None => Some(idle::DEFAULT_IDLE_TIMEOUT),
    };
    let locker = env::current_exe().unwrap_or_else(|_| PathBuf::from("slopos-lock"));
    watch::WatchOptions {
        idle_timeout: timeout.filter(|timeout: &Duration| !timeout.is_zero()),
        locker,
    }
}

fn idle_setting_file() -> Option<PathBuf> {
    let config = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(config.join("slopos-i").join("lock-idle-seconds"))
}
