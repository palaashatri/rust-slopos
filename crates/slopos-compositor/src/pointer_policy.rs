// Copyright (c) 2026 Palaash Atri
// SPDX-License-Identifier: MIT

//! Backend-independent pointer constraint motion policy.
//!
//! Smithay owns the Wayland object lifecycle. This module owns the small,
//! deterministic movement decision used by both SLOPOS compositor backends so
//! locked/confined semantics can be tested without a live input device.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerConstraintMotion {
    Free,
    Locked,
    Confined,
}

/// Resolve a proposed two-axis pointer delta.
///
/// `allow_x` and `allow_y` represent both surface-boundary and optional
/// confinement-region checks performed by the backend in surface-local space.
pub fn resolve_pointer_delta(
    mode: PointerConstraintMotion,
    delta: (f64, f64),
    allow_x: bool,
    allow_y: bool,
) -> (f64, f64) {
    match mode {
        PointerConstraintMotion::Free => delta,
        PointerConstraintMotion::Locked => (0.0, 0.0),
        PointerConstraintMotion::Confined => (
            if allow_x { delta.0 } else { 0.0 },
            if allow_y { delta.1 } else { 0.0 },
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_pointer_keeps_both_axes() {
        assert_eq!(
            resolve_pointer_delta(PointerConstraintMotion::Free, (12.5, -4.0), false, false),
            (12.5, -4.0)
        );
    }

    #[test]
    fn locked_pointer_discards_motion() {
        assert_eq!(
            resolve_pointer_delta(PointerConstraintMotion::Locked, (12.5, -4.0), true, true),
            (0.0, 0.0)
        );
    }

    #[test]
    fn confined_pointer_keeps_allowed_axes_independently() {
        assert_eq!(
            resolve_pointer_delta(PointerConstraintMotion::Confined, (12.5, -4.0), false, true),
            (0.0, -4.0)
        );
        assert_eq!(
            resolve_pointer_delta(PointerConstraintMotion::Confined, (12.5, -4.0), true, false),
            (12.5, 0.0)
        );
    }

    #[test]
    fn confined_pointer_stops_when_neither_axis_is_valid() {
        assert_eq!(
            resolve_pointer_delta(
                PointerConstraintMotion::Confined,
                (12.5, -4.0),
                false,
                false
            ),
            (0.0, 0.0)
        );
    }
}
