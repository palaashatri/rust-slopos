//! Freedesktop icon-theme lookup (Icon Theme Specification 0.13).
//!
//! [`IconLookup`] reads each theme's `index.theme` once, follows `Inherits`
//! down to `hicolor`, and picks the directory whose size best fits the
//! request, preferring an exact match, then scalable SVG, then the nearest
//! bitmap size. Names that are absolute paths are used as they are.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::image::Image;

/// The theme SLOPOS installs; used when `SLOPOS_ICON_THEME` is unset.
pub const DEFAULT_THEME: &str = "SLOPOS-Platinum";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DirectoryType {
    Fixed,
    Scalable,
    Threshold,
}

#[derive(Debug, Clone)]
struct Directory {
    path: String,
    size: u32,
    scale: u32,
    min: u32,
    max: u32,
    threshold: u32,
    kind: DirectoryType,
}

impl Directory {
    fn matches(&self, size: u32, scale: u32) -> bool {
        if self.scale != scale {
            return false;
        }
        match self.kind {
            DirectoryType::Fixed => self.size == size,
            DirectoryType::Scalable => (self.min..=self.max).contains(&size),
            DirectoryType::Threshold => {
                size + self.threshold >= self.size && size <= self.size + self.threshold
            }
        }
    }

    fn distance(&self, size: u32, scale: u32) -> u32 {
        let wanted = size * scale;
        let (low, high) = match self.kind {
            DirectoryType::Fixed => (self.size, self.size),
            DirectoryType::Scalable => (self.min, self.max),
            DirectoryType::Threshold => (
                self.size.saturating_sub(self.threshold),
                self.size + self.threshold,
            ),
        };
        let (low, high) = (low * self.scale, high * self.scale);
        if wanted < low {
            low - wanted
        } else {
            wanted.saturating_sub(high)
        }
    }
}

#[derive(Debug, Clone, Default)]
struct Theme {
    /// Every base directory that holds this theme (`…/icons/<name>`).
    roots: Vec<PathBuf>,
    directories: Vec<Directory>,
    inherits: Vec<String>,
}

/// Icon lookup over a fixed list of base directories.
#[derive(Debug, Clone)]
pub struct IconLookup {
    base_dirs: Vec<PathBuf>,
    theme: String,
    themes: HashMap<String, Option<Theme>>,
}

impl IconLookup {
    /// Search the XDG icon directories with `SLOPOS_ICON_THEME`, else the
    /// SLOPOS theme.
    pub fn from_environment() -> Self {
        let mut base_dirs = Vec::new();
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            base_dirs.push(home.join(".icons"));
        }
        let data_home = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            });
        if let Some(data_home) = data_home {
            base_dirs.push(data_home.join("icons"));
        }
        let data_dirs = std::env::var("XDG_DATA_DIRS")
            .ok()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "/usr/local/share:/usr/share".to_owned());
        for dir in std::env::split_paths(&data_dirs) {
            base_dirs.push(dir.join("icons"));
        }
        base_dirs.push(PathBuf::from("/usr/share/pixmaps"));
        let theme = std::env::var("SLOPOS_ICON_THEME")
            .ok()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_THEME.to_owned());
        Self::new(base_dirs, &theme)
    }

    pub fn new(base_dirs: Vec<PathBuf>, theme: &str) -> Self {
        Self {
            base_dirs,
            theme: theme.to_owned(),
            themes: HashMap::new(),
        }
    }

    /// Path of the best icon for `name` at `size` logical pixels and `scale`.
    pub fn find(&mut self, name: &str, size: u32, scale: u32) -> Option<PathBuf> {
        if name.is_empty() {
            return None;
        }
        let as_path = Path::new(name);
        if as_path.is_absolute() {
            return as_path.is_file().then(|| as_path.to_path_buf());
        }
        let mut visited = HashSet::new();
        let mut queue = vec![self.theme.clone()];
        let mut index = 0;
        while index < queue.len() {
            let theme_name = queue[index].clone();
            index += 1;
            if !visited.insert(theme_name.clone()) {
                continue;
            }
            if let Some(theme) = self.theme(&theme_name) {
                if let Some(path) = lookup_in(&theme, name, size, scale) {
                    return Some(path);
                }
                queue.extend(theme.inherits.iter().cloned());
            }
        }
        if !visited.contains("hicolor") {
            if let Some(path) = self
                .theme("hicolor")
                .and_then(|theme| lookup_in(&theme, name, size, scale))
            {
                return Some(path);
            }
        }
        // Unthemed icons directly in a base directory (/usr/share/pixmaps).
        self.base_dirs.iter().find_map(|dir| {
            ["svg", "png"]
                .iter()
                .map(|extension| dir.join(format!("{name}.{extension}")))
                .find(|path| path.is_file())
        })
    }

    /// Load the icon for `name` as an image of `size × scale` device pixels.
    /// SVGs are rasterised at that size; PNGs are scaled to it.
    pub fn load(&mut self, name: &str, size: u32, scale: u32) -> Option<Image> {
        let path = self.find(name, size, scale)?;
        let device = size * scale;
        let image = Image::load(&path, device).ok()?;
        if image.width() == device && image.height() == device {
            Some(image)
        } else {
            let integral =
                device.is_multiple_of(image.width()) && device.is_multiple_of(image.height());
            image.scaled_with(device, device, integral).ok()
        }
    }

    fn theme(&mut self, name: &str) -> Option<Theme> {
        if !self.themes.contains_key(name) {
            let theme = load_theme(&self.base_dirs, name);
            self.themes.insert(name.to_owned(), theme);
        }
        self.themes.get(name).cloned().flatten()
    }
}

fn lookup_in(theme: &Theme, name: &str, size: u32, scale: u32) -> Option<PathBuf> {
    let existing = |directory: &Directory| -> Option<PathBuf> {
        theme.roots.iter().find_map(|root| {
            ["svg", "png"]
                .iter()
                .map(|extension| {
                    root.join(&directory.path)
                        .join(format!("{name}.{extension}"))
                })
                .find(|path| path.is_file())
        })
    };
    // An exact size match first, scalable before bitmap.
    let mut exact: Vec<&Directory> = theme
        .directories
        .iter()
        .filter(|directory| directory.matches(size, scale))
        .collect();
    exact.sort_by_key(|directory| directory.kind != DirectoryType::Scalable);
    for directory in exact {
        if let Some(path) = existing(directory) {
            return Some(path);
        }
    }
    // Then the closest size, preferring larger (downscaling looks better).
    let mut closest: Vec<&Directory> = theme.directories.iter().collect();
    closest.sort_by_key(|directory| {
        (
            directory.distance(size, scale),
            directory.size * directory.scale < size * scale,
        )
    });
    closest.into_iter().find_map(existing)
}

fn load_theme(base_dirs: &[PathBuf], name: &str) -> Option<Theme> {
    let roots: Vec<PathBuf> = base_dirs
        .iter()
        .map(|dir| dir.join(name))
        .filter(|root| root.is_dir())
        .collect();
    let index = roots
        .iter()
        .find_map(|root| fs::read_to_string(root.join("index.theme")).ok())?;
    let mut theme = parse_index(&index);
    theme.roots = roots;
    Some(theme)
}

fn parse_index(index: &str) -> Theme {
    let mut sections: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut current = String::new();
    for line in index.lines().map(str::trim) {
        if let Some(section) = line
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        {
            current = section.to_owned();
        } else if let Some((key, value)) = line.split_once('=') {
            sections
                .entry(current.clone())
                .or_default()
                .insert(key.trim().to_owned(), value.trim().to_owned());
        }
    }
    let header = sections.get("Icon Theme").cloned().unwrap_or_default();
    let list = |key: &str| -> Vec<String> {
        header
            .get(key)
            .map(|value| {
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut names = list("Directories");
    names.extend(list("ScaledDirectories"));
    let directories = names
        .into_iter()
        .filter_map(|path| {
            let keys = sections.get(&path)?;
            let number = |key: &str| keys.get(key).and_then(|value| value.parse::<u32>().ok());
            let size = number("Size")?;
            Some(Directory {
                kind: match keys.get("Type").map(String::as_str) {
                    Some("Fixed") => DirectoryType::Fixed,
                    Some("Scalable") => DirectoryType::Scalable,
                    _ => DirectoryType::Threshold,
                },
                scale: number("Scale").unwrap_or(1).max(1),
                min: number("MinSize").unwrap_or(size),
                max: number("MaxSize").unwrap_or(size),
                threshold: number("Threshold").unwrap_or(2),
                size,
                path,
            })
        })
        .collect();
    Theme {
        roots: Vec::new(),
        directories,
        inherits: list("Inherits"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="#000"/></svg>"##;

    fn tree() -> (PathBuf, IconLookup) {
        let root = std::env::temp_dir().join(format!(
            "slopos-icons-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&root);
        let write = |path: &str, contents: &[u8]| {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        };
        write(
            "icons/Child/index.theme",
            b"[Icon Theme]\nName=Child\nInherits=Parent\nDirectories=scalable/apps\n\n[scalable/apps]\nSize=48\nMinSize=16\nMaxSize=256\nType=Scalable\n",
        );
        write("icons/Child/scalable/apps/editor.svg", SVG);
        write(
            "icons/Parent/index.theme",
            b"[Icon Theme]\nName=Parent\nDirectories=16x16/apps,48x48/apps\n\n[16x16/apps]\nSize=16\nType=Fixed\n\n[48x48/apps]\nSize=48\nType=Fixed\n",
        );
        write("icons/Parent/16x16/apps/terminal.svg", SVG);
        write("icons/Parent/48x48/apps/terminal.svg", SVG);
        write(
            "icons/hicolor/index.theme",
            b"[Icon Theme]\nName=Hicolor\nDirectories=32x32/apps\n\n[32x32/apps]\nSize=32\nType=Threshold\n",
        );
        write("icons/hicolor/32x32/apps/calculator.svg", SVG);
        write("pixmaps/legacy.svg", SVG);
        let lookup = IconLookup::new(vec![root.join("icons"), root.join("pixmaps")], "Child");
        (root, lookup)
    }

    #[test]
    fn themes_inherit_and_sizes_are_matched() {
        let (root, mut lookup) = tree();
        let find = |lookup: &mut IconLookup, name, size| {
            lookup
                .find(name, size, 1)
                .map(|path| path.strip_prefix(&root).unwrap().to_path_buf())
        };
        assert_eq!(
            find(&mut lookup, "editor", 32),
            Some("icons/Child/scalable/apps/editor.svg".into())
        );
        assert_eq!(
            find(&mut lookup, "terminal", 48),
            Some("icons/Parent/48x48/apps/terminal.svg".into())
        );
        assert_eq!(
            find(&mut lookup, "terminal", 40),
            Some("icons/Parent/48x48/apps/terminal.svg".into())
        );
        assert_eq!(
            find(&mut lookup, "calculator", 48),
            Some("icons/hicolor/32x32/apps/calculator.svg".into())
        );
        assert_eq!(
            find(&mut lookup, "legacy", 48),
            Some("pixmaps/legacy.svg".into())
        );
        assert_eq!(find(&mut lookup, "missing", 48), None);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn icons_load_at_device_size() {
        let (root, mut lookup) = tree();
        let icon = lookup.load("terminal", 24, 2).unwrap();
        assert_eq!((icon.width(), icon.height()), (48, 48));
        let _ = fs::remove_dir_all(root);
    }
}
