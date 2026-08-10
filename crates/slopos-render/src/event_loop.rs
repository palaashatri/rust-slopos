use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::WindowId;

pub struct RetroEventLoop {
    pub event_loop: EventLoop<()>,
}

impl RetroEventLoop {
    /// Fails when no display server connection is available (e.g. no
    /// compositor behind WAYLAND_DISPLAY / DISPLAY); callers must not unwrap
    /// this — a missing compositor is an expected runtime condition.
    pub fn new() -> Result<Self, winit::error::EventLoopError> {
        let event_loop = EventLoop::new()?;
        Ok(Self { event_loop })
    }

    /// Return a wake handle for work arriving on a non-Wayland file
    /// descriptor, such as the SDK's application-menu control socket.
    pub fn proxy(&self) -> EventLoopProxy<()> {
        self.event_loop.create_proxy()
    }
}

pub trait RetroAppHandler {
    fn init(&mut self, event_loop: &ActiveEventLoop);
    fn handle_window_event(&mut self, event_loop: &ActiveEventLoop, event: WindowEvent);
    fn user_event(&mut self, _event_loop: &ActiveEventLoop) {}
    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {}
}

struct AppHandlerWrapper<'a, H: RetroAppHandler> {
    handler: &'a mut H,
}

impl<'a, H: RetroAppHandler> ApplicationHandler for AppHandlerWrapper<'a, H> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.handler.init(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        self.handler.handle_window_event(event_loop, event);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, _event: ()) {
        self.handler.user_event(event_loop);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.handler.about_to_wait(event_loop);
    }
}

impl RetroEventLoop {
    pub fn run<H: RetroAppHandler>(
        self,
        handler: &mut H,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let event_loop = self.event_loop;
        // Redraw requests and input/configure events wake the loop. Polling
        // here makes every SDK client spin through about_to_wait and reread
        // configuration even while its surface is idle.
        event_loop.set_control_flow(ControlFlow::Wait);
        let mut wrapper = AppHandlerWrapper { handler };
        event_loop.run_app(&mut wrapper)?;
        Ok(())
    }
}
