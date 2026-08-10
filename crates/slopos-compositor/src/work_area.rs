//! Pure layer-shell exclusive-zone policy shared by nested and DRM backends.
//!
//! The Wayland-facing backends translate Smithay anchor/margin state into the
//! small data type here. Keeping edge selection and saturation independent of
//! protocol objects prevents the two compositor paths from drifting.

use crate::WindowGeometry;

/// One output edge that a layer-shell surface exclusively reserves.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ExclusiveEdge {
    Top,
    Bottom,
    Left,
    Right,
}

/// Protocol-independent snapshot of a layer-shell exclusive-zone request.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ExclusiveZoneReservation {
    pub exclusive_zone: i32,
    pub anchor_top: bool,
    pub anchor_bottom: bool,
    pub anchor_left: bool,
    pub anchor_right: bool,
    pub margin_top: i32,
    pub margin_bottom: i32,
    pub margin_left: i32,
    pub margin_right: i32,
}

impl ExclusiveZoneReservation {
    /// Resolve a valid exclusive edge according to the layer-shell anchor rule.
    ///
    /// A reservation must be anchored to exactly one edge, while it may stretch
    /// across both perpendicular edges. Corner-only surfaces are deliberately
    /// rejected as ambiguous instead of stealing work area from an arbitrary
    /// edge. Non-positive zones never reserve space.
    pub fn edge(self) -> Option<ExclusiveEdge> {
        if self.exclusive_zone <= 0 {
            return None;
        }

        let horizontally_stretched = self.anchor_left && self.anchor_right;
        let vertically_stretched = self.anchor_top && self.anchor_bottom;
        let no_horizontal_anchor = !self.anchor_left && !self.anchor_right;
        let no_vertical_anchor = !self.anchor_top && !self.anchor_bottom;

        if self.anchor_top
            && !self.anchor_bottom
            && (horizontally_stretched || no_horizontal_anchor)
        {
            Some(ExclusiveEdge::Top)
        } else if self.anchor_bottom
            && !self.anchor_top
            && (horizontally_stretched || no_horizontal_anchor)
        {
            Some(ExclusiveEdge::Bottom)
        } else if self.anchor_left
            && !self.anchor_right
            && (vertically_stretched || no_vertical_anchor)
        {
            Some(ExclusiveEdge::Left)
        } else if self.anchor_right
            && !self.anchor_left
            && (vertically_stretched || no_vertical_anchor)
        {
            Some(ExclusiveEdge::Right)
        } else {
            None
        }
    }

    /// Amount removed from the work area for the resolved edge.
    ///
    /// Positive margins on the exclusive edge sit outside the surface and are
    /// part of the unusable region. Negative margins may visually overlap but
    /// must not increase or invert the reservation.
    pub fn reserved_extent(self) -> Option<(ExclusiveEdge, i32)> {
        let edge = self.edge()?;
        let margin = match edge {
            ExclusiveEdge::Top => self.margin_top,
            ExclusiveEdge::Bottom => self.margin_bottom,
            ExclusiveEdge::Left => self.margin_left,
            ExclusiveEdge::Right => self.margin_right,
        }
        .max(0);
        Some((edge, self.exclusive_zone.saturating_add(margin).max(0)))
    }
}

/// Compute the compositor work area after every valid layer-shell reservation.
///
/// Reservations on the same edge accumulate because each exclusive layer is
/// arranged against the remaining usable area. The final rectangle is always
/// at least 1×1, even when clients request more space than the output owns.
pub fn compute_exclusive_work_area(
    output: WindowGeometry,
    reservations: impl IntoIterator<Item = ExclusiveZoneReservation>,
) -> WindowGeometry {
    let output_width = output.width.max(1);
    let output_height = output.height.max(1);
    let mut top = 0_i64;
    let mut bottom = 0_i64;
    let mut left = 0_i64;
    let mut right = 0_i64;

    for reservation in reservations {
        let Some((edge, extent)) = reservation.reserved_extent() else {
            continue;
        };
        let extent = i64::from(extent);
        match edge {
            ExclusiveEdge::Top => top = top.saturating_add(extent),
            ExclusiveEdge::Bottom => bottom = bottom.saturating_add(extent),
            ExclusiveEdge::Left => left = left.saturating_add(extent),
            ExclusiveEdge::Right => right = right.saturating_add(extent),
        }
    }

    let max_horizontal_reservation = i64::from(output_width.saturating_sub(1));
    let max_vertical_reservation = i64::from(output_height.saturating_sub(1));
    left = left.clamp(0, max_horizontal_reservation);
    right = right.clamp(0, max_horizontal_reservation.saturating_sub(left));
    top = top.clamp(0, max_vertical_reservation);
    bottom = bottom.clamp(0, max_vertical_reservation.saturating_sub(top));

    let left_i32 = i32::try_from(left).unwrap_or(i32::MAX);
    let right_i32 = i32::try_from(right).unwrap_or(i32::MAX);
    let top_i32 = i32::try_from(top).unwrap_or(i32::MAX);
    let bottom_i32 = i32::try_from(bottom).unwrap_or(i32::MAX);

    WindowGeometry::new(
        output.x.saturating_add(left_i32),
        output.y.saturating_add(top_i32),
        output_width
            .saturating_sub(left_i32)
            .saturating_sub(right_i32)
            .max(1),
        output_height
            .saturating_sub(top_i32)
            .saturating_sub(bottom_i32)
            .max(1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reservation(edge: ExclusiveEdge, zone: i32, margin: i32) -> ExclusiveZoneReservation {
        let mut reservation = ExclusiveZoneReservation {
            exclusive_zone: zone,
            ..Default::default()
        };
        match edge {
            ExclusiveEdge::Top => {
                reservation.anchor_top = true;
                reservation.anchor_left = true;
                reservation.anchor_right = true;
                reservation.margin_top = margin;
            }
            ExclusiveEdge::Bottom => {
                reservation.anchor_bottom = true;
                reservation.anchor_left = true;
                reservation.anchor_right = true;
                reservation.margin_bottom = margin;
            }
            ExclusiveEdge::Left => {
                reservation.anchor_left = true;
                reservation.anchor_top = true;
                reservation.anchor_bottom = true;
                reservation.margin_left = margin;
            }
            ExclusiveEdge::Right => {
                reservation.anchor_right = true;
                reservation.anchor_top = true;
                reservation.anchor_bottom = true;
                reservation.margin_right = margin;
            }
        }
        reservation
    }

    #[test]
    fn resolves_each_unique_edge_and_rejects_ambiguous_corners() {
        for edge in [
            ExclusiveEdge::Top,
            ExclusiveEdge::Bottom,
            ExclusiveEdge::Left,
            ExclusiveEdge::Right,
        ] {
            assert_eq!(reservation(edge, 24, 0).edge(), Some(edge));
        }

        let corner = ExclusiveZoneReservation {
            exclusive_zone: 20,
            anchor_top: true,
            anchor_right: true,
            ..Default::default()
        };
        assert_eq!(corner.edge(), None);
        assert_eq!(
            ExclusiveZoneReservation {
                exclusive_zone: 0,
                anchor_top: true,
                anchor_left: true,
                anchor_right: true,
                ..Default::default()
            }
            .edge(),
            None
        );
    }

    #[test]
    fn all_four_edges_and_margins_reduce_the_output() {
        let output = WindowGeometry::new(100, 200, 1200, 800);
        let work = compute_exclusive_work_area(
            output,
            [
                reservation(ExclusiveEdge::Top, 24, 2),
                reservation(ExclusiveEdge::Bottom, 64, 4),
                reservation(ExclusiveEdge::Left, 80, 3),
                reservation(ExclusiveEdge::Right, 48, 5),
            ],
        );
        assert_eq!(work, WindowGeometry::new(183, 226, 1064, 706));
    }

    #[test]
    fn same_edge_reservations_accumulate_in_arrangement_order() {
        let output = WindowGeometry::new(0, 0, 1000, 700);
        let work = compute_exclusive_work_area(
            output,
            [
                reservation(ExclusiveEdge::Top, 24, 0),
                reservation(ExclusiveEdge::Top, 32, 1),
            ],
        );
        assert_eq!(work, WindowGeometry::new(0, 57, 1000, 643));
    }

    #[test]
    fn oversized_and_invalid_requests_leave_a_positive_work_area() {
        let output = WindowGeometry::new(-20, 30, 2, 2);
        let work = compute_exclusive_work_area(
            output,
            [
                reservation(ExclusiveEdge::Top, i32::MAX, i32::MAX),
                reservation(ExclusiveEdge::Bottom, i32::MAX, 0),
                reservation(ExclusiveEdge::Left, i32::MAX, 0),
                reservation(ExclusiveEdge::Right, i32::MAX, 0),
                reservation(ExclusiveEdge::Top, -1, 100),
            ],
        );
        assert_eq!(work.width, 1);
        assert_eq!(work.height, 1);
        assert!(work.x >= output.x);
        assert!(work.y >= output.y);
        assert!(work.x.saturating_add(work.width) <= output.x.saturating_add(output.width));
        assert!(work.y.saturating_add(work.height) <= output.y.saturating_add(output.height));
    }
}
