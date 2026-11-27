//! wgpu executor for [`DrawList`] (phase B, behind the `gpu` feature).
//!
//! [`WgpuExecutor`] implements [`DrawExecutor`] on a headless wgpu device. It
//! has no window, surface or X11 dependency, and it draws into offscreen
//! premultiplied RGBA8 textures laid out like [`RasterSurface::data`]. The
//! CPU path in [`super::list`] stays the reference; the parity suite at the
//! bottom of this file renders the same lists through both and records every
//! pixel that differs (see "Differences from the reference" below).
//!
//! How each command is executed:
//! - The list is first *recorded* on the CPU ([`record`], which needs no
//!   device). Recording turns every command into device-pixel quads, in order,
//!   and tags each quad with a scissor rectangle. The scissor is the active
//!   clip stack, already intersected and clamped the way
//!   [`RasterSurface::push_clip`] does it, so clipping is exact.
//! - `FillRect`, `RoundedRect` and `StrokeRect` use the reference's own
//!   geometry: rounded corners are the spans from [`rounded_spans`], and
//!   outlines are the four edges from [`stroke_edge_rects`]. Each quad is
//!   drawn as a solid premultiplied colour.
//! - `TextRun` is **rasterised on the CPU**. The glyphs come from the existing
//!   [`crate::text`] code, are composited into a premultiplied layer with the
//!   reference's straight-over blend, and then the layer is uploaded as an
//!   image and drawn 1:1. The GPU does not shape or rasterise glyphs.
//! - `ImageBlit` uploads the image as a premultiplied texture. An integer
//!   scale samples nearest. Any other scale samples bicubic, using tiny-skia's
//!   highp weights in the fragment shader. `opacity` multiplies the texel.
//! - Blending is premultiplied source-over (`One`, `OneMinusSrcAlpha`), the
//!   same operator as the reference. Rectangle edges are integer device
//!   coordinates, so no anti-aliasing is involved.
//!
//! Everything here is portable. Nothing depends on X11, Openbox, GTK or a
//! display handle, so the executor runs on a headless machine.
//!
//! # Differences from the reference
//!
//! Coordinates are device pixels. Values are premultiplied RGBA8 bytes, shown
//! as `cpu -> gpu`. The numbers come from llvmpipe 25.2.8 (Mesa, Vulkan,
//! software). The parity test pins this exact list. On another adapter it
//! fails until that adapter's differences are reviewed and added here.
//!
//! Exact on this adapter (zero bytes differ): every fill, rounded rectangle,
//! outline, clip and nested clip, the clear, text runs, nearest-sampled images
//! (integer scale), image opacity, and the bicubic image `image_bicubic_1x`.
//! The text runs are exact because the CPU rasterises the glyphs and the GPU
//! composites the same bytes.
//!
//! Not exact: the bicubic sampler differs by ±1 in one channel at the pixels
//! below. These are the only differences in the suite.
//!
//! - `image_gradient_bicubic_2x`: 37 bytes in 25 device pixels, largest delta 1.
//!   - (3,2): g 62 -> 61
//!   - (6,2): r 170 -> 171
//!   - (3,3): r 251 -> 250, g 62 -> 61
//!   - (4,3): r 229 -> 228, b 78 -> 77
//!   - (5,3): g 166 -> 165, b 107 -> 106
//!   - (7,3): b 170 -> 171
//!   - (6,4): r 168 -> 169, g 215 -> 216
//!   - (7,4): r 172 -> 173, g 246 -> 247
//!   - (8,4): b 205 -> 204
//!   - (9,4): r 230 -> 231
//!   - (10,4): r 252 -> 251, g 255 -> 254, b 252 -> 251
//!   - (3,5): r 181 -> 182
//!   - (4,5): r 176 -> 175, b 80 -> 79
//!   - (6,5): r 165 -> 166
//!   - (7,5): r 177 -> 176
//!   - (9,5): g 251 -> 250
//!   - (3,6): r 130 -> 129
//!   - (4,6): r 137 -> 136, g 91 -> 90
//!   - (8,6): r 209 -> 208
//!   - (3,7): g 59 -> 60
//!   - (4,7): b 82 -> 83
//!   - (9,7): r 237 -> 236, g 241 -> 240
//!   - (10,7): r 252 -> 253, b 252 -> 253
//!   - (5,8): b 119 -> 120
//!   - (4,9): g 82 -> 83, b 83 -> 84
//! - `image_gradient_upscale_1x`: 7 bytes in 6 device pixels, largest delta 1.
//!   - (1,0): r 204 -> 205
//!   - (3,1): b 211 -> 212
//!   - (2,2): r 149 -> 150, g 173 -> 174
//!   - (1,3): b 58 -> 59
//!   - (2,3): b 153 -> 152
//!   - (3,3): b 220 -> 221
//!
//! The likely cause is rounding. The shader sums the 16 weighted taps in f32
//! and rounds once on output, but tiny-skia's highp pipeline rounds at its own
//! stages. The weights are tiny-skia's. This has not been traced further.
//!
//! # Limitations
//!
//! - Text is rasterised on the CPU, so it does not reduce CPU work.
//! - Each execute uploads every image it uses. There is no texture cache yet.
//! - wgpu's default error handler panics on a validation error. Errors are
//!   not yet returned through a result.
//! - Only the Vulkan and GLES backends are compiled in.

use std::sync::mpsc;

use futures::executor::block_on;
use tiny_skia::Pixmap;

use super::adapter::{select_nvidia_vulkan, Backend, Candidate, Kind};
use super::list::{
    rounded_spans, stroke_edge_rects, DrawCommand, DrawExecutor, DrawList, ImageHandle, ListError,
};
use super::{blend_straight_over, RenderError};
use crate::foundation::{
    color::Color,
    geometry::{Rect, Scale},
};
use crate::image::Image;
use crate::text::{self, Font};

/// Premultiplied RGBA8, the same layout as the reference surface.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// Bytes per vertex: position (2), texel coordinate (2), tint (4), all `f32`.
const VERTEX_STRIDE: u64 = 32;
const VERTEX_ATTRIBUTES: &[wgpu::VertexAttribute] = &[
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 8,
        shader_location: 1,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x4,
        offset: 16,
        shader_location: 2,
    },
];

const SHADER: &str = r#"
struct Screen {
    size: vec2<f32>,
    pad: vec2<f32>,
};

@group(0) @binding(0) var<uniform> screen: Screen;
@group(1) @binding(0) var layer: texture_2d<f32>;

struct VertexIn {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) tint: vec4<f32>,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) tint: vec4<f32>,
};

@vertex
fn vs_main(input: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.clip = vec4<f32>(
        input.pos.x / screen.size.x * 2.0 - 1.0,
        1.0 - input.pos.y / screen.size.y * 2.0,
        0.0,
        1.0,
    );
    out.uv = input.uv;
    out.tint = input.tint;
    return out;
}

// Solid fills carry their premultiplied colour in `tint`.
@fragment
fn fs_solid(input: VertexOut) -> @location(0) vec4<f32> {
    return input.tint;
}

// Texel lookup at the pixel centre's image-space coordinate, clamped to the
// image: the reference's nearest sampling with pad spread.
@fragment
fn fs_nearest(input: VertexOut) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(layer));
    let texel = clamp(floor(input.uv), vec2<f32>(0.0, 0.0), dims - vec2<f32>(1.0, 1.0));
    return textureLoad(layer, vec2<i32>(texel), 0) * input.tint.x;
}

// tiny-skia's highp bicubic weights (Mitchell-Netravali, B = C = 1/3).
fn bicubic_near(t: f32) -> f32 {
    return ((-21.0 / 18.0 * t + 27.0 / 18.0) * t + 9.0 / 18.0) * t + 1.0 / 18.0;
}

fn bicubic_far(t: f32) -> f32 {
    return (t * t) * (7.0 / 18.0 * t - 6.0 / 18.0);
}

@fragment
fn fs_bicubic(input: VertexOut) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(layer));
    let fx = fract(input.uv.x + 0.5);
    let fy = fract(input.uv.y + 0.5);
    let wx = array<f32, 4>(
        bicubic_far(1.0 - fx), bicubic_near(1.0 - fx), bicubic_near(fx), bicubic_far(fx));
    let wy = array<f32, 4>(
        bicubic_far(1.0 - fy), bicubic_near(1.0 - fy), bicubic_near(fy), bicubic_far(fy));
    var sum = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    for (var j = 0; j < 4; j = j + 1) {
        for (var i = 0; i < 4; i = i + 1) {
            let at = vec2<f32>(
                input.uv.x - 1.5 + f32(i),
                input.uv.y - 1.5 + f32(j),
            );
            let texel = clamp(floor(at), vec2<f32>(0.0, 0.0), dims - vec2<f32>(1.0, 1.0));
            sum = sum + wx[i] * wy[j] * textureLoad(layer, vec2<i32>(texel), 0);
        }
    }
    let alpha = clamp(sum.a, 0.0, 1.0);
    let rgb = clamp(sum.rgb, vec3<f32>(0.0, 0.0, 0.0), vec3<f32>(alpha, alpha, alpha));
    return vec4<f32>(rgb, alpha) * input.tint.x;
}
"#;

/// A device-pixel rectangle that the GPU scissors to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Scissor {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Which fragment shader draws a quad.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Sampling {
    Solid,
    Nearest,
    Bicubic,
}

/// A run of consecutive vertices drawn with one pipeline and one scissor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Draw {
    pub sampling: Sampling,
    /// Index into [`Recording::layers`]; `None` for solid quads.
    pub layer: Option<usize>,
    pub scissor: Scissor,
    pub first: u32,
    pub count: u32,
}

/// A premultiplied RGBA8 image uploaded for one frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Layer {
    pub width: u32,
    pub height: u32,
    pub premultiplied: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Vertex {
    /// Device pixels.
    pub pos: [f32; 2],
    /// Layer texel coordinates. Unused for solid quads.
    pub uv: [f32; 2],
    /// Solid: premultiplied colour. Textured: `[opacity, 0, 0, 0]`.
    pub tint: [f32; 4],
}

/// The GPU-ready form of a draw list, built without a device.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct Recording {
    pub vertices: Vec<Vertex>,
    pub draws: Vec<Draw>,
    pub layers: Vec<Layer>,
}

/// Turn `list` into quads for a frame of `device_width × device_height` pixels
/// at `scale`. Returns the commands recorded before any error, so the caller
/// can draw them as the reference does ("commands before the failure stay
/// drawn").
pub(crate) fn record(
    list: &DrawList,
    scale: Scale,
    device_width: u32,
    device_height: u32,
) -> (Recording, Option<ListError>) {
    let mut recorder = Recorder::new(scale, device_width, device_height);
    if let Err(error) = check_balanced(list) {
        return (recorder.rec, Some(error));
    }
    for command in list.commands() {
        if let Err(error) = recorder.command(list, command) {
            return (recorder.rec, Some(error));
        }
    }
    (recorder.rec, None)
}

fn check_balanced(list: &DrawList) -> Result<(), ListError> {
    let mut depth = 0usize;
    for command in list.commands() {
        match command {
            DrawCommand::PushClip { .. } => depth += 1,
            DrawCommand::PopClip => {
                depth = depth.checked_sub(1).ok_or(ListError::UnbalancedClip)?;
            }
            _ => {}
        }
    }
    if depth == 0 {
        Ok(())
    } else {
        Err(ListError::UnbalancedClip)
    }
}

fn to_device(rect: Rect, scale: Scale) -> Result<Rect, ListError> {
    rect.to_device(scale)
        .ok_or(ListError::Render(RenderError::GeometryOverflow))
}

/// Device-space clip box: (left, top, right, bottom), as in the reference.
type ClipBox = (i64, i64, i64, i64);

struct Recorder {
    scale: Scale,
    full: ClipBox,
    clips: Vec<ClipBox>,
    rec: Recording,
    /// Layer index for each image handle, uploaded on first use.
    image_layers: Vec<Option<usize>>,
}

impl Recorder {
    fn new(scale: Scale, device_width: u32, device_height: u32) -> Self {
        Self {
            scale,
            full: (0, 0, i64::from(device_width), i64::from(device_height)),
            clips: Vec::new(),
            rec: Recording::default(),
            image_layers: Vec::new(),
        }
    }

    fn clip(&self) -> ClipBox {
        self.clips.last().copied().unwrap_or(self.full)
    }

    fn scissor(&self) -> Option<Scissor> {
        let (left, top, right, bottom) = self.clip();
        if right <= left || bottom <= top {
            return None;
        }
        Some(Scissor {
            x: left as u32,
            y: top as u32,
            width: (right - left) as u32,
            height: (bottom - top) as u32,
        })
    }

    fn command(&mut self, list: &DrawList, command: &DrawCommand) -> Result<(), ListError> {
        match command {
            DrawCommand::FillRect { rect, color } => {
                let device = to_device(*rect, self.scale)?;
                self.solid(device, *color);
            }
            DrawCommand::RoundedRect {
                rect,
                radius,
                color,
            } => {
                for span in rounded_spans(*rect, *radius, self.scale)? {
                    self.solid(span, *color);
                }
            }
            DrawCommand::StrokeRect { rect, color } => {
                for edge in stroke_edge_rects(*rect)? {
                    let device = to_device(edge, self.scale)?;
                    self.solid(device, *color);
                }
            }
            DrawCommand::TextRun {
                x,
                y,
                text,
                font,
                color,
            } => self.text(*x, *y, text, font, *color)?,
            DrawCommand::ImageBlit {
                image,
                rect,
                opacity,
            } => self.image(list, *image, *rect, *opacity)?,
            DrawCommand::PushClip { rect } => {
                let device = to_device(*rect, self.scale)?;
                let (left, top, right, bottom) = self.clip();
                let (x0, y0) = (i64::from(device.x), i64::from(device.y));
                let (x1, y1) = (x0 + i64::from(device.width), y0 + i64::from(device.height));
                self.clips.push((
                    x0.clamp(left, right),
                    y0.clamp(top, bottom),
                    x1.clamp(left, right),
                    y1.clamp(top, bottom),
                ));
            }
            DrawCommand::PopClip => {
                self.clips.pop();
            }
        }
        Ok(())
    }

    fn solid(&mut self, device: Rect, color: Color) {
        if device.width == 0 || device.height == 0 || color.alpha == 0 {
            return;
        }
        let Some(scissor) = self.scissor() else {
            return;
        };
        let alpha = f32::from(color.alpha) / 255.0;
        let tint = [
            f32::from(color.red) / 255.0 * alpha,
            f32::from(color.green) / 255.0 * alpha,
            f32::from(color.blue) / 255.0 * alpha,
            alpha,
        ];
        self.quad(device, Sampling::Solid, None, scissor, [0.0, 0.0], tint);
    }

    /// Rasterise glyphs on the CPU into a premultiplied layer and draw it 1:1.
    fn text(
        &mut self,
        x: i32,
        y: i32,
        content: &str,
        font: &Font,
        color: Color,
    ) -> Result<(), ListError> {
        let overflow = || ListError::Render(RenderError::GeometryOverflow);
        let factor = self.scale.factor();
        let origin_x = x.checked_mul(factor as i32).ok_or_else(overflow)?;
        let origin_y = y.checked_mul(factor as i32).ok_or_else(overflow)?;
        let mut plots = Vec::new();
        text::rasterize(content, font, factor as f32, color, |px, py, coverage| {
            plots.push((px, py, coverage));
        });
        let (Some(min_x), Some(min_y)) = (
            plots.iter().map(|plot| plot.0).min(),
            plots.iter().map(|plot| plot.1).min(),
        ) else {
            return Ok(());
        };
        let max_x = plots.iter().map(|plot| plot.0).max().unwrap_or(min_x);
        let max_y = plots.iter().map(|plot| plot.1).max().unwrap_or(min_y);
        let width = (max_x - min_x + 1) as u32;
        let height = (max_y - min_y + 1) as u32;
        let mut layer = Pixmap::new(width, height).ok_or_else(overflow)?;
        {
            let data = layer.data_mut();
            for (px, py, coverage) in plots {
                let offset = (((py - min_y) as usize) * width as usize + (px - min_x) as usize) * 4;
                blend_straight_over(&mut data[offset..offset + 4], coverage);
            }
        }
        let device = Rect {
            x: origin_x.checked_add(min_x).ok_or_else(overflow)?,
            y: origin_y.checked_add(min_y).ok_or_else(overflow)?,
            width,
            height,
        };
        let Some(scissor) = self.scissor() else {
            return Ok(());
        };
        let layer = self.push_layer(Layer {
            width,
            height,
            premultiplied: layer.data().to_vec(),
        });
        self.quad(
            device,
            Sampling::Nearest,
            Some(layer),
            scissor,
            [width as f32, height as f32],
            [1.0, 0.0, 0.0, 0.0],
        );
        Ok(())
    }

    fn image(
        &mut self,
        list: &DrawList,
        handle: ImageHandle,
        rect: Rect,
        opacity: u8,
    ) -> Result<(), ListError> {
        let image = list.image(handle)?;
        if rect.width == 0 || rect.height == 0 || opacity == 0 || image.pixmap().is_none() {
            return Ok(());
        }
        // The same f32 arithmetic as the reference's transform, so the
        // nearest-or-bicubic choice matches it exactly.
        let factor = self.scale.factor() as f32;
        let sx = rect.width as f32 * factor / image.width() as f32;
        let sy = rect.height as f32 * factor / image.height() as f32;
        let sampling = if sx.fract() == 0.0 && sy.fract() == 0.0 {
            Sampling::Nearest
        } else {
            Sampling::Bicubic
        };
        let device = to_device(rect, self.scale)?;
        let Some(scissor) = self.scissor() else {
            return Ok(());
        };
        let layer = self.image_layer(handle, image);
        self.quad(
            device,
            sampling,
            Some(layer),
            scissor,
            [image.width() as f32, image.height() as f32],
            [f32::from(opacity) / 255.0, 0.0, 0.0, 0.0],
        );
        Ok(())
    }

    fn image_layer(&mut self, handle: ImageHandle, image: &Image) -> usize {
        let slot = handle.index();
        if self.image_layers.len() <= slot {
            self.image_layers.resize(slot + 1, None);
        }
        if let Some(index) = self.image_layers[slot] {
            return index;
        }
        let pixmap = image.pixmap().map(|pixmap| pixmap.data().to_vec());
        let index = self.push_layer(Layer {
            width: image.width(),
            height: image.height(),
            premultiplied: pixmap.unwrap_or_default(),
        });
        self.image_layers[slot] = Some(index);
        index
    }

    fn push_layer(&mut self, layer: Layer) -> usize {
        self.rec.layers.push(layer);
        self.rec.layers.len() - 1
    }

    /// Append a device rectangle as two triangles. `uv_extent` is the texel
    /// size the rectangle maps onto, with the top-left corner at (0, 0).
    fn quad(
        &mut self,
        device: Rect,
        sampling: Sampling,
        layer: Option<usize>,
        scissor: Scissor,
        uv_extent: [f32; 2],
        tint: [f32; 4],
    ) {
        let (x0, y0) = (device.x as f32, device.y as f32);
        let (x1, y1) = (x0 + device.width as f32, y0 + device.height as f32);
        let [u1, v1] = uv_extent;
        let corner = |pos: [f32; 2], uv: [f32; 2]| Vertex { pos, uv, tint };
        let top_left = corner([x0, y0], [0.0, 0.0]);
        let top_right = corner([x1, y0], [u1, 0.0]);
        let bottom_left = corner([x0, y1], [0.0, v1]);
        let bottom_right = corner([x1, y1], [u1, v1]);

        let first = self.rec.vertices.len() as u32;
        self.rec.vertices.extend([
            top_left,
            bottom_left,
            top_right,
            top_right,
            bottom_left,
            bottom_right,
        ]);
        if let Some(last) = self.rec.draws.last_mut() {
            let contiguous = last.first + last.count == first;
            if contiguous
                && last.sampling == sampling
                && last.layer == layer
                && last.scissor == scissor
            {
                last.count += 6;
                return;
            }
        }
        self.rec.draws.push(Draw {
            sampling,
            layer,
            scissor,
            first,
            count: 6,
        });
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GpuError {
    #[error("no wgpu adapter is available: {0}")]
    NoAdapter(String),
    #[error("wgpu could not create a device: {0}")]
    Device(String),
    #[error("could not read the frame back from the GPU: {0}")]
    Readback(String),
    #[error(transparent)]
    Render(#[from] RenderError),
    #[error(transparent)]
    List(#[from] ListError),
}

/// An offscreen frame: a premultiplied RGBA8 texture at a logical size and
/// scale, like [`RasterSurface`].
pub struct GpuFrame {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    scale: Scale,
    device_width: u32,
    device_height: u32,
}

impl GpuFrame {
    pub fn scale(&self) -> Scale {
        self.scale
    }

    pub fn device_width(&self) -> u32 {
        self.device_width
    }

    pub fn device_height(&self) -> u32 {
        self.device_height
    }
}

/// Replays draw lists on a headless wgpu device. Build one with
/// [`WgpuExecutor::new_headless`], which fails with [`GpuError::NoAdapter`]
/// on a machine that has no usable GPU driver.
pub struct WgpuExecutor {
    device: wgpu::Device,
    queue: wgpu::Queue,
    info: wgpu::AdapterInfo,
    uniform_layout: wgpu::BindGroupLayout,
    layer_layout: wgpu::BindGroupLayout,
    solid: wgpu::RenderPipeline,
    nearest: wgpu::RenderPipeline,
    bicubic: wgpu::RenderPipeline,
}

impl WgpuExecutor {
    /// Request an adapter and device with no window or display handle.
    ///
    /// An NVIDIA Vulkan adapter is preferred when one exists (see
    /// [`choose_adapter`]). Otherwise the generic request is used, as before.
    pub fn new_headless() -> Result<Self, GpuError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = choose_adapter(&instance)?;
        let info = adapter.get_info();
        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("slopos-ui headless"),
            required_features: wgpu::Features::empty(),
            required_limits: adapter.limits(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
        }))
        .map_err(|error| GpuError::Device(error.to_string()))?;

        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("slopos screen"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let layer_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("slopos layer"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        // Solid quads read only the screen uniform; textured quads also bind
        // a layer. Separate layouts keep each pipeline's bind requirements
        // exact.
        let solid_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("slopos solid"),
            bind_group_layouts: &[Some(&uniform_layout)],
            immediate_size: 0,
        });
        let textured_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("slopos textured"),
            bind_group_layouts: &[Some(&uniform_layout), Some(&layer_layout)],
            immediate_size: 0,
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("slopos draw shaders"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let solid = pipeline(&device, &solid_layout, &module, "fs_solid");
        let nearest = pipeline(&device, &textured_layout, &module, "fs_nearest");
        let bicubic = pipeline(&device, &textured_layout, &module, "fs_bicubic");

        Ok(Self {
            device,
            queue,
            info,
            uniform_layout,
            layer_layout,
            solid,
            nearest,
            bicubic,
        })
    }

    /// The adapter that was selected, for the parity report.
    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.info
    }

    /// Create a transparent frame of `width × height` logical pixels.
    pub fn create_frame(
        &self,
        width: u32,
        height: u32,
        scale: Scale,
    ) -> Result<GpuFrame, GpuError> {
        if width == 0 || height == 0 {
            return Err(RenderError::InvalidDimensions.into());
        }
        let device_rect = Rect {
            x: 0,
            y: 0,
            width,
            height,
        }
        .to_device(scale)
        .ok_or(RenderError::GeometryOverflow)?;
        let max = self.device.limits().max_texture_dimension_2d;
        if device_rect.width > max || device_rect.height > max {
            return Err(RenderError::GeometryOverflow.into());
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("slopos frame"),
            size: wgpu::Extent3d {
                width: device_rect.width,
                height: device_rect.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Ok(GpuFrame {
            texture,
            view,
            scale,
            device_width: device_rect.width,
            device_height: device_rect.height,
        })
    }

    /// Replace every pixel of `frame`, as [`RasterSurface::clear`] does.
    /// Start `frame` from `pixels`: premultiplied RGBA, row-major, with no
    /// padding, the layout of `RasterSurface::data`. A frame that continues a
    /// surface's painting must start from that surface's pixels, not from
    /// transparent.
    pub fn load(&self, frame: &GpuFrame, pixels: &[u8]) -> Result<(), GpuError> {
        let (width, height) = (frame.device_width, frame.device_height);
        let expected = width as usize * height as usize * 4;
        if pixels.len() != expected {
            return Err(RenderError::InvalidDimensions.into());
        }
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &frame.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        Ok(())
    }

    pub fn clear(&mut self, frame: &mut GpuFrame, color: Color) {
        let alpha = f64::from(color.alpha) / 255.0;
        let value = wgpu::Color {
            r: f64::from(color.red) / 255.0 * alpha,
            g: f64::from(color.green) / 255.0 * alpha,
            b: f64::from(color.blue) / 255.0 * alpha,
            a: alpha,
        };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("slopos clear"),
            });
        drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("slopos clear pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &frame.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(value),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        }));
        self.queue.submit(Some(encoder.finish()));
    }

    /// Premultiplied RGBA8 bytes, row-major and unpadded, in the same layout
    /// as [`RasterSurface::data`].
    pub fn read_pixels(&self, frame: &GpuFrame) -> Result<Vec<u8>, GpuError> {
        let (width, height) = (frame.device_width, frame.device_height);
        let row = width * 4;
        // Buffer copies need rows padded to 256 bytes.
        let padded = row.div_ceil(256) * 256;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("slopos readback"),
            size: u64::from(padded) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("slopos readback"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &frame.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));

        let slice = buffer.slice(..);
        let (sender, receiver) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|error| GpuError::Readback(error.to_string()))?;
        receiver
            .recv()
            .map_err(|error| GpuError::Readback(error.to_string()))?
            .map_err(|error| GpuError::Readback(format!("{error:?}")))?;

        let mapped = slice
            .get_mapped_range()
            .map_err(|error| GpuError::Readback(format!("{error:?}")))?;
        let mut out = Vec::with_capacity(row as usize * height as usize);
        for y in 0..height {
            let start = (y * padded) as usize;
            out.extend_from_slice(&mapped[start..start + row as usize]);
        }
        drop(mapped);
        buffer.unmap();
        Ok(out)
    }

    fn draw(&self, frame: &GpuFrame, recording: &Recording) {
        if recording.draws.is_empty() {
            return;
        }
        let screen = [
            frame.device_width as f32,
            frame.device_height as f32,
            0.0,
            0.0,
        ];
        let uniform = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("slopos screen size"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue
            .write_buffer(&uniform, 0, &floats_to_bytes(&screen));
        let uniform_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("slopos screen size"),
            layout: &self.uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });

        let vertex_bytes = vertices_to_bytes(&recording.vertices);
        let vertices = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("slopos vertices"),
            size: vertex_bytes.len() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&vertices, 0, &vertex_bytes);

        let mut layer_textures = Vec::with_capacity(recording.layers.len());
        let mut layer_groups = Vec::with_capacity(recording.layers.len());
        for layer in &recording.layers {
            let size = wgpu::Extent3d {
                width: layer.width,
                height: layer.height,
                depth_or_array_layers: 1,
            };
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("slopos layer"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &layer.premultiplied,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(layer.width * 4),
                    rows_per_image: Some(layer.height),
                },
                size,
            );
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            layer_groups.push(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("slopos layer"),
                layout: &self.layer_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                }],
            }));
            layer_textures.push((texture, view));
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("slopos draw list"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("slopos draw list pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &frame.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &uniform_group, &[]);
            pass.set_vertex_buffer(0, vertices.slice(..));
            for draw in &recording.draws {
                let scissor = draw.scissor;
                pass.set_scissor_rect(scissor.x, scissor.y, scissor.width, scissor.height);
                match draw.sampling {
                    Sampling::Solid => pass.set_pipeline(&self.solid),
                    Sampling::Nearest | Sampling::Bicubic => {
                        let pipeline = if draw.sampling == Sampling::Nearest {
                            &self.nearest
                        } else {
                            &self.bicubic
                        };
                        pass.set_pipeline(pipeline);
                        if let Some(index) = draw.layer {
                            pass.set_bind_group(1, &layer_groups[index], &[]);
                        }
                    }
                }
                pass.draw(draw.first..draw.first + draw.count, 0..1);
            }
        }
        self.queue.submit(Some(encoder.finish()));
    }
}

impl DrawExecutor for WgpuExecutor {
    type Frame = GpuFrame;
    type Error = GpuError;

    fn execute(&mut self, list: &DrawList, frame: &mut GpuFrame) -> Result<(), GpuError> {
        let (recording, error) = record(list, frame.scale, frame.device_width, frame.device_height);
        // A layer texture larger than the device limit makes wgpu panic, and the
        // window could then not fall back to the CPU path. Refuse it before drawing.
        let max = self.device.limits().max_texture_dimension_2d;
        if recording
            .layers
            .iter()
            .any(|layer| layer.width > max || layer.height > max)
        {
            return Err(RenderError::GeometryOverflow.into());
        }
        self.draw(frame, &recording);
        match error {
            None => Ok(()),
            Some(error) => Err(GpuError::List(error)),
        }
    }
}

/// The adapter to draw on.
///
/// The Vulkan adapters are listed first. When one is from NVIDIA (the
/// proprietary driver or NVK), it is chosen by [`select_nvidia_vulkan`]. The
/// proprietary driver is still an ordinary Vulkan device here: the same
/// pipelines and readback run on it. Without such an adapter, the request is
/// the one SLOPOS has always made, so machines without NVIDIA are unchanged.
fn choose_adapter(instance: &wgpu::Instance) -> Result<wgpu::Adapter, GpuError> {
    let mut adapters = block_on(instance.enumerate_adapters(wgpu::Backends::VULKAN));
    let candidates: Vec<Candidate> = adapters
        .iter()
        .map(|adapter| candidate(&adapter.get_info()))
        .collect();
    if let Some(index) = select_nvidia_vulkan(&candidates) {
        let adapter = adapters.swap_remove(index);
        log::info!(
            "wgpu: using the NVIDIA Vulkan adapter {}",
            adapter.get_info().name
        );
        return Ok(adapter);
    }
    block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
        apply_limit_buckets: false,
    }))
    .map_err(|error| GpuError::NoAdapter(error.to_string()))
}

/// The fields of an adapter that the selection policy reads.
fn candidate(info: &wgpu::AdapterInfo) -> Candidate {
    Candidate {
        vendor: info.vendor,
        kind: match info.device_type {
            wgpu::DeviceType::DiscreteGpu => Kind::Discrete,
            wgpu::DeviceType::IntegratedGpu => Kind::Integrated,
            wgpu::DeviceType::VirtualGpu => Kind::Virtual,
            wgpu::DeviceType::Cpu => Kind::Cpu,
            wgpu::DeviceType::Other => Kind::Other,
        },
        backend: match info.backend {
            wgpu::Backend::Vulkan => Backend::Vulkan,
            wgpu::Backend::Gl => Backend::Gl,
            _ => Backend::Other,
        },
    }
}

fn pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    module: &wgpu::ShaderModule,
    fragment: &str,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(fragment),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: VERTEX_STRIDE,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: VERTEX_ATTRIBUTES,
            })],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(fragment),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: FORMAT,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn floats_to_bytes(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn vertices_to_bytes(vertices: &[Vertex]) -> Vec<u8> {
    let mut out = Vec::with_capacity(vertices.len() * VERTEX_STRIDE as usize);
    for vertex in vertices {
        for value in vertex.pos.iter().chain(&vertex.uv).chain(&vertex.tint) {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::list::parity_fixtures::{scenarios, Scenario};
    use crate::render::list::CpuExecutor;
    use crate::render::RasterSurface;
    use std::io::Write;

    fn rect(x: i32, y: i32, width: u32, height: u32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    fn scale(factor: u32) -> Scale {
        Scale::new(factor).unwrap()
    }

    // The next four tests need no device: the recording is plain data.

    #[test]
    fn recording_scissors_follow_the_clip_stack() {
        let mut list = DrawList::new(16);
        for command in [
            DrawCommand::PushClip {
                rect: rect(2, 2, 6, 6),
            },
            DrawCommand::PushClip {
                rect: rect(4, 0, 20, 20),
            },
            DrawCommand::FillRect {
                rect: rect(0, 0, 12, 10),
                color: Color::BLACK,
            },
            DrawCommand::PopClip,
            DrawCommand::FillRect {
                rect: rect(0, 0, 12, 10),
                color: Color::BLACK,
            },
            DrawCommand::PopClip,
        ] {
            list.push(command).unwrap();
        }
        let (recording, error) = record(&list, scale(1), 12, 10);
        assert!(error.is_none());
        assert_eq!(recording.draws.len(), 2);
        // Nested clips: (2,2)-(8,8) intersected with (4,0)-(24,20), clamped to
        // the outer clip, gives (4,2)-(8,8).
        assert_eq!(
            recording.draws[0].scissor,
            Scissor {
                x: 4,
                y: 2,
                width: 4,
                height: 6
            }
        );
        // After the inner pop, only the outer clip (2,2)-(8,8) applies.
        assert_eq!(
            recording.draws[1].scissor,
            Scissor {
                x: 2,
                y: 2,
                width: 6,
                height: 6
            }
        );
    }

    #[test]
    fn unbalanced_clip_records_nothing() {
        let mut list = DrawList::new(4);
        list.push(DrawCommand::FillRect {
            rect: rect(0, 0, 2, 2),
            color: Color::BLACK,
        })
        .unwrap();
        list.push(DrawCommand::PushClip {
            rect: rect(0, 0, 2, 2),
        })
        .unwrap();
        let (recording, error) = record(&list, scale(1), 4, 4);
        assert!(matches!(error, Some(ListError::UnbalancedClip)));
        assert!(recording.draws.is_empty() && recording.vertices.is_empty());
    }

    #[test]
    fn geometry_error_keeps_the_commands_before_it() {
        let mut list = DrawList::new(8);
        list.push(DrawCommand::FillRect {
            rect: rect(0, 0, 1, 1),
            color: Color::BLACK,
        })
        .unwrap();
        list.push(DrawCommand::FillRect {
            rect: rect(i32::MAX, 0, 1, 1),
            color: Color::BLACK,
        })
        .unwrap();
        list.push(DrawCommand::FillRect {
            rect: rect(1, 0, 1, 1),
            color: Color::BLACK,
        })
        .unwrap();
        let (recording, error) = record(&list, scale(2), 4, 4);
        assert!(matches!(
            error,
            Some(ListError::Render(RenderError::GeometryOverflow))
        ));
        assert_eq!(recording.draws.len(), 1);
    }

    #[test]
    fn rounded_spans_paint_each_device_pixel_at_most_once() {
        for (width, height, radius, factor) in [(8, 6, 2, 1), (16, 12, 2, 2), (6, 4, 100, 1)] {
            let spans = rounded_spans(rect(0, 0, width, height), radius, scale(factor)).unwrap();
            let (device_w, device_h) = (width * factor, height * factor);
            let mut hits = vec![0u32; (device_w * device_h) as usize];
            for span in spans {
                for y in span.y.max(0)..(span.y + span.height as i32).min(device_h as i32) {
                    for x in span.x.max(0)..(span.x + span.width as i32).min(device_w as i32) {
                        hits[(y as u32 * device_w + x as u32) as usize] += 1;
                    }
                }
            }
            assert!(hits.iter().all(|&count| count <= 1), "overlapping spans");
        }
    }

    /// One byte of GPU output that differs from the CPU reference.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Difference {
        scenario: &'static str,
        x: u32,
        y: u32,
        channel: &'static str,
        cpu: u8,
        gpu: u8,
    }

    /// Every byte that differs from the CPU reference, on the adapter recorded
    /// in the module docs. Scenarios not listed match exactly. A different
    /// adapter may round differently, so this test fails there until its
    /// differences are reviewed and recorded here and in the module docs.
    const EXPECTED_DIFFERENCES: &[(&str, u32, u32, &str, u8, u8)] = &[
        ("image_gradient_bicubic_2x", 3, 2, "g", 62, 61),
        ("image_gradient_bicubic_2x", 6, 2, "r", 170, 171),
        ("image_gradient_bicubic_2x", 3, 3, "r", 251, 250),
        ("image_gradient_bicubic_2x", 3, 3, "g", 62, 61),
        ("image_gradient_bicubic_2x", 4, 3, "r", 229, 228),
        ("image_gradient_bicubic_2x", 4, 3, "b", 78, 77),
        ("image_gradient_bicubic_2x", 5, 3, "g", 166, 165),
        ("image_gradient_bicubic_2x", 5, 3, "b", 107, 106),
        ("image_gradient_bicubic_2x", 7, 3, "b", 170, 171),
        ("image_gradient_bicubic_2x", 6, 4, "r", 168, 169),
        ("image_gradient_bicubic_2x", 6, 4, "g", 215, 216),
        ("image_gradient_bicubic_2x", 7, 4, "r", 172, 173),
        ("image_gradient_bicubic_2x", 7, 4, "g", 246, 247),
        ("image_gradient_bicubic_2x", 8, 4, "b", 205, 204),
        ("image_gradient_bicubic_2x", 9, 4, "r", 230, 231),
        ("image_gradient_bicubic_2x", 10, 4, "r", 252, 251),
        ("image_gradient_bicubic_2x", 10, 4, "g", 255, 254),
        ("image_gradient_bicubic_2x", 10, 4, "b", 252, 251),
        ("image_gradient_bicubic_2x", 3, 5, "r", 181, 182),
        ("image_gradient_bicubic_2x", 4, 5, "r", 176, 175),
        ("image_gradient_bicubic_2x", 4, 5, "b", 80, 79),
        ("image_gradient_bicubic_2x", 6, 5, "r", 165, 166),
        ("image_gradient_bicubic_2x", 7, 5, "r", 177, 176),
        ("image_gradient_bicubic_2x", 9, 5, "g", 251, 250),
        ("image_gradient_bicubic_2x", 3, 6, "r", 130, 129),
        ("image_gradient_bicubic_2x", 4, 6, "r", 137, 136),
        ("image_gradient_bicubic_2x", 4, 6, "g", 91, 90),
        ("image_gradient_bicubic_2x", 8, 6, "r", 209, 208),
        ("image_gradient_bicubic_2x", 3, 7, "g", 59, 60),
        ("image_gradient_bicubic_2x", 4, 7, "b", 82, 83),
        ("image_gradient_bicubic_2x", 9, 7, "r", 237, 236),
        ("image_gradient_bicubic_2x", 9, 7, "g", 241, 240),
        ("image_gradient_bicubic_2x", 10, 7, "r", 252, 253),
        ("image_gradient_bicubic_2x", 10, 7, "b", 252, 253),
        ("image_gradient_bicubic_2x", 5, 8, "b", 119, 120),
        ("image_gradient_bicubic_2x", 4, 9, "g", 82, 83),
        ("image_gradient_bicubic_2x", 4, 9, "b", 83, 84),
        ("image_gradient_upscale_1x", 1, 0, "r", 204, 205),
        ("image_gradient_upscale_1x", 3, 1, "b", 211, 212),
        ("image_gradient_upscale_1x", 2, 2, "r", 149, 150),
        ("image_gradient_upscale_1x", 2, 2, "g", 173, 174),
        ("image_gradient_upscale_1x", 1, 3, "b", 58, 59),
        ("image_gradient_upscale_1x", 2, 3, "b", 153, 152),
        ("image_gradient_upscale_1x", 3, 3, "b", 220, 221),
    ];

    fn cpu_reference(scenario: &Scenario) -> Vec<u8> {
        let mut canvas =
            RasterSurface::new(scenario.width, scenario.height, scale(scenario.factor)).unwrap();
        canvas.clear(Color::WHITE).unwrap();
        CpuExecutor.execute(&scenario.list, &mut canvas).unwrap();
        let bytes = canvas.data().to_vec();
        bytes
    }

    fn differences(name: &'static str, cpu: &[u8], gpu: &[u8], width: u32) -> Vec<Difference> {
        let channels = ["r", "g", "b", "a"];
        cpu.iter()
            .zip(gpu)
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(index, (&a, &b))| {
                let pixel = index as u32 / 4;
                Difference {
                    scenario: name,
                    x: pixel % width,
                    y: pixel / width,
                    channel: channels[index % 4],
                    cpu: a,
                    gpu: b,
                }
            })
            .collect()
    }

    #[test]
    fn gpu_output_matches_the_cpu_reference_except_for_the_recorded_differences() {
        let Some(mut executor) = executor_or_not_run("gpu_output_matches_the_cpu_reference") else {
            return;
        };
        eprintln!("adapter: {}", executor.adapter_info().name);
        let mut found = Vec::new();
        for scenario in scenarios() {
            let mut frame = executor
                .create_frame(scenario.width, scenario.height, scale(scenario.factor))
                .unwrap();
            executor.clear(&mut frame, Color::WHITE);
            executor.execute(&scenario.list, &mut frame).unwrap();
            let gpu = executor.read_pixels(&frame).unwrap();
            let cpu = cpu_reference(&scenario);
            assert_eq!(gpu.len(), cpu.len(), "{} size", scenario.name);
            let device_width = scenario.width * scenario.factor;
            found.extend(differences(scenario.name, &cpu, &gpu, device_width));
        }
        for diff in &found {
            eprintln!(
                "PARITY {} ({},{}) {} cpu {} gpu {}",
                diff.scenario, diff.x, diff.y, diff.channel, diff.cpu, diff.gpu
            );
        }
        let expected: Vec<Difference> = EXPECTED_DIFFERENCES
            .iter()
            .map(|&(scenario, x, y, channel, cpu, gpu)| Difference {
                scenario,
                x,
                y,
                channel,
                cpu,
                gpu,
            })
            .collect();
        assert_eq!(
            found, expected,
            "GPU output differs from the CPU reference somewhere not recorded"
        );
    }

    #[test]
    fn clear_matches_the_cpu_reference_for_a_translucent_colour() {
        let Some(mut executor) = executor_or_not_run("clear_matches_the_cpu_reference") else {
            return;
        };
        let colour = Color {
            red: 10,
            green: 200,
            blue: 99,
            alpha: 77,
        };
        let mut frame = executor.create_frame(5, 3, scale(2)).unwrap();
        executor.clear(&mut frame, colour);
        let gpu = executor.read_pixels(&frame).unwrap();
        let canvas = RasterSurface::new(5, 3, scale(2)).unwrap();
        canvas.clear(colour).unwrap();
        let cpu = canvas.data().to_vec();
        assert_eq!(
            differences("clear_translucent", &cpu, &gpu, 10),
            Vec::new(),
            "clear differs from the reference"
        );
    }

    /// Print NOT_RUN and return None when no adapter exists, so the device
    /// tests cannot pass silently. Set `SLOPOS_REQUIRE_WGPU_ADAPTER=1` to fail
    /// instead, on a machine that must have a GPU.
    fn executor_or_not_run(test: &str) -> Option<WgpuExecutor> {
        match WgpuExecutor::new_headless() {
            Ok(executor) => Some(executor),
            Err(error) => {
                // Written to stderr directly: libtest captures eprintln! from
                // passing tests, which would hide the NOT_RUN line.
                let _ = writeln!(std::io::stderr(), "NOT_RUN {test}: {error}");
                assert!(
                    std::env::var_os("SLOPOS_REQUIRE_WGPU_ADAPTER").is_none(),
                    "SLOPOS_REQUIRE_WGPU_ADAPTER is set but no adapter exists: {error}"
                );
                None
            }
        }
    }
}
