use crate::foundation::geometry::{Rect, Size};

use super::linear::LayoutError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    Start,
    Center,
    End,
}

pub fn align(
    container: Rect,
    child: Size,
    horizontal: Alignment,
    vertical: Alignment,
) -> Result<Rect, LayoutError> {
    Ok(Rect {
        x: aligned_origin(container.x, container.width, child.width, horizontal)?,
        y: aligned_origin(container.y, container.height, child.height, vertical)?,
        width: child.width,
        height: child.height,
    })
}

fn aligned_origin(
    origin: i32,
    container_extent: u32,
    child_extent: u32,
    alignment: Alignment,
) -> Result<i32, LayoutError> {
    let free_space = i64::from(container_extent) - i64::from(child_extent);
    let offset = match alignment {
        Alignment::Start => 0,
        Alignment::Center => free_space / 2,
        Alignment::End => free_space,
    };

    i32::try_from(i64::from(origin) + offset).map_err(|_| LayoutError::CoordinateOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_aligns_left_and_top_without_resizing() {
        assert_eq!(
            align(
                Rect {
                    x: -4,
                    y: 6,
                    width: 12,
                    height: 9,
                },
                Size {
                    width: 3,
                    height: 2,
                },
                Alignment::Start,
                Alignment::Start,
            ),
            Ok(Rect {
                x: -4,
                y: 6,
                width: 3,
                height: 2,
            })
        );
    }

    #[test]
    fn center_truncates_odd_signed_free_space_toward_zero() {
        assert_eq!(
            align(
                Rect {
                    x: -10,
                    y: 20,
                    width: 10,
                    height: 3,
                },
                Size {
                    width: 3,
                    height: 8,
                },
                Alignment::Center,
                Alignment::Center,
            ),
            Ok(Rect {
                x: -7,
                y: 18,
                width: 3,
                height: 8,
            })
        );
    }

    #[test]
    fn end_alignment_preserves_an_oversized_child() {
        assert_eq!(
            align(
                Rect {
                    x: 10,
                    y: 20,
                    width: 5,
                    height: 4,
                },
                Size {
                    width: 8,
                    height: 7,
                },
                Alignment::End,
                Alignment::End,
            ),
            Ok(Rect {
                x: 7,
                y: 17,
                width: 8,
                height: 7,
            })
        );
    }

    #[test]
    fn coordinate_overflow_is_reported_on_either_axis() {
        assert_eq!(
            align(
                Rect {
                    x: i32::MAX,
                    y: 0,
                    width: u32::MAX,
                    height: 0,
                },
                Size {
                    width: 0,
                    height: 0,
                },
                Alignment::End,
                Alignment::Start,
            ),
            Err(LayoutError::CoordinateOverflow)
        );
        assert_eq!(
            align(
                Rect {
                    x: 0,
                    y: i32::MAX,
                    width: 0,
                    height: u32::MAX,
                },
                Size {
                    width: 0,
                    height: 0,
                },
                Alignment::Start,
                Alignment::End,
            ),
            Err(LayoutError::CoordinateOverflow)
        );
    }
}
