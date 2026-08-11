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
use std::sync::{Mutex, OnceLock};

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::gles::{ffi, GlesRenderbuffer, GlesRenderer};
use smithay::backend::renderer::utils::draw_render_elements;
use smithay::backend::renderer::{Bind, Color32F, Frame, Offscreen, Renderer};
use smithay::utils::{Buffer as BufferCoord, Physical, Rectangle, Size, Transform};

pub static SHOT_REQUESTED: AtomicBool = AtomicBool::new(false);
static REQUESTED_PATH: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();

const MAX_CAPTURE_DIMENSION: i32 = 16_384;
const MAX_CAPTURE_PIXELS: u64 = 67_108_864; // 8192², 256 MiB at RGBA8.
const GL_ERROR_DRAIN_LIMIT: usize = 16;

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

fn requested_path() -> &'static Mutex<Option<PathBuf>> {
    REQUESTED_PATH.get_or_init(|| Mutex::new(None))
}

/// Request an in-process compositor framebuffer capture for the next frame.
///
/// Portal/session clients use this instead of signalling an arbitrary PID.
/// The path is validated before it is published to the render loop; the
/// existing SIGUSR1 QA path continues to use `SLOPOS_SHOT_PATH`.
pub fn request_capture_to(destination: &Path) -> anyhow::Result<()> {
    let destination = validate_shot_path(destination.to_path_buf())?;
    let mut pending = requested_path()
        .lock()
        .map_err(|_| anyhow::anyhow!("screenshot request state is poisoned"))?;
    if pending.is_some() || SHOT_REQUESTED.load(Ordering::Acquire) {
        anyhow::bail!("a compositor screenshot is already pending");
    }
    *pending = Some(destination);
    SHOT_REQUESTED.store(true, Ordering::Release);
    Ok(())
}

fn shot_path() -> anyhow::Result<PathBuf> {
    if let Some(path) = requested_path()
        .lock()
        .map_err(|_| anyhow::anyhow!("screenshot request state is poisoned"))?
        .take()
    {
        return Ok(path);
    }
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
    let bytes = checked_rgba_byte_len(width as usize, height as usize)?;
    Ok((width as u32, height as u32, bytes))
}

fn checked_rgba_stride(width: usize) -> anyhow::Result<usize> {
    width
        .checked_mul(4)
        .ok_or_else(|| anyhow::anyhow!("RGBA row stride overflow"))
}

fn checked_rgba_byte_len(width: usize, height: usize) -> anyhow::Result<usize> {
    checked_rgba_stride(width)?
        .checked_mul(height)
        .ok_or_else(|| anyhow::anyhow!("RGBA image byte count overflow"))
}

fn is_valid_pack_alignment(value: ffi::types::GLint) -> bool {
    matches!(value, 1 | 2 | 4 | 8)
}

fn checked_aligned_rgba_stride(
    width: usize,
    pack_alignment: ffi::types::GLint,
) -> anyhow::Result<usize> {
    if !is_valid_pack_alignment(pack_alignment) {
        anyhow::bail!("screenshot GL_PACK_ALIGNMENT has invalid value {pack_alignment}");
    }
    let tight_stride = checked_rgba_stride(width)?;
    let alignment = usize::try_from(pack_alignment)?;
    let remainder = tight_stride % alignment;
    let padding = if remainder == 0 {
        0
    } else {
        alignment
            .checked_sub(remainder)
            .ok_or_else(|| anyhow::anyhow!("RGBA row padding overflow"))?
    };
    tight_stride
        .checked_add(padding)
        .ok_or_else(|| anyhow::anyhow!("aligned RGBA row stride overflow"))
}

fn checked_aligned_rgba_byte_len(
    width: usize,
    height: usize,
    pack_alignment: ffi::types::GLint,
) -> anyhow::Result<usize> {
    checked_aligned_rgba_stride(width, pack_alignment)?
        .checked_mul(height)
        .ok_or_else(|| anyhow::anyhow!("aligned RGBA image byte count overflow"))
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

    // Smithay's renderer may leave errors from the preceding target bind or
    // another client operation in the GLES queue. Establish a clean queue
    // before creating this frame so errors generated by this capture are not
    // mislabelled as stale state.
    let stale_errors = renderer
        .with_context(drain_gl_errors)
        .map_err(|error| anyhow::anyhow!("prepare screenshot GL context: {error}"))?;
    if stale_errors.queue_exhausted {
        anyhow::bail!(gl_error_report(
            "pre-render stale-error drain",
            &stale_errors
        ));
    }
    if !stale_errors.errors.is_empty() {
        tracing::warn!(
            errors = ?stale_errors.errors,
            "drained stale OpenGL errors before screenshot render"
        );
    }

    let damage = [Rectangle::from_size(physical)];
    let mut frame = renderer
        .render(&mut framebuffer, physical, Transform::Normal)
        .map_err(|error| anyhow::anyhow!("begin screenshot render: {error}"))?;

    // Keep all frame operations in a result that is evaluated before
    // `finish`. Smithay documents that dropping a frame is not equivalent to
    // finishing it, so even a clear/draw/readback failure must not bypass the
    // explicit finish call below.
    let mut frame_failures = Vec::new();
    if let Err(error) = frame.clear(Color32F::from(clear), &damage) {
        frame_failures.push(format!("clear screenshot frame: {error}"));
    }
    if frame_failures.is_empty() {
        if let Err(error) =
            draw_render_elements::<GlesRenderer, _, _>(&mut frame, scale, elements, &damage)
        {
            frame_failures.push(format!("draw screenshot elements: {error}"));
        }
    }

    // This check is intentionally separate from the readback check. It must
    // report errors generated by clear/draw instead of clearing them as if
    // they were stale state.
    match frame.with_context(drain_gl_errors) {
        Ok(render_errors) if render_errors.queue_exhausted || !render_errors.errors.is_empty() => {
            frame_failures.push(gl_error_report("after clear/draw", &render_errors));
        }
        Ok(_) => {}
        Err(error) => frame_failures.push(format!("check screenshot render errors: {error}")),
    }

    let readback = if frame_failures.is_empty() {
        match frame.with_context(|gl| read_pixels_rgba(gl, width, height, expected_bytes)) {
            Ok(result) => match result {
                Ok(rgba) => Some(rgba),
                Err(error) => {
                    frame_failures.push(format!("read screenshot pixels: {error}"));
                    None
                }
            },
            Err(error) => {
                frame_failures.push(format!("read screenshot pixels context: {error}"));
                None
            }
        }
    } else {
        None
    };

    let finish_result = frame
        .finish()
        .map_err(|error| anyhow::anyhow!("finish screenshot frame: {error}"));
    if let Err(error) = finish_result {
        frame_failures.push(format!("{error:#}"));
    }
    if !frame_failures.is_empty() {
        anyhow::bail!("{}", frame_failures.join("; "));
    }
    let rgba = normalize_readback_rgba(
        readback.ok_or_else(|| anyhow::anyhow!("screenshot readback produced no pixels"))?,
        width as usize,
        height as usize,
    )?;

    let image = image::RgbaImage::from_raw(width_u32, height_u32, rgba)
        .ok_or_else(|| anyhow::anyhow!("RGBA image dimensions do not match readback"))?;
    save_png_atomic(destination, image)?;
    Ok(destination.to_path_buf())
}

#[derive(Debug, Default)]
struct GlErrorDrain {
    errors: Vec<ffi::types::GLenum>,
    queue_exhausted: bool,
}

fn drain_gl_errors(gl: &ffi::Gles2) -> GlErrorDrain {
    let mut errors = Vec::new();
    for _ in 0..GL_ERROR_DRAIN_LIMIT {
        // SAFETY: the caller invokes this only while Smithay's EGL context is
        // current; GetError has no pointer arguments and is safe to poll.
        let error = unsafe { gl.GetError() };
        if error == ffi::NO_ERROR {
            return GlErrorDrain {
                errors,
                queue_exhausted: false,
            };
        }
        errors.push(error);
    }

    // One bounded probe distinguishes a queue that became clean exactly at the
    // limit from a queue that is still producing errors. If the probe is also
    // an error, report the observed values and fail rather than silently
    // masking an unbounded driver error queue.
    // SAFETY: see the GetError call above.
    let probe = unsafe { gl.GetError() };
    if probe == ffi::NO_ERROR {
        GlErrorDrain {
            errors,
            queue_exhausted: false,
        }
    } else {
        errors.push(probe);
        GlErrorDrain {
            errors,
            queue_exhausted: true,
        }
    }
}

fn gl_error_report(stage: &str, drain: &GlErrorDrain) -> String {
    let errors = drain
        .errors
        .iter()
        .map(|error| format!("0x{error:04x}"))
        .collect::<Vec<_>>()
        .join(", ");
    let bound = if drain.queue_exhausted {
        format!(
            "; queue did not reach GL_NO_ERROR within {} + 1 probes",
            GL_ERROR_DRAIN_LIMIT
        )
    } else {
        String::new()
    };
    format!("screenshot {stage}: OpenGL errors [{errors}]{bound}")
}

fn read_pixels_rgba(
    gl: &ffi::Gles2,
    width: i32,
    height: i32,
    tight_expected_bytes: usize,
) -> anyhow::Result<Vec<u8>> {
    let mut pack_alignment = 0;
    // SAFETY: the caller holds the current Smithay EGL context; the
    // destination is a valid, initialized GLint for GetIntegerv.
    unsafe { gl.GetIntegerv(ffi::PACK_ALIGNMENT, &mut pack_alignment) };
    let query_errors = drain_gl_errors(gl);
    if query_errors.queue_exhausted || !query_errors.errors.is_empty() {
        anyhow::bail!(gl_error_report("query PACK_ALIGNMENT", &query_errors));
    }
    if !is_valid_pack_alignment(pack_alignment) {
        anyhow::bail!("screenshot GL_PACK_ALIGNMENT has invalid value {pack_alignment}");
    }

    let width = usize::try_from(width)?;
    let height = usize::try_from(height)?;
    let tight_stride = checked_rgba_stride(width)?;
    let expected_tight = tight_stride
        .checked_mul(height)
        .ok_or_else(|| anyhow::anyhow!("RGBA image byte count overflow"))?;
    if expected_tight != tight_expected_bytes {
        anyhow::bail!("screenshot RGBA allocation size changed during readback");
    }
    let aligned_stride = checked_aligned_rgba_stride(width, pack_alignment)?;
    let aligned_bytes = checked_aligned_rgba_byte_len(width, height, pack_alignment)?;
    let mut rgba = vec![0u8; aligned_bytes];

    // GLES2 direct readback avoids Smithay's GLES3-oriented PBO exporter.
    // ReadPixels itself is synchronous with prior rendering commands.
    // SAFETY: `render` made the renderer's EGL context current and bound this
    // screenshot framebuffer; `with_context` scopes GL access while the frame
    // is alive. The checked dimensions and padded allocation provide at least
    // one complete `aligned_stride` byte row for every GL_PACK_ALIGNMENT
    // padded row, so ReadPixels cannot write beyond `rgba`.
    unsafe {
        gl.ReadPixels(
            0,
            0,
            i32::try_from(width)?,
            i32::try_from(height)?,
            ffi::RGBA,
            ffi::UNSIGNED_BYTE,
            rgba.as_mut_ptr().cast(),
        );
    }
    let readback_errors = drain_gl_errors(gl);
    if readback_errors.queue_exhausted || !readback_errors.errors.is_empty() {
        anyhow::bail!(gl_error_report("after ReadPixels", &readback_errors));
    }
    // Keep the row-stride calculation in this helper's checked path; the
    // normalizer derives and validates the same stride from the exact buffer.
    debug_assert_eq!(aligned_stride.checked_mul(height), Some(rgba.len()));
    Ok(rgba)
}

fn normalize_readback_rgba(bytes: Vec<u8>, width: usize, height: usize) -> anyhow::Result<Vec<u8>> {
    let tight_stride = checked_rgba_stride(width)?;
    let expected = tight_stride
        .checked_mul(height)
        .ok_or_else(|| anyhow::anyhow!("image byte count overflow"))?;
    if height == 0 {
        if bytes.is_empty() {
            return Ok(bytes);
        }
        anyhow::bail!("cannot normalize malformed RGBA buffer");
    }
    let row_stride = bytes
        .len()
        .checked_div(height)
        .ok_or_else(|| anyhow::anyhow!("RGBA row stride overflow"))?;
    if row_stride == 0
        || row_stride.checked_mul(height) != Some(bytes.len())
        || row_stride < tight_stride
    {
        anyhow::bail!("cannot normalize malformed RGBA buffer");
    }
    let mut tight = Vec::new();
    tight
        .try_reserve_exact(expected)
        .map_err(|_| anyhow::anyhow!("normalized RGBA image allocation failed"))?;
    tight.resize(expected, 0);
    for source_row in 0..height {
        let destination_row = height
            .checked_sub(source_row)
            .and_then(|remaining| remaining.checked_sub(1))
            .ok_or_else(|| anyhow::anyhow!("RGBA destination row underflow"))?;
        let source_start = source_row
            .checked_mul(row_stride)
            .ok_or_else(|| anyhow::anyhow!("RGBA source row offset overflow"))?;
        let source_end = source_start
            .checked_add(tight_stride)
            .ok_or_else(|| anyhow::anyhow!("RGBA source row end overflow"))?;
        let destination_start = destination_row
            .checked_mul(tight_stride)
            .ok_or_else(|| anyhow::anyhow!("RGBA destination row offset overflow"))?;
        let destination_end = destination_start
            .checked_add(tight_stride)
            .ok_or_else(|| anyhow::anyhow!("RGBA destination row end overflow"))?;
        let source = bytes
            .get(source_start..source_end)
            .ok_or_else(|| anyhow::anyhow!("RGBA source row exceeds readback buffer"))?;
        let destination = tight
            .get_mut(destination_start..destination_end)
            .ok_or_else(|| anyhow::anyhow!("RGBA destination row exceeds tight buffer"))?;
        destination.copy_from_slice(source);
    }
    if tight.len() != expected {
        anyhow::bail!("normalized RGBA image has unexpected length");
    }
    Ok(tight)
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
        assert!(request_capture_to(Path::new("relative.png")).is_err());
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
    fn rgba_readback_normalization_rejects_malformed_buffer() {
        assert!(normalize_readback_rgba(vec![0u8; 7], 2, 1).is_err());
    }

    #[test]
    fn rgba_readback_normalization_reverses_gl_rows_and_removes_pack_padding() {
        // A 3-pixel RGBA row is 12 bytes. With GL_PACK_ALIGNMENT=8, each row
        // occupies a 16-byte slot, leaving four padding bytes that must not
        // leak into the tight image returned to the PNG encoder. ReadPixels
        // emits GL y=0 first (the bottom row), while PNG rows are top-down.
        let padded = vec![
            0x20, 0xd0, 0xe0, 0xff, // GL y=0 bottom row, pixel 0
            0xc0, 0x21, 0xe1, 0xff, // GL y=0 bottom row, pixel 1
            0xc1, 0xd1, 0x22, 0xff, // GL y=0 bottom row, pixel 2
            0xca, 0xfe, 0xba, 0xbe, // GL y=0 row padding
            0xf0, 0x01, 0x02, 0xff, // GL y=1 top row, pixel 0
            0x10, 0xa0, 0x03, 0xff, // GL y=1 top row, pixel 1
            0x11, 0x12, 0xb0, 0xff, // GL y=1 top row, pixel 2
            0xde, 0xad, 0xbe, 0xef, // GL y=1 row padding
        ];
        assert_eq!(
            normalize_readback_rgba(padded, 3, 2).unwrap(),
            vec![
                0xf0, 0x01, 0x02, 0xff, // PNG top row, pixel 0
                0x10, 0xa0, 0x03, 0xff, // PNG top row, pixel 1
                0x11, 0x12, 0xb0, 0xff, // PNG top row, pixel 2
                0x20, 0xd0, 0xe0, 0xff, // PNG bottom row, pixel 0
                0xc0, 0x21, 0xe1, 0xff, // PNG bottom row, pixel 1
                0xc1, 0xd1, 0x22, 0xff, // PNG bottom row, pixel 2
            ]
        );
    }

    #[test]
    fn pack_alignment_stride_is_checked_for_odd_width() {
        assert_eq!(checked_aligned_rgba_stride(3, 1).unwrap(), 12);
        assert_eq!(checked_aligned_rgba_stride(3, 2).unwrap(), 12);
        assert_eq!(checked_aligned_rgba_stride(3, 4).unwrap(), 12);
        assert_eq!(checked_aligned_rgba_stride(3, 8).unwrap(), 16);
        assert_eq!(checked_aligned_rgba_byte_len(3, 2, 8).unwrap(), 32);
        assert!(checked_aligned_rgba_stride(3, 3).is_err());
    }

    /// This test is intentionally ignored unless explicitly requested. Run it
    /// on Linux with Mesa/EGL available using
    /// `SLOPOS_SCREENSHOT_GLES2_TEST=1 cargo test -p slopos-compositor --lib
    /// gles2_readback_odd_width_channel_order_and_finish -- --ignored`.
    ///
    /// It creates a real surfaceless EGL/GLES2 context and Smithay offscreen
    /// renderbuffer, draws six exact RGBA sentinels, forces GL_PACK_ALIGNMENT=8,
    /// reads the odd-width (3-pixel) FBO with the production helpers, and only
    /// then calls `Frame::finish`. The ignored default prevents a non-EGL host
    /// run from being mistaken for runtime readback evidence.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires SLOPOS_SCREENSHOT_GLES2_TEST=1 and a surfaceless EGL/GLES2 runtime"]
    fn gles2_readback_odd_width_channel_order_and_finish() {
        if std::env::var_os("SLOPOS_SCREENSHOT_GLES2_TEST") != Some(std::ffi::OsString::from("1")) {
            panic!("set SLOPOS_SCREENSHOT_GLES2_TEST=1 to run the EGL/GLES2 readback test");
        }

        use smithay::backend::egl::{native::EGLSurfacelessDisplay, EGLContext, EGLDisplay};

        let display = unsafe { EGLDisplay::new(EGLSurfacelessDisplay) }
            .expect("initialize surfaceless EGL display");
        let context = EGLContext::new(&display).expect("create EGL context");
        let mut renderer = unsafe { GlesRenderer::new(context) }.expect("create GLES2 renderer");
        let width = 3;
        let height = 2;
        let size = Size::<i32, BufferCoord>::from((width, height));
        let expected_bytes = checked_rgba_byte_len(width as usize, height as usize).unwrap();
        let mut target =
            Offscreen::<GlesRenderbuffer>::create_buffer(&mut renderer, Fourcc::Abgr8888, size)
                .expect("create odd-width GLES2 renderbuffer");
        let mut framebuffer = renderer.bind(&mut target).expect("bind screenshot FBO");
        let physical = Size::<i32, Physical>::from((width, height));
        let mut frame = renderer
            .render(&mut framebuffer, physical, Transform::Normal)
            .expect("begin GLES2 frame");
        let damage = [Rectangle::from_size(physical)];
        frame
            .clear(Color32F::from([0.0, 0.0, 0.0, 1.0]), &damage)
            .expect("clear GLES2 sentinel FBO");

        // Smithay's projection maps compositor y=0 to the GL lower edge. The
        // first three sentinels therefore occupy the GL/PNG bottom row; the
        // second three at y=height-1 are the PNG-facing top row.
        let sentinels = [
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0, 1.0],
            [0.0, 1.0, 1.0, 1.0],
            [1.0, 0.0, 1.0, 1.0],
            [1.0, 1.0, 0.0, 1.0],
        ];
        for (index, color) in sentinels.into_iter().enumerate() {
            let x = (index % width as usize) as i32;
            let y = (index / width as usize) as i32;
            let rectangle = Rectangle::new((x, y).into(), (1, 1).into());
            frame
                .draw_solid(rectangle, &damage, Color32F::from(color))
                .expect("draw GLES2 sentinel");
        }

        // Force the renderer's prior state to the widest legal alignment. The
        // 3-pixel RGBA rows are 12 bytes, so GL will pad each row to 16 bytes.
        // The production path must allocate that padded size and leave this
        // shared GL state unchanged for its caller.
        frame
            .with_context(|gl| {
                unsafe { gl.PixelStorei(ffi::PACK_ALIGNMENT, 8) };
                let errors = drain_gl_errors(gl);
                assert!(
                    errors.errors.is_empty(),
                    "set test PACK_ALIGNMENT: {errors:?}"
                );
            })
            .expect("set test PACK_ALIGNMENT");
        let pixels = frame
            .with_context(|gl| read_pixels_rgba(gl, width, height, expected_bytes))
            .expect("run production GLES2 readback helper")
            .expect("read odd-width sentinel FBO");
        let pixels = normalize_readback_rgba(pixels, width as usize, height as usize)
            .expect("normalize odd-width sentinel FBO");
        frame
            .with_context(|gl| {
                let mut restored = 0;
                unsafe { gl.GetIntegerv(ffi::PACK_ALIGNMENT, &mut restored) };
                assert_eq!(restored, 8, "readback must leave PACK_ALIGNMENT unchanged");
                let errors = drain_gl_errors(gl);
                assert!(
                    errors.errors.is_empty(),
                    "query PACK_ALIGNMENT after readback: {errors:?}"
                );
            })
            .expect("query PACK_ALIGNMENT after readback");
        let _render_sync = frame.finish().expect("finish GLES2 frame after readback");

        assert_eq!(
            pixels,
            vec![
                0, 255, 255, 255, 255, 0, 255, 255, 255, 255, 0, 255, // PNG top: GL y=1
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, // PNG bottom: GL y=0
            ],
            "PNG-facing RGBA must place GL y=height-1 before GL y=0"
        );
    }
}
