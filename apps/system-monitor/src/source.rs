//! Reads the system: `/proc`, `/etc/passwd`, `statvfs` and `kill`. The
//! parsing is in [`crate::procfs`]; this module only fetches the text and
//! hands it over, so the app can run against a fake source in tests.

use std::collections::HashMap;
use std::ffi::CString;
use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::gpu::{self, GpuInfo};
use crate::model::{FsSpace, ProcSample, Snapshot};
use crate::policy::Identity;
use crate::procfs;

/// Everything the app needs from the operating system.
pub trait Source {
    /// Read every reading the views use.
    fn sample(&mut self, at: Instant) -> Snapshot;
    /// What `/proc` says about one PID now; `None` when it does not exist.
    fn identity(&self, pid: u32) -> Option<Identity>;
    /// Send SIGTERM to `pid`.
    fn send_sigterm(&self, pid: u32) -> io::Result<()>;
    /// The effective user ID of the monitor.
    fn current_user(&self) -> u32;
    /// User names by UID, read once.
    fn user_names(&self) -> HashMap<u32, String>;
    /// The GPU, probed on first request.
    fn gpu(&mut self) -> GpuInfo;
}

/// The Linux implementation, reading the real system.
pub struct LinuxSource {
    proc_root: PathBuf,
    filesystem: PathBuf,
    passwd: PathBuf,
}

impl Default for LinuxSource {
    fn default() -> Self {
        Self {
            proc_root: PathBuf::from("/proc"),
            filesystem: PathBuf::from("/"),
            passwd: PathBuf::from("/etc/passwd"),
        }
    }
}

impl Source for LinuxSource {
    fn sample(&mut self, at: Instant) -> Snapshot {
        read_snapshot(&self.proc_root, &self.filesystem, at)
    }

    fn identity(&self, pid: u32) -> Option<Identity> {
        read_identity(&self.proc_root, pid)
    }

    fn send_sigterm(&self, pid: u32) -> io::Result<()> {
        let pid = libc::pid_t::try_from(pid)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "PID out of range"))?;
        // SAFETY: kill(2) takes a PID and a signal number and touches no memory.
        if unsafe { libc::kill(pid, libc::SIGTERM) } == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    fn current_user(&self) -> u32 {
        // SAFETY: geteuid(2) has no arguments and cannot fail.
        unsafe { libc::geteuid() }
    }

    fn user_names(&self) -> HashMap<u32, String> {
        fs::read_to_string(&self.passwd)
            .map(|text| procfs::parse_passwd(&text))
            .unwrap_or_default()
    }

    fn gpu(&mut self) -> GpuInfo {
        gpu::probe()
    }
}

/// Read one sampling pass from `proc_root` and the file system at `filesystem`.
pub fn read_snapshot(proc_root: &Path, filesystem: &Path, at: Instant) -> Snapshot {
    let cpu = fs::read_to_string(proc_root.join("stat"))
        .ok()
        .and_then(|text| procfs::parse_cpu_stat(&text));
    let memory = fs::read_to_string(proc_root.join("meminfo"))
        .ok()
        .map(|text| procfs::parse_meminfo(&text))
        .filter(|info| info.total.is_some());
    let disks = fs::read_to_string(proc_root.join("diskstats"))
        .ok()
        .map(|text| procfs::parse_diskstats(&text));
    Snapshot {
        at,
        cpu,
        memory,
        disks,
        filesystem: filesystem_space(filesystem),
        processes: read_processes(proc_root),
    }
}

fn read_processes(proc_root: &Path) -> Option<Vec<ProcSample>> {
    let entries = fs::read_dir(proc_root).ok()?;
    let mut processes = Vec::new();
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        // A process may exit between the listing and the reads: skip it then.
        if let Some(process) = read_process(&entry.path(), pid) {
            processes.push(process);
        }
    }
    Some(processes)
}

fn read_process(dir: &Path, pid: u32) -> Option<ProcSample> {
    let stat = procfs::parse_pid_stat(&fs::read_to_string(dir.join("stat")).ok()?)?;
    let status = fs::read_to_string(dir.join("status"))
        .map(|text| procfs::parse_pid_status(&text))
        .unwrap_or_default();
    let cmdline = fs::read(dir.join("cmdline"))
        .map(|bytes| procfs::parse_cmdline(&bytes))
        .unwrap_or_default();
    Some(ProcSample {
        pid,
        ppid: stat.ppid,
        name: status.name.unwrap_or(stat.comm),
        state: stat.state,
        uid: status.uid,
        cpu_ticks: stat.cpu_ticks,
        start_ticks: stat.start_ticks,
        threads: status.threads.or(Some(stat.threads)),
        rss_bytes: status.rss_bytes,
        cmdline,
    })
}

fn read_identity(proc_root: &Path, pid: u32) -> Option<Identity> {
    let dir = proc_root.join(pid.to_string());
    let stat = procfs::parse_pid_stat(&fs::read_to_string(dir.join("stat")).ok()?)?;
    let uid = fs::read_to_string(dir.join("status"))
        .ok()
        .and_then(|text| procfs::parse_pid_status(&text).uid);
    Some(Identity {
        start_ticks: stat.start_ticks,
        uid,
        state: stat.state,
    })
}

/// Total and available space on the file system holding `path`. The
/// "free" figure is what an ordinary user may use (`f_bavail`).
pub fn filesystem_space(path: &Path) -> Option<FsSpace> {
    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: statvfs(3) writes into the zeroed struct we pass; the path is a
    // valid NUL-terminated string that outlives the call.
    let mut buffer: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c_path.as_ptr(), &mut buffer) } != 0 {
        return None;
    }
    let unit = buffer.f_frsize as u64;
    Some(FsSpace {
        total: (buffer.f_blocks as u64).saturating_mul(unit),
        free: (buffer.f_bavail as u64).saturating_mul(unit),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().expect("a parent")).expect("create directory");
        fs::write(path, text).expect("write fixture");
    }

    #[test]
    fn a_fixture_proc_tree_is_read_into_a_snapshot() {
        let root = tempfile::tempdir().expect("a temporary directory");
        let proc_root = root.path().join("proc");
        write(
            &proc_root.join("stat"),
            "cpu  10 0 10 80 0 0 0 0 0 0\ncpu0 10 0 10 80 0 0 0 0 0 0\n",
        );
        write(
            &proc_root.join("meminfo"),
            "MemTotal: 2048 kB\nMemAvailable: 1024 kB\n",
        );
        write(
            &proc_root.join("diskstats"),
            "8 0 sda 1 0 8 0 1 0 8 0 0 0 0\n",
        );
        write(
            &proc_root.join("42/stat"),
            "42 (sh) S 1 42 42 0 -1 0 0 0 0 0 5 5 0 0 20 0 1 0 900 0 0\n",
        );
        write(
            &proc_root.join("42/status"),
            "Name:\tsh\nUid:\t1000\t1000\t1000\t1000\n",
        );
        fs::write(proc_root.join("42/cmdline"), b"sh\0-c\0true\0").expect("write fixture");
        // A directory that is not a PID is ignored.
        write(&proc_root.join("self-ish/stat"), "garbage");

        let snapshot = read_snapshot(&proc_root, root.path(), Instant::now() + Duration::ZERO);
        let processes = snapshot.processes.expect("proc is readable");
        assert_eq!(processes.len(), 1);
        assert_eq!(processes[0].pid, 42);
        assert_eq!(processes[0].name, "sh");
        assert_eq!(processes[0].uid, Some(1000));
        assert_eq!(processes[0].cmdline, "sh -c true");
        assert_eq!(processes[0].rss_bytes, None, "no VmRSS line is not zero");
        assert_eq!(snapshot.memory.and_then(|m| m.total), Some(2048 * 1024));
        assert_eq!(snapshot.disks.map(|d| d.len()), Some(1));
        assert!(snapshot.cpu.is_some());

        let identity = read_identity(&proc_root, 42).expect("the process exists");
        assert_eq!(identity.start_ticks, 900);
        assert_eq!(read_identity(&proc_root, 43), None);
    }

    #[test]
    fn a_missing_proc_is_reported_as_unreadable() {
        let root = tempfile::tempdir().expect("a temporary directory");
        let snapshot = read_snapshot(&root.path().join("nope"), root.path(), Instant::now());
        assert_eq!(snapshot.processes, None);
        assert_eq!(snapshot.cpu, None);
        assert_eq!(snapshot.memory, None);
    }

    #[test]
    fn the_root_file_system_has_a_real_size() {
        let space = filesystem_space(Path::new("/")).expect("statvfs of / works");
        assert!(space.total > 0);
        assert!(space.free <= space.total);
        assert_eq!(filesystem_space(Path::new("/definitely/not/here")), None);
    }
}
