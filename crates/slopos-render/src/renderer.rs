use crate::{primitives::DrawCommand, Color, Result};
use parking_lot::Mutex;
use wgpu::PowerPreference;
use wgpu::{Device, Instance, Queue, Surface as WgpuSurface, SurfaceConfiguration};

/// Surface format / present-mode policy for the wgpu renderer.
///
/// Safe defaults are SDR + stable Fifo (no HDR float formats, no adaptive sync).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DisplayRenderPolicy {
    /// When true, prefer wide-color / HDR surface formats if the adapter advertises them.
    pub hdr_enabled: bool,
    /// When true, prefer adaptive-sync present modes (AutoVsync / FifoRelaxed).
    pub vrr_adaptive: bool,
}

impl DisplayRenderPolicy {
    /// Stable SDR + Fifo — preferred default for shell UI.
    pub const fn sdr_stable() -> Self {
        Self {
            hdr_enabled: false,
            vrr_adaptive: false,
        }
    }
}

/// Choose a surface texture format from adapter capabilities under `policy`.
///
/// - HDR off: prefer common sRGB 8-bit formats; never pick float16 solely because it is available.
/// - HDR on: prefer `Rgba16Float` / `Rgb10a2Unorm` when listed, else fall back to the SDR path.
pub fn select_surface_format(
    formats: &[wgpu::TextureFormat],
    policy: DisplayRenderPolicy,
) -> wgpu::TextureFormat {
    if formats.is_empty() {
        // wgpu guarantees at least one format on a real surface; keep a compile-time fallback
        // so pure unit tests can still exercise empty-list edge cases safely.
        return wgpu::TextureFormat::Bgra8UnormSrgb;
    }

    if policy.hdr_enabled {
        if let Some(fmt) = formats.iter().copied().find(|&f| {
            f == wgpu::TextureFormat::Rgba16Float || f == wgpu::TextureFormat::Rgb10a2Unorm
        }) {
            return fmt;
        }
    }

    select_sdr_surface_format(formats)
}

/// Prefer standard 8-bit sRGB formats; skip HDR float formats when possible.
fn select_sdr_surface_format(formats: &[wgpu::TextureFormat]) -> wgpu::TextureFormat {
    const PREFERRED_SDR: &[wgpu::TextureFormat] = &[
        wgpu::TextureFormat::Bgra8UnormSrgb,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureFormat::Bgra8Unorm,
        wgpu::TextureFormat::Rgba8Unorm,
    ];

    for preferred in PREFERRED_SDR {
        if formats.contains(preferred) {
            return *preferred;
        }
    }

    if let Some(fmt) = formats.iter().copied().find(|f| f.is_srgb()) {
        return fmt;
    }

    // Avoid float16 / 10-bit HDR formats when a non-HDR option exists.
    if let Some(fmt) = formats.iter().copied().find(|&f| !is_hdr_ish_format(f)) {
        return fmt;
    }

    formats[0]
}

fn is_hdr_ish_format(f: wgpu::TextureFormat) -> bool {
    matches!(
        f,
        wgpu::TextureFormat::Rgba16Float
            | wgpu::TextureFormat::Rgb10a2Unorm
            | wgpu::TextureFormat::Rgb9e5Ufloat
            | wgpu::TextureFormat::Rg11b10Float
    )
}

/// Choose a present mode from adapter capabilities under `policy`.
///
/// - VRR adaptive: AutoVsync → FifoRelaxed → Fifo
/// - Stable: Fifo first, then any remaining mode as last resort
pub fn select_present_mode(
    modes: &[wgpu::PresentMode],
    policy: DisplayRenderPolicy,
) -> wgpu::PresentMode {
    let preference: &[wgpu::PresentMode] = if policy.vrr_adaptive {
        &[
            wgpu::PresentMode::AutoVsync,
            wgpu::PresentMode::FifoRelaxed,
            wgpu::PresentMode::Fifo,
        ]
    } else {
        &[wgpu::PresentMode::Fifo]
    };

    for preferred in preference {
        if modes.contains(preferred) {
            return *preferred;
        }
    }

    if modes.contains(&wgpu::PresentMode::Fifo) {
        return wgpu::PresentMode::Fifo;
    }

    modes.first().copied().unwrap_or(wgpu::PresentMode::Fifo)
}

pub struct Renderer {
    pub instance: Instance,
    pub adapter: wgpu::Adapter,
    pub device: Device,
    pub queue: Queue,
    pub config: SurfaceConfiguration,
    draw_commands: Mutex<Vec<DrawCommand>>,
}

impl Renderer {
    /// Create a renderer with safe defaults: HDR off, VRR off → sRGB + Fifo.
    pub async fn new(surface: &WgpuSurface<'static>, width: u32, height: u32) -> Result<Self> {
        Self::new_with_policy(surface, width, height, DisplayRenderPolicy::default()).await
    }

    /// Create a renderer applying the given display render policy for format / present mode.
    pub async fn new_with_policy(
        surface: &WgpuSurface<'static>,
        width: u32,
        height: u32,
        policy: DisplayRenderPolicy,
    ) -> Result<Self> {
        let instance = Instance::new(Default::default());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: PowerPreference::HighPerformance,
                compatible_surface: Some(surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or(crate::RenderError::Surface("no adapter found".into()))?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("RetroRender Device"),
                    required_features: wgpu::Features::default(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            )
            .await?;

        let capabilities = surface.get_capabilities(&adapter);
        let format = select_surface_format(&capabilities.formats, policy);
        let present_mode = select_present_mode(&capabilities.present_modes, policy);

        let config = SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &config);

        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            config,
            draw_commands: Mutex::new(Vec::new()),
        })
    }

    pub fn resize(&mut self, surface: &WgpuSurface<'static>, width: u32, height: u32) {
        self.config.width = width;
        self.config.height = height;
        surface.configure(&self.device, &self.config);
    }

    pub fn begin_frame(&self, surface: &WgpuSurface<'static>) -> Option<wgpu::SurfaceTexture> {
        surface.get_current_texture().ok()
    }

    pub fn clear(&self, view: &wgpu::TextureView, color: Color) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("clear encoder"),
            });
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("clear pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: color.r as f64,
                        g: color.g as f64,
                        b: color.b as f64,
                        a: color.a as f64,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        self.queue.submit(Some(encoder.finish()));
    }

    pub fn present(frame: wgpu::SurfaceTexture) {
        frame.present();
    }

    pub fn enqueue(&self, command: DrawCommand) {
        self.draw_commands.lock().push(command);
    }

    pub fn drain_commands(&self) -> Vec<DrawCommand> {
        self.draw_commands.lock().drain(..).collect()
    }

    pub fn queued_command_count(&self) -> usize {
        self.draw_commands.lock().len()
    }

    pub fn device(&self) -> &Device {
        &self.device
    }
    pub fn queue(&self) -> &Queue {
        &self.queue
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::{PresentMode, TextureFormat};

    #[test]
    fn default_policy_is_sdr_stable() {
        let p = DisplayRenderPolicy::default();
        assert!(!p.hdr_enabled);
        assert!(!p.vrr_adaptive);
        assert_eq!(p, DisplayRenderPolicy::sdr_stable());
    }

    #[test]
    fn sdr_prefers_bgra8_srgb_over_hdr_formats() {
        let formats = [
            TextureFormat::Rgba16Float,
            TextureFormat::Rgb10a2Unorm,
            TextureFormat::Bgra8UnormSrgb,
            TextureFormat::Rgba8UnormSrgb,
        ];
        let fmt = select_surface_format(&formats, DisplayRenderPolicy::default());
        assert_eq!(fmt, TextureFormat::Bgra8UnormSrgb);
    }

    #[test]
    fn sdr_never_picks_float16_when_srgb_exists() {
        let formats = [TextureFormat::Rgba16Float, TextureFormat::Rgba8UnormSrgb];
        let fmt = select_surface_format(&formats, DisplayRenderPolicy::sdr_stable());
        assert_eq!(fmt, TextureFormat::Rgba8UnormSrgb);
    }

    #[test]
    fn hdr_prefers_rgba16_float_when_available() {
        let formats = [
            TextureFormat::Bgra8UnormSrgb,
            TextureFormat::Rgba16Float,
            TextureFormat::Rgb10a2Unorm,
        ];
        let policy = DisplayRenderPolicy {
            hdr_enabled: true,
            vrr_adaptive: false,
        };
        let fmt = select_surface_format(&formats, policy);
        assert_eq!(fmt, TextureFormat::Rgba16Float);
    }

    #[test]
    fn hdr_falls_back_to_rgb10a2_then_sdr() {
        let formats = [TextureFormat::Bgra8UnormSrgb, TextureFormat::Rgb10a2Unorm];
        let policy = DisplayRenderPolicy {
            hdr_enabled: true,
            vrr_adaptive: false,
        };
        assert_eq!(
            select_surface_format(&formats, policy),
            TextureFormat::Rgb10a2Unorm
        );

        let sdr_only = [TextureFormat::Rgba8UnormSrgb, TextureFormat::Rgba8Unorm];
        assert_eq!(
            select_surface_format(&sdr_only, policy),
            TextureFormat::Rgba8UnormSrgb
        );
    }

    #[test]
    fn stable_vrr_prefers_fifo() {
        let modes = [
            PresentMode::AutoVsync,
            PresentMode::Mailbox,
            PresentMode::FifoRelaxed,
            PresentMode::Fifo,
        ];
        let mode = select_present_mode(&modes, DisplayRenderPolicy::default());
        assert_eq!(mode, PresentMode::Fifo);
    }

    #[test]
    fn adaptive_vrr_prefers_auto_vsync_then_fifo_relaxed() {
        let modes = [
            PresentMode::Mailbox,
            PresentMode::FifoRelaxed,
            PresentMode::Fifo,
            PresentMode::AutoVsync,
        ];
        let policy = DisplayRenderPolicy {
            hdr_enabled: false,
            vrr_adaptive: true,
        };
        assert_eq!(select_present_mode(&modes, policy), PresentMode::AutoVsync);

        let no_auto = [
            PresentMode::Mailbox,
            PresentMode::FifoRelaxed,
            PresentMode::Fifo,
        ];
        assert_eq!(
            select_present_mode(&no_auto, policy),
            PresentMode::FifoRelaxed
        );

        let fifo_only = [PresentMode::Mailbox, PresentMode::Fifo];
        assert_eq!(select_present_mode(&fifo_only, policy), PresentMode::Fifo);
    }

    #[test]
    fn adaptive_does_not_prefer_mailbox() {
        // Mailbox is low-latency tearing-capable; policy prefers AutoVsync/FifoRelaxed/Fifo only.
        let modes = [PresentMode::Mailbox, PresentMode::Fifo];
        let policy = DisplayRenderPolicy {
            hdr_enabled: false,
            vrr_adaptive: true,
        };
        assert_eq!(select_present_mode(&modes, policy), PresentMode::Fifo);
    }
}
