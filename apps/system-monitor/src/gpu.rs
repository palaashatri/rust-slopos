//! The GPU the monitor reports. The name and backend come from the wgpu
//! adapter that `slopos-ui` opens, and only when that crate is built with
//! its `gpu` feature. Otherwise the view says why it has nothing to show.

/// What the GPU view shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpuInfo {
    Adapter { name: String, backend: String },
    Unavailable(String),
}

/// Ask the graphics stack for an adapter. Blocking, and done once.
pub fn probe() -> GpuInfo {
    #[cfg(feature = "gpu")]
    {
        match slopos_ui::render::gpu::WgpuExecutor::new_headless() {
            Ok(executor) => {
                let info = executor.adapter_info();
                GpuInfo::Adapter {
                    name: info.name.clone(),
                    backend: format!("{:?}", info.backend),
                }
            }
            Err(error) => GpuInfo::Unavailable(format!("no usable adapter: {error}")),
        }
    }
    #[cfg(not(feature = "gpu"))]
    {
        GpuInfo::Unavailable("this build has no GPU support (the gpu feature is off)".into())
    }
}
