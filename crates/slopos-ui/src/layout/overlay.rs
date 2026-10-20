use crate::foundation::geometry::{Rect, Size};

pub fn overlay(origin_x: i32, origin_y: i32, children: &[Size]) -> Vec<Rect> {
    children
        .iter()
        .map(|child| Rect {
            x: origin_x,
            y: origin_y,
            width: child.width,
            height: child.height,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_children_at_the_same_origin_in_input_order() {
        assert_eq!(
            overlay(
                -7,
                12,
                &[
                    Size {
                        width: 8,
                        height: 2,
                    },
                    Size {
                        width: 3,
                        height: 6,
                    },
                    Size {
                        width: 4,
                        height: 7,
                    },
                ],
            ),
            vec![
                Rect {
                    x: -7,
                    y: 12,
                    width: 8,
                    height: 2,
                },
                Rect {
                    x: -7,
                    y: 12,
                    width: 3,
                    height: 6,
                },
                Rect {
                    x: -7,
                    y: 12,
                    width: 4,
                    height: 7,
                },
            ]
        );
    }

    #[test]
    fn empty_overlay_is_empty() {
        assert!(overlay(5, -3, &[]).is_empty());
    }
}
