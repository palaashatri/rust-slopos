//! Pure parsers for the Linux `/proc` and `/etc` text formats the monitor
//! reads. Every function takes the file's text (or bytes) and returns plain
//! values; nothing here touches the file system, so the tests run on
//! fixtures.

use std::collections::HashMap;

/// Kernel clock ticks in `/proc/stat` and `/proc/<pid>/stat` are counted in
/// units the monitor only ever compares with each other, never converts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CpuTicks {
    /// Every tick counted for the CPU(s), idle and iowait included.
    pub total: u64,
    /// Idle and iowait ticks.
    pub idle: u64,
}

/// `(all CPUs, one entry per logical CPU)` from `/proc/stat`.
pub type CpuStat = (CpuTicks, Vec<CpuTicks>);

fn parse_ticks(fields: &str) -> Option<CpuTicks> {
    // user nice system idle iowait irq softirq steal [guest guest_nice]:
    // guest time is already inside user time, so only the first eight count.
    let values: Vec<u64> = fields
        .split_whitespace()
        .take(8)
        .map(|field| field.parse().ok())
        .collect::<Option<_>>()?;
    if values.len() < 4 {
        return None;
    }
    let idle = values[3] + values.get(4).copied().unwrap_or(0);
    Some(CpuTicks {
        total: values.iter().sum(),
        idle,
    })
}

/// Parse `/proc/stat`. The aggregate `cpu` line is required; per-CPU
/// `cpuN` lines are optional.
pub fn parse_cpu_stat(text: &str) -> Option<CpuStat> {
    let mut total = None;
    let mut cores = Vec::new();
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("cpu") else {
            continue;
        };
        let (label, fields) = match rest.split_once(char::is_whitespace) {
            Some(pair) => pair,
            None => continue,
        };
        if label.is_empty() {
            total = parse_ticks(fields);
        } else if label.bytes().all(|byte| byte.is_ascii_digit()) {
            if let Some(ticks) = parse_ticks(fields) {
                cores.push(ticks);
            }
        }
    }
    total.map(|total| (total, cores))
}

/// Memory figures from `/proc/meminfo`, in bytes. A field the file does not
/// have is `None`, never zero.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MemInfo {
    pub total: Option<u64>,
    pub free: Option<u64>,
    pub available: Option<u64>,
    pub buffers: Option<u64>,
    pub cached: Option<u64>,
}

/// Parse `/proc/meminfo`. Its values are in kibibytes.
pub fn parse_meminfo(text: &str) -> MemInfo {
    let mut info = MemInfo::default();
    for line in text.lines() {
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        let slot = match key.trim() {
            "MemTotal" => &mut info.total,
            "MemFree" => &mut info.free,
            "MemAvailable" => &mut info.available,
            "Buffers" => &mut info.buffers,
            "Cached" => &mut info.cached,
            _ => continue,
        };
        let mut words = rest.split_whitespace();
        if let (Some(value), Some("kB")) = (words.next(), words.next()) {
            if let Ok(kib) = value.parse::<u64>() {
                *slot = Some(kib.saturating_mul(1024));
            }
        }
    }
    info
}

/// Sectors read and written by one block device, from `/proc/diskstats`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskStat {
    pub name: String,
    pub sectors_read: u64,
    pub sectors_written: u64,
}

/// Bytes per sector in `/proc/diskstats`, whatever the device's own sector
/// size is: the kernel counts those fields in 512-byte units.
pub const DISKSTATS_SECTOR_BYTES: u64 = 512;

/// Parse `/proc/diskstats`: `major minor name reads ... sectors_read ...
/// writes ... sectors_written ...`. Lines too short to hold the counters are
/// skipped.
pub fn parse_diskstats(text: &str) -> Vec<DiskStat> {
    text.lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 10 {
                return None;
            }
            Some(DiskStat {
                name: fields[2].to_owned(),
                sectors_read: fields[5].parse().ok()?,
                sectors_written: fields[9].parse().ok()?,
            })
        })
        .collect()
}

/// Whether a `/proc/diskstats` name is a whole disk rather than a partition,
/// a loop, RAM or ZRAM device. Partitions and virtual devices would count
/// the same bytes twice or add traffic that is not on a disk.
pub fn is_whole_disk(name: &str) -> bool {
    fn letters_only(rest: &str) -> bool {
        !rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_lowercase())
    }
    fn digits_only(rest: &str) -> bool {
        !rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_digit())
    }
    if let Some(rest) = name.strip_prefix("nvme") {
        // nvme0n1, not nvme0n1p1.
        return match rest.split_once('n') {
            Some((controller, namespace)) => digits_only(controller) && digits_only(namespace),
            None => false,
        };
    }
    if let Some(rest) = name.strip_prefix("mmcblk") {
        return digits_only(rest);
    }
    ["sd", "vd", "xvd", "hd"]
        .iter()
        .any(|prefix| name.strip_prefix(prefix).is_some_and(letters_only))
}

/// The fields of `/proc/<pid>/stat` the monitor uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PidStat {
    /// The executable's name as the kernel records it (at most 15 bytes).
    pub comm: String,
    /// The one-letter state: R, S, D, Z, T, t, X, I and so on.
    pub state: char,
    pub ppid: u32,
    /// Ticks spent in user mode plus kernel mode.
    pub cpu_ticks: u64,
    pub threads: u64,
    /// Start time since boot, in ticks. Together with the PID it identifies a
    /// process across the PID being reused.
    pub start_ticks: u64,
}

/// Parse `/proc/<pid>/stat`. The command name is in parentheses and may
/// contain spaces or parentheses, so the record is split at the last `)`.
pub fn parse_pid_stat(text: &str) -> Option<PidStat> {
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    if close < open {
        return None;
    }
    let comm = text[open + 1..close].to_owned();
    let fields: Vec<&str> = text[close + 1..].split_whitespace().collect();
    // Field 3 (state) is fields[0]; field n is fields[n - 3].
    let field = |number: usize| fields.get(number - 3).copied();
    let state = field(3)?.chars().next()?;
    let ppid = field(4)?.parse().ok()?;
    let utime: u64 = field(14)?.parse().ok()?;
    let stime: u64 = field(15)?.parse().ok()?;
    let threads = field(20)?.parse().ok()?;
    let start_ticks = field(22)?.parse().ok()?;
    Some(PidStat {
        comm,
        state,
        ppid,
        cpu_ticks: utime.saturating_add(stime),
        threads,
        start_ticks,
    })
}

/// The fields of `/proc/<pid>/status` the monitor uses.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PidStatus {
    pub name: Option<String>,
    /// The real user ID (the first `Uid:` value).
    pub uid: Option<u32>,
    pub threads: Option<u64>,
    /// Resident set size, in bytes.
    pub rss_bytes: Option<u64>,
}

/// Parse `/proc/<pid>/status`, a list of `Key:\tvalue` lines.
pub fn parse_pid_status(text: &str) -> PidStatus {
    let mut status = PidStatus::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key {
            "Name" => status.name = Some(value.to_owned()),
            "Uid" => status.uid = value.split_whitespace().next().and_then(|v| v.parse().ok()),
            "Threads" => status.threads = value.parse().ok(),
            "VmRSS" => {
                let mut words = value.split_whitespace();
                if let (Some(number), Some("kB")) = (words.next(), words.next()) {
                    status.rss_bytes = number.parse::<u64>().ok().map(|kib| kib * 1024);
                }
            }
            _ => {}
        }
    }
    status
}

/// The command line from `/proc/<pid>/cmdline`: NUL-separated arguments,
/// joined with spaces. Empty for kernel threads and for processes whose
/// arguments are not readable.
pub fn parse_cmdline(bytes: &[u8]) -> String {
    bytes
        .split(|&byte| byte == 0)
        .filter(|argument| !argument.is_empty())
        .map(|argument| String::from_utf8_lossy(argument).into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

/// User names by user ID from `/etc/passwd`. When a name appears twice the
/// first entry wins, as the C library's lookup does.
pub fn parse_passwd(text: &str) -> HashMap<u32, String> {
    let mut users = HashMap::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split(':').collect();
        if fields.len() < 3 {
            continue;
        }
        if let Ok(uid) = fields[2].parse::<u32>() {
            users.entry(uid).or_insert_with(|| fields[0].to_owned());
        }
    }
    users
}

/// The long name of a one-letter process state, as the monitor shows it.
pub fn state_name(state: char) -> &'static str {
    match state {
        'R' => "running",
        'S' => "sleeping",
        'D' => "waiting for disk",
        'Z' => "zombie",
        'T' => "stopped",
        't' => "traced",
        'X' | 'x' => "dead",
        'I' => "idle",
        'K' => "wake kill",
        'W' => "waking",
        'P' => "parked",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAT: &str = "cpu  4705 150 1120 16250 830 0 25 0 0 0\n\
cpu0 1176 37 280 4062 207 0 6 0 0 0\n\
cpu1 1200 40 290 4050 210 0 7 0 0 0\n\
intr 123\n";

    #[test]
    fn cpu_stat_sums_the_aggregate_and_each_core() {
        let (total, cores) = parse_cpu_stat(STAT).expect("the fixture parses");
        // user + nice + system + idle + iowait + irq + softirq + steal.
        assert_eq!(total.total, 4705 + 150 + 1120 + 16250 + 830 + 25);
        assert_eq!(total.idle, 16250 + 830);
        assert_eq!(cores.len(), 2);
        assert_eq!(cores[1].idle, 4050 + 210);
    }

    #[test]
    fn cpu_stat_without_an_aggregate_line_is_none() {
        assert_eq!(parse_cpu_stat("cpu0 1 2 3 4\n"), None);
        assert_eq!(parse_cpu_stat("cpu  1 2 3\n"), None);
    }

    #[test]
    fn meminfo_converts_kib_and_reports_missing_fields_as_none() {
        let info = parse_meminfo(
            "MemTotal:       16000000 kB\nMemFree:  1000 kB\nMemAvailable:   8000000 kB\nCached:  2048 kB\nBuffers: 12 kB\nSwapTotal: 0 kB\n",
        );
        assert_eq!(info.total, Some(16_000_000 * 1024));
        assert_eq!(info.available, Some(8_000_000 * 1024));
        assert_eq!(info.cached, Some(2048 * 1024));
        assert_eq!(info.buffers, Some(12 * 1024));
        assert_eq!(parse_meminfo("Cached: 5 kB\n").total, None);
        // A line without the kB unit is not trusted.
        assert_eq!(parse_meminfo("MemTotal: 5 MB\n").total, None);
    }

    #[test]
    fn diskstats_reads_sectors_read_and_written() {
        let stats = parse_diskstats(
            "   8       0 sda 100 0 2048 30 50 0 4096 20 0 10 50\n\
   8       1 sda1 10 0 16 1 5 0 8 2 0 1 3\n\
short line\n",
        );
        assert_eq!(stats.len(), 2);
        assert_eq!(stats[0].name, "sda");
        assert_eq!(stats[0].sectors_read, 2048);
        assert_eq!(stats[0].sectors_written, 4096);
    }

    #[test]
    fn only_whole_physical_disks_are_counted() {
        for name in ["sda", "vdb", "xvda", "hdc", "nvme0n1", "mmcblk0"] {
            assert!(is_whole_disk(name), "{name} is a whole disk");
        }
        for name in [
            "sda1",
            "nvme0n1p1",
            "mmcblk0p2",
            "loop0",
            "zram0",
            "ram0",
            "sr0",
            "dm-0",
            "sd",
            "nvme",
        ] {
            assert!(!is_whole_disk(name), "{name} is not a whole disk");
        }
    }

    const PID_STAT: &str = "4242 (my (odd) app) S 1 4242 4242 0 -1 4194560 100 0 0 0 \
150 50 0 0 20 0 3 0 9876 12345678 400 18446744073709551615 1 1 0 0 0 0 0 0 0 0 0 0 17 2 0 0 0 0 0";

    #[test]
    fn pid_stat_handles_parentheses_in_the_command_name() {
        let stat = parse_pid_stat(PID_STAT).expect("the fixture parses");
        assert_eq!(stat.comm, "my (odd) app");
        assert_eq!(stat.state, 'S');
        assert_eq!(stat.ppid, 1);
        assert_eq!(stat.cpu_ticks, 200);
        assert_eq!(stat.threads, 3);
        assert_eq!(stat.start_ticks, 9876);
    }

    #[test]
    fn truncated_pid_stat_is_rejected() {
        assert_eq!(parse_pid_stat("12 (short) S 1 2"), None);
        assert_eq!(parse_pid_stat("no parentheses here"), None);
    }

    #[test]
    fn pid_status_reads_uid_threads_and_rss() {
        let status = parse_pid_status(
            "Name:\tbash\nState:\tS (sleeping)\nUid:\t1000\t1000\t1000\t1000\nThreads:\t1\nVmRSS:\t   5120 kB\n",
        );
        assert_eq!(status.name.as_deref(), Some("bash"));
        assert_eq!(status.uid, Some(1000));
        assert_eq!(status.threads, Some(1));
        assert_eq!(status.rss_bytes, Some(5120 * 1024));
    }

    #[test]
    fn kernel_threads_have_no_rss_and_report_none() {
        let status = parse_pid_status("Name:\tkthreadd\nUid:\t0\t0\t0\t0\nThreads:\t1\n");
        assert_eq!(status.rss_bytes, None);
        assert_eq!(status.uid, Some(0));
    }

    #[test]
    fn cmdline_joins_nul_separated_arguments() {
        assert_eq!(
            parse_cmdline(b"/usr/bin/python3\0-m\0http.server\0\0"),
            "/usr/bin/python3 -m http.server"
        );
        assert_eq!(parse_cmdline(b""), "");
        assert_eq!(parse_cmdline(b"caf\xc3\xa9\0"), "café");
    }

    #[test]
    fn passwd_maps_uids_and_keeps_the_first_name() {
        let users = parse_passwd(
            "root:x:0:0:root:/root:/bin/bash\nuser:x:1000:1000::/home/user:/bin/sh\nalias:x:1000:1000::/:/bin/false\nbroken\n",
        );
        assert_eq!(users.get(&0).map(String::as_str), Some("root"));
        assert_eq!(users.get(&1000).map(String::as_str), Some("user"));
        assert_eq!(users.get(&4242), None);
    }

    #[test]
    fn state_letters_have_names_and_unknown_letters_say_so() {
        assert_eq!(state_name('R'), "running");
        assert_eq!(state_name('Z'), "zombie");
        assert_eq!(state_name('?'), "unknown");
    }
}
