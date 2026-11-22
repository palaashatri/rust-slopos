//! The recording mode of [`RasterSurface`].
//!
//! Immediate-mode apps paint through `&RasterSurface`, and their `paint`
//! signature cannot change. A recording surface therefore takes the same
//! calls and records each one as a [`DrawCommand`] instead of rasterising it.
//! The recorded [`DrawList`] replays onto a `RasterSurface` (the CPU reference)
//! or onto any other [`DrawExecutor`].
//!
//! Recording mirrors the reference path call by call:
//! - Each call validates its geometry the same way and returns the same error.
//! - `stroke_rect` is recorded as its four edge fills, in the reference's
//!   order. Overlapping corners are therefore blended twice, exactly as
//!   direct painting blends them. The list's own `StrokeRect` paints each
//!   pixel once, so it is not used here.
//! - `clear` is recorded as a full-surface fill. The two agree on a blank
//!   surface, and for an opaque colour. Any other `clear` is a failure.
//! - Calls that a list cannot express (`draw_surface`, a translucent or
//!   clipped `clear`, an open clip at the end, a full command limit) are latched
//!   as the first failure and the call itself returns `Ok`. Immediate-mode apps
//!   therefore never see an error that the CPU path would not return, and never
//!   unwind out of their paint. `take` reports the latched failure, so the frame
//!   falls back to the CPU and an incomplete list is never returned as whole.

use std::cell::RefCell;

use crate::image::Image;
use crate::render::list::{DrawCommand, DrawList, ImageHandle};
use crate::render::RenderError;

/// Owns the list being recorded and the first failure seen since the last take.
pub(super) struct Recorder {
    command_limit: usize,
    list: RefCell<DrawList>,
    failure: RefCell<Option<String>>,
}

impl Recorder {
    pub(super) fn new(command_limit: usize) -> Self {
        Self {
            command_limit,
            list: RefCell::new(DrawList::new(command_limit)),
            failure: RefCell::new(None),
        }
    }

    /// Append `command`. A command that cannot be recorded is latched as the
    /// failure; the call still returns `Ok`, and `take` reports it.
    pub(super) fn record(&self, command: DrawCommand) -> Result<(), RenderError> {
        if self.failure.borrow().is_some() {
            return Ok(());
        }
        let result = self.list.borrow_mut().push(command);
        if let Err(error) = result {
            self.latch(error.to_string());
        }
        Ok(())
    }

    /// Latch `message` as the failure if none is latched yet. Later commands
    /// are then ignored, and `take` returns the failure.
    pub(super) fn latch(&self, message: String) {
        let mut failure = self.failure.borrow_mut();
        if failure.is_none() {
            *failure = Some(message);
        }
    }

    /// Give the list a copy of `image` and return its handle, or `None` after
    /// latching a failure.
    pub(super) fn image(&self, image: &Image) -> Option<ImageHandle> {
        let result = self.list.borrow_mut().add_image(image.clone());
        match result {
            Ok(handle) => Some(handle),
            Err(error) => {
                self.latch(error.to_string());
                None
            }
        }
    }

    /// Return the recorded list and start an empty one. A latched failure is
    /// returned instead, and stays latched. A list that still has an open clip
    /// is refused and kept, so its clip state never falls out of step with the
    /// surface.
    pub(super) fn take(&self) -> Result<DrawList, RenderError> {
        if let Some(message) = self.failure.borrow().clone() {
            return Err(RenderError::Recording(message));
        }
        if self.list.borrow().clip_depth() != 0 {
            return Err(RenderError::Recording(
                "a clip is still open when the recording ends".into(),
            ));
        }
        Ok(self.list.replace(DrawList::new(self.command_limit)))
    }
}

/// The logical rectangle of a `width × height` surface.
#[cfg(test)]
fn whole(width: u32, height: u32) -> crate::foundation::geometry::Rect {
    crate::foundation::geometry::Rect {
        x: 0,
        y: 0,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controls::button::Button;
    use crate::foundation::color::Color;
    use crate::foundation::geometry::{Rect, Scale};
    use crate::primitives::button_plate::ButtonPlateVariant;
    use crate::render::list::{CpuExecutor, DrawExecutor};
    use crate::render::RasterSurface;
    use crate::text::Font;

    const LIMIT: usize = 4096;
    const CALCULATOR_BACKGROUND: Color = Color {
        red: 0xdd,
        green: 0xdd,
        blue: 0xdd,
        alpha: 0xff,
    };

    /// The Calculator's paint sequence, rebuilt from its public controls. The
    /// app keeps its state in `apps/calculator`, which this crate cannot
    /// depend on, so the keys and display are fixed here: the sequence of
    /// calls is the same, the values are not.
    fn calculator_like(surface: &RasterSurface, display_text: &str) -> Result<(), RenderError> {
        let width = surface.logical_width() as i32;
        surface.clear(CALCULATOR_BACKGROUND)?;
        let display = Rect {
            x: 6,
            y: 6,
            width: (width - 12) as u32,
            height: 30,
        };
        surface.fill_rect(display, Color::WHITE)?;
        surface.stroke_rect(display, Color::BLACK)?;
        let display_font = Font::system(18.0);
        surface.draw_text(
            display_text,
            display.x + 8,
            display.y + 6,
            &display_font,
            Color::BLACK,
        )?;
        let key_font = Font::system(12.0);
        let labels = [
            "7", "8", "9", "/", "4", "5", "6", "*", "1", "2", "3", "-", "0", ".", "=", "+",
        ];
        for (index, label) in labels.iter().enumerate() {
            let (column, row) = ((index % 4) as i32, (index / 4) as i32);
            let variant = if *label == "=" {
                ButtonPlateVariant::Default
            } else {
                ButtonPlateVariant::Regular
            };
            let bounds = Rect {
                x: 6 + column * 40,
                y: 46 + row * 30,
                width: 36,
                height: 26,
            };
            let button = Button::new(index as u64 + 1, label, variant).with_bounds(bounds);
            button.paint(surface, &key_font)?;
        }
        crate::controls::button::paint_focus_ring(
            surface,
            Rect {
                x: 6,
                y: 46,
                width: 36,
                height: 26,
            },
        )
    }

    /// Paint with `paint` directly and as a recording replayed on the CPU
    /// executor, and assert the two surfaces are byte-identical. Returns the
    /// recorded list.
    fn assert_recording_matches_direct(
        name: &str,
        width: u32,
        height: u32,
        scale: Scale,
        paint: impl Fn(&RasterSurface) -> Result<(), RenderError>,
    ) -> DrawList {
        let direct = RasterSurface::new(width, height, scale).unwrap();
        paint(&direct).unwrap_or_else(|error| panic!("{name}: direct paint failed: {error}"));
        let recording = RasterSurface::recording(width, height, scale, LIMIT).unwrap();
        paint(&recording).unwrap_or_else(|error| panic!("{name}: recording failed: {error}"));
        let list = recording
            .take_recording()
            .unwrap_or_else(|error| panic!("{name}: take failed: {error}"));
        assert!(!list.is_empty(), "{name}: nothing was recorded");
        let mut replay = RasterSurface::new(width, height, scale).unwrap();
        CpuExecutor
            .execute(&list, &mut replay)
            .unwrap_or_else(|error| panic!("{name}: replay failed: {error}"));
        assert_same_pixels(name, &direct, &replay);
        list
    }

    fn assert_same_pixels(name: &str, expected: &RasterSurface, actual: &RasterSurface) {
        let (expected_bytes, actual_bytes) = (expected.data(), actual.data());
        if expected_bytes[..] == actual_bytes[..] {
            return;
        }
        let width = expected.device_width() as usize;
        let first = expected_bytes
            .iter()
            .zip(actual_bytes.iter())
            .position(|(a, b)| a != b)
            .unwrap_or(0)
            / 4;
        panic!(
            "{name}: pixels differ first at ({}, {}): direct {:?}, recorded {:?}",
            first % width,
            first / width,
            expected.pixel((first % width) as u32, (first / width) as u32),
            actual.pixel((first % width) as u32, (first / width) as u32),
        );
    }

    #[test]
    fn calculator_paint_records_to_the_same_pixels_at_both_scales() {
        for factor in [1u32, 2] {
            let scale = Scale::new(factor).unwrap();
            let list = assert_recording_matches_direct("calculator", 176, 220, scale, |surface| {
                calculator_like(surface, "1234.5")
            });
            assert!(list.len() > 40, "a calculator frame draws many commands");
        }
    }

    #[test]
    fn translucent_colours_blend_the_same_when_recorded() {
        let scale = Scale::new(1).unwrap();
        let translucent_red = Color {
            red: 255,
            green: 0,
            blue: 0,
            alpha: 128,
        };
        let translucent_blue = Color {
            red: 0,
            green: 0,
            blue: 255,
            alpha: 96,
        };
        assert_recording_matches_direct("translucent", 40, 24, scale, |surface| {
            // A translucent fill of the whole surface stands in for a clear: a
            // translucent clear is not recordable, since replay is onto a
            // surface that already has pixels.
            surface.fill_rect(whole(40, 24), translucent_blue)?;
            let block = Rect {
                x: 2,
                y: 2,
                width: 20,
                height: 12,
            };
            surface.fill_rect(block, translucent_red)?;
            // Outlines blend their corners twice, as direct painting does.
            surface.stroke_rect(block, Color::BLACK)?;
            surface.stroke_rect(
                Rect {
                    x: 6,
                    y: 6,
                    width: 30,
                    height: 14,
                },
                translucent_red,
            )
        });
    }

    #[test]
    fn clips_text_images_and_opacity_match_direct_painting() {
        let scale_one = Scale::new(1).unwrap();
        let checker = Image::from_rgba(
            2,
            2,
            vec![
                255, 0, 0, 255, 0, 255, 0, 128, //
                0, 0, 255, 255, 0, 0, 0, 0,
            ],
        )
        .unwrap();
        for factor in [1u32, 2] {
            let scale = Scale::new(factor).unwrap();
            assert_recording_matches_direct("clips and images", 48, 32, scale, |surface| {
                surface.clear(Color::WHITE)?;
                surface.with_clip(
                    Rect {
                        x: 4,
                        y: 4,
                        width: 30,
                        height: 20,
                    },
                    || {
                        surface.draw_text("Slop", 2, 2, &Font::system(12.0), Color::BLACK)?;
                        // Nearest sampling: integer scale, so no bicubic.
                        surface.draw_image_scaled_with_opacity(&checker, 6, 6, 8, 8, 200)?;
                        surface.with_clip(
                            Rect {
                                x: 8,
                                y: 8,
                                width: 40,
                                height: 40,
                            },
                            || surface.draw_image_scaled(&checker, 8, 8, 4, 4),
                        )
                    },
                )?;
                surface.draw_image(&checker, 30, 20)?;
                surface.fill_rect(
                    Rect {
                        x: 20,
                        y: 2,
                        width: 26,
                        height: 26,
                    },
                    Color {
                        red: 10,
                        green: 200,
                        blue: 30,
                        alpha: 150,
                    },
                )
            });
        }
        // Non-integer ratios sample bicubic, which the CPU replay reproduces.
        assert_recording_matches_direct("bicubic", 30, 30, scale_one, |surface| {
            surface.clear(Color::WHITE)?;
            surface.draw_image_scaled(&checker, 1, 1, 13, 11)
        });
    }

    #[test]
    fn edge_geometry_fails_the_same_way_as_direct_painting() {
        let scale = Scale::new(2).unwrap();
        let direct = RasterSurface::new(8, 8, scale).unwrap();
        let recording = RasterSurface::recording(8, 8, scale, LIMIT).unwrap();
        let huge = Rect {
            x: i32::MAX,
            y: 0,
            width: 1,
            height: 1,
        };
        assert!(matches!(
            direct.fill_rect(huge, Color::BLACK),
            Err(RenderError::GeometryOverflow)
        ));
        assert!(matches!(
            recording.fill_rect(huge, Color::BLACK),
            Err(RenderError::GeometryOverflow)
        ));
        assert!(matches!(
            recording.draw_text("x", i32::MAX, 0, &Font::system(12.0), Color::BLACK),
            Err(RenderError::GeometryOverflow)
        ));
        // Nothing was recorded for the failed calls, so the list is still usable.
        assert!(recording.take_recording().unwrap().is_empty());
    }

    #[test]
    fn recording_surfaces_refuse_calls_a_list_cannot_express() {
        let scale = Scale::new(1).unwrap();
        let source = RasterSurface::new(4, 4, scale).unwrap();
        let surface = RasterSurface::recording(8, 8, scale, LIMIT).unwrap();
        // The call returns Ok, as the CPU path does; the failure is latched.
        surface.draw_surface(&source, 0, 0).unwrap();
        assert!(matches!(
            surface.take_recording(),
            Err(RenderError::Recording(_))
        ));
        // The failure stays latched for the rest of the recording.
        surface.fill_rect(whole(4, 4), Color::BLACK).unwrap();
        assert!(matches!(
            surface.take_recording(),
            Err(RenderError::Recording(_))
        ));
    }

    #[test]
    fn translucent_clear_after_drawing_is_a_recording_failure() {
        let scale = Scale::new(1).unwrap();
        let surface = RasterSurface::recording(8, 8, scale, LIMIT).unwrap();
        surface.fill_rect(whole(2, 2), Color::BLACK).unwrap();
        let translucent = Color {
            red: 1,
            green: 2,
            blue: 3,
            alpha: 4,
        };
        surface.clear(translucent).unwrap();
        assert!(matches!(
            surface.take_recording(),
            Err(RenderError::Recording(_))
        ));
    }

    #[test]
    fn clear_inside_a_clip_is_a_recording_failure() {
        let scale = Scale::new(1).unwrap();
        let surface = RasterSurface::recording(8, 8, scale, LIMIT).unwrap();
        surface.push_clip(whole(4, 4)).unwrap();
        surface.clear(Color::WHITE).unwrap();
        surface.pop_clip();
        assert!(surface.take_recording().is_err());
    }

    #[test]
    fn unbalanced_clips_and_full_command_limits_are_reported() {
        let scale = Scale::new(1).unwrap();
        let unbalanced = RasterSurface::recording(8, 8, scale, LIMIT).unwrap();
        unbalanced.pop_clip();
        assert!(matches!(
            unbalanced.take_recording(),
            Err(RenderError::Recording(_))
        ));

        let full = RasterSurface::recording(8, 8, scale, 2).unwrap();
        full.fill_rect(whole(1, 1), Color::BLACK).unwrap();
        full.fill_rect(whole(2, 2), Color::BLACK).unwrap();
        // Over the limit the call still returns Ok; the take reports it.
        full.fill_rect(whole(3, 3), Color::BLACK).unwrap();
        assert!(matches!(
            full.take_recording(),
            Err(RenderError::Recording(_))
        ));
    }

    #[test]
    fn a_recording_surface_has_no_pixels_and_a_list_cannot_replay_onto_it() {
        let scale = Scale::new(2).unwrap();
        let surface = RasterSurface::recording(5, 3, scale, LIMIT).unwrap();
        assert!(surface.is_recording());
        assert_eq!((surface.device_width(), surface.device_height()), (10, 6));
        assert_eq!(surface.pixel(0, 0), None);
        assert!(surface.data().is_empty());
        assert!(surface.encode_png().is_err());

        let plain = RasterSurface::new(5, 3, scale).unwrap();
        assert!(!plain.is_recording());
        assert!(plain.take_recording().is_err());

        surface.fill_rect(whole(1, 1), Color::BLACK).unwrap();
        let list = surface.take_recording().unwrap();
        assert!(list.execute(&surface).is_err());
        list.execute(&plain).unwrap();
        assert_eq!(plain.pixel(0, 0), Some(0xff000000));
    }

    #[cfg(feature = "gpu")]
    #[test]
    fn a_recorded_calculator_frame_matches_direct_painting_on_the_gpu() {
        use crate::render::gpu::WgpuExecutor;
        use std::io::Write;

        const TEST: &str = "a_recorded_calculator_frame_matches_direct_painting_on_the_gpu";
        let mut executor = match WgpuExecutor::new_headless() {
            Ok(executor) => executor,
            Err(error) => {
                // Written to stderr directly: libtest hides eprintln! from
                // passing tests, which would hide the NOT_RUN line.
                let _ = writeln!(std::io::stderr(), "NOT_RUN {TEST}: {error}");
                assert!(
                    std::env::var_os("SLOPOS_REQUIRE_WGPU_ADAPTER").is_none(),
                    "SLOPOS_REQUIRE_WGPU_ADAPTER is set but no adapter exists: {error}"
                );
                return;
            }
        };
        let scale = Scale::new(2).unwrap();
        let (width, height) = (176, 220);
        let direct = RasterSurface::new(width, height, scale).unwrap();
        calculator_like(&direct, "1234.5").unwrap();

        let recording = RasterSurface::recording(width, height, scale, LIMIT).unwrap();
        calculator_like(&recording, "1234.5").unwrap();
        let list = recording.take_recording().unwrap();

        let mut frame = executor
            .create_frame(width, height, scale)
            .expect("gpu frame");
        executor.clear(&mut frame, Color::TRANSPARENT);
        executor.execute(&list, &mut frame).expect("gpu replay");
        let pixels = executor.read_pixels(&frame).expect("gpu readback");
        assert!(
            pixels[..] == direct.data()[..],
            "GPU replay of a recorded calculator frame differs from direct painting"
        );
    }
}
