//! HDR and color-space management for the compositor.

/// Supported color spaces for output and surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorSpace {
    /// Standard sRGB (SDR, 8-bit per channel).
    SRgb,
    /// Rec. 2020 (wide color gamut, normally 10-bit for HDR10 output).
    Rec2020,
    /// scRGB (linear floating-point working/output space).
    ScRgb,
}

impl ColorSpace {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SRgb => "srgb",
            Self::Rec2020 => "rec2020",
            Self::ScRgb => "scrgb",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(value: &str) -> Option<Self> {
        Self::from_str_flexible(value)
    }

    /// Case-insensitive parse; accepts aliases used in settings and environment
    /// variables.
    pub fn from_str_flexible(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "srgb" | "s-rgb" | "srgb8" => Some(Self::SRgb),
            "rec2020" | "bt2020" | "bt.2020" => Some(Self::Rec2020),
            "scrgb" | "sc-rgb" | "linear" => Some(Self::ScRgb),
            _ => None,
        }
    }

    pub const fn is_hdr_encoding(self) -> bool {
        !matches!(self, Self::SRgb)
    }
}

/// Why a requested output mode could not be applied exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HdrFallbackReason {
    /// The requested policy was applied exactly.
    None,
    /// HDR was requested but the active output path has no verified HDR support.
    HdrUnsupported,
    /// HDR exists, but the specifically requested output color space does not.
    RequestedColorSpaceUnsupported,
    /// HDR was disabled, so the compositor intentionally returned to sRGB.
    SdrPolicyForcesSrgb,
    /// The output claimed HDR support but exposed no usable HDR color encoding.
    NoUsableHdrColorSpace,
}

/// Deterministic result of resolving one user/display HDR request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HdrRequestOutcome {
    pub hdr_requested: bool,
    pub requested_color_space: ColorSpace,
    pub applied_color_space: ColorSpace,
    pub hdr_active: bool,
    pub exact_match: bool,
    pub fallback_reason: HdrFallbackReason,
}

/// HDR capability detection and negotiation.
#[derive(Debug, Clone)]
pub struct HdrCapabilities {
    /// Whether the complete output path has verified HDR support.
    pub hdr_supported: bool,
    /// Color spaces verified for the current output path.
    pub supported_color_spaces: Vec<ColorSpace>,
    /// Current compositor output color space.
    pub current_color_space: ColorSpace,
}

impl Default for HdrCapabilities {
    fn default() -> Self {
        Self {
            hdr_supported: false,
            supported_color_spaces: vec![ColorSpace::SRgb],
            current_color_space: ColorSpace::SRgb,
        }
    }
}

impl HdrCapabilities {
    /// Detect capabilities for a non-DRM or otherwise unprobed path.
    ///
    /// Nested X11, Xvfb and software GL cannot honestly prove an HDR scanout
    /// path, so the conservative baseline is SDR-only. The DRM backend replaces
    /// this baseline with connector/property probe results.
    pub fn detect() -> Self {
        Self::default()
    }

    /// Build a sanitized capability set from hardware/backend probe results.
    pub fn from_hardware(
        hdr_supported: bool,
        supported_color_spaces: impl IntoIterator<Item = ColorSpace>,
    ) -> Self {
        let mut capabilities = Self::default();
        capabilities.update_from_hardware(hdr_supported, supported_color_spaces);
        capabilities
    }

    /// Replace output capabilities without inventing unsupported color spaces.
    ///
    /// sRGB is always retained as the safe fallback. Duplicate entries are
    /// removed while preserving probe order. When HDR is not supported, HDR
    /// encodings are discarded rather than left available for accidental use.
    pub fn update_from_hardware(
        &mut self,
        hdr_supported: bool,
        supported_color_spaces: impl IntoIterator<Item = ColorSpace>,
    ) {
        let mut sanitized = vec![ColorSpace::SRgb];
        if hdr_supported {
            for color_space in supported_color_spaces {
                if !sanitized.contains(&color_space) {
                    sanitized.push(color_space);
                }
            }
        }

        self.hdr_supported = hdr_supported;
        self.supported_color_spaces = sanitized;
        if !self
            .supported_color_spaces
            .contains(&self.current_color_space)
        {
            self.current_color_space = ColorSpace::SRgb;
        }
    }

    /// Set the output color space only when the backend verified it.
    pub fn set_color_space(&mut self, color_space: ColorSpace) -> bool {
        if self.supported_color_spaces.contains(&color_space) {
            self.current_color_space = color_space;
            true
        } else {
            false
        }
    }

    /// Resolve and apply a client/user policy request.
    ///
    /// This function never mutates `supported_color_spaces`: capabilities come
    /// from the output probe, not from user intent. That prevents an HDR toggle
    /// from fabricating scRGB or Rec.2020 support on hardware that did not
    /// advertise it.
    pub fn negotiate_request(
        &mut self,
        hdr_requested: bool,
        requested_color_space: ColorSpace,
    ) -> HdrRequestOutcome {
        let (applied_color_space, fallback_reason) = if !hdr_requested {
            (
                ColorSpace::SRgb,
                if requested_color_space == ColorSpace::SRgb {
                    HdrFallbackReason::None
                } else {
                    HdrFallbackReason::SdrPolicyForcesSrgb
                },
            )
        } else if !self.hdr_supported {
            (ColorSpace::SRgb, HdrFallbackReason::HdrUnsupported)
        } else if requested_color_space.is_hdr_encoding()
            && self.supported_color_spaces.contains(&requested_color_space)
        {
            (requested_color_space, HdrFallbackReason::None)
        } else if self.supported_color_spaces.contains(&ColorSpace::Rec2020) {
            (
                ColorSpace::Rec2020,
                HdrFallbackReason::RequestedColorSpaceUnsupported,
            )
        } else if self.supported_color_spaces.contains(&ColorSpace::ScRgb) {
            (
                ColorSpace::ScRgb,
                HdrFallbackReason::RequestedColorSpaceUnsupported,
            )
        } else {
            (ColorSpace::SRgb, HdrFallbackReason::NoUsableHdrColorSpace)
        };

        // sRGB is guaranteed by capability sanitization and is also present in
        // the conservative default. Keep a defensive fallback for callers that
        // constructed the public fields manually.
        if !self.set_color_space(applied_color_space) {
            self.current_color_space = ColorSpace::SRgb;
        }

        let exact_match = fallback_reason == HdrFallbackReason::None
            && self.current_color_space == requested_color_space;
        HdrRequestOutcome {
            hdr_requested,
            requested_color_space,
            applied_color_space: self.current_color_space,
            hdr_active: hdr_requested
                && self.hdr_supported
                && self.current_color_space.is_hdr_encoding(),
            exact_match,
            fallback_reason,
        }
    }

    /// Compatibility wrapper used by existing backend code.
    ///
    /// Returns true only when the requested policy and color space were applied
    /// exactly. Callers that need diagnostics should use [`Self::negotiate_request`].
    pub fn apply_request(&mut self, hdr_requested: bool, color_space: ColorSpace) -> bool {
        self.negotiate_request(hdr_requested, color_space)
            .exact_match
    }
}

/// Per-surface color-space tracking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceColorSpace {
    /// Client-declared color space (normally sRGB for compatibility).
    pub client_color_space: ColorSpace,
    /// Output color space after compositor conversion/tone mapping.
    pub output_color_space: ColorSpace,
}

impl Default for SurfaceColorSpace {
    fn default() -> Self {
        Self {
            client_color_space: ColorSpace::SRgb,
            output_color_space: ColorSpace::SRgb,
        }
    }
}

/// Tone-mapping mode for SDR-to-HDR working-space conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToneMapperMode {
    /// Global Reinhard curve.
    Reinhard,
    /// Common fitted ACES filmic approximation.
    Aces,
    /// Clamp and pass through the normalized source value.
    None,
}

/// Scalar tone mapper used by compositor color-path tests and policy code.
///
/// Values returned by [`Self::tone_map`] are normalized to `0.0..=1.0`; the
/// backend converts that normalized result into the target output encoding.
#[derive(Debug, Clone, Copy)]
pub struct ToneMapper {
    mode: ToneMapperMode,
    hdr_peak_nits: f32,
}

impl Default for ToneMapper {
    fn default() -> Self {
        Self {
            mode: ToneMapperMode::Reinhard,
            hdr_peak_nits: 1000.0,
        }
    }
}

impl ToneMapper {
    const DEFAULT_PEAK_NITS: f32 = 1000.0;
    const MIN_PEAK_NITS: f32 = 80.0;
    const MAX_PEAK_NITS: f32 = 10_000.0;

    pub fn new(mode: ToneMapperMode, hdr_peak_nits: f32) -> Self {
        let hdr_peak_nits = if hdr_peak_nits.is_finite() {
            hdr_peak_nits.clamp(Self::MIN_PEAK_NITS, Self::MAX_PEAK_NITS)
        } else {
            Self::DEFAULT_PEAK_NITS
        };
        Self {
            mode,
            hdr_peak_nits,
        }
    }

    pub const fn mode(self) -> ToneMapperMode {
        self.mode
    }

    pub const fn peak_nits(self) -> f32 {
        self.hdr_peak_nits
    }

    /// Tone-map one normalized SDR component.
    ///
    /// Invalid and out-of-range input is sanitized before curve evaluation so a
    /// NaN cannot poison an entire composited frame or GPU uniform buffer.
    pub fn tone_map(self, sdr_value: f32) -> f32 {
        let source = if sdr_value.is_finite() {
            sdr_value.clamp(0.0, 1.0)
        } else {
            0.0
        };

        match self.mode {
            ToneMapperMode::Reinhard => {
                // Scale relative to 203-nit reference white, then normalize the
                // global Reinhard curve. The result remains finite and <= 1.
                let exposure = self.hdr_peak_nits / 203.0;
                let mapped = source * exposure;
                mapped / (1.0 + mapped)
            }
            ToneMapperMode::Aces => {
                // Narkowicz fitted ACES curve, evaluated in normalized linear
                // space and clamped for numerical safety.
                let numerator = source * (2.51 * source + 0.03);
                let denominator = source * (2.43 * source + 0.59) + 0.14;
                (numerator / denominator).clamp(0.0, 1.0)
            }
            ToneMapperMode::None => source,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_space_serialization_and_parsing_are_stable() {
        assert_eq!(ColorSpace::SRgb.as_str(), "srgb");
        assert_eq!(ColorSpace::Rec2020.as_str(), "rec2020");
        assert_eq!(ColorSpace::ScRgb.as_str(), "scrgb");
        assert_eq!(ColorSpace::from_str("s-rgb"), Some(ColorSpace::SRgb));
        assert_eq!(ColorSpace::from_str("BT.2020"), Some(ColorSpace::Rec2020));
        assert_eq!(ColorSpace::from_str("linear"), Some(ColorSpace::ScRgb));
        assert_eq!(ColorSpace::from_str("invalid"), None);
    }

    #[test]
    fn conservative_detection_is_sdr_only() {
        let capabilities = HdrCapabilities::detect();
        assert!(!capabilities.hdr_supported);
        assert_eq!(capabilities.supported_color_spaces, vec![ColorSpace::SRgb]);
        assert_eq!(capabilities.current_color_space, ColorSpace::SRgb);
    }

    #[test]
    fn hardware_capabilities_are_deduplicated_without_invention() {
        let capabilities =
            HdrCapabilities::from_hardware(true, [ColorSpace::Rec2020, ColorSpace::Rec2020]);
        assert_eq!(
            capabilities.supported_color_spaces,
            vec![ColorSpace::SRgb, ColorSpace::Rec2020]
        );
        assert!(!capabilities
            .supported_color_spaces
            .contains(&ColorSpace::ScRgb));
    }

    #[test]
    fn disabling_hdr_forces_safe_srgb() {
        let mut capabilities =
            HdrCapabilities::from_hardware(true, [ColorSpace::Rec2020, ColorSpace::ScRgb]);
        capabilities.set_color_space(ColorSpace::Rec2020);

        let outcome = capabilities.negotiate_request(false, ColorSpace::Rec2020);
        assert_eq!(outcome.applied_color_space, ColorSpace::SRgb);
        assert_eq!(
            outcome.fallback_reason,
            HdrFallbackReason::SdrPolicyForcesSrgb
        );
        assert!(!outcome.hdr_active);
        assert!(!outcome.exact_match);
    }

    #[test]
    fn unsupported_hdr_request_does_not_fabricate_capabilities() {
        let mut capabilities = HdrCapabilities::default();
        let outcome = capabilities.negotiate_request(true, ColorSpace::Rec2020);
        assert_eq!(outcome.applied_color_space, ColorSpace::SRgb);
        assert_eq!(outcome.fallback_reason, HdrFallbackReason::HdrUnsupported);
        assert!(!outcome.hdr_active);
        assert_eq!(capabilities.supported_color_spaces, vec![ColorSpace::SRgb]);
    }

    #[test]
    fn unavailable_hdr_encoding_uses_a_verified_fallback() {
        let mut capabilities = HdrCapabilities::from_hardware(true, [ColorSpace::Rec2020]);
        let outcome = capabilities.negotiate_request(true, ColorSpace::ScRgb);
        assert_eq!(outcome.applied_color_space, ColorSpace::Rec2020);
        assert_eq!(
            outcome.fallback_reason,
            HdrFallbackReason::RequestedColorSpaceUnsupported
        );
        assert!(outcome.hdr_active);
        assert!(!outcome.exact_match);
    }

    #[test]
    fn capability_loss_returns_current_output_to_srgb() {
        let mut capabilities = HdrCapabilities::from_hardware(true, [ColorSpace::Rec2020]);
        assert!(capabilities.set_color_space(ColorSpace::Rec2020));
        capabilities.update_from_hardware(false, [ColorSpace::Rec2020]);
        assert_eq!(capabilities.current_color_space, ColorSpace::SRgb);
        assert_eq!(capabilities.supported_color_spaces, vec![ColorSpace::SRgb]);
    }

    #[test]
    fn tone_mappers_are_finite_bounded_and_monotonic_for_sdr_input() {
        for mode in [
            ToneMapperMode::Reinhard,
            ToneMapperMode::Aces,
            ToneMapperMode::None,
        ] {
            let mapper = ToneMapper::new(mode, 1000.0);
            let mut previous = 0.0;
            for step in 0..=1000 {
                let value = mapper.tone_map(step as f32 / 1000.0);
                assert!(value.is_finite());
                assert!((0.0..=1.0).contains(&value));
                assert!(
                    value + f32::EPSILON >= previous,
                    "{mode:?} was not monotonic"
                );
                previous = value;
            }
        }
    }

    #[test]
    fn tone_mapper_sanitizes_invalid_inputs_and_peak_luminance() {
        let mapper = ToneMapper::new(ToneMapperMode::Reinhard, f32::NAN);
        assert_eq!(mapper.peak_nits(), 1000.0);
        assert_eq!(mapper.tone_map(f32::NAN), 0.0);
        assert_eq!(mapper.tone_map(-1.0), 0.0);
        assert!(mapper.tone_map(f32::INFINITY).is_finite());

        let passthrough = ToneMapper::new(ToneMapperMode::None, 1.0);
        assert_eq!(passthrough.peak_nits(), 80.0);
        assert_eq!(passthrough.tone_map(2.0), 1.0);
    }
}
