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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
