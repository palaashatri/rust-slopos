use crate::foundation::geometry::{Rect, Size};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LayoutError {
    #[error("layout position exceeds the coordinate range")]
    CoordinateOverflow,
}

pub fn row(
    origin_x: i32,
    origin_y: i32,
    children: &[Size],
    gap: u32,
) -> Result<Vec<Rect>, LayoutError> {
    linear_layout(origin_x, origin_y, children, gap, false)
}

pub fn column(
    origin_x: i32,
    origin_y: i32,
    children: &[Size],
    gap: u32,
) -> Result<Vec<Rect>, LayoutError> {
    linear_layout(origin_x, origin_y, children, gap, true)
}

fn linear_layout(
    origin_x: i32,
    origin_y: i32,
    children: &[Size],
    gap: u32,
    vertical: bool,
) -> Result<Vec<Rect>, LayoutError> {
    let mut cursor = i64::from(if vertical { origin_y } else { origin_x });
    let mut rectangles = Vec::with_capacity(children.len());

    for (index, child) in children.iter().enumerate() {
        let position = i32::try_from(cursor).map_err(|_| LayoutError::CoordinateOverflow)?;
        rectangles.push(if vertical {
            Rect {
                x: origin_x,
                y: position,
                width: child.width,
                height: child.height,
            }
        } else {
            Rect {
                x: position,
                y: origin_y,
                width: child.width,
                height: child.height,
            }
        });

        if index + 1 < children.len() {
            let extent = if vertical { child.height } else { child.width };
            cursor = cursor
                .checked_add(i64::from(extent))
                .and_then(|position| position.checked_add(i64::from(gap)))
                .ok_or(LayoutError::CoordinateOverflow)?;
        }
    }

    Ok(rectangles)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_positions_children_from_their_widths_and_explicit_gap() {
        assert_eq!(
            row(
                -4,
                7,
                &[
                    Size {
                        width: 8,
                        height: 3,
                    },
                    Size {
                        width: 2,
                        height: 5,
                    },
                ],
                6,
            ),
            Ok(vec![
                Rect {
                    x: -4,
                    y: 7,
                    width: 8,
                    height: 3,
                },
                Rect {
                    x: 10,
                    y: 7,
                    width: 2,
                    height: 5,
                },
            ])
        );
    }

    #[test]
    fn column_positions_children_from_their_heights_and_explicit_gap() {
        assert_eq!(
            column(
                11,
                -3,
                &[
                    Size {
                        width: 5,
                        height: 4,
                    },
                    Size {
                        width: 2,
                        height: 3,
                    },
                ],
                2,
            ),
            Ok(vec![
                Rect {
                    x: 11,
                    y: -3,
                    width: 5,
                    height: 4,
                },
                Rect {
                    x: 11,
                    y: 3,
                    width: 2,
                    height: 3,
                },
            ])
        );
    }

    #[test]
    fn empty_linear_layout_is_empty() {
        assert_eq!(row(0, 0, &[], 12), Ok(Vec::new()));
        assert_eq!(column(0, 0, &[], 12), Ok(Vec::new()));
    }

    #[test]
    fn next_child_coordinate_overflow_is_reported() {
        assert_eq!(
            row(
                i32::MAX,
                0,
                &[
                    Size {
                        width: 1,
                        height: 1,
                    },
                    Size {
                        width: 1,
                        height: 1,
                    },
                ],
                1,
            ),
            Err(LayoutError::CoordinateOverflow)
        );
    }

    #[test]
    fn coordinate_overflow_message_is_layout_generic() {
        assert_eq!(
            LayoutError::CoordinateOverflow.to_string(),
            "layout position exceeds the coordinate range"
        );
    }
}
