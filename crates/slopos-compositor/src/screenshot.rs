//! SIGUSR1-triggered compositor framebuffer screenshots for QA.
//!
//! Capture is performed inside the compositor because DRM scanout cannot be
//! safely read from a second process while the compositor owns the device.
//! Requests are signal-safe, dimensions are bounded before GPU allocation, and
//! PNG output is committed atomically with private file permissions.

use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::gles::{GlesRenderbuffer, GlesRenderer};
use smithay::backend::renderer::utils::draw_render_elements;
use smithay::backend::renderer::{
    Bind, Color32F, ExportMem, Frame, Offscreen, Renderer, TextureMapping,
};
use smithay::utils::{Buffer as BufferCoord, Physical, Rectangle, Size, Transform};

pub static SHOT_REQUESTED: AtomicBool = AtomicBool::new(false);

const MAX_CAPTURE_DIMENSION: i32 = 16_384;
const MAX_CAPTURE_PIXELS: u64 = 67_108_864; // 8192², 256 MiB at RGBA8.

extern "C" fn on_sigusr1(_signal: libc::c_int) {
    SHOT_REQUESTED.store(true, Ordering::SeqCst);
}

/// Install a SIGUSR1 request handler without changing unrelated signal state.
///
/// Failure is logged rather than fatal because screenshots are a QA facility,
/// not part of frame presentation.
pub fn install_signal_handler() {
    let result = unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = on_sigusr1 as *const () as usize;
        action.sa_flags = libc::SA_RESTART;
        libc::sigemptyset(&mut action.sa_mask);
        if libc::sigaction(libc::SIGUSR1, &action, std::ptr::null_mut()) == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    };
    if let Err(error) = result {
        tracing::warn!(error = %error, "could not install compositor screenshot signal handler");
    }
}

fn validate_shot_path(path: PathBuf) -> anyhow::Result<PathBuf> {
    if !path.is_absolute() {
        anyhow::bail!(
            "SLOPOS_SHOT_PATH must be absolute so runtime evidence cannot be redirected by cwd: {}",
            path.display()
        );
    }
    Ok(path)
}

fn shot_path() -> anyhow::Result<PathBuf> {
    let path = std::env::var_os("SLOPOS_SHOT_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/slopos-i-shot.png"));
    validate_shot_path(path)
}

/// Capture on the next rendered frame when SIGUSR1 requested it.
pub fn capture_if_requested(
    renderer: &mut GlesRenderer,
    elements: &[WaylandSurfaceRenderElement<GlesRenderer>],
    size: (i32, i32),
    scale: f64,
    clear: [f32; 4],
) -> Option<PathBuf> {
    if !SHOT_REQUESTED.swap(false, Ordering::SeqCst) {
        return None;
    }
    let path = match shot_path() {
        Ok(path) => path,
        Err(error) => {
            tracing::warn!(error = %error, "screenshot path rejected");
            eprintln!("[slopos-compositor] screenshot failed: {error:#}");
            return None;
        }
    };
    match capture_to_path(renderer, elements, size, scale, clear, &path) {
        Ok(path) => {
            tracing::info!(path = %path.display(), "screenshot written");
            eprintln!("[slopos-compositor] screenshot written: {}", path.display());
            Some(path)
        }
        Err(error) => {
            tracing::warn!(error = %error, "screenshot failed");
            eprintln!("[slopos-compositor] screenshot failed: {error:#}");
            None
        }
    }
}

fn validate_capture_size(width: i32, height: i32) -> anyhow::Result<(u32, u32, usize)> {
    if !(1..=MAX_CAPTURE_DIMENSION).contains(&width)
        || !(1..=MAX_CAPTURE_DIMENSION).contains(&height)
    {
        anyhow::bail!(
            "capture dimensions {width}x{height} are outside 1..={MAX_CAPTURE_DIMENSION}"
        );
    }
    let pixels = u64::try_from(width)?
        .checked_mul(u64::try_from(height)?)
        .ok_or_else(|| anyhow::anyhow!("capture pixel count overflow"))?;
    if pixels > MAX_CAPTURE_PIXELS {
        anyhow::bail!("capture dimensions {width}x{height} exceed {MAX_CAPTURE_PIXELS} pixels");
    }
    let bytes = pixels
        .checked_mul(4)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| anyhow::anyhow!("capture byte count overflow"))?;
    Ok((width as u32, height as u32, bytes))
}

fn validate_render_scale(scale: f64) -> anyhow::Result<f64> {
    if !scale.is_finite() || !(0.01..=64.0).contains(&scale) {
        anyhow::bail!("render scale {scale} is outside the finite range 0.01..=64.0");
    }
    Ok(scale)
}

/// Render real compositor elements into a bounded offscreen PNG.
///
/// The caller supplies the destination so product surfaces such as
/// compositor-owned Spaces thumbnails can use the same readback path as the
/// SIGUSR1 QA capture without sharing a global filename.
pub fn capture_to_path(
    renderer: &mut GlesRenderer,
    elements: &[WaylandSurfaceRenderElement<GlesRenderer>],
    (width, height): (i32, i32),
    scale: f64,
    clear: [f32; 4],
    destination: &Path,
) -> anyhow::Result<PathBuf> {
    let (width_u32, height_u32, expected_bytes) = validate_capture_size(width, height)?;
    let scale = validate_render_scale(scale)?;
    let physical: Size<i32, Physical> = Size::from((width, height));
    let buffer: Size<i32, BufferCoord> = Size::from((width, height));

    let mut target: GlesRenderbuffer =
        Offscreen::<GlesRenderbuffer>::create_buffer(renderer, Fourcc::Abgr8888, buffer)
            .map_err(|error| anyhow::anyhow!("create screenshot buffer: {error}"))?;
    let mut framebuffer = renderer
        .bind(&mut target)
        .map_err(|error| anyhow::anyhow!("bind screenshot buffer: {error}"))?;

    let damage = [Rectangle::from_size(physical)];
    {
        let mut frame = renderer
            .render(&mut framebuffer, physical, Transform::Normal)
            .map_err(|error| anyhow::anyhow!("begin screenshot render: {error}"))?;
        frame
            .clear(Color32F::from(clear), &damage)
            .map_err(|error| anyhow::anyhow!("clear screenshot frame: {error}"))?;
        draw_render_elements::<GlesRenderer, _, _>(&mut frame, scale, elements, &damage)
            .map_err(|error| anyhow::anyhow!("draw screenshot elements: {error}"))?;
        let _render_sync = frame
            .finish()
            .map_err(|error| anyhow::anyhow!("finish screenshot frame: {error}"))?;
    }

    // Mesa accepts BGRA readback for Argb8888 here. Little-endian Argb8888 is
    // [B, G, R, A] in memory; PNG expects [R, G, B, A]. copy_framebuffer is
    // issued after finish on the same renderer/context, so command ordering is
    // preserved even when the returned synchronization point has no CPU wait.
    let mapping = renderer
        .copy_framebuffer(&framebuffer, Rectangle::from_size(buffer), Fourcc::Argb8888)
        .map_err(|error| anyhow::anyhow!("copy screenshot framebuffer: {error}"))?;
    let flipped = mapping.flipped();
    let pixels = renderer
        .map_texture(&mapping)
        .map_err(|error| anyhow::anyhow!("map screenshot pixels: {error}"))?;
    if pixels.len() != expected_bytes {
        anyhow::bail!(
            "screenshot readback size mismatch: expected {expected_bytes}, got {}",
            pixels.len()
        );
    }

    let mut rgba = pixels.to_vec();
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    if flipped {
        flip_rows_in_place(&mut rgba, width as usize, height as usize)?;
    }

    let image = image::RgbaImage::from_raw(width_u32, height_u32, rgba)
        .ok_or_else(|| anyhow::anyhow!("RGBA image dimensions do not match readback"))?;
    save_png_atomic(destination, image)?;
    Ok(destination.to_path_buf())
}

fn flip_rows_in_place(bytes: &mut [u8], width: usize, height: usize) -> anyhow::Result<()> {
    let stride = width
        .checked_mul(4)
        .ok_or_else(|| anyhow::anyhow!("row stride overflow"))?;
    let expected = stride
        .checked_mul(height)
        .ok_or_else(|| anyhow::anyhow!("image byte count overflow"))?;
    if bytes.len() != expected {
        anyhow::bail!("cannot flip malformed RGBA buffer");
    }
    let mut scratch = vec![0u8; stride];
    for row in 0..height / 2 {
        let opposite = height - 1 - row;
        let top = row * stride;
        let bottom = opposite * stride;
        scratch.copy_from_slice(&bytes[top..top + stride]);
        bytes.copy_within(bottom..bottom + stride, top);
        bytes[bottom..bottom + stride].copy_from_slice(&scratch);
    }
    Ok(())
}

fn save_png_atomic(destination: &Path, image: image::RgbaImage) -> anyhow::Result<()> {
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if !parent.is_dir() {
        anyhow::bail!(
            "screenshot parent directory does not exist: {}",
            parent.display()
        );
    }
    let filename = destination
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("screenshot path has no filename"))?
        .to_string_lossy();
    let temporary = parent.join(format!(
        ".{filename}.tmp-{}-{}",
        std::process::id(),
        monotonic_nonce()
    ));

    let result = (|| -> anyhow::Result<()> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        let mut writer = BufWriter::new(file);
        image::DynamicImage::ImageRgba8(image).write_to(&mut writer, image::ImageFormat::Png)?;
        writer.flush()?;
        writer.get_ref().sync_all()?;
        drop(writer);
        fs::rename(&temporary, destination)?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn monotonic_nonce() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_destination_must_be_absolute() {
        assert!(validate_shot_path(PathBuf::from("relative.png")).is_err());
        assert_eq!(
            validate_shot_path(PathBuf::from("/tmp/slopos-capture.png")).unwrap(),
            PathBuf::from("/tmp/slopos-capture.png")
        );
    }

    #[test]
    fn capture_dimensions_are_bounded_before_allocation() {
        assert_eq!(
            validate_capture_size(1920, 1080).unwrap().2,
            1920 * 1080 * 4
        );
        assert!(validate_capture_size(0, 1080).is_err());
        assert!(validate_capture_size(-1, 1080).is_err());
        assert!(validate_capture_size(MAX_CAPTURE_DIMENSION + 1, 1).is_err());
        assert!(validate_capture_size(16_384, 16_384).is_err());
    }

    #[test]
    fn render_scale_accepts_supported_integer_and_fractional_values() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            assert_eq!(validate_render_scale(scale).unwrap(), scale);
        }
        assert!(validate_render_scale(0.0).is_err());
        assert!(validate_render_scale(f64::NAN).is_err());
        assert!(validate_render_scale(f64::INFINITY).is_err());
    }

    #[test]
    fn row_flip_is_exact_and_in_place() {
        let mut pixels = vec![
            1, 0, 0, 255, 2, 0, 0, 255, 3, 0, 0, 255, 4, 0, 0, 255, 5, 0, 0, 255, 6, 0, 0, 255,
        ];
        flip_rows_in_place(&mut pixels, 2, 3).unwrap();
        assert_eq!(
            pixels,
            vec![
                5, 0, 0, 255, 6, 0, 0, 255, 3, 0, 0, 255, 4, 0, 0, 255, 1, 0, 0, 255, 2, 0, 0, 255,
            ]
        );
    }

    #[test]
    fn malformed_row_buffer_is_rejected() {
        assert!(flip_rows_in_place(&mut [0u8; 7], 2, 1).is_err());
    }
}
