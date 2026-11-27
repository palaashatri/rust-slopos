//! GPU hardware as the kernel reports it: the PCI display controllers under
//! sysfs, the driver bound to each one, and whether NVIDIA's proprietary
//! driver is loaded.
//!
//! The parsing is pure. It takes the text of the sysfs files, so it is tested
//! with fixtures. [`read_report`] is the thin adapter that reads a real sysfs
//! tree and `/proc`. Nothing is guessed: a value that cannot be read is
//! reported as unknown, and a tree that cannot be read as unavailable.

use std::fs;
use std::io;
use std::path::Path;

/// The PCI bus in sysfs.
pub const PCI_DEVICES: &str = "/sys/bus/pci/devices";
/// Present only while NVIDIA's proprietary kernel module is loaded.
pub const NVIDIA_PROPRIETARY_VERSION: &str = "/proc/driver/nvidia/version";

/// The PCI vendor, from the `vendor` file of a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vendor {
    Nvidia,
    Amd,
    Intel,
    /// A vendor ID this module does not name. The ID is kept.
    Other(u16),
    /// The `vendor` file is missing or is not a hexadecimal ID.
    Unknown,
}

impl Vendor {
    pub fn from_pci_id(id: u16) -> Self {
        match id {
            0x10de => Vendor::Nvidia,
            0x1002 => Vendor::Amd,
            0x8086 => Vendor::Intel,
            other => Vendor::Other(other),
        }
    }
}

/// The driver in use, from the name of the `driver` symlink.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Driver {
    Nvidia,
    Nouveau,
    Amdgpu,
    I915,
    /// Any other bound driver, under the name the kernel gives it.
    Other(String),
    /// No driver is bound to the device.
    None,
    /// The `driver` link exists but cannot be read.
    Unknown,
}

impl Driver {
    fn from_link_name(name: &str) -> Self {
        match name {
            "nvidia" => Driver::Nvidia,
            "nouveau" => Driver::Nouveau,
            "amdgpu" => Driver::Amdgpu,
            "i915" => Driver::I915,
            other => Driver::Other(other.to_string()),
        }
    }
}

/// What the kernel says about one device's `driver` link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriverLink {
    /// The link points at a driver, by its file name (the driver's name).
    Bound(String),
    /// The link does not exist: no driver is bound.
    Unbound,
    /// The link exists but cannot be read.
    Unreadable,
}

/// The text of one PCI device directory. `None` means the file could not be
/// read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PciDevice {
    /// The directory name, for example `0000:01:00.0`.
    pub address: String,
    pub class: Option<String>,
    pub vendor: Option<String>,
    pub device: Option<String>,
    pub driver: DriverLink,
}

/// A PCI display controller: a class of `0x03`, which covers VGA, 3D and
/// display controllers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gpu {
    pub address: String,
    pub vendor: Vendor,
    /// The PCI device ID, or `None` when the `device` file cannot be parsed.
    pub device_id: Option<u16>,
    pub driver: Driver,
}

/// The GPUs under a PCI tree, or the fact that the tree cannot be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inventory {
    Unavailable,
    Devices(Vec<Gpu>),
}

/// The result of probing one path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Probe {
    Present,
    Absent,
    Unreadable,
}

/// Whether NVIDIA's proprietary driver is loaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProprietaryNvidia {
    Loaded,
    NotLoaded,
    Unavailable,
}

/// The full report: the GPUs and the state of the proprietary driver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuReport {
    pub inventory: Inventory,
    pub proprietary_nvidia: ProprietaryNvidia,
}

/// Parses a sysfs hexadecimal value such as `0x10de\n`.
pub fn parse_hex_id(text: &str) -> Option<u16> {
    let trimmed = text.trim();
    let digits = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))?;
    u16::from_str_radix(digits, 16).ok()
}

/// Whether a `class` value (for example `0x030000`) is a display controller.
fn is_display_class(text: &str) -> bool {
    text.trim()
        .strip_prefix("0x")
        .is_some_and(|hex| hex.starts_with("03"))
}

/// The GPUs among `devices`, in the order given. A device whose `class` is
/// missing or not a display class is not a GPU and is left out. A device
/// whose vendor cannot be read is kept, with [`Vendor::Unknown`].
pub fn gpus_from_devices(devices: &[PciDevice]) -> Vec<Gpu> {
    devices
        .iter()
        .filter(|device| device.class.as_deref().is_some_and(is_display_class))
        .map(|device| Gpu {
            address: device.address.clone(),
            vendor: device
                .vendor
                .as_deref()
                .and_then(parse_hex_id)
                .map_or(Vendor::Unknown, Vendor::from_pci_id),
            device_id: device.device.as_deref().and_then(parse_hex_id),
            driver: match &device.driver {
                DriverLink::Bound(name) => Driver::from_link_name(name),
                DriverLink::Unbound => Driver::None,
                DriverLink::Unreadable => Driver::Unknown,
            },
        })
        .collect()
}

/// The proprietary driver's state from a probe of [`NVIDIA_PROPRIETARY_VERSION`].
pub fn proprietary_nvidia(probe: Probe) -> ProprietaryNvidia {
    match probe {
        Probe::Present => ProprietaryNvidia::Loaded,
        Probe::Absent => ProprietaryNvidia::NotLoaded,
        Probe::Unreadable => ProprietaryNvidia::Unavailable,
    }
}

impl GpuReport {
    /// Plain-text lines for a status display. An unreadable tree is
    /// `unavailable`; nothing is filled in.
    pub fn describe(&self) -> Vec<String> {
        let mut lines = Vec::new();
        match &self.inventory {
            Inventory::Unavailable => lines.push("GPU: unavailable".to_string()),
            Inventory::Devices(gpus) if gpus.is_empty() => {
                lines.push("GPU: no PCI display controller found".to_string());
            }
            Inventory::Devices(gpus) => {
                for gpu in gpus {
                    lines.push(format!(
                        "{} GPU {} (device {}), driver {}",
                        vendor_name(gpu.vendor),
                        gpu.address,
                        gpu.device_id
                            .map_or_else(|| "unknown".to_string(), |id| format!("0x{id:04x}")),
                        driver_name(&gpu.driver),
                    ));
                }
            }
        }
        lines.push(
            match self.proprietary_nvidia {
                ProprietaryNvidia::Loaded => "NVIDIA proprietary driver: loaded",
                ProprietaryNvidia::NotLoaded => "NVIDIA proprietary driver: not loaded",
                ProprietaryNvidia::Unavailable => "NVIDIA proprietary driver: unavailable",
            }
            .to_string(),
        );
        lines
    }
}

fn vendor_name(vendor: Vendor) -> String {
    match vendor {
        Vendor::Nvidia => "NVIDIA".to_string(),
        Vendor::Amd => "AMD".to_string(),
        Vendor::Intel => "Intel".to_string(),
        Vendor::Other(id) => format!("PCI vendor 0x{id:04x}"),
        Vendor::Unknown => "unknown vendor".to_string(),
    }
}

fn driver_name(driver: &Driver) -> String {
    match driver {
        Driver::Nvidia => "nvidia".to_string(),
        Driver::Nouveau => "nouveau".to_string(),
        Driver::Amdgpu => "amdgpu".to_string(),
        Driver::I915 => "i915".to_string(),
        Driver::Other(name) => name.clone(),
        Driver::None => "none".to_string(),
        Driver::Unknown => "unknown".to_string(),
    }
}

fn read_text(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

fn read_driver_link(path: &Path) -> DriverLink {
    match fs::read_link(path) {
        Ok(target) => match target.file_name().and_then(|name| name.to_str()) {
            Some(name) => DriverLink::Bound(name.to_string()),
            None => DriverLink::Unreadable,
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => DriverLink::Unbound,
        Err(_) => DriverLink::Unreadable,
    }
}

/// Lists the devices under a PCI tree such as [`PCI_DEVICES`], sorted by
/// address. [`Inventory::Unavailable`] when the tree itself cannot be read.
pub fn read_devices(root: &Path) -> Inventory {
    let Ok(entries) = fs::read_dir(root) else {
        return Inventory::Unavailable;
    };
    let mut devices: Vec<PciDevice> = entries
        .flatten()
        .map(|entry| {
            let dir = entry.path();
            PciDevice {
                address: entry.file_name().to_string_lossy().into_owned(),
                class: read_text(&dir.join("class")),
                vendor: read_text(&dir.join("vendor")),
                device: read_text(&dir.join("device")),
                driver: read_driver_link(&dir.join("driver")),
            }
        })
        .collect();
    devices.sort_by(|a, b| a.address.cmp(&b.address));
    Inventory::Devices(gpus_from_devices(&devices))
}

/// Probes one path: present, absent (not found) or unreadable.
pub fn probe(path: &Path) -> Probe {
    match fs::metadata(path) {
        Ok(_) => Probe::Present,
        Err(error) if error.kind() == io::ErrorKind::NotFound => Probe::Absent,
        Err(_) => Probe::Unreadable,
    }
}

/// The report for a PCI tree and a proprietary-driver marker path.
pub fn read_report(pci_root: &Path, proprietary_marker: &Path) -> GpuReport {
    GpuReport {
        inventory: read_devices(pci_root),
        proprietary_nvidia: proprietary_nvidia(probe(proprietary_marker)),
    }
}

/// The report for this machine.
pub fn read_system_report() -> GpuReport {
    read_report(
        Path::new(PCI_DEVICES),
        Path::new(NVIDIA_PROPRIETARY_VERSION),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sysfs text of an RTX 3080 (GA102) display controller.
    fn rtx_3080(driver: DriverLink) -> PciDevice {
        PciDevice {
            address: "0000:01:00.0".to_string(),
            class: Some("0x030000\n".to_string()),
            vendor: Some("0x10de\n".to_string()),
            device: Some("0x2206\n".to_string()),
            driver,
        }
    }

    fn bound(name: &str) -> DriverLink {
        DriverLink::Bound(name.to_string())
    }

    fn only_gpu(device: PciDevice) -> Gpu {
        let mut gpus = gpus_from_devices(&[device]);
        assert_eq!(gpus.len(), 1, "one GPU expected");
        gpus.remove(0)
    }

    #[test]
    fn hex_ids_parse_with_or_without_the_newline_sysfs_adds() {
        assert_eq!(parse_hex_id("0x10de\n"), Some(0x10de));
        assert_eq!(parse_hex_id("  0x2206  "), Some(0x2206));
        assert_eq!(parse_hex_id("0X8086"), Some(0x8086));
    }

    #[test]
    fn values_that_are_not_hex_ids_are_not_parsed() {
        assert_eq!(parse_hex_id(""), None);
        assert_eq!(parse_hex_id("10de"), None);
        assert_eq!(parse_hex_id("0xZZZZ"), None);
        assert_eq!(parse_hex_id("0x12345"), None);
    }

    #[test]
    fn an_nvidia_card_bound_to_the_proprietary_driver_is_detected() {
        let gpu = only_gpu(rtx_3080(bound("nvidia")));
        assert_eq!(gpu.address, "0000:01:00.0");
        assert_eq!(gpu.vendor, Vendor::Nvidia);
        assert_eq!(gpu.device_id, Some(0x2206));
        assert_eq!(gpu.driver, Driver::Nvidia);
    }

    #[test]
    fn the_driver_is_named_from_the_symlink_target() {
        let cases = [
            ("nouveau", Driver::Nouveau),
            ("amdgpu", Driver::Amdgpu),
            ("i915", Driver::I915),
            ("xe", Driver::Other("xe".to_string())),
        ];
        for (name, expected) in cases {
            assert_eq!(only_gpu(rtx_3080(bound(name))).driver, expected, "{name}");
        }
    }

    #[test]
    fn a_display_device_with_no_bound_driver_has_driver_none() {
        assert_eq!(only_gpu(rtx_3080(DriverLink::Unbound)).driver, Driver::None);
    }

    #[test]
    fn an_unreadable_driver_link_is_unknown_not_none() {
        assert_eq!(
            only_gpu(rtx_3080(DriverLink::Unreadable)).driver,
            Driver::Unknown
        );
    }

    #[test]
    fn amd_and_intel_vendors_are_named_and_other_ids_are_kept() {
        let mut device = rtx_3080(bound("amdgpu"));
        device.vendor = Some("0x1002\n".to_string());
        assert_eq!(only_gpu(device).vendor, Vendor::Amd);

        let mut device = rtx_3080(bound("i915"));
        device.vendor = Some("0x8086\n".to_string());
        assert_eq!(only_gpu(device).vendor, Vendor::Intel);

        let mut device = rtx_3080(bound("x"));
        device.vendor = Some("0x1234\n".to_string());
        assert_eq!(only_gpu(device).vendor, Vendor::Other(0x1234));
    }

    #[test]
    fn a_missing_vendor_file_is_unknown_and_the_device_is_still_listed() {
        let mut device = rtx_3080(bound("nvidia"));
        device.vendor = None;
        let gpu = only_gpu(device);
        assert_eq!(gpu.vendor, Vendor::Unknown);
        assert_eq!(gpu.driver, Driver::Nvidia);
    }

    #[test]
    fn an_unparseable_device_file_leaves_the_device_id_unknown() {
        let mut device = rtx_3080(bound("nvidia"));
        device.device = Some("garbage".to_string());
        assert_eq!(only_gpu(device).device_id, None);
    }

    #[test]
    fn display_classes_are_gpus_and_other_classes_are_not() {
        let mut network = rtx_3080(bound("e1000e"));
        network.class = Some("0x020000\n".to_string());
        let mut bridge = rtx_3080(bound("pcieport"));
        bridge.class = Some("0x060400\n".to_string());
        // A 3D controller (class 0x0302) is a display-class device too: it is
        // how NVIDIA compute cards are reported.
        let mut compute = rtx_3080(bound("nvidia"));
        compute.class = Some("0x030200\n".to_string());

        let gpus = gpus_from_devices(&[network, bridge, compute]);
        assert_eq!(gpus.len(), 1);
        assert_eq!(gpus[0].address, "0000:01:00.0");
    }

    #[test]
    fn a_device_with_no_readable_class_is_not_reported_as_a_gpu() {
        let mut device = rtx_3080(bound("nvidia"));
        device.class = None;
        assert!(gpus_from_devices(&[device]).is_empty());
    }

    #[test]
    fn the_proprietary_driver_follows_the_marker_probe() {
        assert_eq!(
            proprietary_nvidia(Probe::Present),
            ProprietaryNvidia::Loaded
        );
        assert_eq!(
            proprietary_nvidia(Probe::Absent),
            ProprietaryNvidia::NotLoaded
        );
        assert_eq!(
            proprietary_nvidia(Probe::Unreadable),
            ProprietaryNvidia::Unavailable
        );
    }

    #[test]
    fn the_description_says_unavailable_when_sysfs_cannot_be_read() {
        let report = GpuReport {
            inventory: Inventory::Unavailable,
            proprietary_nvidia: ProprietaryNvidia::Unavailable,
        };
        let lines = report.describe();
        assert_eq!(lines[0], "GPU: unavailable");
        assert_eq!(lines[1], "NVIDIA proprietary driver: unavailable");
    }

    #[test]
    fn the_description_names_the_card_and_its_driver() {
        let report = GpuReport {
            inventory: Inventory::Devices(vec![only_gpu(rtx_3080(bound("nvidia")))]),
            proprietary_nvidia: ProprietaryNvidia::Loaded,
        };
        assert_eq!(
            report.describe(),
            vec![
                "NVIDIA GPU 0000:01:00.0 (device 0x2206), driver nvidia".to_string(),
                "NVIDIA proprietary driver: loaded".to_string(),
            ]
        );
    }

    #[test]
    fn an_empty_tree_reports_no_gpu_rather_than_unavailable() {
        let report = GpuReport {
            inventory: Inventory::Devices(Vec::new()),
            proprietary_nvidia: ProprietaryNvidia::NotLoaded,
        };
        assert_eq!(report.describe()[0], "GPU: no PCI display controller found");
    }

    #[test]
    fn a_missing_pci_tree_is_unavailable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("no-such-bus");
        assert_eq!(read_devices(&missing), Inventory::Unavailable);
    }

    /// A PCI tree on disk, laid out like sysfs: a directory per device with
    /// `vendor`, `device` and `class` files, and a `driver` symlink when a
    /// driver is bound.
    #[cfg(unix)]
    #[test]
    fn the_adapter_reads_a_fixture_tree_with_driver_symlinks() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();

        // The RTX 3080, bound to the proprietary driver.
        let gpu = root.join("0000:01:00.0");
        fs::create_dir_all(&gpu).unwrap();
        fs::write(gpu.join("vendor"), "0x10de\n").unwrap();
        fs::write(gpu.join("device"), "0x2206\n").unwrap();
        fs::write(gpu.join("class"), "0x030000\n").unwrap();
        symlink("../../../bus/pci/drivers/nvidia", gpu.join("driver")).unwrap();

        // An Intel display controller with no driver bound.
        let intel = root.join("0000:00:02.0");
        fs::create_dir_all(&intel).unwrap();
        fs::write(intel.join("vendor"), "0x8086\n").unwrap();
        fs::write(intel.join("device"), "0x9a49\n").unwrap();
        fs::write(intel.join("class"), "0x030000\n").unwrap();

        // A network card, which is not a GPU.
        let nic = root.join("0000:02:00.0");
        fs::create_dir_all(&nic).unwrap();
        fs::write(nic.join("vendor"), "0x10ec\n").unwrap();
        fs::write(nic.join("class"), "0x020000\n").unwrap();

        let Inventory::Devices(gpus) = read_devices(root) else {
            panic!("the fixture tree must be readable");
        };
        assert_eq!(gpus.len(), 2);
        assert_eq!(gpus[0].address, "0000:00:02.0");
        assert_eq!(gpus[0].vendor, Vendor::Intel);
        assert_eq!(gpus[0].driver, Driver::None);
        assert_eq!(gpus[1].address, "0000:01:00.0");
        assert_eq!(gpus[1].vendor, Vendor::Nvidia);
        assert_eq!(gpus[1].driver, Driver::Nvidia);
    }

    #[test]
    fn the_proprietary_marker_is_present_only_when_the_file_exists() {
        let dir = tempfile::tempdir().expect("tempdir");
        let marker = dir.path().join("version");
        assert_eq!(probe(&marker), Probe::Absent);
        fs::write(&marker, "NVRM version: NVIDIA UNIX x86_64\n").unwrap();
        assert_eq!(probe(&marker), Probe::Present);
    }

    #[test]
    fn a_report_from_fixtures_combines_both_probes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pci = dir.path().join("pci");
        fs::create_dir_all(pci.join("0000:01:00.0")).unwrap();
        let marker = dir.path().join("nvidia-version");
        fs::write(&marker, "loaded\n").unwrap();

        let report = read_report(&pci, &marker);
        assert_eq!(report.proprietary_nvidia, ProprietaryNvidia::Loaded);
        // The directory has no `class` file, so the device is not a GPU.
        assert_eq!(report.inventory, Inventory::Devices(Vec::new()));
    }
}
