//! Where screenshots go and what they are called. Plain code, no X11.

use std::path::{Path, PathBuf};

/// The Pictures folder: `XDG_PICTURES_DIR` from the environment, then the
/// `XDG_PICTURES_DIR` line of `user-dirs.dirs`, then `~/Pictures`. A relative
/// path is ignored at each step because it would depend on the working
/// directory.
pub fn pictures_dir(
    env_value: Option<&str>,
    user_dirs: Option<&str>,
    home: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(path) = env_value
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
    {
        return Some(path);
    }
    if let (Some(text), Some(home)) = (user_dirs, home) {
        if let Some(path) = parse_user_dirs(text, home) {
            return Some(path);
        }
    }
    home.map(|home| home.join("Pictures"))
}

/// Reads `XDG_PICTURES_DIR="$HOME/Pictures"` from a `user-dirs.dirs` file.
fn parse_user_dirs(text: &str, home: &Path) -> Option<PathBuf> {
    let home = home.to_str()?;
    text.lines().find_map(|line| {
        let value = line.trim().strip_prefix("XDG_PICTURES_DIR=")?;
        let expanded = value.trim().trim_matches('"').replace("$HOME", home);
        let path = PathBuf::from(expanded);
        path.is_absolute().then_some(path)
    })
}

/// `Screenshot-YYYY-MM-DD-HHMMSS.png` for a Unix time, in UTC.
pub fn file_name(unix_seconds: u64) -> String {
    let days = (unix_seconds / 86_400) as i64;
    let seconds_of_day = unix_seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "Screenshot-{year:04}-{month:02}-{day:02}-{:02}{:02}{:02}.png",
        seconds_of_day / 3_600,
        (seconds_of_day % 3_600) / 60,
        seconds_of_day % 60
    )
}

/// Days since 1970-01-01 to (year, month, day). Howard Hinnant's algorithm.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let day_of_era = (z - era * 146_097) as u64;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let march_based_month = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * march_based_month + 2) / 5 + 1) as u32;
    let month = if march_based_month < 10 {
        march_based_month + 3
    } else {
        march_based_month - 9
    } as u32;
    let year = year_of_era as i64 + era * 400;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// How many numbered names are tried before a save gives up.
const MAX_NAME_ATTEMPTS: u32 = 1_000;

/// `name` for the first attempt, then `stem-2.png`, `stem-3.png`, ...
pub fn candidate_name(name: &str, attempt: u32) -> String {
    if attempt <= 1 {
        return name.to_owned();
    }
    let stem = name.strip_suffix(".png").unwrap_or(name);
    format!("{stem}-{attempt}.png")
}

/// Encodes `image` as PNG bytes. Every saved screenshot goes through this.
pub fn encode_png(image: &image::RgbaImage) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    image
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .map_err(|error| error.to_string())?;
    Ok(bytes)
}

/// Creates `path` and writes `bytes` into it. The create is exclusive, so an
/// existing file is never truncated or replaced. If the write fails after the
/// file was created, the partial file is removed.
pub fn write_new_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    if let Err(error) = file.write_all(bytes) {
        drop(file);
        let _ = std::fs::remove_file(path);
        return Err(error);
    }
    Ok(())
}

/// Writes `bytes` into `dir` as `name`, or as the first free numbered name.
/// Never overwrites: a name that exists is skipped.
pub fn create_unique_file(dir: &Path, name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    for attempt in 1..=MAX_NAME_ATTEMPTS {
        let path = dir.join(candidate_name(name, attempt));
        match write_new_file(&path, bytes) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
    }
    Err(format!("no free file name for {name}"))
}

/// Writes `image` as a PNG into `dir`, creating the folder when needed.
pub fn save_png(
    image: &image::RgbaImage,
    dir: &Path,
    unix_seconds: u64,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let bytes = encode_png(image)?;
    create_unique_file(dir, &file_name(unix_seconds), &bytes)
}

/// Shortens `text` to `max_chars` characters, keeping both ends of a path.
pub fn shorten(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars || max_chars < 5 {
        return text.to_owned();
    }
    let keep = max_chars - 3;
    let head = keep / 2;
    let tail = keep - head;
    let start: String = text.chars().take(head).collect();
    let end: String = text.chars().skip(count - tail).collect();
    format!("{start}...{end}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_wins_over_user_dirs_and_home() {
        let home = Path::new("/home/ann");
        let user_dirs = "XDG_PICTURES_DIR=\"$HOME/Images\"\n";
        assert_eq!(
            pictures_dir(Some("/data/shots"), Some(user_dirs), Some(home)),
            Some(PathBuf::from("/data/shots"))
        );
    }

    #[test]
    fn relative_environment_values_are_ignored() {
        let home = Path::new("/home/ann");
        assert_eq!(
            pictures_dir(Some("shots"), None, Some(home)),
            Some(PathBuf::from("/home/ann/Pictures"))
        );
        assert_eq!(
            pictures_dir(Some(""), None, Some(home)),
            Some(PathBuf::from("/home/ann/Pictures"))
        );
    }

    #[test]
    fn user_dirs_file_is_read_with_home_expanded() {
        let home = Path::new("/home/ann");
        let text =
            "# comment\nXDG_DOWNLOAD_DIR=\"$HOME/Downloads\"\nXDG_PICTURES_DIR=\"$HOME/Images\"\n";
        assert_eq!(
            pictures_dir(None, Some(text), Some(home)),
            Some(PathBuf::from("/home/ann/Images"))
        );
    }

    #[test]
    fn user_dirs_values_must_be_absolute_after_expansion() {
        let home = Path::new("/home/ann");
        let text = "XDG_PICTURES_DIR=\"relative/Pics\"\n";
        assert_eq!(
            pictures_dir(None, Some(text), Some(home)),
            Some(PathBuf::from("/home/ann/Pictures"))
        );
    }

    #[test]
    fn without_environment_or_home_there_is_no_folder() {
        assert_eq!(pictures_dir(None, None, None), None);
    }

    #[test]
    fn file_names_use_utc_calendar_dates() {
        assert_eq!(file_name(0), "Screenshot-1970-01-01-000000.png");
        assert_eq!(file_name(951_782_400), "Screenshot-2000-02-29-000000.png");
        // 2026-11-27 12:34:56 UTC.
        assert_eq!(file_name(1_795_782_896), "Screenshot-2026-11-27-123456.png");
    }

    #[test]
    fn candidate_names_add_a_counter_after_the_first_attempt() {
        let name = "Screenshot-2026-11-27-123456.png";
        assert_eq!(candidate_name(name, 1), name);
        assert_eq!(
            candidate_name(name, 2),
            "Screenshot-2026-11-27-123456-2.png"
        );
        assert_eq!(
            candidate_name(name, 3),
            "Screenshot-2026-11-27-123456-3.png"
        );
    }

    #[test]
    fn an_existing_file_is_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let name = "Screenshot-2026-11-27-123456.png";
        std::fs::write(dir.path().join(name), b"keep me").unwrap();
        let written = create_unique_file(dir.path(), name, b"new").unwrap();
        assert_eq!(
            written,
            dir.path().join("Screenshot-2026-11-27-123456-2.png")
        );
        assert_eq!(std::fs::read(dir.path().join(name)).unwrap(), b"keep me");
        assert_eq!(std::fs::read(&written).unwrap(), b"new");
    }

    #[test]
    fn write_new_file_refuses_an_existing_path_and_keeps_its_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shot.png");
        std::fs::write(&path, b"original").unwrap();
        let error = write_new_file(&path, b"replacement").unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
    }

    #[test]
    fn a_save_into_a_path_that_is_a_file_reports_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("not-a-folder");
        std::fs::write(&blocker, b"file").unwrap();
        let image = image::RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 0, 255]));
        assert!(save_png(&image, &blocker, 0).is_err());
        assert_eq!(std::fs::read(&blocker).unwrap(), b"file");
    }

    #[test]
    fn save_png_writes_a_png_file_into_a_new_folder() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("Pictures");
        let image = image::RgbaImage::from_pixel(3, 2, image::Rgba([10, 20, 30, 255]));
        let path = save_png(&image, &folder, 0).unwrap();
        assert_eq!(path, folder.join("Screenshot-1970-01-01-000000.png"));
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        let decoded = image::open(&path).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (3, 2));
        assert_eq!(decoded.get_pixel(0, 0).0, [10, 20, 30, 255]);
    }

    #[test]
    fn shorten_keeps_both_ends_of_long_paths() {
        assert_eq!(shorten("/home/ann/Pictures", 40), "/home/ann/Pictures");
        let short = shorten("/home/ann/a/very/long/path/to/Pictures", 16);
        assert_eq!(short.chars().count(), 16);
        assert!(short.starts_with("/home"));
        assert!(short.ends_with("ictures"));
    }
}
