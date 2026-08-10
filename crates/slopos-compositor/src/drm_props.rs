//! Generic DRM/KMS property access and capability-driven HDR/VRR control.
//!
//! Every capability reported here comes from real kernel object properties.
//! Unsupported connectors remain unsupported; user policy never fabricates an
//! HDR or variable-refresh path.
//!
//! References:
//! - `include/uapi/drm/drm_mode.h` — HDR metadata structures;
//! - `drivers/gpu/drm/drm_connector.c` — connector properties;
//! - CTA-861-G §6.9 — static HDR metadata encoding.

use std::collections::HashMap;
use std::io;

// Use Smithay's re-export so the DRM version always matches the backend.
use drm::control::{property, Device as ControlDevice, ResourceHandle};
use smithay::reexports::drm;

/// Kernel EOTF values for `hdr_metadata_infoframe.eotf`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Eotf {
    TraditionalSdr = 0,
    TraditionalHdr = 1,
    St2084 = 2,
    Hlg = 3,
}

/// One CIE 1931 xy coordinate pair in 0.00002 units.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChromaticityPoint {
    pub x: u16,
    pub y: u16,
}

impl ChromaticityPoint {
    pub fn from_xy(x: f32, y: f32) -> Self {
        fn encode(value: f32) -> u16 {
            let value = if value.is_finite() { value } else { 0.0 };
            (value.clamp(0.0, 1.0) * 50_000.0).round() as u16
        }
        Self {
            x: encode(x),
            y: encode(y),
        }
    }
}

/// Exact `struct hdr_metadata_infoframe` layout from the kernel UAPI.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HdrMetadataInfoframe {
    pub eotf: u8,
    pub metadata_type: u8,
    pub display_primaries: [ChromaticityPoint; 3],
    pub white_point: ChromaticityPoint,
    pub max_display_mastering_luminance: u16,
    pub min_display_mastering_luminance: u16,
    pub max_cll: u16,
    pub max_fall: u16,
}

/// Exact `struct hdr_output_metadata` payload used by
/// `HDR_OUTPUT_METADATA` connector properties.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HdrOutputMetadata {
    pub metadata_type: u32,
    pub hdmi_metadata_type1: HdrMetadataInfoframe,
}

impl HdrOutputMetadata {
    /// HDR10: PQ transfer, BT.2020 primaries and D65 white point.
    pub fn hdr10(
        max_mastering_nits: u16,
        min_mastering_nits: f32,
        max_cll: u16,
        max_fall: u16,
    ) -> Self {
        let minimum = if min_mastering_nits.is_finite() {
            min_mastering_nits.max(0.0)
        } else {
            0.0
        };
        Self {
            metadata_type: 0,
            hdmi_metadata_type1: HdrMetadataInfoframe {
                eotf: Eotf::St2084 as u8,
                metadata_type: 0,
                display_primaries: [
                    ChromaticityPoint::from_xy(0.708, 0.292),
                    ChromaticityPoint::from_xy(0.170, 0.797),
                    ChromaticityPoint::from_xy(0.131, 0.046),
                ],
                white_point: ChromaticityPoint::from_xy(0.3127, 0.3290),
                max_display_mastering_luminance: max_mastering_nits,
                min_display_mastering_luminance: (minimum * 10_000.0)
                    .round()
                    .clamp(0.0, f32::from(u16::MAX))
                    as u16,
                max_cll,
                max_fall,
            },
        }
    }

    pub fn sdr() -> Self {
        Self {
            metadata_type: 0,
            hdmi_metadata_type1: HdrMetadataInfoframe {
                eotf: Eotf::TraditionalSdr as u8,
                metadata_type: 0,
                ..Default::default()
            },
        }
    }
}

/// One property as reported for a specific DRM object.
#[derive(Debug, Clone)]
pub struct PropEntry {
    pub handle: property::Handle,
    pub name: String,
    pub value_type: property::ValueType,
    pub raw_value: u64,
}

impl PropEntry {
    pub fn enum_name(&self) -> Option<String> {
        match &self.value_type {
            property::ValueType::Enum(values) => values
                .get_value_from_raw_value(self.raw_value)
                .map(|value| value.name().to_string_lossy().into_owned()),
            _ => None,
        }
    }

    pub fn enum_value(&self, name: &str) -> Option<u64> {
        match &self.value_type {
            property::ValueType::Enum(values) => {
                let (_, enums) = values.values();
                enums
                    .iter()
                    .find(|entry| entry.name().to_string_lossy() == name)
                    .map(|entry| entry.value())
            }
            _ => None,
        }
    }

    pub fn range(&self) -> Option<(u64, u64)> {
        match self.value_type {
            property::ValueType::UnsignedRange(low, high) => Some((low, high)),
            _ => None,
        }
    }
}

/// Snapshot of every property on one DRM object, keyed by kernel name.
#[derive(Debug, Clone, Default)]
pub struct PropertyIndex {
    by_name: HashMap<String, PropEntry>,
}

impl PropertyIndex {
    pub fn read<D, H>(device: &D, handle: H) -> io::Result<Self>
    where
        D: ControlDevice,
        H: ResourceHandle,
    {
        let set = device.get_properties(handle)?;
        let (handles, raw_values) = set.as_props_and_values();
        let mut by_name = HashMap::with_capacity(handles.len());
        for (handle, raw_value) in handles.iter().zip(raw_values.iter()) {
            let info = device.get_property(*handle)?;
            let name = info.name().to_string_lossy().into_owned();
            by_name.insert(
                name.clone(),
                PropEntry {
                    handle: *handle,
                    name,
                    value_type: info.value_type(),
                    raw_value: *raw_value,
                },
            );
        }
        Ok(Self { by_name })
    }

    pub fn get(&self, name: &str) -> Option<&PropEntry> {
        self.by_name.get(name)
    }

    pub fn has(&self, name: &str) -> bool {
        self.by_name.contains_key(name)
    }

    pub fn names(&self) -> Vec<&str> {
        let mut names: Vec<_> = self.by_name.keys().map(String::as_str).collect();
        names.sort_unstable();
        names
    }
}

/// Set a property by name, returning false when the object does not expose it.
pub fn set_property_by_name<D, H>(
    device: &D,
    handle: H,
    index: &PropertyIndex,
    name: &str,
    value: u64,
) -> io::Result<bool>
where
    D: ControlDevice,
    H: ResourceHandle,
{
    let Some(entry) = index.get(name) else {
        return Ok(false);
    };
    device.set_property(handle, entry.handle, value)?;
    Ok(true)
}

// ---------------------------------------------------------------------------
// VRR
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VrrState {
    pub capable: bool,
    pub controllable: bool,
    pub enabled: bool,
}

pub fn probe_vrr(connector_props: &PropertyIndex, crtc_props: &PropertyIndex) -> VrrState {
    let capable = connector_props
        .get("vrr_capable")
        .map(|property| property.raw_value != 0)
        .unwrap_or(false);
    let enabled_property = crtc_props.get("VRR_ENABLED");
    VrrState {
        capable,
        controllable: enabled_property.is_some(),
        enabled: enabled_property
            .map(|property| property.raw_value != 0)
            .unwrap_or(false),
    }
}

fn vrr_request_allowed(state: VrrState, enable: bool) -> bool {
    state.controllable && (!enable || state.capable)
}

/// Turn variable refresh on or off for a CRTC.
///
/// A missing `VRR_ENABLED` property is not success, and enable is rejected when
/// the connector does not advertise `vrr_capable`.
pub fn set_vrr_enabled<D>(
    device: &D,
    crtc: drm::control::crtc::Handle,
    crtc_props: &PropertyIndex,
    state: VrrState,
    enable: bool,
) -> io::Result<bool>
where
    D: ControlDevice,
{
    if !vrr_request_allowed(state, enable) {
        return Ok(false);
    }
    if state.enabled == enable {
        return Ok(true);
    }
    set_property_by_name(device, crtc, crtc_props, "VRR_ENABLED", u64::from(enable))
}

// ---------------------------------------------------------------------------
// HDR
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct HdrConnectorCaps {
    pub has_hdr_metadata: bool,
    pub has_bt2020_colorspace: bool,
    pub max_bpc: Option<u64>,
    pub colorspaces: Vec<String>,
}

impl HdrConnectorCaps {
    pub fn hdr10_capable(&self) -> bool {
        self.has_hdr_metadata && self.has_bt2020_colorspace && self.max_bpc.unwrap_or(8) >= 10
    }

    pub fn summary(&self) -> String {
        format!(
            "hdr_metadata={} bt2020_colorspace={} max_bpc={} => hdr10_capable={}",
            self.has_hdr_metadata,
            self.has_bt2020_colorspace,
            self.max_bpc
                .map(|value| value.to_string())
                .unwrap_or_else(|| "n/a".to_owned()),
            self.hdr10_capable()
        )
    }
}

pub const COLORSPACE_BT2020_RGB: &str = "BT2020_RGB";
pub const COLORSPACE_DEFAULT: &str = "Default";

pub fn probe_hdr(connector_props: &PropertyIndex) -> HdrConnectorCaps {
    let colorspaces = connector_props
        .get("Colorspace")
        .and_then(|property| match &property.value_type {
            property::ValueType::Enum(values) => {
                let (_, entries) = values.values();
                Some(
                    entries
                        .iter()
                        .map(|entry| entry.name().to_string_lossy().into_owned())
                        .collect::<Vec<_>>(),
                )
            }
            _ => None,
        })
        .unwrap_or_default();

    HdrConnectorCaps {
        has_hdr_metadata: connector_props.has("HDR_OUTPUT_METADATA"),
        has_bt2020_colorspace: colorspaces
            .iter()
            .any(|value| value == COLORSPACE_BT2020_RGB),
        max_bpc: connector_props
            .get("max bpc")
            .and_then(|property| property.range().map(|(_, high)| high)),
        colorspaces,
    }
}

#[derive(Clone, Copy, Debug)]
struct PropertySnapshot {
    max_bpc: Option<(property::Handle, u64)>,
    colorspace: Option<(property::Handle, u64)>,
    metadata: Option<(property::Handle, u64)>,
}

impl PropertySnapshot {
    fn capture(properties: &PropertyIndex) -> Self {
        Self {
            max_bpc: properties
                .get("max bpc")
                .map(|entry| (entry.handle, entry.raw_value)),
            colorspace: properties
                .get("Colorspace")
                .map(|entry| (entry.handle, entry.raw_value)),
            metadata: properties
                .get("HDR_OUTPUT_METADATA")
                .map(|entry| (entry.handle, entry.raw_value)),
        }
    }

    fn restore<D>(self, device: &D, connector: drm::control::connector::Handle)
    where
        D: ControlDevice,
    {
        // Restore in reverse order of the normal HDR apply path. Rollback is
        // best-effort because the original operation's error is the useful one.
        if let Some((handle, value)) = self.metadata {
            let _ = device.set_property(connector, handle, value);
        }
        if let Some((handle, value)) = self.colorspace {
            let _ = device.set_property(connector, handle, value);
        }
        if let Some((handle, value)) = self.max_bpc {
            let _ = device.set_property(connector, handle, value);
        }
    }
}

fn bpc_target(range: Option<(u64, u64)>, desired: u64) -> Option<u64> {
    let (low, high) = range?;
    (low <= high).then(|| desired.clamp(low, high))
}

/// Drive a connector into HDR10.
///
/// The three property changes are not an atomic KMS commit in this bootstrap
/// path, so this function explicitly snapshots and rolls back every changed
/// property if any later step fails. The returned blob handle remains owned by
/// the caller until [`clear_hdr`] moves the connector away from it.
pub fn apply_hdr10<D>(
    device: &D,
    connector: drm::control::connector::Handle,
    properties: &PropertyIndex,
    metadata: &HdrOutputMetadata,
) -> io::Result<Option<property::Value<'static>>>
where
    D: ControlDevice,
{
    let capabilities = probe_hdr(properties);
    if !capabilities.hdr10_capable() {
        return Ok(None);
    }

    let snapshot = PropertySnapshot::capture(properties);
    let metadata_property = properties.get("HDR_OUTPUT_METADATA").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::Unsupported,
            "HDR10 capability probe succeeded without HDR_OUTPUT_METADATA",
        )
    })?;
    let colorspace_property = properties.get("Colorspace").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::Unsupported,
            "HDR10 capability probe succeeded without Colorspace",
        )
    })?;
    let colorspace_value = colorspace_property
        .enum_value(COLORSPACE_BT2020_RGB)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Unsupported,
                "HDR10 capability probe succeeded without BT2020_RGB enum value",
            )
        })?;
    let max_bpc_property = properties.get("max bpc").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::Unsupported,
            "HDR10 capability probe succeeded without max bpc",
        )
    })?;
    let max_bpc_value = bpc_target(max_bpc_property.range(), 10).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "max bpc property has an invalid range",
        )
    })?;

    // Allocate before mutating connector state so allocation failure needs no
    // rollback.
    let blob = device.create_property_blob(metadata)?;
    let result = (|| -> io::Result<()> {
        device.set_property(connector, max_bpc_property.handle, max_bpc_value)?;
        device.set_property(connector, colorspace_property.handle, colorspace_value)?;
        device.set_property(connector, metadata_property.handle, blob.into())?;
        Ok(())
    })();

    if let Err(error) = result {
        snapshot.restore(device, connector);
        let _ = device.destroy_property_blob(blob.into());
        return Err(error);
    }

    Ok(Some(blob))
}

/// Return a connector to SDR and release compositor-owned blob handles.
///
/// The SDR metadata blob's creation reference is destroyed after the property
/// is installed; the connector property retains its own kernel reference. This
/// prevents one leaked blob per HDR disable operation.
pub fn clear_hdr<D>(
    device: &D,
    connector: drm::control::connector::Handle,
    properties: &PropertyIndex,
    previous_blob: Option<property::Value<'static>>,
) -> io::Result<()>
where
    D: ControlDevice,
{
    let snapshot = PropertySnapshot::capture(properties);
    let sdr_blob = if properties.has("HDR_OUTPUT_METADATA") {
        Some(device.create_property_blob(&HdrOutputMetadata::sdr())?)
    } else {
        None
    };

    let result = (|| -> io::Result<()> {
        if let Some(metadata_property) = properties.get("HDR_OUTPUT_METADATA") {
            let blob = sdr_blob.expect("created when metadata property exists");
            device.set_property(connector, metadata_property.handle, blob.into())?;
        }
        if let Some(colorspace_property) = properties.get("Colorspace") {
            if let Some(value) = colorspace_property.enum_value(COLORSPACE_DEFAULT) {
                device.set_property(connector, colorspace_property.handle, value)?;
            }
        }
        if let Some(max_bpc_property) = properties.get("max bpc") {
            if let Some(value) = bpc_target(max_bpc_property.range(), 8) {
                device.set_property(connector, max_bpc_property.handle, value)?;
            }
        }
        Ok(())
    })();

    if let Err(error) = result {
        snapshot.restore(device, connector);
        if let Some(blob) = sdr_blob {
            let _ = device.destroy_property_blob(blob.into());
        }
        return Err(error);
    }

    if let Some(old_blob) = previous_blob {
        let _ = device.destroy_property_blob(old_blob.into());
    }
    if let Some(blob) = sdr_blob {
        let _ = device.destroy_property_blob(blob.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hdr_output_metadata_matches_kernel_layout() {
        assert_eq!(core::mem::size_of::<HdrMetadataInfoframe>(), 26);
        assert_eq!(core::mem::align_of::<HdrOutputMetadata>(), 4);
        assert_eq!(core::mem::size_of::<HdrOutputMetadata>(), 32);
    }

    #[test]
    fn chromaticity_encoding_is_bounded_and_nan_safe() {
        let red = ChromaticityPoint::from_xy(0.708, 0.292);
        assert_eq!(red.x, 35_400);
        assert_eq!(red.y, 14_600);
        assert_eq!(
            ChromaticityPoint::from_xy(f32::NAN, 2.0),
            ChromaticityPoint { x: 0, y: 50_000 }
        );
    }

    #[test]
    fn hdr10_uses_pq_bt2020_and_sanitized_luminance() {
        let metadata = HdrOutputMetadata::hdr10(1000, 0.005, 1000, 400);
        assert_eq!(metadata.metadata_type, 0);
        assert_eq!(metadata.hdmi_metadata_type1.eotf, Eotf::St2084 as u8);
        assert_eq!(
            metadata.hdmi_metadata_type1.min_display_mastering_luminance,
            50
        );
        assert_eq!(metadata.hdmi_metadata_type1.max_cll, 1000);
        assert_eq!(metadata.hdmi_metadata_type1.max_fall, 400);

        let invalid = HdrOutputMetadata::hdr10(1000, f32::NAN, 0, 0);
        assert_eq!(
            invalid.hdmi_metadata_type1.min_display_mastering_luminance,
            0
        );
    }

    #[test]
    fn sdr_metadata_uses_traditional_gamma() {
        assert_eq!(
            HdrOutputMetadata::sdr().hdmi_metadata_type1.eotf,
            Eotf::TraditionalSdr as u8
        );
    }

    #[test]
    fn hdr10_capability_requires_every_property_condition() {
        let complete = HdrConnectorCaps {
            has_hdr_metadata: true,
            has_bt2020_colorspace: true,
            max_bpc: Some(12),
            colorspaces: vec![COLORSPACE_BT2020_RGB.to_owned()],
        };
        assert!(complete.hdr10_capable());
        assert!(!HdrConnectorCaps {
            max_bpc: Some(8),
            ..complete.clone()
        }
        .hdr10_capable());
        assert!(!HdrConnectorCaps {
            has_hdr_metadata: false,
            ..complete.clone()
        }
        .hdr10_capable());
        assert!(!HdrConnectorCaps {
            has_bt2020_colorspace: false,
            ..complete
        }
        .hdr10_capable());
    }

    #[test]
    fn bpc_target_clamps_inside_kernel_range() {
        assert_eq!(bpc_target(Some((6, 12)), 10), Some(10));
        assert_eq!(bpc_target(Some((12, 16)), 10), Some(12));
        assert_eq!(bpc_target(Some((6, 8)), 10), Some(8));
        assert_eq!(bpc_target(Some((12, 6)), 10), None);
        assert_eq!(bpc_target(None, 10), None);
    }

    #[test]
    fn vrr_policy_requires_control_and_capability_for_enable() {
        let unsupported = VrrState::default();
        assert!(!vrr_request_allowed(unsupported, true));
        assert!(!vrr_request_allowed(unsupported, false));

        let controllable = VrrState {
            capable: false,
            controllable: true,
            enabled: false,
        };
        assert!(!vrr_request_allowed(controllable, true));
        assert!(vrr_request_allowed(controllable, false));

        let capable = VrrState {
            capable: true,
            controllable: true,
            enabled: false,
        };
        assert!(vrr_request_allowed(capable, true));
    }

    #[test]
    fn absent_properties_never_report_hdr_or_vrr() {
        let empty = PropertyIndex::default();
        let vrr = probe_vrr(&empty, &empty);
        assert!(!vrr.capable);
        assert!(!vrr.controllable);
        assert!(!vrr.enabled);

        let hdr = probe_hdr(&empty);
        assert!(!hdr.hdr10_capable());
        assert!(hdr.colorspaces.is_empty());
        assert!(hdr.summary().contains("hdr10_capable=false"));
    }
}
