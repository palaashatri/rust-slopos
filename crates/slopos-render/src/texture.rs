use crate::Result;
use wgpu::{
    Device, Extent3d, ImageCopyTexture, Origin3d, Queue, TextureDescriptor, TextureDimension,
    TextureFormat, TextureUsages,
};

pub struct Texture {
    pub texture: wgpu::Texture,
    pub width: u32,
    pub height: u32,
    pub format: TextureFormat,
}

impl Texture {
    pub fn new(device: &Device, width: u32, height: u32, format: TextureFormat) -> Self {
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("retro texture"),
            size: Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format,
            usage: TextureUsages::TEXTURE_BINDING
                | TextureUsages::RENDER_ATTACHMENT
                | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        Self {
            texture,
            width,
            height,
            format,
        }
    }

    pub fn from_bytes(
        device: &Device,
        queue: &Queue,
        bytes: &[u8],
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("retro texture from bytes"),
            size: Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8UnormSrgb,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytes,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        Ok(Self {
            texture,
            width,
            height,
            format: TextureFormat::Rgba8UnormSrgb,
        })
    }

    pub fn create_view(&self) -> wgpu::TextureView {
        self.texture.create_view(&Default::default())
    }

    pub fn format(&self) -> TextureFormat {
        self.format
    }
}
