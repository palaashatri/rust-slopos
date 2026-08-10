use crate::Extent2D;

pub struct Surface {
    pub texture_view: wgpu::TextureView,
    pub extent: Extent2D,
}

impl Surface {
    pub fn new(texture_view: wgpu::TextureView, width: u32, height: u32) -> Self {
        Self {
            texture_view,
            extent: Extent2D { width, height },
        }
    }

    pub fn width(&self) -> u32 {
        self.extent.width
    }
    pub fn height(&self) -> u32 {
        self.extent.height
    }
}
