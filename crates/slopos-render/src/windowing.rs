use crate::Renderer;
use wgpu::Surface;
use winit::window::Window as WinitWindow;

pub struct WindowHandle {
    pub window: WinitWindow,
    pub surface: Surface<'static>,
    pub renderer: Renderer,
}

impl WindowHandle {
    pub fn new(window: WinitWindow, surface: Surface<'static>, renderer: Renderer) -> Self {
        Self {
            window,
            surface,
            renderer,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.renderer.resize(&self.surface, width, height);
    }

    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }
}
