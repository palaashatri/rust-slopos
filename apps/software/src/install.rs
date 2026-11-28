//! AppImage install, update and uninstall transactions (STORE-01).
//!
//! Locations match the legacy GTK catalogue, so either tool sees the other's
//! installs: `~/.local/share/slopos-i/applications/<id>.AppImage` and
//! `~/.local/share/applications/slopos-appimage-<id>.desktop`.
//!
//! An install:
//! 1. refuses an entry that did not verify against the trust store;
//! 2. streams the download to `<id>.AppImage.part`, enforcing the signed size
//!    and checking for cancellation between chunks;
//! 3. checks the SHA-256 and the ELF header;
//! 4. moves any installed version aside to `<id>.AppImage.previous`, places
//!    the new file with an atomic rename, and writes the desktop entry; if
//!    that fails the previous version is put back;
//! 5. removes the backup only after everything succeeded.
//!
//! [`recover`] repairs an interrupted transaction at start-up: a leftover
//! `.previous` with no installed file is restored, and `.part` files are
//! deleted. Nothing here needs root or touches distribution packages.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::Mutex;
use std::thread;

use sha2::{Digest, Sha256};

use crate::trust::{valid_id, CatalogueEntry, TrustStore};

/// The message an installer returns when the user cancelled it. The window
/// matches on it to say "cancelled" rather than "not installed".
pub const CANCELLED: &str = "installation was cancelled";

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("installation was cancelled")]
    Cancelled,
    /// The entry failed the checks the installer relies on, so nothing was
    /// downloaded.
    #[error("the entry is not safe to download: {0}")]
    Unsafe(String),
    #[error("“{0}” is not a valid application id")]
    InvalidId(String),
    #[error("the download failed: {0}")]
    Download(String),
    #[error("the download is larger than its signed size ({0} bytes)")]
    TooLarge(u64),
    #[error("the download is {got} bytes but its signed size is {expected}")]
    WrongSize { expected: u64, got: u64 },
    #[error("the download's SHA-256 does not match the signed catalogue")]
    Checksum,
    #[error("the download is not an AppImage (no ELF header)")]
    NotAppImage,
    #[error("{0} is not installed")]
    NotInstalled(String),
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: io::Error },
}

fn io_error(path: &Path) -> impl FnOnce(io::Error) -> InstallError + '_ {
    move |source| InstallError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Where installs go; tests point this at a temporary home.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub apps: PathBuf,
    pub desktop: PathBuf,
}

impl Layout {
    pub fn for_home(home: &Path) -> Self {
        Self {
            apps: home.join(".local/share/slopos-i/applications"),
            desktop: home.join(".local/share/applications"),
        }
    }

    pub fn from_environment() -> Self {
        let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("/tmp"), PathBuf::from);
        Self::for_home(&home)
    }

    pub fn appimage(&self, id: &str) -> PathBuf {
        self.apps.join(format!("{id}.AppImage"))
    }

    pub fn desktop_entry(&self, id: &str) -> PathBuf {
        self.desktop.join(format!("slopos-appimage-{id}.desktop"))
    }

    pub fn is_installed(&self, id: &str) -> bool {
        valid_id(id) && self.appimage(id).is_file()
    }
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

/// Cancellation and progress shared with the UI thread.
#[derive(Default)]
pub struct Progress {
    cancelled: AtomicBool,
    done: AtomicU64,
    total: AtomicU64,
    notify: std::sync::OnceLock<Box<dyn Fn() + Send + Sync>>,
    /// The channel of the download in progress, if any. `cancel` sends on it
    /// so a wait for the network returns at once.
    reader: Mutex<Option<SyncSender<Chunk>>>,
}

impl Progress {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
        let sender = self.reader.lock().ok().and_then(|guard| guard.clone());
        if let Some(sender) = sender {
            // A full channel is fine: the stream checks the flag before each read.
            let _ = sender.try_send(Chunk::Cancel);
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    pub fn fraction(&self) -> Option<f64> {
        let total = self.total.load(Ordering::Relaxed);
        (total > 0).then(|| (self.done.load(Ordering::Relaxed) as f64 / total as f64).min(1.0))
    }

    /// Called when progress moves by at least one percent.
    pub fn on_change(&self, notify: impl Fn() + Send + Sync + 'static) {
        let _ = self.notify.set(Box::new(notify));
    }

    fn advance(&self, bytes: u64) {
        let total = self.total.load(Ordering::Relaxed).max(1);
        let before = self.done.fetch_add(bytes, Ordering::Relaxed);
        if (before * 100 / total) != ((before + bytes) * 100 / total) {
            if let Some(notify) = self.notify.get() {
                notify();
            }
        }
    }
}

/// What the download's reader thread hands the installer.
enum Chunk {
    Data(Vec<u8>),
    End,
    Failed(String),
    /// Sent by [`Progress::cancel`] so a wait for the network returns at once.
    Cancel,
}

/// A response body read on its own thread. The installer waits on a channel,
/// so a cancel takes effect even while the server is silent. A reader stuck
/// on a dead connection ends when the socket does; nothing waits for it.
pub struct Chunks {
    rx: Receiver<Chunk>,
    current: Vec<u8>,
    pos: usize,
}

impl Chunks {
    /// Start reading `body`. The progress object is attached to this download.
    pub fn spawn(mut body: impl Read + Send + 'static, progress: &Progress) -> io::Result<Self> {
        let (tx, rx) = mpsc::sync_channel(8);
        if let Ok(mut reader) = progress.reader.lock() {
            *reader = Some(tx.clone());
        }
        thread::Builder::new()
            .name("software-download".into())
            .spawn(move || {
                let mut buffer = vec![0u8; 64 * 1024];
                loop {
                    let message = match body.read(&mut buffer) {
                        Ok(0) => Chunk::End,
                        Ok(read) => Chunk::Data(buffer[..read].to_vec()),
                        Err(error) => Chunk::Failed(error.to_string()),
                    };
                    let last = !matches!(message, Chunk::Data(_));
                    if tx.send(message).is_err() || last {
                        break;
                    }
                }
            })?;
        Ok(Self {
            rx,
            current: Vec::new(),
            pos: 0,
        })
    }
}

impl Read for Chunks {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        while self.pos == self.current.len() {
            match self.rx.recv() {
                Ok(Chunk::Data(bytes)) => {
                    self.current = bytes;
                    self.pos = 0;
                }
                Ok(Chunk::End) => return Ok(0),
                Ok(Chunk::Failed(message)) => return Err(io::Error::other(message)),
                Ok(Chunk::Cancel) => {
                    return Err(io::Error::new(io::ErrorKind::Interrupted, CANCELLED))
                }
                Err(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "the connection stopped",
                    ))
                }
            }
        }
        let count = buf.len().min(self.current.len() - self.pos);
        buf[..count].copy_from_slice(&self.current[self.pos..self.pos + count]);
        self.pos += count;
        Ok(count)
    }
}

/// Copy `source` into `part`, enforcing `size`, hashing and checking for
/// cancellation. Returns the hex SHA-256.
fn stream(
    source: &mut dyn Read,
    part: &Path,
    size: u64,
    progress: &Progress,
) -> Result<String, InstallError> {
    progress.total.store(size, Ordering::Relaxed);
    let mut file = File::create(part).map_err(io_error(part))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 128 * 1024];
    let mut written = 0u64;
    loop {
        if progress.is_cancelled() {
            return Err(InstallError::Cancelled);
        }
        let read = match source.read(&mut buffer) {
            Ok(read) => read,
            // A cancel wakes the wait as an error: that is a cancel, not a
            // failed download.
            Err(_) if progress.is_cancelled() => return Err(InstallError::Cancelled),
            Err(error) => return Err(InstallError::Download(error.to_string())),
        };
        if read == 0 {
            break;
        }
        written += read as u64;
        if written > size {
            return Err(InstallError::TooLarge(size));
        }
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read]).map_err(io_error(part))?;
        progress.advance(read as u64);
    }
    if written != size {
        return Err(InstallError::WrongSize {
            expected: size,
            got: written,
        });
    }
    file.sync_all().map_err(io_error(part))?;
    Ok(hex::encode(hasher.finalize()))
}

fn escape_desktop_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace(['\n', '\r'], " ")
}

fn quote_exec_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    let escaped = value
        .replace('\\', "\\\\\\\\")
        .replace('"', "\\\\\"")
        .replace('`', "\\\\`")
        .replace('$', "\\\\$")
        .replace('%', "%%");
    format!("\"{escaped}\"")
}

fn desktop_entry(entry: &CatalogueEntry, appimage: &Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName={}\nComment={}\nExec={} %F\nIcon={}\nCategories={};\nTerminal=false\nX-SLOPOS-AppImage=true\nX-SLOPOS-Version={}\nX-SLOPOS-Publisher={}\n",
        escape_desktop_value(&entry.name),
        escape_desktop_value(&entry.summary),
        quote_exec_path(appimage),
        escape_desktop_value(&entry.icon),
        escape_desktop_value(&entry.category),
        escape_desktop_value(&entry.version),
        escape_desktop_value(&entry.publisher),
    )
}

/// Install (or update) a verified `entry` reading its bytes from `source`.
/// The caller must have verified the entry against the trust store.
pub fn install_from(
    entry: &CatalogueEntry,
    source: &mut dyn Read,
    layout: &Layout,
    progress: &Progress,
) -> Result<PathBuf, InstallError> {
    if !valid_id(&entry.id) {
        return Err(InstallError::InvalidId(entry.id.clone()));
    }
    fs::create_dir_all(&layout.apps).map_err(io_error(&layout.apps))?;
    fs::create_dir_all(&layout.desktop).map_err(io_error(&layout.desktop))?;
    let target = layout.appimage(&entry.id);
    let part = with_suffix(&target, ".part");
    let previous = with_suffix(&target, ".previous");

    let staged = (|| {
        let digest = stream(source, &part, entry.size, progress)?;
        if !digest.eq_ignore_ascii_case(&entry.sha256) {
            return Err(InstallError::Checksum);
        }
        let mut magic = [0u8; 4];
        File::open(&part)
            .and_then(|mut file| file.read_exact(&mut magic))
            .map_err(io_error(&part))?;
        if magic != *b"\x7fELF" {
            return Err(InstallError::NotAppImage);
        }
        fs::set_permissions(&part, fs::Permissions::from_mode(0o755)).map_err(io_error(&part))
    })();
    if let Err(error) = staged {
        let _ = fs::remove_file(&part);
        return Err(error);
    }

    // Commit: keep the old version until the new one is fully in place.
    let had_previous = target.is_file();
    if had_previous {
        fs::rename(&target, &previous).map_err(|error| {
            let _ = fs::remove_file(&part);
            io_error(&target)(error)
        })?;
    }
    let committed = fs::rename(&part, &target)
        .map_err(io_error(&target))
        .and_then(|()| write_desktop_entry(entry, layout, &target));
    match committed {
        Ok(()) => {
            if had_previous {
                let _ = fs::remove_file(&previous);
            }
            Ok(target)
        }
        Err(error) => {
            // Roll back to exactly what was there before.
            let _ = fs::remove_file(&part);
            if had_previous {
                let _ = fs::rename(&previous, &target);
            } else {
                let _ = fs::remove_file(&target);
            }
            Err(error)
        }
    }
}

fn write_desktop_entry(
    entry: &CatalogueEntry,
    layout: &Layout,
    target: &Path,
) -> Result<(), InstallError> {
    let path = layout.desktop_entry(&entry.id);
    let part = with_suffix(&path, ".part");
    if let Err(error) = fs::write(&part, desktop_entry(entry, target)) {
        let _ = fs::remove_file(&part);
        return Err(io_error(&part)(error));
    }
    // A failed rename must not leave the staged file behind: the caller rolls
    // the app back, and nothing else cleans this up.
    fs::rename(&part, &path).map_err(|error| {
        let _ = fs::remove_file(&part);
        io_error(&path)(error)
    })
}

/// Download over HTTPS (redirects must stay HTTPS, at most 5) and install.
/// The entry must verify against `trust`, so an entry the publisher did not
/// sign is refused before any connection is made.
pub fn install(
    entry: &CatalogueEntry,
    trust: &TrustStore,
    layout: &Layout,
    progress: &Progress,
) -> Result<PathBuf, InstallError> {
    // Refuse an entry the trust store does not vouch for (unsigned, tampered,
    // unknown or revoked key, another publisher) or one that is unsafe (plain
    // HTTP, a bad id or hash, no size), before any connection is made.
    entry
        .verify(trust)
        .map_err(|error| InstallError::Unsafe(error.to_string()))?;
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 || attempt.url().scheme() != "https" {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .connect_timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|error| InstallError::Download(error.to_string()))?;
    let response = client
        .get(&entry.url)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| InstallError::Download(error.to_string()))?;
    if response.url().scheme() != "https" {
        return Err(InstallError::Download("redirected away from HTTPS".into()));
    }
    let mut body = Chunks::spawn(response, progress)
        .map_err(|error| InstallError::Download(error.to_string()))?;
    install_from(entry, &mut body, layout, progress)
}

/// Remove an installed app and its desktop entry.
pub fn uninstall(id: &str, layout: &Layout) -> Result<(), InstallError> {
    if !valid_id(id) {
        return Err(InstallError::InvalidId(id.into()));
    }
    let target = layout.appimage(id);
    if !target.is_file() {
        return Err(InstallError::NotInstalled(id.into()));
    }
    // Remove the launcher first so a half-finished uninstall never leaves a
    // menu entry pointing at nothing.
    let desktop = layout.desktop_entry(id);
    match fs::remove_file(&desktop) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_error(&desktop)(error)),
    }
    fs::remove_file(&target).map_err(io_error(&target))
}

/// Repair interrupted transactions. Returns how many items were repaired.
pub fn recover(layout: &Layout) -> usize {
    let Ok(entries) = fs::read_dir(&layout.apps) else {
        return 0;
    };
    let mut repaired = 0;
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(base) = name.strip_suffix(".AppImage.previous") {
            let target = layout.apps.join(format!("{base}.AppImage"));
            if target.is_file() {
                let _ = fs::remove_file(&path);
            } else {
                let _ = fs::rename(&path, &target);
            }
            repaired += 1;
        } else if name.ends_with(".AppImage.part") {
            let _ = fs::remove_file(&path);
            repaired += 1;
        }
    }
    repaired
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trust::tests::entry;
    use std::io::Cursor;

    fn payload(len: usize) -> Vec<u8> {
        let mut data = b"\x7fELF".to_vec();
        data.resize(len, 7);
        data
    }

    fn entry_for(data: &[u8]) -> CatalogueEntry {
        let mut entry = entry("example");
        entry.sha256 = hex::encode(Sha256::digest(data));
        entry.size = data.len() as u64;
        entry.name = "Example \"$(rm -rf ~)\"\nExec=evil".into();
        entry
    }

    fn layout() -> (tempfile::TempDir, Layout) {
        let home = tempfile::tempdir().unwrap();
        let layout = Layout::for_home(home.path());
        (home, layout)
    }

    #[test]
    fn install_update_and_uninstall() {
        let (_home, layout) = layout();
        let data = payload(300_000);
        let entry = entry_for(&data);
        let progress = Progress::default();
        let path = install_from(&entry, &mut Cursor::new(&data), &layout, &progress).unwrap();
        assert_eq!(fs::read(&path).unwrap(), data);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert_eq!(progress.fraction(), Some(1.0));
        let desktop = fs::read_to_string(layout.desktop_entry("example")).unwrap();
        // The hostile name cannot add a second Exec line.
        assert_eq!(
            desktop.lines().filter(|l| l.starts_with("Exec=")).count(),
            1
        );
        let parsed = slopos_core::desktop_entry::parse_desktop_entry(&desktop, "x");
        let _ = parsed; // Exec target may not exist on this machine.
                        // Update in place.
        let newer = payload(1000);
        let mut update = entry_for(&newer);
        update.version = "2.0".into();
        install_from(
            &update,
            &mut Cursor::new(&newer),
            &layout,
            &Progress::default(),
        )
        .unwrap();
        assert_eq!(fs::read(&path).unwrap(), newer);
        assert!(!with_suffix(&path, ".previous").exists());
        uninstall("example", &layout).unwrap();
        assert!(!path.exists() && !layout.desktop_entry("example").exists());
        assert!(matches!(
            uninstall("example", &layout),
            Err(InstallError::NotInstalled(_))
        ));
        assert!(matches!(
            uninstall("../x", &layout),
            Err(InstallError::InvalidId(_))
        ));
    }

    #[test]
    fn bad_downloads_never_replace_an_installed_version() {
        let (_home, layout) = layout();
        let good = payload(1000);
        let entry = entry_for(&good);
        install_from(
            &entry,
            &mut Cursor::new(&good),
            &layout,
            &Progress::default(),
        )
        .unwrap();
        let installed = layout.appimage("example");
        let mut corrupt = good.clone();
        corrupt[500] ^= 1;
        // Same size and ELF header, different bytes: the signed SHA-256 refuses it.
        assert!(matches!(
            install_from(
                &entry,
                &mut Cursor::new(&corrupt),
                &layout,
                &Progress::default()
            ),
            Err(InstallError::Checksum)
        ));
        let cases: Vec<(Vec<u8>, &str)> = vec![
            (corrupt, "checksum"),
            (payload(999), "short"),
            (payload(5000), "long"),
        ];
        for (data, what) in cases {
            let result = install_from(
                &entry,
                &mut Cursor::new(&data),
                &layout,
                &Progress::default(),
            );
            assert!(result.is_err(), "{what}");
            assert_eq!(fs::read(&installed).unwrap(), good, "{what}");
        }
        // Signed and correct but not an ELF file.
        let text = b"#!/bin/sh\necho not an appimage\n".to_vec();
        let mut script = entry_for(&text);
        script.id = "script".into();
        assert!(matches!(
            install_from(
                &script,
                &mut Cursor::new(&text),
                &layout,
                &Progress::default()
            ),
            Err(InstallError::NotAppImage)
        ));
        assert!(!layout.appimage("script").exists());
        let leftovers: Vec<_> = fs::read_dir(&layout.apps)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name())
            .collect();
        assert_eq!(leftovers.len(), 1, "{leftovers:?}");
    }

    #[test]
    fn cancellation_leaves_nothing_behind() {
        let (_home, layout) = layout();
        let data = payload(1_000_000);
        let entry = entry_for(&data);
        let progress = Progress::default();
        progress.cancel();
        assert!(matches!(
            install_from(&entry, &mut Cursor::new(&data), &layout, &progress),
            Err(InstallError::Cancelled)
        ));
        assert!(fs::read_dir(&layout.apps).unwrap().next().is_none());
        assert!(!layout.desktop_entry("example").exists());
    }

    /// A local listener that is never asked to accept. Any connection an
    /// install opens is queued on it, so `accept` finds it.
    fn listener() -> (std::net::TcpListener, u16) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        (listener, port)
    }

    fn assert_no_connection(listener: &std::net::TcpListener) {
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            io::ErrorKind::WouldBlock,
            "an unsafe entry reached the network"
        );
    }

    #[test]
    fn an_unsafe_entry_is_refused_before_any_connection() {
        let (listener, port) = listener();
        let (_home, layout) = layout();
        let progress = Progress::default();
        // A correctly signed entry, so only the safety checks can refuse it.
        let (signed, trust) = crate::trust::tests::signed(3);

        let mut plain = signed.entries[0].clone();
        plain.url = format!("http://127.0.0.1:{port}/example.AppImage");
        match install(&plain, &trust, &layout, &progress) {
            Err(InstallError::Unsafe(reason)) => assert!(reason.contains("HTTPS"), "{reason}"),
            other => panic!("plain HTTP was not refused: {other:?}"),
        }

        // An option-like id is refused by the same check even when the URL is HTTPS.
        let mut option_like = signed.entries[0].clone();
        option_like.id = "--rm".into();
        option_like.url = format!("https://127.0.0.1:{port}/example.AppImage");
        assert!(matches!(
            install(&option_like, &trust, &layout, &progress),
            Err(InstallError::Unsafe(_))
        ));

        assert_no_connection(&listener);
        assert!(!layout.apps.exists() || fs::read_dir(&layout.apps).unwrap().next().is_none());
    }

    #[test]
    fn an_unsigned_or_tampered_entry_is_refused_before_any_connection() {
        let (listener, port) = listener();
        let (_home, layout) = layout();
        let progress = Progress::default();
        let (signed, trust) = crate::trust::tests::signed(3);

        // Not signed at all, with an HTTPS URL that would otherwise pass.
        let mut unsigned = entry("example");
        unsigned.url = format!("https://127.0.0.1:{port}/example.AppImage");
        assert!(matches!(
            install(&unsigned, &trust, &layout, &progress),
            Err(InstallError::Unsafe(_))
        ));

        // Signed, then its URL is changed: the signature no longer holds.
        let mut tampered = signed.entries[0].clone();
        tampered.url = format!("https://127.0.0.1:{port}/other.AppImage");
        match install(&tampered, &trust, &layout, &progress) {
            Err(InstallError::Unsafe(reason)) => {
                assert!(reason.contains("signature"), "{reason}");
            }
            other => panic!("a tampered entry was not refused: {other:?}"),
        }

        // Signed with another key: the trust store holds a different key for
        // this key id, so the signature does not verify.
        let (_, other_trust) = crate::trust::tests::signed(9);
        let mut foreign = signed.entries[0].clone();
        foreign.url = format!("https://127.0.0.1:{port}/example.AppImage");
        assert!(matches!(
            install(&foreign, &other_trust, &layout, &progress),
            Err(InstallError::Unsafe(_))
        ));

        assert_no_connection(&listener);
        assert!(!layout.apps.exists() || fs::read_dir(&layout.apps).unwrap().next().is_none());
    }

    #[test]
    fn a_failed_update_rolls_back_to_the_installed_version() {
        let (_home, layout) = layout();
        let old = payload(2000);
        let old_entry = entry_for(&old);
        install_from(
            &old_entry,
            &mut Cursor::new(&old),
            &layout,
            &Progress::default(),
        )
        .unwrap();
        // The commit fails after the new app is in place: the desktop entry
        // path is a non-empty directory, so its rename cannot succeed.
        let desktop = layout.desktop_entry("example");
        fs::remove_file(&desktop).unwrap();
        fs::create_dir(&desktop).unwrap();
        fs::write(desktop.join("keep"), b"x").unwrap();

        let new = payload(3000);
        let mut update = entry_for(&new);
        update.version = "2.0".into();
        assert!(matches!(
            install_from(
                &update,
                &mut Cursor::new(&new),
                &layout,
                &Progress::default()
            ),
            Err(InstallError::Io { .. })
        ));
        assert_eq!(fs::read(layout.appimage("example")).unwrap(), old);
        assert_eq!(names(&layout.apps), ["example.AppImage"]);
        // Nothing staged is left in the menu directory.
        assert_eq!(names(&layout.desktop), ["slopos-appimage-example.desktop"]);
    }

    #[test]
    fn a_failed_first_install_leaves_nothing_behind() {
        let (_home, layout) = layout();
        fs::create_dir_all(&layout.apps).unwrap();
        fs::create_dir_all(layout.desktop_entry("example")).unwrap();
        let data = payload(1500);
        assert!(matches!(
            install_from(
                &entry_for(&data),
                &mut Cursor::new(&data),
                &layout,
                &Progress::default()
            ),
            Err(InstallError::Io { .. })
        ));
        assert!(!layout.appimage("example").exists());
        assert!(names(&layout.apps).is_empty(), "{:?}", names(&layout.apps));
        assert_eq!(names(&layout.desktop), ["slopos-appimage-example.desktop"]);
    }

    #[test]
    fn a_download_that_breaks_midway_keeps_the_installed_version() {
        struct Reset;
        impl Read for Reset {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::ConnectionReset, "reset"))
            }
        }
        let (_home, layout) = layout();
        let old = payload(2000);
        install_from(
            &entry_for(&old),
            &mut Cursor::new(&old),
            &layout,
            &Progress::default(),
        )
        .unwrap();
        let new = payload(3000);
        let mut update = entry_for(&new);
        update.version = "2.0".into();
        let mut half = Cursor::new(new[..1500].to_vec()).chain(Reset);
        assert!(matches!(
            install_from(&update, &mut half, &layout, &Progress::default()),
            Err(InstallError::Download(_))
        ));
        assert_eq!(fs::read(layout.appimage("example")).unwrap(), old);
        assert_eq!(names(&layout.apps), ["example.AppImage"]);
    }

    #[test]
    fn a_cancel_during_a_stalled_update_download_keeps_the_installed_version() {
        use std::time::{Duration, Instant};

        // The server sends the first part of the update, then goes silent.
        struct Stalled(mpsc::Receiver<()>);
        impl Read for Stalled {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                // Returns only when the test drops its sender.
                let _ = self.0.recv();
                Ok(0)
            }
        }
        let (_home, layout) = layout();
        let old = payload(2000);
        install_from(
            &entry_for(&old),
            &mut Cursor::new(&old),
            &layout,
            &Progress::default(),
        )
        .unwrap();
        let new = payload(3000);
        let mut update = entry_for(&new);
        update.version = "2.0".into();

        let (keep_stalled, stalled) = mpsc::channel::<()>();
        let progress = Progress::default();
        let mut body = Chunks::spawn(
            Cursor::new(new[..1500].to_vec()).chain(Stalled(stalled)),
            &progress,
        )
        .unwrap();
        let started = Instant::now();
        thread::scope(|scope| {
            scope.spawn(|| {
                thread::sleep(Duration::from_millis(50));
                progress.cancel();
            });
            assert!(matches!(
                install_from(&update, &mut body, &layout, &progress),
                Err(InstallError::Cancelled)
            ));
        });
        // Cancel does not wait for the silent server.
        assert!(started.elapsed() < Duration::from_secs(10));
        assert_eq!(fs::read(layout.appimage("example")).unwrap(), old);
        assert_eq!(names(&layout.apps), ["example.AppImage"]);
        drop(keep_stalled);
    }

    /// The names in a directory, sorted; empty when it does not exist.
    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    #[test]
    fn the_cancel_message_is_the_one_the_window_matches() {
        assert_eq!(InstallError::Cancelled.to_string(), CANCELLED);
    }

    #[test]
    fn recovery_restores_an_interrupted_update() {
        let (_home, layout) = layout();
        fs::create_dir_all(&layout.apps).unwrap();
        // Crash after moving the old version aside, before placing the new.
        fs::write(layout.apps.join("a.AppImage.previous"), b"old").unwrap();
        fs::write(layout.apps.join("a.AppImage.part"), b"half").unwrap();
        // Crash after placing the new version, before removing the backup.
        fs::write(layout.apps.join("b.AppImage"), b"new").unwrap();
        fs::write(layout.apps.join("b.AppImage.previous"), b"old").unwrap();
        assert_eq!(recover(&layout), 3);
        assert_eq!(fs::read(layout.appimage("a")).unwrap(), b"old");
        assert_eq!(fs::read(layout.appimage("b")).unwrap(), b"new");
        assert_eq!(fs::read_dir(&layout.apps).unwrap().count(), 2);
    }

    #[test]
    fn exec_paths_are_quoted_for_the_desktop_spec() {
        let quoted = quote_exec_path(Path::new("/home/a b/$x\"y`z%.AppImage"));
        assert_eq!(quoted, "\"/home/a b/\\\\$x\\\\\"y\\\\`z%%.AppImage\"");
    }
}
