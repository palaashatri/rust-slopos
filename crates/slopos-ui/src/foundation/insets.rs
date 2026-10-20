use super::geometry::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Insets {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

impl Insets {
    pub fn apply(self, rect: Rect) -> Option<Rect> {
        Some(Rect {
            x: i32::try_from(i64::from(rect.x) + i64::from(self.left)).ok()?,
            y: i32::try_from(i64::from(rect.y) + i64::from(self.top)).ok()?,
            width: rect.width.checked_sub(self.left)?.checked_sub(self.right)?,
            height: rect
                .height
                .checked_sub(self.top)?
                .checked_sub(self.bottom)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asymmetric_insets_shift_and_shrink_rect() {
        let rect = Rect {
            x: -5,
            y: 10,
            width: 20,
            height: 15,
        };
        let insets = Insets {
            left: 2,
            top: 3,
            right: 4,
            bottom: 5,
        };
        assert_eq!(
            insets.apply(rect),
            Some(Rect {
                x: -3,
                y: 13,
                width: 14,
                height: 7,
            })
        );
    }

    #[test]
    fn zero_insets_preserve_rect() {
        let rect = Rect {
            x: -7,
            y: 8,
            width: 0,
            height: 9,
        };
        assert_eq!(
            Insets {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            }
            .apply(rect),
            Some(rect)
        );
    }

    #[test]
    fn exact_insets_allow_collapsed_rect() {
        let rect = Rect {
            x: 2,
            y: 3,
            width: 5,
            height: 7,
        };
        assert_eq!(
            Insets {
                left: 2,
                top: 3,
                right: 3,
                bottom: 4,
            }
            .apply(rect),
            Some(Rect {
                x: 4,
                y: 6,
                width: 0,
                height: 0,
            })
        );
    }

    #[test]
    fn oversized_insets_are_rejected() {
        let rect = Rect {
            x: 0,
            y: 0,
            width: 5,
            height: 7,
        };
        for insets in [
            Insets {
                left: 3,
                top: 0,
                right: 3,
                bottom: 0,
            },
            Insets {
                left: 0,
                top: 4,
                right: 0,
                bottom: 4,
            },
            Insets {
                left: u32::MAX,
                top: 0,
                right: 1,
                bottom: 0,
            },
        ] {
            assert_eq!(insets.apply(rect), None);
        }
    }

    #[test]
    fn coordinate_overflow_is_rejected() {
        let rect = Rect {
            x: i32::MAX,
            y: i32::MAX,
            width: 1,
            height: 1,
        };
        for insets in [
            Insets {
                left: 1,
                top: 0,
                right: 0,
                bottom: 0,
            },
            Insets {
                left: 0,
                top: 1,
                right: 0,
                bottom: 0,
            },
        ] {
            assert_eq!(insets.apply(rect), None);
        }
    }
}
