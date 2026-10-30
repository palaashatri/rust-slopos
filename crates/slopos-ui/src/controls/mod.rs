pub mod button;
pub mod focus;
pub mod text_field;

use crate::foundation::geometry::Rect;

/// AccessKit bounds for a logical rectangle.
pub(crate) fn bounds_of(rect: Rect) -> accesskit::Rect {
    accesskit::Rect {
        x0: f64::from(rect.x),
        y0: f64::from(rect.y),
        x1: f64::from(rect.x) + f64::from(rect.width),
        y1: f64::from(rect.y) + f64::from(rect.height),
    }
}
