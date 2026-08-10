use crate::{AccessibilityNode, AccessibilityRole, LayoutConstraint, Size, Widget, WidgetState};
use std::any::Any;
use std::sync::Arc;

/// A decoded RGBA8 image owned by the widget tree.
///
/// The SDK painter uploads this source to retained GPU tile textures. Keeping
/// the bytes behind an `Arc` lets every visible tile share one immutable source
/// without copying the image on each frame.
pub struct ImageView {
    state: WidgetState,
    width: u32,
    height: u32,
    pixels: Arc<[u8]>,
    rotation_quadrants: u8,
}

impl ImageView {
    /// Creates an image view from tightly packed RGBA8 pixels.
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, String> {
        if width == 0 || height == 0 {
            return Err("image dimensions must be non-zero".to_string());
        }
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| "image dimensions overflow RGBA8 storage".to_string())?;
        if pixels.len() != expected {
            return Err(format!(
                "RGBA8 image has {} bytes; expected {}",
                pixels.len(),
                expected
            ));
        }
        Ok(Self {
            state: WidgetState::new(),
            width,
            height,
            pixels: Arc::from(pixels),
            rotation_quadrants: 0,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Dimensions occupied by the image after the current clockwise
    /// quarter-turn rotation is applied. The decoded source dimensions and
    /// bytes remain unchanged.
    pub fn display_dimensions(&self) -> (u32, u32) {
        if self.rotation_quadrants.is_multiple_of(2) {
            (self.width, self.height)
        } else {
            (self.height, self.width)
        }
    }

    pub fn display_width(&self) -> u32 {
        self.display_dimensions().0
    }

    pub fn display_height(&self) -> u32 {
        self.display_dimensions().1
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn pixels_arc(&self) -> Arc<[u8]> {
        Arc::clone(&self.pixels)
    }

    /// Number of clockwise quarter turns applied when the SDK paints this
    /// image. Pixel storage remains in source orientation, so rotation does
    /// not duplicate decoded image memory.
    pub fn rotation_quadrants(&self) -> u8 {
        self.rotation_quadrants
    }

    pub fn set_rotation_quadrants(&mut self, quadrants: u8) {
        self.rotation_quadrants = quadrants % 4;
    }
}

impl Widget for ImageView {
    fn widget_state(&self) -> &WidgetState {
        &self.state
    }

    fn widget_state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        let size = constraint.clamp(Size::new(
            self.display_width() as f32,
            self.display_height() as f32,
        ));
        let rect = self.rect();
        self.set_rect(crate::Rect::new(rect.x, rect.y, size.width, size.height));
        size
    }

    fn draw(&self, _theme: &crate::ThemeContext) {}

    fn accessibility(&self) -> Option<AccessibilityNode> {
        Some(AccessibilityNode::new(AccessibilityRole::Image, "Image"))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::ImageView;

    #[test]
    fn rejects_invalid_rgba8_storage() {
        assert!(ImageView::new(2, 2, vec![0; 15]).is_err());
        assert!(ImageView::new(0, 2, vec![]).is_err());
    }

    #[test]
    fn retains_exact_source_dimensions_and_bytes() {
        let pixels = vec![7; 3 * 2 * 4];
        let view = ImageView::new(3, 2, pixels.clone()).unwrap();
        assert_eq!(view.width(), 3);
        assert_eq!(view.height(), 2);
        assert_eq!(view.pixels(), pixels.as_slice());
    }

    #[test]
    fn rotation_is_normalized_to_clockwise_quarter_turns() {
        let mut view = ImageView::new(1, 1, vec![0; 4]).unwrap();
        view.set_rotation_quadrants(6);
        assert_eq!(view.rotation_quadrants(), 2);
    }

    #[test]
    fn rotation_swaps_display_dimensions_without_touching_source_storage() {
        let pixels = vec![7; 3 * 2 * 4];
        let mut view = ImageView::new(3, 2, pixels.clone()).unwrap();

        assert_eq!(view.display_dimensions(), (3, 2));
        view.set_rotation_quadrants(1);
        assert_eq!(view.display_dimensions(), (2, 3));
        view.set_rotation_quadrants(3);
        assert_eq!(view.display_dimensions(), (2, 3));
        assert_eq!(view.pixels(), pixels.as_slice());
    }
}
