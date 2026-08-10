//! Shared text measurements for geometry that is computed outside a canvas.
//!
//! Widget hit regions and render-tree nodes do not always have a canvas
//! available when they calculate their logical bounds.  Keep those callers on
//! the same shaped-text service as the immediate-mode presenter instead of
//! estimating widths from UTF-8 byte counts.

use slopos_render::font::{shape_text, TextLayoutOptions};
use std::collections::HashMap;

/// Horizontal inset used by the SDK presenter when painting a text field's
/// value or placeholder.  Hit-testing uses the same origin so click-to-caret
/// mapping follows the pixels the user actually sees.
pub const TEXT_FIELD_TEXT_INSET: f32 = 6.0;

/// Measure one line of SLOPOS UI text in logical pixels.
///
/// The SDK canvas uses the same 13 px UI font at scale 1 for its default
/// logical measurements.  The returned value is therefore suitable for
/// hit-target and render-tree geometry; physical framebuffer scaling is
/// applied by the eventual canvas/renderer.
pub fn measure_text_width(text: &str) -> f32 {
    shape_text(text, TextLayoutOptions::new(13.0, 1.0)).first_line_width()
}

/// Map a logical x coordinate relative to a shaped text origin to the nearest
/// valid UTF-8 byte boundary on the first laid-out line.
///
/// The renderer exposes cluster ranges and glyph advances, so this follows
/// proportional widths, ligatures, combining marks and fallback glyphs
/// instead of treating every source character as a fixed-width cell.  A
/// caret is never placed inside a shaped cluster (or inside a UTF-8 codepoint).
pub fn text_byte_offset_at_x(text: &str, x: f32) -> usize {
    if text.is_empty() || !x.is_finite() {
        return 0;
    }

    let layout = shape_text(text, TextLayoutOptions::new(13.0, 1.0));

    // Aggregate visual bounds for every shaped cluster.  Cosmic normally
    // emits one range per cluster, but collecting by source range also keeps
    // this robust when a cluster contains multiple glyphs (for example a
    // base character plus combining mark).
    let mut cluster_bounds: HashMap<(usize, usize), (f32, f32)> = HashMap::new();
    for glyph in layout.glyphs() {
        if glyph.cluster_start >= glyph.cluster_end
            || glyph.cluster_end > text.len()
            || !text.is_char_boundary(glyph.cluster_start)
            || !text.is_char_boundary(glyph.cluster_end)
            || !glyph.x.is_finite()
            || !glyph.advance.is_finite()
        {
            continue;
        }
        let glyph_end = glyph.x + glyph.advance;
        if !glyph_end.is_finite() {
            continue;
        }
        let left = glyph.x.min(glyph_end);
        let right = glyph.x.max(glyph_end);
        cluster_bounds
            .entry((glyph.cluster_start, glyph.cluster_end))
            .and_modify(|bounds| {
                bounds.0 = bounds.0.min(left);
                bounds.1 = bounds.1.max(right);
            })
            .or_insert((left, right));
    }

    // Include the leading boundary even when the first glyph has a negative
    // bearing, then add each first-line cluster's two legal caret boundaries.
    let mut caret_boundaries = vec![(0.0, 0usize)];
    let mut first_line_end = 0usize;
    for range in layout.cluster_ranges() {
        first_line_end = first_line_end.max(range.end);
        let bounds = cluster_bounds
            .get(&(range.start, range.end))
            .copied()
            .or_else(|| {
                // Defensive fallback for a renderer that exposes a merged
                // cluster range while retaining sub-ranges on individual
                // glyphs.
                cluster_bounds
                    .iter()
                    .filter(|((start, end), _)| *start < range.end && *end > range.start)
                    .map(|(_, bounds)| *bounds)
                    .reduce(|(left, right), (other_left, other_right)| {
                        (left.min(other_left), right.max(other_right))
                    })
            });
        if let Some((left, right)) = bounds {
            caret_boundaries.push((left, range.start));
            caret_boundaries.push((right, range.end));
        }
    }

    // A final line boundary covers trailing whitespace and the degenerate
    // case where a line has no drawable glyphs.  `cluster_ranges` is scoped
    // to the first line, so this intentionally does not jump over a newline.
    if first_line_end > 0 {
        caret_boundaries.push((layout.first_line_width(), first_line_end));
    }

    let target = x.max(0.0);
    caret_boundaries
        .into_iter()
        .min_by(|(left_x, _), (right_x, _)| {
            let left_distance = (*left_x - target).abs();
            let right_distance = (*right_x - target).abs();
            left_distance
                .partial_cmp(&right_distance)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(_, offset)| offset)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{measure_text_width, text_byte_offset_at_x};
    use slopos_render::font::{shape_text, TextLayoutOptions};

    #[test]
    fn width_comes_from_shaped_text_for_variable_width_and_unicode_labels() {
        for text in ["Wiii", "日本語", "Aé e\u{301} fi"] {
            let expected = shape_text(text, TextLayoutOptions::new(13.0, 1.0)).first_line_width();
            assert!((measure_text_width(text) - expected).abs() < 0.01);
        }
    }

    #[test]
    fn caret_mapping_uses_proportional_cluster_advances() {
        let text = "Wiii";
        let layout = shape_text(text, TextLayoutOptions::new(13.0, 1.0));
        let first = layout
            .glyphs()
            .iter()
            .find(|glyph| glyph.cluster_start == 0)
            .expect("first glyph");
        let next = layout
            .glyphs()
            .iter()
            .find(|glyph| glyph.cluster_start == first.cluster_end)
            .expect("second glyph");
        let first_end = first.x + first.advance;
        let next_end = next.x + next.advance;
        let click_x = first_end + (next_end - first_end) * 0.2;

        assert_eq!(
            text_byte_offset_at_x(text, click_x),
            first.cluster_end,
            "click should land after the wide W, not at a fixed 7px character index"
        );
    }

    #[test]
    fn caret_mapping_keeps_combining_cluster_intact() {
        let text = "Ae\u{301}B";
        let layout = shape_text(text, TextLayoutOptions::new(13.0, 1.0));
        let cluster = layout
            .cluster_ranges()
            .iter()
            .find(|range| text.get((*range).clone()) == Some("e\u{301}"))
            .expect("combining cluster");
        let glyph = layout
            .glyphs()
            .iter()
            .find(|glyph| glyph.cluster_start == cluster.start)
            .expect("combining glyph");
        let click_x = glyph.x + glyph.advance * 0.5;
        let offset = text_byte_offset_at_x(text, click_x);

        assert!(
            offset == cluster.start || offset == cluster.end,
            "caret must stop at cluster boundaries, got {offset} for {cluster:?}"
        );
    }
}
