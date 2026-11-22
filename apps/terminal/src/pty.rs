//! A program on a pseudo-terminal. `forkpty` gives the child a terminal as
//! its controlling tty; this module keeps the master side.
//!
//! Four threads serve one program:
//! - a **waiter** blocks in `waitpid` and is the only code that reaps the child;
//! - a **reader** polls the master and the waiter's exit pipe, so it sees the
//!   exit even while a background process still holds the terminal open, and
//!   it drains the last output before it reports [`Output::Exited`];
//! - a **writer** sends queued input to the program, so a program that stops
//!   reading cannot freeze the window.
//!
//! Nothing polls on a timer: the reader sleeps in `poll` until the program
//! writes or exits. Output is queued in a bounded channel, so a fast program
//! waits for the window instead of filling memory, and no byte is dropped.
//!
//! The child runs no Rust code between `fork` and `exec`: the program, its
//! argument vector and its environment are prepared in the parent. A failed
//! `exec` is reported back through a close-on-exec pipe, so `spawn` returns the
//! error instead of a window that closes at once.

use std::ffi::{CString, OsStr};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::Arc;
use std::thread;

/// Chunks of program output the reader may hold before it waits for the window.
const OUTPUT_QUEUE: usize = 64;
/// Bytes of input that may be waiting for the program. A write past this is
/// refused, so a program that stops reading cannot grow the queue without limit.
pub const INPUT_BUDGET: usize = 1 << 20;

/// What the program printed, or that it has exited.
#[derive(Debug, PartialEq, Eq)]
pub enum Output {
    Data(Vec<u8>),
    Exited,
}

/// The master side of a running program.
pub struct Pty {
    master: OwnedFd,
    /// A descriptor for the child that stays valid, and refers to the same
    /// process, after it is reaped. Signalling it after the reap fails rather
    /// than reaching a process that reused the child's PID.
    pidfd: OwnedFd,
    writes: mpsc::Sender<Vec<u8>>,
    queued: Arc<AtomicUsize>,
}

/// Start `program` with `args` on a new pseudo-terminal of `cols` by `rows`
/// characters. `wake` is called from the reader thread after each message on
/// the returned channel (the window's [`slopos_ui::platform::Proxy::wake`]).
pub fn spawn(
    program: &str,
    args: &[&str],
    cols: u16,
    rows: u16,
    wake: impl Fn() + Send + 'static,
) -> io::Result<(Pty, Receiver<Output>)> {
    let path = resolve_program(program)?;
    let mut argv_c = vec![c_string(program)?];
    for arg in args {
        argv_c.push(c_string(arg)?);
    }
    let mut argv: Vec<*const libc::c_char> = argv_c.iter().map(|arg| arg.as_ptr()).collect();
    argv.push(std::ptr::null());
    let env = child_environment()?;
    let mut envp: Vec<*const libc::c_char> = env.iter().map(|entry| entry.as_ptr()).collect();
    envp.push(std::ptr::null());
    let size = winsize(cols, rows);

    // A close-on-exec pipe: the child writes the errno here only if exec fails.
    let (exec_read, exec_write) = pipe_cloexec()?;
    let exec_write_fd = exec_write.as_raw_fd();
    let mut master: libc::c_int = -1;
    // SAFETY: on success forkpty stores a new master descriptor in `master`
    // and returns 0 in the child. `size` and the C strings outlive the call.
    let pid = unsafe { libc::forkpty(&mut master, std::ptr::null_mut(), std::ptr::null(), &size) };
    if pid < 0 {
        return Err(io::Error::last_os_error());
    }
    if pid == 0 {
        // SAFETY: after fork the child calls only async-signal-safe functions
        // (signal, execve, write, _exit) on memory prepared before the fork.
        unsafe {
            libc::signal(libc::SIGPIPE, libc::SIG_DFL);
            libc::execve(path.as_ptr(), argv.as_ptr(), envp.as_ptr());
            let errno = *libc::__errno_location();
            let bytes = errno.to_ne_bytes();
            libc::write(exec_write_fd, bytes.as_ptr().cast(), bytes.len());
            libc::_exit(127);
        }
    }
    // SAFETY: forkpty returned this descriptor, and nothing else owns it.
    let master = unsafe { OwnedFd::from_raw_fd(master) };
    drop(exec_write);

    // The parent's copy of the write end is closed, so end-of-file on the read
    // end means the exec succeeded. An errno means it failed; the child has
    // already exited, so it is reaped here.
    if let Some(errno) = read_exec_errno(&exec_read) {
        reap(pid);
        return Err(io::Error::other(format!(
            "could not start {program}: {}",
            io::Error::from_raw_os_error(errno)
        )));
    }

    let pidfd = match open_pidfd(pid) {
        Ok(pidfd) => pidfd,
        Err(error) => {
            kill_and_reap(pid);
            return Err(error);
        }
    };
    // Nothing below may leave the child running unowned: a failure kills it,
    // and the waiter (or a synchronous reap, before the waiter exists) collects it.
    let (exit_read, exit_write) = match pipe_cloexec() {
        Ok(pipe) => pipe,
        Err(error) => {
            kill_and_reap(pid);
            return Err(error);
        }
    };
    let reader_fd = match master.try_clone() {
        Ok(fd) => fd,
        Err(error) => {
            kill_and_reap(pid);
            return Err(error);
        }
    };
    let writer_fd = match master.try_clone() {
        Ok(fd) => fd,
        Err(error) => {
            kill_and_reap(pid);
            return Err(error);
        }
    };

    let (output_tx, output_rx) = mpsc::sync_channel(OUTPUT_QUEUE);
    // The waiter goes first: once it runs it reaps the child, so a later
    // failure only has to kill the child, and the waiter collects it.
    if let Err(error) = thread::Builder::new()
        .name("terminal-pty-waiter".into())
        .spawn(move || wait_for_child(pid, exit_write))
    {
        // No waiter exists, so this thread must reap the child itself.
        kill_and_reap(pid);
        return Err(error);
    }
    if let Err(error) = thread::Builder::new()
        .name("terminal-pty-reader".into())
        .spawn(move || read_output(reader_fd, exit_read, output_tx, wake))
    {
        kill_child(&pidfd);
        return Err(error);
    }

    let (write_tx, write_rx) = mpsc::channel::<Vec<u8>>();
    let queued = Arc::new(AtomicUsize::new(0));
    let writer_queued = Arc::clone(&queued);
    if let Err(error) = thread::Builder::new()
        .name("terminal-pty-writer".into())
        .spawn(move || {
            for chunk in write_rx {
                let result = write_all(writer_fd.as_raw_fd(), &chunk);
                writer_queued.fetch_sub(chunk.len(), Ordering::AcqRel);
                if result.is_err() {
                    break;
                }
            }
        })
    {
        kill_child(&pidfd);
        return Err(error);
    }

    Ok((
        Pty {
            master,
            pidfd,
            writes: write_tx,
            queued,
        },
        output_rx,
    ))
}

impl Pty {
    /// Queue bytes for the program's input. Never blocks the caller. Returns an
    /// error, and queues nothing, when the program is not reading fast enough
    /// to take the input within [`INPUT_BUDGET`].
    pub fn write(&self, bytes: &[u8]) -> io::Result<()> {
        let previous = self.queued.fetch_add(bytes.len(), Ordering::AcqRel);
        if previous + bytes.len() > INPUT_BUDGET {
            self.queued.fetch_sub(bytes.len(), Ordering::AcqRel);
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "the program is not reading its input",
            ));
        }
        self.writes.send(bytes.to_vec()).map_err(|_| {
            self.queued.fetch_sub(bytes.len(), Ordering::AcqRel);
            io::Error::new(io::ErrorKind::BrokenPipe, "the program has exited")
        })
    }

    /// Tell the program the window is now `cols` by `rows` characters.
    pub fn resize(&self, cols: u16, rows: u16) -> io::Result<()> {
        let size = winsize(cols, rows);
        // SAFETY: TIOCSWINSZ reads one `winsize` from the pointer given.
        let result = unsafe {
            libc::ioctl(
                self.master.as_raw_fd(),
                libc::TIOCSWINSZ,
                &size as *const libc::winsize,
            )
        };
        if result < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        // Hang up the program, as closing a real terminal does. The waiter
        // reaps it once it has exited; after that this signals nothing.
        kill_child_with(&self.pidfd, libc::SIGHUP);
    }
}

/// Block until the child exits, reap it, then tell the reader through the pipe.
fn wait_for_child(pid: libc::pid_t, exit: OwnedFd) {
    let mut status: libc::c_int = 0;
    loop {
        // SAFETY: waits for our own child and writes one status into `status`.
        let result = unsafe { libc::waitpid(pid, &mut status, 0) };
        if result == pid {
            break;
        }
        if result < 0 && last_errno() == libc::EINTR {
            continue;
        }
        // ECHILD: the child is already gone, which is as good as an exit.
        break;
    }
    let byte = [1u8];
    // SAFETY: writes one byte from memory we own. The reader only needs the
    // wake-up, so a short write or a closed reader is harmless.
    unsafe {
        libc::write(exit.as_raw_fd(), byte.as_ptr().cast(), 1);
    }
}

/// Forward program output until the program exits. Waits in `poll` for the
/// master or the exit pipe, whichever comes first.
fn read_output(master: OwnedFd, exit: OwnedFd, output: SyncSender<Output>, wake: impl Fn()) {
    let mut buffer = vec![0u8; 8192];
    let mut master_open = true;
    loop {
        let mut fds = Vec::with_capacity(2);
        if master_open {
            fds.push(libc::pollfd {
                fd: master.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            });
        }
        fds.push(libc::pollfd {
            fd: exit.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        });
        // SAFETY: poll reads and writes only the array we pass.
        let ready = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, -1) };
        if ready < 0 {
            if last_errno() == libc::EINTR {
                continue;
            }
            // poll cannot be relied on any more: report the exit now rather
            // than wait for it. The waiter still reaps the child.
            let _ = output.send(Output::Exited);
            wake();
            return;
        }
        let exit_ready = fds.last().is_some_and(|fd| fd.revents != 0);
        if master_open && fds[0].revents != 0 {
            match read_chunk(master.as_raw_fd(), &mut buffer) {
                Some(bytes) => {
                    // Blocks while the window is behind: backpressure, not loss.
                    if output.send(Output::Data(bytes)).is_err() {
                        return;
                    }
                    wake();
                }
                // End of file, or EIO once the program has closed its terminal.
                None => master_open = false,
            }
        }
        if exit_ready {
            // The program has exited, but a background job may still hold the
            // terminal open. Send what is already readable, then stop.
            while master_open && is_readable(master.as_raw_fd()) {
                match read_chunk(master.as_raw_fd(), &mut buffer) {
                    Some(bytes) => {
                        if output.send(Output::Data(bytes)).is_err() {
                            return;
                        }
                        wake();
                    }
                    None => break,
                }
            }
            let _ = output.send(Output::Exited);
            wake();
            return;
        }
    }
}

/// Read one chunk, or `None` at end of file or on an error that ends the stream.
fn read_chunk(fd: RawFd, buffer: &mut [u8]) -> Option<Vec<u8>> {
    loop {
        // SAFETY: reads at most `buffer.len()` bytes into memory we own.
        let count = unsafe { libc::read(fd, buffer.as_mut_ptr().cast(), buffer.len()) };
        if count > 0 {
            return Some(buffer[..count as usize].to_vec());
        }
        if count < 0 && last_errno() == libc::EINTR {
            continue;
        }
        return None;
    }
}

/// Whether `fd` has data or a hang-up pending, without blocking.
fn is_readable(fd: RawFd) -> bool {
    let mut poll_fd = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    // SAFETY: a zero timeout polls once and writes only `poll_fd`.
    let ready = unsafe { libc::poll(&mut poll_fd, 1, 0) };
    ready > 0 && poll_fd.revents != 0
}

fn write_all(fd: libc::c_int, mut bytes: &[u8]) -> io::Result<()> {
    while !bytes.is_empty() {
        // SAFETY: writes at most `bytes.len()` bytes from memory we borrow.
        let count = unsafe { libc::write(fd, bytes.as_ptr().cast(), bytes.len()) };
        if count < 0 {
            if last_errno() == libc::EINTR {
                continue;
            }
            return Err(io::Error::last_os_error());
        }
        bytes = &bytes[count as usize..];
    }
    Ok(())
}

/// Read the errno the child wrote before a failed exec, or `None` when exec
/// succeeded (the pipe reached end of file with nothing written).
fn read_exec_errno(pipe: &OwnedFd) -> Option<i32> {
    let mut bytes = [0u8; 4];
    let mut filled = 0;
    while filled < bytes.len() {
        // SAFETY: reads at most the remaining space of `bytes`.
        let count = unsafe {
            libc::read(
                pipe.as_raw_fd(),
                bytes[filled..].as_mut_ptr().cast(),
                bytes.len() - filled,
            )
        };
        if count < 0 && last_errno() == libc::EINTR {
            continue;
        }
        if count <= 0 {
            break;
        }
        filled += count as usize;
    }
    (filled == bytes.len()).then(|| i32::from_ne_bytes(bytes))
}

/// Reap a child that has already exited, retrying an interrupted wait.
fn reap(pid: libc::pid_t) {
    let mut status: libc::c_int = 0;
    loop {
        // SAFETY: waits for our own child and writes one status into `status`.
        let result = unsafe { libc::waitpid(pid, &mut status, 0) };
        if result < 0 && last_errno() == libc::EINTR {
            continue;
        }
        break;
    }
}

/// Kill a child that no waiter owns yet, then reap it.
fn kill_and_reap(pid: libc::pid_t) {
    // SAFETY: signals the child we forked; no other thread reaps it yet.
    unsafe {
        libc::kill(pid, libc::SIGKILL);
    }
    reap(pid);
}

fn kill_child(pidfd: &OwnedFd) {
    kill_child_with(pidfd, libc::SIGKILL);
}

fn kill_child_with(pidfd: &OwnedFd, signal: libc::c_int) {
    // SAFETY: a pidfd names one process. After it is reaped the call fails with
    // ESRCH and signals nothing else.
    unsafe {
        libc::syscall(
            libc::SYS_pidfd_send_signal,
            pidfd.as_raw_fd(),
            signal,
            std::ptr::null::<libc::c_void>(),
            0,
        );
    }
}

fn open_pidfd(pid: libc::pid_t) -> io::Result<OwnedFd> {
    // SAFETY: pidfd_open returns a new descriptor or -1.
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the descriptor was just returned by the kernel and nothing else owns it.
    Ok(unsafe { OwnedFd::from_raw_fd(fd as RawFd) })
}

fn pipe_cloexec() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut fds = [0 as libc::c_int; 2];
    // SAFETY: pipe2 writes two descriptors into `fds`.
    let result = unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the kernel just returned both descriptors, and nothing else owns them.
    Ok(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
}

/// The executable's path. A name with a slash is used as given; otherwise the
/// `PATH` entries are searched, as `execvp` would.
fn resolve_program(program: &str) -> io::Result<CString> {
    if program.contains('/') {
        return c_string(program);
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(program);
        let Ok(text) = CString::new(candidate.as_os_str().as_bytes()) else {
            continue;
        };
        // SAFETY: access checks the path and writes nothing.
        if unsafe { libc::access(text.as_ptr(), libc::X_OK) } == 0 {
            return Ok(text);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("no program named {program} on PATH"),
    ))
}

/// The parent's environment, with `TERM` set for the terminal.
fn child_environment() -> io::Result<Vec<CString>> {
    let mut entries = Vec::new();
    for (key, value) in std::env::vars_os() {
        if key == OsStr::new("TERM") {
            continue;
        }
        let mut entry = key.as_bytes().to_vec();
        entry.push(b'=');
        entry.extend_from_slice(value.as_bytes());
        entries.push(c_bytes(entry)?);
    }
    entries.push(c_string("TERM=xterm-256color")?);
    Ok(entries)
}

fn last_errno() -> i32 {
    io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

fn winsize(cols: u16, rows: u16) -> libc::winsize {
    libc::winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    }
}

fn c_string(text: &str) -> io::Result<CString> {
    c_bytes(text.as_bytes().to_vec())
}

fn c_bytes(bytes: Vec<u8>) -> io::Result<CString> {
    CString::new(bytes).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "a program name or argument contains a NUL byte",
        )
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::parser::Terminal;

    /// Feed the program's output to `terminal` until `needle` shows on the
    /// screen or the program exits. Returns whether the needle showed.
    fn pump_until(output: &Receiver<Output>, terminal: &mut Terminal, needle: &str) -> bool {
        while let Ok(message) = output.recv_timeout(Duration::from_secs(10)) {
            match message {
                Output::Data(bytes) => terminal.feed(&bytes),
                Output::Exited => break,
            }
            if terminal.grid().text().contains(needle) {
                return true;
            }
        }
        terminal.grid().text().contains(needle)
    }

    #[test]
    fn echo_output_appears_in_the_terminal_grid() {
        let (pty, output) = spawn("sh", &["-c", "echo hello-from-pty"], 40, 5, || {})
            .expect("a pseudo-terminal and /bin/sh are available");
        let mut terminal = Terminal::new(40, 5);
        assert!(pump_until(&output, &mut terminal, "hello-from-pty"));
        drop(pty);
    }

    #[test]
    fn resizing_the_pty_tells_the_program_the_new_size() {
        // `stty size` prints "rows cols" as the terminal reports them.
        let (pty, output) = spawn("sh", &["-c", "read line; stty size"], 80, 24, || {})
            .expect("a pseudo-terminal and /bin/sh are available");
        pty.resize(100, 30).expect("TIOCSWINSZ succeeds on a pty");
        pty.write(b"\n").expect("the input is accepted");
        let mut terminal = Terminal::new(100, 30);
        assert!(pump_until(&output, &mut terminal, "30 100"));
        drop(pty);
    }

    #[test]
    fn a_program_that_exits_is_reported() {
        let (pty, output) = spawn("sh", &["-c", "exit 0"], 20, 3, || {})
            .expect("a pseudo-terminal and /bin/sh are available");
        let mut seen_exit = false;
        while let Ok(message) = output.recv_timeout(Duration::from_secs(10)) {
            if message == Output::Exited {
                seen_exit = true;
                break;
            }
        }
        assert!(seen_exit);
        drop(pty);
    }

    #[test]
    fn a_missing_program_is_an_error_not_a_window_that_closes() {
        let error = spawn("slopos-no-such-program", &[], 20, 3, || {})
            .err()
            .expect("the program does not exist");
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn a_background_job_holding_the_terminal_does_not_keep_an_exited_shell_open() {
        // The shell exits while a background `sleep` still holds the slave.
        let (pty, output) = spawn("sh", &["-c", "echo done; sleep 30 & exit 0"], 20, 3, || {})
            .expect("a pseudo-terminal and /bin/sh are available");
        let mut terminal = Terminal::new(20, 3);
        let started = std::time::Instant::now();
        assert!(pump_until(&output, &mut terminal, "done"));
        let mut seen_exit = false;
        while let Ok(message) = output.recv_timeout(Duration::from_secs(10)) {
            if message == Output::Exited {
                seen_exit = true;
                break;
            }
        }
        assert!(
            seen_exit,
            "the exit is reported without waiting for the sleep"
        );
        assert!(started.elapsed() < Duration::from_secs(20));
        drop(pty);
    }

    #[test]
    fn input_past_the_budget_is_refused_not_queued() {
        // `sleep` never reads its input, so the queue can only grow.
        let (pty, _output) = spawn("sh", &["-c", "sleep 5"], 20, 3, || {})
            .expect("a pseudo-terminal and /bin/sh are available");
        let chunk = vec![b'x'; INPUT_BUDGET / 4];
        let mut refused = false;
        for _ in 0..16 {
            if pty.write(&chunk).is_err() {
                refused = true;
                break;
            }
        }
        assert!(refused, "writes beyond the budget are refused");
        drop(pty);
    }
}
