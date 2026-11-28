//! The font catalogue: faces from fontconfig (`fc-list`), who owns each
//! face, and installing or removing the user's own fonts.
//!
//! Only the user's font folder is ever written or deleted. A system font is
//! listed as system-owned and is never touched. Refreshing the cache runs
//! `fc-cache` on the user's folder only.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

/// `fc-list` output: file, family and style, tab-separated, one face per line.
pub const FIELD_FORMAT: &str = "%{file}\t%{family}\t%{style}\n";
/// Largest font the window installs.
pub const MAX_INSTALL_BYTES: u64 = 64 * 1024 * 1024;
const FONT_EXTENSIONS: [&str; 4] = ["ttf", "otf", "ttc", "otc"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    System,
    User,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Face {
    pub family: String,
    pub style: String,
    pub file: PathBuf,
    pub owner: Owner,
}

/// Where the faces come from. The window holds one; tests use a fake.
pub trait FontSource {
    fn faces(&self) -> Result<Vec<Face>, String>;
    /// Rebuild the font cache for the user's folder.
    fn refresh(&self) -> Result<(), String>;
}

/// fontconfig through its command-line tools.
pub struct Fontconfig {
    pub user_dir: PathBuf,
}

fn tool(name: &str) -> Result<PathBuf, String> {
    slopos_core::process::resolve_program(name)
        .ok_or_else(|| format!("fontconfig is not installed: {name} was not found"))
}

impl FontSource for Fontconfig {
    fn faces(&self) -> Result<Vec<Face>, String> {
        let output = Command::new(tool("fc-list")?)
            .args(["-f", FIELD_FORMAT])
            .output()
            .map_err(|error| format!("cannot run fc-list: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "fc-list failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(parse_fc_list(
            &String::from_utf8_lossy(&output.stdout),
            &self.user_dir,
        ))
    }

    fn refresh(&self) -> Result<(), String> {
        fs::create_dir_all(&self.user_dir)
            .map_err(|error| format!("cannot create {}: {error}", self.user_dir.display()))?;
        let output = Command::new(tool("fc-cache")?)
            .arg("-f")
            .arg(&self.user_dir)
            .output()
            .map_err(|error| format!("cannot run fc-cache: {error}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "fc-cache failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    }
}

/// Parses `fc-list -f FIELD_FORMAT` output. A line with no file or family is
/// skipped; the first name of a comma-separated family or style is kept.
pub fn parse_fc_list(text: &str, user_dir: &Path) -> Vec<Face> {
    let mut faces: Vec<Face> = text
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let file = fields.next()?.trim();
            let family = fields.next()?.split(',').next()?.trim();
            let style = fields
                .next()
                .unwrap_or("")
                .split(',')
                .next()
                .unwrap_or("")
                .trim();
            if file.is_empty() || family.is_empty() {
                return None;
            }
            let file = PathBuf::from(file);
            let owner = if file.starts_with(user_dir) {
                Owner::User
            } else {
                Owner::System
            };
            Some(Face {
                family: family.to_owned(),
                style: if style.is_empty() {
                    "Regular".to_owned()
                } else {
                    style.to_owned()
                },
                file,
                owner,
            })
        })
        .collect();
    faces.sort_by(|a, b| {
        (a.family.to_lowercase(), a.style.to_lowercase(), &a.file).cmp(&(
            b.family.to_lowercase(),
            b.style.to_lowercase(),
            &b.file,
        ))
    });
    faces.dedup();
    faces
}

/// Copies a font into the user's folder, under its own name. An existing file
/// of that name is never overwritten.
pub fn install(source: &Path, user_dir: &Path) -> Result<PathBuf, String> {
    let name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "The file name is not text.".to_owned())?
        .to_owned();
    let extension = source
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if !FONT_EXTENSIONS.contains(&extension.as_str()) {
        return Err(format!(
            "{name} is not a font file. Use .ttf, .otf, .ttc or .otc."
        ));
    }
    let metadata = fs::metadata(source).map_err(|error| format!("Cannot read {name}: {error}."))?;
    if !metadata.is_file() {
        return Err(format!("{name} is not a file."));
    }
    if metadata.len() > MAX_INSTALL_BYTES {
        return Err(format!("{name} is larger than 64 MiB."));
    }
    fs::create_dir_all(user_dir)
        .map_err(|error| format!("Cannot create {}: {error}.", user_dir.display()))?;
    let target = user_dir.join(&name);
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .map_err(|error| match error.kind() {
            io::ErrorKind::AlreadyExists => {
                format!("{name} is already installed. Nothing was overwritten.")
            }
            _ => format!("Cannot write {name}: {error}."),
        })?;
    let mut input =
        fs::File::open(source).map_err(|error| format!("Cannot read {name}: {error}."))?;
    if let Err(error) = io::copy(&mut input, &mut output) {
        let _ = fs::remove_file(&target);
        return Err(format!("Cannot copy {name}: {error}."));
    }
    Ok(target)
}

/// Deletes a font the user installed. Refuses anything outside the user's
/// folder, and any path that climbs with `..`.
pub fn remove(file: &Path, user_dir: &Path) -> Result<(), String> {
    let climbs = file.components().any(|c| matches!(c, Component::ParentDir));
    if climbs || !file.starts_with(user_dir) {
        return Err(
            "Only fonts you installed can be removed. System fonts belong to the system."
                .to_owned(),
        );
    }
    fs::remove_file(file).map_err(|error| format!("Cannot remove {}: {error}.", file.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LISTING: &str = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf\tDejaVu Sans\tBook\n\
/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf\tDejaVu Sans,DejaVu Sans Bold\tBold\n\
/home/u/.local/share/fonts/Mine.otf\tMine Font\tRegular,Roman\n\
\n\
/usr/share/fonts/broken\t\tRegular\n\
/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf\tDejaVu Sans\tBook\n";

    #[test]
    fn fc_list_lines_become_faces_with_owners() {
        let user = Path::new("/home/u/.local/share/fonts");
        let faces = parse_fc_list(LISTING, user);
        assert_eq!(faces.len(), 3, "{faces:?}");
        // Sorted by family, then style: "Bold" comes before "Book".
        assert_eq!(faces[0].family, "DejaVu Sans");
        assert_eq!(faces[0].style, "Bold");
        assert_eq!(faces[0].owner, Owner::System);
        assert_eq!(faces[1].family, "DejaVu Sans");
        assert_eq!(faces[1].style, "Book");
        assert_eq!(faces[2].family, "Mine Font");
        assert_eq!(faces[2].style, "Regular");
        assert_eq!(faces[2].owner, Owner::User);
    }

    #[test]
    fn a_missing_style_is_regular() {
        let faces = parse_fc_list("/x/y.ttf\tPlain\t\n", Path::new("/u"));
        assert_eq!(faces[0].style, "Regular");
    }

    #[test]
    fn install_copies_a_font_and_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("Pick.ttf");
        fs::write(&source, b"not really a font, but bytes").unwrap();
        let user = dir.path().join("user-fonts");
        let installed = install(&source, &user).unwrap();
        assert_eq!(installed, user.join("Pick.ttf"));
        assert_eq!(
            fs::read(&installed).unwrap(),
            b"not really a font, but bytes"
        );

        let again = install(&source, &user).unwrap_err();
        assert!(again.contains("already installed"), "{again}");
    }

    #[test]
    fn install_refuses_what_is_not_a_font() {
        let dir = tempfile::tempdir().unwrap();
        let text = dir.path().join("notes.txt");
        fs::write(&text, "hi").unwrap();
        let error = install(&text, &dir.path().join("fonts")).unwrap_err();
        assert!(error.contains("not a font file"), "{error}");
        assert!(!dir.path().join("fonts").exists());
        let missing = install(&dir.path().join("gone.otf"), &dir.path().join("fonts"));
        assert!(missing.unwrap_err().contains("Cannot read gone.otf"));
    }

    #[test]
    fn remove_deletes_only_user_fonts() {
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path().join("user-fonts");
        fs::create_dir_all(&user).unwrap();
        let mine = user.join("Mine.ttf");
        fs::write(&mine, "x").unwrap();
        let system = dir.path().join("system.ttf");
        fs::write(&system, "y").unwrap();

        assert!(remove(&system, &user).unwrap_err().contains("System fonts"));
        assert!(system.exists());
        let escape = user.join("..").join("system.ttf");
        assert!(remove(&escape, &user).is_err());
        assert!(system.exists());

        remove(&mine, &user).unwrap();
        assert!(!mine.exists());
    }
}
