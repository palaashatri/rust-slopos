//! Removable volumes (P1 item 6), read from the kernel's mount table.
//!
//! A volume is a block-device mount under `/media/` or `/run/media/`, the
//! places udisks and the desktop mount removable media. Nothing is guessed:
//! when the mount table cannot be read the list is `None`, which the UI shows
//! as unavailable, and when it is read but holds no such mount the list is
//! empty, which is also shown as unavailable.
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    pub mount: PathBuf,
    /// The mount point's last component, as the desktop shows it.
    pub label: String,
    pub fs_type: String,
}

const MOUNT_TABLE: &str = "/proc/self/mountinfo";
const MOUNT_ROOTS: [&str; 2] = ["/media/", "/run/media/"];

/// Volumes mounted now, or `None` when the mount table is unavailable.
pub fn current() -> Option<Vec<Volume>> {
    std::fs::read_to_string(MOUNT_TABLE)
        .ok()
        .map(|text| parse_mountinfo(&text))
}

/// Parse `mountinfo` text (see proc(5)): `id parent maj:min root mount opts
/// [optional...] - fstype source superopts`.
pub fn parse_mountinfo(text: &str) -> Vec<Volume> {
    let mut volumes: Vec<Volume> = Vec::new();
    for line in text.lines() {
        let Some((head, tail)) = line.split_once(" - ") else {
            continue;
        };
        let head: Vec<&str> = head.split(' ').collect();
        let tail: Vec<&str> = tail.split(' ').collect();
        let (Some(mount_field), Some(fs_type), Some(source)) =
            (head.get(4), tail.first(), tail.get(1))
        else {
            continue;
        };
        let mount = PathBuf::from(unescape(mount_field));
        if !source.starts_with("/dev/") || !is_removable_mount(&mount) {
            continue;
        }
        if volumes.iter().any(|volume| volume.mount == mount) {
            continue;
        }
        let label = mount
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        volumes.push(Volume {
            mount,
            label,
            fs_type: (*fs_type).to_owned(),
        });
    }
    volumes.sort_by(|a, b| a.mount.cmp(&b.mount));
    volumes
}

/// True for a mount point strictly inside one of the removable-media roots.
fn is_removable_mount(mount: &std::path::Path) -> bool {
    let text = mount.to_string_lossy();
    MOUNT_ROOTS
        .iter()
        .any(|root| text.starts_with(root) && text.len() > root.len())
}

/// Undo the `\040`-style escapes mountinfo uses for spaces and tabs.
fn unescape(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' && index + 4 <= bytes.len() {
            let digits = &bytes[index + 1..index + 4];
            if digits.iter().all(|digit| (b'0'..=b'7').contains(digit)) {
                let value = digits
                    .iter()
                    .fold(0u32, |acc, &d| acc * 8 + u32::from(d - b'0'));
                out.push(value as u8);
                index += 4;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &str = "\
22 1 8:1 / / rw,relatime shared:1 - ext4 /dev/sda1 rw
30 22 8:17 / /media/alice/USB\\040KEY rw,nosuid shared:9 - vfat /dev/sdb1 rw,fmask=0022
31 22 8:33 / /run/media/alice/CAMERA rw,nosuid - exfat /dev/sdc1 rw
32 22 0:44 / /media/tmpfs rw - tmpfs tmpfs rw
33 22 8:49 / /home/alice/mnt rw - ext4 /dev/sdd1 rw
34 22 8:65 / /run/media/alice/SHARE rw - cifs //server/share rw
35 22 8:17 / /media/alice/USB\\040KEY rw - vfat /dev/sdb1 rw
";

    #[test]
    fn lists_only_block_devices_mounted_under_the_media_roots() {
        let volumes = parse_mountinfo(TABLE);
        let mounts: Vec<&str> = volumes.iter().map(|v| v.mount.to_str().unwrap()).collect();
        assert_eq!(
            mounts,
            ["/media/alice/USB KEY", "/run/media/alice/CAMERA"],
            "root, /home, a network share and a tmpfs are not removable volumes"
        );
    }

    #[test]
    fn escapes_labels_and_filesystem_types_are_read() {
        let volumes = parse_mountinfo(TABLE);
        assert_eq!(volumes[0].label, "USB KEY");
        assert_eq!(volumes[0].fs_type, "vfat");
        assert_eq!(volumes[1].fs_type, "exfat");
    }

    #[test]
    fn a_table_without_removable_media_gives_no_volumes() {
        assert!(parse_mountinfo("22 1 8:1 / / rw - ext4 /dev/sda1 rw\n").is_empty());
        assert!(parse_mountinfo("").is_empty());
        assert!(parse_mountinfo("garbage line without separator\n").is_empty());
    }

    #[test]
    fn a_root_directory_alone_is_not_a_volume() {
        assert!(parse_mountinfo("30 22 8:17 / /media/ rw - vfat /dev/sdb1 rw\n").is_empty());
    }

    #[test]
    fn unescape_handles_octal_and_leaves_malformed_escapes_alone() {
        assert_eq!(unescape("a\\040b"), "a b");
        assert_eq!(unescape("tab\\011x"), "tab\tx");
        assert_eq!(unescape("trail\\04"), "trail\\04");
        assert_eq!(unescape("bad\\9999"), "bad\\9999");
        assert_eq!(unescape("plain"), "plain");
    }
}
