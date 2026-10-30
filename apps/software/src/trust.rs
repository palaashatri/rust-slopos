//! Signed catalogue and publisher trust (STORE-01).
//!
//! Ported from the archived App Store's `bundle_install.rs`
//! (`archive/2026-10-30-wayland-compositor-and-apps`, first-party MIT
//! source, reuse approved by the owner) and adapted to AppImage entries.
//!
//! Trust model:
//! - A [`TrustStore`] lists publisher keys (Ed25519) by id. It is a regular,
//!   non-symlink file that is not group- or world-writable. A revoked key is
//!   rejected before any signature check.
//! - A [`SignedCatalogue`] is signed as a whole by one publisher key, and
//!   every entry must name that same publisher and key.
//! - Each [`CatalogueEntry`] carries its own signature over everything in it
//!   except the signature, binding name, version, URL, SHA-256 and size, so a
//!   catalogue cannot authorise an altered download.

use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};

/// Largest catalogue or trust store accepted, in bytes.
pub const MAX_METADATA_BYTES: u64 = 4 * 1024 * 1024;
/// The catalogue format this build understands.
pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TrustError {
    #[error("can't read {0}")]
    Io(String),
    #[error("the trust store is invalid: {0}")]
    InvalidTrustStore(String),
    #[error("the catalogue is invalid: {0}")]
    InvalidCatalogue(String),
    #[error("{0} is not signed")]
    MissingSignature(&'static str),
    #[error("the signing key {0} is not trusted")]
    UnknownKey(String),
    #[error("the signing key {0} has been revoked")]
    RevokedKey(String),
    #[error("key {key_id} does not belong to “{publisher}”")]
    PublisherMismatch { key_id: String, publisher: String },
    #[error("the {0} signature is not valid")]
    BadSignature(&'static str),
}

/// One application in the catalogue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogueEntry {
    /// Stable id: lower-case letters, digits and `-`, e.g. `inkscape`.
    pub id: String,
    pub name: String,
    pub summary: String,
    #[serde(default)]
    pub description: String,
    pub version: String,
    pub category: String,
    /// Freedesktop icon name used until the app is installed.
    #[serde(default)]
    pub icon: String,
    /// `x86_64` or `aarch64`.
    pub architecture: String,
    /// HTTPS download URL of the AppImage.
    pub url: String,
    /// Lower-case hex SHA-256 of the AppImage.
    pub sha256: String,
    /// Exact size in bytes.
    pub size: u64,
    #[serde(default)]
    pub license: String,
    pub publisher: String,
    pub key_id: String,
    /// Hex Ed25519 signature over [`Self::signing_bytes`].
    #[serde(default)]
    pub signature: String,
}

impl CatalogueEntry {
    /// Canonical bytes: the entry as JSON (fixed field order) with an empty
    /// signature.
    pub fn signing_bytes(&self) -> Vec<u8> {
        let unsigned = Self {
            signature: String::new(),
            ..self.clone()
        };
        serde_json::to_vec(&unsigned).expect("entries serialise")
    }

    /// Check the fields an installer relies on before any network access.
    pub fn validate(&self) -> Result<(), TrustError> {
        let bad = |why: &str| Err(TrustError::InvalidCatalogue(format!("{}: {why}", self.id)));
        if !valid_id(&self.id) {
            return bad("invalid id");
        }
        if !self.url.starts_with("https://") {
            return bad("downloads must use HTTPS");
        }
        if self.sha256.len() != 64 || !self.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return bad("SHA-256 must be 64 hex digits");
        }
        if self.size == 0 {
            return bad("size must be known");
        }
        if !matches!(self.architecture.as_str(), "x86_64" | "aarch64") {
            return bad("unknown architecture");
        }
        Ok(())
    }

    pub fn verify(&self, trust: &TrustStore) -> Result<(), TrustError> {
        self.validate()?;
        trust.verify(
            &self.key_id,
            &self.publisher,
            &self.signature,
            &self.signing_bytes(),
            "app",
        )
    }
}

/// Ids are used in file names, so they are kept to a safe alphabet.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && !id.starts_with('-')
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedCatalogue {
    pub format_version: u32,
    pub publisher: String,
    pub key_id: String,
    /// Unix seconds when the catalogue was signed.
    pub generated: u64,
    pub entries: Vec<CatalogueEntry>,
    /// Hex Ed25519 signature over [`Self::signing_bytes`].
    #[serde(default)]
    pub signature: String,
}

impl SignedCatalogue {
    pub fn signing_bytes(&self) -> Vec<u8> {
        let unsigned = Self {
            signature: String::new(),
            ..self.clone()
        };
        serde_json::to_vec(&unsigned).expect("catalogues serialise")
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self, TrustError> {
        if bytes.len() as u64 > MAX_METADATA_BYTES {
            return Err(TrustError::InvalidCatalogue("too large".into()));
        }
        let catalogue: Self = serde_json::from_slice(bytes)
            .map_err(|error| TrustError::InvalidCatalogue(error.to_string()))?;
        if catalogue.format_version != FORMAT_VERSION {
            return Err(TrustError::InvalidCatalogue(format!(
                "unsupported format version {}",
                catalogue.format_version
            )));
        }
        let mut ids = HashSet::new();
        for entry in &catalogue.entries {
            if !ids.insert((&entry.id, &entry.architecture)) {
                return Err(TrustError::InvalidCatalogue(format!(
                    "duplicate entry {}",
                    entry.id
                )));
            }
        }
        Ok(catalogue)
    }

    /// Verify the catalogue signature, then that every entry is from the same
    /// publisher and individually signed.
    pub fn verify(&self, trust: &TrustStore) -> Result<(), TrustError> {
        trust.verify(
            &self.key_id,
            &self.publisher,
            &self.signature,
            &self.signing_bytes(),
            "catalogue",
        )?;
        for entry in &self.entries {
            if entry.publisher != self.publisher || entry.key_id != self.key_id {
                return Err(TrustError::PublisherMismatch {
                    key_id: entry.key_id.clone(),
                    publisher: entry.publisher.clone(),
                });
            }
            entry.verify(trust)?;
        }
        Ok(())
    }

    /// Entries for this machine's architecture.
    pub fn for_architecture(&self, architecture: &str) -> Vec<&CatalogueEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.architecture == architecture)
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedPublisher {
    pub key_id: String,
    pub publisher: String,
    /// Hex 32-byte Ed25519 public key.
    pub public_key: String,
    #[serde(default)]
    pub revoked: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustStore {
    pub publishers: Vec<TrustedPublisher>,
}

impl TrustStore {
    pub fn from_json(bytes: &[u8]) -> Result<Self, TrustError> {
        let store: Self = serde_json::from_slice(bytes)
            .map_err(|error| TrustError::InvalidTrustStore(error.to_string()))?;
        let mut seen = HashSet::new();
        for publisher in &store.publishers {
            if publisher.key_id.trim().is_empty() || publisher.publisher.trim().is_empty() {
                return Err(TrustError::InvalidTrustStore(
                    "empty key id or publisher".into(),
                ));
            }
            if !seen.insert(&publisher.key_id) {
                return Err(TrustError::InvalidTrustStore(format!(
                    "duplicate key {}",
                    publisher.key_id
                )));
            }
            let key = hex::decode(&publisher.public_key).map_err(|_| {
                TrustError::InvalidTrustStore(format!("key {} is not hex", publisher.key_id))
            })?;
            if key.len() != 32 {
                return Err(TrustError::InvalidTrustStore(format!(
                    "key {} is not 32 bytes",
                    publisher.key_id
                )));
            }
        }
        Ok(store)
    }

    /// Load from a regular file that no other user can modify.
    pub fn load(path: &Path) -> Result<Self, TrustError> {
        Self::from_json(&read_protected(path, "trust store")?)
    }

    fn verify(
        &self,
        key_id: &str,
        publisher: &str,
        signature: &str,
        message: &[u8],
        artifact: &'static str,
    ) -> Result<(), TrustError> {
        if signature.is_empty() {
            return Err(TrustError::MissingSignature(artifact));
        }
        let trusted = self
            .publishers
            .iter()
            .find(|candidate| candidate.key_id == key_id)
            .ok_or_else(|| TrustError::UnknownKey(key_id.into()))?;
        if trusted.revoked {
            return Err(TrustError::RevokedKey(key_id.into()));
        }
        if trusted.publisher != publisher {
            return Err(TrustError::PublisherMismatch {
                key_id: key_id.into(),
                publisher: publisher.into(),
            });
        }
        let key_bytes: [u8; 32] = hex::decode(&trusted.public_key)
            .ok()
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or_else(|| TrustError::InvalidTrustStore(format!("key {key_id}")))?;
        let key = VerifyingKey::from_bytes(&key_bytes).map_err(|_| {
            TrustError::InvalidTrustStore(format!("key {key_id} is not a valid point"))
        })?;
        let signature_bytes: [u8; 64] = hex::decode(signature)
            .ok()
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or(TrustError::BadSignature(artifact))?;
        key.verify_strict(message, &Signature::from_bytes(&signature_bytes))
            .map_err(|_| TrustError::BadSignature(artifact))
    }
}

/// Read a small metadata file that must be a regular, non-symlink file not
/// writable by group or others.
pub fn read_protected(path: &Path, what: &str) -> Result<Vec<u8>, TrustError> {
    let meta =
        fs::symlink_metadata(path).map_err(|error| TrustError::Io(format!("{what}: {error}")))?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err(TrustError::Io(format!("{what} must be a regular file")));
    }
    if meta.permissions().mode() & 0o022 != 0 {
        return Err(TrustError::Io(format!(
            "{what} must not be writable by other users"
        )));
    }
    if meta.len() > MAX_METADATA_BYTES {
        return Err(TrustError::Io(format!("{what} is too large")));
    }
    fs::read(path).map_err(|error| TrustError::Io(format!("{what}: {error}")))
}

/// Signing helpers for the `slopos-software sign` tool and tests.
pub mod signing {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    pub fn public_key_hex(key: &SigningKey) -> String {
        hex::encode(key.verifying_key().to_bytes())
    }

    /// Sign every entry and then the catalogue.
    pub fn sign_catalogue(catalogue: &mut SignedCatalogue, key: &SigningKey) {
        for entry in &mut catalogue.entries {
            entry.publisher = catalogue.publisher.clone();
            entry.key_id = catalogue.key_id.clone();
            entry.signature = hex::encode(key.sign(&entry.signing_bytes()).to_bytes());
        }
        catalogue.signature = hex::encode(key.sign(&catalogue.signing_bytes()).to_bytes());
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::signing::*;
    use super::*;
    use ed25519_dalek::SigningKey;

    pub fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    pub fn entry(id: &str) -> CatalogueEntry {
        CatalogueEntry {
            id: id.into(),
            name: "Example".into(),
            summary: "An example app".into(),
            description: String::new(),
            version: "1.0".into(),
            category: "Utilities".into(),
            icon: "application-x-executable".into(),
            architecture: "x86_64".into(),
            url: "https://example.org/example.AppImage".into(),
            sha256: "ab".repeat(32),
            size: 1234,
            license: "MIT".into(),
            publisher: String::new(),
            key_id: String::new(),
            signature: String::new(),
        }
    }

    pub fn signed(key_seed: u8) -> (SignedCatalogue, TrustStore) {
        let signer = key(key_seed);
        let mut catalogue = SignedCatalogue {
            format_version: FORMAT_VERSION,
            publisher: "SLOPOS".into(),
            key_id: "slopos-2026".into(),
            generated: 1_800_000_000,
            entries: vec![entry("example"), entry("other")],
            signature: String::new(),
        };
        sign_catalogue(&mut catalogue, &signer);
        let trust = TrustStore {
            publishers: vec![TrustedPublisher {
                key_id: "slopos-2026".into(),
                publisher: "SLOPOS".into(),
                public_key: public_key_hex(&signer),
                revoked: false,
            }],
        };
        (catalogue, trust)
    }

    #[test]
    fn signed_catalogues_verify_and_round_trip_through_json() {
        let (catalogue, trust) = signed(7);
        catalogue.verify(&trust).unwrap();
        let json = serde_json::to_vec(&catalogue).unwrap();
        let parsed = SignedCatalogue::from_json(&json).unwrap();
        parsed.verify(&trust).unwrap();
        assert_eq!(parsed.for_architecture("x86_64").len(), 2);
        assert!(parsed.for_architecture("aarch64").is_empty());
    }

    #[test]
    fn tampering_with_any_field_breaks_the_signature() {
        let (catalogue, trust) = signed(7);
        let mut tampered = catalogue.clone();
        tampered.entries[0].url = "https://evil.example/x.AppImage".into();
        assert!(matches!(
            tampered.verify(&trust),
            Err(TrustError::BadSignature(_))
        ));
        let mut tampered = catalogue.clone();
        tampered.entries[1].sha256 = "cd".repeat(32);
        assert!(tampered.verify(&trust).is_err());
        let mut tampered = catalogue.clone();
        tampered.entries.pop();
        assert_eq!(
            tampered.verify(&trust),
            Err(TrustError::BadSignature("catalogue"))
        );
        // An entry signed by another key cannot be smuggled in.
        let (foreign, _) = signed(9);
        let mut mixed = catalogue.clone();
        mixed.entries[0] = foreign.entries[0].clone();
        assert!(mixed.verify(&trust).is_err());
    }

    #[test]
    fn unknown_revoked_and_mismatched_keys_are_refused() {
        let (catalogue, mut trust) = signed(7);
        let (_, other_trust) = signed(9);
        // Same key id, different public key.
        assert!(matches!(
            catalogue.verify(&other_trust),
            Err(TrustError::BadSignature(_))
        ));
        trust.publishers[0].revoked = true;
        assert_eq!(
            catalogue.verify(&trust),
            Err(TrustError::RevokedKey("slopos-2026".into()))
        );
        trust.publishers[0].revoked = false;
        trust.publishers[0].publisher = "Someone Else".into();
        assert!(matches!(
            catalogue.verify(&trust),
            Err(TrustError::PublisherMismatch { .. })
        ));
        assert_eq!(
            catalogue.verify(&TrustStore::default()),
            Err(TrustError::UnknownKey("slopos-2026".into()))
        );
        let mut unsigned = catalogue.clone();
        unsigned.signature.clear();
        assert_eq!(
            unsigned.verify(&trust),
            Err(TrustError::MissingSignature("catalogue"))
        );
    }

    #[test]
    fn entries_must_be_safe_before_any_download() {
        let mut bad = entry("example");
        bad.url = "http://example.org/x".into();
        assert!(bad.validate().is_err());
        let mut bad = entry("../etc");
        bad.id = "../etc".into();
        assert!(bad.validate().is_err());
        let mut bad = entry("example");
        bad.sha256 = "zz".repeat(32);
        assert!(bad.validate().is_err());
        let mut bad = entry("example");
        bad.size = 0;
        assert!(bad.validate().is_err());
        assert!(valid_id("gimp-2") && !valid_id("Gimp") && !valid_id("-x") && !valid_id(""));
    }

    #[test]
    fn trust_store_files_must_be_protected_and_well_formed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("trust.json");
        let (_, trust) = signed(7);
        fs::write(&path, serde_json::to_vec(&trust).unwrap()).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(TrustStore::load(&path).unwrap(), trust);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
        assert!(TrustStore::load(&path).is_err());
        let link = dir.path().join("link.json");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(TrustStore::load(&link).is_err());
        for bad in [
            r#"{"publishers":[{"key_id":"","publisher":"x","public_key":"00"}]}"#,
            r#"{"publishers":[{"key_id":"a","publisher":"x","public_key":"zz"}]}"#,
            r#"{"publishers":[{"key_id":"a","publisher":"x","public_key":"00"}]}"#,
        ] {
            assert!(TrustStore::from_json(bad.as_bytes()).is_err(), "{bad}");
        }
        assert!(SignedCatalogue::from_json(
            br#"{"format_version":2,"publisher":"x","key_id":"k","generated":0,"entries":[]}"#
        )
        .is_err());
    }
}
