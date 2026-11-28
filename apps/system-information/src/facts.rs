//! The facts System Information shows, read from the system.
//!
//! [`gather`] asks a [`Source`] for each fact. [`LinuxSource`] reads
//! `/etc/os-release`, `/proc`, the environment and `statvfs`; the tests give
//! a fake source. A fact that cannot be read is [`Value::Unavailable`] with
//! the reason, never a guessed value.

use std::io;

/// The licence line shown in the window. It names the file that lists the
/// third-party licences; it does not restate them.
pub const LICENSE: &str =
    "SLOPOS-I is MIT licensed. Third-party licences are listed in THIRD_PARTY_LICENSES.txt.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Known(String),
    Unavailable(String),
}

impl Value {
    /// The text shown for this value.
    pub fn display(&self) -> String {
        match self {
            Self::Known(text) => text.clone(),
            Self::Unavailable(reason) => format!("Unavailable ({reason})"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fact {
    pub name: &'static str,
    pub value: Value,
}

/// Total and available bytes of a filesystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Space {
    pub total: u64,
    pub available: u64,
}

/// Where the facts come from.
pub trait Source {
    /// The text of a file, such as `/proc/meminfo`.
    fn read_file(&self, path: &str) -> io::Result<String>;
    /// An environment variable, if it is set to something other than blanks.
    fn env(&self, name: &str) -> Option<String>;
    /// Size and free space of the filesystem that holds `path`.
    fn space(&self, path: &str) -> io::Result<Space>;
    /// The CPU architecture, named as Rust names it (`x86_64`, `aarch64`).
    fn arch(&self) -> &'static str;
}

/// The running Linux system.
pub struct LinuxSource;

impl Source for LinuxSource {
    fn read_file(&self, path: &str) -> io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn env(&self, name: &str) -> Option<String> {
        std::env::var(name)
            .ok()
            .filter(|value| !value.trim().is_empty())
    }

    fn space(&self, path: &str) -> io::Result<Space> {
        let c_path = std::ffi::CString::new(path)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains NUL"))?;
        // SAFETY: statvfs writes into the zeroed struct we pass, and the path
        // is a valid NUL-terminated string that outlives the call.
        let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let block = stat.f_frsize as u64;
        Ok(Space {
            total: (stat.f_blocks as u64).saturating_mul(block),
            available: (stat.f_bavail as u64).saturating_mul(block),
        })
    }

    fn arch(&self) -> &'static str {
        std::env::consts::ARCH
    }
}

/// Every fact, in the order the window shows them.
pub fn gather(source: &dyn Source) -> Vec<Fact> {
    vec![
        fact(
            "SLOPOS version",
            Value::Known(env!("CARGO_PKG_VERSION").to_owned()),
        ),
        fact("Source revision", revision()),
        fact("Operating system", operating_system(source)),
        fact("Kernel", kernel(source)),
        fact("Architecture", Value::Known(source.arch().to_owned())),
        fact("CPU", cpu(source)),
        fact("Memory", memory(source)),
        fact("Storage (/)", storage(source)),
        fact("Display", display(source)),
        fact("Session", session(source)),
        fact("License", Value::Known(LICENSE.to_owned())),
    ]
}

fn fact(name: &'static str, value: Value) -> Fact {
    Fact { name, value }
}

fn revision() -> Value {
    match option_env!("SLOPOS_GIT_REVISION") {
        Some(revision) if !revision.trim().is_empty() => Value::Known(revision.to_owned()),
        _ => Value::Unavailable("not recorded in this build".to_owned()),
    }
}

/// Why a file could not be read, in words a user can act on.
fn unreadable(what: &str, error: &io::Error) -> Value {
    let reason = match error.kind() {
        io::ErrorKind::NotFound => "not found".to_owned(),
        io::ErrorKind::PermissionDenied => "permission denied".to_owned(),
        _ => error.to_string(),
    };
    Value::Unavailable(format!("cannot read {what}: {reason}"))
}

fn operating_system(source: &dyn Source) -> Value {
    match source.read_file("/etc/os-release") {
        Ok(text) => match parse_os_release(&text) {
            Some(name) => Value::Known(name),
            None => Value::Unavailable("no NAME in /etc/os-release".to_owned()),
        },
        Err(error) => unreadable("/etc/os-release", &error),
    }
}

fn kernel(source: &dyn Source) -> Value {
    match source.read_file("/proc/sys/kernel/osrelease") {
        Ok(text) if !text.trim().is_empty() => Value::Known(text.trim().to_owned()),
        Ok(_) => Value::Unavailable("the kernel release is empty".to_owned()),
        Err(error) => unreadable("/proc/sys/kernel/osrelease", &error),
    }
}

fn cpu(source: &dyn Source) -> Value {
    match source.read_file("/proc/cpuinfo") {
        Ok(text) => match parse_cpuinfo(&text) {
            Some(cpu) => Value::Known(cpu.describe()),
            None => Value::Unavailable("no processors in /proc/cpuinfo".to_owned()),
        },
        Err(error) => unreadable("/proc/cpuinfo", &error),
    }
}

fn memory(source: &dyn Source) -> Value {
    match source.read_file("/proc/meminfo") {
        Ok(text) => match parse_mem_total(&text) {
            Some(bytes) => Value::Known(format_bytes(bytes)),
            None => Value::Unavailable("no MemTotal in /proc/meminfo".to_owned()),
        },
        Err(error) => unreadable("/proc/meminfo", &error),
    }
}

fn storage(source: &dyn Source) -> Value {
    match source.space("/") {
        Ok(space) => Value::Known(format!(
            "{} total, {} free",
            format_bytes(space.total),
            format_bytes(space.available)
        )),
        Err(error) => {
            Value::Unavailable(format!("cannot read the filesystem of /: {}", error.kind()))
        }
    }
}

fn display(source: &dyn Source) -> Value {
    if let Some(display) = source.env("DISPLAY") {
        Value::Known(format!("X11 display {display}"))
    } else if let Some(display) = source.env("WAYLAND_DISPLAY") {
        Value::Known(format!("Wayland display {display}"))
    } else {
        Value::Unavailable("no display in the environment".to_owned())
    }
}

fn session(source: &dyn Source) -> Value {
    let parts: Vec<String> = [
        source
            .env("XDG_SESSION_TYPE")
            .map(|kind| format!("type {kind}")),
        source
            .env("XDG_CURRENT_DESKTOP")
            .map(|desktop| format!("desktop {desktop}")),
    ]
    .into_iter()
    .flatten()
    .collect();
    if parts.is_empty() {
        Value::Unavailable("not set in the environment".to_owned())
    } else {
        Value::Known(parts.join(", "))
    }
}

/// `NAME` or `PRETTY_NAME` from an os-release file; `PRETTY_NAME` wins.
pub fn parse_os_release(text: &str) -> Option<String> {
    let mut pretty = None;
    let mut name = None;
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches(|c| c == '"' || c == '\'').trim();
        if value.is_empty() {
            continue;
        }
        match key.trim() {
            "PRETTY_NAME" => pretty = Some(value.to_owned()),
            "NAME" => name = Some(value.to_owned()),
            _ => {}
        }
    }
    pretty.or(name)
}

/// The processors listed in `/proc/cpuinfo`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cpu {
    /// `model name` on x86, or the board `Model`/`Hardware` line on ARM.
    pub model: Option<String>,
    pub logical: usize,
}

impl Cpu {
    pub fn describe(&self) -> String {
        let count = if self.logical == 1 {
            "1 logical CPU".to_owned()
        } else {
            format!("{} logical CPUs", self.logical)
        };
        match &self.model {
            Some(model) => format!("{model}, {count}"),
            None => count,
        }
    }
}

pub fn parse_cpuinfo(text: &str) -> Option<Cpu> {
    let mut model = None;
    let mut logical = 0;
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match key.trim() {
            "processor" => logical += 1,
            "model name" | "Model" | "Hardware" if model.is_none() => {
                let value = value.trim();
                if !value.is_empty() {
                    model = Some(value.to_owned());
                }
            }
            _ => {}
        }
    }
    (logical > 0).then_some(Cpu { model, logical })
}

/// `MemTotal` from `/proc/meminfo`, in bytes. The file reports kibibytes.
pub fn parse_mem_total(text: &str) -> Option<u64> {
    text.lines().find_map(|line| {
        let rest = line.strip_prefix("MemTotal:")?;
        let kib: u64 = rest.split_whitespace().next()?.parse().ok()?;
        Some(kib.saturating_mul(1024))
    })
}

/// Binary units with one decimal place: `1536` is `1.5 KiB`.
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

#[cfg(test)]
pub(crate) mod fake {
    use super::*;
    use std::collections::HashMap;

    /// A source that answers from fixtures. A missing entry is an error,
    /// the way a missing file is on a real system.
    #[derive(Default)]
    pub struct Fake {
        pub files: HashMap<String, String>,
        pub env: HashMap<String, String>,
        pub space: Option<Space>,
    }

    impl Source for Fake {
        fn read_file(&self, path: &str) -> io::Result<String> {
            self.files
                .get(path)
                .cloned()
                .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
        }

        fn env(&self, name: &str) -> Option<String> {
            self.env.get(name).cloned()
        }

        fn space(&self, _path: &str) -> io::Result<Space> {
            self.space
                .ok_or_else(|| io::Error::from(io::ErrorKind::PermissionDenied))
        }

        fn arch(&self) -> &'static str {
            "x86_64"
        }
    }

    /// A complete, healthy system.
    pub fn healthy() -> Fake {
        let mut fake = Fake::default();
        fake.files.insert(
            "/etc/os-release".to_owned(),
            "NAME=\"Debian GNU/Linux\"\nPRETTY_NAME=\"Debian GNU/Linux 12 (bookworm)\"\n"
                .to_owned(),
        );
        fake.files.insert(
            "/proc/sys/kernel/osrelease".to_owned(),
            "6.1.0-18-amd64\n".to_owned(),
        );
        fake.files.insert(
            "/proc/cpuinfo".to_owned(),
            "processor\t: 0\nmodel name\t: Intel(R) Core(TM) i5\n\nprocessor\t: 1\nmodel name\t: Intel(R) Core(TM) i5\n"
                .to_owned(),
        );
        fake.files.insert(
            "/proc/meminfo".to_owned(),
            "MemTotal:       16384 kB\nMemFree:         1024 kB\n".to_owned(),
        );
        fake.env.insert("DISPLAY".to_owned(), ":0".to_owned());
        fake.env
            .insert("XDG_SESSION_TYPE".to_owned(), "x11".to_owned());
        fake.env
            .insert("XDG_CURRENT_DESKTOP".to_owned(), "SLOPOS".to_owned());
        fake.space = Some(Space {
            total: 64 * 1024 * 1024 * 1024,
            available: 16 * 1024 * 1024 * 1024,
        });
        fake
    }
}

#[cfg(test)]
mod tests {
    use super::fake::{healthy, Fake};
    use super::*;

    fn value(facts: &[Fact], name: &str) -> String {
        facts
            .iter()
            .find(|fact| fact.name == name)
            .unwrap_or_else(|| panic!("no fact named {name}"))
            .value
            .display()
    }

    #[test]
    fn os_release_prefers_pretty_name_and_strips_quotes() {
        let text = "NAME=\"Arch Linux\"\nPRETTY_NAME=\"Arch Linux\"\nID=arch\n";
        assert_eq!(parse_os_release(text).as_deref(), Some("Arch Linux"));
        assert_eq!(parse_os_release("NAME=Fedora\n").as_deref(), Some("Fedora"));
        assert_eq!(parse_os_release("ID=arch\n"), None);
        assert_eq!(parse_os_release("PRETTY_NAME=\"\"\n"), None);
    }

    #[test]
    fn cpuinfo_counts_processors_and_reads_the_model() {
        let text = "processor\t: 0\nmodel name\t: Intel(R) Core(TM) i5\n\nprocessor\t: 1\nmodel name\t: Intel(R) Core(TM) i5\n";
        let cpu = parse_cpuinfo(text).unwrap();
        assert_eq!(cpu.logical, 2);
        assert_eq!(cpu.describe(), "Intel(R) Core(TM) i5, 2 logical CPUs");
    }

    #[test]
    fn arm_cpuinfo_uses_the_board_model_and_counts_cores() {
        let text = "processor\t: 0\nprocessor\t: 1\nprocessor\t: 2\nprocessor\t: 3\nHardware\t: BCM2835\nModel\t\t: Raspberry Pi Zero 2 W Rev 1.0\n";
        let cpu = parse_cpuinfo(text).unwrap();
        assert_eq!(cpu.logical, 4);
        assert_eq!(cpu.model.as_deref(), Some("BCM2835"));
        assert_eq!(cpu.describe(), "BCM2835, 4 logical CPUs");
    }

    #[test]
    fn cpuinfo_without_processors_is_none() {
        assert_eq!(parse_cpuinfo("model name : x\n"), None);
        assert_eq!(parse_cpuinfo(""), None);
    }

    #[test]
    fn meminfo_total_is_read_in_kibibytes() {
        let text = "MemTotal:       16384 kB\nMemFree:  1 kB\n";
        assert_eq!(parse_mem_total(text), Some(16 * 1024 * 1024));
        assert_eq!(parse_mem_total("MemFree: 1 kB\n"), None);
        assert_eq!(parse_mem_total("MemTotal: lots kB\n"), None);
    }

    #[test]
    fn byte_counts_use_binary_units() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1536), "1.5 KiB");
        assert_eq!(format_bytes(16 * 1024 * 1024 * 1024), "16.0 GiB");
    }

    #[test]
    fn a_healthy_system_shows_every_value() {
        let facts = gather(&healthy());
        assert_eq!(value(&facts, "SLOPOS version"), env!("CARGO_PKG_VERSION"));
        assert_eq!(
            value(&facts, "Operating system"),
            "Debian GNU/Linux 12 (bookworm)"
        );
        assert_eq!(value(&facts, "Kernel"), "6.1.0-18-amd64");
        assert_eq!(value(&facts, "Architecture"), "x86_64");
        assert_eq!(value(&facts, "CPU"), "Intel(R) Core(TM) i5, 2 logical CPUs");
        assert_eq!(value(&facts, "Memory"), "16.0 MiB");
        assert_eq!(
            value(&facts, "Storage (/)"),
            "64.0 GiB total, 16.0 GiB free"
        );
        assert_eq!(value(&facts, "Display"), "X11 display :0");
        assert_eq!(value(&facts, "Session"), "type x11, desktop SLOPOS");
    }

    #[test]
    fn a_missing_file_is_shown_as_unavailable_with_the_reason() {
        let facts = gather(&Fake::default());
        assert_eq!(
            value(&facts, "Operating system"),
            "Unavailable (cannot read /etc/os-release: not found)"
        );
        assert_eq!(
            value(&facts, "Memory"),
            "Unavailable (cannot read /proc/meminfo: not found)"
        );
        assert_eq!(
            value(&facts, "Storage (/)"),
            "Unavailable (cannot read the filesystem of /: permission denied)"
        );
    }

    #[test]
    fn missing_environment_is_unavailable_not_guessed() {
        let facts = gather(&Fake::default());
        assert_eq!(
            value(&facts, "Display"),
            "Unavailable (no display in the environment)"
        );
        assert_eq!(
            value(&facts, "Session"),
            "Unavailable (not set in the environment)"
        );
    }

    #[test]
    fn a_cpuinfo_without_processors_says_so() {
        let mut fake = healthy();
        fake.files
            .insert("/proc/cpuinfo".to_owned(), "nothing here\n".to_owned());
        let facts = gather(&fake);
        assert_eq!(
            value(&facts, "CPU"),
            "Unavailable (no processors in /proc/cpuinfo)"
        );
    }

    #[test]
    fn the_license_line_names_the_third_party_file() {
        let facts = gather(&healthy());
        assert_eq!(value(&facts, "License"), LICENSE);
        assert!(LICENSE.contains("THIRD_PARTY_LICENSES.txt"));
    }
}
