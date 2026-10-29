//! Platform-independent image framing, adapted from the historical SLOPOS
//! Preview work (archived compositor-era source). No rendering or I/O here.
//! A window host may use these bounds with the CPU or GPU scene adapter.

use crate::foundation::geometry::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoomMode {
    Fit,
    ActualPixels,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageViewport {
    image_width: u32,
    image_height: u32,
    area: Rect,
    mode: ZoomMode,
    custom_zoom: f64,
}

impl ImageViewport {
    pub fn new(image_width: u32, image_height: u32, area: Rect) -> Self {
        Self {
            image_width,
            image_height,
            area,
            mode: ZoomMode::Fit,
            custom_zoom: 1.0,
        }
    }

    pub fn resize(&mut self, area: Rect) {
        self.area = area;
    }

    pub fn mode(&self) -> ZoomMode {
        self.mode
    }

    pub fn fit(&mut self) {
        self.mode = ZoomMode::Fit;
    }

    pub fn actual_pixels(&mut self) {
        self.mode = ZoomMode::ActualPixels;
    }

    /// Set an explicit scale, rejecting NaN/infinity/negative zoom.
    pub fn set_zoom(&mut self, zoom: f64) -> bool {
        if !zoom.is_finite() || zoom <= 0.0 {
            return false;
        }
        self.custom_zoom = zoom.clamp(0.05, 16.0);
        self.mode = ZoomMode::Custom;
        true
    }

    pub fn zoom_by(&mut self, factor: f64) -> bool {
        if !factor.is_finite() || factor <= 0.0 {
            return false;
        }
        self.set_zoom(self.zoom() * factor)
    }

    pub fn zoom(&self) -> f64 {
        if self.image_width == 0 || self.image_height == 0 {
            return 1.0;
        }
        match self.mode {
            ZoomMode::Fit => {
                if self.area.width == 0 || self.area.height == 0 {
                    return 1.0;
                }
                (f64::from(self.area.width) / f64::from(self.image_width))
                    .min(f64::from(self.area.height) / f64::from(self.image_height))
                    .min(1.0)
            }
            ZoomMode::ActualPixels => 1.0,
            ZoomMode::Custom => self.custom_zoom,
        }
    }

    /// Centered destination in logical pixels; the host clips to its window.
    /// Arithmetic saturates safely for unusually large dimensions.
    pub fn image_bounds(&self) -> Option<Rect> {
        if self.image_width == 0
            || self.image_height == 0
            || self.area.width == 0
            || self.area.height == 0
        {
            return None;
        }
        let z = self.zoom();
        let w = (f64::from(self.image_width) * z)
            .round()
            .clamp(1.0, f64::from(u32::MAX)) as u32;
        let h = (f64::from(self.image_height) * z)
            .round()
            .clamp(1.0, f64::from(u32::MAX)) as u32;
        let x = i64::from(self.area.x) + (i64::from(self.area.width) - i64::from(w)) / 2;
        let y = i64::from(self.area.y) + (i64::from(self.area.height) - i64::from(h)) / 2;
        Some(Rect {
            x: x.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
            y: y.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
            width: w,
            height: h,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Rect {
        Rect { x: 10, y: 24, width: 300, height: 200 }
    }

    #[test]
    fn fit_keeps_large_image_inside_the_viewport() {
        let view = ImageViewport::new(1200, 600, area());
        assert_eq!(view.zoom(), 0.25);
        assert_eq!(view.image_bounds(), Some(Rect { x: 10, y: 49, width: 300, height: 150 }));
    }

    #[test]
    fn fit_does_not_upscale_small_images_and_tracks_resize() {
        let mut view = ImageViewport::new(50, 30, area());
        assert_eq!(view.image_bounds(), Some(Rect { x: 135, y: 109, width: 50, height: 30 }));
        view.resize(Rect { x: 0, y: 0, width: 20, height: 20 });
        assert_eq!(view.image_bounds(), Some(Rect { x: 0, y: 4, width: 20, height: 12 }));
    }

    #[test]
    fn custom_zoom_and_actual_pixels_are_explicit() {
        let mut view = ImageViewport::new(100, 50, area());
        assert!(view.set_zoom(2.0));
        assert_eq!(view.mode(), ZoomMode::Custom);
        assert!(view.zoom_by(0.5));
        assert_eq!(view.zoom(), 1.0);
        view.actual_pixels();
        assert_eq!(view.mode(), ZoomMode::ActualPixels);
        view.fit();
        assert_eq!(view.mode(), ZoomMode::Fit);
    }

    #[test]
    fn invalid_zoom_is_rejected_and_extreme_dimensions_are_safe() {
        let mut view = ImageViewport::new(u32::MAX, u32::MAX, area());
        assert!(!view.set_zoom(f64::NAN));
        assert!(!view.set_zoom(-1.0));
        assert!(!view.zoom_by(f64::INFINITY));
        assert!(view.set_zoom(1e100));
        assert_eq!(view.zoom(), 16.0);
        assert!(view.image_bounds().is_some());
    }

    #[test]
    fn empty_image_or_area_never_produces_invalid_bounds() {
        assert_eq!(ImageViewport::new(0, 2, area()).image_bounds(), None);
        assert_eq!(ImageViewport::new(2, 2, Rect { width: 0, ..area() }).image_bounds(), None);
    }
}
