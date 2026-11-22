//! Damage tracking for frames sent to a window.
//!
//! This module has no X11 types, so it is tested without a display. A
//! [`Presenter`] keeps the bytes the window's server-side copy is known to
//! hold. For each new frame it returns the smallest device-pixel rectangle that
//! differs from those bytes, and the host uploads only that rectangle. A frame
//! identical to the last one sends nothing at all.
//!
//! What the presenter tracks is the server's copy, not the app's. After an
//! Expose, a resize or a remap the server may have discarded the window's
//! pixels, so the host calls [`Presenter::invalidate`] and the next frame is
//! sent whole.
//!
//! Granularity: one bounding rectangle per frame. It is found by comparing the
//! frame with a copy held here, so each present costs one pass over the pixels
//! and one frame-sized copy per window (the copy is reused). Reporting several
//! rectangles, or taking damage from the toolkit instead of comparing, are
//! follow-ups.

/// Premultiplied RGBA8 pixels at device size, row-major and unpadded: the
/// layout of [`crate::render::RasterSurface::data`] and of GPU readback.
#[derive(Debug, Clone, Copy)]
pub struct Frame<'a> {
    width: u32,
    height: u32,
    pixels: &'a [u8],
}

impl<'a> Frame<'a> {
    /// `None` when a dimension is zero, the size overflows, or `pixels` is not
    /// exactly `width × height × 4` bytes.
    pub fn new(width: u32, height: u32, pixels: &'a [u8]) -> Option<Self> {
        let expected = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(4)?;
        (width > 0 && height > 0 && pixels.len() == expected).then_some(Self {
            width,
            height,
            pixels,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels(&self) -> &'a [u8] {
        self.pixels
    }
}

/// A half-open rectangle in device pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl DeviceRect {
    /// The whole of a `width × height` device frame.
    pub fn full(width: u32, height: u32) -> Self {
        Self {
            x: 0,
            y: 0,
            width,
            height,
        }
    }
}

/// Tracks what one window's server-side copy holds.
#[derive(Debug, Default)]
pub struct Presenter {
    /// The bytes the server is known to hold, valid only when `valid`.
    sent: Vec<u8>,
    width: u32,
    height: u32,
    /// False until the first frame, and after [`Presenter::invalidate`].
    valid: bool,
}

impl Presenter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Forget what the server holds: the next frame is sent whole. Call this
    /// when the server may have lost the window's pixels (Expose, remap).
    pub fn invalidate(&mut self) {
        self.valid = false;
    }

    /// Record `frame` as the window's new contents and return the device
    /// rectangle that must be uploaded, or `None` when nothing changed.
    ///
    /// The returned rectangle is the bounding box of every differing pixel.
    /// Pixels outside it already match the server's copy, so uploading only
    /// the rectangle leaves the window showing `frame`.
    pub fn damage(&mut self, frame: &Frame) -> Option<DeviceRect> {
        let (width, height) = (frame.width(), frame.height());
        let stride = width as usize * 4;
        if !self.valid || self.width != width || self.height != height {
            self.sent.clear();
            self.sent.extend_from_slice(frame.pixels());
            self.width = width;
            self.height = height;
            self.valid = true;
            return Some(DeviceRect::full(width, height));
        }

        let mut bounds: Option<(u32, u32, u32, u32)> = None;
        let rows = frame
            .pixels()
            .chunks_exact(stride)
            .zip(self.sent.chunks_exact_mut(stride));
        for (row, (new_row, sent_row)) in rows.enumerate() {
            if new_row == &*sent_row {
                continue;
            }
            let row = row as u32;
            let mut first = None;
            let mut last = 0;
            for (column, (new, old)) in new_row
                .as_chunks::<4>()
                .0
                .iter()
                .zip(sent_row.as_chunks::<4>().0)
                .enumerate()
            {
                if new != old {
                    first.get_or_insert(column as u32);
                    last = column as u32;
                }
            }
            // The rows differ, so at least one pixel does.
            let first = first.unwrap_or(0);
            sent_row.copy_from_slice(new_row);
            bounds = Some(match bounds {
                None => (first, row, last, row),
                Some((left, top, right, bottom)) => {
                    (left.min(first), top, right.max(last), bottom.max(row))
                }
            });
        }
        bounds.map(|(left, top, right, bottom)| DeviceRect {
            x: left,
            y: top,
            width: right - left + 1,
            height: bottom - top + 1,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `width × height` opaque frame filled with one colour.
    fn solid(width: u32, height: u32, value: u8) -> Vec<u8> {
        vec![value; width as usize * height as usize * 4]
    }

    fn set_pixel(pixels: &mut [u8], width: u32, x: u32, y: u32, value: u8) {
        let at = (y as usize * width as usize + x as usize) * 4;
        pixels[at..at + 4].fill(value);
    }

    #[test]
    fn malformed_frames_are_refused() {
        assert!(Frame::new(0, 1, &[]).is_none());
        assert!(Frame::new(1, 0, &[]).is_none());
        assert!(Frame::new(2, 2, &[0; 15]).is_none());
        assert!(Frame::new(2, 2, &[0; 17]).is_none());
        assert!(Frame::new(u32::MAX, u32::MAX, &[]).is_none());
        let pixels = [0; 16];
        let frame = Frame::new(2, 2, &pixels).unwrap();
        assert_eq!((frame.width(), frame.height()), (2, 2));
    }

    #[test]
    fn the_first_frame_is_sent_whole() {
        let pixels = solid(5, 3, 7);
        let mut presenter = Presenter::new();
        let frame = Frame::new(5, 3, &pixels).unwrap();
        assert_eq!(presenter.damage(&frame), Some(DeviceRect::full(5, 3)));
    }

    #[test]
    fn an_unchanged_frame_sends_nothing() {
        let pixels = solid(4, 4, 9);
        let mut presenter = Presenter::new();
        let frame = Frame::new(4, 4, &pixels).unwrap();
        presenter.damage(&frame);
        assert_eq!(presenter.damage(&frame), None);
        assert_eq!(presenter.damage(&frame), None);
    }

    #[test]
    fn one_changed_pixel_is_a_one_pixel_rectangle() {
        let mut pixels = solid(8, 6, 1);
        let mut presenter = Presenter::new();
        presenter.damage(&Frame::new(8, 6, &pixels).unwrap());
        set_pixel(&mut pixels, 8, 5, 2, 200);
        let damage = presenter.damage(&Frame::new(8, 6, &pixels).unwrap());
        assert_eq!(
            damage,
            Some(DeviceRect {
                x: 5,
                y: 2,
                width: 1,
                height: 1
            })
        );
    }

    #[test]
    fn a_change_in_the_alpha_byte_alone_is_damage() {
        let mut pixels = solid(3, 3, 50);
        let mut presenter = Presenter::new();
        presenter.damage(&Frame::new(3, 3, &pixels).unwrap());
        pixels[(3 + 2) * 4 + 3] ^= 0x80;
        let damage = presenter.damage(&Frame::new(3, 3, &pixels).unwrap());
        assert_eq!(
            damage,
            Some(DeviceRect {
                x: 2,
                y: 1,
                width: 1,
                height: 1
            })
        );
    }

    #[test]
    fn changes_far_apart_give_their_bounding_box() {
        let mut pixels = solid(10, 10, 0);
        let mut presenter = Presenter::new();
        presenter.damage(&Frame::new(10, 10, &pixels).unwrap());
        set_pixel(&mut pixels, 10, 1, 8, 1);
        set_pixel(&mut pixels, 10, 7, 2, 1);
        set_pixel(&mut pixels, 10, 4, 5, 1);
        let damage = presenter.damage(&Frame::new(10, 10, &pixels).unwrap());
        assert_eq!(
            damage,
            Some(DeviceRect {
                x: 1,
                y: 2,
                width: 7,
                height: 7
            })
        );
    }

    #[test]
    fn the_last_pixel_of_the_last_row_is_reported() {
        let mut pixels = solid(4, 3, 0);
        let mut presenter = Presenter::new();
        presenter.damage(&Frame::new(4, 3, &pixels).unwrap());
        set_pixel(&mut pixels, 4, 3, 2, 9);
        assert_eq!(
            presenter.damage(&Frame::new(4, 3, &pixels).unwrap()),
            Some(DeviceRect {
                x: 3,
                y: 2,
                width: 1,
                height: 1
            })
        );
    }

    #[test]
    fn the_stored_copy_follows_each_frame() {
        // A change that is reported once must not be reported again.
        let mut pixels = solid(4, 4, 0);
        let mut presenter = Presenter::new();
        presenter.damage(&Frame::new(4, 4, &pixels).unwrap());
        set_pixel(&mut pixels, 4, 0, 0, 5);
        assert!(presenter
            .damage(&Frame::new(4, 4, &pixels).unwrap())
            .is_some());
        assert_eq!(presenter.damage(&Frame::new(4, 4, &pixels).unwrap()), None);
        set_pixel(&mut pixels, 4, 0, 0, 0);
        assert_eq!(
            presenter.damage(&Frame::new(4, 4, &pixels).unwrap()),
            Some(DeviceRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1
            })
        );
    }

    #[test]
    fn invalidation_resends_the_whole_frame_once() {
        let pixels = solid(6, 2, 3);
        let mut presenter = Presenter::new();
        let frame = Frame::new(6, 2, &pixels).unwrap();
        presenter.damage(&frame);
        presenter.invalidate();
        assert_eq!(presenter.damage(&frame), Some(DeviceRect::full(6, 2)));
        assert_eq!(presenter.damage(&frame), None);
    }

    #[test]
    fn a_size_change_resends_the_whole_frame() {
        let mut presenter = Presenter::new();
        presenter.damage(&Frame::new(4, 4, &solid(4, 4, 1)).unwrap());
        // Same byte count, different shape: must not be compared row by row.
        let reshaped = solid(8, 2, 1);
        assert_eq!(
            presenter.damage(&Frame::new(8, 2, &reshaped).unwrap()),
            Some(DeviceRect::full(8, 2))
        );
    }
}
