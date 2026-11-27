//! What the Software window lists: the verified catalogue's apps for this
//! machine plus every AppImage already installed, each with the action it
//! offers. Loading never trusts an unsigned or unverifiable catalogue; it
//! reports why instead and still lists installed apps so they can be removed.

use std::fs;
use std::path::{Path, PathBuf};

use crate::install::Layout;
use crate::trust::{self, CatalogueEntry, SignedCatalogue, TrustStore};

/// Where the trust store and catalogue come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sources {
    pub trust: PathBuf,
    pub catalogue: Option<PathBuf>,
}

impl Sources {
    /// `SLOPOS_SOFTWARE_TRUST` / `SLOPOS_SOFTWARE_CATALOGUE` when set (QA),
    /// else `/etc/slopos-i/software/trust.json` and the first
    /// `slopos-i/software/catalogue.json` under `$XDG_DATA_DIRS`.
    pub fn from_environment() -> Self {
        let trust = std::env::var_os("SLOPOS_SOFTWARE_TRUST")
            .map_or_else(|| "/etc/slopos-i/software/trust.json".into(), PathBuf::from);
        let catalogue = std::env::var_os("SLOPOS_SOFTWARE_CATALOGUE")
            .map(PathBuf::from)
            .or_else(|| {
                let dirs = std::env::var_os("XDG_DATA_DIRS")
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
                std::env::split_paths(&dirs)
                    .map(|dir| dir.join("slopos-i/software/catalogue.json"))
                    .find(|path| path.is_file())
            });
        Self { trust, catalogue }
    }
}

/// The catalogue's standing, shown above the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogueState {
    /// No catalogue file is installed.
    Missing,
    /// A catalogue exists but failed to load or verify; nothing from it is
    /// offered.
    Rejected(String),
    Verified {
        publisher: String,
        key_id: String,
    },
}

impl CatalogueState {
    pub fn describe(&self) -> String {
        match self {
            Self::Missing => {
                "No software catalogue is installed. Installed apps are listed below.".into()
            }
            Self::Rejected(why) => format!("The software catalogue was rejected: {why}"),
            Self::Verified { publisher, key_id } => {
                format!("Catalogue from {publisher}, signature verified (key {key_id}).")
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Install,
    Update,
    /// Installed and current (or not in the catalogue): only removal.
    UpToDate,
}

/// One listed application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub description: String,
    pub category: String,
    pub icon: String,
    pub publisher: String,
    pub license: String,
    /// The verified catalogue entry, when the app is offered.
    pub entry: Option<CatalogueEntry>,
    pub installed_version: Option<String>,
}

impl Row {
    pub fn action(&self) -> Action {
        match (&self.entry, &self.installed_version) {
            (Some(_), None) => Action::Install,
            (Some(entry), Some(installed)) if *installed != entry.version => Action::Update,
            _ => Action::UpToDate,
        }
    }

    pub fn is_installed(&self) -> bool {
        self.installed_version.is_some()
    }

    /// The version line: "1.2", "1.0 installed, 1.2 available", ….
    pub fn version_text(&self) -> String {
        match (&self.entry, &self.installed_version) {
            (Some(entry), Some(installed)) if *installed != entry.version => {
                format!("{installed} installed, {} available", entry.version)
            }
            (Some(entry), Some(_)) => format!("{} (installed)", entry.version),
            (Some(entry), None) => entry.version.clone(),
            (None, Some(installed)) if installed.is_empty() => "installed".into(),
            (None, Some(installed)) => format!("{installed} (installed)"),
            (None, None) => String::new(),
        }
    }
}

/// `std::env::consts::ARCH` in catalogue terms.
pub fn machine_architecture() -> &'static str {
    std::env::consts::ARCH
}

/// Load and verify the catalogue, then merge in installed AppImages.
pub fn load(sources: &Sources, layout: &Layout, architecture: &str) -> (CatalogueState, Vec<Row>) {
    let (state, entries) = match &sources.catalogue {
        None => (CatalogueState::Missing, Vec::new()),
        Some(path) => match verified(&sources.trust, path) {
            Ok(catalogue) => (
                CatalogueState::Verified {
                    publisher: catalogue.publisher.clone(),
                    key_id: catalogue.key_id.clone(),
                },
                catalogue
                    .for_architecture(architecture)
                    .into_iter()
                    .cloned()
                    .collect(),
            ),
            Err(why) => (CatalogueState::Rejected(why), Vec::new()),
        },
    };
    let mut rows: Vec<Row> = entries
        .into_iter()
        .map(|entry| Row {
            id: entry.id.clone(),
            name: entry.name.clone(),
            summary: entry.summary.clone(),
            description: entry.description.clone(),
            category: entry.category.clone(),
            icon: entry.icon.clone(),
            publisher: entry.publisher.clone(),
            license: entry.license.clone(),
            installed_version: None,
            entry: Some(entry),
        })
        .collect();
    for installed in installed_apps(layout) {
        match rows.iter_mut().find(|row| row.id == installed.id) {
            Some(row) => row.installed_version = installed.installed_version,
            None => rows.push(installed),
        }
    }
    rows.sort_by_key(|row| row.name.to_lowercase());
    (state, rows)
}

fn verified(trust_path: &Path, catalogue_path: &Path) -> Result<SignedCatalogue, String> {
    let trust = TrustStore::load(trust_path).map_err(|error| error.to_string())?;
    let bytes =
        trust::read_protected(catalogue_path, "catalogue").map_err(|error| error.to_string())?;
    let catalogue = SignedCatalogue::from_json(&bytes).map_err(|error| error.to_string())?;
    catalogue
        .verify(&trust)
        .map_err(|error| error.to_string())?;
    Ok(catalogue)
}

/// AppImages in the install folder, described by the desktop entries the
/// installer wrote.
fn installed_apps(layout: &Layout) -> Vec<Row> {
    let Ok(files) = fs::read_dir(&layout.apps) else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for file in files.filter_map(Result::ok) {
        let name = file.file_name().to_string_lossy().into_owned();
        let Some(id) = name.strip_suffix(".AppImage") else {
            continue;
        };
        if !layout.is_installed(id) {
            continue;
        }
        let entry = fs::read_to_string(layout.desktop_entry(id)).unwrap_or_default();
        let field = |key: &str| {
            entry
                .lines()
                .find_map(|line| line.strip_prefix(key)?.strip_prefix('='))
                .map(str::to_owned)
                .unwrap_or_default()
        };
        let display = field("Name");
        rows.push(Row {
            id: id.to_owned(),
            name: if display.is_empty() {
                id.to_owned()
            } else {
                display
            },
            summary: field("Comment"),
            description: String::new(),
            category: field("Categories").trim_end_matches(';').to_owned(),
            icon: field("Icon"),
            publisher: field("X-SLOPOS-Publisher"),
            license: String::new(),
            entry: None,
            installed_version: Some(field("X-SLOPOS-Version")),
        });
    }
    rows
}

/// "12.3 MB" for a byte count.
pub fn size_text(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["bytes", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} bytes")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::install::{install_from, Progress};
    use crate::trust::tests::signed;
    use sha2::{Digest, Sha256};
    use std::io::Cursor;
    use std::os::unix::fs::PermissionsExt;

    /// Write a signed catalogue and trust store with safe permissions.
    pub fn write_sources(dir: &Path, catalogue: &SignedCatalogue, trust: &TrustStore) -> Sources {
        let sources = Sources {
            trust: dir.join("trust.json"),
            catalogue: Some(dir.join("catalogue.json")),
        };
        for (path, json) in [
            (&sources.trust, serde_json::to_vec(trust).unwrap()),
            (
                sources.catalogue.as_ref().unwrap(),
                serde_json::to_vec(catalogue).unwrap(),
            ),
        ] {
            fs::write(path, json).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o644)).unwrap();
        }
        sources
    }

    #[test]
    fn verified_catalogue_merges_with_installed_apps() {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::for_home(&dir.path().join("home"));
        let (mut catalogue, trust) = signed(3);
        // Install "example" 1.0 from a matching payload, plus an AppImage the
        // catalogue no longer offers.
        let mut data = b"\x7fELF".to_vec();
        data.resize(64, 1);
        let mut first = catalogue.entries[0].clone();
        first.sha256 = hex::encode(Sha256::digest(&data));
        first.size = data.len() as u64;
        install_from(
            &first,
            &mut Cursor::new(&data),
            &layout,
            &Progress::default(),
        )
        .unwrap();
        let mut gone = first.clone();
        gone.id = "retired".into();
        gone.name = "Retired Tool".into();
        install_from(
            &gone,
            &mut Cursor::new(&data),
            &layout,
            &Progress::default(),
        )
        .unwrap();
        // The catalogue now offers 2.0 of example; resign it.
        catalogue.entries[0].version = "2.0".into();
        crate::trust::signing::sign_catalogue(&mut catalogue, &crate::trust::tests::key(3));
        let sources = write_sources(dir.path(), &catalogue, &trust);

        let (state, rows) = load(&sources, &layout, "x86_64");
        assert_eq!(
            state,
            CatalogueState::Verified {
                publisher: "SLOPOS".into(),
                key_id: "slopos-2026".into()
            }
        );
        let by_id = |id: &str| rows.iter().find(|row| row.id == id).unwrap();
        assert_eq!(by_id("example").action(), Action::Update);
        assert_eq!(
            by_id("example").version_text(),
            "1.0 installed, 2.0 available"
        );
        assert_eq!(by_id("other").action(), Action::Install);
        let retired = by_id("retired");
        assert_eq!(retired.action(), Action::UpToDate);
        assert!(retired.entry.is_none() && retired.is_installed());
        assert_eq!(retired.name, "Retired Tool");
        // Other architectures are not offered.
        let (_, rows) = load(&sources, &layout, "aarch64");
        assert!(rows.iter().all(|row| row.entry.is_none()));
    }

    #[test]
    fn tampered_or_missing_catalogues_offer_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::for_home(&dir.path().join("home"));
        let (mut catalogue, trust) = signed(4);
        catalogue.entries[0].url = "https://evil.example/x.AppImage".into();
        let sources = write_sources(dir.path(), &catalogue, &trust);
        let (state, rows) = load(&sources, &layout, "x86_64");
        assert!(matches!(state, CatalogueState::Rejected(_)), "{state:?}");
        assert!(state
            .describe()
            .starts_with("The software catalogue was rejected"));
        assert!(rows.is_empty());
        // A group-writable catalogue is refused before parsing.
        let (catalogue, trust) = signed(4);
        let sources = write_sources(dir.path(), &catalogue, &trust);
        fs::set_permissions(
            sources.catalogue.as_ref().unwrap(),
            fs::Permissions::from_mode(0o666),
        )
        .unwrap();
        assert!(matches!(
            load(&sources, &layout, "x86_64").0,
            CatalogueState::Rejected(_)
        ));
        let missing = Sources {
            trust: dir.path().join("none.json"),
            catalogue: None,
        };
        assert_eq!(load(&missing, &layout, "x86_64").0, CatalogueState::Missing);
    }

    #[test]
    fn sizes_read_naturally() {
        assert_eq!(size_text(999), "999 bytes");
        assert_eq!(size_text(12_300_000), "12.3 MB");
        assert_eq!(size_text(1_500_000_000), "1.5 GB");
    }
}
