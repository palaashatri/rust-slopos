//! Compress and Extract Here, under the same rules as copy (see `ops`):
//! output is written under a `.slopos-partial-*` name and committed with a
//! no-replace rename; cancelling or failing removes only that partial item.
//!
//! Extraction never trusts the archive:
//! - entry paths that are absolute or climb with `..` are refused;
//! - symbolic links are created only after every file is written, and only
//!   when they point inside the extracted folder, so no file is ever
//!   written through a link; hard links, devices and FIFOs are skipped;
//! - modes keep only the permission bits (no setuid/setgid/sticky);
//! - the number of entries and the bytes written are bounded by the
//!   archive's size and the free space, against archive bombs.
//!
//! A single top-level folder is not nested again: "Photos.zip" holding
//! "Photos/…" gives "Photos", not "Photos/Photos".

use std::fs;
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{symlink, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};

use crate::ops::{self, io_error, Job, OpError, OpResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Zip,
    Tar,
    TarGz,
    TarXz,
    TarZst,
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
        let stem = &name[..name.len().checked_sub(suffix.len())?];
        (lower.ends_with(suffix) && !stem.is_empty()).then_some((format, stem))
    })
}

/// A safe relative path for an archive entry, or `None` to refuse it.
pub fn entry_path(raw: &Path) -> Option<PathBuf> {
    let mut path = PathBuf::new();
    for component in raw.components() {
        match component {
            Component::Normal(part) => path.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    (!path.as_os_str().is_empty() && path.as_os_str().len() <= 4096).then_some(path)
}

/// Whether a link at `link` (relative to the extraction root) pointing at
/// `target` stays inside the root.
pub fn link_stays_inside(link: &Path, target: &Path) -> bool {
    if target.is_absolute() {
        return false;
    }
    let mut depth: i64 = 0;
    let parent = link.parent().unwrap_or(Path::new(""));
    for component in parent.components().chain(target.components()) {
        match component {
            Component::Normal(_) => depth += 1,
            Component::ParentDir => depth -= 1,
            Component::CurDir => {}
            Component::RootDir | Component::Prefix(_) => return false,
        }
        if depth < 0 {
            return false;
        }
    }
    true
}

/// How much an extraction may write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub bytes: u64,
    pub entries: u64,
}

const MIB: u64 = 1024 * 1024;

impl Limits {
    /// 200 times the archive (at least 256 MiB), at most 64 GiB, and never
    /// more than the free space minus a 64 MiB reserve.
    pub fn for_archive(archive_bytes: u64, free_bytes: u64) -> Self {
        let by_ratio = archive_bytes.saturating_mul(200).max(256 * MIB);
        Self {
            bytes: by_ratio
                .min(64 * 1024 * MIB)
                .min(free_bytes.saturating_sub(64 * MIB)),
            entries: 200_000,
        }
    }
}

fn free_space(dir: &Path) -> u64 {
    let Ok(path) = std::ffi::CString::new(std::os::unix::ffi::OsStrExt::as_bytes(dir.as_os_str()))
    else {
        return u64::MAX;
    };
    // SAFETY: statvfs writes into the zeroed struct we pass.
    unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(path.as_ptr(), &mut stat) == 0 {
            (stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64)
        } else {
            u64::MAX
        }
    }
}

fn human(bytes: u64) -> String {
    if bytes >= 1024 * MIB {
        format!("{:.1} GB", bytes as f64 / (1024 * MIB) as f64)
    } else {
        format!("{} MB", bytes / MIB)
    }
}

/// Reads through to `inner`, reporting progress and stopping on cancel.
struct Progress<'a, R> {
    inner: R,
    job: &'a Job,
}

impl<R: Read> Read for Progress<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.job.is_cancelled() {
            // Not `Interrupted`: readers and `io::copy` retry that forever.
            return Err(io::Error::other("cancelled"));
        }
        let read = self.inner.read(buffer)?;
        self.job.add_done(read as u64);
        Ok(read)
    }
}

impl<R: Seek> Seek for Progress<'_, R> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.inner.seek(position)
    }
}

/// Turn a reader error into the operation's error (cancel stays cancel).
fn read_error(job: &Job, path: &Path, error: io::Error) -> OpError {
    if job.is_cancelled() {
        OpError::Cancelled
    } else {
        OpError::NotArchive(path.to_owned(), error.to_string())
    }
}

/// Writes entries into the partial folder, keeping count against limits.
struct Unpacker<'a> {
    root: PathBuf,
    archive: PathBuf,
    limits: Limits,
    written: u64,
    entries: u64,
    links: Vec<(PathBuf, PathBuf)>,
    dirs: Vec<(PathBuf, u32)>,
    job: &'a Job,
}

impl Unpacker<'_> {
    fn count(&mut self) -> OpResult<()> {
        self.job.check()?;
        self.entries += 1;
        if self.entries > self.limits.entries {
            return Err(OpError::TooBig(
                self.archive.clone(),
                format!("{} items", self.limits.entries),
            ));
        }
        Ok(())
    }

    fn dir(&mut self, relative: &Path, mode: u32) -> OpResult<()> {
        self.count()?;
        let path = self.root.join(relative);
        fs::create_dir_all(&path).map_err(io_error(&path))?;
        self.dirs.push((path, mode));
        Ok(())
    }

    fn file(&mut self, relative: &Path, mode: u32, data: &mut dyn Read) -> OpResult<()> {
        self.count()?;
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(io_error(parent))?;
        }
        let mut output = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(file) => file,
            // A repeated name: the first one stays.
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return Ok(()),
            Err(error) => return Err(io_error(&path)(error)),
        };
        let mut buffer = vec![0u8; 128 * 1024];
        loop {
            let read = match data.read(&mut buffer) {
                Ok(read) => read,
                Err(error) => return Err(read_error(self.job, &self.archive, error)),
            };
            if read == 0 {
                break;
            }
            self.written += read as u64;
            if self.written > self.limits.bytes {
                return Err(OpError::TooBig(
                    self.archive.clone(),
                    human(self.limits.bytes),
                ));
            }
            output.write_all(&buffer[..read]).map_err(io_error(&path))?;
        }
        fs::set_permissions(&path, fs::Permissions::from_mode((mode & 0o777) | 0o600))
            .map_err(io_error(&path))
    }

    fn link(&mut self, relative: &Path, target: &Path) -> OpResult<()> {
        self.count()?;
        if link_stays_inside(relative, target) {
            self.links.push((relative.to_owned(), target.to_owned()));
        }
        Ok(())
    }

    /// Links last (nothing is written through them), then folder modes.
    fn finish(self) -> OpResult<()> {
        for (relative, target) in &self.links {
            let path = self.root.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(io_error(parent))?;
            }
            // An existing name keeps the file that is already there.
            let _ = symlink(target, &path);
        }
        for (path, mode) in self.dirs.iter().rev() {
            let _ = fs::set_permissions(path, fs::Permissions::from_mode((mode & 0o777) | 0o700));
        }
        Ok(())
    }
}

fn unpack_zip(file: Progress<'_, fs::File>, unpacker: &mut Unpacker<'_>) -> OpResult<()> {
    let job = unpacker.job;
    let archive_path = unpacker.archive.clone();
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|error| OpError::NotArchive(archive_path.clone(), error.to_string()))?;
    // Refuse early when the archive itself says it is too big.
    let declared: u64 = (0..zip.len())
        .filter_map(|index| zip.by_index_raw(index).ok().map(|entry| entry.size()))
        .fold(0u64, u64::saturating_add);
    if declared > unpacker.limits.bytes {
        return Err(OpError::TooBig(archive_path, human(unpacker.limits.bytes)));
    }
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|error| match job.is_cancelled() {
                true => OpError::Cancelled,
                false => OpError::NotArchive(archive_path.clone(), error.to_string()),
            })?;
        let Some(relative) = entry.enclosed_name().as_deref().and_then(entry_path) else {
            continue;
        };
        let mode = entry.unix_mode().unwrap_or(0o644);
        if entry.is_dir() {
            unpacker.dir(&relative, mode)?;
        } else if entry.is_symlink() {
            let mut target = String::new();
            entry
                .by_ref()
                .take(4096)
                .read_to_string(&mut target)
                .map_err(|error| read_error(job, &archive_path, error))?;
            unpacker.link(&relative, Path::new(&target))?;
        } else {
            unpacker.file(&relative, mode, &mut entry)?;
        }
    }
    Ok(())
}

fn unpack_tar<R: Read>(reader: R, unpacker: &mut Unpacker<'_>) -> OpResult<()> {
    let job = unpacker.job;
    let archive_path = unpacker.archive.clone();
    let mut tar = tar::Archive::new(reader);
    let entries = tar
        .entries()
        .map_err(|error| read_error(job, &archive_path, error))?;
    for entry in entries {
        let mut entry = entry.map_err(|error| read_error(job, &archive_path, error))?;
        let header = entry.header();
        let kind = header.entry_type();
        let mode = header.mode().unwrap_or(0o644);
        let raw = entry
            .path()
            .map_err(|error| read_error(job, &archive_path, error))?
            .into_owned();
        let Some(relative) = entry_path(&raw) else {
            continue;
        };
        if kind.is_dir() {
            unpacker.dir(&relative, mode)?;
        } else if kind.is_symlink() {
            if let Some(target) = entry
                .link_name()
                .map_err(|error| read_error(job, &archive_path, error))?
            {
                unpacker.link(&relative, &target)?;
            }
        } else if kind.is_file() || kind == tar::EntryType::Continuous {
            unpacker.file(&relative, mode, &mut entry)?;
        } else if kind.is_pax_global_extensions()
            || kind.is_pax_local_extensions()
            || kind.is_gnu_longname()
            || kind.is_gnu_longlink()
        {
            // Metadata the tar reader already applied.
        } else {
            // Hard links, devices and FIFOs are not recreated.
            unpacker.count()?;
        }
    }
    Ok(())
}

/// Extract `archive` next to itself. Returns the folder created.
pub fn extract(archive: &Path, job: &Job) -> OpResult<PathBuf> {
    let name = archive
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (format, stem) = format_of(&name)
        .ok_or_else(|| OpError::NotArchive(archive.to_owned(), "unknown kind of archive".into()))?;
    let dir = archive.parent().unwrap_or(Path::new("/")).to_owned();
    let file = fs::File::open(archive).map_err(io_error(archive))?;
    let size = file.metadata().map_err(io_error(archive))?.len();
    job.set_total(size);
    let partial = ops::partial_path(&dir, stem);
    fs::create_dir(&partial).map_err(io_error(&partial))?;
    let mut unpacker = Unpacker {
        root: partial.clone(),
        archive: archive.to_owned(),
        limits: Limits::for_archive(size, free_space(&dir)),
        written: 0,
        entries: 0,
        links: Vec::new(),
        dirs: Vec::new(),
        job,
    };
    let reader = Progress { inner: file, job };
    let result = match format {
        Format::Zip => unpack_zip(reader, &mut unpacker),
        Format::Tar => unpack_tar(BufReader::new(reader), &mut unpacker),
        Format::TarGz => unpack_tar(
            flate2::read::MultiGzDecoder::new(BufReader::new(reader)),
            &mut unpacker,
        ),
        Format::TarZst => match ruzstd::StreamingDecoder::new(BufReader::new(reader)) {
            Ok(decoder) => unpack_tar(decoder, &mut unpacker),
            Err(error) => Err(OpError::NotArchive(archive.to_owned(), error.to_string())),
        },
        Format::TarXz => unpack_xz(reader, &mut unpacker),
    }
    .and_then(|()| unpacker.finish());
    if let Err(error) = result {
        let _ = ops::delete_permanently(&partial);
        return Err(error);
    }
    commit_extracted(&partial, &dir, stem)
}

/// lzma-rs decodes into a writer; a pipe turns that into the reader tar
/// needs. Dropping the reader (on an error or cancel) stops the decoder.
fn unpack_xz(reader: Progress<'_, fs::File>, unpacker: &mut Unpacker<'_>) -> OpResult<()> {
    let (pipe_reader, mut pipe_writer) = io::pipe().map_err(io_error(&unpacker.archive))?;
    let archive = unpacker.archive.clone();
    std::thread::scope(|scope| {
        let decoder = scope.spawn(move || {
            let mut input = BufReader::new(reader);
            lzma_rs::xz_decompress(&mut input, &mut pipe_writer).map_err(|error| error.to_string())
        });
        let mut pipe_reader = pipe_reader;
        let unpacked = unpack_tar(&mut pipe_reader, unpacker);
        if unpacked.is_ok() {
            // tar stops at its end marker; let the decoder finish the
            // record padding after it (bounded, in case it is not padding).
            let _ = io::copy(&mut (&mut pipe_reader).take(64 * MIB), &mut io::sink());
        }
        drop(pipe_reader);
        let decoded = decoder
            .join()
            .unwrap_or_else(|_| Err("the decoder stopped".into()));
        match (unpacked, decoded) {
            (Err(error), _) => Err(error),
            (Ok(()), Err(error)) if !unpacker.job.is_cancelled() => {
                Err(OpError::NotArchive(archive, error))
            }
            (Ok(()), Err(_)) => Err(OpError::Cancelled),
            (Ok(()), Ok(())) => Ok(()),
        }
    })
}

/// Commit the partial folder, unwrapping a single top-level folder.
fn commit_extracted(partial: &Path, dir: &Path, stem: &str) -> OpResult<PathBuf> {
    let entries: Vec<fs::DirEntry> = fs::read_dir(partial)
        .map_err(io_error(partial))?
        .flatten()
        .take(2)
        .collect();
    if let [only] = entries.as_slice() {
        if only.file_type().is_ok_and(|kind| kind.is_dir()) {
            let name = ops::unique_name(dir, &only.file_name().to_string_lossy());
            let target = dir.join(&name);
            ops::commit(&only.path(), &target, false)?;
            let _ = fs::remove_dir(partial);
            return Ok(target);
        }
    }
    let target = dir.join(ops::unique_name(dir, stem));
    ops::commit(partial, &target, false)?;
    Ok(target)
}

/// The archive's name for `sources`: "Name.zip" for one item, else
/// "Archive.zip", made unique in `dir`.
pub fn archive_name(sources: &[PathBuf], format: Format, dir: &Path) -> String {
    let extension = match format {
        Format::Zip => "zip",
        _ => "tar.gz",
    };
    let base = match sources {
        [one] => one
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Archive".into()),
        _ => "Archive".into(),
    };
    let wanted = format!("{base}.{extension}");
    if fs::symlink_metadata(dir.join(&wanted)).is_err() {
        return wanted;
    }
    (2..)
        .map(|n| format!("{base} {n}.{extension}"))
        .find(|name| fs::symlink_metadata(dir.join(name)).is_err())
        .expect("an unused name")
}

/// Everything under `source` (not following links), as (path, archive
/// name) pairs, parents before children.
fn walk(source: &Path, name: &Path, job: &Job, out: &mut Vec<(PathBuf, PathBuf)>) -> OpResult<()> {
    job.check()?;
    out.push((source.to_owned(), name.to_owned()));
    let meta = fs::symlink_metadata(source).map_err(io_error(source))?;
    if meta.is_dir() {
        let mut children: Vec<_> = fs::read_dir(source)
            .map_err(io_error(source))?
            .flatten()
            .map(|entry| entry.file_name())
            .collect();
        children.sort();
        for child in children {
            walk(&source.join(&child), &name.join(&child), job, out)?;
        }
    }
    Ok(())
}

/// Compress `sources` into a new archive in `dest`. Returns its path.
pub fn compress(sources: &[PathBuf], dest: &Path, format: Format, job: &Job) -> OpResult<PathBuf> {
    let mut items = Vec::new();
    let mut total = 0;
    for source in sources {
        let name = source
            .file_name()
            .ok_or_else(|| OpError::InvalidName(source.display().to_string(), "no file name"))?;
        walk(source, Path::new(name), job, &mut items)?;
        total += ops::tree_size(source, job)?;
    }
    job.set_total(total);
    let name = archive_name(sources, format, dest);
    let partial = ops::partial_path(dest, &name);
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&partial)
        .map_err(io_error(&partial))?;
    let written = match format {
        Format::Zip => write_zip(file, &items, &partial, job),
        _ => write_tar_gz(file, &items, &partial, job),
    };
    if let Err(error) = written {
        let _ = fs::remove_file(&partial);
        return Err(error);
    }
    let target = dest.join(&name);
    ops::commit(&partial, &target, false)?;
    Ok(target)
}

fn write_zip(
    file: fs::File,
    items: &[(PathBuf, PathBuf)],
    partial: &Path,
    job: &Job,
) -> OpResult<()> {
    let failed = |error: zip::result::ZipError| OpError::Io {
        path: partial.to_owned(),
        source: io::Error::other(error.to_string()),
    };
    let mut zip = zip::ZipWriter::new(io::BufWriter::new(file));
    for (path, name) in items {
        job.check()?;
        let meta = fs::symlink_metadata(path).map_err(io_error(path))?;
        let entry_name = name.to_string_lossy().into_owned();
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(meta.mode() & 0o777)
            .large_file(meta.len() >= u64::from(u32::MAX));
        if meta.file_type().is_symlink() {
            let target = fs::read_link(path).map_err(io_error(path))?;
            zip.add_symlink(entry_name, target.to_string_lossy(), options)
                .map_err(failed)?;
        } else if meta.is_dir() {
            zip.add_directory(format!("{entry_name}/"), options)
                .map_err(failed)?;
        } else if meta.is_file() {
            zip.start_file(entry_name, options).map_err(failed)?;
            let mut input = Progress {
                inner: fs::File::open(path).map_err(io_error(path))?,
                job,
            };
            io::copy(&mut input, &mut zip).map_err(|error| match job.is_cancelled() {
                true => OpError::Cancelled,
                false => io_error(path)(error),
            })?;
        } else {
            return Err(OpError::Unsupported(path.clone()));
        }
    }
    let mut writer = zip.finish().map_err(failed)?;
    writer.flush().map_err(io_error(partial))?;
    writer
        .into_inner()
        .map_err(|error| io_error(partial)(error.into_error()))?
        .sync_all()
        .map_err(io_error(partial))
}

fn write_tar_gz(
    file: fs::File,
    items: &[(PathBuf, PathBuf)],
    partial: &Path,
    job: &Job,
) -> OpResult<()> {
    let encoder =
        flate2::write::GzEncoder::new(io::BufWriter::new(file), flate2::Compression::default());
    let mut tar = tar::Builder::new(encoder);
    tar.follow_symlinks(false);
    for (path, name) in items {
        job.check()?;
        let meta = fs::symlink_metadata(path).map_err(io_error(path))?;
        let mut header = tar::Header::new_gnu();
        header.set_metadata_in_mode(&meta, tar::HeaderMode::Deterministic);
        header.set_mtime(meta.mtime().max(0) as u64);
        header.set_mode(meta.mode() & 0o777);
        let result = if meta.file_type().is_symlink() {
            let target = fs::read_link(path).map_err(io_error(path))?;
            header.set_size(0);
            tar.append_link(&mut header, name, &target)
        } else if meta.is_dir() {
            header.set_size(0);
            tar.append_data(&mut header, name, io::empty())
        } else if meta.is_file() {
            let input = Progress {
                inner: fs::File::open(path).map_err(io_error(path))?,
                job,
            };
            tar.append_data(&mut header, name, input)
        } else {
            return Err(OpError::Unsupported(path.clone()));
        };
        result.map_err(|error| match job.is_cancelled() {
            true => OpError::Cancelled,
            false => io_error(path)(error),
        })?;
    }
    let encoder = tar.into_inner().map_err(io_error(partial))?;
    let buffered = encoder.finish().map_err(io_error(partial))?;
    buffered
        .into_inner()
        .map_err(|error| io_error(partial)(error.into_error()))?
        .sync_all()
        .map_err(io_error(partial))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(dir: &Path) -> Vec<String> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_owned()];
        while let Some(path) = stack.pop() {
            for entry in fs::read_dir(&path).unwrap().flatten() {
                let p = entry.path();
                let rel = p.strip_prefix(dir).unwrap().display().to_string();
                let meta = fs::symlink_metadata(&p).unwrap();
                if meta.file_type().is_symlink() {
                    out.push(format!("{rel} -> {}", fs::read_link(&p).unwrap().display()));
                } else if meta.is_dir() {
                    out.push(format!("{rel}/"));
                    stack.push(p);
                } else {
                    out.push(format!("{rel} ({})", fs::read_to_string(&p).unwrap()));
                }
            }
        }
        out.sort();
        out
    }

    #[test]
    fn names_paths_links_and_limits() {
        assert_eq!(format_of("Photos.zip"), Some((Format::Zip, "Photos")));
        assert_eq!(format_of("a.TAR.GZ"), Some((Format::TarGz, "a")));
        assert_eq!(format_of("b.tgz"), Some((Format::TarGz, "b")));
        assert_eq!(format_of("c.tar.zst"), Some((Format::TarZst, "c")));
        assert_eq!(format_of("d.txz"), Some((Format::TarXz, "d")));
        assert_eq!(format_of(".zip"), None);
        assert_eq!(format_of("notes.txt"), None);
        assert_eq!(entry_path(Path::new("a/./b")), Some(PathBuf::from("a/b")));
        assert_eq!(entry_path(Path::new("../etc/passwd")), None);
        assert_eq!(entry_path(Path::new("a/../../b")), None);
        assert_eq!(entry_path(Path::new("/etc/passwd")), None);
        assert_eq!(entry_path(Path::new("./")), None);
        assert!(link_stays_inside(Path::new("a/b/link"), Path::new("../c")));
        assert!(!link_stays_inside(
            Path::new("a/link"),
            Path::new("../../x")
        ));
        assert!(!link_stays_inside(Path::new("link"), Path::new("/etc")));
        assert!(!link_stays_inside(Path::new("link"), Path::new("..")));
        assert!(link_stays_inside(Path::new("link"), Path::new("a/../b")));
        let limits = Limits::for_archive(10 * MIB, u64::MAX);
        assert_eq!(limits.bytes, 2000 * MIB);
        assert_eq!(Limits::for_archive(1, u64::MAX).bytes, 256 * MIB);
        assert_eq!(
            Limits::for_archive(1 << 40, u64::MAX).bytes,
            64 * 1024 * MIB
        );
        assert_eq!(Limits::for_archive(10 * MIB, 100 * MIB).bytes, 36 * MIB);
        assert_eq!(Limits::for_archive(10 * MIB, 10 * MIB).bytes, 0);
    }

    fn sample(dir: &Path) -> PathBuf {
        let folder = dir.join("Project");
        fs::create_dir_all(folder.join("src")).unwrap();
        fs::write(folder.join("README"), "hello").unwrap();
        fs::write(folder.join("src/main.rs"), "fn main() {}").unwrap();
        symlink("README", folder.join("LINK")).unwrap();
        folder
    }

    #[test]
    fn compress_then_extract_round_trips_both_formats() {
        for format in [Format::Zip, Format::TarGz] {
            let dir = tempfile::tempdir().unwrap();
            let folder = sample(dir.path());
            let job = Job::default();
            let archive =
                compress(std::slice::from_ref(&folder), dir.path(), format, &job).unwrap();
            let expected = match format {
                Format::Zip => "Project.zip",
                _ => "Project.tar.gz",
            };
            assert_eq!(archive.file_name().unwrap(), expected);
            // Same name again: kept apart, never replaced.
            let second = compress(std::slice::from_ref(&folder), dir.path(), format, &job).unwrap();
            assert!(second
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("Project 2."));
            let before = tree(&folder);
            fs::rename(&folder, dir.path().join("original")).unwrap();
            let out = extract(&archive, &Job::default()).unwrap();
            // One top-level folder: "Project", not "Project/Project".
            assert_eq!(out, dir.path().join("Project"));
            assert_eq!(tree(&out), before, "{format:?}");
            // Extracting again picks a free name.
            let again = extract(&archive, &Job::default()).unwrap();
            assert_eq!(again, dir.path().join("Project 2"));
            assert!(fs::read_dir(dir.path())
                .unwrap()
                .flatten()
                .all(|entry| !entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".slopos-partial")));
        }
    }

    /// A tar file built by hand, so hostile entries can be written.
    fn hostile_tar(path: &Path) {
        let file = fs::File::create(path).unwrap();
        let mut tar = tar::Builder::new(file);
        let mut add = |name: &str, kind: tar::EntryType, link: Option<&str>, data: &[u8]| {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(kind);
            header.set_size(data.len() as u64);
            header.set_mode(0o4755);
            // Raw names, bypassing the builder's own path checks.
            let bytes = name.as_bytes();
            header.as_old_mut().name[..bytes.len()].copy_from_slice(bytes);
            if let Some(link) = link {
                header.set_link_name(link).unwrap();
            }
            header.set_cksum();
            tar.append(&header, data).unwrap();
        };
        add("../escape.txt", tar::EntryType::Regular, None, b"x");
        add("/abs.txt", tar::EntryType::Regular, None, b"x");
        add("out", tar::EntryType::Symlink, Some("/etc"), b"");
        add("up", tar::EntryType::Symlink, Some("../.."), b"");
        add("ok/link", tar::EntryType::Symlink, Some("../file.txt"), b"");
        add("hard", tar::EntryType::Link, Some("file.txt"), b"");
        add("file.txt", tar::EntryType::Regular, None, b"safe");
        tar.finish().unwrap();
    }

    #[test]
    fn hostile_entries_are_refused_and_links_cannot_escape() {
        let dir = tempfile::tempdir().unwrap();
        let inner = dir.path().join("inner");
        fs::create_dir(&inner).unwrap();
        let archive = inner.join("bad.tar");
        hostile_tar(&archive);
        let out = extract(&archive, &Job::default()).unwrap();
        assert_eq!(out, inner.join("bad"));
        assert_eq!(
            tree(&out),
            ["file.txt (safe)", "ok/", "ok/link -> ../file.txt"]
        );
        assert!(!dir.path().join("escape.txt").exists());
        assert!(!inner.join("escape.txt").exists());
        // setuid is dropped.
        let mode = fs::metadata(out.join("file.txt")).unwrap().mode();
        assert_eq!(mode & 0o7000, 0);
    }

    #[test]
    fn bombs_cancel_and_garbage_leave_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        // 300 MiB of zeros compresses to well under 1 MiB.
        let archive = dir.path().join("bomb.tar.gz");
        {
            let encoder = flate2::write::GzEncoder::new(
                fs::File::create(&archive).unwrap(),
                flate2::Compression::best(),
            );
            let mut tar = tar::Builder::new(encoder);
            let mut header = tar::Header::new_gnu();
            header.set_size(300 * MIB);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, "zeros", io::repeat(0).take(300 * MIB))
                .unwrap();
            tar.into_inner().unwrap().finish().unwrap();
        }
        assert!(fs::metadata(&archive).unwrap().len() < MIB);
        assert!(matches!(
            extract(&archive, &Job::default()),
            Err(OpError::TooBig(..))
        ));
        let job = Job::default();
        job.cancel();
        let folder = sample(dir.path());
        assert!(matches!(
            compress(&[folder], dir.path(), Format::Zip, &job),
            Err(OpError::Cancelled)
        ));
        let garbage = dir.path().join("garbage.zip");
        fs::write(&garbage, "not a zip").unwrap();
        assert!(matches!(
            extract(&garbage, &Job::default()),
            Err(OpError::NotArchive(..))
        ));
        let names: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            names
                .iter()
                .all(|name| !name.starts_with(".slopos-partial")),
            "{names:?}"
        );
        assert!(!names.contains(&"zeros".to_owned()) && !names.contains(&"bomb".to_owned()));
    }

    #[test]
    fn tar_xz_and_tar_zst_extract() {
        let dir = tempfile::tempdir().unwrap();
        // Build a plain tar, then compress it with the system tools when
        // present (the decoders are what is under test).
        let tar_path = dir.path().join("t.tar");
        {
            let mut tar = tar::Builder::new(fs::File::create(&tar_path).unwrap());
            let mut header = tar::Header::new_gnu();
            header.set_size(5);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, "hello.txt", &b"hello"[..])
                .unwrap();
            tar.finish().unwrap();
        }
        let mut ran = 0;
        for (tool, extension) in [("xz", "tar.xz"), ("zstd", "tar.zst")] {
            let output = dir.path().join(format!("t.{extension}"));
            let status = std::process::Command::new(tool)
                .arg("-c")
                .arg(&tar_path)
                .stdout(fs::File::create(&output).unwrap())
                .status();
            if !status.is_ok_and(|status| status.success()) {
                eprintln!("{tool} is not installed: NOT_RUN for .{extension}");
                continue;
            }
            ran += 1;
            let out = extract(&output, &Job::default()).unwrap();
            assert_eq!(fs::read_to_string(out.join("hello.txt")).unwrap(), "hello");
            fs::remove_dir_all(out).unwrap();
        }
        eprintln!("tar.xz/tar.zst formats checked: {ran}");
    }
}
