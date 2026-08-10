//! `slopos-fonts` — shared font service, discovery, font roles, and profiles for SLOPOS-I.
//!
//! Copyright (c) 2026 Palaash Atri
//! SPDX-License-Identifier: MIT

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const MAX_FONT_FILE_BYTES: u64 = 64 * 1024 * 1024;
const DISABLED_MARKER_DIR: &str = ".disabled";

/// Smallest size emitted by the profile resolver, in logical points.
pub const MIN_FONT_SIZE: u32 = 8;
/// Largest size emitted by the profile resolver, in logical points.
pub const MAX_FONT_SIZE: u32 = 72;
/// Smallest accepted display scale for profile resolution.
pub const MIN_FONT_SCALE: f32 = 0.5;
/// Largest accepted display scale for profile resolution.
pub const MAX_FONT_SCALE: f32 = 4.0;
/// Logical family name used at the font-service recovery boundary.
///
/// `slopos-fonts` supplies this name and its selection contract only. It does
/// not embed a font face or font bytes; `slopos-render` currently supplies the
/// bitmap fallback when this family is selected.
pub const LOGICAL_RECOVERY_FONT_FAMILY: &str = "SLOPOS Embedded Recovery";
/// Short alias for the logical recovery family name.
pub const RECOVERY_FONT_FAMILY: &str = LOGICAL_RECOVERY_FONT_FAMILY;
/// Historical compatibility alias for the logical recovery family name.
///
/// Despite the old name, this constant does not imply embedded font bytes.
#[deprecated(note = "this is a logical family name; slopos-render supplies the bitmap fallback")]
pub const EMBEDDED_RECOVERY_FONT_FAMILY: &str = LOGICAL_RECOVERY_FONT_FAMILY;

/// Explicit contract between the font service and renderer recovery paths.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecoveryFallbackContract;

impl RecoveryFallbackContract {
    /// Logical family returned by `slopos-fonts` when no usable face exists.
    pub const FAMILY: &'static str = LOGICAL_RECOVERY_FONT_FAMILY;
    /// Current component that turns the logical family into visible fallback.
    pub const PROVIDER: &'static str = "slopos-render bitmap fallback";
    /// No font binary asset is embedded in this crate.
    pub const HAS_EMBEDDED_FONT_BYTES: bool = false;

    pub const fn family() -> &'static str {
        Self::FAMILY
    }

    pub const fn provider() -> &'static str {
        Self::PROVIDER
    }

    pub const fn has_embedded_font_bytes() -> bool {
        Self::HAS_EMBEDDED_FONT_BYTES
    }
}

fn is_font_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "ttf" | "otf" | "ttc"
            )
        })
}

fn safe_file_name(file_name: &str) -> bool {
    !file_name.is_empty()
        && Path::new(file_name)
            .file_name()
            .and_then(|name| name.to_str())
            == Some(file_name)
        && !file_name.chars().any(|ch| ch.is_control() || ch == '\0')
}

fn sha256_file(path: &Path) -> Result<String, FontManagerError> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut DigestWriter(&mut hasher))?;
    Ok(hex::encode(hasher.finalize()))
}

struct DigestWriter<'a>(&'a mut Sha256);

impl Write for DigestWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FontManagerError {
    #[error("font source is not a regular font file: {0}")]
    InvalidSource(String),
    #[error("font file is too large ({actual} bytes; maximum {maximum})")]
    TooLarge { actual: u64, maximum: u64 },
    #[error("font file name is unsafe: {0}")]
    UnsafeFileName(String),
    #[error("installed font was not found: {0}")]
    NotInstalled(String),
    #[error("font I/O failed: {0}")]
    Io(#[from] io::Error),
}

/// Validation failures for role specifications and display-scale resolution.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum FontResolutionError {
    #[error("font profile is invalid")]
    InvalidProfile,
    #[error("font family must not be empty, whitespace-only, or contain control characters")]
    InvalidFamily,
    #[error("font size {size} is outside the supported range {minimum}..={maximum}")]
    InvalidSize {
        size: u32,
        minimum: u32,
        maximum: u32,
    },
    #[error("font weight {weight} is outside the supported range 100..=900")]
    InvalidWeight { weight: u16 },
    #[error(
        "font scale {scale} is not finite or is outside the supported range {minimum}..={maximum}"
    )]
    InvalidScale {
        scale: f32,
        minimum: f32,
        maximum: f32,
    },
}

/// Validate a logical font size before it enters a profile configuration.
pub fn validate_font_size(size: u32) -> Result<(), FontResolutionError> {
    if (MIN_FONT_SIZE..=MAX_FONT_SIZE).contains(&size) {
        Ok(())
    } else {
        Err(FontResolutionError::InvalidSize {
            size,
            minimum: MIN_FONT_SIZE,
            maximum: MAX_FONT_SIZE,
        })
    }
}

/// Validate a display scale before resolving a profile.
pub fn validate_font_scale(scale: f32) -> Result<(), FontResolutionError> {
    if scale.is_finite() && (MIN_FONT_SCALE..=MAX_FONT_SCALE).contains(&scale) {
        Ok(())
    } else {
        Err(FontResolutionError::InvalidScale {
            scale,
            minimum: MIN_FONT_SCALE,
            maximum: MAX_FONT_SCALE,
        })
    }
}

/// Metadata and enablement state for one installed font file.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InstalledFont {
    pub file_name: String,
    pub path: PathBuf,
    pub bytes: u64,
    pub sha256: String,
    pub enabled: bool,
}

/// User-font installation and state manager.
///
/// This manager deliberately does not parse or render font tables. It owns the
/// filesystem lifecycle and exposes immutable file metadata to the future
/// font database/renderer, so a UI never has to perform unsafe path operations.
#[derive(Clone, Debug)]
pub struct FontManager {
    discovery: FontDiscoveryService,
    install_dir: PathBuf,
}

impl FontManager {
    pub fn new(install_dir: impl Into<PathBuf>) -> Self {
        Self {
            discovery: FontDiscoveryService::new(),
            install_dir: install_dir.into(),
        }
    }

    pub fn with_discovery(
        install_dir: impl Into<PathBuf>,
        discovery: FontDiscoveryService,
    ) -> Self {
        Self {
            discovery,
            install_dir: install_dir.into(),
        }
    }

    pub fn install_dir(&self) -> &Path {
        &self.install_dir
    }

    pub fn discover_system_files(&self) -> Vec<PathBuf> {
        self.discovery.discover_font_files()
    }

    pub fn installed_fonts(&self) -> Result<Vec<InstalledFont>, FontManagerError> {
        let mut fonts = Vec::new();
        if !self.install_dir.exists() {
            return Ok(fonts);
        }
        for entry in fs::read_dir(&self.install_dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() || !is_font_extension(&path) {
                continue;
            }
            fonts.push(self.describe_installed(&path)?);
        }
        fonts.sort_by(|left, right| left.file_name.cmp(&right.file_name));
        Ok(fonts)
    }

    pub fn install(&self, source: &Path) -> Result<InstalledFont, FontManagerError> {
        let (source_name, _source_size) = validate_font_source(source)?;
        fs::create_dir_all(&self.install_dir)?;
        let source_hash = sha256_file(source)?;

        for installed in self.installed_fonts()? {
            if installed.sha256 == source_hash {
                return Ok(installed);
            }
        }

        let mut file_name = source_name;
        let mut destination = self.install_dir.join(&file_name);
        if destination.exists() {
            let stem = Path::new(&file_name)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .ok_or_else(|| FontManagerError::UnsafeFileName(file_name.clone()))?;
            let extension = Path::new(&file_name)
                .extension()
                .and_then(|extension| extension.to_str())
                .ok_or_else(|| FontManagerError::UnsafeFileName(file_name.clone()))?;
            file_name = format!("{stem}-{}.{extension}", &source_hash[..8]);
            destination = self.install_dir.join(&file_name);
        }

        let temporary = self
            .install_dir
            .join(format!(".{file_name}.{}.tmp", std::process::id()));
        let mut input = fs::File::open(source)?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        io::copy(&mut input, &mut output)?;
        output.sync_all()?;
        drop(output);
        if let Err(error) = fs::rename(&temporary, &destination) {
            let _ = fs::remove_file(&temporary);
            return Err(error.into());
        }
        self.describe_installed(&destination)
    }

    pub fn set_enabled(
        &self,
        file_name: &str,
        enabled: bool,
    ) -> Result<InstalledFont, FontManagerError> {
        let font = self.require_installed(file_name)?;
        let marker_dir = self.install_dir.join(DISABLED_MARKER_DIR);
        let marker = marker_dir.join(file_name);
        if enabled {
            if marker.exists() {
                fs::remove_file(marker)?;
            }
        } else {
            fs::create_dir_all(&marker_dir)?;
            let temporary = marker_dir.join(format!(".{file_name}.{}.tmp", std::process::id()));
            let mut marker_file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            marker_file.write_all(b"disabled\n")?;
            marker_file.sync_all()?;
            drop(marker_file);
            if let Err(error) = fs::rename(&temporary, &marker) {
                let _ = fs::remove_file(&temporary);
                return Err(error.into());
            }
        }
        self.describe_installed(&font.path)
    }

    pub fn remove(&self, file_name: &str) -> Result<(), FontManagerError> {
        let font = self.require_installed(file_name)?;
        fs::remove_file(font.path)?;
        let marker = self.install_dir.join(DISABLED_MARKER_DIR).join(file_name);
        if marker.exists() {
            fs::remove_file(marker)?;
        }
        Ok(())
    }

    fn require_installed(&self, file_name: &str) -> Result<InstalledFont, FontManagerError> {
        if !safe_file_name(file_name) || !is_font_extension(Path::new(file_name)) {
            return Err(FontManagerError::UnsafeFileName(file_name.to_string()));
        }
        let path = self.install_dir.join(file_name);
        let metadata = fs::symlink_metadata(&path)
            .map_err(|_| FontManagerError::NotInstalled(file_name.to_string()))?;
        if !metadata.file_type().is_file() {
            return Err(FontManagerError::NotInstalled(file_name.to_string()));
        }
        self.describe_installed(&path)
    }

    fn describe_installed(&self, path: &Path) -> Result<InstalledFont, FontManagerError> {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.file_type().is_file() || !is_font_extension(path) {
            return Err(FontManagerError::InvalidSource(path.display().to_string()));
        }
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| FontManagerError::UnsafeFileName(path.display().to_string()))?;
        Ok(InstalledFont {
            file_name: file_name.to_string(),
            path: path.to_path_buf(),
            bytes: metadata.len(),
            sha256: sha256_file(path)?,
            enabled: !self
                .install_dir
                .join(DISABLED_MARKER_DIR)
                .join(file_name)
                .exists(),
        })
    }
}

fn validate_font_source(source: &Path) -> Result<(String, u64), FontManagerError> {
    let metadata = fs::symlink_metadata(source)
        .map_err(|_| FontManagerError::InvalidSource(source.display().to_string()))?;
    if !metadata.file_type().is_file() || !is_font_extension(source) {
        return Err(FontManagerError::InvalidSource(
            source.display().to_string(),
        ));
    }
    if metadata.len() == 0 {
        return Err(FontManagerError::InvalidSource(
            source.display().to_string(),
        ));
    }
    if metadata.len() > MAX_FONT_FILE_BYTES {
        return Err(FontManagerError::TooLarge {
            actual: metadata.len(),
            maximum: MAX_FONT_FILE_BYTES,
        });
    }
    let file_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| FontManagerError::UnsafeFileName(source.display().to_string()))?;
    if !safe_file_name(file_name) {
        return Err(FontManagerError::UnsafeFileName(file_name.to_string()));
    }
    Ok((file_name.to_string(), metadata.len()))
}

/// Standard typography roles across the SLOPOS-I desktop environment.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FontRole {
    SystemUi,
    Menu,
    WindowTitle,
    Body,
    Small,
    Monospace,
    DocumentDefault,
}

impl FontRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SystemUi => "system_ui",
            Self::Menu => "menu",
            Self::WindowTitle => "window_title",
            Self::Body => "body",
            Self::Small => "small",
            Self::Monospace => "monospace",
            Self::DocumentDefault => "document_default",
        }
    }

    pub fn default_size(self) -> f32 {
        match self {
            Self::SystemUi => 13.0,
            Self::Menu => 13.0,
            Self::WindowTitle => 13.0,
            Self::Body => 13.0,
            Self::Small => 11.0,
            Self::Monospace => 12.0,
            Self::DocumentDefault => 14.0,
        }
    }

    /// Compatibility name for the final family used when no requested or
    /// safe available family can be selected.
    pub fn generic_fallback_family(self) -> &'static str {
        self.recovery_fallback_family()
    }

    pub fn recovery_fallback_family(self) -> &'static str {
        let _ = self;
        LOGICAL_RECOVERY_FONT_FAMILY
    }

    pub fn all() -> &'static [FontRole] {
        &[
            Self::SystemUi,
            Self::Menu,
            Self::WindowTitle,
            Self::Body,
            Self::Small,
            Self::Monospace,
            Self::DocumentDefault,
        ]
    }
}

/// Pre-configured appearance typography profile.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FontProfile {
    Classic,
    #[default]
    Modern,
    Accessible,
    Custom,
}

impl FontProfile {
    /// Parse a profile using the historical lossy behavior: unknown values
    /// select the Modern profile.
    pub fn parse(s: &str) -> Self {
        Self::try_parse(s).unwrap_or(Self::Modern)
    }

    /// Parse a profile while reporting malformed input to new callers.
    pub fn try_parse(s: &str) -> Result<Self, FontResolutionError> {
        match s.trim().to_lowercase().as_str() {
            "classic" => Ok(Self::Classic),
            "modern" => Ok(Self::Modern),
            "accessible" => Ok(Self::Accessible),
            "custom" => Ok(Self::Custom),
            _ => Err(FontResolutionError::InvalidProfile),
        }
    }

    pub fn parse_lossy(s: &str) -> Self {
        Self::parse(s)
    }
}

/// Specification for a single font role (family name, size, weight).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FontRoleSpec {
    pub family: String,
    pub size: u32,
    pub weight: u16,
}

impl FontRoleSpec {
    /// Construct a role specification using the historical clamping behavior.
    pub fn new(family: impl Into<String>, size: u32, weight: u16) -> Self {
        Self {
            family: family.into(),
            size: size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE),
            weight: weight.clamp(100, 900),
        }
    }

    /// Construct a role specification after validating every caller-provided
    /// value. Invalid sizes, weights, and family names are returned to the
    /// caller; the authoritative profile path never clamps them silently.
    pub fn try_new(
        family: impl Into<String>,
        size: u32,
        weight: u16,
    ) -> Result<Self, FontResolutionError> {
        let family = family.into();
        validate_family_name(&family)?;
        let spec = Self {
            family: canonical_family_name(&family),
            size,
            weight,
        };
        spec.validate()?;
        Ok(spec)
    }

    /// Explicitly lossy constructor for callers that intentionally want
    /// bounded defaults. Profile construction and resolution do not use it.
    pub fn lossy(family: impl Into<String>, size: u32, weight: u16) -> Self {
        Self {
            family: canonical_family_name(&family.into()),
            size: size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE),
            weight: weight.clamp(100, 900),
        }
    }

    pub fn validate(&self) -> Result<(), FontResolutionError> {
        validate_family_name(&self.family)?;
        validate_font_size(self.size)?;
        if (100..=900).contains(&self.weight) {
            Ok(())
        } else {
            Err(FontResolutionError::InvalidWeight {
                weight: self.weight,
            })
        }
    }
}

fn default_role_spec(
    profile: FontProfile,
    role: FontRole,
) -> Result<FontRoleSpec, FontResolutionError> {
    let (family, size, weight) = match (profile, role) {
        // Classic keeps compact period-inspired metrics while using
        // permissively licensed family preferences only.
        (FontProfile::Classic, FontRole::SystemUi | FontRole::Menu) => ("Noto Sans", 12, 400),
        (FontProfile::Classic, FontRole::WindowTitle) => ("Noto Sans", 12, 700),
        (FontProfile::Classic, FontRole::Body) => ("Noto Sans", 12, 400),
        (FontProfile::Classic, FontRole::Small) => ("Noto Sans", 10, 400),
        (FontProfile::Classic, FontRole::Monospace) => ("Noto Sans Mono", 12, 400),
        (FontProfile::Classic, FontRole::DocumentDefault) => ("Noto Sans", 13, 400),
        (FontProfile::Modern, FontRole::SystemUi | FontRole::Menu) => ("Inter", 13, 400),
        (FontProfile::Modern, FontRole::WindowTitle) => ("Inter", 13, 600),
        (FontProfile::Modern, FontRole::Body) => ("Inter", 13, 400),
        (FontProfile::Modern, FontRole::Small) => ("Inter", 11, 400),
        (FontProfile::Modern, FontRole::Monospace) => ("JetBrains Mono", 12, 400),
        (FontProfile::Modern, FontRole::DocumentDefault) => ("Inter", 14, 400),
        (FontProfile::Accessible, FontRole::SystemUi | FontRole::Menu) => {
            ("Atkinson Hyperlegible", 15, 600)
        }
        (FontProfile::Accessible, FontRole::WindowTitle) => ("Atkinson Hyperlegible", 16, 700),
        (FontProfile::Accessible, FontRole::Body) => ("Atkinson Hyperlegible", 15, 400),
        (FontProfile::Accessible, FontRole::Small) => ("Atkinson Hyperlegible", 13, 400),
        (FontProfile::Accessible, FontRole::Monospace) => ("JetBrains Mono", 14, 500),
        (FontProfile::Accessible, FontRole::DocumentDefault) => ("Atkinson Hyperlegible", 16, 400),
        // A custom profile is immediately usable and starts from Modern until
        // an individual role is overridden.
        (FontProfile::Custom, role) => {
            return default_role_spec(FontProfile::Modern, role);
        }
    };
    FontRoleSpec::try_new(family, size, weight)
}

/// Active font profile configuration with per-role font specs.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FontProfileConfig {
    pub profile: FontProfile,
    pub roles: HashMap<FontRole, FontRoleSpec>,
}

impl Default for FontProfileConfig {
    fn default() -> Self {
        Self::for_profile(FontProfile::Modern)
    }
}

impl FontProfileConfig {
    /// Construct a built-in profile using the historical infallible API.
    pub fn for_profile(profile: FontProfile) -> Self {
        Self::try_for_profile(profile)
            .expect("built-in SLOPOS-I font profile defaults must be valid")
    }

    /// Construct a built-in profile while exposing validation failures.
    pub fn try_for_profile(profile: FontProfile) -> Result<Self, FontResolutionError> {
        let roles = FontRole::all()
            .iter()
            .copied()
            .map(|role| default_role_spec(profile, role).map(|spec| (role, spec)))
            .collect::<Result<HashMap<_, _>, _>>()?;
        Ok(Self { profile, roles })
    }

    /// Return a role specification using the historical infallible API.
    pub fn get_spec(&self, role: FontRole) -> FontRoleSpec {
        self.roles.get(&role).cloned().unwrap_or_else(|| {
            FontRoleSpec::new(
                LOGICAL_RECOVERY_FONT_FAMILY,
                role.default_size() as u32,
                400,
            )
        })
    }

    /// Return a role specification after validating stored configuration.
    pub fn try_get_spec(&self, role: FontRole) -> Result<FontRoleSpec, FontResolutionError> {
        if let Some(spec) = self.roles.get(&role).cloned() {
            spec.validate()?;
            Ok(spec)
        } else {
            default_role_spec(self.profile, role)
        }
    }

    pub fn validate(&self) -> Result<(), FontResolutionError> {
        for role in FontRole::all() {
            if let Some(spec) = self.roles.get(role) {
                spec.validate()?;
            }
        }

        // Keep validation deterministic if a future FontRole is added to the
        // enum before it is added to the canonical role list above.
        let known_roles = FontRole::all();
        let mut unknown_roles: Vec<_> = self
            .roles
            .keys()
            .copied()
            .filter(|role| !known_roles.contains(role))
            .collect();
        unknown_roles.sort_unstable();
        for role in unknown_roles {
            self.roles
                .get(&role)
                .expect("unknown role key collected from profile")
                .validate()?;
        }
        Ok(())
    }

    /// Return the complete, ordered role fallback chain, ending in the
    /// logical recovery family supplied by this crate's renderer boundary.
    pub fn fallback_chain(&self, role: FontRole) -> Result<Vec<String>, FontResolutionError> {
        let spec = self.try_get_spec(role)?;
        Ok(fallback_chain(self.profile, role, &spec.family))
    }

    /// Resolve every role against an explicit set of available family names.
    ///
    /// Profile names are preferences only. A family is selected only when the
    /// caller reports a matching available family; otherwise the ordered
    /// fallback chain reaches the embedded recovery family.
    pub fn resolve<I, S>(&self, available_families: I, scale: f32) -> ResolvedFontProfile
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        FontProfileResolver::new(available_families).resolve(self, scale)
    }

    pub fn try_resolve<I, S>(
        &self,
        available_families: I,
        scale: f32,
    ) -> Result<ResolvedFontProfile, FontResolutionError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        FontProfileResolver::new(available_families).try_resolve(self, scale)
    }

    pub fn resolve_with_user_and_system<U, US, T, TS>(
        &self,
        user_families: U,
        system_families: T,
        scale: f32,
    ) -> ResolvedFontProfile
    where
        U: IntoIterator<Item = US>,
        US: AsRef<str>,
        T: IntoIterator<Item = TS>,
        TS: AsRef<str>,
    {
        FontProfileResolver::from_user_and_system(user_families, system_families)
            .resolve(self, scale)
    }

    pub fn try_resolve_with_user_and_system<U, US, T, TS>(
        &self,
        user_families: U,
        system_families: T,
        scale: f32,
    ) -> Result<ResolvedFontProfile, FontResolutionError>
    where
        U: IntoIterator<Item = US>,
        US: AsRef<str>,
        T: IntoIterator<Item = TS>,
        TS: AsRef<str>,
    {
        FontProfileResolver::from_user_and_system(user_families, system_families)
            .try_resolve(self, scale)
    }
}

/// Provenance of a family selected by the resolver.
#[derive(
    Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum FontSource {
    User,
    System,
    #[default]
    RendererBitmapFallback,
}

/// Normalized family availability with explicit user-over-system precedence.
///
/// A user family and a system family with the same normalized name are one
/// logical family, and `source_for` always reports the user copy. Family
/// membership is stored in ordered sets so debug output and future iteration
/// stay deterministic regardless of discovery order.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FontFamilyAvailability {
    user: BTreeSet<String>,
    system: BTreeSet<String>,
}

impl FontFamilyAvailability {
    pub fn new<UserFamilies, UserFamily, SystemFamilies, SystemFamily>(
        user_families: UserFamilies,
        system_families: SystemFamilies,
    ) -> Self
    where
        UserFamilies: IntoIterator<Item = UserFamily>,
        UserFamily: AsRef<str>,
        SystemFamilies: IntoIterator<Item = SystemFamily>,
        SystemFamily: AsRef<str>,
    {
        Self {
            user: normalize_family_set(user_families),
            system: normalize_family_set(system_families),
        }
    }

    pub fn system_only<I, S>(system_families: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self::new(std::iter::empty::<&str>(), system_families)
    }

    pub fn user_families(&self) -> impl Iterator<Item = &str> {
        self.user.iter().map(String::as_str)
    }

    pub fn system_families(&self) -> impl Iterator<Item = &str> {
        self.system.iter().map(String::as_str)
    }

    pub fn contains(&self, family: &str) -> bool {
        self.source_for(family).is_some()
    }

    pub fn source_for(&self, family: &str) -> Option<FontSource> {
        let key = normalize_family_name(family);
        if key.is_empty() {
            return None;
        }
        if key == normalize_family_name(LOGICAL_RECOVERY_FONT_FAMILY) {
            return Some(FontSource::RendererBitmapFallback);
        }
        if self.user.contains(&key) {
            Some(FontSource::User)
        } else if self.system.contains(&key) {
            Some(FontSource::System)
        } else {
            None
        }
    }
}

fn normalize_family_set<I, S>(families: I) -> BTreeSet<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    families
        .into_iter()
        .map(|family| normalize_family_name(family.as_ref()))
        .filter(|family| {
            !family.is_empty() && family != &normalize_family_name(LOGICAL_RECOVERY_FONT_FAMILY)
        })
        .collect()
}

/// A resolved role after availability, fallback, and display-scale policy is
/// applied.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolvedFontRole {
    /// The normalized configured family name, before fallback selection.
    pub requested_family: String,
    /// The selected family, or the reserved embedded recovery family when no
    /// safe available family matched. This is not a claim that any selected
    /// system family is bundled by SLOPOS-I.
    pub family: String,
    /// The clamped logical size after applying the display scale.
    pub size: u32,
    /// The clamped font weight.
    pub weight: u16,
    /// Whether the configured family was unavailable or malformed.
    pub used_fallback: bool,
    /// Whether the result came from user fonts, system fonts, or the embedded
    /// recovery path.
    #[serde(default)]
    pub source: FontSource,
}

impl ResolvedFontRole {
    /// Convert to a role specification using the historical infallible API.
    pub fn as_spec(&self) -> FontRoleSpec {
        FontRoleSpec::new(self.family.clone(), self.size, self.weight)
    }

    /// Convert to a role specification while reporting invalid serialized
    /// values to new callers.
    pub fn try_as_spec(&self) -> Result<FontRoleSpec, FontResolutionError> {
        FontRoleSpec::try_new(self.family.clone(), self.size, self.weight)
    }
}

/// Fully resolved typography profile with deterministic role ordering.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolvedFontProfile {
    pub profile: FontProfile,
    pub roles: BTreeMap<FontRole, ResolvedFontRole>,
}

impl ResolvedFontProfile {
    pub fn get_role(&self, role: FontRole) -> Option<&ResolvedFontRole> {
        self.roles.get(&role)
    }

    /// Return a role specification using the historical infallible API.
    pub fn get_spec(&self, role: FontRole) -> FontRoleSpec {
        self.roles
            .get(&role)
            .map(ResolvedFontRole::as_spec)
            .unwrap_or_else(|| {
                FontRoleSpec::new(
                    LOGICAL_RECOVERY_FONT_FAMILY,
                    role.default_size() as u32,
                    400,
                )
            })
    }

    /// Return a role specification while reporting invalid serialized values.
    pub fn try_get_spec(&self, role: FontRole) -> Result<FontRoleSpec, FontResolutionError> {
        self.roles
            .get(&role)
            .map(ResolvedFontRole::try_as_spec)
            .unwrap_or_else(|| {
                FontRoleSpec::try_new(
                    LOGICAL_RECOVERY_FONT_FAMILY,
                    role.default_size() as u32,
                    400,
                )
            })
    }
}

/// Authoritative resolver for profile roles.
///
/// The available-family set is explicit and normalized once at construction,
/// so resolution never scans the filesystem or depends on `HashSet` order.
/// No family is considered bundled; a family is selected by name only when it
/// is present in the supplied user/system availability, otherwise the
/// logical recovery family is the final result.
#[derive(Clone, Debug, Default)]
pub struct FontProfileResolver {
    availability: FontFamilyAvailability,
}

impl FontProfileResolver {
    pub fn new<I, S>(available_families: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self::from_availability(FontFamilyAvailability::system_only(available_families))
    }

    pub fn from_user_and_system<U, US, T, TS>(user_families: U, system_families: T) -> Self
    where
        U: IntoIterator<Item = US>,
        US: AsRef<str>,
        T: IntoIterator<Item = TS>,
        TS: AsRef<str>,
    {
        Self::from_availability(FontFamilyAvailability::new(user_families, system_families))
    }

    pub fn from_availability(availability: FontFamilyAvailability) -> Self {
        Self { availability }
    }

    pub fn availability(&self) -> &FontFamilyAvailability {
        &self.availability
    }

    /// Resolve using the historical lossy behavior for malformed values.
    pub fn resolve(&self, config: &FontProfileConfig, scale: f32) -> ResolvedFontProfile {
        let scale = if scale.is_finite() {
            scale.clamp(MIN_FONT_SCALE, MAX_FONT_SCALE)
        } else {
            1.0
        };
        let roles = FontRole::all()
            .iter()
            .copied()
            .map(|role| {
                let configured = config.get_spec(role);
                let requested_family = canonical_family_name(&configured.family);
                let (family, used_fallback, source) =
                    resolve_family(config.profile, role, &requested_family, &self.availability);
                let size = scaled_font_size(configured.size, scale);
                let weight = configured.weight.clamp(100, 900);

                (
                    role,
                    ResolvedFontRole {
                        requested_family,
                        family,
                        size,
                        weight,
                        used_fallback,
                        source,
                    },
                )
            })
            .collect();

        ResolvedFontProfile {
            profile: config.profile,
            roles,
        }
    }

    pub fn try_resolve(
        &self,
        config: &FontProfileConfig,
        scale: f32,
    ) -> Result<ResolvedFontProfile, FontResolutionError> {
        validate_font_scale(scale)?;
        config.validate()?;
        self.resolve_validated(config, scale)
    }

    fn resolve_validated(
        &self,
        config: &FontProfileConfig,
        scale: f32,
    ) -> Result<ResolvedFontProfile, FontResolutionError> {
        let roles = FontRole::all()
            .iter()
            .copied()
            .map(|role| -> Result<_, FontResolutionError> {
                let configured = config.try_get_spec(role)?;
                let requested_family = canonical_family_name(&configured.family);
                let (family, used_fallback, source) =
                    resolve_family(config.profile, role, &requested_family, &self.availability);
                let size = try_scaled_font_size(configured.size, scale)?;
                let weight = configured.weight.clamp(100, 900);

                Ok((
                    role,
                    ResolvedFontRole {
                        requested_family,
                        family,
                        size,
                        weight,
                        used_fallback,
                        source,
                    },
                ))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;

        Ok(ResolvedFontProfile {
            profile: config.profile,
            roles,
        })
    }
}

const SAFE_SANS_FALLBACKS: &[&str] = &["Noto Sans", "DejaVu Sans", "Liberation Sans"];

const SAFE_ACCESSIBLE_SANS_FALLBACKS: &[&str] = &[
    "Atkinson Hyperlegible",
    "Noto Sans",
    "DejaVu Sans",
    "Liberation Sans",
];

const SAFE_MONOSPACE_FALLBACKS: &[&str] =
    &["Noto Sans Mono", "DejaVu Sans Mono", "Liberation Mono"];

fn fallback_families(profile: FontProfile, role: FontRole) -> &'static [&'static str] {
    if role == FontRole::Monospace {
        SAFE_MONOSPACE_FALLBACKS
    } else if profile == FontProfile::Accessible {
        SAFE_ACCESSIBLE_SANS_FALLBACKS
    } else {
        SAFE_SANS_FALLBACKS
    }
}

/// Build the deterministic family chain for one role. The configured family
/// is first when valid, safe profile candidates follow, and the embedded
/// recovery family is always last.
pub fn fallback_chain(profile: FontProfile, role: FontRole, requested_family: &str) -> Vec<String> {
    let mut chain = Vec::new();
    let mut add_unique = |family: &str| {
        let family = canonical_family_name(family);
        if family.is_empty()
            || chain.iter().any(|existing: &String| {
                normalize_family_name(existing) == normalize_family_name(&family)
            })
        {
            return;
        }
        chain.push(family);
    };

    let recovery_key = normalize_family_name(LOGICAL_RECOVERY_FONT_FAMILY);
    let requested_family = canonical_family_name(requested_family);
    if !requested_family.is_empty() && normalize_family_name(&requested_family) != recovery_key {
        add_unique(&requested_family);
    }
    for family in fallback_families(profile, role) {
        add_unique(family);
    }
    add_unique(LOGICAL_RECOVERY_FONT_FAMILY);
    chain
}

fn canonical_family_name(family: &str) -> String {
    if family.chars().any(|character| character.is_control()) {
        return String::new();
    }

    let family = family
        .chars()
        .map(|character| match character {
            '-' | '_' => ' ',
            _ => character,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    family
}

fn validate_family_name(family: &str) -> Result<(), FontResolutionError> {
    if family.is_empty()
        || family.chars().any(|character| character.is_control())
        || family
            .chars()
            .all(|character| character.is_whitespace() || character.is_control())
    {
        Err(FontResolutionError::InvalidFamily)
    } else {
        Ok(())
    }
}

/// Normalize family names for case-, separator-, and whitespace-insensitive
/// matching while retaining a stable display spelling separately.
pub fn normalize_family_name(family: &str) -> String {
    canonical_family_name(family).to_lowercase()
}

fn resolve_family(
    profile: FontProfile,
    role: FontRole,
    requested_family: &str,
    availability: &FontFamilyAvailability,
) -> (String, bool, FontSource) {
    let requested_key = normalize_family_name(requested_family);
    for family in fallback_chain(profile, role, requested_family) {
        let family_key = normalize_family_name(&family);
        let Some(source) = availability.source_for(&family) else {
            continue;
        };
        if family_key == normalize_family_name(LOGICAL_RECOVERY_FONT_FAMILY) {
            return (
                LOGICAL_RECOVERY_FONT_FAMILY.to_string(),
                family_key != requested_key,
                FontSource::RendererBitmapFallback,
            );
        }
        if !requested_key.is_empty() && family_key == requested_key {
            return (requested_family.to_string(), false, source);
        }
        return (family, true, source);
    }

    (
        LOGICAL_RECOVERY_FONT_FAMILY.to_string(),
        true,
        FontSource::RendererBitmapFallback,
    )
}

fn scaled_font_size(size: u32, scale: f32) -> u32 {
    ((size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE) as f32 * scale).round() as u32)
        .clamp(MIN_FONT_SIZE, MAX_FONT_SIZE)
}

fn try_scaled_font_size(size: u32, scale: f32) -> Result<u32, FontResolutionError> {
    validate_font_size(size)?;
    validate_font_scale(scale)?;
    Ok(scaled_font_size(size, scale))
}

/// Font discovery service searching user and system directories.
#[derive(Clone, Debug)]
pub struct FontDiscoveryService {
    search_paths: Vec<PathBuf>,
}

impl Default for FontDiscoveryService {
    fn default() -> Self {
        Self::new()
    }
}

impl FontDiscoveryService {
    pub fn new() -> Self {
        let mut search_paths = Vec::new();

        if let Ok(data_home) = std::env::var("XDG_DATA_HOME") {
            let p1 = PathBuf::from(&data_home).join("fonts");
            let p2 = PathBuf::from(&data_home).join("slopos-i/fonts");
            search_paths.push(p1);
            search_paths.push(p2);
        } else if let Ok(home) = std::env::var("HOME") {
            search_paths.push(PathBuf::from(&home).join(".local/share/fonts"));
            search_paths.push(PathBuf::from(&home).join(".local/share/slopos-i/fonts"));
        }

        if let Ok(data_dirs) = std::env::var("XDG_DATA_DIRS") {
            for dir in data_dirs.split(':') {
                if !dir.is_empty() {
                    search_paths.push(PathBuf::from(dir).join("fonts"));
                }
            }
        }

        search_paths.push(PathBuf::from("/usr/local/share/fonts"));
        search_paths.push(PathBuf::from("/usr/share/fonts"));

        Self { search_paths }
    }

    pub fn search_paths(&self) -> &[PathBuf] {
        &self.search_paths
    }

    /// Discover available font files (`.ttf`, `.otf`, `.ttc`).
    pub fn discover_font_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let mut seen_paths = HashSet::new();
        for base_path in &self.search_paths {
            discover_font_files_in_dir(base_path, &mut files, &mut seen_paths);
        }
        files
    }
}

fn discover_font_files_in_dir(
    base_path: &Path,
    files: &mut Vec<PathBuf>,
    seen_paths: &mut HashSet<PathBuf>,
) {
    let Ok(metadata) = fs::symlink_metadata(base_path) else {
        return;
    };
    if !metadata.file_type().is_dir() {
        return;
    }

    let Ok(entries) = std::fs::read_dir(base_path) else {
        return;
    };

    let mut paths = Vec::new();
    for entry in entries.flatten() {
        paths.push(entry.path());
    }
    paths.sort();

    for path in paths {
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        let file_type = metadata.file_type();

        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            discover_font_files_in_dir(&path, files, seen_paths);
            continue;
        }

        if !file_type.is_file() || !is_font_extension(&path) {
            continue;
        }

        let Ok(canonical_path) = fs::canonicalize(&path) else {
            continue;
        };
        if seen_paths.insert(canonical_path) {
            files.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn make_temp_dir(prefix: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "slopos-fonts-{prefix}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock moved backwards")
                .as_nanos()
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn write_font_file(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent directories");
        }
        fs::write(path, b"font").expect("write font file");
    }

    fn profile(profile: FontProfile) -> FontProfileConfig {
        FontProfileConfig::for_profile(profile)
    }

    #[test]
    fn test_font_roles_and_defaults() {
        assert_eq!(FontRole::SystemUi.as_str(), "system_ui");
        assert_eq!(FontRole::all().len(), 7);
        assert_eq!(FontRole::SystemUi.default_size(), 13.0);
    }

    #[test]
    fn test_font_profile_config() {
        let classic = profile(FontProfile::Classic);
        assert_eq!(classic.profile, FontProfile::Classic);
        let spec = classic.get_spec(FontRole::SystemUi);
        assert_eq!(spec.family, "Noto Sans");

        let modern = profile(FontProfile::Modern);
        assert_eq!(modern.get_spec(FontRole::SystemUi).family, "Inter");

        let accessible = profile(FontProfile::Accessible);
        assert_eq!(
            accessible.get_spec(FontRole::SystemUi).family,
            "Atkinson Hyperlegible"
        );

        let custom = profile(FontProfile::Custom);
        assert_eq!(custom.profile, FontProfile::Custom);
        assert_eq!(custom.roles.len(), FontRole::all().len());
        assert_eq!(custom.get_spec(FontRole::Body).family, "Inter");
    }

    #[test]
    fn font_profile_defaults_use_nonproprietary_preferences_for_every_role() {
        let proprietary_names = ["Chicago", "Geneva", "Monaco", "San Francisco"];
        for selected_profile in [
            FontProfile::Classic,
            FontProfile::Modern,
            FontProfile::Accessible,
            FontProfile::Custom,
        ] {
            let config = profile(selected_profile);
            assert_eq!(config.roles.len(), FontRole::all().len());
            for role in FontRole::all() {
                let spec = config.get_spec(*role);
                assert!(spec.validate().is_ok());
                assert!(!proprietary_names.contains(&spec.family.as_str()));
            }
        }
    }

    #[test]
    fn checked_font_inputs_reject_invalid_sizes_weights_and_scales() {
        assert!(FontRoleSpec::try_new("Noto Sans", MIN_FONT_SIZE, 400).is_ok());
        assert!(matches!(
            FontRoleSpec::try_new("Noto Sans", 0, 400),
            Err(FontResolutionError::InvalidSize { size: 0, .. })
        ));
        assert!(matches!(
            FontRoleSpec::try_new("Noto Sans", MAX_FONT_SIZE + 1, 400),
            Err(FontResolutionError::InvalidSize { .. })
        ));
        assert!(matches!(
            FontRoleSpec::try_new("Noto Sans", 13, 99),
            Err(FontResolutionError::InvalidWeight { weight: 99 })
        ));
        assert!(matches!(
            FontRoleSpec::try_new("\u{0}", 13, 400),
            Err(FontResolutionError::InvalidFamily)
        ));
        let lossy = FontRoleSpec::lossy("Noto Sans", 0, 99);
        assert_eq!(lossy.size, MIN_FONT_SIZE);
        assert_eq!(lossy.weight, 100);

        let config = profile(FontProfile::Modern);
        for scale in [0.0, -1.0, f32::NAN, f32::INFINITY, 4.1] {
            assert!(matches!(
                config.try_resolve(["Inter"], scale),
                Err(FontResolutionError::InvalidScale { .. })
            ));
        }
        assert!(config.try_resolve(["Inter"], 1.25).is_ok());

        let mut malformed = config;
        malformed.roles.insert(
            FontRole::Body,
            FontRoleSpec {
                family: "Inter".to_string(),
                size: 0,
                weight: 400,
            },
        );
        assert!(matches!(
            malformed.try_resolve(["Inter"], 1.0),
            Err(FontResolutionError::InvalidSize { size: 0, .. })
        ));
    }

    #[test]
    fn legacy_infallible_calls_keep_their_original_return_types() {
        assert_eq!(FontProfile::parse("unknown-profile"), FontProfile::Modern);

        let spec = FontRoleSpec::new("Inter", 0, 99);
        assert_eq!(spec.size, MIN_FONT_SIZE);
        assert_eq!(spec.weight, 100);

        let config = FontProfileConfig::for_profile(FontProfile::Modern);
        assert_eq!(config.get_spec(FontRole::SystemUi).family, "Inter");

        let resolved = config.resolve(["Inter"], 1.0);
        let resolved_again = FontProfileResolver::new(["Inter"]).resolve(&config, 1.0);
        assert_eq!(resolved, resolved_again);
        assert_eq!(resolved.get_spec(FontRole::SystemUi).family, "Inter");
        assert_eq!(
            resolved
                .get_role(FontRole::SystemUi)
                .expect("system UI role")
                .as_spec()
                .family,
            "Inter"
        );
    }

    #[test]
    fn family_validation_rejects_whitespace_and_control_only_names() {
        for family in ["", "   ", "\u{2003}\u{2003}", "\u{0}\u{1}"] {
            assert!(matches!(
                FontRoleSpec::try_new(family, 13, 400),
                Err(FontResolutionError::InvalidFamily)
            ));

            let stored = FontRoleSpec {
                family: family.to_string(),
                size: 13,
                weight: 400,
            };
            assert!(matches!(
                stored.validate(),
                Err(FontResolutionError::InvalidFamily)
            ));
        }

        assert_eq!(normalize_family_name("\tInter"), "");
        assert!(FontRoleSpec::try_new("\tInter", 13, 400).is_err());
    }

    #[test]
    fn resolved_font_role_deserializes_payloads_without_new_provenance_field() {
        let role: ResolvedFontRole = serde_json::from_str(
            r#"{
                "requested_family": "Inter",
                "family": "Inter",
                "size": 13,
                "weight": 400,
                "used_fallback": false
            }"#,
        )
        .expect("legacy resolved role payload is compatible");

        assert_eq!(role.requested_family, "Inter");
        assert_eq!(role.family, "Inter");
        assert_eq!(role.source, FontSource::RendererBitmapFallback);
    }

    #[test]
    fn custom_overrides_keep_complete_safe_defaults_and_apply_scale() {
        let mut config = profile(FontProfile::Custom);
        config.roles.insert(
            FontRole::Body,
            FontRoleSpec::try_new("Nimbus Sans", 20, 500).expect("valid custom role"),
        );

        let resolved = config
            .try_resolve(["Nimbus Sans", "Inter", "Noto Sans Mono"], 1.25)
            .expect("valid custom profile");
        assert_eq!(
            resolved
                .try_get_spec(FontRole::Body)
                .expect("resolved role is valid")
                .family,
            "Nimbus Sans"
        );
        assert_eq!(
            resolved
                .try_get_spec(FontRole::Body)
                .expect("resolved role is valid")
                .size,
            25
        );
        assert_eq!(
            resolved
                .try_get_spec(FontRole::Body)
                .expect("resolved role is valid")
                .weight,
            500
        );
        assert_eq!(
            resolved
                .try_get_spec(FontRole::SystemUi)
                .expect("resolved role is valid")
                .family,
            "Inter"
        );
        assert_eq!(resolved.roles.len(), FontRole::all().len());
    }

    #[test]
    fn fallback_chain_is_deterministic_and_always_ends_in_recovery() {
        let first = fallback_chain(FontProfile::Modern, FontRole::Body, "  Missing_Family ");
        let second = fallback_chain(FontProfile::Modern, FontRole::Body, "Missing Family");
        assert_eq!(first, second);
        assert_eq!(first.first().map(String::as_str), Some("Missing Family"));
        assert_eq!(
            first.last().map(String::as_str),
            Some(LOGICAL_RECOVERY_FONT_FAMILY)
        );
        assert!(first
            .windows(2)
            .all(|pair| normalize_family_name(&pair[0]) != normalize_family_name(&pair[1])));

        let config = profile(FontProfile::Modern);
        let one = config
            .try_resolve(["Liberation Sans", "Noto Sans"], 1.0)
            .expect("valid profile resolution");
        let two = config
            .try_resolve(["Noto Sans", "Liberation Sans"], 1.0)
            .expect("valid profile resolution");
        assert_eq!(one, two);
        assert_eq!(
            one.try_get_spec(FontRole::Body)
                .expect("resolved role is valid")
                .family,
            "Noto Sans"
        );

        let empty = config
            .try_resolve(std::iter::empty::<&str>(), 1.0)
            .expect("recovery resolution is valid");
        assert!(empty.roles.values().all(|role| {
            role.family == LOGICAL_RECOVERY_FONT_FAMILY
                && role.source == FontSource::RendererBitmapFallback
                && role.used_fallback
        }));
    }

    #[test]
    fn fallback_chain_keeps_requested_recovery_family_unique_and_last() {
        let chain = fallback_chain(
            FontProfile::Modern,
            FontRole::Body,
            LOGICAL_RECOVERY_FONT_FAMILY,
        );
        let recovery_count = chain
            .iter()
            .filter(|family| {
                normalize_family_name(family) == normalize_family_name(LOGICAL_RECOVERY_FONT_FAMILY)
            })
            .count();

        assert_eq!(
            chain.last().map(String::as_str),
            Some(LOGICAL_RECOVERY_FONT_FAMILY)
        );
        assert_eq!(recovery_count, 1);
        assert_ne!(
            chain.first().map(String::as_str),
            Some(LOGICAL_RECOVERY_FONT_FAMILY)
        );
    }

    #[test]
    fn recovery_boundary_identifies_logical_name_and_bitmap_provider() {
        assert_eq!(
            RecoveryFallbackContract::family(),
            LOGICAL_RECOVERY_FONT_FAMILY
        );
        assert_eq!(
            RecoveryFallbackContract::provider(),
            "slopos-render bitmap fallback"
        );
        assert!(!RecoveryFallbackContract::has_embedded_font_bytes());
    }

    #[test]
    fn user_family_precedence_is_explicit_and_stable() {
        let resolver = FontProfileResolver::from_user_and_system(
            ["nOtO sAnS"],
            ["Inter", "Noto Sans", "DejaVu Sans"],
        );
        let config = profile(FontProfile::Modern);
        let resolved = resolver
            .try_resolve(&config, 1.0)
            .expect("valid profile resolution");
        let body = resolved.get_role(FontRole::Body).expect("body role");

        assert_eq!(body.family, "Inter");
        assert_eq!(body.source, FontSource::System);

        let user_requested = FontProfileConfig {
            profile: FontProfile::Custom,
            roles: HashMap::from([(
                FontRole::Body,
                FontRoleSpec::try_new("Noto Sans", 13, 400).expect("valid custom role"),
            )]),
        };
        let resolved = resolver
            .try_resolve(&user_requested, 1.0)
            .expect("valid profile resolution");
        let body = resolved.get_role(FontRole::Body).expect("body role");
        assert_eq!(body.family, "Noto Sans");
        assert_eq!(body.source, FontSource::User);
        assert!(!body.used_fallback);
    }

    #[test]
    fn font_profile_resolver_resolves_classic_modern_accessible_and_custom_profiles() {
        let classic = profile(FontProfile::Classic)
            .try_resolve(["noto sans", "noto sans mono"], 1.0)
            .expect("valid profile resolution");
        assert_eq!(classic.profile, FontProfile::Classic);
        assert_eq!(
            classic
                .try_get_spec(FontRole::SystemUi)
                .expect("resolved role is valid")
                .family,
            "Noto Sans"
        );
        assert_eq!(
            classic
                .try_get_spec(FontRole::Body)
                .expect("resolved role is valid")
                .family,
            "Noto Sans"
        );
        assert_eq!(
            classic
                .try_get_spec(FontRole::Monospace)
                .expect("resolved role is valid")
                .family,
            "Noto Sans Mono"
        );

        let modern = profile(FontProfile::Modern)
            .try_resolve([" INTER ", "jetbrains-mono"], 1.0)
            .expect("valid profile resolution");
        assert_eq!(
            modern
                .try_get_spec(FontRole::SystemUi)
                .expect("resolved role is valid")
                .family,
            "Inter"
        );
        assert_eq!(
            modern
                .try_get_spec(FontRole::Monospace)
                .expect("resolved role is valid")
                .family,
            "JetBrains Mono"
        );

        let accessible = profile(FontProfile::Accessible)
            .try_resolve(["atkinson hyperlegible", "JETBRAINS MONO"], 1.0)
            .expect("valid profile resolution");
        assert_eq!(
            accessible
                .try_get_spec(FontRole::SystemUi)
                .expect("resolved role is valid")
                .family,
            "Atkinson Hyperlegible"
        );
        assert_eq!(
            accessible
                .try_get_spec(FontRole::Monospace)
                .expect("resolved role is valid")
                .family,
            "JetBrains Mono"
        );

        let mut custom = profile(FontProfile::Custom);
        custom.roles.insert(
            FontRole::SystemUi,
            FontRoleSpec::try_new("  Nimbus   Sans ", 20, 500).expect("valid custom role"),
        );
        custom.roles.insert(
            FontRole::Monospace,
            FontRoleSpec::try_new("My_Mono", 10, 300).expect("valid custom role"),
        );
        let custom = custom
            .try_resolve(["nImBuS sAnS", "my mono"], 1.25)
            .expect("valid profile resolution");
        assert_eq!(custom.profile, FontProfile::Custom);
        assert_eq!(
            custom
                .try_get_spec(FontRole::SystemUi)
                .expect("resolved role is valid")
                .family,
            "Nimbus Sans"
        );
        assert_eq!(
            custom
                .try_get_spec(FontRole::SystemUi)
                .expect("resolved role is valid")
                .size,
            25
        );
        assert_eq!(
            custom
                .try_get_spec(FontRole::Monospace)
                .expect("resolved role is valid")
                .family,
            "My Mono"
        );
        assert_eq!(
            custom
                .try_get_spec(FontRole::Monospace)
                .expect("resolved role is valid")
                .size,
            13
        );
    }

    #[test]
    fn font_profile_resolver_uses_safe_fallbacks_for_unavailable_families() {
        let classic = profile(FontProfile::Classic)
            .try_resolve(["DejaVu Sans", "Noto Sans Mono"], 1.0)
            .expect("valid profile resolution");

        assert_eq!(
            classic
                .try_get_spec(FontRole::SystemUi)
                .expect("resolved role is valid")
                .family,
            "DejaVu Sans"
        );
        assert_eq!(
            classic
                .try_get_spec(FontRole::Body)
                .expect("resolved role is valid")
                .family,
            "DejaVu Sans"
        );
        assert_eq!(
            classic
                .try_get_spec(FontRole::Monospace)
                .expect("resolved role is valid")
                .family,
            "Noto Sans Mono"
        );
        assert!(
            classic
                .get_role(FontRole::SystemUi)
                .expect("system UI role")
                .used_fallback
        );
        assert!(!["Chicago", "Geneva", "Monaco"].contains(
            &classic
                .try_get_spec(FontRole::SystemUi)
                .expect("resolved role is valid")
                .family
                .as_str()
        ));

        let mut custom = profile(FontProfile::Custom);
        custom.roles.insert(
            FontRole::Body,
            FontRoleSpec::try_new("Unavailable Family", 13, 400).expect("valid custom role"),
        );
        let custom = custom
            .try_resolve(["DejaVu Sans"], 1.0)
            .expect("valid profile resolution");
        assert_eq!(
            custom
                .try_get_spec(FontRole::Body)
                .expect("resolved role is valid")
                .family,
            "DejaVu Sans"
        );
        assert!(
            custom
                .get_role(FontRole::Body)
                .expect("body role")
                .used_fallback
        );

        let empty = profile(FontProfile::Classic)
            .try_resolve(std::iter::empty::<&str>(), 1.0)
            .expect("recovery resolution is valid");
        assert_eq!(
            empty
                .try_get_spec(FontRole::SystemUi)
                .expect("resolved role is valid")
                .family,
            LOGICAL_RECOVERY_FONT_FAMILY
        );
        assert_eq!(
            empty
                .try_get_spec(FontRole::Monospace)
                .expect("resolved role is valid")
                .family,
            LOGICAL_RECOVERY_FONT_FAMILY
        );
    }

    #[test]
    fn font_profile_resolver_has_stable_fallback_order_and_matching() {
        let config = profile(FontProfile::Modern);
        let first = config
            .try_resolve(
                [
                    "Liberation Sans",
                    "DejaVu Sans",
                    "Noto Sans",
                    "Liberation Mono",
                    "DejaVu Sans Mono",
                ],
                1.0,
            )
            .expect("valid profile resolution");
        let second = config
            .try_resolve(
                [
                    "DejaVu Sans Mono",
                    "Noto Sans",
                    "Liberation Mono",
                    "DejaVu Sans",
                    "Liberation Sans",
                ],
                1.0,
            )
            .expect("valid profile resolution");

        assert_eq!(first, second);
        assert_eq!(
            first
                .try_get_spec(FontRole::SystemUi)
                .expect("resolved role is valid")
                .family,
            "Noto Sans"
        );
        assert_eq!(
            first
                .try_get_spec(FontRole::Monospace)
                .expect("resolved role is valid")
                .family,
            "DejaVu Sans Mono"
        );
        assert_eq!(
            normalize_family_name("  JetBrains-MONO  "),
            "jetbrains mono"
        );
    }

    #[test]
    fn font_profile_resolver_rejects_malformed_profile_values() {
        assert_eq!(
            FontProfile::try_parse("  ACCESSIBLE ").expect("known profile"),
            FontProfile::Accessible
        );
        assert!(matches!(
            FontProfile::try_parse("unknown-profile"),
            Err(FontResolutionError::InvalidProfile)
        ));
        assert!(matches!(
            FontProfile::try_parse(""),
            Err(FontResolutionError::InvalidProfile)
        ));
        assert_eq!(
            FontProfile::parse_lossy("unknown-profile"),
            FontProfile::Modern
        );
        assert_eq!(FontProfile::parse_lossy(""), FontProfile::Modern);

        let mut malformed = profile(FontProfile::Custom);
        malformed.roles.insert(
            FontRole::SystemUi,
            FontRoleSpec {
                family: " \t".to_string(),
                size: u32::MAX,
                weight: u16::MAX,
            },
        );
        assert!(matches!(
            malformed.try_resolve(std::iter::empty::<&str>(), f32::NAN),
            Err(FontResolutionError::InvalidScale { .. })
        ));
        assert!(matches!(
            malformed.try_resolve(std::iter::empty::<&str>(), 1.0),
            Err(FontResolutionError::InvalidFamily)
        ));

        malformed.roles.insert(
            FontRole::Body,
            FontRoleSpec {
                family: "Inter".to_string(),
                size: 0,
                weight: 0,
            },
        );
        assert!(matches!(
            malformed.try_resolve(["Inter"], 2.0),
            Err(FontResolutionError::InvalidFamily)
        ));
    }

    #[test]
    fn profile_spec_accessors_reject_stored_malformed_specs() {
        let mut malformed = profile(FontProfile::Custom);
        malformed.roles.insert(
            FontRole::SystemUi,
            FontRoleSpec {
                family: " 	".to_string(),
                size: 13,
                weight: 400,
            },
        );

        assert!(matches!(
            malformed.try_get_spec(FontRole::SystemUi),
            Err(FontResolutionError::InvalidFamily)
        ));
        assert!(matches!(
            malformed.fallback_chain(FontRole::SystemUi),
            Err(FontResolutionError::InvalidFamily)
        ));
    }

    #[test]
    fn test_font_discovery_search_paths() {
        let service = FontDiscoveryService::new();
        assert!(!service.search_paths().is_empty());
        let has_system_fonts = service
            .search_paths()
            .iter()
            .any(|p| p.to_string_lossy().contains("fonts"));
        assert!(has_system_fonts);
    }

    #[test]
    fn test_font_discovery_recurses_into_nested_directories() {
        let temp_dir = make_temp_dir("recursive");
        let root_font = temp_dir.join("root.ttf");
        let nested_font = temp_dir.join("nested").join("family.otf");
        let deep_font = temp_dir.join("nested").join("deep").join("mono.ttc");
        let ignored = temp_dir.join("nested").join("notes.txt");

        write_font_file(&root_font);
        write_font_file(&nested_font);
        write_font_file(&deep_font);
        write_font_file(&ignored);

        let service = FontDiscoveryService {
            search_paths: vec![temp_dir.clone()],
        };
        let discovered = service.discover_font_files();

        assert!(discovered.contains(&root_font));
        assert!(discovered.contains(&nested_font));
        assert!(discovered.contains(&deep_font));
        assert!(!discovered.contains(&ignored));

        fs::remove_dir_all(&temp_dir).expect("cleanup temp dir");
    }

    #[test]
    fn font_discovery_preserves_root_precedence_and_deduplicates_overlapping_roots() {
        let temp_dir = make_temp_dir("overlap");
        let root_font = temp_dir.join("00-root.ttf");
        let nested_dir = temp_dir.join("01-nested");
        let shared_font = nested_dir.join("00-shared.otf");
        let later_font = nested_dir.join("01-later.ttc");

        write_font_file(&root_font);
        write_font_file(&shared_font);
        write_font_file(&later_font);

        let service = FontDiscoveryService {
            search_paths: vec![temp_dir.clone(), nested_dir.clone()],
        };
        let discovered = service.discover_font_files();

        assert_eq!(discovered, vec![root_font, shared_font, later_font]);

        fs::remove_dir_all(&temp_dir).expect("cleanup overlap temp dir");
    }

    #[cfg(unix)]
    #[test]
    fn font_discovery_skips_symlinked_fonts_and_directories() {
        use std::os::unix::fs::symlink;

        let temp_dir = make_temp_dir("symlinks");
        let real_font = temp_dir.join("real.ttf");
        let nested_dir = temp_dir.join("nested");
        let nested_font = nested_dir.join("nested.otf");
        let linked_font = temp_dir.join("linked.ttc");
        let linked_dir = temp_dir.join("linked-directory");
        let cycle = nested_dir.join("cycle");

        write_font_file(&real_font);
        write_font_file(&nested_font);
        symlink(&real_font, &linked_font).expect("create symlinked font");
        symlink(&nested_dir, &linked_dir).expect("create symlinked directory");
        symlink(&temp_dir, &cycle).expect("create directory cycle");

        let service = FontDiscoveryService {
            search_paths: vec![temp_dir.clone()],
        };
        let discovered = service.discover_font_files();

        assert_eq!(discovered, vec![nested_font, real_font]);
        assert!(!discovered.contains(&linked_font));
        assert!(!discovered.contains(&linked_dir));
        assert!(!discovered.iter().any(|path| path.starts_with(&cycle)));

        fs::remove_dir_all(&temp_dir).expect("cleanup symlink temp dir");
    }

    #[cfg(unix)]
    #[test]
    fn test_font_discovery_skips_unreadable_nested_directories() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = make_temp_dir("unreadable");
        let readable_font = temp_dir.join("readable").join("ok.ttf");
        let unreadable_dir = temp_dir.join("private");
        let unreadable_font = unreadable_dir.join("hidden.otf");

        write_font_file(&readable_font);
        write_font_file(&unreadable_font);

        let mut permissions = fs::metadata(&unreadable_dir)
            .expect("unreadable dir metadata")
            .permissions();
        permissions.set_mode(0o000);
        fs::set_permissions(&unreadable_dir, permissions).expect("make unreadable");

        let service = FontDiscoveryService {
            search_paths: vec![temp_dir.clone()],
        };
        let discovered = service.discover_font_files();

        assert!(discovered.contains(&readable_font));
        assert!(!discovered.contains(&unreadable_font));

        let mut permissions = fs::metadata(&unreadable_dir)
            .expect("restore dir metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&unreadable_dir, permissions).expect("restore permissions");
        fs::remove_dir_all(&temp_dir).expect("cleanup temp dir");
    }

    #[test]
    fn font_manager_installs_deduplicates_toggles_and_removes_atomically() {
        let temp_dir = make_temp_dir("manager");
        let source = temp_dir.join("Nested Family.ttf");
        let install_dir = temp_dir.join("fonts");
        fs::write(&source, b"valid font bytes").expect("write source font");
        let manager = FontManager::new(&install_dir);

        let installed = manager.install(&source).expect("install font");
        assert_eq!(installed.file_name, "Nested Family.ttf");
        assert!(installed.enabled);
        assert_eq!(manager.installed_fonts().unwrap().len(), 1);

        let duplicate = manager.install(&source).expect("deduplicate font");
        assert_eq!(duplicate.sha256, installed.sha256);
        assert_eq!(manager.installed_fonts().unwrap().len(), 1);

        let disabled = manager
            .set_enabled(&installed.file_name, false)
            .expect("disable font");
        assert!(!disabled.enabled);
        let enabled = manager
            .set_enabled(&installed.file_name, true)
            .expect("enable font");
        assert!(enabled.enabled);

        manager.remove(&installed.file_name).expect("remove font");
        assert!(manager.installed_fonts().unwrap().is_empty());
        fs::remove_dir_all(temp_dir).expect("cleanup manager temp dir");
    }

    #[test]
    fn font_manager_rejects_unsafe_or_non_font_sources() {
        let temp_dir = make_temp_dir("manager-validation");
        let install_dir = temp_dir.join("fonts");
        let bad_extension = temp_dir.join("notes.txt");
        fs::write(&bad_extension, b"not a font").expect("write invalid source");
        let manager = FontManager::new(&install_dir);

        assert!(matches!(
            manager.install(&bad_extension),
            Err(FontManagerError::InvalidSource(_))
        ));
        assert!(matches!(
            manager.remove("../escape.ttf"),
            Err(FontManagerError::UnsafeFileName(_))
        ));
        assert!(matches!(
            manager.set_enabled("../escape.ttf", false),
            Err(FontManagerError::UnsafeFileName(_))
        ));
        assert!(matches!(
            manager.remove("nested/escape.ttf"),
            Err(FontManagerError::UnsafeFileName(_))
        ));
        fs::remove_dir_all(temp_dir).expect("cleanup validation temp dir");
    }
}
