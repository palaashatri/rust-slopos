//! Rectangle and preview maths for the Screenshot app. Plain code with no X11,
//! so every rule here is tested without a display.

use slopos_ui::foundation::geometry::Rect;

/// The rectangle spanned by two corners, in either order.
pub fn from_corners(a: (i32, i32), b: (i32, i32)) -> Rect {
    let x = a.0.min(b.0);
    let y = a.1.min(b.1);
    Rect {
        x,
        y,
        width: (i64::from(a.0.max(b.0)) - i64::from(x)) as u32,
        height: (i64::from(a.1.max(b.1)) - i64::from(y)) as u32,
    }
}

/// Clips `rect` to a screen whose origin is (0, 0). `None` when nothing is left
/// on screen, including for an empty rectangle.
pub fn clamp_to_screen(rect: Rect, screen: (u32, u32)) -> Option<Rect> {
    let (screen_width, screen_height) = (i64::from(screen.0), i64::from(screen.1));
    let left = i64::from(rect.x).clamp(0, screen_width);
    let top = i64::from(rect.y).clamp(0, screen_height);
    let right = (i64::from(rect.x) + i64::from(rect.width)).clamp(0, screen_width);
    let bottom = (i64::from(rect.y) + i64::from(rect.height)).clamp(0, screen_height);
    if right <= left || bottom <= top {
        return None;
    }
    Some(Rect {
        x: left as i32,
        y: top as i32,
        width: (right - left) as u32,
        height: (bottom - top) as u32,
    })
}

/// The largest size with the aspect ratio of `source` that fits in `max`.
/// Never enlarges: a source that already fits is returned unchanged.
pub fn fit_within(source: (u32, u32), max: (u32, u32)) -> (u32, u32) {
    let (width, height) = source;
    if width == 0 || height == 0 {
        return (0, 0);
    }
    if width <= max.0 && height <= max.1 {
        return source;
    }
    let scale = (f64::from(max.0) / f64::from(width)).min(f64::from(max.1) / f64::from(height));
    let fitted_width = (f64::from(width) * scale).floor() as u32;
    let fitted_height = (f64::from(height) * scale).floor() as u32;
    (
        fitted_width.clamp(1, max.0.max(1)),
        fitted_height.clamp(1, max.1.max(1)),
    )
}

/// Maps points on the on-screen preview of a captured screen back to screen
/// pixels. The preview is `shown` pixels, the screen is `screen` pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preview {
    pub screen: (u32, u32),
    pub shown: (u32, u32),
}

impl Preview {
    /// `None` when either size is empty, because nothing can be mapped.
    pub fn new(screen: (u32, u32), shown: (u32, u32)) -> Option<Self> {
        (screen.0 > 0 && screen.1 > 0 && shown.0 > 0 && shown.1 > 0)
            .then_some(Self { screen, shown })
    }

    /// Whether a preview point lies over the preview.
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as u32) < self.shown.0 && (y as u32) < self.shown.1
    }

    /// The screen pixel under a preview point. Points outside the preview are
    /// clamped to its edge first.
    pub fn screen_point(&self, x: i32, y: i32) -> (i32, i32) {
        (
            scale(x, self.shown.0, self.screen.0),
            scale(y, self.shown.1, self.screen.1),
        )
    }

    /// The preview rectangle between two preview points, clamped to the preview.
    pub fn outline(&self, a: (i32, i32), b: (i32, i32)) -> Rect {
        from_corners(self.clamp_point(a), self.clamp_point(b))
    }

    /// The screen rectangle between two preview points. `None` when it is empty.
    pub fn selection(&self, a: (i32, i32), b: (i32, i32)) -> Option<Rect> {
        clamp_to_screen(
            from_corners(self.screen_point(a.0, a.1), self.screen_point(b.0, b.1)),
            self.screen,
        )
    }

    fn clamp_point(&self, point: (i32, i32)) -> (i32, i32) {
        (
            point.0.clamp(0, self.shown.0 as i32),
            point.1.clamp(0, self.shown.1 as i32),
        )
    }
}

/// Converts one preview coordinate to a screen coordinate, rounding to the
/// nearest pixel and clamping to the preview first.
fn scale(value: i32, shown: u32, screen: u32) -> i32 {
    let shown = i64::from(shown);
    let clamped = i64::from(value).clamp(0, shown);
    let scaled = (clamped * i64::from(screen) + shown / 2) / shown;
    scaled.clamp(0, i64::from(screen)) as i32
}

#[cfg(test)]
mod tests {
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
    fn corners_normalise_in_either_order() {
        assert_eq!(from_corners((10, 20), (30, 50)), rect(10, 20, 20, 30));
        assert_eq!(from_corners((30, 50), (10, 20)), rect(10, 20, 20, 30));
        assert_eq!(from_corners((10, 50), (30, 20)), rect(10, 20, 20, 30));
        assert_eq!(from_corners((5, 5), (5, 5)), rect(5, 5, 0, 0));
    }

    #[test]
    fn corners_do_not_overflow_at_the_integer_limits() {
        let wide = from_corners((i32::MIN, i32::MIN), (i32::MAX, i32::MAX));
        assert_eq!(wide, rect(i32::MIN, i32::MIN, u32::MAX, u32::MAX));
    }

    #[test]
    fn clamping_keeps_only_the_visible_part() {
        assert_eq!(
            clamp_to_screen(rect(-10, -10, 30, 30), (100, 100)),
            Some(rect(0, 0, 20, 20))
        );
        assert_eq!(
            clamp_to_screen(rect(90, 90, 30, 30), (100, 100)),
            Some(rect(90, 90, 10, 10))
        );
        assert_eq!(
            clamp_to_screen(rect(10, 10, 20, 20), (100, 100)),
            Some(rect(10, 10, 20, 20))
        );
    }

    #[test]
    fn clamping_rejects_empty_and_off_screen_rectangles() {
        assert_eq!(clamp_to_screen(rect(200, 0, 10, 10), (100, 100)), None);
        assert_eq!(clamp_to_screen(rect(-30, 5, 10, 10), (100, 100)), None);
        assert_eq!(clamp_to_screen(rect(5, 5, 0, 10), (100, 100)), None);
        assert_eq!(clamp_to_screen(rect(0, 0, 10, 10), (0, 0)), None);
    }

    #[test]
    fn clamping_survives_extreme_coordinates() {
        assert_eq!(
            clamp_to_screen(rect(i32::MAX, i32::MAX, u32::MAX, u32::MAX), (1, 1)),
            None
        );
        assert_eq!(
            clamp_to_screen(rect(i32::MIN, i32::MIN, u32::MAX, u32::MAX), (8, 4)),
            Some(rect(0, 0, 8, 4))
        );
    }

    #[test]
    fn fit_within_keeps_small_sources_and_preserves_aspect() {
        assert_eq!(fit_within((640, 480), (1024, 720)), (640, 480));
        assert_eq!(fit_within((2048, 1024), (1024, 720)), (1024, 512));
        assert_eq!(fit_within((1000, 4000), (1024, 720)), (180, 720));
        assert_eq!(fit_within((0, 480), (1024, 720)), (0, 0));
    }

    #[test]
    fn identity_preview_maps_points_unchanged() {
        let preview = Preview::new((200, 100), (200, 100)).unwrap();
        assert_eq!(preview.screen_point(10, 20), (10, 20));
        assert_eq!(
            preview.selection((10, 10), (60, 40)),
            Some(rect(10, 10, 50, 30))
        );
    }

    #[test]
    fn scaled_preview_maps_to_screen_pixels() {
        let preview = Preview::new((2000, 1000), (1000, 500)).unwrap();
        assert_eq!(preview.screen_point(100, 50), (200, 100));
        assert_eq!(
            preview.selection((100, 50), (300, 150)),
            Some(rect(200, 100, 400, 200))
        );
    }

    #[test]
    fn preview_points_outside_the_preview_are_clamped() {
        let preview = Preview::new((200, 100), (100, 50)).unwrap();
        assert_eq!(preview.screen_point(-50, -50), (0, 0));
        assert_eq!(preview.screen_point(500, 500), (200, 100));
        assert_eq!(
            preview.selection((-20, -20), (50, 25)),
            Some(rect(0, 0, 100, 50))
        );
        assert_eq!(preview.outline((-20, 10), (300, 60)), rect(0, 10, 100, 40));
    }

    #[test]
    fn reversed_drags_select_the_same_region() {
        let preview = Preview::new((200, 100), (200, 100)).unwrap();
        assert_eq!(
            preview.selection((60, 40), (10, 10)),
            preview.selection((10, 10), (60, 40))
        );
    }

    #[test]
    fn a_click_without_movement_selects_nothing() {
        let preview = Preview::new((200, 100), (200, 100)).unwrap();
        assert_eq!(preview.selection((30, 30), (30, 30)), None);
    }

    #[test]
    fn contains_is_half_open_on_the_preview_size() {
        let preview = Preview::new((200, 100), (100, 50)).unwrap();
        assert!(preview.contains(0, 0));
        assert!(preview.contains(99, 49));
        assert!(!preview.contains(100, 10));
        assert!(!preview.contains(-1, 10));
    }

    #[test]
    fn empty_sizes_have_no_preview() {
        assert_eq!(Preview::new((0, 100), (10, 10)), None);
        assert_eq!(Preview::new((100, 100), (10, 0)), None);
    }
}
