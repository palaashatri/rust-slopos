use std::num::NonZeroU32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scale(NonZeroU32);

impl Scale {
    pub fn new(factor: u32) -> Option<Self> {
        NonZeroU32::new(factor).map(Self)
    }

    pub fn factor(self) -> u32 {
        self.0.get()
    }
}

/// A device scale with a fraction, such as 5/4 for 1.25 or 3/2 for 1.5.
///
/// A logical coordinate maps to a device coordinate by rounding half up,
/// each edge on its own. Neighbouring rectangles therefore still meet without
/// gaps or overlaps. For an integer scale the result equals
/// [`Rect::to_device`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FractionalScale {
    numerator: NonZeroU32,
    denominator: NonZeroU32,
}

impl FractionalScale {
    /// Largest accepted numerator or denominator. It keeps every product in
    /// 64 bits.
    pub const MAX_TERM: u32 = 1 << 16;

    /// The scale `numerator / denominator`, or `None` when a term is zero or
    /// above [`Self::MAX_TERM`].
    pub fn new(numerator: u32, denominator: u32) -> Option<Self> {
        if numerator > Self::MAX_TERM || denominator > Self::MAX_TERM {
            return None;
        }
        Some(Self {
            numerator: NonZeroU32::new(numerator)?,
            denominator: NonZeroU32::new(denominator)?,
        })
    }

    /// The fractional form of an integer [`Scale`], or `None` when its factor
    /// is above [`Self::MAX_TERM`].
    pub fn from_scale(scale: Scale) -> Option<Self> {
        Self::new(scale.factor(), 1)
    }

    /// The device coordinate of logical coordinate `value`, rounded half up
    /// and clamped to the `i32` range.
    pub fn device_coordinate(self, value: i32) -> i32 {
        let numerator = i64::from(self.numerator.get());
        let denominator = i64::from(self.denominator.get());
        // round(value * n / d) = floor((2 * value * n + d) / (2 * d)).
        let scaled = (2 * i64::from(value) * numerator + denominator).div_euclid(2 * denominator);
        i32::try_from(scaled).unwrap_or(if scaled < 0 { i32::MIN } else { i32::MAX })
    }

    /// The device rectangle of a logical one. `None` when the far edge or a
    /// size does not fit in `i32` (malformed input), rather than wrapping.
    pub fn device_rect(self, rect: Rect) -> Option<Rect> {
        let width = i32::try_from(rect.width).ok()?;
        let height = i32::try_from(rect.height).ok()?;
        let far_x = rect.x.checked_add(width)?;
        let far_y = rect.y.checked_add(height)?;
        let left = self.device_coordinate(rect.x);
        let top = self.device_coordinate(rect.y);
        let right = self.device_coordinate(far_x);
        let bottom = self.device_coordinate(far_y);
        Some(Rect {
            x: left,
            y: top,
            width: u32::try_from(i64::from(right) - i64::from(left)).ok()?,
            height: u32::try_from(i64::from(bottom) - i64::from(top)).ok()?,
        })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub fn contains(self, point_x: i32, point_y: i32) -> bool {
        let left = i64::from(self.x);
        let top = i64::from(self.y);
        let point_x = i64::from(point_x);
        let point_y = i64::from(point_y);
        point_x >= left
            && point_y >= top
            && point_x < left + i64::from(self.width)
            && point_y < top + i64::from(self.height)
    }

    pub fn to_device(self, scale: Scale) -> Option<Self> {
        let factor = scale.factor();
        Some(Self {
            x: i32::try_from(i64::from(self.x) * i64::from(factor)).ok()?,
            y: i32::try_from(i64::from(self.y) * i64::from(factor)).ok()?,
            width: self.width.checked_mul(factor)?,
            height: self.height.checked_mul(factor)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_testing_is_half_open_and_handles_negative_origins() {
        let rect = Rect {
            x: -3,
            y: -2,
            width: 8,
            height: 4,
        };
        assert!(rect.contains(-3, -2));
        assert!(rect.contains(4, 1));
        assert!(!rect.contains(5, 1));
        assert!(!rect.contains(4, 2));
        assert!(!Rect { width: 0, ..rect }.contains(-3, -2));
    }

    #[test]
    fn integer_scale_preserves_exact_geometry() {
        let rect = Rect {
            x: -3,
            y: 2,
            width: 80,
            height: 20,
        };
        assert!(Scale::new(0).is_none());
        assert_eq!(rect.to_device(Scale::new(1).unwrap()), Some(rect));
        assert_eq!(
            rect.to_device(Scale::new(2).unwrap()),
            Some(Rect {
                x: -6,
                y: 4,
                width: 160,
                height: 40
            })
        );
    }

    #[test]
    fn overflow_is_rejected_without_wrapping() {
        let rect = Rect {
            x: i32::MAX,
            y: i32::MIN,
            width: u32::MAX,
            height: 1,
        };
        assert!(rect.contains(i32::MAX, i32::MIN));
        assert_eq!(rect.to_device(Scale::new(2).unwrap()), None);
    }

    #[test]
    fn y_overflow_is_rejected_independently() {
        for origin_y in [i32::MIN, i32::MAX] {
            let rect = Rect {
                x: 0,
                y: origin_y,
                width: 1,
                height: 1,
            };
            assert_eq!(rect.to_device(Scale::new(2).unwrap()), None);
        }
    }

    #[test]
    fn width_overflow_is_rejected_independently() {
        let rect = Rect {
            x: 0,
            y: 0,
            width: u32::MAX,
            height: 1,
        };
        assert_eq!(rect.to_device(Scale::new(2).unwrap()), None);
    }

    #[test]
    fn height_overflow_is_rejected_independently() {
        let rect = Rect {
            x: 0,
            y: 0,
            width: 1,
            height: u32::MAX,
        };
        assert_eq!(rect.to_device(Scale::new(2).unwrap()), None);
    }
}

#[cfg(test)]
mod fractional_tests {
    use super::*;

    fn rect(x: i32, y: i32, width: u32, height: u32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn integer_scales_agree_with_to_device() {
        for factor in [1, 2, 3] {
            let scale = Scale::new(factor).unwrap();
            let fractional = FractionalScale::from_scale(scale).unwrap();
            for logical in [rect(0, 0, 1, 1), rect(5, 7, 10, 3), rect(-4, -9, 6, 2)] {
                assert_eq!(
                    fractional.device_rect(logical),
                    logical.to_device(scale),
                    "factor {factor}"
                );
            }
        }
    }

    #[test]
    fn quarter_scale_rounds_each_edge_half_up() {
        // 1.25: 1 -> 1.25, 2 -> 2.5 (up), -2 -> -2.5 (up to -2).
        let scale = FractionalScale::new(5, 4).unwrap();
        assert_eq!(scale.device_coordinate(1), 1);
        assert_eq!(scale.device_coordinate(2), 3);
        assert_eq!(scale.device_coordinate(-2), -2);
        // Width 3 spans 0..3.75, which rounds to 0..4.
        assert_eq!(scale.device_rect(rect(0, 0, 3, 3)), Some(rect(0, 0, 4, 4)));
    }

    #[test]
    fn adjacent_rects_tile_without_gaps_or_overlaps() {
        for (numerator, denominator) in [(5, 4), (3, 2), (7, 3), (9, 5)] {
            let scale = FractionalScale::new(numerator, denominator).unwrap();
            let mut edge = scale.device_coordinate(0);
            for column in 0..50 {
                let device = scale.device_rect(rect(column * 3, 0, 3, 1)).unwrap();
                assert_eq!(
                    device.x, edge,
                    "scale {numerator}/{denominator}, column {column}"
                );
                edge = device.x + device.width as i32;
            }
        }
    }

    #[test]
    fn half_scale_places_a_logical_pixel_on_rounded_device_edges() {
        let scale = FractionalScale::new(3, 2).unwrap();
        // Logical pixel 1 covers device 1.5..3.0, which rounds to 2..3.
        assert_eq!(scale.device_rect(rect(1, 1, 1, 1)), Some(rect(2, 2, 1, 1)));
        // Negative origins: -4.5 rounds to -4 and -1.5 rounds to -1.
        assert_eq!(
            scale.device_rect(rect(-3, 0, 2, 1)),
            Some(rect(-4, 0, 3, 2))
        );
        // An empty rectangle stays empty.
        assert_eq!(scale.device_rect(rect(4, 4, 0, 0)), Some(rect(6, 6, 0, 0)));
    }

    #[test]
    fn malformed_scales_and_rects_are_refused_not_wrapped() {
        assert!(FractionalScale::new(0, 1).is_none());
        assert!(FractionalScale::new(1, 0).is_none());
        assert!(FractionalScale::new(FractionalScale::MAX_TERM + 1, 1).is_none());
        assert!(
            FractionalScale::from_scale(Scale::new(FractionalScale::MAX_TERM + 1).unwrap())
                .is_none()
        );

        let scale = FractionalScale::new(5, 4).unwrap();
        assert_eq!(
            scale.device_rect(rect(i32::MAX, 0, 1, 1)),
            None,
            "far edge past i32"
        );
        assert_eq!(
            scale.device_rect(rect(0, 0, u32::MAX, 1)),
            None,
            "size past i32"
        );
        // Representable extremes clamp rather than wrap.
        let huge = FractionalScale::new(FractionalScale::MAX_TERM, 1).unwrap();
        assert_eq!(huge.device_coordinate(i32::MAX), i32::MAX);
        assert_eq!(huge.device_coordinate(i32::MIN), i32::MIN);
    }
}
