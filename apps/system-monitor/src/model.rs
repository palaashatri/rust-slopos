//! The monitor's data model: raw readings in a [`Snapshot`], and the values
//! the views show, computed from two snapshots by [`Monitor`].
//!
//! Rates and percentages need two readings, so until the second snapshot
//! they are `None` and the views say "measuring". Nothing here invents a
//! value: a reading that is missing stays `None` all the way to the screen.

use std::collections::HashMap;
use std::time::Instant;

use crate::procfs::{self, CpuStat, CpuTicks, DiskStat, MemInfo, DISKSTATS_SECTOR_BYTES};

/// Space on a mounted file system, from `statvfs`, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FsSpace {
    pub total: u64,
    pub free: u64,
}

/// One process as read from `/proc/<pid>/{stat,status,cmdline}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcSample {
    pub pid: u32,
    pub ppid: u32,
    pub name: String,
    pub state: char,
    pub uid: Option<u32>,
    pub cpu_ticks: u64,
    pub start_ticks: u64,
    pub threads: Option<u64>,
    pub rss_bytes: Option<u64>,
    pub cmdline: String,
}

/// Everything read in one sampling pass. A part that could not be read is
/// `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub at: Instant,
    pub cpu: Option<CpuStat>,
    pub memory: Option<MemInfo>,
    pub disks: Option<Vec<DiskStat>>,
    pub filesystem: Option<FsSpace>,
    /// `None` when `/proc` itself cannot be listed.
    pub processes: Option<Vec<ProcSample>>,
}

/// CPU share of one process over the last interval, 0 to 100 of the whole
/// machine (so 100 means every logical CPU was busy with it).
pub fn process_cpu_percent(delta_ticks: u64, total_delta_ticks: u64) -> Option<f64> {
    if total_delta_ticks == 0 {
        return None;
    }
    let share = delta_ticks as f64 / total_delta_ticks as f64 * 100.0;
    Some(share.clamp(0.0, 100.0))
}

/// Busy share of all CPUs between two readings, 0 to 100. `None` when the
/// counters went backwards or nothing ticked.
pub fn cpu_busy_percent(previous: CpuTicks, current: CpuTicks) -> Option<f64> {
    let total = current.total.checked_sub(previous.total)?;
    let idle = current.idle.checked_sub(previous.idle)?;
    if total == 0 {
        return None;
    }
    let busy = total.saturating_sub(idle);
    Some((busy as f64 / total as f64 * 100.0).clamp(0.0, 100.0))
}

/// Bytes per second read and written by the whole disks, between two
/// `/proc/diskstats` readings. Only devices present in both readings are
/// summed, so a disk plugged in between does not show a spike.
pub fn disk_rates(
    previous: (&[DiskStat], Instant),
    current: (&[DiskStat], Instant),
) -> Option<(f64, f64)> {
    let elapsed = current.1.checked_duration_since(previous.1)?.as_secs_f64();
    if elapsed <= 0.0 {
        return None;
    }
    let (mut read, mut written) = (0u64, 0u64);
    let mut matched = false;
    for now in current
        .0
        .iter()
        .filter(|stat| procfs::is_whole_disk(&stat.name))
    {
        let Some(before) = previous.0.iter().find(|stat| stat.name == now.name) else {
            continue;
        };
        matched = true;
        read += now.sectors_read.saturating_sub(before.sectors_read);
        written += now.sectors_written.saturating_sub(before.sectors_written);
    }
    if !matched {
        return None;
    }
    let bytes_per_second = |sectors: u64| sectors as f64 * DISKSTATS_SECTOR_BYTES as f64 / elapsed;
    Some((bytes_per_second(read), bytes_per_second(written)))
}

/// A process as the views show it.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessRow {
    pub pid: u32,
    pub ppid: u32,
    pub name: String,
    pub state: char,
    pub uid: Option<u32>,
    /// The user's name, or the numeric ID when `/etc/passwd` has no entry.
    pub user: String,
    pub cpu_percent: Option<f64>,
    pub rss_bytes: Option<u64>,
    pub threads: Option<u64>,
    pub start_ticks: u64,
    pub cmdline: String,
}

/// Values for the CPU view.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CpuView {
    pub logical_cpus: usize,
    pub total_percent: Option<f64>,
    /// `(CPU number, busy share)` for each CPU that is online in this sample.
    pub per_cpu_percent: Vec<(usize, Option<f64>)>,
}

/// Values for the Disk view.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DiskView {
    pub filesystem: Option<FsSpace>,
    pub read_bytes_per_second: Option<f64>,
    pub written_bytes_per_second: Option<f64>,
    pub whole_disks: usize,
}

/// Everything the views show, as of the latest snapshot.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct View {
    pub cpu: Option<CpuView>,
    pub memory: Option<MemInfo>,
    pub disk: DiskView,
    /// Sorted by CPU share (highest first, unknown last), then PID.
    pub processes: Vec<ProcessRow>,
    /// Whether `/proc` could be listed in the latest snapshot.
    pub processes_readable: bool,
}

/// Turns snapshots into a [`View`], remembering what the rates need.
#[derive(Debug, Default)]
pub struct Monitor {
    users: HashMap<u32, String>,
    previous_cpu: Option<CpuStat>,
    previous_disks: Option<(Vec<DiskStat>, Instant)>,
    previous_processes: HashMap<u32, (u64, u64)>,
    pub view: View,
}

impl Monitor {
    pub fn new(users: HashMap<u32, String>) -> Self {
        Self {
            users,
            ..Self::default()
        }
    }

    /// Fold one snapshot into the view.
    pub fn ingest(&mut self, snapshot: Snapshot) {
        let total_delta = self.ingest_cpu(&snapshot);
        self.view.memory = snapshot.memory;
        self.ingest_disks(&snapshot);
        self.ingest_processes(snapshot.processes, total_delta);
    }

    /// Returns the change in all-CPU ticks since the previous snapshot, which
    /// the process shares are measured against.
    fn ingest_cpu(&mut self, snapshot: &Snapshot) -> Option<u64> {
        let Some((total, cores)) = &snapshot.cpu else {
            self.previous_cpu = None;
            self.view.cpu = None;
            return None;
        };
        let (total_delta, cpu) = match &self.previous_cpu {
            Some((previous_total, previous_cores)) => {
                let total_delta = total.total.checked_sub(previous_total.total);
                // Match each CPU with its own earlier reading by number, so an
                // offline CPU does not shift the others onto the wrong history.
                let per_cpu = cores
                    .iter()
                    .map(|(id, now)| {
                        let before = previous_cores
                            .iter()
                            .find(|(previous_id, _)| previous_id == id)
                            .map(|(_, ticks)| *ticks);
                        (
                            *id,
                            before.and_then(|before| cpu_busy_percent(before, *now)),
                        )
                    })
                    .collect();
                (
                    total_delta,
                    CpuView {
                        logical_cpus: cores.len(),
                        total_percent: cpu_busy_percent(*previous_total, *total),
                        per_cpu_percent: per_cpu,
                    },
                )
            }
            None => (
                None,
                CpuView {
                    logical_cpus: cores.len(),
                    total_percent: None,
                    per_cpu_percent: cores.iter().map(|(id, _)| (*id, None)).collect(),
                },
            ),
        };
        self.view.cpu = Some(cpu);
        self.previous_cpu = Some((*total, cores.clone()));
        total_delta.filter(|delta| *delta > 0)
    }

    fn ingest_disks(&mut self, snapshot: &Snapshot) {
        let rates = match (&self.previous_disks, &snapshot.disks) {
            (Some((before, before_at)), Some(now)) => {
                disk_rates((before, *before_at), (now, snapshot.at))
            }
            _ => None,
        };
        let whole_disks = snapshot.disks.as_ref().map_or(0, |disks| {
            disks
                .iter()
                .filter(|d| procfs::is_whole_disk(&d.name))
                .count()
        });
        self.view.disk = DiskView {
            filesystem: snapshot.filesystem,
            read_bytes_per_second: rates.map(|(read, _)| read),
            written_bytes_per_second: rates.map(|(_, written)| written),
            whole_disks,
        };
        self.previous_disks = snapshot.disks.clone().map(|disks| (disks, snapshot.at));
    }

    fn ingest_processes(&mut self, processes: Option<Vec<ProcSample>>, total_delta: Option<u64>) {
        let Some(processes) = processes else {
            self.view.processes.clear();
            self.view.processes_readable = false;
            self.previous_processes.clear();
            return;
        };
        let mut rows = Vec::with_capacity(processes.len());
        let mut next = HashMap::with_capacity(processes.len());
        for process in processes {
            let cpu_percent = match self.previous_processes.get(&process.pid) {
                // The same process as before: a reused PID has another start time.
                Some(&(start, ticks)) if start == process.start_ticks => {
                    total_delta.and_then(|total| {
                        process_cpu_percent(process.cpu_ticks.saturating_sub(ticks), total)
                    })
                }
                _ => None,
            };
            next.insert(process.pid, (process.start_ticks, process.cpu_ticks));
            let user = match process.uid {
                Some(uid) => self
                    .users
                    .get(&uid)
                    .cloned()
                    .unwrap_or_else(|| uid.to_string()),
                None => String::from("unavailable"),
            };
            rows.push(ProcessRow {
                pid: process.pid,
                ppid: process.ppid,
                name: process.name,
                state: process.state,
                uid: process.uid,
                user,
                cpu_percent,
                rss_bytes: process.rss_bytes,
                threads: process.threads,
                start_ticks: process.start_ticks,
                cmdline: process.cmdline,
            });
        }
        rows.sort_by(|a, b| {
            let key = |row: &ProcessRow| row.cpu_percent.unwrap_or(-1.0);
            key(b)
                .partial_cmp(&key(a))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.pid.cmp(&b.pid))
        });
        self.view.processes = rows;
        self.view.processes_readable = true;
        self.previous_processes = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn ticks(total: u64, idle: u64) -> CpuTicks {
        CpuTicks { total, idle }
    }

    #[test]
    fn busy_percent_is_the_non_idle_share_of_the_interval() {
        // 100 ticks passed, 25 of them idle: 75% busy.
        let percent = cpu_busy_percent(ticks(1000, 500), ticks(1100, 525));
        assert_eq!(percent, Some(75.0));
        assert_eq!(cpu_busy_percent(ticks(1000, 500), ticks(1000, 500)), None);
        // Counters that went backwards (a counter reset) give no value.
        assert_eq!(cpu_busy_percent(ticks(1000, 500), ticks(900, 400)), None);
    }

    #[test]
    fn process_share_is_relative_to_all_cpu_ticks() {
        assert_eq!(process_cpu_percent(50, 200), Some(25.0));
        assert_eq!(process_cpu_percent(300, 200), Some(100.0));
        assert_eq!(process_cpu_percent(5, 0), None);
    }

    fn disk(name: &str, read: u64, written: u64) -> DiskStat {
        DiskStat {
            name: name.to_owned(),
            sectors_read: read,
            sectors_written: written,
        }
    }

    #[test]
    fn disk_rates_sum_whole_disks_per_second() {
        let start = Instant::now();
        let before = vec![disk("sda", 0, 0), disk("sda1", 0, 0), disk("loop0", 0, 0)];
        let now = vec![
            disk("sda", 2048, 4096),
            disk("sda1", 9999, 9999),
            disk("loop0", 9999, 9999),
        ];
        let (read, written) = disk_rates((&before, start), (&now, start + Duration::from_secs(2)))
            .expect("the disk is in both readings");
        // 2048 sectors of 512 bytes in 2 seconds: 512 KiB/s.
        assert_eq!(read, 512.0 * 1024.0);
        assert_eq!(written, 1024.0 * 1024.0);
    }

    #[test]
    fn disk_rates_need_a_positive_interval_and_a_common_disk() {
        let start = Instant::now();
        let disks = vec![disk("sda", 1, 1)];
        assert_eq!(disk_rates((&disks, start), (&disks, start)), None);
        let other = vec![disk("sdb", 1, 1)];
        assert_eq!(
            disk_rates((&disks, start), (&other, start + Duration::from_secs(1))),
            None
        );
    }

    fn proc_sample(pid: u32, start: u64, ticks: u64, name: &str) -> ProcSample {
        ProcSample {
            pid,
            ppid: 1,
            name: name.to_owned(),
            state: 'S',
            uid: Some(1000),
            cpu_ticks: ticks,
            start_ticks: start,
            threads: Some(1),
            rss_bytes: Some(4096),
            cmdline: String::new(),
        }
    }

    fn snapshot(at: Instant, total: u64, idle: u64, processes: Vec<ProcSample>) -> Snapshot {
        Snapshot {
            at,
            cpu: Some((
                ticks(total, idle),
                vec![
                    (0, ticks(total / 2, idle / 2)),
                    (1, ticks(total / 2, idle / 2)),
                ],
            )),
            memory: None,
            disks: None,
            filesystem: None,
            processes: Some(processes),
        }
    }

    #[test]
    fn the_first_snapshot_measures_nothing_and_the_second_gives_shares() {
        let mut monitor = Monitor::new(HashMap::from([(1000, "user".to_owned())]));
        let start = Instant::now();
        monitor.ingest(snapshot(
            start,
            1000,
            500,
            vec![proc_sample(7, 42, 10, "worker")],
        ));
        let cpu = monitor.view.cpu.clone().expect("the CPU is read");
        assert_eq!(cpu.total_percent, None, "one reading is not a rate");
        assert_eq!(monitor.view.processes[0].cpu_percent, None);

        monitor.ingest(snapshot(
            start + Duration::from_secs(1),
            1200,
            550,
            vec![
                proc_sample(7, 42, 60, "worker"),
                proc_sample(9, 5, 3, "idle"),
            ],
        ));
        let cpu = monitor.view.cpu.clone().expect("the CPU is read");
        assert_eq!(cpu.total_percent, Some(75.0));
        assert_eq!(cpu.logical_cpus, 2);
        let worker = monitor
            .view
            .processes
            .iter()
            .find(|row| row.pid == 7)
            .expect("the worker is listed");
        // 50 ticks of its own out of 200 of the machine.
        assert_eq!(worker.cpu_percent, Some(25.0));
        assert_eq!(worker.user, "user");
        // Sorted by share: the worker first.
        assert_eq!(monitor.view.processes[0].pid, 7);
    }

    #[test]
    fn a_reused_pid_does_not_inherit_the_old_process_share() {
        let mut monitor = Monitor::new(HashMap::new());
        let start = Instant::now();
        monitor.ingest(snapshot(
            start,
            1000,
            500,
            vec![proc_sample(7, 42, 10, "old")],
        ));
        monitor.ingest(snapshot(
            start + Duration::from_secs(1),
            1200,
            550,
            vec![proc_sample(7, 99, 500, "new")],
        ));
        assert_eq!(monitor.view.processes[0].cpu_percent, None);
    }

    #[test]
    fn unknown_users_show_their_numeric_id_and_unreadable_proc_is_flagged() {
        let mut monitor = Monitor::new(HashMap::new());
        let mut process = proc_sample(3, 1, 1, "svc");
        process.uid = Some(977);
        monitor.ingest(Snapshot {
            at: Instant::now(),
            cpu: None,
            memory: None,
            disks: None,
            filesystem: None,
            processes: Some(vec![process]),
        });
        assert_eq!(monitor.view.processes[0].user, "977");
        assert!(
            monitor.view.cpu.is_none(),
            "a missing CPU reading stays missing"
        );

        monitor.ingest(Snapshot {
            at: Instant::now(),
            cpu: None,
            memory: None,
            disks: None,
            filesystem: None,
            processes: None,
        });
        assert!(!monitor.view.processes_readable);
        assert!(monitor.view.processes.is_empty());
    }

    #[test]
    fn a_cpu_that_goes_offline_keeps_its_own_history() {
        let mut monitor = Monitor::new(HashMap::new());
        let at = Instant::now();
        let reading = |cores: Vec<(usize, CpuTicks)>| Snapshot {
            at,
            cpu: Some((ticks(1000, 500), cores)),
            memory: None,
            disks: None,
            filesystem: None,
            processes: Some(Vec::new()),
        };
        monitor.ingest(reading(vec![
            (0, ticks(100, 50)),
            (1, ticks(100, 10)),
            (2, ticks(1000, 500)),
        ]));
        // CPU 1 goes offline. CPU 2 must still be compared with its own earlier
        // counters, not with the counters that used to sit at its position.
        monitor.ingest(reading(vec![(0, ticks(200, 100)), (2, ticks(1100, 500))]));
        let cpu = monitor.view.cpu.clone().expect("the CPU view is present");
        assert_eq!(cpu.per_cpu_percent.len(), 2);
        assert_eq!(cpu.per_cpu_percent[1], (2, Some(100.0)));
    }
}
