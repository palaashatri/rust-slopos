//! What Disks shows, from the storage provider's snapshot. Nothing here
//! invents a value: a field the provider did not report is left out, and a
//! provider failure becomes the text the window shows.

use slopos_core::capability::Availability;
use slopos_services::storage::{StorageBlock, StorageDrive, StorageError, StorageSnapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Drive,
    Volume,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub kind: Kind,
    /// "Vendor Model" for a drive, the device path for a volume.
    pub name: String,
    pub size: u64,
    /// One line of facts about the row.
    pub detail: String,
}

impl Row {
    /// The row as the list and the accessibility tree show it.
    pub fn label(&self) -> String {
        let kind = match self.kind {
            Kind::Drive => "Drive",
            Kind::Volume => "Volume",
        };
        format!("{kind}: {}, {}", self.name, human_size(self.size))
    }
}

/// Drives first, then the block devices the system does not hide.
pub fn rows(snapshot: &StorageSnapshot) -> Vec<Row> {
    let mut rows: Vec<Row> = snapshot.drives.iter().map(drive_row).collect();
    rows.extend(
        snapshot
            .blocks
            .iter()
            .filter(|block| !block.hint_ignore)
            .map(block_row),
    );
    rows
}

fn drive_row(drive: &StorageDrive) -> Row {
    let name = [drive.vendor.as_deref(), drive.model.as_deref()]
        .into_iter()
        .flatten()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let mut facts = Vec::new();
    facts.push(if drive.removable {
        "removable".to_owned()
    } else {
        "fixed".to_owned()
    });
    if drive.ejectable {
        facts.push("can be ejected".to_owned());
    }
    if !drive.media_available {
        facts.push("no media".to_owned());
    }
    Row {
        kind: Kind::Drive,
        name: if name.is_empty() {
            "Unnamed drive".to_owned()
        } else {
            name
        },
        size: drive.size,
        detail: facts.join(", "),
    }
}

fn block_row(block: &StorageBlock) -> Row {
    let mut facts = Vec::new();
    match &block.filesystem_type {
        Some(kind) if !kind.is_empty() => facts.push(format!("filesystem {kind}")),
        _ => facts.push("no filesystem".to_owned()),
    }
    if let Some(label) = block.label.as_deref().filter(|label| !label.is_empty()) {
        facts.push(format!("label {label}"));
    }
    match block
        .filesystem
        .as_ref()
        .map(|fs| fs.mount_points.as_slice())
    {
        Some(points) if !points.is_empty() => {
            let points: Vec<String> = points.iter().map(|p| p.display().to_string()).collect();
            facts.push(format!("mounted at {}", points.join(", ")));
        }
        _ => facts.push("not mounted".to_owned()),
    }
    Row {
        kind: Kind::Volume,
        name: block.device.display().to_string(),
        size: block.size,
        detail: facts.join(", "),
    }
}

/// Why no rows can be shown, when the provider answered with a state other
/// than "available". `None` means the snapshot is usable.
pub fn availability_text(availability: Availability) -> Option<String> {
    match availability {
        Availability::Available => None,
        Availability::Loading => Some("Still reading disks.".to_owned()),
        Availability::UnsupportedHardware => {
            Some("Unavailable: this system reports no storage UDisks2 can manage.".to_owned())
        }
        Availability::MissingService => Some(MISSING_SERVICE.to_owned()),
        Availability::PermissionDenied => Some(PERMISSION_DENIED.to_owned()),
        Availability::Disconnected => Some(DISCONNECTED.to_owned()),
        Availability::Failed => Some("Unavailable: UDisks2 reported a failure.".to_owned()),
    }
}

const MISSING_SERVICE: &str =
    "Unavailable: UDisks2 is not running. Install or start udisks2 to see disks.";
const PERMISSION_DENIED: &str = "Permission denied: UDisks2 refused to list disks for this user.";
const DISCONNECTED: &str = "Unavailable: the system D-Bus connection was lost.";

/// The text for a failed read.
pub fn error_text(error: &StorageError) -> String {
    match error.availability() {
        Availability::MissingService => MISSING_SERVICE.to_owned(),
        Availability::PermissionDenied => PERMISSION_DENIED.to_owned(),
        Availability::Disconnected => DISCONNECTED.to_owned(),
        _ if matches!(error, StorageError::Timeout) => {
            "Unavailable: UDisks2 did not answer in time.".to_owned()
        }
        _ => format!("Unavailable: {error}"),
    }
}

/// Binary units with one decimal place.
pub fn human_size(bytes: u64) -> String {
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
mod tests {
    use super::*;
    use slopos_core::ipc::IpcObjectPath;
    use std::path::PathBuf;

    fn drive(vendor: Option<&str>, model: Option<&str>, removable: bool) -> StorageDrive {
        StorageDrive {
            path: IpcObjectPath::new("/org/freedesktop/UDisks2/drives/one").unwrap(),
            id: None,
            vendor: vendor.map(str::to_owned),
            model: model.map(str::to_owned),
            size: 500 * 1024 * 1024 * 1024,
            media_available: true,
            media_removable: removable,
            removable,
            ejectable: removable,
            can_power_off: false,
        }
    }

    fn block(device: &str, fs: Option<&str>, mounts: Vec<&str>, hidden: bool) -> StorageBlock {
        StorageBlock {
            path: IpcObjectPath::new("/org/freedesktop/UDisks2/block_devices/sda1").unwrap(),
            id: None,
            device: PathBuf::from(device),
            preferred_device: PathBuf::from(device),
            size: 1024 * 1024,
            label: Some("home".to_owned()),
            filesystem_type: fs.map(str::to_owned),
            uuid: None,
            usage: None,
            hint_ignore: hidden,
            hint_system: false,
            drive_path: None,
            filesystem: if mounts.is_empty() && fs.is_none() {
                None
            } else {
                Some(slopos_services::storage::StorageFilesystem {
                    mount_points: mounts.into_iter().map(PathBuf::from).collect(),
                })
            },
        }
    }

    fn snapshot(drives: Vec<StorageDrive>, blocks: Vec<StorageBlock>) -> StorageSnapshot {
        StorageSnapshot {
            revision: 1,
            availability: Availability::Available,
            blocks,
            drives,
        }
    }

    #[test]
    fn drives_are_named_by_vendor_and_model_and_described() {
        let rows = rows(&snapshot(
            vec![drive(Some("ACME"), Some("Disk 1"), true)],
            Vec::new(),
        ));
        assert_eq!(rows[0].kind, Kind::Drive);
        assert_eq!(rows[0].name, "ACME Disk 1");
        assert_eq!(rows[0].detail, "removable, can be ejected");
        assert_eq!(rows[0].label(), "Drive: ACME Disk 1, 500.0 GiB");
    }

    #[test]
    fn a_drive_without_a_name_says_so() {
        let rows = rows(&snapshot(vec![drive(None, Some("  "), false)], Vec::new()));
        assert_eq!(rows[0].name, "Unnamed drive");
        assert_eq!(rows[0].detail, "fixed");
    }

    #[test]
    fn volumes_show_their_filesystem_label_and_mounts() {
        let rows = rows(&snapshot(
            Vec::new(),
            vec![
                block("/dev/sda1", Some("ext4"), vec!["/home"], false),
                block("/dev/sdb1", None, Vec::new(), false),
            ],
        ));
        assert_eq!(rows[0].name, "/dev/sda1");
        assert_eq!(
            rows[0].detail,
            "filesystem ext4, label home, mounted at /home"
        );
        assert_eq!(rows[1].detail, "no filesystem, label home, not mounted");
    }

    #[test]
    fn volumes_the_system_hides_are_left_out() {
        let rows = rows(&snapshot(
            Vec::new(),
            vec![block("/dev/sda2", Some("swap"), Vec::new(), true)],
        ));
        assert!(rows.is_empty());
    }

    #[test]
    fn a_missing_service_and_a_denied_read_are_text_not_empty_lists() {
        assert_eq!(
            availability_text(Availability::MissingService).as_deref(),
            Some(MISSING_SERVICE)
        );
        assert_eq!(
            availability_text(Availability::PermissionDenied).as_deref(),
            Some(PERMISSION_DENIED)
        );
        assert_eq!(availability_text(Availability::Available), None);
        let denied = StorageError::PermissionDenied(zbus::Error::Failure("no".to_owned()));
        assert_eq!(error_text(&denied), PERMISSION_DENIED);
        assert_eq!(
            error_text(&StorageError::Timeout),
            "Unavailable: UDisks2 did not answer in time."
        );
        let worker = StorageError::Worker("no threads".to_owned());
        assert!(error_text(&worker).contains("no threads"));
    }

    #[test]
    fn sizes_use_binary_units() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1536), "1.5 KiB");
    }
}
