use wgpu::{Device, ShaderModule, ShaderSource};

pub struct Shader {
    pub module: ShaderModule,
}

impl Shader {
    pub fn from_wgsl(device: &Device, source: &str, label: &str) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(label),
            source: ShaderSource::Wgsl(std::borrow::Cow::Borrowed(source)),
        });
        Self { module }
    }
}
