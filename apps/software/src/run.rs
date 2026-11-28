//! Starting a tool for a search or an install, and stopping it on request.
//!
//! The child's stdout and stderr are read on two scoped threads that hand
//! each line to the caller as it arrives, so nothing polls. [`Cancel::cancel`]
//! kills the child; the readers then reach end of file and the run reports
//! [`CANCELLED`] instead of an exit status. A child that finished on its own,
//! or whose kill failed, reports its real exit status.

use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Mutex, MutexGuard, PoisonError};
use std::thread;

/// The error text for a run that was cancelled.
pub const CANCELLED: &str = "cancelled";

/// Stops one running child. The window keeps one per job and the worker
/// thread runs the job with it.
#[derive(Default)]
pub struct Cancel {
    cancelled: AtomicBool,
    /// Set once a kill was delivered to the child.
    killed: AtomicBool,
    child: Mutex<Option<Child>>,
}

impl Cancel {
    /// Mark the run cancelled and kill its child if it has started. A cancel
    /// that arrives before the spawn is honoured as soon as the child exists.
    /// An error means the child could not be killed: it may still be running,
    /// and the run reports its real outcome when it ends.
    pub fn cancel(&self) -> Result<(), String> {
        self.cancelled.store(true, Ordering::SeqCst);
        match self.slot().as_mut() {
            Some(child) => self.kill(child),
            None => Ok(()),
        }
    }

    fn kill(&self, child: &mut Child) -> Result<(), String> {
        child
            .kill()
            .map_err(|error| format!("Could not stop the running command: {error}"))?;
        self.killed.store(true, Ordering::SeqCst);
        Ok(())
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    fn slot(&self) -> MutexGuard<'_, Option<Child>> {
        self.child.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Which pipe a line came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Out,
    Err,
}

/// Run `argv` with `env` added to the environment. `argv[0]` is looked up on
/// `PATH`; the arguments are never passed through a shell. Stdin is closed.
/// Each output line goes to `on_line` as it arrives. Returns the exit status,
/// or [`CANCELLED`] when [`Cancel::cancel`] was called during the run.
pub fn run(
    argv: &[String],
    env: &[(&str, &str)],
    cancel: &Cancel,
    on_line: &mut dyn FnMut(Stream, &str),
) -> Result<ExitStatus, String> {
    let Some((program, args)) = argv.split_first() else {
        return Err("there is no command to run".into());
    };
    if cancel.is_cancelled() {
        return Err(CANCELLED.into());
    }
    let path = slopos_core::process::resolve_program(program)
        .ok_or_else(|| format!("{program} not found"))?;
    let mut child = Command::new(path)
        .args(args)
        .envs(env.iter().copied())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("{program}: {error}"))?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    {
        // Register the child under the lock and check the flag there, so a
        // cancel that raced the spawn still kills it. If that kill fails the
        // child runs to its own end, and its real status is returned below.
        let mut slot = cancel.slot();
        if cancel.is_cancelled() {
            // A failed stop is reported at once on the output stream. The run
            // keeps draining and waits for the child's real exit status.
            if let Err(error) = cancel.kill(&mut child) {
                on_line(Stream::Err, &error);
            }
        }
        *slot = Some(child);
    }

    let (sender, lines) = mpsc::channel::<(Stream, String)>();
    thread::scope(|scope| {
        if let Some(pipe) = stdout {
            let sender = sender.clone();
            scope.spawn(move || pump(pipe, Stream::Out, &sender));
        }
        if let Some(pipe) = stderr {
            let sender = sender.clone();
            scope.spawn(move || pump(pipe, Stream::Err, &sender));
        }
        drop(sender);
        // Ends once both pipes have reached end of file.
        for (stream, line) in lines {
            on_line(stream, &line);
        }
    });

    // Take the child out before waiting, so a cancel never blocks on it.
    let child = cancel.slot().take();
    let mut child = child.ok_or_else(|| format!("{program}: the process was not registered"))?;
    let status = child
        .wait()
        .map_err(|error| format!("{program}: {error}"))?;
    // Only a child that our kill ended is reported as cancelled. One that
    // finished on its own keeps its real status, even when a cancel came too
    // late to stop it.
    if cancel.killed.load(Ordering::SeqCst) && status.code().is_none() {
        return Err(CANCELLED.into());
    }
    Ok(status)
}

/// Send each line of `pipe` until end of file. Progress output ends its lines
/// with carriage returns, so only the last piece of each line is kept.
fn pump(pipe: impl Read, stream: Stream, sender: &mpsc::Sender<(Stream, String)>) {
    let mut reader = BufReader::new(pipe);
    let mut raw = Vec::new();
    loop {
        raw.clear();
        match reader.read_until(b'\n', &mut raw) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        let text = String::from_utf8_lossy(&raw);
        let line = text
            .trim_end_matches(['\r', '\n'])
            .rsplit('\r')
            .next()
            .unwrap_or("");
        if sender.send((stream, line.to_owned())).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| (*part).to_owned()).collect()
    }

    #[test]
    fn output_lines_are_passed_on_by_stream_with_the_exit_status() {
        let cancel = Cancel::default();
        let mut out = Vec::new();
        let mut err = Vec::new();
        let status = run(
            &argv(&[
                "sh",
                "-c",
                "echo one; echo two >&2; printf 'a\\rb\\n'; exit 3",
            ]),
            &[("SLOPOS_TEST", "1")],
            &cancel,
            &mut |stream, line| match stream {
                Stream::Out => out.push(line.to_owned()),
                Stream::Err => err.push(line.to_owned()),
            },
        )
        .unwrap();
        assert_eq!(status.code(), Some(3));
        assert_eq!(out, ["one", "b"], "a carriage return keeps the last piece");
        assert_eq!(err, ["two"]);
    }

    #[test]
    fn a_missing_program_is_named() {
        let cancel = Cancel::default();
        let error = run(
            &argv(&["slopos-no-such-tool-xyz"]),
            &[],
            &cancel,
            &mut |_, _| {},
        )
        .unwrap_err();
        assert_eq!(error, "slopos-no-such-tool-xyz not found");
        assert!(run(&[], &[], &cancel, &mut |_, _| {}).is_err());
    }

    #[test]
    fn cancelling_stops_the_child_promptly() {
        let cancel = std::sync::Arc::new(Cancel::default());
        let stopper = cancel.clone();
        let started = Instant::now();
        let canceller = thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            stopper.cancel().unwrap();
        });
        let result = run(&argv(&["sleep", "30"]), &[], &cancel, &mut |_, _| {});
        canceller.join().unwrap();
        assert_eq!(result, Err(CANCELLED.to_owned()));
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the child was not killed"
        );
    }

    #[test]
    fn a_child_that_dies_on_its_own_is_not_reported_as_cancelled() {
        let cancel = Cancel::default();
        let status = run(
            &argv(&["sh", "-c", "kill -KILL $$"]),
            &[],
            &cancel,
            &mut |_, _| {},
        )
        .unwrap();
        assert_eq!(status.code(), None, "it was stopped by a signal");
        assert!(cancel.cancel().is_ok(), "an idle cancel is fine");
    }

    #[test]
    fn a_cancel_before_the_run_starts_nothing() {
        let cancel = Cancel::default();
        cancel.cancel().unwrap();
        assert_eq!(
            run(&argv(&["sleep", "30"]), &[], &cancel, &mut |_, _| {}),
            Err(CANCELLED.to_owned())
        );
    }
}
