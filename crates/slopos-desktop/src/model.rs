//! What the desktop shows and where: Home, mounted volumes, the items in
//! `~/Desktop` and the Trash, placed on a grid of cells that fills from the
//! top-right corner downwards (the Classic place for disks), with
//! positions the user dragged to remembered by item.
//!
//! Everything here is plain data so it can be tested without X.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// One grid cell, in logical pixels.
pub const CELL: (u32, u32) = (96, 94);
/// Space kept clear for the top bar and the Dock.
pub const MARGIN_TOP: u32 = 34;
pub const MARGIN_BOTTOM: u32 = 92;
/// Desktop folders bigger than this show their first items only.
pub const MAX_ITEMS: usize = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Home,
    Volume,
    Folder,
    File,
    Trash,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub label: String,
    pub path: PathBuf,
    pub kind: Kind,
    /// Icon names to try, best first.
    pub icons: Vec<String>,
    /// Stable identity for remembered positions.
    pub key: String,
}

/// Mount points of removable volumes from `/proc/self/mountinfo`: those
/// under `/media/` or `/run/media/`.
pub fn parse_mounts(mountinfo: &str) -> Vec<PathBuf> {
    let mut mounts: Vec<PathBuf> = mountinfo
        .lines()
        .filter_map(|line| line.split(' ').nth(4))
        .map(unescape_mount)
        .filter(|path| path.starts_with("/media/") || path.starts_with("/run/media/"))
        .map(PathBuf::from)
        .collect();
    mounts.sort();
    mounts.dedup();
    mounts
}

/// mountinfo writes space, tab, newline and backslash as `\ooo`.
fn unescape_mount(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let octal = bytes.get(i + 1..i + 4);
        if let (b'\\', Some(digits)) = (bytes[i], octal) {
            if digits.iter().all(|b| (b'0'..=b'7').contains(b)) {
                let value = digits
                    .iter()
                    .fold(0u32, |value, digit| value * 8 + u32::from(digit - b'0'));
                if let Ok(value) = u8::try_from(value) {
                    out.push(value);
                    i += 4;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Icon names for a file name, by its MIME type.
pub fn file_icons(mime: &str) -> Vec<String> {
    let mut icons = vec![mime.replace('/', "-")];
    if let Some((media, _)) = mime.split_once('/') {
        icons.push(format!("{media}-x-generic"));
    }
    icons.push("text-x-generic".into());
    icons
}

/// Everything the desktop shows, in default order.
pub fn items(
    home: &Path,
    desktop: &Path,
    mounts: &[PathBuf],
    entries: &[(String, bool)],
    trash_full: bool,
) -> Vec<Item> {
    let mut items = vec![Item {
        label: "Home".into(),
        path: home.to_owned(),
        kind: Kind::Home,
        icons: vec!["user-home".into(), "folder".into()],
        key: "home".into(),
    }];
    for mount in mounts {
        let label = mount
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| mount.display().to_string());
        items.push(Item {
            label,
            path: mount.clone(),
            kind: Kind::Volume,
            icons: vec![
                "drive-removable-media".into(),
                "drive-harddisk".into(),
                "folder".into(),
            ],
            key: format!("volume:{}", mount.display()),
        });
    }
    let mut entries: Vec<&(String, bool)> = entries
        .iter()
        .filter(|(name, _)| !name.starts_with('.') && !name.is_empty())
        .collect();
    entries.sort_by_key(|entry| entry.0.to_lowercase());
    for (name, is_dir) in entries.into_iter().take(MAX_ITEMS) {
        let path = desktop.join(name);
        let (kind, icons) = if *is_dir {
            (Kind::Folder, vec!["folder".to_owned()])
        } else {
            (
                Kind::File,
                file_icons(&slopos_core::mime::guess_for_name(name)),
            )
        };
        items.push(Item {
            label: name.clone(),
            path,
            kind,
            icons,
            key: format!("file:{name}"),
        });
    }
    items.push(Item {
        label: "Trash".into(),
        path: PathBuf::new(),
        kind: Kind::Trash,
        icons: if trash_full {
            vec!["user-trash-full".into(), "user-trash".into()]
        } else {
            vec!["user-trash".into()]
        },
        key: "trash".into(),
    });
    items
}

/// The grid that fits a screen of `size` logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grid {
    pub columns: u32,
    pub rows: u32,
    pub width: u32,
}

impl Grid {
    pub fn new(size: (u32, u32)) -> Self {
        let usable = size.1.saturating_sub(MARGIN_TOP + MARGIN_BOTTOM);
        Self {
            columns: (size.0 / CELL.0).max(1),
            rows: (usable / CELL.1).max(1),
            width: size.0,
        }
    }

    /// Cells in fill order: right column first, top to bottom.
    pub fn order(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        (0..self.columns).flat_map(move |column| (0..self.rows).map(move |row| (column, row)))
    }

    /// The cell's rectangle (x, y, width, height); column 0 is rightmost.
    pub fn rect(&self, (column, row): (u32, u32)) -> (i32, i32, u32, u32) {
        let right = self.width as i32 - 8;
        (
            right - (column as i32 + 1) * CELL.0 as i32,
            (MARGIN_TOP + row * CELL.1) as i32,
            CELL.0,
            CELL.1,
        )
    }

    /// The cell under a point, if it is on the grid.
    pub fn cell_at(&self, x: i32, y: i32) -> Option<(u32, u32)> {
        let right = self.width as i32 - 8;
        if x >= right || y < MARGIN_TOP as i32 {
            return None;
        }
        let column = ((right - x - 1) / CELL.0 as i32) as u32;
        let row = ((y - MARGIN_TOP as i32) / CELL.1 as i32) as u32;
        (column < self.columns && row < self.rows).then_some((column, row))
    }
}

/// A cell for every item: remembered ones where they were (if still on
/// the grid and free), the rest in the next free cells. Items that don't
/// fit share the last cell.
pub fn arrange(items: &[Item], saved: &HashMap<String, (u32, u32)>, grid: Grid) -> Vec<(u32, u32)> {
    let mut taken = std::collections::HashSet::new();
    let mut cells: Vec<Option<(u32, u32)>> = items
        .iter()
        .map(|item| {
            saved
                .get(&item.key)
                .copied()
                .filter(|&(c, r)| c < grid.columns && r < grid.rows)
                .filter(|cell| taken.insert(*cell))
        })
        .collect();
    let mut free = grid.order().filter(|cell| !taken.contains(cell));
    let last = (grid.columns - 1, grid.rows - 1);
    for cell in &mut cells {
        if cell.is_none() {
            *cell = Some(free.next().unwrap_or(last));
        }
    }
    cells.into_iter().map(|cell| cell.unwrap_or(last)).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

/// The nearest item from `from` in a direction on the grid.
pub fn neighbour(cells: &[(u32, u32)], from: usize, direction: Direction) -> Option<usize> {
    let (column, row) = (cells[from].0 as i64, cells[from].1 as i64);
    cells
        .iter()
        .enumerate()
        .filter(|&(index, _)| index != from)
        .filter_map(|(index, &(c, r))| {
            let (dc, dr) = (c as i64 - column, r as i64 - row);
            // Column 0 is on the right, so Left means a larger column.
            let (ahead, across) = match direction {
                Direction::Down => (dr, dc),
                Direction::Up => (-dr, dc),
                Direction::Left => (dc, dr),
                Direction::Right => (-dc, dr),
            };
            (ahead > 0).then_some((ahead * ahead * 4 + across * across, index))
        })
        .min()
        .map(|(_, index)| index)
}

/// The next item after `after` whose label starts with `prefix`
/// (case-insensitive), wrapping around; typing a letter again cycles.
pub fn next_with_prefix(items: &[Item], after: Option<usize>, prefix: &str) -> Option<usize> {
    let prefix = prefix.to_lowercase();
    let start = after.map_or(0, |i| i + 1);
    (0..items.len())
        .map(|k| (start + k) % items.len())
        .find(|&i| items[i].label.to_lowercase().starts_with(&prefix))
}

/// A free name for a new folder in `dir`: "untitled folder", then
/// "untitled folder 2", …
pub fn new_folder_name(exists: impl Fn(&str) -> bool) -> String {
    let base = "untitled folder";
    if !exists(base) {
        return base.into();
    }
    (2..)
        .map(|n| format!("{base} {n}"))
        .find(|name| !exists(name))
        .expect("an unused name")
}

/// Remembered positions: one `column row key` line each.
pub fn load_positions(text: &str) -> HashMap<String, (u32, u32)> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, ' ');
            let column = parts.next()?.parse().ok()?;
            let row = parts.next()?.parse().ok()?;
            let key = parts.next()?.trim();
            (!key.is_empty() && column < 1000 && row < 1000)
                .then(|| (key.to_owned(), (column, row)))
        })
        .collect()
}

pub fn save_positions(positions: &HashMap<String, (u32, u32)>) -> String {
    let mut lines: Vec<String> = positions
        .iter()
        .filter(|(key, _)| !key.contains('\n'))
        .map(|(key, (column, row))| format!("{column} {row} {key}"))
        .collect();
    lines.sort();
    lines.join("\n") + "\n"
}

/// Cut a label to fit `width` logical pixels on at most two lines.
pub fn label_lines(label: &str, fits: impl Fn(&str) -> bool) -> Vec<String> {
    if fits(label) {
        return vec![label.to_owned()];
    }
    let chars: Vec<char> = label.chars().collect();
    // The longest first line that fits, broken at a space when possible.
    let mut split = chars.len();
    while split > 1 && !fits(&chars[..split].iter().collect::<String>()) {
        split -= 1;
    }
    if let Some(space) = chars[..split].iter().rposition(|&c| c == ' ') {
        if space > 0 {
            split = space + 1;
        }
    }
    let first: String = chars[..split]
        .iter()
        .collect::<String>()
        .trim_end()
        .to_owned();
    let mut rest: String = chars[split..].iter().collect();
    if !fits(&rest) {
        while !rest.is_empty() && !fits(&format!("{rest}…")) {
            rest.pop();
        }
        rest.push('…');
    }
    vec![first, rest]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Item> {
        items(
            Path::new("/home/ada"),
            Path::new("/home/ada/Desktop"),
            &[PathBuf::from("/media/ada/USB STICK")],
            &[
                ("notes.txt".into(), false),
                (".hidden".into(), false),
                ("Projects".into(), true),
            ],
            true,
        )
    }

    #[test]
    fn items_come_from_home_volumes_desktop_and_trash() {
        let items = sample();
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(
            labels,
            ["Home", "USB STICK", "notes.txt", "Projects", "Trash"]
        );
        assert_eq!(items[3].kind, Kind::Folder);
        assert_eq!(items[3].path, Path::new("/home/ada/Desktop/Projects"));
        assert_eq!(items[2].icons[0], "text-plain");
        assert_eq!(items[4].icons[0], "user-trash-full");
        let info = "36 35 98:0 / /mnt rw - ext4 /dev/sda1 rw\n\
                    37 35 8:17 / /media/ada/USB\\040STICK rw - vfat /dev/sdb1 rw\n\
                    38 35 8:18 / /run/media/ada/CAM rw - vfat /dev/sdc1 rw\n\
                    39 35 0:5 / /media/ada/USB\\040STICK rw - vfat /dev/sdb1 rw\n";
        assert_eq!(
            parse_mounts(info),
            vec![
                PathBuf::from("/media/ada/USB STICK"),
                PathBuf::from("/run/media/ada/CAM")
            ]
        );
        assert!(parse_mounts("").is_empty());
        assert_eq!(unescape_mount("a\\134b\\0"), "a\\b\\0");
    }

    #[test]
    fn the_grid_fills_from_the_top_right_and_keeps_saved_places() {
        let grid = Grid::new((1280, 800));
        assert_eq!((grid.columns, grid.rows), (13, 7));
        assert_eq!(grid.rect((0, 0)), (1176, 34, 96, 94));
        assert_eq!(grid.cell_at(1200, 40), Some((0, 0)));
        assert_eq!(grid.cell_at(1100, 130), Some((1, 1)));
        assert_eq!(grid.cell_at(1279, 40), None);
        assert_eq!(grid.cell_at(1200, 10), None);
        assert_eq!(grid.cell_at(1200, 790), None);
        let items = sample();
        let mut saved = HashMap::new();
        saved.insert("trash".to_owned(), (0, 6));
        saved.insert("file:notes.txt".to_owned(), (0, 0)); // Home goes on.
        saved.insert("home".to_owned(), (99, 0)); // Off the grid.
        let cells = arrange(&items, &saved, grid);
        assert_eq!(cells[4], (0, 6));
        assert_eq!(cells[2], (0, 0));
        assert_eq!(cells[0], (0, 1));
        assert_eq!(cells[1], (0, 2));
        assert_eq!(cells[3], (0, 3));
        // A tiny screen still places everything.
        let tiny = Grid::new((50, 50));
        assert!(arrange(&items, &HashMap::new(), tiny)
            .iter()
            .all(|&cell| cell == (0, 0)));
        let text = save_positions(&saved);
        assert_eq!(load_positions(&text), saved);
        assert!(load_positions("x y z\n1 2\n5 5 \n").is_empty());
    }

    #[test]
    fn arrows_type_ahead_and_new_names() {
        let cells = vec![(0, 0), (0, 1), (1, 0), (1, 1), (0, 3)];
        assert_eq!(neighbour(&cells, 0, Direction::Down), Some(1));
        assert_eq!(neighbour(&cells, 1, Direction::Down), Some(4));
        assert_eq!(neighbour(&cells, 0, Direction::Left), Some(2));
        assert_eq!(neighbour(&cells, 2, Direction::Right), Some(0));
        assert_eq!(neighbour(&cells, 0, Direction::Up), None);
        assert_eq!(neighbour(&cells, 0, Direction::Right), None);
        let items = sample();
        assert_eq!(next_with_prefix(&items, None, "pro"), Some(3));
        assert_eq!(next_with_prefix(&items, Some(3), "p"), Some(3));
        assert_eq!(next_with_prefix(&items, Some(0), "h"), Some(0));
        assert_eq!(next_with_prefix(&items, None, "zzz"), None);
        assert_eq!(next_with_prefix(&[], None, "a"), None);
        let existing = ["untitled folder", "untitled folder 2"];
        assert_eq!(
            new_folder_name(|name| existing.contains(&name)),
            "untitled folder 3"
        );
        assert_eq!(new_folder_name(|_| false), "untitled folder");
        let fits = |text: &str| text.chars().count() <= 10;
        assert_eq!(label_lines("short", fits), vec!["short"]);
        assert_eq!(
            label_lines("Holiday photos 2026", fits),
            vec!["Holiday", "photos 20…"]
        );
        assert_eq!(
            label_lines("abcdefghijklmn", fits),
            vec!["abcdefghij", "klmn"]
        );
    }
}
