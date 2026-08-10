//! Shell-side HiDPI / content scale (mirrors compositor SLOPOS_OUTPUT_SCALE).
//!
//! Pure helpers so tests and layout can share the same scale policy without
//! depending on the compositor crate.

/// Rational scale factor (reduced).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShellScale {
    pub numerator: u32,
    pub denominator: u32,
}

impl ShellScale {
    pub const IDENTITY: Self = Self {
        numerator: 1,
        denominator: 1,
    };

    pub fn new(n: u32, d: u32) -> Option<Self> {
        if n == 0 || d == 0 {
            return None;
        }
        let g = gcd(n, d);
        Some(Self {
            numerator: n / g,
            denominator: d / g,
        })
    }

    pub fn as_f64(self) -> f64 {
        f64::from(self.numerator) / f64::from(self.denominator)
    }

    pub fn is_identity(self) -> bool {
        self.numerator == self.denominator
    }

    /// Return the integer buffer scale supported by the current Wayland path.
    ///
    /// Fractional output scale needs the fractional-scale and viewporter
    /// protocols, which are not advertised yet. Keep the shell's effective
    /// scale aligned with the compositor's integer output scale rather than
    /// applying a second fractional transform to an already-scaled surface.
    pub fn integer_buffer_scale(self) -> u32 {
        self.as_f64().round().max(1.0).min(u32::MAX as f64) as u32
    }

    pub fn quantized_integer(self) -> Self {
        Self::new(self.integer_buffer_scale(), 1).unwrap_or(Self::IDENTITY)
    }
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a.max(1)
}

/// Parse `"2"`, `"1.5"`, `"3/2"` style scale strings.
pub fn parse_shell_scale(raw: &str) -> Option<ShellScale> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if let Some((a, b)) = raw.split_once('/') {
        let n: u32 = a.trim().parse().ok()?;
        let d: u32 = b.trim().parse().ok()?;
        return ShellScale::new(n, d);
    }
    if let Ok(n) = raw.parse::<u32>() {
        return ShellScale::new(n, 1);
    }
    // Fixed-point decimal ×1000
    let f: f64 = raw.parse().ok()?;
    if !(0.25..=64.0).contains(&f) {
        return None;
    }
    let num = (f * 1000.0).round() as u32;
    ShellScale::new(num, 1000)
}

/// Read `SLOPOS_OUTPUT_SCALE` or `SLOPOS_SHELL_SCALE`.
pub fn detect_shell_scale_from_env() -> ShellScale {
    let v = std::env::var("SLOPOS_SHELL_SCALE")
        .ok()
        .or_else(|| std::env::var("SLOPOS_OUTPUT_SCALE").ok());
    v.as_deref()
        .and_then(parse_shell_scale)
        .map(ShellScale::quantized_integer)
        .unwrap_or(ShellScale::IDENTITY)
}

/// Scale a layout dimension (ceil so chrome never shrinks under fractional scale).
pub fn scale_layout_dim(logical: f64, scale: ShellScale) -> f64 {
    if scale.is_identity() {
        return logical;
    }
    (logical * scale.as_f64()).ceil()
}

/// Menu bar / dock heights under scale (logical chrome still exclusive-zone sized).
pub fn scaled_chrome_insets(scale: ShellScale, menu_h: f64, dock_h: f64) -> (f64, f64) {
    (
        scale_layout_dim(menu_h, scale).max(menu_h),
        scale_layout_dim(dock_h, scale).max(dock_h),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_integer_fraction_decimal() {
        assert_eq!(
            parse_shell_scale("2"),
            Some(ShellScale {
                numerator: 2,
                denominator: 1
            })
        );
        assert_eq!(
            parse_shell_scale("3/2"),
            Some(ShellScale {
                numerator: 3,
                denominator: 2
            })
        );
        let s = parse_shell_scale("1.5").unwrap();
        assert!((s.as_f64() - 1.5).abs() < 0.001);
    }

    #[test]
    fn scale_layout_ceil() {
        let s = ShellScale::new(3, 2).unwrap();
        assert_eq!(scale_layout_dim(24.0, s), 36.0);
        assert_eq!(scale_layout_dim(10.0, ShellScale::IDENTITY), 10.0);
    }

    #[test]
    fn chrome_insets_never_below_logical() {
        let (m, d) = scaled_chrome_insets(ShellScale::new(2, 1).unwrap(), 24.0, 64.0);
        assert_eq!(m, 48.0);
        assert_eq!(d, 128.0);
    }

    #[test]
    fn fractional_scale_quantizes_until_fractional_protocol_exists() {
        assert_eq!(ShellScale::new(5, 4).unwrap().integer_buffer_scale(), 1);
        assert_eq!(ShellScale::new(3, 2).unwrap().integer_buffer_scale(), 2);
        assert_eq!(
            ShellScale::new(3, 2).unwrap().quantized_integer().as_f64(),
            2.0
        );
    }
}
