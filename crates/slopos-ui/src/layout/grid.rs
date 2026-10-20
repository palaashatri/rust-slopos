use std::num::NonZeroUsize;

use crate::foundation::geometry::{Rect, Size};

use super::LayoutError;

pub fn grid(
    origin_x: i32,
    origin_y: i32,
    children: &[Size],
    columns: NonZeroUsize,
    horizontal_gap: u32,
    vertical_gap: u32,
) -> Result<Vec<Rect>, LayoutError> {
    if children.is_empty() {
        return Ok(Vec::new());
    }

    let column_count = columns.get();
    let row_count = children.len().div_ceil(column_count);
    let mut column_widths = vec![0; column_count.min(children.len())];
    let mut row_heights = vec![0; row_count];

    for (index, child) in children.iter().enumerate() {
        let column = index % column_count;
        let row = index / column_count;
        column_widths[column] = column_widths[column].max(child.width);
        row_heights[row] = row_heights[row].max(child.height);
    }

    let x_positions = track_positions(origin_x, &column_widths, horizontal_gap)?;
    let y_positions = track_positions(origin_y, &row_heights, vertical_gap)?;

    Ok(children
        .iter()
        .enumerate()
        .map(|(index, child)| Rect {
            x: x_positions[index % column_count],
            y: y_positions[index / column_count],
            width: child.width,
            height: child.height,
        })
        .collect())
}

fn track_positions(origin: i32, extents: &[u32], gap: u32) -> Result<Vec<i32>, LayoutError> {
    let mut position = i64::from(origin);
    let mut positions = Vec::with_capacity(extents.len());

    for (index, extent) in extents.iter().enumerate() {
        positions.push(i32::try_from(position).map_err(|_| LayoutError::CoordinateOverflow)?);
        if index + 1 < extents.len() {
            position = position
                .checked_add(i64::from(*extent))
                .and_then(|position| position.checked_add(i64::from(gap)))
                .ok_or(LayoutError::CoordinateOverflow)?;
        }
    }

    Ok(positions)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size(width: u32, height: u32) -> Size {
        Size { width, height }
    }

    fn columns(count: usize) -> NonZeroUsize {
        NonZeroUsize::new(count).unwrap()
    }

    #[test]
    fn tracks_use_maximum_extents_and_preserve_child_sizes() {
        assert_eq!(
            grid(
                10,
                20,
                &[size(8, 2), size(3, 6), size(4, 7), size(10, 1)],
                columns(2),
                2,
                4,
            ),
            Ok(vec![
                Rect {
                    x: 10,
                    y: 20,
                    width: 8,
                    height: 2,
                },
                Rect {
                    x: 20,
                    y: 20,
                    width: 3,
                    height: 6,
                },
                Rect {
                    x: 10,
                    y: 30,
                    width: 4,
                    height: 7,
                },
                Rect {
                    x: 20,
                    y: 30,
                    width: 10,
                    height: 1,
                },
            ])
        );
    }

    #[test]
    fn negative_origin_is_preserved() {
        assert_eq!(
            grid(
                -8,
                -12,
                &[size(1, 1), size(1, 1), size(1, 1)],
                columns(2),
                1,
                1
            ),
            Ok(vec![
                Rect {
                    x: -8,
                    y: -12,
                    width: 1,
                    height: 1,
                },
                Rect {
                    x: -6,
                    y: -12,
                    width: 1,
                    height: 1,
                },
                Rect {
                    x: -8,
                    y: -10,
                    width: 1,
                    height: 1,
                },
            ])
        );
    }

    #[test]
    fn short_final_row_emits_no_phantom_items() {
        assert_eq!(
            grid(
                0,
                0,
                &[size(1, 2), size(2, 4), size(3, 1), size(6, 5)],
                columns(3),
                1,
                2,
            ),
            Ok(vec![
                Rect {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 2,
                },
                Rect {
                    x: 7,
                    y: 0,
                    width: 2,
                    height: 4,
                },
                Rect {
                    x: 10,
                    y: 0,
                    width: 3,
                    height: 1,
                },
                Rect {
                    x: 0,
                    y: 6,
                    width: 6,
                    height: 5,
                },
            ])
        );
    }

    #[test]
    fn empty_grid_is_empty_even_with_large_gaps() {
        assert_eq!(
            grid(i32::MAX, i32::MIN, &[], columns(3), u32::MAX, u32::MAX),
            Ok(Vec::new())
        );
    }

    #[test]
    fn coordinate_overflow_is_reported_on_each_axis() {
        assert_eq!(
            grid(i32::MAX, 0, &[size(1, 1), size(1, 1)], columns(2), 1, 0),
            Err(LayoutError::CoordinateOverflow)
        );
        assert_eq!(
            grid(0, i32::MAX, &[size(1, 1), size(1, 1)], columns(1), 0, 1),
            Err(LayoutError::CoordinateOverflow)
        );
    }
}
