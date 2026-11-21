//! A program on a pseudo-terminal. `forkpty` gives the child a terminal as
//! its controlling tty; this module keeps the master side.
//!
//! A reader thread blocks in `read` on the master, sends what it gets over a
//! channel and wakes the window, so nothing polls. Writes go through a writer
//! thread, so a program that stops reading cannot freeze the window. When the
//! program exits, the reader reaps it and sends [`Output::Exited`].

use std::ffi::CString;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

/// What the program printed, or that it has exited.
#[derive(Debug, PartialEq, Eq)]
pub enum Output {
    Data(Vec<u8>),
    Exited,
}

/// The master side of a running program.
pub struct Pty {
    master: OwnedFd,
    pid: libc::pid_t,
    writes: Sender<Vec<u8>>,
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
    let program_c = c_string(program)?;
    let mut argv_c = vec![program_c.clone()];
    for arg in args {
        argv_c.push(c_string(arg)?);
    }
    let mut argv: Vec<*const libc::c_char> = argv_c.iter().map(|arg| arg.as_ptr()).collect();
    argv.push(std::ptr::null());
    let term_name = c_string("TERM")?;
    let term_value = c_string("xterm-256color")?;
    let size = winsize(cols, rows);

    let mut master: libc::c_int = -1;
    // SAFETY: on success forkpty stores a new master descriptor in `master`
    // and returns 0 in the child. `size` and the C strings outlive the call.
    let pid = unsafe { libc::forkpty(&mut master, std::ptr::null_mut(), std::ptr::null(), &size) };
    if pid < 0 {
        return Err(io::Error::last_os_error());
    }
    if pid == 0 {
        // SAFETY: the child only resets a signal, sets one variable and execs.
        // It never returns into Rust code that could allocate or lock.
        unsafe {
            libc::signal(libc::SIGPIPE, libc::SIG_DFL);
            libc::setenv(term_name.as_ptr(), term_value.as_ptr(), 1);
            libc::execvp(program_c.as_ptr(), argv.as_ptr());
            libc::_exit(127);
        }
    }

    // SAFETY: forkpty returned this descriptor, and nothing else owns it.
    let master = unsafe { OwnedFd::from_raw_fd(master) };
    let reader_fd = master.try_clone()?;
    let writer_fd = master.try_clone()?;

    let (output_tx, output_rx) = mpsc::channel();
    thread::Builder::new()
        .name("terminal-pty-reader".into())
        .spawn(move || read_until_exit(reader_fd, pid, output_tx, wake))?;

    let (write_tx, write_rx) = mpsc::channel::<Vec<u8>>();
    thread::Builder::new()
        .name("terminal-pty-writer".into())
        .spawn(move || {
            for chunk in write_rx {
                if write_all(writer_fd.as_raw_fd(), &chunk).is_err() {
                    break;
                }
            }
        })?;

    Ok((
        Pty {
            master,
            pid,
            writes: write_tx,
        },
        output_rx,
    ))
}

impl Pty {
    /// Queue bytes for the program's input. Never blocks the caller.
    pub fn write(&self, bytes: &[u8]) {
        // The writer thread is gone only after the program has gone too.
        let _ = self.writes.send(bytes.to_vec());
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
        // Hang up the program, as closing a real terminal does. The reader
        // thread reaps it once it has exited.
        // SAFETY: signalling the child we started; the pid stays ours until reaped.
        unsafe {
            libc::kill(self.pid, libc::SIGHUP);
        }
    }
}

fn read_until_exit(fd: OwnedFd, pid: libc::pid_t, output: Sender<Output>, wake: impl Fn()) {
    let mut buffer = vec![0u8; 8192];
    loop {
        // SAFETY: reads at most `buffer.len()` bytes into memory we own.
        let count = unsafe { libc::read(fd.as_raw_fd(), buffer.as_mut_ptr().cast(), buffer.len()) };
        if count > 0 {
            if output
                .send(Output::Data(buffer[..count as usize].to_vec()))
                .is_err()
            {
                break;
            }
            wake();
        } else if count < 0 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
            continue;
        } else {
            // End of file, or EIO once the program has closed its terminal.
            break;
        }
    }
    let mut status: libc::c_int = 0;
    // SAFETY: waits for our own child; the program has closed its terminal, so
    // it is exiting.
    unsafe {
        libc::waitpid(pid, &mut status, 0);
    }
    let _ = output.send(Output::Exited);
    wake();
}

fn write_all(fd: libc::c_int, mut bytes: &[u8]) -> io::Result<()> {
    while !bytes.is_empty() {
        // SAFETY: writes at most `bytes.len()` bytes from memory we borrow.
        let count = unsafe { libc::write(fd, bytes.as_ptr().cast(), bytes.len()) };
        if count < 0 {
            if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(io::Error::last_os_error());
        }
        bytes = &bytes[count as usize..];
    }
    Ok(())
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
    CString::new(text).map_err(|_| {
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
        pty.write(b"\n");
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
}
