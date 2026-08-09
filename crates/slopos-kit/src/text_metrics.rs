//! Shared text measurements for geometry that is computed outside a canvas.
//!
//! Widget hit regions and render-tree nodes do not always have a canvas
//! available when they calculate their logical bounds.  Keep those callers on
//! the same shaped-text service as the immediate-mode presenter instead of
//! estimating widths from UTF-8 byte counts.

use slopos_render::font::{shape_text, TextLayoutOptions};

/// Measure one line of SLOPOS UI text in logical pixels.
///
/// The SDK canvas uses the same 13 px UI font at scale 1 for its default
/// logical measurements.  The returned value is therefore suitable for
/// hit-target and render-tree geometry; physical framebuffer scaling is
/// applied by the eventual canvas/renderer.
pub fn measure_text_width(text: &str) -> f32 {
    shape_text(text, TextLayoutOptions::new(13.0, 1.0)).first_line_width()
}

#[cfg(test)]
mod tests {
    use super::measure_text_width;
    use slopos_render::font::{shape_text, TextLayoutOptions};

    #[test]
    fn width_comes_from_shaped_text_for_variable_width_and_unicode_labels() {
        for text in ["Wiii", "日本語", "Aé e\u{301} fi"] {
            let expected = shape_text(text, TextLayoutOptions::new(13.0, 1.0)).first_line_width();
            assert!((measure_text_width(text) - expected).abs() < 0.01);
        }
    }
}
