//! Background work for Files, all event driven: folder scans, file
//! operations and an inotify watcher. Each wakes the UI through the
//! platform [`Proxy`] when it has something new; nothing polls.

use std::ffi::CString;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use slopos_ui::platform::Proxy;

use crate::archive;
use crate::model::{self, Entry};
use crate::ops::{self, Conflict, Job, OpError};
use crate::trash::{self, TrashItem};

/// Where the platform proxy is stored once the window exists.
pub type WakeSlot = Arc<Mutex<Option<Proxy>>>;

pub fn wake(slot: &WakeSlot) {
    if let Ok(guard) = slot.lock() {
        if let Some(proxy) = guard.as_ref() {
            proxy.wake();
        }
    }
}

pub type ScanResult = (u64, PathBuf, io::Result<Vec<Entry>>);

/// One thread reading folders; at most two requests queue.
pub struct Scanner {
    requests: mpsc::SyncSender<(u64, PathBuf)>,
    pub results: mpsc::Receiver<ScanResult>,
}

impl Scanner {
    pub fn new(slot: WakeSlot) -> io::Result<Self> {
        let (requests, rx) = mpsc::sync_channel::<(u64, PathBuf)>(2);
        let (tx, results) = mpsc::channel::<ScanResult>();
        thread::Builder::new()
            .name("files-read-dir".into())
            .spawn(move || {
                while let Ok((seq, path)) = rx.recv() {
                    let result = model::read_directory(&path);
                    if tx.send((seq, path, result)).is_err() {
                        break;
                    }
                    wake(&slot);
                }
            })?;
        Ok(Self { requests, results })
    }

    pub fn request(&self, seq: u64, path: PathBuf) -> Result<(), &'static str> {
        match self.requests.try_send((seq, path)) {
            Ok(()) => Ok(()),
            Err(mpsc::TrySendError::Full(_)) => Err("Folder worker busy. Try again."),
            Err(mpsc::TrySendError::Disconnected(_)) => Err("Folder worker unavailable."),
        }
    }
}

/// A file operation the user asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    Copy {
        sources: Vec<PathBuf>,
        dest: PathBuf,
        conflict: Conflict,
    },
    Move {
        sources: Vec<PathBuf>,
        dest: PathBuf,
        conflict: Conflict,
    },
    Trash(Vec<PathBuf>),
    Delete(Vec<PathBuf>),
    Restore(Vec<TrashItem>),
    Erase(Vec<TrashItem>),
    EmptyTrash,
    Compress {
        sources: Vec<PathBuf>,
        dest: PathBuf,
        format: archive::Format,
    },
    /// Extract each archive next to itself.
    Extract(Vec<PathBuf>),
}

impl Operation {
    pub fn verb(&self) -> &'static str {
        match self {
            Self::Copy { .. } => "Copying",
            Self::Move { .. } => "Moving",
            Self::Trash(_) => "Moving to Trash",
            Self::Delete(_) => "Deleting",
            Self::Restore(_) => "Restoring",
            Self::Erase(_) | Self::EmptyTrash => "Emptying Trash",
            Self::Compress { .. } => "Compressing",
            Self::Extract(_) => "Extracting",
        }
    }

    /// The same operation with a different conflict policy (after asking).
    pub fn with_conflict(&self, policy: Conflict) -> Self {
        match self.clone() {
            Self::Copy { sources, dest, .. } => Self::Copy {
                sources,
                dest,
                conflict: policy,
            },
            Self::Move { sources, dest, .. } => Self::Move {
                sources,
                dest,
                conflict: policy,
            },
            other => other,
        }
    }
}

#[derive(Debug)]
pub struct Finished {
    pub operation: Operation,
    /// Paths to select afterwards, or the error.
    pub result: Result<Vec<PathBuf>, OpError>,
}

/// Run `operation` on its own thread; the result arrives on the returned
/// receiver and wakes the UI. The job carries cancel and progress.
pub fn start(
    operation: Operation,
    slot: WakeSlot,
) -> io::Result<(Arc<Job>, mpsc::Receiver<Finished>)> {
    let job = Arc::new(Job::default());
    let progress_slot = slot.clone();
    job.on_progress(move || wake(&progress_slot));
    let (tx, rx) = mpsc::channel();
    let worker_job = job.clone();
    thread::Builder::new()
        .name("files-operation".into())
        .spawn(move || {
            let result = run(&operation, &worker_job);
            let _ = tx.send(Finished { operation, result });
            wake(&slot);
        })?;
    Ok((job, rx))
}

fn run(operation: &Operation, job: &Job) -> Result<Vec<PathBuf>, OpError> {
    match operation {
        Operation::Copy {
            sources,
            dest,
            conflict,
        } => ops::copy_into(sources, dest, *conflict, job),
        Operation::Move {
            sources,
            dest,
            conflict,
        } => ops::move_into(sources, dest, *conflict, job),
        Operation::Trash(paths) => {
            for path in paths {
                if job.is_cancelled() {
                    return Err(OpError::Cancelled);
                }
                trash::trash(path)?;
            }
            Ok(Vec::new())
        }
        Operation::Delete(paths) => {
            for path in paths {
                if job.is_cancelled() {
                    return Err(OpError::Cancelled);
                }
                ops::delete_permanently(path)?;
            }
            Ok(Vec::new())
        }
        Operation::Restore(items) => items.iter().map(trash::restore).collect(),
        Operation::Erase(items) => {
            for item in items {
                trash::erase(item)?;
            }
            Ok(Vec::new())
        }
        Operation::EmptyTrash => trash::empty().map(|_| Vec::new()),
        Operation::Compress {
            sources,
            dest,
            format,
        } => archive::compress(sources, dest, *format, job).map(|path| vec![path]),
        Operation::Extract(archives) => archives
            .iter()
            .map(|path| archive::extract(path, job))
            .collect(),
    }
}

/// Watches one folder with inotify and sets `changed` (and wakes the UI)
/// when its entries change. The thread blocks in read(2).
pub struct Watcher {
    fd: i32,
    watch: Mutex<i32>,
    pub changed: Arc<AtomicBool>,
}

const WATCH_MASK: u32 = libc::IN_CREATE
    | libc::IN_DELETE
    | libc::IN_MOVED_FROM
    | libc::IN_MOVED_TO
    | libc::IN_CLOSE_WRITE
    | libc::IN_ATTRIB
    | libc::IN_DELETE_SELF
    | libc::IN_MOVE_SELF;

impl Watcher {
    pub fn new(slot: WakeSlot) -> io::Result<Arc<Self>> {
        // SAFETY: plain syscall.
        let fd = unsafe { libc::inotify_init1(libc::IN_CLOEXEC) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let watcher = Arc::new(Self {
            fd,
            watch: Mutex::new(-1),
            changed: Arc::new(AtomicBool::new(false)),
        });
        let changed = watcher.changed.clone();
        thread::Builder::new()
            .name("files-watch".into())
            .spawn(move || {
                let mut buffer = [0u8; 4096];
                loop {
                    // SAFETY: reading into a local buffer of the given size.
                    let read = unsafe { libc::read(fd, buffer.as_mut_ptr().cast(), buffer.len()) };
                    if read < 0 {
                        if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                            continue;
                        }
                        break;
                    }
                    if read == 0 {
                        break;
                    }
                    changed.store(true, Ordering::Relaxed);
                    wake(&slot);
                }
            })?;
        Ok(watcher)
    }

    /// Watch `dir` instead of the previous folder.
    pub fn watch(&self, dir: &Path) {
        let mut current = self.watch.lock().unwrap_or_else(|p| p.into_inner());
        if *current >= 0 {
            // SAFETY: removing a watch descriptor we added.
            unsafe { libc::inotify_rm_watch(self.fd, *current) };
        }
        *current = -1;
        let Ok(path) = CString::new(dir.as_os_str().as_bytes()) else {
            return;
        };
        // SAFETY: valid fd and NUL-terminated path.
        let wd = unsafe { libc::inotify_add_watch(self.fd, path.as_ptr(), WATCH_MASK) };
        if wd >= 0 {
            *current = wd;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn operations_report_results_and_cancellation() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("a.txt");
        std::fs::write(&src, b"a").unwrap();
        let dest = tmp.path().join("out");
        std::fs::create_dir(&dest).unwrap();
        let slot: WakeSlot = Arc::new(Mutex::new(None));
        let (_, rx) = start(
            Operation::Copy {
                sources: vec![src.clone()],
                dest: dest.clone(),
                conflict: Conflict::Ask,
            },
            slot.clone(),
        )
        .unwrap();
        let done = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(done.result.unwrap(), [dest.join("a.txt")]);
        // A second copy asks about the conflict; retrying keeps both.
        let op = Operation::Copy {
            sources: vec![src],
            dest: dest.clone(),
            conflict: Conflict::Ask,
        };
        let (_, rx) = start(op.clone(), slot.clone()).unwrap();
        let done = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(done.result, Err(OpError::Exists(_))));
        let (_, rx) = start(op.with_conflict(Conflict::KeepBoth), slot).unwrap();
        let done = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(done.result.unwrap(), [dest.join("a 2.txt")]);
    }

    #[test]
    fn watcher_flags_changes_in_the_watched_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let slot: WakeSlot = Arc::new(Mutex::new(None));
        let watcher = Watcher::new(slot).unwrap();
        watcher.watch(tmp.path());
        std::fs::write(tmp.path().join("new"), b"x").unwrap();
        for _ in 0..100 {
            if watcher.changed.load(Ordering::Relaxed) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("no inotify change seen");
    }
}
