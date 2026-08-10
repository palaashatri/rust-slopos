use image::RgbaImage;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// A rectangle in source-image pixel coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl PixelRect {
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn contains(&self, px: u32, py: u32) -> bool {
        px >= self.x
            && py >= self.y
            && px < self.x.saturating_add(self.width)
            && py < self.y.saturating_add(self.height)
    }

    /// Bounding rectangle of two rectangles.
    pub fn union(a: &Self, b: &Self) -> Self {
        let x1 = a.x.min(b.x);
        let y1 = a.y.min(b.y);
        let x2 = a.x.saturating_add(a.width).max(b.x.saturating_add(b.width));
        let y2 =
            a.y.saturating_add(a.height)
                .max(b.y.saturating_add(b.height));
        Self::new(x1, y1, x2.saturating_sub(x1), y2.saturating_sub(y1))
    }
}

/// A single recognized word with its source-image bounds.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TextWord {
    pub text: String,
    pub bounds: PixelRect,
    pub confidence: Option<f32>,
}

/// A recognized line, composed of one or more words.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TextLine {
    pub text: String,
    pub bounds: PixelRect,
    pub words: Vec<TextWord>,
    pub confidence: Option<f32>,
}

/// The full OCR result for one image.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OcrResult {
    pub image_width: u32,
    pub image_height: u32,
    pub lines: Vec<TextLine>,
}

impl OcrResult {
    /// Concatenated recognized text, one line per entry.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for (i, line) in self.lines.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(&line.text);
        }
        out
    }

    /// Flat iterator over all words in reading order.
    pub fn all_words(&self) -> impl Iterator<Item = &TextWord> {
        self.lines.iter().flat_map(|line| line.words.iter())
    }
}

/// A subject alpha mask at source-image dimensions.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SubjectMask {
    pub width: u32,
    pub height: u32,
    /// Per-pixel alpha, row-major, `width * height` bytes.
    pub alpha: Vec<u8>,
    pub confidence: Option<f32>,
}

/// The result of lifting the main subject out of an image.
#[derive(Debug, Clone)]
pub struct LiftedSubject {
    pub image: RgbaImage,
    pub mask: SubjectMask,
    pub source_bounds: PixelRect,
}

/// Options controlling OCR inference.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OcrOptions {
    /// Drop recognized words whose confidence is below this value.
    ///
    /// The effective floor is `max(min_confidence, 0.5)`.
    pub min_confidence: f32,
    /// Optional cancellation flag checked between inference stages.
    #[serde(skip)]
    pub cancel: Option<Arc<AtomicBool>>,
}

impl Default for OcrOptions {
    fn default() -> Self {
        Self {
            min_confidence: 0.5,
            cancel: None,
        }
    }
}

/// Options controlling subject segmentation.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SegmentationOptions {
    pub mask_post: MaskPostProcessOptions,
    #[serde(skip)]
    pub cancel: Option<Arc<AtomicBool>>,
}

/// Conservative mask post-processing options.
///
/// The defaults are deliberately conservative: a hard threshold with the
/// largest connected component retained and no feathering. Callers that want
/// softer edges can raise [`Self::feather_radius`].
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MaskPostProcessOptions {
    /// Alpha threshold below which a pixel is treated as background.
    pub threshold: f32,
    /// Edge feathering radius in source pixels. `0` disables feathering.
    pub feather_radius: u32,
    /// Remove connected foreground components with an area below this many
    /// pixels. `0` keeps everything above the threshold.
    pub remove_small_components_below: u32,
    /// Fill background holes with an area below this many pixels. `0`
    /// disables hole filling.
    pub fill_holes_below: u32,
    /// Keep only the largest connected foreground component.
    pub keep_largest_component: bool,
}

impl Default for MaskPostProcessOptions {
    fn default() -> Self {
        Self {
            threshold: 0.5,
            feather_radius: 0,
            remove_small_components_below: 0,
            fill_holes_below: 0,
            keep_largest_component: true,
        }
    }
}
