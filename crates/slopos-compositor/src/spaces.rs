use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Stable, nonzero identity for a dynamic SLOPOS Space.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SpaceId(NonZeroU64);

impl SpaceId {
    /// Construct an ID, rejecting the reserved zero value.
    pub fn new(value: u64) -> Option<Self> {
        NonZeroU64::new(value).map(Self)
    }

    /// Return the stable numeric representation.
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

impl fmt::Display for SpaceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.get().fmt(formatter)
    }
}

/// Whether a Space is a normal workspace or a fullscreen classification.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FullscreenClassification {
    #[default]
    Normal,
    Fullscreen,
}

/// How Spaces are associated with multiple displays.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MultiMonitorPolicy {
    /// One ordered Space set spans all displays.
    #[default]
    SharedSpan,
    /// Each display may own an independent ordered Space set.
    IndependentPerDisplay,
}

/// Convert the compositor's model enum to its typed session-bus form.
pub fn fullscreen_classification_to_wire(
    classification: FullscreenClassification,
) -> slopos_bus::SpaceClassification {
    match classification {
        FullscreenClassification::Normal => slopos_bus::SpaceClassification::Normal,
        FullscreenClassification::Fullscreen => slopos_bus::SpaceClassification::Fullscreen,
    }
}

/// Convert a session-bus fullscreen classification into the model enum.
pub fn fullscreen_classification_from_wire(
    classification: slopos_bus::SpaceClassification,
) -> FullscreenClassification {
    match classification {
        slopos_bus::SpaceClassification::Normal => FullscreenClassification::Normal,
        slopos_bus::SpaceClassification::Fullscreen => FullscreenClassification::Fullscreen,
    }
}

/// Convert the compositor's multi-display policy to its wire representation.
pub fn multi_monitor_policy_to_wire(policy: MultiMonitorPolicy) -> slopos_bus::SpacesDisplayPolicy {
    match policy {
        MultiMonitorPolicy::SharedSpan => slopos_bus::SpacesDisplayPolicy::SharedSpan,
        MultiMonitorPolicy::IndependentPerDisplay => {
            slopos_bus::SpacesDisplayPolicy::IndependentPerDisplay
        }
    }
}

/// Convert a session-bus display policy into the model enum.
pub fn multi_monitor_policy_from_wire(
    policy: slopos_bus::SpacesDisplayPolicy,
) -> MultiMonitorPolicy {
    match policy {
        slopos_bus::SpacesDisplayPolicy::SharedSpan => MultiMonitorPolicy::SharedSpan,
        slopos_bus::SpacesDisplayPolicy::IndependentPerDisplay => {
            MultiMonitorPolicy::IndependentPerDisplay
        }
    }
}

/// Convert a validated persisted application policy to its session-bus form.
/// `Current` and named targets are never stored in the application-policy map.
pub fn application_target_to_wire(target: &SpaceTarget) -> slopos_bus::SpaceTargetWire {
    match target {
        SpaceTarget::Id(id) => slopos_bus::SpaceTargetWire::Id { id: id.get() },
        SpaceTarget::All => slopos_bus::SpaceTargetWire::All,
        SpaceTarget::Current | SpaceTarget::Named(_) => {
            unreachable!("invalid application policy target was persisted")
        }
    }
}

/// Convert a session-bus application policy target into the validated model
/// representation, rejecting the reserved zero Space ID.
pub fn application_target_from_wire(
    target: slopos_bus::SpaceTargetWire,
) -> Result<SpaceTarget, SpacesError> {
    match target {
        slopos_bus::SpaceTargetWire::Current => Ok(SpaceTarget::Current),
        slopos_bus::SpaceTargetWire::Id { id } => SpaceId::new(id)
            .map(SpaceTarget::Id)
            .ok_or(SpacesError::InvalidSpaceId(id)),
        slopos_bus::SpaceTargetWire::All => Ok(SpaceTarget::All),
    }
}

/// Generate an epoch that is unique for the lifetime of a compositor process.
///
/// The epoch is deliberately independent of the monotonic mutation revision:
/// a shell that outlives the compositor must be able to distinguish a fresh
/// session whose first snapshot has a lower revision than the prior session.
pub fn new_session_epoch() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or_default();
    nanos ^ u64::from(std::process::id()).rotate_left(17)
}

/// The membership scope used when assigning a window.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpaceTarget {
    Current,
    Id(SpaceId),
    Named(String),
    All,
}

/// Errors returned by the pure Spaces state model and its persistence helpers.
#[derive(Debug)]
pub enum SpacesError {
    EmptySpaces,
    InvalidSpaceId(u64),
    DuplicateSpaceId(SpaceId),
    InvalidSpaceName(String),
    DuplicateSpaceName(String),
    ActiveSpaceMissing(SpaceId),
    InvalidNextSpaceId(u64),
    NextSpaceIdNotAfterExisting { next: u64, maximum_existing: u64 },
    SpaceNotFound(SpaceId),
    SpaceNameNotFound(String),
    CannotRemoveLastSpace,
    InvalidOrderIndex { index: usize, len: usize },
    InvalidWindowId(String),
    DuplicateWindowId(String),
    InvalidApplicationId(String),
    InvalidApplicationTarget(String),
    InvalidMetadata { field: &'static str, value: String },
    InvalidOutputId(String),
    OutputNotAvailable { space: SpaceId, output_id: String },
    OutputAssignmentNotAllowedInSharedSpan { space: SpaceId, output_id: String },
    OutputMigrationToSameOutput(String),
    SpaceIdExhausted,
    InvalidPath(PathBuf),
    Io { path: PathBuf, source: io::Error },
    Serialize(serde_json::Error),
    Deserialize(serde_json::Error),
}

impl fmt::Display for SpacesError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptySpaces => write!(formatter, "at least one Space is required"),
            Self::InvalidSpaceId(id) => {
                write!(formatter, "Space ID {id} is invalid; IDs are nonzero")
            }
            Self::DuplicateSpaceId(id) => write!(formatter, "duplicate Space ID {id}"),
            Self::InvalidSpaceName(name) => write!(formatter, "invalid Space name {name:?}"),
            Self::DuplicateSpaceName(name) => write!(formatter, "duplicate Space name {name:?}"),
            Self::ActiveSpaceMissing(id) => write!(formatter, "active Space {id} does not exist"),
            Self::InvalidNextSpaceId(id) => {
                write!(formatter, "next Space ID {id} is invalid; IDs are nonzero")
            }
            Self::NextSpaceIdNotAfterExisting {
                next,
                maximum_existing,
            } => write!(
                formatter,
                "next Space ID {next} must be greater than existing maximum {maximum_existing}"
            ),
            Self::SpaceNotFound(id) => write!(formatter, "Space {id} does not exist"),
            Self::SpaceNameNotFound(name) => {
                write!(formatter, "Space named {name:?} does not exist")
            }
            Self::CannotRemoveLastSpace => write!(formatter, "the last Space cannot be removed"),
            Self::InvalidOrderIndex { index, len } => {
                write!(formatter, "order index {index} is outside 0..{len}")
            }
            Self::InvalidWindowId(id) => write!(formatter, "invalid window ID {id:?}"),
            Self::DuplicateWindowId(id) => write!(formatter, "duplicate window ID {id:?}"),
            Self::InvalidApplicationId(id) => write!(formatter, "invalid application ID {id:?}"),
            Self::InvalidApplicationTarget(target) => {
                write!(formatter, "invalid application Space target {target}")
            }
            Self::InvalidMetadata { field, value } => {
                write!(formatter, "invalid {field} value {value:?}")
            }
            Self::InvalidOutputId(id) => write!(formatter, "invalid output ID {id:?}"),
            Self::OutputNotAvailable { space, output_id } => write!(
                formatter,
                "output {output_id:?} is not available for Space {space}"
            ),
            Self::OutputAssignmentNotAllowedInSharedSpan { space, output_id } => write!(
                formatter,
                "Space {space} cannot be assigned to output {output_id:?} in shared-span mode"
            ),
            Self::OutputMigrationToSameOutput(output_id) => write!(
                formatter,
                "output migration source and destination are both {output_id:?}"
            ),
            Self::SpaceIdExhausted => write!(formatter, "no stable Space IDs remain"),
            Self::InvalidPath(path) => {
                write!(formatter, "path has no file name: {}", path.display())
            }
            Self::Io { path, source } => {
                write!(formatter, "I/O error for {}: {source}", path.display())
            }
            Self::Serialize(source) => write!(formatter, "Space serialization failed: {source}"),
            Self::Deserialize(source) => {
                write!(formatter, "Space deserialization failed: {source}")
            }
        }
    }
}

impl Error for SpacesError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Serialize(source) | Self::Deserialize(source) => Some(source),
            _ => None,
        }
    }
}

/// One ordered Space and its shell-facing metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawSpace")]
pub struct Space {
    id: SpaceId,
    name: String,
    wallpaper: Option<String>,
    appearance: Option<String>,
    classification: FullscreenClassification,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    output_id: Option<String>,
    windows: Vec<String>,
}

impl Space {
    pub fn new(id: SpaceId, name: impl Into<String>) -> Result<Self, SpacesError> {
        let space = Self {
            id,
            name: name.into(),
            wallpaper: None,
            appearance: None,
            classification: FullscreenClassification::Normal,
            output_id: None,
            windows: Vec::new(),
        };
        space.validate()?;
        Ok(space)
    }

    pub const fn id(&self) -> SpaceId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn wallpaper(&self) -> Option<&str> {
        self.wallpaper.as_deref()
    }

    pub fn appearance(&self) -> Option<&str> {
        self.appearance.as_deref()
    }

    pub const fn classification(&self) -> FullscreenClassification {
        self.classification
    }

    /// Return the stable compositor output identifier assigned to this Space.
    /// The identifier is metadata only; geometry and output capabilities remain
    /// authoritative in the compositor.
    pub fn output_id(&self) -> Option<&str> {
        self.output_id.as_deref()
    }

    /// Display terminology alias for [`Self::output_id`].
    pub fn display_id(&self) -> Option<&str> {
        self.output_id()
    }

    pub fn windows(&self) -> &[String] {
        &self.windows
    }

    fn validate(&self) -> Result<(), SpacesError> {
        if self.id.get() == 0 {
            return Err(SpacesError::InvalidSpaceId(0));
        }
        validate_space_name(&self.name)?;
        validate_metadata("wallpaper", self.wallpaper.as_deref())?;
        validate_metadata("appearance", self.appearance.as_deref())?;
        if let Some(output_id) = self.output_id.as_deref() {
            validate_output_id(output_id)?;
        }

        let mut seen = BTreeSet::new();
        for window in &self.windows {
            validate_window_id(window)?;
            if !seen.insert(window) {
                return Err(SpacesError::DuplicateWindowId(window.clone()));
            }
        }
        Ok(())
    }
}

#[derive(Deserialize)]
struct RawSpace {
    id: u64,
    name: String,
    #[serde(default)]
    wallpaper: Option<String>,
    #[serde(default)]
    appearance: Option<String>,
    #[serde(default)]
    classification: FullscreenClassification,
    #[serde(default)]
    output_id: Option<String>,
    #[serde(default)]
    windows: Vec<String>,
}

impl TryFrom<RawSpace> for Space {
    type Error = SpacesError;

    fn try_from(raw: RawSpace) -> Result<Self, Self::Error> {
        let id = SpaceId::new(raw.id).ok_or(SpacesError::InvalidSpaceId(raw.id))?;
        let space = Self {
            id,
            name: raw.name,
            wallpaper: raw.wallpaper,
            appearance: raw.appearance,
            classification: raw.classification,
            output_id: raw.output_id,
            windows: raw.windows,
        };
        space.validate()?;
        Ok(space)
    }
}

/// The compositor-owned, shell-facing row for a Space overview.
///
/// This is deliberately a projection: it includes membership count rather
/// than ordinary window records or geometry. `order` is the zero-based order
/// in the compositor's authoritative Space list.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SpaceOverview {
    id: SpaceId,
    order: usize,
    name: String,
    #[serde(rename = "active", alias = "is_active")]
    active: bool,
    classification: FullscreenClassification,
    output_id: Option<String>,
    wallpaper: Option<String>,
    appearance: Option<String>,
    window_count: usize,
}

impl SpaceOverview {
    fn from_space(space: &Space, order: usize, active: bool) -> Self {
        Self {
            id: space.id(),
            order,
            name: space.name().to_owned(),
            active,
            classification: space.classification(),
            output_id: space.output_id().map(str::to_owned),
            wallpaper: space.wallpaper().map(str::to_owned),
            appearance: space.appearance().map(str::to_owned),
            window_count: space.windows().len(),
        }
    }

    pub const fn id(&self) -> SpaceId {
        self.id
    }

    pub const fn order(&self) -> usize {
        self.order
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn active(&self) -> bool {
        self.active
    }

    pub const fn is_active(&self) -> bool {
        self.active()
    }

    pub const fn classification(&self) -> FullscreenClassification {
        self.classification
    }

    pub fn output_id(&self) -> Option<&str> {
        self.output_id.as_deref()
    }

    pub fn display_id(&self) -> Option<&str> {
        self.output_id()
    }

    pub fn wallpaper(&self) -> Option<&str> {
        self.wallpaper.as_deref()
    }

    pub fn appearance(&self) -> Option<&str> {
        self.appearance.as_deref()
    }

    pub const fn window_count(&self) -> usize {
        self.window_count
    }
}

/// Deterministic mutations exposed to a shell-owned overview controller.
///
/// Applying a command changes only the Spaces model. Ordinary window
/// geometry remains compositor-owned elsewhere.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum SpacesCommand {
    Select { id: SpaceId },
    Create { name: String },
    Rename { id: SpaceId, name: String },
    Reorder { id: SpaceId, order: usize },
    Remove { id: SpaceId },
}

/// Compatibility aliases for callers that name the command after the
/// overview surface rather than the underlying Spaces model.
pub type SpaceOverviewEntry = SpaceOverview;
pub type SpaceOverviewCommand = SpacesCommand;
pub type SpacesOverviewCommand = SpacesCommand;

/// Pure, serializable dynamic Spaces state. Window IDs are opaque strings; the
/// compositor remains the owner of actual window geometry and protocol state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawSpacesModel")]
pub struct SpacesModel {
    spaces: Vec<Space>,
    active_space: SpaceId,
    next_space_id: NonZeroU64,
    multi_monitor_policy: MultiMonitorPolicy,
    #[serde(default)]
    application_policies: BTreeMap<String, SpaceTarget>,
}

impl Default for SpacesModel {
    fn default() -> Self {
        Self::new()
    }
}

impl SpacesModel {
    pub fn new() -> Self {
        let first = SpaceId::new(1).expect("literal one is a valid Space ID");
        Self {
            spaces: vec![Space::new(first, "Space 1").expect("default Space name is valid")],
            active_space: first,
            next_space_id: NonZeroU64::new(2).expect("literal two is a valid Space ID"),
            multi_monitor_policy: MultiMonitorPolicy::SharedSpan,
            application_policies: BTreeMap::new(),
        }
    }

    /// Build the default ordered desktop set used when no persisted Spaces
    /// state exists.  IDs remain stable for the lifetime of the model and the
    /// model is still fully dynamic after construction.
    pub fn with_default_count(count: usize) -> Result<Self, SpacesError> {
        if count == 0 {
            return Err(SpacesError::EmptySpaces);
        }
        let mut model = Self::new();
        model.spaces[0] = Space::new(model.spaces[0].id(), "Desktop 1")?;
        for index in 1..count {
            model.create_space(format!("Desktop {}", index + 1))?;
        }
        Ok(model)
    }

    pub fn with_initial_name(name: impl Into<String>) -> Result<Self, SpacesError> {
        let mut model = Self::new();
        let first = model.active_space;
        model.spaces[0] = Space::new(first, name)?;
        Ok(model)
    }

    pub fn spaces(&self) -> &[Space] {
        &self.spaces
    }

    pub fn space_ids(&self) -> Vec<SpaceId> {
        self.spaces.iter().map(Space::id).collect()
    }

    /// Return the ordered overview projection owned by the compositor.
    ///
    /// The projection intentionally contains no ordinary window geometry or
    /// records. It is safe to serialize and hand to a shell overview.
    pub fn overview(&self) -> Vec<SpaceOverview> {
        self.spaces
            .iter()
            .enumerate()
            .map(|(order, space)| {
                SpaceOverview::from_space(space, order, space.id() == self.active_space)
            })
            .collect()
    }

    /// Alias that makes the projection boundary explicit to callers.
    pub fn overview_projection(&self) -> Vec<SpaceOverview> {
        self.overview()
    }

    pub const fn active_space(&self) -> SpaceId {
        self.active_space
    }

    pub fn active(&self) -> &Space {
        self.space(self.active_space)
            .expect("validated model always has an active Space")
    }

    pub fn space(&self, id: SpaceId) -> Option<&Space> {
        self.spaces.iter().find(|space| space.id == id)
    }

    pub fn space_by_name(&self, name: &str) -> Option<&Space> {
        self.spaces.iter().find(|space| space.name == name)
    }

    pub fn position_of(&self, id: SpaceId) -> Option<usize> {
        self.spaces.iter().position(|space| space.id == id)
    }

    /// Return the active Space's current ordered index.
    pub fn active_index(&self) -> usize {
        self.position_of(self.active_space)
            .expect("validated model always contains active Space")
    }

    /// Activate the next ordered Space, wrapping at the end.
    pub fn cycle_next(&mut self) -> SpaceId {
        let next = (self.active_index() + 1) % self.spaces.len();
        let id = self.spaces[next].id();
        self.active_space = id;
        id
    }

    /// Activate the previous ordered Space, wrapping at the beginning.
    pub fn cycle_previous(&mut self) -> SpaceId {
        let index = self.active_index();
        let previous = if index == 0 {
            self.spaces.len() - 1
        } else {
            index - 1
        };
        let id = self.spaces[previous].id();
        self.active_space = id;
        id
    }

    pub const fn multi_monitor_policy(&self) -> MultiMonitorPolicy {
        self.multi_monitor_policy
    }

    pub fn set_multi_monitor_policy(&mut self, policy: MultiMonitorPolicy) {
        if policy == MultiMonitorPolicy::SharedSpan {
            // Display partitioning has no meaning in shared-span mode. Clear
            // only the assignment metadata while preserving Space IDs, order,
            // windows, and presentation metadata for a stable state model.
            for space in &mut self.spaces {
                space.output_id = None;
            }
        }
        self.multi_monitor_policy = policy;
    }

    /// Return the output assignment for a Space, if that Space exists and has
    /// an assignment.
    pub fn output_for_space(&self, id: SpaceId) -> Option<&str> {
        self.space(id).and_then(Space::output_id)
    }

    /// Display terminology alias for [`Self::output_for_space`].
    pub fn display_for_space(&self, id: SpaceId) -> Option<&str> {
        self.output_for_space(id)
    }

    /// Set or clear a Space's output assignment.
    ///
    /// Shared-span mode intentionally rejects a concrete assignment: all
    /// outputs observe the one ordered Space set. Switching into shared-span
    /// mode through [`Self::set_multi_monitor_policy`] clears existing
    /// assignments safely.
    pub fn set_space_output(
        &mut self,
        id: SpaceId,
        output_id: Option<String>,
    ) -> Result<(), SpacesError> {
        self.space(id).ok_or(SpacesError::SpaceNotFound(id))?;
        if let Some(output_id) = output_id.as_deref() {
            validate_output_id(output_id)?;
            if self.multi_monitor_policy == MultiMonitorPolicy::SharedSpan {
                return Err(SpacesError::OutputAssignmentNotAllowedInSharedSpan {
                    space: id,
                    output_id: output_id.to_owned(),
                });
            }
        }
        self.space_mut(id)?.output_id = output_id;
        Ok(())
    }

    /// Set or clear a Space's output assignment against the compositor's
    /// current output inventory.
    ///
    /// Syntax validation alone is insufficient after a connector disappears:
    /// accepting a stale name would persist an assignment that can never be
    /// presented.  Validate the entire inventory before mutating the model,
    /// then reject an unavailable requested output without changing the
    /// existing assignment.
    pub fn set_space_output_with_inventory<I, S>(
        &mut self,
        id: SpaceId,
        output_id: Option<String>,
        available_outputs: I,
    ) -> Result<(), SpacesError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let available_outputs = collect_output_inventory(available_outputs)?;
        if let Some(output_id) = output_id.as_deref() {
            if !available_outputs.contains(output_id) {
                return Err(SpacesError::OutputNotAvailable {
                    space: id,
                    output_id: output_id.to_owned(),
                });
            }
        }
        self.set_space_output(id, output_id)
    }

    /// Reconcile persisted output assignments after the compositor observes a
    /// new output topology.
    ///
    /// Assignments to disconnected outputs are cleared, preserving the Space
    /// identity, ordering, windows and other metadata.  The returned IDs are
    /// the rows whose assignment changed so the caller can publish one
    /// authoritative snapshot and persist the repaired state.  Shared-span
    /// mode has no per-output assignments, so any legacy values are cleared as
    /// part of the same recovery operation.
    pub fn reconcile_output_assignments<I, S>(
        &mut self,
        available_outputs: I,
    ) -> Result<Vec<SpaceId>, SpacesError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let available_outputs = collect_output_inventory(available_outputs)?;
        let stale = self
            .spaces
            .iter()
            .filter_map(|space| {
                let output_id = space.output_id.as_deref()?;
                let stale = self.multi_monitor_policy == MultiMonitorPolicy::SharedSpan
                    || !available_outputs.contains(output_id);
                stale.then_some(space.id)
            })
            .collect::<Vec<_>>();

        for id in &stale {
            self.space_mut(*id)?.output_id = None;
        }
        self.validate()?;
        Ok(stale)
    }

    /// Assign a Space to one output in independent-per-display mode.
    pub fn assign_space_to_output(
        &mut self,
        id: SpaceId,
        output_id: impl Into<String>,
    ) -> Result<(), SpacesError> {
        self.set_space_output(id, Some(output_id.into()))
    }

    /// Display terminology alias for [`Self::assign_space_to_output`].
    pub fn assign_space_to_display(
        &mut self,
        id: SpaceId,
        display_id: impl Into<String>,
    ) -> Result<(), SpacesError> {
        self.assign_space_to_output(id, display_id)
    }

    /// Clear a Space's output assignment without changing its identity or
    /// ordering. This is the safe destination for a disconnected output.
    pub fn clear_space_output(&mut self, id: SpaceId) -> Result<(), SpacesError> {
        self.set_space_output(id, None)
    }

    /// Display terminology alias for [`Self::clear_space_output`].
    pub fn clear_space_display(&mut self, id: SpaceId) -> Result<(), SpacesError> {
        self.clear_space_output(id)
    }

    /// Return the persisted application-ID policy map. The compositor owns
    /// mutations; callers use this read-only view to build authoritative
    /// session snapshots.
    pub fn application_policies(&self) -> &BTreeMap<String, SpaceTarget> {
        &self.application_policies
    }

    /// Resolve an application ID to its configured target. An absent policy
    /// (and a policy cleared with `Current`) follows the active Space.
    pub fn application_target_for(&self, app_id: &str) -> Result<SpaceTarget, SpacesError> {
        validate_application_id(app_id)?;
        Ok(self
            .application_policies
            .get(app_id)
            .cloned()
            .unwrap_or(SpaceTarget::Current))
    }

    /// Set or clear the target applied to newly mapped windows for `app_id`.
    /// `Current` removes the persisted policy and restores active-Space
    /// placement. Named targets are deliberately not persisted because the
    /// session bus exposes stable IDs for policy authority.
    pub fn set_application_policy(
        &mut self,
        app_id: impl Into<String>,
        target: SpaceTarget,
    ) -> Result<(), SpacesError> {
        let app_id = app_id.into();
        validate_application_id(&app_id)?;
        match target {
            SpaceTarget::Current => {
                self.application_policies.remove(&app_id);
            }
            SpaceTarget::Id(id) => {
                self.space(id).ok_or(SpacesError::SpaceNotFound(id))?;
                self.application_policies
                    .insert(app_id, SpaceTarget::Id(id));
            }
            SpaceTarget::All => {
                self.application_policies.insert(app_id, SpaceTarget::All);
            }
            SpaceTarget::Named(name) => {
                return Err(SpacesError::InvalidApplicationTarget(name));
            }
        }
        Ok(())
    }

    /// Assign a live window according to its compositor-observed app ID.
    pub fn assign_window_for_application(
        &mut self,
        window: impl Into<String>,
        app_id: &str,
    ) -> Result<(), SpacesError> {
        let target = self.application_target_for(app_id)?;
        self.assign_window(window, target)
    }

    /// Return Spaces visible on an output under the active policy.
    ///
    /// Shared-span mode returns the complete ordered set. Independent mode
    /// returns only Spaces explicitly assigned to `output_id`; unassigned
    /// Spaces are retained for deterministic restore fallback.
    pub fn spaces_for_output(&self, output_id: &str) -> Result<Vec<SpaceId>, SpacesError> {
        validate_output_id(output_id)?;
        if self.multi_monitor_policy == MultiMonitorPolicy::SharedSpan {
            return Ok(self.space_ids());
        }
        Ok(self
            .spaces
            .iter()
            .filter(|space| space.output_id.as_deref() == Some(output_id))
            .map(Space::id)
            .collect())
    }

    /// Display terminology alias for [`Self::spaces_for_output`].
    pub fn spaces_for_display(&self, display_id: &str) -> Result<Vec<SpaceId>, SpacesError> {
        self.spaces_for_output(display_id)
    }

    /// Return the ordered overview rows visible on an output under the active
    /// multi-monitor policy.
    pub fn overview_for_output(&self, output_id: &str) -> Result<Vec<SpaceOverview>, SpacesError> {
        let visible = self.spaces_for_output(output_id)?;
        Ok(self
            .overview()
            .into_iter()
            .filter(|space| visible.contains(&space.id()))
            .collect())
    }

    /// Display terminology alias for [`Self::overview_for_output`].
    pub fn overview_for_display(
        &self,
        display_id: &str,
    ) -> Result<Vec<SpaceOverview>, SpacesError> {
        self.overview_for_output(display_id)
    }

    /// Select a stable Space when restoring a display layout.
    ///
    /// In shared-span mode, an existing preferred Space wins and the active
    /// Space is the fallback. In independent mode, a preferred Space already
    /// assigned to the requested output (or still unassigned after a display
    /// change) wins; then the first assigned Space, the first unassigned Space,
    /// and finally the active Space are considered. The method selects state;
    /// it does not alter compositor geometry or claim an output.
    pub fn restore_space_for_output(
        &self,
        output_id: Option<&str>,
        preferred: Option<SpaceId>,
    ) -> Result<SpaceId, SpacesError> {
        if let Some(output_id) = output_id {
            validate_output_id(output_id)?;
        }

        let preferred = preferred.filter(|id| self.space(*id).is_some());
        if self.multi_monitor_policy == MultiMonitorPolicy::SharedSpan {
            return Ok(preferred.unwrap_or(self.active_space));
        }

        if let Some(output_id) = output_id {
            if let Some(id) = preferred {
                let matches_output = self
                    .space(id)
                    .map(|space| {
                        space.output_id.is_none() || space.output_id.as_deref() == Some(output_id)
                    })
                    .unwrap_or(false);
                if matches_output {
                    return Ok(id);
                }
            }
            if let Some(space) = self
                .spaces
                .iter()
                .find(|space| space.output_id.as_deref() == Some(output_id))
            {
                return Ok(space.id);
            }
            if let Some(space) = self.spaces.iter().find(|space| space.output_id.is_none()) {
                return Ok(space.id);
            }
            return Ok(self.active_space);
        }

        if let Some(id) = preferred {
            if self
                .space(id)
                .map(|space| space.output_id.is_none())
                .unwrap_or(false)
            {
                return Ok(id);
            }
        }
        if let Some(space) = self.spaces.iter().find(|space| space.output_id.is_none()) {
            return Ok(space.id);
        }
        Ok(preferred.unwrap_or(self.active_space))
    }

    /// Display terminology alias for [`Self::restore_space_for_output`].
    pub fn restore_space_for_display(
        &self,
        display_id: Option<&str>,
        preferred: Option<SpaceId>,
    ) -> Result<SpaceId, SpacesError> {
        self.restore_space_for_output(display_id, preferred)
    }

    /// Migrate every Space assigned to `source_output` to another output or
    /// leave it unassigned when the source output disappeared. Space IDs,
    /// ordering, window membership, and other metadata remain unchanged.
    pub fn migrate_output(
        &mut self,
        source_output: &str,
        replacement_output: Option<&str>,
    ) -> Result<Vec<SpaceId>, SpacesError> {
        validate_output_id(source_output)?;
        if let Some(replacement_output) = replacement_output {
            validate_output_id(replacement_output)?;
            if replacement_output == source_output {
                return Err(SpacesError::OutputMigrationToSameOutput(
                    source_output.to_owned(),
                ));
            }
        }

        if self.multi_monitor_policy == MultiMonitorPolicy::SharedSpan {
            return Ok(Vec::new());
        }

        let mut migrated = Vec::new();
        for space in &mut self.spaces {
            if space.output_id.as_deref() == Some(source_output) {
                space.output_id = replacement_output.map(str::to_owned);
                migrated.push(space.id);
            }
        }
        Ok(migrated)
    }

    /// Remove an output assignment while preserving the affected Spaces for a
    /// later restore decision.
    pub fn remove_output(&mut self, output_id: &str) -> Result<Vec<SpaceId>, SpacesError> {
        self.migrate_output(output_id, None)
    }

    pub fn create_space(&mut self, name: impl Into<String>) -> Result<SpaceId, SpacesError> {
        let name = name.into();
        validate_space_name(&name)?;
        self.ensure_unique_name(&name, None)?;

        let id = SpaceId(self.next_space_id);
        let next = id
            .get()
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .ok_or(SpacesError::SpaceIdExhausted)?;
        self.next_space_id = next;
        self.spaces.push(Space::new(id, name)?);
        Ok(id)
    }

    pub fn rename_space(
        &mut self,
        id: SpaceId,
        name: impl Into<String>,
    ) -> Result<(), SpacesError> {
        let name = name.into();
        validate_space_name(&name)?;
        self.space(id).ok_or(SpacesError::SpaceNotFound(id))?;
        self.ensure_unique_name(&name, Some(id))?;
        self.space_mut(id)?.name = name;
        Ok(())
    }

    pub fn activate_space(&mut self, id: SpaceId) -> Result<(), SpacesError> {
        self.space(id).ok_or(SpacesError::SpaceNotFound(id))?;
        self.active_space = id;
        Ok(())
    }

    /// Select a Space for the overview without exposing mutable Space state.
    pub fn select_space(&mut self, id: SpaceId) -> Result<SpaceId, SpacesError> {
        self.activate_space(id)?;
        Ok(id)
    }

    pub fn reorder_space(&mut self, id: SpaceId, new_index: usize) -> Result<(), SpacesError> {
        if new_index >= self.spaces.len() {
            return Err(SpacesError::InvalidOrderIndex {
                index: new_index,
                len: self.spaces.len(),
            });
        }
        let old_index = self.position_of(id).ok_or(SpacesError::SpaceNotFound(id))?;
        let space = self.spaces.remove(old_index);
        self.spaces.insert(new_index, space);
        Ok(())
    }

    /// Apply one overview command and return its deterministic affected or
    /// selected Space ID. The command path delegates to the existing model
    /// operations so their validation, output policy, and safe removal
    /// fallback remain authoritative.
    pub fn apply_command(&mut self, command: SpacesCommand) -> Result<SpaceId, SpacesError> {
        match command {
            SpacesCommand::Select { id } => self.select_space(id),
            SpacesCommand::Create { name } => self.create_space(name),
            SpacesCommand::Rename { id, name } => {
                self.rename_space(id, name)?;
                Ok(id)
            }
            SpacesCommand::Reorder { id, order } => {
                self.reorder_space(id, order)?;
                Ok(id)
            }
            SpacesCommand::Remove { id } => self.remove_space(id),
        }
    }

    /// Alias for callers dispatching commands from a Spaces overview.
    pub fn apply_overview_command(
        &mut self,
        command: SpacesOverviewCommand,
    ) -> Result<SpaceId, SpacesError> {
        self.apply_command(command)
    }

    /// Remove a Space and return the active fallback. Independent-per-display
    /// mode first prefers another Space on the removed Space's output; for an
    /// active removal, the following ordered Space is then preferred, followed
    /// by the preceding one. Any window that would otherwise lose all
    /// membership is moved to that fallback so the model never strands it.
    pub fn remove_space(&mut self, id: SpaceId) -> Result<SpaceId, SpacesError> {
        let index = self.position_of(id).ok_or(SpacesError::SpaceNotFound(id))?;
        if self.spaces.len() == 1 {
            return Err(SpacesError::CannotRemoveLastSpace);
        }
        let removed_output = self.spaces[index].output_id.clone();
        let fallback = self.fallback_space_for_removal(id, removed_output.as_deref());
        let removed = self.spaces.remove(index);
        if id == self.active_space {
            self.active_space = fallback;
        }

        for window in removed.windows {
            if !self
                .spaces
                .iter()
                .any(|space| space.windows.iter().any(|candidate| candidate == &window))
            {
                self.space_mut(fallback)?.windows.push(window);
            }
        }
        for target in self.application_policies.values_mut() {
            if matches!(target, SpaceTarget::Id(target_id) if *target_id == id) {
                *target = SpaceTarget::Id(fallback);
            }
        }
        Ok(fallback)
    }

    pub fn set_wallpaper(
        &mut self,
        id: SpaceId,
        wallpaper: Option<String>,
    ) -> Result<(), SpacesError> {
        validate_metadata("wallpaper", wallpaper.as_deref())?;
        self.space_mut(id)?.wallpaper = wallpaper;
        Ok(())
    }

    pub fn set_appearance(
        &mut self,
        id: SpaceId,
        appearance: Option<String>,
    ) -> Result<(), SpacesError> {
        validate_metadata("appearance", appearance.as_deref())?;
        self.space_mut(id)?.appearance = appearance;
        Ok(())
    }

    pub fn set_classification(
        &mut self,
        id: SpaceId,
        classification: FullscreenClassification,
    ) -> Result<(), SpacesError> {
        self.space_mut(id)?.classification = classification;
        Ok(())
    }

    /// Assign a window to the current Space, a uniquely named Space, or every
    /// Space. Current and named assignments are exclusive; All is inclusive.
    pub fn assign_window(
        &mut self,
        window: impl Into<String>,
        target: SpaceTarget,
    ) -> Result<(), SpacesError> {
        let window = window.into();
        validate_window_id(&window)?;
        let target_ids = self.target_ids(&target)?;

        for space in &mut self.spaces {
            space.windows.retain(|candidate| candidate != &window);
        }
        for id in target_ids {
            self.space_mut(id)?.windows.push(window.clone());
        }
        Ok(())
    }

    pub fn move_window(
        &mut self,
        window: impl Into<String>,
        target: SpaceTarget,
    ) -> Result<(), SpacesError> {
        self.assign_window(window, target)
    }

    /// Move the compositor's currently activated window to a wire-selected
    /// Space target.
    ///
    /// The caller supplies the live mapped-window IDs so a stale activation
    /// cannot mutate the model. Target conversion and validation happen before
    /// [`Self::move_window`] removes the old membership, keeping rejected
    /// commands transactional.
    pub fn move_active_window<I, S>(
        &mut self,
        active_window_id: Option<&str>,
        known_window_ids: I,
        target: slopos_bus::SpaceTargetWire,
    ) -> Result<(), SpacesError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let window_id =
            active_window_id.ok_or_else(|| SpacesError::InvalidWindowId(String::new()))?;
        if !known_window_ids
            .into_iter()
            .any(|candidate| candidate.as_ref() == window_id)
        {
            return Err(SpacesError::InvalidWindowId(window_id.to_owned()));
        }
        let target = application_target_from_wire(target)?;
        self.move_window(window_id.to_owned(), target)
    }

    pub fn assign_window_to_current(
        &mut self,
        window: impl Into<String>,
    ) -> Result<(), SpacesError> {
        self.assign_window(window, SpaceTarget::Current)
    }

    pub fn assign_window_to_named(
        &mut self,
        window: impl Into<String>,
        name: impl Into<String>,
    ) -> Result<(), SpacesError> {
        self.assign_window(window, SpaceTarget::Named(name.into()))
    }

    pub fn assign_window_to_all(&mut self, window: impl Into<String>) -> Result<(), SpacesError> {
        self.assign_window(window, SpaceTarget::All)
    }

    pub fn windows_in_space(&self, id: SpaceId) -> Option<&[String]> {
        self.space(id).map(Space::windows)
    }

    pub fn window_spaces(&self, window: &str) -> Vec<SpaceId> {
        self.spaces
            .iter()
            .filter(|space| space.windows.iter().any(|candidate| candidate == window))
            .map(Space::id)
            .collect()
    }

    pub fn remove_window(&mut self, window: &str) -> bool {
        let mut removed = false;
        for space in &mut self.spaces {
            let old_len = space.windows.len();
            space.windows.retain(|candidate| candidate != window);
            removed |= old_len != space.windows.len();
        }
        removed
    }

    /// Clear session-scoped window membership after loading persisted Space
    /// metadata. Window identifiers belong to one compositor session and must
    /// never be resurrected as live windows after a restart.
    pub fn clear_window_memberships(&mut self) {
        for space in &mut self.spaces {
            space.windows.clear();
        }
    }

    /// Validate invariants before persistence or after a caller deserializes a
    /// value through another serde format.
    pub fn validate(&self) -> Result<(), SpacesError> {
        if self.spaces.is_empty() {
            return Err(SpacesError::EmptySpaces);
        }

        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        let mut maximum_existing = 0;
        for space in &self.spaces {
            space.validate()?;
            if !ids.insert(space.id) {
                return Err(SpacesError::DuplicateSpaceId(space.id));
            }
            let normalized = normalize_name(space.name());
            if !names.insert(normalized) {
                return Err(SpacesError::DuplicateSpaceName(space.name.clone()));
            }
            maximum_existing = maximum_existing.max(space.id.get());
        }

        if !ids.contains(&self.active_space) {
            return Err(SpacesError::ActiveSpaceMissing(self.active_space));
        }
        if self.multi_monitor_policy == MultiMonitorPolicy::SharedSpan {
            if let Some(space) = self.spaces.iter().find(|space| space.output_id.is_some()) {
                return Err(SpacesError::OutputAssignmentNotAllowedInSharedSpan {
                    space: space.id,
                    output_id: space.output_id.clone().expect("checked above"),
                });
            }
        }
        if self.next_space_id.get() <= maximum_existing {
            return Err(SpacesError::NextSpaceIdNotAfterExisting {
                next: self.next_space_id.get(),
                maximum_existing,
            });
        }
        for (app_id, target) in &self.application_policies {
            validate_application_id(app_id)?;
            match target {
                SpaceTarget::Id(id) => {
                    if !ids.contains(id) {
                        return Err(SpacesError::SpaceNotFound(*id));
                    }
                }
                SpaceTarget::All => {}
                SpaceTarget::Current | SpaceTarget::Named(_) => {
                    return Err(SpacesError::InvalidApplicationTarget(format!("{target:?}")));
                }
            }
        }
        Ok(())
    }

    pub fn save_atomic(&self, path: impl AsRef<Path>) -> Result<(), SpacesError> {
        self.validate()?;
        let path = path.as_ref();
        let encoded = serde_json::to_vec_pretty(self).map_err(SpacesError::Serialize)?;
        atomic_write(path, &encoded)
    }

    /// Move an unreadable persisted model out of the active path without
    /// overwriting an earlier quarantine artifact.  The exclusive hard link
    /// makes the destination allocation no-replace; removing the original
    /// only after the link succeeds preserves the bytes if cleanup is
    /// interrupted.  Startup can then recover from the default model while
    /// retaining the original bytes for repair.
    pub fn quarantine_invalid_state(
        path: impl AsRef<Path>,
    ) -> Result<Option<PathBuf>, SpacesError> {
        let path = path.as_ref();
        match fs::symlink_metadata(path) {
            Ok(_) => {}
            Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(SpacesError::Io {
                    path: path.to_path_buf(),
                    source,
                });
            }
        }

        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let file_name = path
            .file_name()
            .ok_or_else(|| SpacesError::InvalidPath(path.to_path_buf()))?
            .to_string_lossy();

        for _ in 0..100 {
            let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
            let candidate = parent.join(format!(".{file_name}.invalid-{counter}"));
            match fs::symlink_metadata(&candidate) {
                Ok(_) => continue,
                Err(source) if source.kind() == io::ErrorKind::NotFound => {}
                Err(source) => {
                    return Err(SpacesError::Io {
                        path: candidate,
                        source,
                    });
                }
            }

            match fs::hard_link(path, &candidate) {
                Ok(()) => {
                    if let Err(source) = fs::remove_file(path) {
                        let _ = fs::remove_file(&candidate);
                        return Err(SpacesError::Io {
                            path: path.to_path_buf(),
                            source,
                        });
                    }
                    return Ok(Some(candidate));
                }
                Err(source) if source.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(None),
                Err(source) => {
                    return Err(SpacesError::Io {
                        path: candidate,
                        source,
                    });
                }
            }
        }

        Err(SpacesError::Io {
            path: path.to_path_buf(),
            source: io::Error::new(
                io::ErrorKind::AlreadyExists,
                "could not allocate a unique Spaces quarantine path",
            ),
        })
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, SpacesError> {
        let path = path.as_ref();
        let bytes = fs::read(path).map_err(|source| SpacesError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        serde_json::from_slice(&bytes).map_err(SpacesError::Deserialize)
    }

    fn ensure_unique_name(&self, name: &str, except: Option<SpaceId>) -> Result<(), SpacesError> {
        let normalized = normalize_name(name);
        if self
            .spaces
            .iter()
            .any(|space| Some(space.id) != except && normalize_name(space.name()) == normalized)
        {
            return Err(SpacesError::DuplicateSpaceName(name.to_owned()));
        }
        Ok(())
    }

    fn target_ids(&self, target: &SpaceTarget) -> Result<Vec<SpaceId>, SpacesError> {
        match target {
            SpaceTarget::Current => Ok(vec![self.active_space]),
            SpaceTarget::Id(id) => self
                .space(*id)
                .map(|space| vec![space.id])
                .ok_or(SpacesError::SpaceNotFound(*id)),
            SpaceTarget::All => Ok(self.space_ids()),
            SpaceTarget::Named(name) => self
                .space_by_name(name)
                .map(|space| vec![space.id])
                .ok_or_else(|| SpacesError::SpaceNameNotFound(name.clone())),
        }
    }

    fn space_mut(&mut self, id: SpaceId) -> Result<&mut Space, SpacesError> {
        self.spaces
            .iter_mut()
            .find(|space| space.id == id)
            .ok_or(SpacesError::SpaceNotFound(id))
    }

    fn fallback_space_for_removal(&self, id: SpaceId, output_id: Option<&str>) -> SpaceId {
        let index = self
            .position_of(id)
            .expect("removal fallback requires an existing Space");

        if self.multi_monitor_policy == MultiMonitorPolicy::IndependentPerDisplay {
            if let Some(output_id) = output_id {
                if let Some((_, space)) = self
                    .spaces
                    .iter()
                    .enumerate()
                    .filter(|(candidate_index, space)| {
                        *candidate_index != index && space.output_id.as_deref() == Some(output_id)
                    })
                    .min_by_key(|(candidate_index, _)| (*candidate_index).abs_diff(index))
                {
                    return space.id;
                }
            }
        }

        if id == self.active_space {
            let fallback_index = if index + 1 < self.spaces.len() {
                index + 1
            } else {
                index - 1
            };
            self.spaces[fallback_index].id
        } else {
            self.active_space
        }
    }
}

#[derive(Deserialize)]
struct RawSpacesModel {
    spaces: Vec<Space>,
    active_space: u64,
    next_space_id: u64,
    #[serde(default)]
    multi_monitor_policy: MultiMonitorPolicy,
    #[serde(default)]
    application_policies: BTreeMap<String, SpaceTarget>,
}

impl TryFrom<RawSpacesModel> for SpacesModel {
    type Error = SpacesError;

    fn try_from(raw: RawSpacesModel) -> Result<Self, Self::Error> {
        let active_space =
            SpaceId::new(raw.active_space).ok_or(SpacesError::InvalidSpaceId(raw.active_space))?;
        let next_space_id = NonZeroU64::new(raw.next_space_id)
            .ok_or(SpacesError::InvalidNextSpaceId(raw.next_space_id))?;
        let model = Self {
            spaces: raw.spaces,
            active_space,
            next_space_id,
            multi_monitor_policy: raw.multi_monitor_policy,
            application_policies: raw.application_policies,
        };
        model.validate()?;
        Ok(model)
    }
}

fn validate_space_name(name: &str) -> Result<(), SpacesError> {
    if name.is_empty()
        || name.trim() != name
        || name
            .chars()
            .any(|character| character == '\0' || character.is_control())
    {
        return Err(SpacesError::InvalidSpaceName(name.to_owned()));
    }
    Ok(())
}

fn normalize_name(name: &str) -> String {
    name.to_lowercase()
}

fn validate_application_id(app_id: &str) -> Result<(), SpacesError> {
    if app_id.is_empty() || app_id.trim() != app_id || app_id.chars().any(char::is_control) {
        return Err(SpacesError::InvalidApplicationId(app_id.to_owned()));
    }
    Ok(())
}

fn validate_output_id(output_id: &str) -> Result<(), SpacesError> {
    if output_id.is_empty()
        || output_id.trim() != output_id
        || output_id
            .chars()
            .any(|character| character == '\0' || character.is_control())
    {
        return Err(SpacesError::InvalidOutputId(output_id.to_owned()));
    }
    Ok(())
}

fn collect_output_inventory<I, S>(available_outputs: I) -> Result<BTreeSet<String>, SpacesError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut inventory = BTreeSet::new();
    for output_id in available_outputs {
        let output_id = output_id.as_ref();
        validate_output_id(output_id)?;
        inventory.insert(output_id.to_owned());
    }
    Ok(inventory)
}

fn validate_window_id(window: &str) -> Result<(), SpacesError> {
    if window.is_empty()
        || window
            .chars()
            .any(|character| character == '\0' || character.is_control())
    {
        return Err(SpacesError::InvalidWindowId(window.to_owned()));
    }
    Ok(())
}

fn validate_metadata(field: &'static str, value: Option<&str>) -> Result<(), SpacesError> {
    if let Some(value) = value {
        if value.is_empty()
            || value
                .chars()
                .any(|character| character == '\0' || character.is_control())
        {
            return Err(SpacesError::InvalidMetadata {
                field,
                value: value.to_owned(),
            });
        }
    }
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), SpacesError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .ok_or_else(|| SpacesError::InvalidPath(path.to_path_buf()))?
        .to_string_lossy();

    let mut temp_path = None;
    let mut temp_file = None;
    for _ in 0..100 {
        let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(".{file_name}.{counter}.tmp"));
        match OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&candidate)
        {
            Ok(file) => {
                temp_path = Some(candidate);
                temp_file = Some(file);
                break;
            }
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(source) => {
                return Err(SpacesError::Io {
                    path: candidate,
                    source,
                });
            }
        }
    }

    let temp_path = temp_path.ok_or_else(|| SpacesError::Io {
        path: path.to_path_buf(),
        source: io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate temporary path",
        ),
    })?;
    let mut temp_file = temp_file.expect("temporary path and file are created together");

    let result = (|| {
        temp_file
            .write_all(bytes)
            .map_err(|source| SpacesError::Io {
                path: temp_path.clone(),
                source,
            })?;
        temp_file.sync_all().map_err(|source| SpacesError::Io {
            path: temp_path.clone(),
            source,
        })?;
        drop(temp_file);
        fs::rename(&temp_path, path).map_err(|source| SpacesError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

pub fn save_spaces_atomic(model: &SpacesModel, path: impl AsRef<Path>) -> Result<(), SpacesError> {
    model.save_atomic(path)
}

pub fn load_spaces(path: impl AsRef<Path>) -> Result<SpacesModel, SpacesError> {
    SpacesModel::load(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_path(label: &str) -> PathBuf {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "slopos-spaces-{label}-{}-{id}.json",
            std::process::id()
        ))
    }

    #[test]
    fn lifecycle_keeps_stable_nonzero_ids_and_unique_names() {
        let mut model = SpacesModel::new();
        let first = model.active_space();
        let second = model.create_space("Work").expect("create work space");

        assert_ne!(first.get(), 0);
        assert_ne!(first, second);
        assert_eq!(model.space_ids(), &[first, second]);
        assert!(model.rename_space(second, "Projects").is_ok());
        assert_eq!(model.space(second).expect("projects").name(), "Projects");
        assert!(matches!(
            model.rename_space(first, "projects"),
            Err(SpacesError::DuplicateSpaceName(_))
        ));

        model.remove_space(second).expect("remove projects");
        let recreated = model.create_space("Recreated").expect("recreate space");
        assert!(recreated.get() > second.get());
    }

    #[test]
    fn default_desktop_set_is_dynamic_after_startup() {
        let mut model = SpacesModel::with_default_count(8).expect("default desktops");
        assert_eq!(model.spaces().len(), 8);
        assert_eq!(model.active().name(), "Desktop 1");

        let created = model.create_space("Projects").expect("create Projects");
        model.activate_space(created).expect("activate Projects");
        assert_eq!(model.active_index(), 8);
        assert_eq!(model.active().name(), "Projects");
        assert_eq!(model.cycle_previous(), SpaceId::new(8).unwrap());
    }

    #[test]
    fn ordering_and_active_space_are_independent() {
        let mut model = SpacesModel::new();
        let first = model.active_space();
        let work = model.create_space("Work").expect("work");
        let play = model.create_space("Play").expect("play");

        model.activate_space(play).expect("activate play");
        model.reorder_space(play, 0).expect("move play first");

        assert_eq!(model.space_ids(), &[play, first, work]);
        assert_eq!(model.active_space(), play);
        assert_eq!(model.position_of(work), Some(2));
    }

    #[test]
    fn move_active_window_updates_authoritative_membership() {
        let mut model = SpacesModel::with_default_count(2).expect("default spaces");
        let source = model.active_space();
        let target = model.space_ids()[1];
        model
            .assign_window("window-7", SpaceTarget::Id(source))
            .expect("assign focused window");

        model
            .move_active_window(
                Some("window-7"),
                ["window-7"],
                slopos_bus::SpaceTargetWire::Id { id: target.get() },
            )
            .expect("move focused window");

        assert_eq!(model.window_spaces("window-7"), vec![target]);
    }

    #[test]
    fn move_active_window_rejects_stale_focus_without_mutation() {
        let mut model = SpacesModel::with_default_count(2).expect("default spaces");
        let target = model.space_ids()[1];
        model
            .assign_window("window-7", SpaceTarget::Current)
            .expect("assign window");
        let before = model.clone();

        let result = model.move_active_window(
            Some("window-stale"),
            ["window-7"],
            slopos_bus::SpaceTargetWire::Id { id: target.get() },
        );

        assert!(matches!(
            result,
            Err(SpacesError::InvalidWindowId(id)) if id == "window-stale"
        ));
        assert_eq!(model, before);
    }

    #[test]
    fn move_active_window_rejects_invalid_target_without_mutation() {
        let mut model = SpacesModel::with_default_count(2).expect("default spaces");
        model
            .assign_window("window-7", SpaceTarget::Current)
            .expect("assign window");
        let before = model.clone();

        let result = model.move_active_window(
            Some("window-7"),
            ["window-7"],
            slopos_bus::SpaceTargetWire::Id { id: 0 },
        );

        assert!(matches!(result, Err(SpacesError::InvalidSpaceId(0))));
        assert_eq!(model, before);
    }

    #[test]
    fn membership_targets_current_named_and_all() {
        let mut model = SpacesModel::new();
        let work = model.create_space("Work").expect("work");
        let play = model.create_space("Play").expect("play");

        model
            .assign_window("finder", SpaceTarget::Current)
            .expect("current assignment");
        model
            .assign_window("editor", SpaceTarget::Named("Work".into()))
            .expect("named assignment");
        model
            .assign_window("terminal", SpaceTarget::All)
            .expect("all assignment");

        assert_eq!(model.window_spaces("finder"), vec![model.space_ids()[0]]);
        assert_eq!(model.window_spaces("editor"), vec![work]);
        assert_eq!(model.window_spaces("terminal"), model.space_ids());
        assert!(model
            .windows_in_space(play)
            .expect("play space")
            .contains(&"terminal".to_string()));

        model
            .move_window("finder", SpaceTarget::Named("Play".into()))
            .expect("move finder");
        assert_eq!(model.window_spaces("finder"), vec![play]);
    }

    #[test]
    fn removing_active_or_last_space_has_safe_fallback() {
        let mut model = SpacesModel::new();
        let first = model.active_space();
        let second = model.create_space("Second").expect("second");
        let third = model.create_space("Third").expect("third");
        model
            .assign_window("only-second", SpaceTarget::Named("Second".into()))
            .expect("assign orphan candidate");
        model.activate_space(second).expect("activate second");

        let fallback = model.remove_space(second).expect("remove active");
        assert_eq!(fallback, third);
        assert_eq!(model.active_space(), third);
        assert_eq!(model.window_spaces("only-second"), vec![third]);

        model.activate_space(first).expect("activate first");
        model.remove_space(third).expect("remove third");
        assert_eq!(model.active_space(), first);
        assert!(matches!(
            model.remove_space(first),
            Err(SpacesError::CannotRemoveLastSpace)
        ));
        assert_eq!(model.space_ids(), &[first]);
    }

    #[test]
    fn policy_and_per_space_presentation_metadata_round_trip() {
        let mut model = SpacesModel::new();
        let fullscreen = model.create_space("Video").expect("video");
        model
            .set_wallpaper(fullscreen, Some("wallpapers/video.png".into()))
            .expect("wallpaper");
        model
            .set_appearance(fullscreen, Some("graphite".into()))
            .expect("appearance");
        model
            .set_classification(fullscreen, FullscreenClassification::Fullscreen)
            .expect("classification");
        model.set_multi_monitor_policy(MultiMonitorPolicy::IndependentPerDisplay);

        assert_eq!(
            model.space(fullscreen).expect("video").wallpaper(),
            Some("wallpapers/video.png")
        );
        assert_eq!(
            model.space(fullscreen).expect("video").appearance(),
            Some("graphite")
        );
        assert_eq!(
            model.space(fullscreen).expect("video").classification(),
            FullscreenClassification::Fullscreen
        );
        assert_eq!(
            model.multi_monitor_policy(),
            MultiMonitorPolicy::IndependentPerDisplay
        );

        let encoded = serde_json::to_string(&model).expect("serialize metadata");
        let decoded: SpacesModel = serde_json::from_str(&encoded).expect("deserialize metadata");
        assert_eq!(decoded, model);
    }

    #[test]
    fn output_assignment_defaults_for_legacy_state_and_round_trips() {
        let legacy = r#"{
            "spaces": [{"id": 1, "name": "Main", "windows": []}],
            "active_space": 1,
            "next_space_id": 2
        }"#;
        let mut model: SpacesModel = serde_json::from_str(legacy).expect("legacy Spaces state");
        let first = model.active_space();

        assert_eq!(model.multi_monitor_policy(), MultiMonitorPolicy::SharedSpan);
        assert_eq!(model.space(first).expect("legacy Space").output_id(), None);

        model.set_multi_monitor_policy(MultiMonitorPolicy::IndependentPerDisplay);
        model
            .assign_space_to_output(first, "DP-1")
            .expect("assign legacy Space");
        let encoded = serde_json::to_string(&model).expect("serialize output assignment");
        let decoded: SpacesModel = serde_json::from_str(&encoded).expect("deserialize assignment");

        assert_eq!(decoded, model);
        assert_eq!(decoded.output_for_space(first), Some("DP-1"));
    }

    #[test]
    fn output_assignment_rejects_unknown_inventory_without_mutating() {
        let mut model = SpacesModel::new();
        let first = model.active_space();
        model.set_multi_monitor_policy(MultiMonitorPolicy::IndependentPerDisplay);
        model
            .assign_space_to_output(first, "DP-1")
            .expect("initial output assignment");

        let result =
            model.set_space_output_with_inventory(first, Some("DP-2".to_string()), ["DP-1"]);

        assert!(matches!(
            result,
            Err(SpacesError::OutputNotAvailable { space, output_id })
                if space == first && output_id == "DP-2"
        ));
        assert_eq!(model.output_for_space(first), Some("DP-1"));
    }

    #[test]
    fn reconcile_output_assignments_clears_disconnected_outputs_transactionally() {
        let mut model = SpacesModel::new();
        let first = model.active_space();
        let second = model.create_space("Work").expect("work");
        model.set_multi_monitor_policy(MultiMonitorPolicy::IndependentPerDisplay);
        model
            .assign_space_to_output(first, "DP-1")
            .expect("first output");
        model
            .assign_space_to_output(second, "DP-2")
            .expect("second output");

        let cleared = model
            .reconcile_output_assignments(["DP-1"])
            .expect("reconcile outputs");

        assert_eq!(cleared, vec![second]);
        assert_eq!(model.output_for_space(first), Some("DP-1"));
        assert_eq!(model.output_for_space(second), None);
        model.validate().expect("reconciled model remains valid");
    }

    #[test]
    fn reconcile_output_assignments_rejects_invalid_inventory_without_mutating() {
        let mut model = SpacesModel::new();
        let first = model.active_space();
        model.set_multi_monitor_policy(MultiMonitorPolicy::IndependentPerDisplay);
        model
            .assign_space_to_output(first, "DP-1")
            .expect("output assignment");

        let result = model.reconcile_output_assignments(["DP-1", " DP-2"]);

        assert!(matches!(result, Err(SpacesError::InvalidOutputId(id)) if id == " DP-2"));
        assert_eq!(model.output_for_space(first), Some("DP-1"));
    }

    #[test]
    fn policy_controls_output_visibility_and_clears_shared_assignments() {
        let mut model = SpacesModel::new();
        let first = model.active_space();
        let second = model.create_space("Work").expect("work");

        model.set_multi_monitor_policy(MultiMonitorPolicy::IndependentPerDisplay);
        model
            .assign_space_to_display(first, "DP-1")
            .expect("assign first output");
        model
            .assign_space_to_output(second, "DP-2")
            .expect("assign second output");
        assert_eq!(
            model.spaces_for_output("DP-1").expect("DP-1 Spaces"),
            vec![first]
        );

        model.set_multi_monitor_policy(MultiMonitorPolicy::SharedSpan);
        assert_eq!(model.output_for_space(first), None);
        assert_eq!(model.output_for_space(second), None);
        assert_eq!(
            model.spaces_for_display("DP-2").expect("shared Spaces"),
            vec![first, second]
        );
        model.validate().expect("shared policy remains valid");
        assert!(matches!(
            model.assign_space_to_output(first, "DP-3"),
            Err(SpacesError::OutputAssignmentNotAllowedInSharedSpan { .. })
        ));
    }

    #[test]
    fn independent_restore_and_output_migration_are_deterministic() {
        let mut model = SpacesModel::new();
        let first = model.active_space();
        let second = model.create_space("Work").expect("work");
        let third = model.create_space("Play").expect("play");

        model.set_multi_monitor_policy(MultiMonitorPolicy::IndependentPerDisplay);
        model
            .assign_space_to_output(first, "DP-1")
            .expect("assign first output");
        model
            .assign_space_to_output(second, "DP-2")
            .expect("assign second output");

        assert_eq!(
            model
                .restore_space_for_output(Some("DP-1"), Some(second))
                .expect("restore DP-1"),
            first
        );
        assert_eq!(
            model
                .restore_space_for_output(Some("DP-2"), Some(second))
                .expect("restore DP-2"),
            second
        );
        assert_eq!(
            model
                .restore_space_for_output(Some("DP-3"), Some(second))
                .expect("restore new output"),
            third
        );

        model
            .assign_window("editor", SpaceTarget::Named("Space 1".into()))
            .expect("assign window");
        let migrated = model
            .migrate_output("DP-1", Some("DP-3"))
            .expect("migrate output");
        assert_eq!(migrated, vec![first]);
        assert_eq!(model.output_for_space(first), Some("DP-3"));
        assert_eq!(model.window_spaces("editor"), vec![first]);

        let removed = model.remove_output("DP-3").expect("remove output");
        assert_eq!(removed, vec![first]);
        assert_eq!(model.output_for_space(first), None);
        assert!(matches!(
            model.migrate_output("DP-2", Some("DP-2")),
            Err(SpacesError::OutputMigrationToSameOutput(_))
        ));
    }

    #[test]
    fn independent_space_removal_prefers_same_output_fallback() {
        let mut model = SpacesModel::new();
        let first = model.active_space();
        let second = model.create_space("Second").expect("second");
        let third = model.create_space("Third").expect("third");

        model.set_multi_monitor_policy(MultiMonitorPolicy::IndependentPerDisplay);
        model
            .assign_space_to_output(first, "DP-1")
            .expect("assign first output");
        model
            .assign_space_to_output(second, "DP-1")
            .expect("assign second output");
        model
            .assign_space_to_output(third, "DP-2")
            .expect("assign third output");
        model
            .assign_window("terminal", SpaceTarget::Current)
            .expect("assign terminal");

        let fallback = model.remove_space(first).expect("remove first");
        assert_eq!(fallback, second);
        assert_eq!(model.window_spaces("terminal"), vec![second]);
        assert_eq!(model.output_for_space(second), Some("DP-1"));
    }

    #[test]
    fn overview_projection_preserves_order_and_only_exposes_window_counts() {
        let mut model = SpacesModel::new();
        let first = model.active_space();
        let work = model.create_space("Work").expect("work");
        let fullscreen = model.create_space("Video").expect("video");

        model
            .assign_window("finder", SpaceTarget::Named("Work".into()))
            .expect("assign finder");
        model
            .assign_window("terminal", SpaceTarget::Named("Video".into()))
            .expect("assign terminal");
        model
            .set_classification(fullscreen, FullscreenClassification::Fullscreen)
            .expect("classify fullscreen");
        model
            .set_wallpaper(fullscreen, Some("wallpapers/video.png".into()))
            .expect("wallpaper");
        model
            .set_appearance(fullscreen, Some("graphite".into()))
            .expect("appearance");

        let overview = model.overview();
        assert_eq!(
            overview.iter().map(SpaceOverview::id).collect::<Vec<_>>(),
            vec![first, work, fullscreen]
        );
        assert_eq!(
            overview
                .iter()
                .map(SpaceOverview::order)
                .collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert_eq!(overview[0].name(), "Space 1");
        assert!(overview[0].is_active());
        assert!(!overview[1].is_active());
        assert_eq!(overview[1].window_count(), 1);
        assert_eq!(
            overview[2].classification(),
            FullscreenClassification::Fullscreen
        );
        assert_eq!(overview[2].wallpaper(), Some("wallpapers/video.png"));
        assert_eq!(overview[2].appearance(), Some("graphite"));

        let encoded = serde_json::to_value(&overview).expect("serialize overview");
        assert_eq!(encoded[0]["active"], true);
        assert_eq!(encoded[1]["window_count"], 1);
        assert!(encoded[1].get("windows").is_none());
    }

    #[test]
    fn overview_selection_marks_exactly_one_active_space() {
        let mut model = SpacesModel::new();
        let first = model.active_space();
        let work = model.create_space("Work").expect("work");

        assert_eq!(model.select_space(work).expect("select work"), work);
        let active = model
            .overview()
            .into_iter()
            .filter(SpaceOverview::is_active)
            .map(|space| space.id())
            .collect::<Vec<_>>();

        assert_eq!(active, vec![work]);
        assert_eq!(model.active_space(), work);
        assert_ne!(first, work);
    }

    #[test]
    fn independent_overview_filters_assigned_output_in_model_order() {
        let mut model = SpacesModel::new();
        let first = model.active_space();
        let second = model.create_space("Work").expect("work");
        let third = model.create_space("Play").expect("play");
        model.set_multi_monitor_policy(MultiMonitorPolicy::IndependentPerDisplay);
        model
            .assign_space_to_output(third, "DP-1")
            .expect("assign third");
        model
            .assign_space_to_output(first, "DP-1")
            .expect("assign first");
        model
            .assign_space_to_output(second, "DP-2")
            .expect("assign second");

        let dp1_overview = model.overview_for_output("DP-1").expect("DP-1 overview");
        let dp2 = model
            .overview_for_display("DP-2")
            .expect("DP-2 overview")
            .into_iter()
            .map(|space| space.id())
            .collect::<Vec<_>>();

        assert_eq!(
            dp1_overview
                .iter()
                .map(SpaceOverview::id)
                .collect::<Vec<_>>(),
            vec![first, third]
        );
        assert_eq!(
            dp1_overview
                .iter()
                .map(SpaceOverview::output_id)
                .collect::<Vec<_>>(),
            vec![Some("DP-1"), Some("DP-1")]
        );
        assert_eq!(dp2, vec![second]);
    }

    #[test]
    fn overview_commands_mutate_safely_and_keep_window_membership() {
        let mut model = SpacesModel::new();
        let first = model.active_space();
        let second = model
            .apply_command(SpacesCommand::Create {
                name: "Work".into(),
            })
            .expect("create work");
        model
            .apply_command(SpacesCommand::Select { id: second })
            .expect("select work");
        model
            .assign_window_to_current("terminal")
            .expect("assign terminal");
        model
            .apply_command(SpacesCommand::Rename {
                id: second,
                name: "Projects".into(),
            })
            .expect("rename work");
        model
            .apply_command(SpacesCommand::Reorder {
                id: second,
                order: 0,
            })
            .expect("reorder work");

        assert_eq!(model.space_ids(), vec![second, first]);
        assert_eq!(model.active_space(), second);
        assert_eq!(model.space(second).expect("projects").name(), "Projects");

        let fallback = model
            .apply_command(SpacesCommand::Remove { id: second })
            .expect("remove projects");
        assert_eq!(fallback, first);
        assert_eq!(model.active_space(), first);
        assert_eq!(model.window_spaces("terminal"), vec![first]);
        assert_eq!(model.space_ids(), vec![first]);

        assert!(matches!(
            model.apply_command(SpacesCommand::Remove { id: first }),
            Err(SpacesError::CannotRemoveLastSpace)
        ));
        assert_eq!(model.space_ids(), vec![first]);
        assert_eq!(model.active_space(), first);
    }

    #[test]
    fn serde_rejects_zero_or_duplicate_ids_and_duplicate_names() {
        let zero_id = r#"{
            "spaces": [{"id": 0, "name": "Main", "windows": []}],
            "active_space": 0,
            "next_space_id": 2,
            "multi_monitor_policy": "shared_span"
        }"#;
        assert!(serde_json::from_str::<SpacesModel>(zero_id).is_err());

        let duplicate_id = r#"{
            "spaces": [
                {"id": 1, "name": "Main", "windows": []},
                {"id": 1, "name": "Other", "windows": []}
            ],
            "active_space": 1,
            "next_space_id": 2,
            "multi_monitor_policy": "shared_span"
        }"#;
        assert!(serde_json::from_str::<SpacesModel>(duplicate_id).is_err());

        let duplicate_name = r#"{
            "spaces": [
                {"id": 1, "name": "Main", "windows": []},
                {"id": 2, "name": "main", "windows": []}
            ],
            "active_space": 1,
            "next_space_id": 3,
            "multi_monitor_policy": "shared_span"
        }"#;
        assert!(serde_json::from_str::<SpacesModel>(duplicate_name).is_err());

        let output_in_shared_span = r#"{
            "spaces": [{
                "id": 1,
                "name": "Main",
                "output_id": "DP-1",
                "windows": []
            }],
            "active_space": 1,
            "next_space_id": 2,
            "multi_monitor_policy": "shared_span"
        }"#;
        assert!(serde_json::from_str::<SpacesModel>(output_in_shared_span).is_err());

        let invalid_output = r#"{
            "spaces": [{
                "id": 1,
                "name": "Main",
                "output_id": "",
                "windows": []
            }],
            "active_space": 1,
            "next_space_id": 2,
            "multi_monitor_policy": "independent_per_display"
        }"#;
        assert!(serde_json::from_str::<SpacesModel>(invalid_output).is_err());
    }

    #[test]
    fn atomic_persistence_round_trips_and_leaves_no_temp_file() {
        let path = temp_path("atomic");
        let _ = fs::remove_file(&path);

        let mut model = SpacesModel::new();
        model.create_space("Work").expect("work");
        model.save_atomic(&path).expect("save spaces atomically");

        let loaded = SpacesModel::load(&path).expect("load spaces");
        assert_eq!(loaded, model);
        let directory = path.parent().expect("temp directory");
        let prefix = format!(".{}.", path.file_name().unwrap().to_string_lossy());
        assert!(!fs::read_dir(directory)
            .expect("read temp directory")
            .filter_map(Result::ok)
            .any(|entry| entry.file_name().to_string_lossy().starts_with(&prefix)));

        let sentinel = temp_path("sentinel");
        let target_directory = temp_path("target-directory");
        fs::write(&sentinel, b"keep this file").expect("write sentinel");
        fs::create_dir(&target_directory).expect("create directory target");
        assert!(model.save_atomic(&target_directory).is_err());
        assert_eq!(
            fs::read(&sentinel).expect("read sentinel"),
            b"keep this file"
        );

        fs::remove_file(path).expect("remove test state");
        fs::remove_file(sentinel).expect("remove sentinel");
        fs::remove_dir(target_directory).expect("remove directory target");
    }

    #[test]
    fn persisted_space_metadata_drops_session_window_membership_on_reload() {
        let mut model = SpacesModel::new();
        let work = model.create_space("Work").expect("work");
        model
            .assign_window("session-window", SpaceTarget::Id(work))
            .expect("assign session window");
        let path = temp_path("restart-membership");
        let _ = fs::remove_file(&path);
        model.save_atomic(&path).expect("save Spaces metadata");

        let mut reloaded = SpacesModel::load(&path).expect("load Spaces metadata");
        reloaded.clear_window_memberships();
        assert!(reloaded.window_spaces("session-window").is_empty());
        assert!(reloaded.space(work).expect("work").windows().is_empty());
        assert_eq!(reloaded.space(work).expect("work").name(), "Work");

        fs::remove_file(path).expect("remove restart fixture");
    }

    #[test]
    fn invalid_persisted_state_is_quarantined_without_overwriting_it() {
        let path = temp_path("quarantine");
        let invalid = br#"{"spaces":[]}"#;
        fs::write(&path, invalid).expect("write invalid Spaces state");

        let quarantined = SpacesModel::quarantine_invalid_state(&path)
            .expect("quarantine should be recoverable")
            .expect("existing state should be moved");

        assert!(!path.exists(), "the corrupt active path must be vacated");
        assert_ne!(quarantined, path);
        assert_eq!(fs::read(&quarantined).expect("read quarantine"), invalid);
        let expected_prefix = format!(
            ".{}.invalid-",
            path.file_name()
                .and_then(|name| name.to_str())
                .expect("fixture has a UTF-8 file name")
        );
        assert!(quarantined
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(&expected_prefix)));

        fs::remove_file(quarantined).expect("remove quarantine fixture");
    }

    #[test]
    fn quarantine_missing_persisted_state_is_a_noop() {
        let path = temp_path("quarantine-missing");
        let _ = fs::remove_file(&path);

        assert_eq!(
            SpacesModel::quarantine_invalid_state(&path).expect("missing state is recoverable"),
            None
        );
    }

    #[test]
    fn application_space_policy_resolves_current_all_and_specific_targets() {
        let mut model = SpacesModel::with_initial_name("Personal").expect("model");
        let work = model.create_space("Work").expect("work");

        assert_eq!(
            model
                .application_target_for("org.example.Editor")
                .expect("default target"),
            SpaceTarget::Current
        );
        model
            .set_application_policy("org.example.Editor", SpaceTarget::Id(work))
            .expect("specific policy");
        assert_eq!(
            model
                .application_target_for("org.example.Editor")
                .expect("specific target"),
            SpaceTarget::Id(work)
        );
        model
            .assign_window_for_application("editor-window", "org.example.Editor")
            .expect("specific assignment");
        assert_eq!(model.window_spaces("editor-window"), vec![work]);

        model
            .set_application_policy("org.example.Editor", SpaceTarget::All)
            .expect("all policy");
        model
            .assign_window_for_application("editor-window", "org.example.Editor")
            .expect("all assignment");
        assert_eq!(model.window_spaces("editor-window"), model.space_ids());

        model
            .set_application_policy("org.example.Editor", SpaceTarget::Current)
            .expect("clear policy");
        assert_eq!(
            model
                .application_target_for("org.example.Editor")
                .expect("cleared target"),
            SpaceTarget::Current
        );
    }

    #[test]
    fn application_space_policy_rejects_invalid_ids_and_unknown_spaces() {
        let mut model = SpacesModel::new();
        assert!(matches!(
            model.set_application_policy("", SpaceTarget::All),
            Err(SpacesError::InvalidApplicationId(_))
        ));
        assert!(matches!(
            model.set_application_policy("org.example.\nEditor", SpaceTarget::All),
            Err(SpacesError::InvalidApplicationId(_))
        ));
        assert!(matches!(
            model.set_application_policy(
                "org.example.Editor",
                SpaceTarget::Id(SpaceId::new(99).unwrap())
            ),
            Err(SpacesError::SpaceNotFound(_))
        ));
        assert!(matches!(
            model.set_application_policy("org.example.Editor", SpaceTarget::Named("Main".into())),
            Err(SpacesError::InvalidApplicationTarget(_))
        ));
    }

    #[test]
    fn application_space_policy_persists_and_retargets_removed_spaces() {
        let mut model = SpacesModel::new();
        let work = model.create_space("Work").expect("work");
        model
            .set_application_policy("org.example.Editor", SpaceTarget::Id(work))
            .expect("policy");
        let path = temp_path("application-policy");
        let _ = fs::remove_file(&path);
        model.save_atomic(&path).expect("save policy");
        let loaded = SpacesModel::load(&path).expect("load policy");
        assert_eq!(loaded, model);

        let fallback = model.remove_space(work).expect("remove work");
        assert_eq!(
            model
                .application_target_for("org.example.Editor")
                .expect("retargeted policy"),
            SpaceTarget::Id(fallback)
        );
        model.validate().expect("retargeted policy remains valid");
        fs::remove_file(path).expect("remove policy fixture");
    }
}
