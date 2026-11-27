//! Backend-neutral draw lists, executed onto [`RasterSurface`] as the CPU
//! reference.
//!
//! A [`DrawList`] records logical-pixel commands: filled, rounded and stroked
//! rectangles, text runs, image blits and nested clips. It holds no window,
//! GPU or platform handle. [`DrawList::execute`] replays it onto a
//! `RasterSurface`, whose output is the pixel contract that any other backend
//! must reproduce. The parity tests at the bottom of this file pin that output.
//!
//! Semantics every backend must follow:
//! - Coordinates are logical pixels. A surface's integer [`Scale`] maps each
//!   one to an exact `scale × scale` block of device pixels.
//! - Rectangles are half-open and never anti-aliased. Fills are source-over
//!   and use the colour's alpha. Image blits multiply the image's alpha by
//!   their `opacity`.
//! - Drawing is clipped to the intersection of every active clip.
//! - Commands take effect in recording order. Execution stops at the first
//!   failing command, and the commands before it stay drawn.
//! - Each pixel of a stroke or rounded rectangle is painted once, so a
//!   translucent outline does not double-blend at its corners.
//!
//! [`Scene`](crate::scene::Scene) is the older M0 recorder (clear, fill and
//! stroke only). This module is the richer list that later backends target.

use crate::foundation::{color::Color, geometry::Rect};
use crate::image::Image;
use crate::render::{RasterSurface, RenderError};
use crate::text::Font;

/// Largest corner radius, in logical pixels, that the rounded-rectangle
/// executor draws. Larger radii are clamped to this value, which bounds the
/// per-corner work.
pub const MAX_CORNER_RADIUS: u32 = 256;

/// Identifies an image owned by the [`DrawList`] that returned it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageHandle(u32);

#[derive(Debug, Clone, PartialEq)]
pub enum DrawCommand {
    /// Source-over fill. The colour's alpha is the coverage.
    FillRect {
        rect: Rect,
        color: Color,
    },
    /// Fill with corners of `radius` logical pixels. The radius is clamped to
    /// half the shorter side and to [`MAX_CORNER_RADIUS`].
    RoundedRect {
        rect: Rect,
        radius: u32,
        color: Color,
    },
    /// A one-logical-pixel outline just inside `rect`.
    StrokeRect {
        rect: Rect,
        color: Color,
    },
    /// Text with its top-left corner at (`x`, `y`). The colour's alpha applies.
    TextRun {
        x: i32,
        y: i32,
        text: String,
        font: Font,
        color: Color,
    },
    /// Image scaled into `rect`. Integer ratios sample nearest, others
    /// bicubic. `opacity` multiplies the image's alpha (255 leaves it as is,
    /// 0 draws nothing).
    ImageBlit {
        image: ImageHandle,
        rect: Rect,
        opacity: u8,
    },
    /// Intersect all following drawing with `rect` until the matching
    /// [`DrawCommand::PopClip`].
    PushClip {
        rect: Rect,
    },
    PopClip,
}

#[derive(Debug, thiserror::Error)]
pub enum ListError {
    #[error("draw list command limit exceeded")]
    CommandLimitExceeded,
    #[error("draw list image limit exceeded")]
    ImageLimitExceeded,
    #[error("draw list allocation failed")]
    AllocationFailed,
    #[error("rectangle sizes must not be negative")]
    NegativeSize,
    #[error("PopClip without a matching PushClip")]
    UnbalancedClip,
    #[error("command refers to an image that this list does not own")]
    UnknownImage,
    #[error(transparent)]
    Render(#[from] RenderError),
}

/// Build a [`Rect`] from signed extents, rejecting negative sizes. The
/// origin may be negative: such a rectangle is clipped at execution time.
pub fn rect_from_signed(x: i32, y: i32, width: i32, height: i32) -> Result<Rect, ListError> {
    if width < 0 || height < 0 {
        return Err(ListError::NegativeSize);
    }
    Ok(Rect {
        x,
        y,
        width: width as u32,
        height: height as u32,
    })
}

/// A bounded recording of [`DrawCommand`]s and the images they reference.
///
/// `command_limit` caps the number of commands and, separately, the number of
/// images. Both are checked at record time, so the list's size is known
/// before any drawing happens.
#[derive(Debug, Clone)]
pub struct DrawList {
    commands: Vec<DrawCommand>,
    images: Vec<Image>,
    command_limit: usize,
    clip_depth: usize,
}

impl DrawList {
    pub fn new(command_limit: usize) -> Self {
        Self {
            commands: Vec::new(),
            images: Vec::new(),
            command_limit,
            clip_depth: 0,
        }
    }

    pub fn commands(&self) -> &[DrawCommand] {
        &self.commands
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Remove every command and image.
    pub fn clear(&mut self) {
        self.commands.clear();
        self.images.clear();
        self.clip_depth = 0;
    }

    /// Give the list ownership of `image` and return the handle that
    /// [`DrawCommand::ImageBlit`] uses.
    pub fn add_image(&mut self, image: Image) -> Result<ImageHandle, ListError> {
        if self.images.len() >= self.command_limit {
            return Err(ListError::ImageLimitExceeded);
        }
        let index = u32::try_from(self.images.len()).map_err(|_| ListError::ImageLimitExceeded)?;
        self.images
            .try_reserve(1)
            .map_err(|_| ListError::AllocationFailed)?;
        self.images.push(image);
        Ok(ImageHandle(index))
    }

    /// Record a command. Unbalanced clips and unknown image handles are
    /// rejected here, so an accepted list can always be replayed in order.
    pub fn push(&mut self, command: DrawCommand) -> Result<(), ListError> {
        if self.commands.len() >= self.command_limit {
            return Err(ListError::CommandLimitExceeded);
        }
        match &command {
            DrawCommand::ImageBlit { image, .. } => {
                self.image(*image)?;
            }
            DrawCommand::PopClip if self.clip_depth == 0 => {
                return Err(ListError::UnbalancedClip);
            }
            _ => {}
        }
        self.commands
            .try_reserve(1)
            .map_err(|_| ListError::AllocationFailed)?;
        match command {
            DrawCommand::PushClip { .. } => self.clip_depth += 1,
            DrawCommand::PopClip => self.clip_depth -= 1,
            _ => {}
        }
        self.commands.push(command);
        Ok(())
    }

    fn image(&self, handle: ImageHandle) -> Result<&Image, ListError> {
        self.images
            .get(handle.0 as usize)
            .ok_or(ListError::UnknownImage)
    }

    /// Replay the list onto `surface`. Every clip the list pushes is popped
    /// again before this returns, even when it fails. A list whose clips are
    /// unbalanced is refused before anything is drawn.
    pub fn execute(&self, surface: &RasterSurface) -> Result<(), ListError> {
        if self.clip_depth != 0 {
            return Err(ListError::UnbalancedClip);
        }
        let mut pushed = 0usize;
        let result = self.replay(surface, &mut pushed);
        for _ in 0..pushed {
            surface.pop_clip();
        }
        result
    }

    fn replay(&self, surface: &RasterSurface, pushed: &mut usize) -> Result<(), ListError> {
        for command in &self.commands {
            match command {
                DrawCommand::FillRect { rect, color } => surface.fill_rect(*rect, *color)?,
                DrawCommand::RoundedRect {
                    rect,
                    radius,
                    color,
                } => fill_rounded(surface, *rect, *radius, *color)?,
                DrawCommand::StrokeRect { rect, color } => stroke_edges(surface, *rect, *color)?,
                DrawCommand::TextRun {
                    x,
                    y,
                    text,
                    font,
                    color,
                } => surface.draw_text(text, *x, *y, font, *color)?,
                DrawCommand::ImageBlit {
                    image,
                    rect,
                    opacity,
                } => {
                    let image = self.image(*image)?;
                    if rect.width > 0 && rect.height > 0 && *opacity > 0 {
                        surface.draw_image_scaled_with_opacity(
                            image,
                            rect.x,
                            rect.y,
                            rect.width,
                            rect.height,
                            *opacity,
                        )?;
                    }
                }
                DrawCommand::PushClip { rect } => {
                    surface.push_clip(*rect)?;
                    *pushed += 1;
                }
                DrawCommand::PopClip => {
                    if *pushed == 0 {
                        return Err(ListError::UnbalancedClip);
                    }
                    *pushed -= 1;
                    surface.pop_clip();
                }
            }
        }
        Ok(())
    }
}

/// A rectangle from 64-bit logical or device extents. Negative sizes and
/// values beyond the `i32`/`u32` range are overflow errors.
fn rect_from_wide(x: i64, y: i64, width: i64, height: i64) -> Result<Rect, RenderError> {
    let overflow = || RenderError::GeometryOverflow;
    Ok(Rect {
        x: i32::try_from(x).map_err(|_| overflow())?,
        y: i32::try_from(y).map_err(|_| overflow())?,
        width: u32::try_from(width).map_err(|_| overflow())?,
        height: u32::try_from(height).map_err(|_| overflow())?,
    })
}

/// Outline as four disjoint edges, so each pixel is painted once.
fn stroke_edges(surface: &RasterSurface, rect: Rect, color: Color) -> Result<(), RenderError> {
    if rect.width == 0 || rect.height == 0 {
        return Ok(());
    }
    let (x, y) = (i64::from(rect.x), i64::from(rect.y));
    let (width, height) = (i64::from(rect.width), i64::from(rect.height));
    let edge = |x: i64, y: i64, width: i64, height: i64| -> Result<(), RenderError> {
        surface.fill_rect(rect_from_wide(x, y, width, height)?, color)
    };
    edge(x, y, width, 1)?;
    if height >= 2 {
        edge(x, y + height - 1, width, 1)?;
    }
    if height >= 3 {
        edge(x, y + 1, 1, height - 2)?;
        if width >= 2 {
            edge(x + width - 1, y + 1, 1, height - 2)?;
        }
    }
    Ok(())
}

/// Horizontal inset, in device pixels, of the first painted pixel of row
/// `row` in a top-left corner of radius `radius`. Pixel centres are tested in
/// doubled coordinates, so every comparison is an exact integer test: a pixel
/// is painted when its centre lies inside the circle of radius `radius`
/// centred at (`radius`, `radius`). Callers use `row` both from the top and,
/// mirrored, from the bottom.
fn corner_inset(radius: i64, row: i64) -> i64 {
    let diameter = 2 * radius;
    let dy = diameter - (2 * row + 1);
    let reach = diameter * diameter;
    (0..radius)
        .find(|&column| {
            let dx = diameter - (2 * column + 1);
            dx * dx + dy * dy <= reach
        })
        // Unreachable for radius >= 1: the column next to the edge is always
        // inside. Fall back to a full inset rather than a panic.
        .unwrap_or(radius)
}

/// Fill a rounded rectangle, painting every device pixel once. The middle
/// band is one fill, and each corner row is one fill per corner.
fn fill_rounded(
    surface: &RasterSurface,
    rect: Rect,
    radius: u32,
    color: Color,
) -> Result<(), RenderError> {
    if rect.width == 0 || rect.height == 0 {
        return Ok(());
    }
    let scale = surface.scale();
    let device = rect.to_device(scale).ok_or(RenderError::GeometryOverflow)?;
    let (left, top) = (i64::from(device.x), i64::from(device.y));
    let (width, height) = (i64::from(device.width), i64::from(device.height));
    let logical = radius.min(MAX_CORNER_RADIUS);
    let radius = i64::from(logical.saturating_mul(scale.factor())).min(width.min(height) / 2);
    if radius == 0 {
        return surface.fill_rect(rect, color);
    }
    surface.fill_device_rect(
        rect_from_wide(left, top + radius, width, height - 2 * radius)?,
        color,
    );
    for row in 0..radius {
        let inset = corner_inset(radius, row);
        let (start, end) = (left + inset, left + width - inset);
        if end > start {
            let span = end - start;
            surface.fill_device_rect(rect_from_wide(start, top + row, span, 1)?, color);
            surface.fill_device_rect(
                rect_from_wide(start, top + height - 1 - row, span, 1)?,
                color,
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::geometry::Scale;

    const WHITE: Color = Color::WHITE;
    const BLACK: Color = Color::BLACK;
    const HALF_RED: Color = Color {
        red: 255,
        green: 0,
        blue: 0,
        alpha: 128,
    };

    // Golden checksums (FNV-1a over the premultiplied device bytes) of the
    // fixed lists below, captured from the CPU reference at this commit.
    // They are regression pins: a backend must produce the same bytes, and a
    // change here must be a deliberate, reviewed change to the reference.
    const FILL_1X: u64 = 0x7192a7574e6eccb5;
    const FILL_2X: u64 = 0x499962cc9375d965;
    const ROUNDED_1X: u64 = 0x369f639324bfe295;
    const ROUNDED_2X: u64 = 0x999528050416a555;
    const STROKE_1X: u64 = 0xb7ab6fbd5a71c4e5;
    const STROKE_2X: u64 = 0x7778df89027fbc45;
    const IMAGE_1X: u64 = 0x7d246493659b09c8;
    const IMAGE_2X: u64 = 0x28a5e998ecd8d225;
    const CLIP_1X: u64 = 0xcd9cafa395d16464;
    const CLIP_2X: u64 = 0x15aed91def657515;

    fn rect(x: i32, y: i32, width: u32, height: u32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    fn surface(width: u32, height: u32, factor: u32) -> RasterSurface {
        RasterSurface::new(width, height, Scale::new(factor).unwrap()).unwrap()
    }

    fn blank(width: u32, height: u32, factor: u32, background: Color) -> RasterSurface {
        let canvas = surface(width, height, factor);
        canvas.clear(background).unwrap();
        canvas
    }

    fn list_of(commands: &[DrawCommand]) -> DrawList {
        let mut list = DrawList::new(64);
        for command in commands {
            list.push(command.clone()).unwrap();
        }
        list
    }

    fn render(list: &DrawList, width: u32, height: u32, factor: u32) -> RasterSurface {
        let canvas = blank(width, height, factor, WHITE);
        list.execute(&canvas).unwrap();
        canvas
    }

    fn fnv1a(bytes: &[u8]) -> u64 {
        bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, &byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    }

    fn checksum(canvas: &RasterSurface) -> u64 {
        fnv1a(&canvas.data()[..])
    }

    /// Render `list` on a white canvas and check it against the golden
    /// checksum.
    fn golden(
        list: &DrawList,
        width: u32,
        height: u32,
        factor: u32,
        expected: u64,
    ) -> RasterSurface {
        let canvas = render(list, width, height, factor);
        assert_eq!(
            checksum(&canvas),
            expected,
            "golden checksum changed at scale {factor}"
        );
        canvas
    }

    fn device(canvas: &RasterSurface, x: u32, y: u32) -> u32 {
        canvas.pixel(x, y).unwrap()
    }

    fn checker() -> Image {
        Image::from_rgba(
            2,
            2,
            vec![
                0, 0, 0, 255, // black
                255, 255, 255, 255, // white
                255, 0, 0, 255, // red
                0, 0, 255, 255, // blue
            ],
        )
        .unwrap()
    }

    #[test]
    fn fill_matches_the_direct_raster_call_at_both_scales() {
        let list = list_of(&[DrawCommand::FillRect {
            rect: rect(1, 1, 4, 3),
            color: BLACK,
        }]);
        for (factor, expected) in [(1, FILL_1X), (2, FILL_2X)] {
            let listed = golden(&list, 8, 6, factor, expected);
            let direct = blank(8, 6, factor, WHITE);
            direct.fill_rect(rect(1, 1, 4, 3), BLACK).unwrap();
            assert_eq!(&listed.data()[..], &direct.data()[..]);
        }
    }

    #[test]
    fn rounded_corners_match_hand_derived_pixels_at_scale_one() {
        let list = list_of(&[DrawCommand::RoundedRect {
            rect: rect(0, 0, 8, 6),
            radius: 2,
            color: BLACK,
        }]);
        let canvas = render(&list, 8, 6, 1);
        // Radius 2 cuts one pixel from the top row's ends, none from the
        // second row. The bottom mirrors that.
        assert_eq!(device(&canvas, 0, 0), 0xffffffff);
        assert_eq!(device(&canvas, 1, 0), 0xff000000);
        assert_eq!(device(&canvas, 6, 0), 0xff000000);
        assert_eq!(device(&canvas, 7, 0), 0xffffffff);
        assert_eq!(device(&canvas, 0, 1), 0xff000000);
        assert_eq!(device(&canvas, 7, 1), 0xff000000);
        assert_eq!(device(&canvas, 0, 4), 0xff000000);
        assert_eq!(device(&canvas, 0, 5), 0xffffffff);
        assert_eq!(device(&canvas, 1, 5), 0xff000000);
        assert_eq!(device(&canvas, 7, 5), 0xffffffff);
        assert_eq!(device(&canvas, 3, 3), 0xff000000);
    }

    #[test]
    fn rounded_corners_match_hand_derived_pixels_at_scale_two() {
        let list = list_of(&[DrawCommand::RoundedRect {
            rect: rect(0, 0, 8, 6),
            radius: 2,
            color: BLACK,
        }]);
        let canvas = render(&list, 16, 12, 2);
        // Device radius 4 on a 16x12 block. Sampling is finer than at scale
        // one, so the boundary pixels differ from a 2x copy of that result.
        assert_eq!(device(&canvas, 0, 0), 0xffffffff);
        assert_eq!(device(&canvas, 1, 0), 0xffffffff);
        assert_eq!(device(&canvas, 2, 0), 0xff000000);
        assert_eq!(device(&canvas, 13, 0), 0xff000000);
        assert_eq!(device(&canvas, 14, 0), 0xffffffff);
        assert_eq!(device(&canvas, 1, 1), 0xff000000);
        assert_eq!(device(&canvas, 0, 1), 0xffffffff);
        assert_eq!(device(&canvas, 0, 2), 0xff000000);
        assert_eq!(device(&canvas, 2, 11), 0xff000000);
        assert_eq!(device(&canvas, 1, 11), 0xffffffff);
        assert_eq!(device(&canvas, 8, 6), 0xff000000);
    }

    #[test]
    fn rounded_radius_is_clamped_and_zero_radius_is_a_plain_fill() {
        let fill = |radius| {
            let list = list_of(&[DrawCommand::RoundedRect {
                rect: rect(0, 0, 8, 6),
                radius,
                color: HALF_RED,
            }]);
            render(&list, 8, 6, 1)
        };
        // Half of the shorter side (6) is 3, so 100 is the same shape as 3.
        assert_eq!(&fill(100).data()[..], &fill(3).data()[..]);
        let plain = blank(8, 6, 1, WHITE);
        plain.fill_rect(rect(0, 0, 8, 6), HALF_RED).unwrap();
        assert_eq!(&fill(0).data()[..], &plain.data()[..]);
    }

    #[test]
    fn rounded_rectangles_match_the_golden_output_at_both_scales() {
        let list = list_of(&[DrawCommand::RoundedRect {
            rect: rect(1, 1, 6, 4),
            radius: 2,
            color: BLACK,
        }]);
        golden(&list, 8, 6, 1, ROUNDED_1X);
        golden(&list, 16, 12, 2, ROUNDED_2X);
    }

    #[test]
    fn stroke_matches_the_direct_outline_for_opaque_colour() {
        let list = list_of(&[DrawCommand::StrokeRect {
            rect: rect(0, 0, 5, 4),
            color: BLACK,
        }]);
        for (factor, expected) in [(1, STROKE_1X), (2, STROKE_2X)] {
            let listed = golden(&list, 8, 6, factor, expected);
            let direct = blank(8, 6, factor, WHITE);
            direct.stroke_rect(rect(0, 0, 5, 4), BLACK).unwrap();
            assert_eq!(&listed.data()[..], &direct.data()[..]);
        }
    }

    #[test]
    fn thin_outlines_paint_each_edge_pixel_once() {
        let list = list_of(&[
            DrawCommand::StrokeRect {
                rect: rect(0, 0, 1, 3),
                color: BLACK,
            },
            DrawCommand::StrokeRect {
                rect: rect(3, 0, 3, 1),
                color: BLACK,
            },
        ]);
        let canvas = render(&list, 8, 4, 1);
        for y in 0..3 {
            assert_eq!(device(&canvas, 0, y), 0xff000000, "column at row {y}");
        }
        for x in 3..6 {
            assert_eq!(device(&canvas, x, 0), 0xff000000, "row at column {x}");
        }
        assert_eq!(device(&canvas, 6, 0), 0xffffffff);
    }

    #[test]
    fn translucent_outline_corners_are_not_double_blended() {
        let list = list_of(&[DrawCommand::StrokeRect {
            rect: rect(0, 0, 5, 4),
            color: HALF_RED,
        }]);
        let canvas = render(&list, 5, 4, 1);
        assert_eq!(device(&canvas, 0, 0), 0xffff7f7f);
        assert_eq!(device(&canvas, 4, 3), 0xffff7f7f);
        // The direct call paints corners twice, so it is darker there.
        let direct = blank(5, 4, 1, WHITE);
        direct.stroke_rect(rect(0, 0, 5, 4), HALF_RED).unwrap();
        assert_ne!(device(&direct, 0, 0), 0xffff7f7f);
    }

    #[test]
    fn translucent_fill_and_rounded_fill_use_source_over() {
        let fill = list_of(&[DrawCommand::FillRect {
            rect: rect(0, 0, 2, 1),
            color: HALF_RED,
        }]);
        let canvas = render(&fill, 2, 1, 1);
        assert_eq!(device(&canvas, 0, 0), 0xffff7f7f);
        assert_eq!(device(&canvas, 1, 0), 0xffff7f7f);

        let rounded = list_of(&[DrawCommand::RoundedRect {
            rect: rect(0, 0, 8, 6),
            radius: 2,
            color: HALF_RED,
        }]);
        let canvas = render(&rounded, 8, 6, 1);
        assert_eq!(device(&canvas, 3, 3), 0xffff7f7f);
        assert_eq!(device(&canvas, 0, 0), 0xffffffff);
    }

    #[test]
    fn text_runs_match_the_direct_draw_call_at_both_scales() {
        let font = Font::system(12.0);
        let list = list_of(&[DrawCommand::TextRun {
            x: 1,
            y: 1,
            text: "SLOPOS".to_string(),
            font: font.clone(),
            color: BLACK,
        }]);
        for factor in [1u32, 2] {
            let listed = render(&list, 48, 24, factor);
            let direct = blank(48, 24, factor, WHITE);
            direct.draw_text("SLOPOS", 1, 1, &font, BLACK).unwrap();
            assert_eq!(&listed.data()[..], &direct.data()[..]);
            let dark = listed
                .data()
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|pixel| pixel[0] < 128)
                .count();
            assert!(dark > 10, "text run left only {dark} dark pixels");
        }
    }

    #[test]
    fn image_blits_match_the_direct_draw_at_both_scales() {
        let mut list = DrawList::new(8);
        let handle = list.add_image(checker()).unwrap();
        list.push(DrawCommand::ImageBlit {
            image: handle,
            rect: rect(1, 1, 2, 2),
            opacity: 255,
        })
        .unwrap();
        for (factor, expected) in [(1, IMAGE_1X), (2, IMAGE_2X)] {
            let listed = golden(&list, 6, 5, factor, expected);
            let direct = blank(6, 5, factor, WHITE);
            direct.draw_image_scaled(&checker(), 1, 1, 2, 2).unwrap();
            assert_eq!(&listed.data()[..], &direct.data()[..]);
        }
    }

    #[test]
    fn image_opacity_scales_the_alpha_and_zero_draws_nothing() {
        let red = Image::from_rgba(1, 1, vec![255, 0, 0, 255]).unwrap();
        let mut list = DrawList::new(8);
        let handle = list.add_image(red).unwrap();
        list.push(DrawCommand::ImageBlit {
            image: handle,
            rect: rect(0, 0, 1, 1),
            opacity: 128,
        })
        .unwrap();
        let canvas = render(&list, 2, 1, 1);
        // Half-opaque red over white leaves red at full and green and blue
        // near 127.
        let pixel = device(&canvas, 0, 0);
        assert_eq!(pixel >> 24, 0xff);
        assert_eq!((pixel >> 16) & 0xff, 0xff);
        for shift in [8, 0] {
            let channel = (pixel >> shift) & 0xff;
            assert!(
                (126..=128).contains(&channel),
                "channel {shift} was {channel}"
            );
        }
        assert_eq!(device(&canvas, 1, 0), 0xffffffff);

        let mut silent = DrawList::new(8);
        let handle = silent
            .add_image(Image::from_rgba(1, 1, vec![255, 0, 0, 255]).unwrap())
            .unwrap();
        silent
            .push(DrawCommand::ImageBlit {
                image: handle,
                rect: rect(0, 0, 1, 1),
                opacity: 0,
            })
            .unwrap();
        let canvas = render(&silent, 2, 1, 1);
        assert_eq!(&canvas.data()[..], &blank(2, 1, 1, WHITE).data()[..]);
    }

    #[test]
    fn clip_limits_every_command_kind_and_is_popped_afterwards() {
        let font = Font::system(10.0);
        let mut list = DrawList::new(32);
        let handle = list.add_image(checker()).unwrap();
        for command in [
            DrawCommand::PushClip {
                rect: rect(0, 0, 4, 4),
            },
            DrawCommand::RoundedRect {
                rect: rect(-2, -2, 12, 12),
                radius: 3,
                color: BLACK,
            },
            DrawCommand::StrokeRect {
                rect: rect(0, 0, 8, 8),
                color: BLACK,
            },
            DrawCommand::ImageBlit {
                image: handle,
                rect: rect(3, 3, 4, 4),
                opacity: 255,
            },
            DrawCommand::TextRun {
                x: 1,
                y: 1,
                text: "SLOPOS".to_string(),
                font,
                color: BLACK,
            },
            DrawCommand::PopClip,
        ] {
            list.push(command).unwrap();
        }
        for factor in [1u32, 2] {
            let canvas = render(&list, 8, 8, factor);
            let clip_edge = 4 * factor;
            let mut inside_changed = false;
            for y in 0..8 * factor {
                for x in 0..8 * factor {
                    let value = device(&canvas, x, y);
                    if x < clip_edge && y < clip_edge {
                        inside_changed |= value != 0xffffffff;
                    } else {
                        assert_eq!(value, 0xffffffff, "leak at {x},{y} at scale {factor}");
                    }
                }
            }
            assert!(inside_changed, "nothing was drawn inside the clip");
        }

        // The list must have popped its clip: a direct fill reaches the corner.
        let canvas = render(&list, 8, 8, 1);
        canvas.fill_rect(rect(7, 7, 1, 1), BLACK).unwrap();
        assert_eq!(device(&canvas, 7, 7), 0xff000000);
    }

    #[test]
    fn nested_clips_intersect_and_restore_the_outer_clip() {
        let list = list_of(&[
            DrawCommand::PushClip {
                rect: rect(2, 2, 6, 6),
            },
            DrawCommand::PushClip {
                rect: rect(4, 0, 20, 20),
            },
            DrawCommand::FillRect {
                rect: rect(0, 0, 12, 10),
                color: BLACK,
            },
            DrawCommand::PopClip,
            DrawCommand::PopClip,
        ]);
        let canvas = render(&list, 12, 10, 1);
        assert_eq!(device(&canvas, 3, 3), 0xffffffff);
        assert_eq!(device(&canvas, 4, 2), 0xff000000);
        assert_eq!(device(&canvas, 7, 7), 0xff000000);
        assert_eq!(device(&canvas, 8, 7), 0xffffffff);
        assert_eq!(device(&canvas, 4, 8), 0xffffffff);
    }

    #[test]
    fn clips_and_fills_clamp_at_surface_edges_at_both_scales() {
        for factor in [1u32, 2] {
            let list = list_of(&[DrawCommand::FillRect {
                rect: rect(-2, -2, 4, 4),
                color: BLACK,
            }]);
            let canvas = render(&list, 4, 4, factor);
            // Only logical 0..2 is on the surface.
            assert_eq!(device(&canvas, 2 * factor - 1, 2 * factor - 1), 0xff000000);
            assert_eq!(device(&canvas, 2 * factor, 0), 0xffffffff);
            assert_eq!(device(&canvas, 0, 2 * factor), 0xffffffff);

            let oversized = list_of(&[
                DrawCommand::PushClip {
                    rect: rect(-100, -100, 1000, 1000),
                },
                DrawCommand::FillRect {
                    rect: rect(0, 0, 4, 4),
                    color: BLACK,
                },
                DrawCommand::PopClip,
            ]);
            let canvas = render(&oversized, 4, 4, factor);
            assert_eq!(device(&canvas, 4 * factor - 1, 4 * factor - 1), 0xff000000);
        }
    }

    #[test]
    fn clip_golden_output_matches_at_both_scales() {
        let list = list_of(&[
            DrawCommand::PushClip {
                rect: rect(2, 2, 3, 3),
            },
            DrawCommand::FillRect {
                rect: rect(0, 0, 8, 6),
                color: BLACK,
            },
            DrawCommand::PopClip,
        ]);
        golden(&list, 8, 6, 1, CLIP_1X);
        let canvas = golden(&list, 16, 12, 2, CLIP_2X);
        // The clip covers device 4..10 on both axes.
        assert_eq!(device(&canvas, 3, 3), 0xffffffff);
        assert_eq!(device(&canvas, 4, 4), 0xff000000);
        assert_eq!(device(&canvas, 9, 9), 0xff000000);
        assert_eq!(device(&canvas, 10, 10), 0xffffffff);
    }

    #[test]
    fn zero_sized_commands_and_empty_text_leave_the_surface_unchanged() {
        let mut list = DrawList::new(16);
        let handle = list.add_image(checker()).unwrap();
        for command in [
            DrawCommand::FillRect {
                rect: rect(1, 1, 0, 3),
                color: BLACK,
            },
            DrawCommand::FillRect {
                rect: rect(1, 1, 3, 0),
                color: BLACK,
            },
            DrawCommand::RoundedRect {
                rect: rect(1, 1, 0, 3),
                radius: 4,
                color: BLACK,
            },
            DrawCommand::StrokeRect {
                rect: rect(1, 1, 3, 0),
                color: BLACK,
            },
            DrawCommand::ImageBlit {
                image: handle,
                rect: rect(1, 1, 0, 2),
                opacity: 255,
            },
            DrawCommand::TextRun {
                x: 1,
                y: 1,
                text: String::new(),
                font: Font::system(12.0),
                color: BLACK,
            },
        ] {
            list.push(command).unwrap();
        }
        for factor in [1u32, 2] {
            let canvas = render(&list, 6, 6, factor);
            assert_eq!(&canvas.data()[..], &blank(6, 6, factor, WHITE).data()[..]);
        }
    }

    #[test]
    fn negative_sizes_are_rejected_and_negative_origins_are_allowed() {
        assert_eq!(
            rect_from_signed(0, 0, -1, 4).unwrap_err().to_string(),
            "rectangle sizes must not be negative"
        );
        assert!(matches!(
            rect_from_signed(0, 0, 4, -1),
            Err(ListError::NegativeSize)
        ));
        assert_eq!(rect_from_signed(-3, -4, 0, 0).unwrap(), rect(-3, -4, 0, 0));
        assert_eq!(rect_from_signed(-3, 2, 5, 6).unwrap(), rect(-3, 2, 5, 6));
    }

    #[test]
    fn unbalanced_clips_are_refused_before_any_drawing() {
        let mut list = DrawList::new(8);
        assert!(matches!(
            list.push(DrawCommand::PopClip),
            Err(ListError::UnbalancedClip)
        ));
        list.push(DrawCommand::FillRect {
            rect: rect(0, 0, 2, 2),
            color: BLACK,
        })
        .unwrap();
        list.push(DrawCommand::PushClip {
            rect: rect(0, 0, 2, 2),
        })
        .unwrap();
        let canvas = blank(4, 4, 1, WHITE);
        assert!(matches!(
            list.execute(&canvas),
            Err(ListError::UnbalancedClip)
        ));
        assert_eq!(&canvas.data()[..], &blank(4, 4, 1, WHITE).data()[..]);
    }

    #[test]
    fn image_handles_belong_to_their_list() {
        let mut owner = DrawList::new(8);
        let handle = owner.add_image(checker()).unwrap();
        let mut other = DrawList::new(8);
        assert!(matches!(
            other.push(DrawCommand::ImageBlit {
                image: handle,
                rect: rect(0, 0, 2, 2),
                opacity: 255,
            }),
            Err(ListError::UnknownImage)
        ));
    }

    #[test]
    fn command_and_image_limits_are_enforced_at_record_time() {
        let mut list = DrawList::new(2);
        list.push(DrawCommand::PopClip).unwrap_err();
        list.push(DrawCommand::FillRect {
            rect: rect(0, 0, 1, 1),
            color: BLACK,
        })
        .unwrap();
        list.push(DrawCommand::FillRect {
            rect: rect(0, 0, 1, 1),
            color: BLACK,
        })
        .unwrap();
        assert!(matches!(
            list.push(DrawCommand::FillRect {
                rect: rect(0, 0, 1, 1),
                color: BLACK,
            }),
            Err(ListError::CommandLimitExceeded)
        ));
        assert_eq!(list.len(), 2);
        list.clear();
        assert!(list.is_empty());

        let mut images = DrawList::new(1);
        images.add_image(checker()).unwrap();
        assert!(matches!(
            images.add_image(checker()),
            Err(ListError::ImageLimitExceeded)
        ));
    }

    #[test]
    fn execution_stops_at_the_first_geometry_error_and_restores_clips() {
        let list = list_of(&[
            DrawCommand::FillRect {
                rect: rect(0, 0, 1, 1),
                color: BLACK,
            },
            DrawCommand::PushClip {
                rect: rect(0, 0, 2, 2),
            },
            DrawCommand::FillRect {
                rect: rect(i32::MAX, 0, 1, 1),
                color: BLACK,
            },
            DrawCommand::FillRect {
                rect: rect(1, 0, 1, 1),
                color: BLACK,
            },
            DrawCommand::PopClip,
        ]);
        let canvas = blank(4, 4, 2, WHITE);
        assert!(matches!(
            list.execute(&canvas),
            Err(ListError::Render(RenderError::GeometryOverflow))
        ));
        assert_eq!(device(&canvas, 0, 0), 0xff000000);
        // The command after the failure never ran.
        assert_eq!(device(&canvas, 2, 0), 0xffffffff);
        // The list's clip was popped, so a fill outside its old area lands.
        canvas.fill_rect(rect(3, 3, 1, 1), BLACK).unwrap();
        assert_eq!(device(&canvas, 7, 7), 0xff000000);
    }

    #[test]
    fn hidpi_scale_two_is_an_exact_upscale_for_rects_outlines_clips_and_images() {
        let mut list = DrawList::new(16);
        let handle = list.add_image(checker()).unwrap();
        for command in [
            DrawCommand::FillRect {
                rect: rect(1, 1, 4, 3),
                color: BLACK,
            },
            DrawCommand::StrokeRect {
                rect: rect(0, 0, 6, 5),
                color: HALF_RED,
            },
            DrawCommand::PushClip {
                rect: rect(2, 0, 5, 4),
            },
            DrawCommand::FillRect {
                rect: rect(3, 2, 5, 5),
                color: BLACK,
            },
            DrawCommand::PopClip,
            DrawCommand::ImageBlit {
                image: handle,
                rect: rect(5, 4, 2, 2),
                opacity: 255,
            },
        ] {
            list.push(command).unwrap();
        }
        let low = render(&list, 10, 8, 1);
        let high = render(&list, 10, 8, 2);
        for y in 0..16 {
            for x in 0..20 {
                assert_eq!(
                    device(&high, x, y),
                    device(&low, x / 2, y / 2),
                    "device pixel {x},{y} differs from its 1x source"
                );
            }
        }
    }
}
