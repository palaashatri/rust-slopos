//! `.app` bundle install — authenticated metadata, SHA-256 integrity, extraction,
//! and atomic replacement.
//!
//! The legacy [`install_from_archive`] helper remains checksum-only for callers
//! that explicitly provide an already-authenticated artifact.  The App Store
//! production path must use [`install_signed_archive`], which authenticates the
//! publisher and the complete archive metadata before touching the install
//! directory.

#![allow(dead_code, unused_imports)]

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use tar::Archive;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct CatalogEntry {
    pub name: String,
    pub bundle_id: String,
    pub version: String,
    pub url: String,
    pub sha256: String,
    #[serde(default)]
    pub size: u64,
    /// Human-readable publisher identity bound to the signing key.
    #[serde(default)]
    pub publisher: String,
    /// Stable key identifier resolved through the local trust store.
    #[serde(default)]
    pub key_id: String,
    /// Hex-encoded Ed25519 signature over [`Self::archive_signing_bytes`].
    #[serde(default)]
    pub signature: String,
}

impl CatalogEntry {
    /// Canonical, signature-free bytes for this archive's signed metadata.
    ///
    /// The explicit field order prevents serde implementation details from
    /// changing the message and binds the publisher, key, URL, checksum and
    /// advertised size together.  The signature itself is deliberately absent
    /// to avoid a self-referential message.
    pub fn archive_signing_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(&ArchiveSigningPayload {
            name: &self.name,
            bundle_id: &self.bundle_id,
            version: &self.version,
            url: &self.url,
            sha256: &self.sha256,
            size: self.size,
            publisher: &self.publisher,
            key_id: &self.key_id,
        })
        .expect("archive signing payload contains only serializable strings")
    }

    fn verify(&self, trust_store: &TrustStore, artifact: &'static str) -> Result<(), InstallError> {
        verify_signed_message(
            &self.key_id,
            &self.publisher,
            &self.signature,
            &self.archive_signing_bytes(),
            artifact,
            trust_store,
        )
    }
}

#[derive(serde::Serialize)]
struct ArchiveSigningPayload<'a> {
    name: &'a str,
    bundle_id: &'a str,
    version: &'a str,
    url: &'a str,
    sha256: &'a str,
    size: u64,
    publisher: &'a str,
    key_id: &'a str,
}

/// A catalog whose publisher and complete entry list are authenticated by the
/// same trust-store key.  Individual package signatures are still required at
/// install time so a catalog cannot be used to authorize an altered archive.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct SignedCatalog {
    pub format_version: u32,
    pub publisher: String,
    pub key_id: String,
    pub entries: Vec<CatalogEntry>,
    /// Hex-encoded Ed25519 signature over [`Self::canonical_bytes`].
    pub signature: String,
}

impl SignedCatalog {
    /// Canonical, signature-free bytes for the catalog metadata.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(&CatalogSigningPayload {
            format_version: self.format_version,
            publisher: &self.publisher,
            key_id: &self.key_id,
            entries: &self.entries,
        })
        .expect("catalog signing payload contains only serializable values")
    }

    /// Verify the catalog publisher, signature and per-entry publisher binding.
    pub fn verify(&self, trust_store: &TrustStore) -> Result<(), InstallError> {
        verify_signed_message(
            &self.key_id,
            &self.publisher,
            &self.signature,
            &self.canonical_bytes(),
            "catalog",
            trust_store,
        )?;
        for entry in &self.entries {
            if entry.publisher != self.publisher || entry.key_id != self.key_id {
                return Err(InstallError::PublisherMismatch {
                    key_id: self.key_id.clone(),
                    publisher: entry.publisher.clone(),
                });
            }
        }
        Ok(())
    }
}

#[derive(serde::Serialize)]
struct CatalogSigningPayload<'a> {
    format_version: u32,
    publisher: &'a str,
    key_id: &'a str,
    entries: &'a [CatalogEntry],
}

/// One locally trusted publisher key.  Revocation is checked before signature
/// verification so a revoked publisher cannot be used even with valid bytes.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct TrustedPublisher {
    pub key_id: String,
    pub publisher: String,
    /// Hex-encoded 32-byte Ed25519 public key.
    pub public_key: String,
    #[serde(default)]
    pub revoked: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default, Eq, PartialEq)]
pub struct TrustStore {
    pub publishers: Vec<TrustedPublisher>,
}

impl TrustStore {
    /// Parse and validate a trust store without silently accepting malformed
    /// keys, duplicate IDs or empty publisher identities.
    pub fn from_json(bytes: &[u8]) -> Result<Self, InstallError> {
        let store: Self = serde_json::from_slice(bytes)
            .map_err(|error| InstallError::InvalidTrustStore(error.to_string()))?;
        let mut seen = HashSet::new();
        for publisher in &store.publishers {
            if publisher.key_id.trim().is_empty() || publisher.publisher.trim().is_empty() {
                return Err(InstallError::InvalidTrustStore(
                    "publisher key_id and publisher must be non-empty".to_string(),
                ));
            }
            if !seen.insert(publisher.key_id.clone()) {
                return Err(InstallError::InvalidTrustStore(format!(
                    "duplicate publisher key id {}",
                    publisher.key_id
                )));
            }
            let bytes = hex::decode(&publisher.public_key).map_err(|error| {
                InstallError::InvalidTrustStore(format!(
                    "publisher {} has invalid public key: {error}",
                    publisher.key_id
                ))
            })?;
            if bytes.len() != 32 {
                return Err(InstallError::InvalidTrustStore(format!(
                    "publisher {} public key must be 32 bytes",
                    publisher.key_id
                )));
            }
        }
        Ok(store)
    }

    pub fn load(path: &Path) -> Result<Self, InstallError> {
        let file_type = fs::symlink_metadata(path)
            .map_err(|error| InstallError::Io(error.to_string()))?
            .file_type();
        if file_type.is_symlink() || !file_type.is_file() {
            return Err(InstallError::InvalidTrustStore(
                "trust store must be a regular non-symlink file".to_string(),
            ));
        }
        let bytes = fs::read(path).map_err(|error| InstallError::Io(error.to_string()))?;
        #[cfg(unix)]
        {
            let mode = fs::metadata(path)
                .map_err(|error| InstallError::Io(error.to_string()))?
                .permissions()
                .mode();
            if mode & 0o077 != 0 {
                return Err(InstallError::InvalidTrustStore(format!(
                    "trust store must not be group/world writable or readable (mode {:o})",
                    mode & 0o777
                )));
            }
        }
        Self::from_json(&bytes)
    }

    fn publisher(&self, key_id: &str) -> Result<&TrustedPublisher, InstallError> {
        let publisher = self
            .publishers
            .iter()
            .find(|publisher| publisher.key_id == key_id)
            .ok_or_else(|| InstallError::UnknownKey(key_id.to_string()))?;
        if publisher.revoked {
            return Err(InstallError::RevokedKey(key_id.to_string()));
        }
        Ok(publisher)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum InstallError {
    Io(String),
    Checksum { expected: String, got: String },
    Extract(String),
    NoDotApp,
    InvalidBundle(String),
    MissingSignature { artifact: &'static str },
    UnknownKey(String),
    RevokedKey(String),
    InvalidSignature { artifact: &'static str },
    PublisherMismatch { key_id: String, publisher: String },
    InvalidTrustStore(String),
    ArchiveSizeMismatch { expected: u64, got: u64 },
}

fn verify_signed_message(
    key_id: &str,
    publisher: &str,
    signature_hex: &str,
    message: &[u8],
    artifact: &'static str,
    trust_store: &TrustStore,
) -> Result<(), InstallError> {
    if signature_hex.trim().is_empty() {
        return Err(InstallError::MissingSignature { artifact });
    }
    let trusted = trust_store.publisher(key_id)?;
    if trusted.publisher != publisher {
        return Err(InstallError::PublisherMismatch {
            key_id: key_id.to_string(),
            publisher: publisher.to_string(),
        });
    }
    let public_key_bytes = hex::decode(&trusted.public_key)
        .map_err(|_| InstallError::InvalidTrustStore(format!("invalid public key for {key_id}")))?;
    let public_key_array: [u8; 32] = public_key_bytes
        .try_into()
        .map_err(|_| InstallError::InvalidTrustStore(format!("invalid public key for {key_id}")))?;
    let verifying_key = VerifyingKey::from_bytes(&public_key_array)
        .map_err(|_| InstallError::InvalidTrustStore(format!("invalid public key for {key_id}")))?;
    let signature_bytes =
        hex::decode(signature_hex).map_err(|_| InstallError::InvalidSignature { artifact })?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|_| InstallError::InvalidSignature { artifact })?;
    verifying_key
        .verify(message, &signature)
        .map_err(|_| InstallError::InvalidSignature { artifact })
}

/// Parse a signed catalog.  Call [`SignedCatalog::verify`] with a local trust
/// store before exposing its entries to installation or the user.
pub fn parse_signed_catalog(bytes: &[u8]) -> Result<SignedCatalog, InstallError> {
    serde_json::from_slice(bytes).map_err(|error| InstallError::Io(error.to_string()))
}

/// Authenticate a package entry and then run the existing checksum/path-safe
/// installer.  Signature verification happens before `install_from_archive`
/// creates the install directory or staging tree.
pub fn install_signed_archive(
    archive: &Path,
    entry: &CatalogEntry,
    trust_store: &TrustStore,
    install_dir: &Path,
) -> Result<PathBuf, InstallError> {
    entry.verify(trust_store, "archive")?;
    if entry.size != 0 {
        let got = fs::metadata(archive)
            .map_err(|error| InstallError::Io(error.to_string()))?
            .len();
        if got != entry.size {
            return Err(InstallError::ArchiveSizeMismatch {
                expected: entry.size,
                got,
            });
        }
    }
    install_from_archive(archive, &entry.sha256, install_dir)
}

/// Verify `archive`'s sha256 == `expected` (integrity only).
///
/// Production App Store installs must call [`install_signed_archive`] first;
/// this lower-level helper intentionally does not infer authenticity from a
/// checksum.
/// Extract the `.app.tar.gz` into a staging dir and atomically rename the top-level
/// `<Name>.app` into `install_dir`. Returns the installed `<Name>.app` path.
pub fn install_from_archive(
    archive: &Path,
    expected_sha256: &str,
    install_dir: &Path,
) -> Result<PathBuf, InstallError> {
    let got = sha256_hex_file(archive)?;
    let expected = expected_sha256.to_ascii_lowercase();
    if got != expected {
        return Err(InstallError::Checksum { expected, got });
    }

    fs::create_dir_all(install_dir).map_err(|e| InstallError::Io(e.to_string()))?;
    let install_root = install_dir
        .canonicalize()
        .map_err(|e| InstallError::Io(e.to_string()))?;
    let staging = create_unique_directory(&install_root, "staging")?;

    let result = (|| {
        extract_tar_gz(archive, &staging)?;
        let bundle = validate_staged_bundle(&staging)?;
        let final_path = install_root.join(&bundle.name);
        let returned_path = install_dir.join(&bundle.name);

        replace_staged_bundle(&bundle.app_path, &final_path, &install_root)?;
        remove_path(&staging).ok();
        Ok(returned_path)
    })();

    if result.is_err() {
        remove_path(&staging).ok();
    }

    result
}

/// Parse a JSON catalog (array of `CatalogEntry`) from bytes.
pub fn parse_catalog(bytes: &[u8]) -> Result<Vec<CatalogEntry>, InstallError> {
    serde_json::from_slice(bytes).map_err(|e| InstallError::Io(e.to_string()))
}

fn sha256_hex_file(path: &Path) -> Result<String, InstallError> {
    let mut file = File::open(path).map_err(|e| InstallError::Io(e.to_string()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| InstallError::Io(e.to_string()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

fn unique_child_path(parent: &Path, kind: &str) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let sequence = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    parent.join(format!(
        ".slopos-{kind}-{}-{timestamp}-{sequence}",
        std::process::id()
    ))
}

fn create_unique_directory(parent: &Path, kind: &str) -> Result<PathBuf, InstallError> {
    for _ in 0..128 {
        let candidate = unique_child_path(parent, kind);
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(InstallError::Io(error.to_string())),
        }
    }

    Err(InstallError::Io(format!(
        "could not allocate unique {kind} directory under {}",
        parent.display()
    )))
}

fn path_exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn remove_path(path: &Path) -> Result<(), InstallError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(InstallError::Io(error.to_string())),
    };

    if metadata.file_type().is_symlink() || metadata.is_file() {
        fs::remove_file(path).map_err(|e| InstallError::Io(e.to_string()))
    } else if metadata.is_dir() {
        fs::remove_dir_all(path).map_err(|e| InstallError::Io(e.to_string()))
    } else {
        Err(InstallError::Io(format!(
            "cannot remove unsupported filesystem entry {}",
            path.display()
        )))
    }
}

fn replace_staged_bundle(
    staged_app: &Path,
    final_path: &Path,
    install_root: &Path,
) -> Result<(), InstallError> {
    replace_staged_bundle_with(
        staged_app,
        final_path,
        install_root,
        |source, destination| {
            fs::rename(source, destination).map_err(|e| InstallError::Io(e.to_string()))
        },
    )
}

fn replace_staged_bundle_with<F>(
    staged_app: &Path,
    final_path: &Path,
    install_root: &Path,
    rename: F,
) -> Result<(), InstallError>
where
    F: Fn(&Path, &Path) -> Result<(), InstallError>,
{
    let backup_path = path_exists(final_path).then(|| unique_child_path(install_root, "backup"));

    if let Some(backup_path) = backup_path.as_ref() {
        rename(final_path, backup_path).map_err(|error| {
            InstallError::Io(format!(
                "could not move the installed bundle to backup {}: {error:?}",
                backup_path.display()
            ))
        })?;
    }

    if let Err(error) = rename(staged_app, final_path) {
        if let Some(backup_path) = backup_path.as_ref() {
            if let Err(rollback_error) = rename(backup_path, final_path) {
                return Err(InstallError::Io(format!(
                    "bundle commit failed ({error:?}) and rollback failed ({rollback_error:?}); old bundle remains at {}",
                    backup_path.display()
                )));
            }
        }
        return Err(error);
    }

    // The new bundle is committed only after its rename succeeds. Removing the
    // backup is cleanup after that commit; if cleanup is interrupted, retaining
    // the backup is safer than risking the newly installed bundle.
    if let Some(backup_path) = backup_path {
        let _ = remove_path(&backup_path);
    }

    Ok(())
}

struct StagedBundle {
    app_path: PathBuf,
    name: String,
}

fn validate_staged_bundle(staging: &Path) -> Result<StagedBundle, InstallError> {
    let roots: Vec<PathBuf> = fs::read_dir(staging)
        .map_err(|e| InstallError::Io(e.to_string()))?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|e| InstallError::Io(e.to_string()))
        })
        .collect::<Result<_, _>>()?;

    if roots.len() != 1 {
        return Err(InstallError::NoDotApp);
    }

    let app_path = roots[0].clone();
    let app_metadata = fs::symlink_metadata(&app_path)
        .map_err(|e| InstallError::InvalidBundle(format!("cannot inspect bundle root: {e}")))?;
    if app_metadata.file_type().is_symlink() || !app_metadata.is_dir() {
        return Err(InstallError::InvalidBundle(
            "archive root must be one .app directory".to_string(),
        ));
    }

    let name = app_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| InstallError::InvalidBundle("bundle name is not valid UTF-8".to_string()))?
        .to_string();
    if !is_safe_bundle_name(&name) {
        return Err(InstallError::InvalidBundle(format!(
            "unsafe .app bundle name: {name}"
        )));
    }

    validate_bundle_tree(&app_path)?;

    let resources = app_path.join("Resources");
    let resources_metadata = fs::symlink_metadata(&resources)
        .map_err(|e| InstallError::InvalidBundle(format!("missing Resources directory: {e}")))?;
    if resources_metadata.file_type().is_symlink() || !resources_metadata.is_dir() {
        return Err(InstallError::InvalidBundle(
            "Resources must be a directory".to_string(),
        ));
    }

    let info_path = resources.join("Info.toml");
    let info_metadata = fs::symlink_metadata(&info_path)
        .map_err(|e| InstallError::InvalidBundle(format!("missing Resources/Info.toml: {e}")))?;
    if info_metadata.file_type().is_symlink() || !info_metadata.is_file() {
        return Err(InstallError::InvalidBundle(
            "Resources/Info.toml must be a regular file".to_string(),
        ));
    }

    let info = fs::read(&info_path).map_err(|e| {
        InstallError::InvalidBundle(format!("cannot read Resources/Info.toml: {e}"))
    })?;
    let entrypoint = parse_info_entrypoint(&info)?;
    let entrypoint_path = app_path.join(&entrypoint);
    let entrypoint_metadata = fs::symlink_metadata(&entrypoint_path).map_err(|e| {
        InstallError::InvalidBundle(format!(
            "declared entrypoint {} is missing: {e}",
            entrypoint.display()
        ))
    })?;
    if entrypoint_metadata.file_type().is_symlink() || !entrypoint_metadata.is_file() {
        return Err(InstallError::InvalidBundle(format!(
            "declared entrypoint {} must be a regular file",
            entrypoint.display()
        )));
    }

    Ok(StagedBundle { app_path, name })
}

fn validate_bundle_tree(app_path: &Path) -> Result<(), InstallError> {
    let mut directories = vec![app_path.to_path_buf()];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(&directory).map_err(|e| InstallError::Io(e.to_string()))? {
            let entry = entry.map_err(|e| InstallError::Io(e.to_string()))?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)
                .map_err(|e| InstallError::InvalidBundle(format!("cannot inspect entry: {e}")))?;

            if metadata.file_type().is_symlink() {
                return Err(InstallError::InvalidBundle(format!(
                    "bundle contains a symlink: {}",
                    path.display()
                )));
            }
            if metadata.is_dir() {
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default();
                if has_app_suffix(name) {
                    return Err(InstallError::InvalidBundle(format!(
                        "nested .app bundle is not allowed: {}",
                        path.display()
                    )));
                }
                directories.push(path);
            } else if !metadata.is_file() {
                return Err(InstallError::InvalidBundle(format!(
                    "bundle contains unsupported filesystem entry: {}",
                    path.display()
                )));
            }
        }
    }

    Ok(())
}

fn has_app_suffix(name: &str) -> bool {
    name.len() > ".app".len() && name.to_ascii_lowercase().ends_with(".app")
}

fn is_safe_bundle_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".app") else {
        return false;
    };
    if stem.is_empty() || stem == "." || stem == ".." || stem.starts_with('.') {
        return false;
    }
    if name.contains('/')
        || name.contains('\\')
        || name.chars().any(|character| character.is_control())
        || name.ends_with('.')
        || name.ends_with(' ')
    {
        return false;
    }

    !matches!(
        stem.to_ascii_uppercase().as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

fn parse_info_entrypoint(info: &[u8]) -> Result<PathBuf, InstallError> {
    let text = std::str::from_utf8(info).map_err(|e| {
        InstallError::InvalidBundle(format!("Resources/Info.toml is not UTF-8: {e}"))
    })?;
    let mut seen_keys = HashSet::new();
    let mut required: [Option<String>; 4] = [None, None, None, None];

    for (line_number, line) in text.lines().enumerate() {
        let line = strip_toml_comment(line).trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            return Err(InstallError::InvalidBundle(format!(
                "Info.toml tables are not allowed (line {})",
                line_number + 1
            )));
        }

        let (key, value) = split_toml_assignment(line).ok_or_else(|| {
            InstallError::InvalidBundle(format!(
                "invalid Info.toml assignment (line {})",
                line_number + 1
            ))
        })?;
        let key = key.trim();
        if key.is_empty() || !seen_keys.insert(key.to_string()) {
            return Err(InstallError::InvalidBundle(format!(
                "duplicate or empty Info.toml key `{key}` (line {})",
                line_number + 1
            )));
        }

        let required_index = match key {
            "bundle_id" => Some(0),
            "name" => Some(1),
            "version" => Some(2),
            "entrypoint" => Some(3),
            _ => None,
        };
        if let Some(index) = required_index {
            let value = parse_toml_string(value).ok_or_else(|| {
                InstallError::InvalidBundle(format!(
                    "Info.toml key `{key}` must be a string (line {})",
                    line_number + 1
                ))
            })?;
            if value.is_empty() {
                return Err(InstallError::InvalidBundle(format!(
                    "Info.toml key `{key}` must not be empty"
                )));
            }
            required[index] = Some(value);
        }
    }

    for (key, value) in [
        ("bundle_id", &required[0]),
        ("name", &required[1]),
        ("version", &required[2]),
        ("entrypoint", &required[3]),
    ] {
        if value.is_none() {
            return Err(InstallError::InvalidBundle(format!(
                "Info.toml is missing required key `{key}`"
            )));
        }
    }

    let entrypoint = required[3].as_ref().expect("checked above");
    let entrypoint_path = normalized_relative_path(Path::new(entrypoint)).map_err(|reason| {
        InstallError::InvalidBundle(format!("unsafe declared entrypoint: {reason}"))
    })?;
    if entrypoint_path == Path::new("Resources/Info.toml") {
        return Err(InstallError::InvalidBundle(
            "declared entrypoint cannot be Resources/Info.toml".to_string(),
        ));
    }
    Ok(entrypoint_path)
}

fn strip_toml_comment(line: &str) -> &str {
    let mut quote = None;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        match quote {
            Some('"') => {
                if escaped {
                    escaped = false;
                } else if character == '\\' {
                    escaped = true;
                } else if character == '"' {
                    quote = None;
                }
            }
            Some('\'') => {
                if character == '\'' {
                    quote = None;
                }
            }
            None if character == '"' || character == '\'' => quote = Some(character),
            None if character == '#' => return &line[..index],
            None => {}
            _ => {}
        }
    }
    line
}

fn split_toml_assignment(line: &str) -> Option<(&str, &str)> {
    let mut quote = None;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        match quote {
            Some('"') => {
                if escaped {
                    escaped = false;
                } else if character == '\\' {
                    escaped = true;
                } else if character == '"' {
                    quote = None;
                }
            }
            Some('\'') => {
                if character == '\'' {
                    quote = None;
                }
            }
            None if character == '"' || character == '\'' => quote = Some(character),
            None if character == '=' => return Some((&line[..index], &line[index + 1..])),
            None => {}
            _ => {}
        }
    }
    None
}

fn parse_toml_string(value: &str) -> Option<String> {
    let value = value.trim();
    if value.starts_with('"') {
        let mut escaped = false;
        for (index, character) in value.char_indices().skip(1) {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                let literal = &value[..=index];
                if !value[index + 1..].trim().is_empty() {
                    return None;
                }
                return serde_json::from_str(literal).ok();
            }
        }
        None
    } else if let Some(value) = value.strip_prefix('\'') {
        let end = value.find('\'')?;
        if !value[end + 1..].trim().is_empty() {
            return None;
        }
        Some(value[..end].to_string())
    } else {
        None
    }
}

fn normalized_relative_path(path: &Path) -> Result<PathBuf, &'static str> {
    if path.to_string_lossy().contains('\\') {
        return Err("backslash path separators are not allowed");
    }

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                if part.to_string_lossy().ends_with(':') {
                    return Err("drive-prefixed path is not allowed");
                }
                normalized.push(part);
            }
            Component::CurDir => {}
            Component::ParentDir => return Err("parent path components are not allowed"),
            Component::RootDir | Component::Prefix(_) => {
                return Err("absolute path components are not allowed")
            }
        }
    }

    if normalized.as_os_str().is_empty() {
        return Err("empty path is not allowed");
    }
    Ok(normalized)
}

fn is_safe_tar_path(path: &Path) -> bool {
    normalized_relative_path(path).is_ok()
}

fn extract_tar_gz(archive: &Path, dest: &Path) -> Result<(), InstallError> {
    let file = File::open(archive).map_err(|e| InstallError::Io(e.to_string()))?;
    let decoder = GzDecoder::new(file);
    let mut tar = Archive::new(decoder);
    let dest_root = dest.canonicalize().map_err(|e| {
        InstallError::Extract(format!("cannot canonicalize staging directory: {e}"))
    })?;
    let mut seen_paths = HashSet::new();

    for entry in tar
        .entries()
        .map_err(|e| InstallError::Extract(e.to_string()))?
    {
        let mut entry = entry.map_err(|e| InstallError::Extract(e.to_string()))?;
        let path = entry
            .path()
            .map_err(|e| InstallError::Extract(e.to_string()))?
            .into_owned();
        let path = normalized_relative_path(&path).map_err(|reason| {
            InstallError::Extract(format!("unsafe tar path {}: {reason}", path.display()))
        })?;
        if !seen_paths.insert(path.clone()) {
            return Err(InstallError::Extract(format!(
                "duplicate tar path: {}",
                path.display()
            )));
        }

        let entry_type = entry.header().entry_type();
        if !entry_type.is_file() && !entry_type.is_dir() {
            return Err(InstallError::Extract(format!(
                "unsupported tar entry type {entry_type:?} at {}",
                path.display()
            )));
        }

        let entry_size = entry
            .header()
            .size()
            .map_err(|e| InstallError::Extract(e.to_string()))?;
        if entry_type.is_dir() {
            if entry_size != 0 {
                return Err(InstallError::Extract(format!(
                    "directory tar entry has data: {}",
                    path.display()
                )));
            }
            ensure_directory_chain(&dest_root, &path)?;
            continue;
        }

        let parent = path.parent().unwrap_or_else(|| Path::new(""));
        let parent_path = ensure_directory_chain(&dest_root, parent)?;
        let file_name = path.file_name().ok_or_else(|| {
            InstallError::Extract(format!(
                "tar file has no final component: {}",
                path.display()
            ))
        })?;
        let destination = parent_path.join(file_name);
        ensure_existing_path_inside(&dest_root, &destination)?;

        #[cfg(unix)]
        let mode = entry
            .header()
            .mode()
            .map_err(|e| InstallError::Extract(e.to_string()))?;

        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
            .map_err(|e| {
                InstallError::Extract(format!("cannot create {}: {e}", destination.display()))
            })?;
        if let Err(error) = io::copy(&mut entry, &mut output) {
            drop(output);
            fs::remove_file(&destination).ok();
            return Err(InstallError::Extract(format!(
                "cannot extract {}: {error}",
                path.display()
            )));
        }
        drop(output);

        #[cfg(unix)]
        fs::set_permissions(&destination, fs::Permissions::from_mode(mode & 0o7777)).map_err(
            |e| InstallError::Extract(format!("cannot set {} mode: {e}", path.display())),
        )?;
    }

    Ok(())
}

fn ensure_directory_chain(root: &Path, relative: &Path) -> Result<PathBuf, InstallError> {
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err(InstallError::Extract(format!(
                "invalid extraction directory component: {}",
                relative.display()
            )));
        };
        current.push(name);

        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(InstallError::Extract(format!(
                        "extraction path is not a directory: {}",
                        current.display()
                    )));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&current).map_err(|e| {
                    InstallError::Extract(format!("cannot create {}: {e}", current.display()))
                })?;
            }
            Err(error) => {
                return Err(InstallError::Extract(format!(
                    "cannot inspect {}: {error}",
                    current.display()
                )))
            }
        }

        ensure_existing_path_inside(root, &current)?;
    }
    Ok(current)
}

fn ensure_existing_path_inside(root: &Path, path: &Path) -> Result<(), InstallError> {
    let parent = path.parent().unwrap_or(root);
    let canonical_parent = parent.canonicalize().map_err(|e| {
        InstallError::Extract(format!(
            "cannot canonicalize extraction parent {}: {e}",
            parent.display()
        ))
    })?;
    if !canonical_parent.starts_with(root) {
        return Err(InstallError::Extract(format!(
            "extraction path escapes staging: {}",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::{self, Write};
    use std::sync::atomic::{AtomicU64, Ordering};
    use tar::{Builder, EntryType, Header};

    static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(0);

    fn test_work(label: &str) -> PathBuf {
        let id = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);
        let work = std::env::temp_dir().join(format!(
            "slopos_appstore_{label}_{}_{}",
            std::process::id(),
            id
        ));
        fs::remove_dir_all(&work).ok();
        fs::create_dir_all(&work).unwrap();
        work
    }

    fn sha256_bytes(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("{:x}", hasher.finalize())
    }

    fn append_entry(
        builder: &mut Builder<Vec<u8>>,
        path: &str,
        entry_type: EntryType,
        data: &[u8],
        link_name: Option<&str>,
    ) -> io::Result<()> {
        let mut header = Header::new_gnu();
        header.set_entry_type(entry_type);
        header.set_size(data.len() as u64);
        header.set_mode(if entry_type.is_dir() { 0o755 } else { 0o644 });
        if let Some(link_name) = link_name {
            header.set_link_name(link_name)?;
        }
        header.set_cksum();
        builder.append_data(&mut header, path, data)
    }

    fn append_raw_path_entry(
        builder: &mut Builder<Vec<u8>>,
        path: &str,
        entry_type: EntryType,
        data: &[u8],
    ) -> io::Result<()> {
        let mut header = Header::new_gnu();
        header.set_entry_type(entry_type);
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        let path_bytes = path.as_bytes();
        assert!(path_bytes.len() <= header.as_old().name.len());
        header.as_old_mut().name[..path_bytes.len()].copy_from_slice(path_bytes);
        header.set_cksum();
        builder.append(&header, data)
    }

    fn build_archive<F>(work: &Path, name: &str, build: F) -> (PathBuf, String)
    where
        F: FnOnce(&mut Builder<Vec<u8>>) -> io::Result<()>,
    {
        let mut builder = Builder::new(Vec::new());
        build(&mut builder).unwrap();
        let tar_bytes = builder.into_inner().unwrap();

        let archive_path = work.join(name);
        let file = File::create(&archive_path).unwrap();
        let mut encoder = GzEncoder::new(file, Compression::default());
        encoder.write_all(&tar_bytes).unwrap();
        encoder.finish().unwrap();

        let bytes = fs::read(&archive_path).unwrap();
        let sha = sha256_bytes(&bytes);
        (archive_path, sha)
    }

    fn append_valid_bundle(
        builder: &mut Builder<Vec<u8>>,
        app_name: &str,
        version: &str,
    ) -> io::Result<()> {
        append_entry(builder, app_name, EntryType::dir(), &[], None)?;
        append_entry(
            builder,
            &format!("{app_name}/Resources"),
            EntryType::dir(),
            &[],
            None,
        )?;
        append_entry(
            builder,
            &format!("{app_name}/bin"),
            EntryType::dir(),
            &[],
            None,
        )?;
        append_entry(
            builder,
            &format!("{app_name}/Resources/Info.toml"),
            EntryType::file(),
            format!(
                "bundle_id = \"com.slopos.tiny\"\nname = \"TinyApp\"\nversion = \"{version}\"\nentrypoint = \"bin/tiny\"\n"
            )
            .as_bytes(),
            None,
        )?;
        append_entry(
            builder,
            &format!("{app_name}/bin/tiny"),
            EntryType::file(),
            b"#!/bin/sh\n",
            None,
        )
    }

    fn build_tiny_app_tar_gz(work: &Path) -> (PathBuf, String) {
        let app_dir = work.join("TinyApp.app");
        fs::create_dir_all(app_dir.join("Resources")).unwrap();
        fs::create_dir_all(app_dir.join("bin")).unwrap();
        fs::write(
            app_dir.join("Resources").join("Info.toml"),
            "bundle_id=\"com.slopos.tiny\"\nname=\"TinyApp\"\nversion=\"0.1.0\"\nentrypoint=\"bin/tiny\"\n",
        )
        .unwrap();
        fs::write(app_dir.join("bin").join("tiny"), "#!/bin/sh\n").unwrap();

        let archive_path = work.join("TinyApp.app.tar.gz");
        let file = File::create(&archive_path).unwrap();
        let enc = GzEncoder::new(file, Compression::default());
        let mut tar = Builder::new(enc);
        tar.append_dir_all("TinyApp.app", &app_dir).unwrap();
        let enc = tar.into_inner().unwrap();
        enc.finish().unwrap();

        let bytes = fs::read(&archive_path).unwrap();
        let sha = sha256_bytes(&bytes);
        (archive_path, sha)
    }

    fn signing_fixture(revoked: bool) -> (SigningKey, TrustStore) {
        let signing_key = SigningKey::from_bytes(&[7u8; 32]);
        let trust_store = TrustStore {
            publishers: vec![TrustedPublisher {
                key_id: "slopos-test".to_string(),
                publisher: "SLOPOS Test Publisher".to_string(),
                public_key: hex::encode(signing_key.verifying_key().to_bytes()),
                revoked,
            }],
        };
        (signing_key, trust_store)
    }

    fn signed_entry(signing_key: &SigningKey, sha256: &str) -> CatalogEntry {
        let mut entry = CatalogEntry {
            name: "TinyApp".to_string(),
            bundle_id: "com.slopos.tiny".to_string(),
            version: "0.1.0".to_string(),
            url: "file:///tmp/TinyApp.app.tar.gz".to_string(),
            sha256: sha256.to_string(),
            size: 0,
            publisher: "SLOPOS Test Publisher".to_string(),
            key_id: "slopos-test".to_string(),
            signature: String::new(),
        };
        entry.signature = hex::encode(signing_key.sign(&entry.archive_signing_bytes()).to_bytes());
        entry
    }

    fn signed_catalog(signing_key: &SigningKey, entry: CatalogEntry) -> SignedCatalog {
        let mut catalog = SignedCatalog {
            format_version: 1,
            publisher: "SLOPOS Test Publisher".to_string(),
            key_id: "slopos-test".to_string(),
            entries: vec![entry],
            signature: String::new(),
        };
        catalog.signature = hex::encode(signing_key.sign(&catalog.canonical_bytes()).to_bytes());
        catalog
    }

    #[test]
    fn signed_catalog_canonical_json_roundtrips_and_verifies() {
        let (signing_key, trust_store) = signing_fixture(false);
        let entry = signed_entry(&signing_key, &"a".repeat(64));
        let catalog = signed_catalog(&signing_key, entry);
        let encoded = serde_json::to_vec(&catalog).expect("encode signed catalog");
        let decoded = parse_signed_catalog(&encoded).expect("decode signed catalog");

        assert_eq!(decoded, catalog);
        decoded.verify(&trust_store).expect("verify signed catalog");
    }

    #[test]
    fn signed_catalog_rejects_metadata_tampering_before_exposing_entries() {
        let (signing_key, trust_store) = signing_fixture(false);
        let entry = signed_entry(&signing_key, &"a".repeat(64));
        let mut catalog = signed_catalog(&signing_key, entry);
        catalog.entries[0].version = "9.9.9".to_string();

        assert_eq!(
            catalog.verify(&trust_store),
            Err(InstallError::InvalidSignature {
                artifact: "catalog"
            })
        );
    }

    #[cfg(unix)]
    #[test]
    fn trust_store_rejects_symlinked_files() {
        let work = test_work("trust-store-symlink");
        let target = work.join("trusted.json");
        let link = work.join("appstore-trust.json");
        fs::write(&target, br#"{"publishers":[]}"#).unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        assert!(matches!(
            TrustStore::load(&link),
            Err(InstallError::InvalidTrustStore(message))
                if message.contains("regular non-symlink")
        ));
        fs::remove_dir_all(&work).ok();
    }

    #[test]
    fn signed_archive_installs_only_after_signature_and_publisher_verification() {
        let work = test_work("signed-install");
        let install_dir = work.join("Applications");
        fs::create_dir_all(&install_dir).unwrap();
        let (archive, sha) = build_tiny_app_tar_gz(&work);
        let (signing_key, trust_store) = signing_fixture(false);
        let entry = signed_entry(&signing_key, &sha);

        let installed = install_signed_archive(&archive, &entry, &trust_store, &install_dir)
            .expect("signed archive should install");
        assert_eq!(installed, install_dir.join("TinyApp.app"));
        assert!(installed.join("Resources").join("Info.toml").is_file());
        fs::remove_dir_all(&work).ok();
    }

    #[test]
    fn signed_archive_rejects_authenticated_size_mismatch_before_extract() {
        let work = test_work("signed-size-mismatch");
        let install_dir = work.join("Applications");
        fs::create_dir_all(&install_dir).unwrap();
        fs::write(install_dir.join("sentinel"), b"untouched").unwrap();
        let (archive, sha) = build_tiny_app_tar_gz(&work);
        let (signing_key, trust_store) = signing_fixture(false);
        let mut entry = signed_entry(&signing_key, &sha);
        entry.size = fs::metadata(&archive).unwrap().len() + 1;
        entry.signature = hex::encode(signing_key.sign(&entry.archive_signing_bytes()).to_bytes());

        let error = install_signed_archive(&archive, &entry, &trust_store, &install_dir)
            .expect_err("signed size mismatch must be rejected");
        assert!(matches!(error, InstallError::ArchiveSizeMismatch { .. }));
        assert!(install_dir.join("sentinel").is_file());
        assert!(!install_dir.join("TinyApp.app").exists());
        fs::remove_dir_all(&work).ok();
    }

    #[test]
    fn signed_archive_rejects_missing_unknown_revoked_and_tampered_signatures_without_mutation() {
        let cases = [
            (
                "missing",
                None,
                false,
                false,
                InstallError::MissingSignature {
                    artifact: "archive",
                },
            ),
            (
                "unknown",
                Some("unknown-key"),
                false,
                false,
                InstallError::UnknownKey("unknown-key".to_string()),
            ),
            (
                "revoked",
                None,
                true,
                false,
                InstallError::RevokedKey("slopos-test".to_string()),
            ),
            (
                "tampered",
                None,
                false,
                true,
                InstallError::InvalidSignature {
                    artifact: "archive",
                },
            ),
        ];

        for (label, key_id, revoked, tampered, expected) in cases {
            let work = test_work(&format!("signed-failure-{label}"));
            let install_dir = work.join("Applications");
            fs::create_dir_all(&install_dir).unwrap();
            fs::write(install_dir.join("sentinel"), b"untouched").unwrap();
            let before = fs::read_dir(&install_dir)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect::<Vec<_>>();
            let (archive, sha) = build_tiny_app_tar_gz(&work);
            let (signing_key, trust_store) = signing_fixture(revoked);
            let mut entry = signed_entry(&signing_key, &sha);
            if let Some(key_id) = key_id {
                entry.key_id = key_id.to_string();
            }
            if label == "missing" {
                entry.signature.clear();
            }
            if tampered {
                entry.signature = "00".repeat(64);
            }

            let error = install_signed_archive(&archive, &entry, &trust_store, &install_dir)
                .expect_err("unauthenticated archive must be rejected");
            assert_eq!(error, expected, "failure case {label}");
            let after = fs::read_dir(&install_dir)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect::<Vec<_>>();
            assert_eq!(after, before, "failure case {label} mutated install dir");
            assert!(!install_dir.join("TinyApp.app").exists());
            fs::remove_dir_all(&work).ok();
        }
    }

    #[test]
    fn signed_archive_rejects_publisher_metadata_mismatch_before_checksum_or_extract() {
        let work = test_work("signed-publisher-mismatch");
        let install_dir = work.join("Applications");
        fs::create_dir_all(&install_dir).unwrap();
        fs::write(install_dir.join("sentinel"), b"untouched").unwrap();
        let (archive, sha) = build_tiny_app_tar_gz(&work);
        let (signing_key, trust_store) = signing_fixture(false);
        let mut entry = signed_entry(&signing_key, &sha);
        entry.publisher = "Impostor Publisher".to_string();

        let error = install_signed_archive(&archive, &entry, &trust_store, &install_dir)
            .expect_err("publisher mismatch must be rejected");
        assert_eq!(
            error,
            InstallError::PublisherMismatch {
                key_id: "slopos-test".to_string(),
                publisher: "Impostor Publisher".to_string(),
            }
        );
        assert!(install_dir.join("sentinel").is_file());
        assert!(!install_dir.join("TinyApp.app").exists());
        fs::remove_dir_all(&work).ok();
    }

    #[test]
    fn install_from_archive_roundtrip() {
        let work = test_work("install");
        let install_dir = work.join("Applications");
        fs::create_dir_all(&install_dir).unwrap();

        let (archive, sha) = build_tiny_app_tar_gz(&work);
        let installed =
            install_from_archive(&archive, &sha, &install_dir).expect("install should succeed");

        assert_eq!(installed, install_dir.join("TinyApp.app"));
        assert!(installed.join("Resources").join("Info.toml").is_file());

        fs::remove_dir_all(&work).ok();
    }

    #[test]
    fn install_from_archive_checksum_mismatch() {
        let work = test_work("checksum");
        let install_dir = work.join("Applications");
        fs::create_dir_all(&install_dir).unwrap();

        let (archive, _) = build_tiny_app_tar_gz(&work);
        let err = install_from_archive(
            &archive,
            "0000000000000000000000000000000000000000000000000000000000000000",
            &install_dir,
        )
        .unwrap_err();

        match err {
            InstallError::Checksum { expected, got } => {
                assert_eq!(
                    expected,
                    "0000000000000000000000000000000000000000000000000000000000000000"
                );
                assert_ne!(got, expected);
            }
            other => panic!("expected Checksum error, got {:?}", other),
        }

        fs::remove_dir_all(&work).ok();
    }

    #[test]
    fn extraction_rejects_absolute_and_parent_paths() {
        for (label, path) in [
            ("parent", "../../slopos_escape"),
            ("absolute", "/slopos_escape"),
        ] {
            let work = test_work(label);
            let install_dir = work.join("Applications");
            fs::create_dir_all(&install_dir).unwrap();
            let (archive, sha) = build_archive(&work, "malicious.tar.gz", |builder| {
                append_raw_path_entry(builder, path, EntryType::file(), b"escape")
            });

            let err = install_from_archive(&archive, &sha, &install_dir).unwrap_err();
            assert!(
                matches!(err, InstallError::Extract(_)),
                "unexpected error: {err:?}"
            );
            assert!(!work.join("slopos_escape").exists());
            assert!(!install_dir.join("slopos_escape").exists());
            fs::remove_dir_all(&work).ok();
        }
    }

    #[test]
    fn extraction_rejects_symlink_hardlink_and_device_entries() {
        for (label, entry_type, link_name) in [
            ("symlink", EntryType::symlink(), Some("target")),
            (
                "hardlink",
                EntryType::hard_link(),
                Some("TinyApp.app/bin/tiny"),
            ),
            ("device", EntryType::character_special(), None),
        ] {
            let work = test_work(label);
            let install_dir = work.join("Applications");
            fs::create_dir_all(&install_dir).unwrap();
            let (archive, sha) = build_archive(&work, "malicious.tar.gz", |builder| {
                append_entry(builder, "TinyApp.app/unsafe", entry_type, &[], link_name)
            });

            let err = install_from_archive(&archive, &sha, &install_dir).unwrap_err();
            let message = format!("{err:?}");
            assert!(
                matches!(err, InstallError::Extract(_))
                    && message.contains("unsupported tar entry type"),
                "unexpected error: {message}"
            );
            assert!(!install_dir.join("TinyApp.app").exists());
            assert!(!work.join("slopos_escape").exists());
            fs::remove_dir_all(&work).ok();
        }
    }

    #[test]
    fn staged_bundle_requires_manifest_and_entrypoint() {
        let work = test_work("invalid-layout");
        let install_dir = work.join("Applications");
        fs::create_dir_all(&install_dir).unwrap();
        let (archive, sha) = build_archive(&work, "invalid.tar.gz", |builder| {
            append_entry(builder, "TinyApp.app", EntryType::dir(), &[], None)?;
            append_entry(
                builder,
                "TinyApp.app/Resources",
                EntryType::dir(),
                &[],
                None,
            )
        });

        let err = install_from_archive(&archive, &sha, &install_dir).unwrap_err();
        let message = format!("{err:?}");
        assert!(
            message.starts_with("InvalidBundle("),
            "unexpected error: {message}"
        );
        assert!(!install_dir.join("TinyApp.app").exists());
        fs::remove_dir_all(&work).ok();
    }

    #[test]
    fn staged_bundle_requires_declared_entrypoint_file() {
        let work = test_work("missing-entrypoint");
        let install_dir = work.join("Applications");
        fs::create_dir_all(&install_dir).unwrap();
        let (archive, sha) = build_archive(&work, "missing-entrypoint.tar.gz", |builder| {
            append_entry(builder, "TinyApp.app", EntryType::dir(), &[], None)?;
            append_entry(
                builder,
                "TinyApp.app/Resources",
                EntryType::dir(),
                &[],
                None,
            )?;
            append_entry(
                builder,
                "TinyApp.app/Resources/Info.toml",
                EntryType::file(),
                b"bundle_id = \"com.slopos.tiny\"\nname = \"TinyApp\"\nversion = \"0.1.0\"\n",
                None,
            )
        });

        let err = install_from_archive(&archive, &sha, &install_dir).unwrap_err();
        let message = format!("{err:?}");
        assert!(
            message.starts_with("InvalidBundle("),
            "unexpected error: {message}"
        );
        assert!(!install_dir.join("TinyApp.app").exists());
        fs::remove_dir_all(&work).ok();
    }

    #[test]
    fn staged_bundle_rejects_nested_bundle_layout() {
        let work = test_work("nested-layout");
        let install_dir = work.join("Applications");
        fs::create_dir_all(&install_dir).unwrap();
        let (archive, sha) = build_archive(&work, "nested.tar.gz", |builder| {
            append_valid_bundle(builder, "TinyApp.app", "0.1.0")?;
            append_entry(
                builder,
                "TinyApp.app/Nested.app",
                EntryType::dir(),
                &[],
                None,
            )
        });

        let err = install_from_archive(&archive, &sha, &install_dir).unwrap_err();
        let message = format!("{err:?}");
        assert!(
            message.starts_with("InvalidBundle("),
            "unexpected error: {message}"
        );
        assert!(!install_dir.join("TinyApp.app").exists());
        fs::remove_dir_all(&work).ok();
    }

    #[test]
    fn staged_bundle_rejects_unsafe_app_name() {
        let work = test_work("unsafe-name");
        let install_dir = work.join("Applications");
        fs::create_dir_all(&install_dir).unwrap();
        let (archive, sha) = build_archive(&work, "unsafe-name.tar.gz", |builder| {
            append_valid_bundle(builder, ".Hidden.app", "0.1.0")
        });

        let err = install_from_archive(&archive, &sha, &install_dir).unwrap_err();
        let message = format!("{err:?}");
        assert!(
            message.starts_with("InvalidBundle("),
            "unexpected error: {message}"
        );
        assert!(!install_dir.join(".Hidden.app").exists());
        fs::remove_dir_all(&work).ok();
    }

    #[test]
    fn replacement_installs_new_version_after_validation() {
        let work = test_work("replacement");
        let install_dir = work.join("Applications");
        fs::create_dir_all(&install_dir).unwrap();

        let (old_archive, old_sha) = build_archive(&work, "old.tar.gz", |builder| {
            append_valid_bundle(builder, "TinyApp.app", "0.1.0")
        });
        install_from_archive(&old_archive, &old_sha, &install_dir).unwrap();

        let (new_archive, new_sha) = build_archive(&work, "new.tar.gz", |builder| {
            append_valid_bundle(builder, "TinyApp.app", "0.2.0")
        });
        let installed = install_from_archive(&new_archive, &new_sha, &install_dir).unwrap();

        let info = fs::read_to_string(installed.join("Resources").join("Info.toml")).unwrap();
        assert!(info.contains("version = \"0.2.0\""));
        assert!(!info.contains("version = \"0.1.0\""));
        fs::remove_dir_all(&work).ok();
    }

    #[test]
    fn replacement_failure_restores_previous_bundle() {
        let work = test_work("rollback");
        let install_dir = work.join("Applications");
        let final_path = install_dir.join("TinyApp.app");
        let staged_app = install_dir.join(".test-staging").join("TinyApp.app");
        fs::create_dir_all(final_path.join("Resources")).unwrap();
        fs::create_dir_all(staged_app.join("Resources")).unwrap();
        fs::write(final_path.join("Resources").join("version"), "old").unwrap();
        fs::write(staged_app.join("Resources").join("version"), "new").unwrap();

        let staged_for_failure = staged_app.clone();
        let result = replace_staged_bundle_with(
            &staged_app,
            &final_path,
            &install_dir,
            |source, destination| {
                if source == staged_for_failure {
                    return Err(InstallError::Io("forced commit failure".to_string()));
                }
                fs::rename(source, destination).map_err(|e| InstallError::Io(e.to_string()))
            },
        );

        assert!(result.is_err());
        assert_eq!(
            fs::read_to_string(final_path.join("Resources").join("version")).unwrap(),
            "old"
        );
        assert!(staged_app.exists());
        assert_eq!(
            fs::read_dir(&install_dir)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().contains("backup"))
                .count(),
            0
        );
        fs::remove_dir_all(&work).ok();
    }

    #[test]
    fn parse_catalog_reads_entries() {
        let json = r#"[
            {
                "name": "TextEdit",
                "bundle_id": "com.slopos.textedit",
                "version": "0.1.0",
                "url": "/tmp/TextEdit.app.tar.gz",
                "sha256": "abc123",
                "size": 42
            }
        ]"#;
        let entries = parse_catalog(json.as_bytes()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "TextEdit");
        assert_eq!(entries[0].bundle_id, "com.slopos.textedit");
        assert_eq!(entries[0].size, 42);
    }
}
