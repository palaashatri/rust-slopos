//! The archive model: list and extract zip, tar, and tar compressed with
//! gzip, xz or zstd. Pure Rust, no window.
//!
//! Safety rules:
//! - an entry whose path is absolute, climbs with `..`, or is too long
//!   refuses the whole archive. The listing shows the entry, and extraction
//!   checks every entry before it writes anything;
//! - links, devices and FIFOs are not recreated. They are counted as skipped;
//! - extraction creates the destination folder itself and never overwrites:
//!   a folder that already exists is an error;
//! - listing stops after [`MAX_ENTRIES`] items, and extraction stops after
//!   [`MAX_BYTES`] of data, both while reading and before writing;
//! - a failed or cancelled extraction removes the folder it created.

use std::fs;
use std::io::{self, BufReader, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Most items a listing shows, and most a single extraction writes.
pub const MAX_ENTRIES: u64 = 100_000;
/// Most data an extraction writes: 4 GiB of uncompressed content.
pub const MAX_BYTES: u64 = 4 * 1024 * 1024 * 1024;
/// Longest entry path accepted, in bytes.
const MAX_PATH_BYTES: usize = 4096;
const CHUNK: usize = 128 * 1024;
const MIB: u64 = 1024 * 1024;
/// How much data an extraction writes between two progress notifications.
pub const NOTIFY_EVERY: u64 = 16 * MIB;
/// The most xz padding the reader drains after the tar end marker.
const XZ_TRAILER: u64 = 64 * MIB;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Zip,
    Tar,
    TarGz,
    TarXz,
    TarZst,
}

impl Format {
    pub fn label(self) -> &'static str {
        match self {
            Self::Zip => "ZIP",
            Self::Tar => "TAR",
            Self::TarGz => "TAR.GZ",
            Self::TarXz => "TAR.XZ",
            Self::TarZst => "TAR.ZST",
        }
    }
}

/// The format and the name without its extension, from a file name.
pub fn format_of(name: &str) -> Option<(Format, &str)> {
    let lower = name.to_ascii_lowercase();
    let formats = [
        (".tar.gz", Format::TarGz),
        (".tgz", Format::TarGz),
        (".tar.xz", Format::TarXz),
        (".txz", Format::TarXz),
        (".tar.zst", Format::TarZst),
        (".tzst", Format::TarZst),
        (".tar", Format::Tar),
        (".zip", Format::Zip),
    ];
    formats.into_iter().find_map(|(suffix, format)| {
        let stem = name.get(..name.len().checked_sub(suffix.len())?)?;
        (lower.ends_with(suffix) && !stem.is_empty()).then_some((format, stem))
    })
}

/// Why an entry path is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Only `.` components: the archive's own folder, nothing to write.
    Empty,
    Absolute,
    ParentDirectory,
    TooLong,
}

impl Refusal {
    fn reason(self) -> &'static str {
        match self {
            Self::Empty => "names no file",
            Self::Absolute => "has an absolute path",
            Self::ParentDirectory => "climbs out of the folder with ..",
            Self::TooLong => "has a path that is too long",
        }
    }
}

/// A safe relative path for an entry, or the reason to refuse it.
pub fn check_path(raw: &str) -> Result<PathBuf, Refusal> {
    if raw.len() > MAX_PATH_BYTES {
        return Err(Refusal::TooLong);
    }
    let mut path = PathBuf::new();
    for component in Path::new(raw).components() {
        match component {
            Component::Normal(part) => path.push(part),
            Component::CurDir => {}
            Component::RootDir | Component::Prefix(_) => return Err(Refusal::Absolute),
            Component::ParentDir => return Err(Refusal::ParentDirectory),
        }
    }
    if path.as_os_str().is_empty() {
        Err(Refusal::Empty)
    } else {
        Ok(path)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    File,
    Directory,
    Link,
    Other,
}

/// One item of an archive, as the listing shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The path exactly as the archive stores it.
    pub name: String,
    pub kind: Kind,
    /// Uncompressed size in bytes; 0 for folders and links.
    pub size: u64,
    pub refusal: Option<Refusal>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub format: Format,
    pub entries: Vec<Entry>,
    /// The archive has more items than [`MAX_ENTRIES`]; the rest are not listed.
    pub truncated: bool,
}

impl Listing {
    /// Sum of the uncompressed sizes of the files, saturating.
    pub fn total_size(&self) -> u64 {
        self.entries
            .iter()
            .filter(|entry| entry.kind == Kind::File)
            .fold(0u64, |sum, entry| sum.saturating_add(entry.size))
    }

    /// The first entry the extraction would refuse.
    pub fn first_refusal(&self) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.refusal.is_some())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchiveError {
    UnknownFormat,
    NotArchive(String),
    Refused { entry: String, reason: Refusal },
    TooLarge(String),
    DestinationExists(PathBuf),
    Duplicate(String),
    Cancelled,
    Io(String),
}

impl std::fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownFormat => write!(
                f,
                "Not a supported archive. Use .zip, .tar, .tar.gz, .tar.xz or .tar.zst."
            ),
            Self::NotArchive(reason) => write!(f, "This file is not a readable archive: {reason}"),
            Self::Refused { entry, reason } => write!(
                f,
                "Refused: the entry \"{entry}\" {}. Nothing was extracted.",
                reason.reason()
            ),
            Self::TooLarge(limit) => {
                write!(
                    f,
                    "The archive is larger than {limit}. Nothing was extracted."
                )
            }
            Self::DestinationExists(path) => write!(
                f,
                "{} already exists. Nothing was overwritten.",
                path.display()
            ),
            Self::Duplicate(name) => write!(
                f,
                "\"{name}\" is in the archive twice. Nothing was extracted."
            ),
            Self::Cancelled => write!(
                f,
                "Extraction was cancelled. The partial folder was removed."
            ),
            Self::Io(reason) => write!(f, "A file could not be written: {reason}"),
        }
    }
}

fn io_error(error: io::Error) -> ArchiveError {
    ArchiveError::Io(error.to_string())
}

/// Shared between the window and a worker thread. The worker checks
/// [`Progress::is_cancelled`] between chunks; `notify` runs every
/// [`NOTIFY_EVERY`] bytes so the window can repaint without polling.
pub struct Progress {
    cancelled: AtomicBool,
    bytes: AtomicU64,
    notify: Box<dyn Fn() + Send + Sync>,
}

impl Progress {
    pub fn new(notify: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            cancelled: AtomicBool::new(false),
            bytes: AtomicU64::new(0),
            notify: Box::new(notify),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    pub fn bytes(&self) -> u64 {
        self.bytes.load(Ordering::Relaxed)
    }

    fn add(&self, count: u64) {
        let before = self.bytes.fetch_add(count, Ordering::Relaxed);
        if before / NOTIFY_EVERY != (before + count) / NOTIFY_EVERY {
            (self.notify)();
        }
    }

    fn check(&self) -> Result<(), ArchiveError> {
        if self.is_cancelled() {
            Err(ArchiveError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// What the extraction wrote.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    pub files: u64,
    pub directories: u64,
    pub bytes: u64,
    pub skipped_links: u64,
    pub skipped_other: u64,
}

/// One item as the readers pass it on.
struct Header {
    raw_name: String,
    kind: Kind,
    size: u64,
    mode: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Flow {
    Continue,
    Stop,
}

type Visit<'a> = dyn FnMut(&Header, &mut dyn Read) -> Result<Flow, ArchiveError> + 'a;

/// Calls `visit` for each item in order, with a reader for its data.
fn walk(archive: &Path, format: Format, visit: &mut Visit<'_>) -> Result<(), ArchiveError> {
    let file =
        fs::File::open(archive).map_err(|error| ArchiveError::NotArchive(format!("{error}")))?;
    match format {
        Format::Zip => walk_zip(BufReader::new(file), visit),
        Format::Tar => walk_tar(BufReader::new(file), visit),
        Format::TarGz => walk_tar(
            flate2::read::MultiGzDecoder::new(BufReader::new(file)),
            visit,
        ),
        Format::TarZst => {
            let mut decoder = ruzstd::StreamingDecoder::new(BufReader::new(file))
                .map_err(|error| ArchiveError::NotArchive(error.to_string()))?;
            walk_tar(&mut decoder, visit)
        }
        Format::TarXz => with_xz(file, |reader| walk_tar(reader, visit)),
    }
}

fn walk_zip<R: Read + io::Seek>(reader: R, visit: &mut Visit<'_>) -> Result<(), ArchiveError> {
    let mut zip = zip::ZipArchive::new(reader)
        .map_err(|error| ArchiveError::NotArchive(error.to_string()))?;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|error| ArchiveError::NotArchive(error.to_string()))?;
        let kind = if entry.is_dir() {
            Kind::Directory
        } else if entry.is_symlink() {
            Kind::Link
        } else {
            Kind::File
        };
        let header = Header {
            raw_name: entry.name().to_owned(),
            kind,
            size: entry.size(),
            mode: entry.unix_mode().unwrap_or(0o644),
        };
        if visit(&header, &mut entry)? == Flow::Stop {
            break;
        }
    }
    Ok(())
}

fn walk_tar<R: Read>(reader: R, visit: &mut Visit<'_>) -> Result<(), ArchiveError> {
    let mut tar = tar::Archive::new(reader);
    let entries = tar
        .entries()
        .map_err(|error| ArchiveError::NotArchive(error.to_string()))?;
    for entry in entries {
        let mut entry = entry.map_err(|error| ArchiveError::NotArchive(error.to_string()))?;
        let (kind, size, mode) = {
            let header = entry.header();
            let kind = match header.entry_type() {
                tar::EntryType::Directory => Some(Kind::Directory),
                tar::EntryType::Regular | tar::EntryType::Continuous => Some(Kind::File),
                tar::EntryType::Symlink | tar::EntryType::Link => Some(Kind::Link),
                // Extension records: the tar reader applies them to the next entry.
                tar::EntryType::XGlobalHeader
                | tar::EntryType::XHeader
                | tar::EntryType::GNULongName
                | tar::EntryType::GNULongLink => None,
                _ => Some(Kind::Other),
            };
            (
                kind,
                header.size().unwrap_or(0),
                header.mode().unwrap_or(0o644),
            )
        };
        let Some(kind) = kind else {
            continue;
        };
        let raw_name = entry
            .path()
            .map_err(|error| ArchiveError::NotArchive(error.to_string()))?
            .to_string_lossy()
            .into_owned();
        let header = Header {
            raw_name,
            kind,
            size,
            mode,
        };
        if visit(&header, &mut entry)? == Flow::Stop {
            break;
        }
    }
    Ok(())
}

/// lzma-rs decodes into a writer; a pipe turns that into the reader tar
/// needs. Dropping the reader on an error or cancel stops the decoder.
fn with_xz<T>(
    file: fs::File,
    body: impl FnOnce(&mut dyn Read) -> Result<T, ArchiveError>,
) -> Result<T, ArchiveError> {
    let (mut pipe_reader, mut pipe_writer) = io::pipe().map_err(io_error)?;
    std::thread::scope(|scope| {
        let decoder = scope.spawn(move || {
            let mut input = BufReader::new(file);
            lzma_rs::xz_decompress(&mut input, &mut pipe_writer).map_err(|error| error.to_string())
        });
        let result = body(&mut pipe_reader);
        if result.is_ok() {
            // tar stops at its end marker; drain the record padding after it.
            let _ = io::copy(&mut (&mut pipe_reader).take(XZ_TRAILER), &mut io::sink());
        }
        drop(pipe_reader);
        let decoded = decoder
            .join()
            .unwrap_or_else(|_| Err("the decoder stopped".to_owned()));
        match (result, decoded) {
            (Err(error), _) => Err(error),
            (Ok(_), Err(reason)) => Err(ArchiveError::NotArchive(reason)),
            (Ok(value), Ok(())) => Ok(value),
        }
    })
}

/// The archive's items, without extracting anything.
pub fn list(archive: &Path) -> Result<Listing, ArchiveError> {
    let name = file_name(archive);
    let (format, _) = format_of(&name).ok_or(ArchiveError::UnknownFormat)?;
    let mut entries = Vec::new();
    let mut truncated = false;
    walk(archive, format, &mut |header, _data| {
        if entries.len() as u64 >= MAX_ENTRIES {
            truncated = true;
            return Ok(Flow::Stop);
        }
        match check_path(&header.raw_name) {
            // The archive's own folder entry ("./") is not listed.
            Err(Refusal::Empty) => {}
            refusal => entries.push(Entry {
                name: header.raw_name.clone(),
                kind: header.kind,
                size: header.size,
                refusal: refusal.err(),
            }),
        }
        Ok(Flow::Continue)
    })?;
    Ok(Listing {
        format,
        entries,
        truncated,
    })
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The folder an extraction creates: the archive's name without its
/// extension, next to the archive.
pub fn destination_for(archive: &Path) -> Option<PathBuf> {
    let name = file_name(archive);
    let (_, stem) = format_of(&name)?;
    Some(archive.parent()?.join(stem))
}

/// Extract `archive` into `destination`, which must not exist yet.
///
/// The archive is listed first, so a refused path or an oversized archive
/// stops the extraction before anything is written.
pub fn extract(
    archive: &Path,
    destination: &Path,
    progress: &Progress,
) -> Result<Summary, ArchiveError> {
    let listing = list(archive)?;
    if let Some(entry) = listing.first_refusal() {
        return Err(ArchiveError::Refused {
            entry: entry.name.clone(),
            reason: entry.refusal.unwrap_or(Refusal::Empty),
        });
    }
    if listing.truncated {
        return Err(ArchiveError::TooLarge(format!("{MAX_ENTRIES} items")));
    }
    if listing.total_size() > MAX_BYTES {
        return Err(ArchiveError::TooLarge(format!(
            "{} GiB of data",
            MAX_BYTES / (1024 * MIB)
        )));
    }
    if fs::symlink_metadata(destination).is_ok() {
        return Err(ArchiveError::DestinationExists(destination.to_owned()));
    }
    let strip = strip_archive_folder(&listing, archive);
    fs::create_dir(destination).map_err(io_error)?;
    match unpack(archive, listing.format, destination, strip, progress) {
        Ok(summary) => Ok(summary),
        Err(error) => {
            // The folder is ours: it was created above, and links are never written.
            let _ = fs::remove_dir_all(destination);
            Err(error)
        }
    }
}

/// Whether every item sits inside a folder named like the archive ("Photos.zip"
/// holding "Photos/..."). Such a folder is not nested again: the items go
/// straight into the destination.
fn strip_archive_folder(listing: &Listing, archive: &Path) -> bool {
    let name = file_name(archive);
    let Some((_, stem)) = format_of(&name) else {
        return false;
    };
    let folder = Component::Normal(std::ffi::OsStr::new(stem));
    !listing.entries.is_empty()
        && listing.entries.iter().all(|entry| {
            check_path(&entry.name).is_ok_and(|path| path.components().next() == Some(folder))
        })
}

fn unpack(
    archive: &Path,
    format: Format,
    root: &Path,
    strip: bool,
    progress: &Progress,
) -> Result<Summary, ArchiveError> {
    let mut summary = Summary::default();
    let mut written: u64 = 0;
    let mut items: u64 = 0;
    walk(archive, format, &mut |header, data| {
        progress.check()?;
        items += 1;
        if items > MAX_ENTRIES {
            return Err(ArchiveError::TooLarge(format!("{MAX_ENTRIES} items")));
        }
        let relative = match check_path(&header.raw_name) {
            Ok(relative) if strip => {
                let rest: PathBuf = relative.components().skip(1).collect();
                if rest.as_os_str().is_empty() {
                    // The archive's own folder entry.
                    return Ok(Flow::Continue);
                }
                rest
            }
            Ok(relative) => relative,
            Err(Refusal::Empty) => return Ok(Flow::Continue),
            Err(reason) => {
                return Err(ArchiveError::Refused {
                    entry: header.raw_name.clone(),
                    reason,
                })
            }
        };
        let target = root.join(&relative);
        match header.kind {
            Kind::Directory => {
                fs::create_dir_all(&target).map_err(io_error)?;
                summary.directories += 1;
            }
            Kind::File => {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent).map_err(io_error)?;
                }
                let copied = copy_entry(data, &target, progress, &mut written)?;
                set_mode(&target, header.mode)?;
                summary.files += 1;
                summary.bytes += copied;
            }
            Kind::Link => summary.skipped_links += 1,
            Kind::Other => summary.skipped_other += 1,
        }
        Ok(Flow::Continue)
    })?;
    Ok(summary)
}

/// Writes one file. It must not exist yet, and the running total of data
/// written is checked against [`MAX_BYTES`] before each chunk is written.
fn copy_entry(
    data: &mut dyn Read,
    target: &Path,
    progress: &Progress,
    written: &mut u64,
) -> Result<u64, ArchiveError> {
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(|error| match error.kind() {
            io::ErrorKind::AlreadyExists => ArchiveError::Duplicate(
                target
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            ),
            _ => io_error(error),
        })?;
    let mut buffer = vec![0u8; CHUNK];
    let mut copied = 0u64;
    loop {
        progress.check()?;
        let read = match data.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) => return Err(io_error(error)),
        };
        *written += read as u64;
        if *written > MAX_BYTES {
            return Err(ArchiveError::TooLarge(format!(
                "{} GiB of data",
                MAX_BYTES / (1024 * MIB)
            )));
        }
        output.write_all(&buffer[..read]).map_err(io_error)?;
        copied += read as u64;
        progress.add(read as u64);
    }
    Ok(copied)
}

/// Keeps the permission bits and makes the file readable and writable by
/// its owner. No setuid, setgid or sticky bits.
fn set_mode(target: &Path, mode: u32) -> Result<(), ArchiveError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(target, fs::Permissions::from_mode((mode & 0o777) | 0o600))
        .map_err(io_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;

    fn no_progress() -> Progress {
        Progress::new(|| {})
    }

    /// Writes a zip with the given (name, contents) files; a name ending in
    /// `/` is a folder.
    fn make_zip(path: &Path, items: &[(&str, &str)]) {
        let file = fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(io::BufWriter::new(file));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, body) in items {
            if name.ends_with('/') {
                zip.add_directory(*name, options).unwrap();
            } else {
                zip.start_file(*name, options).unwrap();
                zip.write_all(body.as_bytes()).unwrap();
            }
        }
        zip.finish().unwrap();
    }

    /// Writes a tar with the given (name, contents) regular files. The name is
    /// stored as given, so an unsafe path can be tested.
    fn make_tar(path: &Path, items: &[(&str, &str)]) {
        let mut builder = tar::Builder::new(fs::File::create(path).unwrap());
        for (name, body) in items {
            let mut header = tar::Header::new_ustar();
            let bytes = name.as_bytes();
            header.as_old_mut().name[..bytes.len()].copy_from_slice(bytes);
            header.set_size(body.len() as u64);
            header.set_mode(0o644);
            header.set_entry_type(tar::EntryType::Regular);
            header.set_cksum();
            builder.append(&header, body.as_bytes()).unwrap();
        }
        builder.into_inner().unwrap().flush().unwrap();
    }

    #[test]
    fn formats_come_from_the_file_name() {
        assert_eq!(format_of("Photos.zip"), Some((Format::Zip, "Photos")));
        assert_eq!(format_of("a.TAR.GZ"), Some((Format::TarGz, "a")));
        assert_eq!(format_of("b.tgz"), Some((Format::TarGz, "b")));
        assert_eq!(format_of("c.tar.zst"), Some((Format::TarZst, "c")));
        assert_eq!(format_of("d.txz"), Some((Format::TarXz, "d")));
        assert_eq!(format_of("e.tar"), Some((Format::Tar, "e")));
        assert_eq!(format_of(".zip"), None);
        assert_eq!(format_of("notes.txt"), None);
    }

    #[test]
    fn safe_paths_are_relative_and_refusals_are_named() {
        assert_eq!(check_path("a/./b"), Ok(PathBuf::from("a/b")));
        assert_eq!(check_path("dir/"), Ok(PathBuf::from("dir")));
        assert_eq!(check_path("/etc/passwd"), Err(Refusal::Absolute));
        assert_eq!(check_path("../secret"), Err(Refusal::ParentDirectory));
        assert_eq!(check_path("a/../../b"), Err(Refusal::ParentDirectory));
        assert_eq!(check_path("./"), Err(Refusal::Empty));
        let long = "a/".repeat(MAX_PATH_BYTES);
        assert_eq!(check_path(&long), Err(Refusal::TooLong));
    }

    #[test]
    fn a_zip_lists_its_files_folders_and_sizes() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("Photos.zip");
        make_zip(&archive, &[("Photos/", ""), ("Photos/a.txt", "hello")]);
        let listing = list(&archive).unwrap();
        assert_eq!(listing.format, Format::Zip);
        assert!(!listing.truncated);
        let names: Vec<(&str, Kind, u64)> = listing
            .entries
            .iter()
            .map(|entry| (entry.name.as_str(), entry.kind, entry.size))
            .collect();
        assert_eq!(
            names,
            vec![
                ("Photos/", Kind::Directory, 0),
                ("Photos/a.txt", Kind::File, 5)
            ]
        );
        assert_eq!(listing.total_size(), 5);
        assert!(listing.first_refusal().is_none());
    }

    #[test]
    fn listing_shows_an_unsafe_entry_and_extraction_refuses_it() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("evil.tar");
        make_tar(&archive, &[("ok.txt", "fine"), ("../escape.txt", "no")]);
        let listing = list(&archive).unwrap();
        let refused = listing.first_refusal().expect("a refused entry");
        assert_eq!(refused.name, "../escape.txt");
        assert_eq!(refused.refusal, Some(Refusal::ParentDirectory));

        let destination = dir.path().join("evil");
        let error = extract(&archive, &destination, &no_progress()).unwrap_err();
        assert!(matches!(error, ArchiveError::Refused { .. }));
        assert!(
            !destination.exists(),
            "nothing is written for a refused archive"
        );
        assert!(!dir.path().join("escape.txt").exists());
    }

    #[test]
    fn an_absolute_path_is_refused_before_anything_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("abs.tar");
        make_tar(&archive, &[("/tmp/slopos-archive-test-abs", "no")]);
        let destination = dir.path().join("abs");
        let error = extract(&archive, &destination, &no_progress()).unwrap_err();
        assert_eq!(
            error,
            ArchiveError::Refused {
                entry: "/tmp/slopos-archive-test-abs".to_owned(),
                reason: Refusal::Absolute,
            }
        );
        assert!(!destination.exists());
    }

    #[test]
    fn a_zip_in_its_own_folder_is_not_nested_again() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("Photos.zip");
        make_zip(
            &archive,
            &[
                ("Photos/", ""),
                ("Photos/a.txt", "hello"),
                ("Photos/sub/b.txt", "world!"),
            ],
        );
        let destination = destination_for(&archive).unwrap();
        assert_eq!(destination, dir.path().join("Photos"));
        let summary = extract(&archive, &destination, &no_progress()).unwrap();
        assert_eq!(summary.files, 2);
        assert_eq!(summary.directories, 0);
        assert_eq!(summary.bytes, 11);
        assert_eq!(
            fs::read_to_string(destination.join("a.txt")).unwrap(),
            "hello"
        );
        assert_eq!(
            fs::read_to_string(destination.join("sub/b.txt")).unwrap(),
            "world!"
        );
        assert!(!destination.join("Photos").exists());
    }

    #[test]
    fn a_zip_without_its_own_folder_extracts_as_it_is() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("Loose.zip");
        make_zip(&archive, &[("a.txt", "one"), ("docs/b.txt", "two")]);
        let destination = destination_for(&archive).unwrap();
        let summary = extract(&archive, &destination, &no_progress()).unwrap();
        assert_eq!(summary.files, 2);
        assert_eq!(
            fs::read_to_string(destination.join("docs/b.txt")).unwrap(),
            "two"
        );
    }

    #[test]
    fn an_existing_destination_is_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("Photos.zip");
        make_zip(&archive, &[("a.txt", "new")]);
        let destination = dir.path().join("Photos");
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("keep.txt"), "mine").unwrap();
        let error = extract(&archive, &destination, &no_progress()).unwrap_err();
        assert_eq!(error, ArchiveError::DestinationExists(destination.clone()));
        assert_eq!(
            fs::read_to_string(destination.join("keep.txt")).unwrap(),
            "mine"
        );
        assert!(!destination.join("a.txt").exists());
    }

    #[test]
    fn a_tar_gz_extracts_its_files() {
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("plain.tar");
        make_tar(&plain, &[("doc/readme.txt", "read me")]);
        let archive = dir.path().join("pack.tar.gz");
        let mut encoder = flate2::write::GzEncoder::new(
            fs::File::create(&archive).unwrap(),
            flate2::Compression::fast(),
        );
        encoder.write_all(&fs::read(&plain).unwrap()).unwrap();
        encoder.finish().unwrap();
        let destination = dir.path().join("pack");
        let summary = extract(&archive, &destination, &no_progress()).unwrap();
        assert_eq!(summary.files, 1);
        assert_eq!(
            fs::read_to_string(destination.join("doc/readme.txt")).unwrap(),
            "read me"
        );
    }

    #[test]
    fn a_file_that_is_not_an_archive_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("fake.zip");
        fs::write(&fake, "this is not a zip file at all").unwrap();
        assert!(matches!(list(&fake), Err(ArchiveError::NotArchive(_))));
        let unknown = dir.path().join("notes.txt");
        fs::write(&unknown, "x").unwrap();
        assert_eq!(list(&unknown), Err(ArchiveError::UnknownFormat));
    }

    #[test]
    fn cancelling_removes_the_partial_folder() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("Big.zip");
        make_zip(&archive, &[("a.txt", "some data")]);
        let progress = no_progress();
        progress.cancel();
        let destination = dir.path().join("Big");
        assert_eq!(
            extract(&archive, &destination, &progress),
            Err(ArchiveError::Cancelled)
        );
        assert!(!destination.exists());
    }

    #[test]
    fn progress_notifies_at_each_step_of_bytes() {
        let count = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&count);
        let progress = Progress::new(move || {
            seen.fetch_add(1, Ordering::Relaxed);
        });
        progress.add(NOTIFY_EVERY / 2);
        assert_eq!(count.load(Ordering::Relaxed), 0);
        progress.add(NOTIFY_EVERY);
        assert_eq!(count.load(Ordering::Relaxed), 1);
        assert_eq!(progress.bytes(), NOTIFY_EVERY / 2 + NOTIFY_EVERY);
    }

    #[test]
    fn the_destination_is_next_to_the_archive() {
        assert_eq!(
            destination_for(Path::new("/home/u/Downloads/pack.tar.xz")),
            Some(PathBuf::from("/home/u/Downloads/pack"))
        );
        assert_eq!(destination_for(Path::new("/home/u/notes.txt")), None);
    }
}
