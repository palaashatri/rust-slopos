//! `ext-session-lock-v1` lock client for SLOPOS-I.
//!
//! Password auth uses `SLOPOS_LOCK_PASSWORD` or `lock_password` in
//! `~/.config/slopos-i/settings.conf` only — no PAM in this cycle.

#[allow(dead_code)]
mod lock_geometry {
    pub const BITMAP_GLYPH_ADVANCE: u32 = 8;

    pub fn bitmap_text_width(text: &str) -> u32 {
        (text.chars().count() as u32).saturating_mul(BITMAP_GLYPH_ADVANCE)
    }

    pub fn centered_bitmap_text_x(width: u32, text: &str) -> u32 {
        width.saturating_sub(bitmap_text_width(text)) / 2
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::lock_geometry::{centered_bitmap_text_x, BITMAP_GLYPH_ADVANCE};
    use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
    use smithay_client_toolkit::delegate_compositor;
    use smithay_client_toolkit::delegate_keyboard;
    use smithay_client_toolkit::delegate_output;
    use smithay_client_toolkit::delegate_registry;
    use smithay_client_toolkit::delegate_seat;
    use smithay_client_toolkit::delegate_session_lock;
    use smithay_client_toolkit::delegate_shm;
    use smithay_client_toolkit::output::{OutputHandler, OutputState};
    use smithay_client_toolkit::reexports::calloop::EventLoop;
    use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
    use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
    use smithay_client_toolkit::registry_handlers;
    use smithay_client_toolkit::seat::keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers};
    use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
    use smithay_client_toolkit::session_lock::{
        SessionLock, SessionLockHandler, SessionLockState, SessionLockSurface,
        SessionLockSurfaceConfigure,
    };
    use smithay_client_toolkit::shm::{raw::RawPool, Shm, ShmHandler};
    use wayland_client::globals::registry_queue_init;
    use wayland_client::protocol::{
        wl_buffer, wl_keyboard, wl_output, wl_seat, wl_shm, wl_surface,
    };
    use wayland_client::{Connection, QueueHandle};

    struct LockApp {
        conn: Connection,
        compositor_state: CompositorState,
        output_state: OutputState,
        registry_state: RegistryState,
        seat_state: SeatState,
        shm: Shm,
        session_lock_state: SessionLockState,
        session_lock: Option<SessionLock>,
        lock_surfaces: Vec<SessionLockSurface>,
        keyboard: Option<wl_keyboard::WlKeyboard>,
        last_configure: Option<SessionLockSurfaceConfigure>,
        expected_password: String,
        entered_password: String,
        exit: bool,
    }

    impl LockApp {
        fn new(
            conn: Connection,
            globals: &wayland_client::globals::GlobalList,
            qh: &QueueHandle<Self>,
        ) -> Self {
            let expected_password = std::env::var("SLOPOS_LOCK_PASSWORD")
                .ok()
                .or_else(read_settings_password)
                .unwrap_or_else(|| "slopos-i".to_string());
            Self {
                conn,
                compositor_state: CompositorState::bind(globals, qh).expect("wl_compositor"),
                output_state: OutputState::new(globals, qh),
                registry_state: RegistryState::new(globals),
                seat_state: SeatState::new(globals, qh),
                shm: Shm::bind(globals, qh).expect("wl_shm"),
                session_lock_state: SessionLockState::new(globals, qh),
                session_lock: None,
                lock_surfaces: Vec::new(),
                keyboard: None,
                last_configure: None,
                expected_password,
                entered_password: String::new(),
                exit: false,
            }
        }

        fn try_unlock(&mut self) {
            if self.entered_password == self.expected_password {
                if let Some(lock) = self.session_lock.take() {
                    lock.unlock();
                }
                let _ = self.conn.roundtrip();
                self.exit = true;
            } else {
                self.entered_password.clear();
            }
        }

        fn repaint(&mut self, qh: &QueueHandle<Self>) {
            let Some(configure) = self.last_configure.clone() else {
                return;
            };
            if let Some(surface) = self.lock_surfaces.first() {
                paint_lock_surface(self, surface, configure, qh);
            }
        }
    }

    fn keysym_to_char(keysym: Keysym) -> Option<char> {
        let raw = keysym.raw();
        if (0x20..=0x7e).contains(&raw) {
            char::from_u32(raw)
        } else {
            None
        }
    }

    fn read_settings_password() -> Option<String> {
        let home = std::env::var("HOME").ok()?;
        let path = std::path::Path::new(&home).join(".config/slopos-i/settings.conf");
        let text = std::fs::read_to_string(path).ok()?;
        text.lines()
            .find_map(|line| line.strip_prefix("lock_password=").map(str::trim))
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    }

    fn paint_lock_surface(
        app: &LockApp,
        session_lock_surface: &SessionLockSurface,
        configure: SessionLockSurfaceConfigure,
        qh: &QueueHandle<LockApp>,
    ) {
        let (width, height) = configure.new_size;
        let w = width.max(1);
        let h = height.max(1);
        let mut pool = RawPool::new(w as usize * h as usize * 4, &app.shm).expect("shm pool");
        let canvas = pool.mmap();
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                canvas[i] = 0x28;
                canvas[i + 1] = 0x28;
                canvas[i + 2] = 0x30;
                canvas[i + 3] = 0xff;
            }
        }
        draw_prompt(canvas, w, h, &app.entered_password);

        let buffer = pool.create_buffer(
            0,
            w as i32,
            h as i32,
            (w * 4) as i32,
            wl_shm::Format::Argb8888,
            (),
            qh,
        );
        session_lock_surface
            .wl_surface()
            .attach(Some(&buffer), 0, 0);
        session_lock_surface.wl_surface().commit();
        buffer.destroy();
    }

    fn draw_prompt(pixels: &mut [u8], width: u32, height: u32, password: &str) {
        let label = "Enter password:";
        let y = height / 2;
        draw_text(
            pixels,
            width,
            centered_bitmap_text_x(width, label),
            y.saturating_sub(24),
            label,
            0xff,
            0xff,
            0xff,
        );
        let stars: String = std::iter::repeat_n('*', password.chars().count()).collect();
        draw_text(
            pixels,
            width,
            centered_bitmap_text_x(width, &stars),
            y.saturating_add(8),
            &stars,
            0xff,
            0xe0,
            0x60,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_text(
        pixels: &mut [u8],
        width: u32,
        mut x: u32,
        y: u32,
        text: &str,
        r: u8,
        g: u8,
        b: u8,
    ) {
        for ch in text.chars() {
            draw_char(pixels, width, x, y, ch, r, g, b);
            x = x.saturating_add(BITMAP_GLYPH_ADVANCE);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_char(pixels: &mut [u8], width: u32, x: u32, y: u32, ch: char, r: u8, g: u8, b: u8) {
        let stride = width * 4;
        let glyph = simple_glyph(ch);
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..5 {
                if bits & (1 << (4 - col)) != 0 {
                    let px = x + col as u32;
                    let py = y + row as u32;
                    let i = (py * stride + px * 4) as usize;
                    if i + 3 < pixels.len() {
                        pixels[i] = b;
                        pixels[i + 1] = g;
                        pixels[i + 2] = r;
                        pixels[i + 3] = 0xff;
                    }
                }
            }
        }
    }

    fn simple_glyph(ch: char) -> [u8; 7] {
        match ch {
            'A' => [0x0e, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
            'E' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x1f],
            'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
            'P' => [0x1e, 0x11, 0x11, 0x1e, 0x10, 0x10, 0x10],
            'R' => [0x1e, 0x11, 0x11, 0x1e, 0x14, 0x12, 0x11],
            'S' => [0x0f, 0x10, 0x10, 0x0e, 0x01, 0x01, 0x1e],
            'T' => [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
            'a' => [0x00, 0x00, 0x0e, 0x01, 0x0f, 0x11, 0x0f],
            'd' => [0x01, 0x01, 0x0d, 0x13, 0x11, 0x13, 0x0d],
            'e' => [0x00, 0x00, 0x0e, 0x11, 0x1f, 0x10, 0x0e],
            'n' => [0x00, 0x00, 0x1a, 0x15, 0x11, 0x11, 0x11],
            'o' => [0x00, 0x00, 0x0e, 0x11, 0x11, 0x11, 0x0e],
            'p' => [0x00, 0x00, 0x1e, 0x11, 0x1e, 0x10, 0x10],
            'r' => [0x00, 0x00, 0x16, 0x19, 0x10, 0x10, 0x10],
            's' => [0x00, 0x00, 0x0f, 0x10, 0x0e, 0x01, 0x1e],
            't' => [0x04, 0x04, 0x0e, 0x04, 0x04, 0x05, 0x02],
            'w' => [0x00, 0x00, 0x11, 0x11, 0x15, 0x15, 0x0a],
            ':' => [0x00, 0x04, 0x00, 0x00, 0x00, 0x04, 0x00],
            '*' => [0x00, 0x04, 0x15, 0x0e, 0x15, 0x04, 0x00],
            ' ' => [0; 7],
            _ => [0x1f, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1f],
        }
    }

    impl SessionLockHandler for LockApp {
        fn locked(
            &mut self,
            _conn: &Connection,
            qh: &QueueHandle<Self>,
            session_lock: SessionLock,
        ) {
            for output in self.output_state.outputs() {
                let surface = self.compositor_state.create_surface(qh);
                let lock_surface = session_lock.create_lock_surface(surface, &output, qh);
                self.lock_surfaces.push(lock_surface);
            }
        }

        fn finished(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _session_lock: SessionLock,
        ) {
            self.exit = true;
        }

        fn configure(
            &mut self,
            _conn: &Connection,
            qh: &QueueHandle<Self>,
            session_lock_surface: SessionLockSurface,
            configure: SessionLockSurfaceConfigure,
            _serial: u32,
        ) {
            self.last_configure = Some(configure.clone());
            paint_lock_surface(self, &session_lock_surface, configure, qh);
        }
    }
    delegate_session_lock!(LockApp);

    impl CompositorHandler for LockApp {
        fn scale_factor_changed(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _surface: &wl_surface::WlSurface,
            _new_factor: i32,
        ) {
        }

        fn transform_changed(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _surface: &wl_surface::WlSurface,
            _new_transform: wl_output::Transform,
        ) {
        }

        fn frame(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _surface: &wl_surface::WlSurface,
            _time: u32,
        ) {
        }

        fn surface_enter(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _surface: &wl_surface::WlSurface,
            _output: &wl_output::WlOutput,
        ) {
        }

        fn surface_leave(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _surface: &wl_surface::WlSurface,
            _output: &wl_output::WlOutput,
        ) {
        }
    }
    delegate_compositor!(LockApp);

    impl OutputHandler for LockApp {
        fn output_state(&mut self) -> &mut OutputState {
            &mut self.output_state
        }

        fn new_output(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _output: wl_output::WlOutput,
        ) {
        }

        fn update_output(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _output: wl_output::WlOutput,
        ) {
        }

        fn output_destroyed(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _output: wl_output::WlOutput,
        ) {
        }
    }
    delegate_output!(LockApp);

    impl SeatHandler for LockApp {
        fn seat_state(&mut self) -> &mut SeatState {
            &mut self.seat_state
        }

        fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

        fn new_capability(
            &mut self,
            _conn: &Connection,
            qh: &QueueHandle<Self>,
            seat: wl_seat::WlSeat,
            capability: Capability,
        ) {
            if capability == Capability::Keyboard && self.keyboard.is_none() {
                let keyboard = self
                    .seat_state
                    .get_keyboard(qh, &seat, None)
                    .expect("keyboard capability");
                self.keyboard = Some(keyboard);
            }
        }

        fn remove_capability(
            &mut self,
            _conn: &Connection,
            _: &QueueHandle<Self>,
            _: wl_seat::WlSeat,
            capability: Capability,
        ) {
            if capability == Capability::Keyboard {
                if let Some(keyboard) = self.keyboard.take() {
                    keyboard.release();
                }
            }
        }

        fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    }
    delegate_seat!(LockApp);

    impl KeyboardHandler for LockApp {
        fn enter(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _keyboard: &wl_keyboard::WlKeyboard,
            _surface: &wl_surface::WlSurface,
            _serial: u32,
            _raw: &[u32],
            _keysyms: &[Keysym],
        ) {
        }

        fn leave(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _keyboard: &wl_keyboard::WlKeyboard,
            _surface: &wl_surface::WlSurface,
            _serial: u32,
        ) {
        }

        fn press_key(
            &mut self,
            _conn: &Connection,
            qh: &QueueHandle<Self>,
            _keyboard: &wl_keyboard::WlKeyboard,
            _serial: u32,
            event: KeyEvent,
        ) {
            if event.keysym == Keysym::Return || event.keysym == Keysym::KP_Enter {
                self.try_unlock();
                return;
            }
            if event.keysym == Keysym::BackSpace {
                self.entered_password.pop();
                self.repaint(qh);
                return;
            }
            if let Some(text) = event.utf8 {
                if !text.chars().all(|c| c.is_control()) {
                    self.entered_password.push_str(&text);
                    self.repaint(qh);
                    return;
                }
            }
            if let Some(ch) = keysym_to_char(event.keysym) {
                self.entered_password.push(ch);
                self.repaint(qh);
            }
        }

        fn release_key(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _keyboard: &wl_keyboard::WlKeyboard,
            _serial: u32,
            _event: KeyEvent,
        ) {
        }

        fn update_modifiers(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _keyboard: &wl_keyboard::WlKeyboard,
            _serial: u32,
            _modifiers: Modifiers,
            _layout: u32,
        ) {
        }
    }
    delegate_keyboard!(LockApp);

    impl ProvidesRegistryState for LockApp {
        fn registry(&mut self) -> &mut RegistryState {
            &mut self.registry_state
        }
        registry_handlers![OutputState, SeatState];
    }
    delegate_registry!(LockApp);

    impl ShmHandler for LockApp {
        fn shm_state(&mut self) -> &mut Shm {
            &mut self.shm
        }
    }
    delegate_shm!(LockApp);

    wayland_client::delegate_noop!(LockApp: ignore wl_buffer::WlBuffer);

    pub fn main() -> anyhow::Result<()> {
        let conn = Connection::connect_to_env()?;
        let (globals, event_queue) = registry_queue_init(&conn)?;
        let qh = event_queue.handle();

        let mut event_loop = EventLoop::try_new()?;
        let mut app = LockApp::new(conn.clone(), &globals, &qh);

        app.session_lock = Some(
            app.session_lock_state
                .lock(&qh)
                .map_err(|e| anyhow::anyhow!("ext-session-lock unavailable: {e}"))?,
        );

        WaylandSource::new(conn, event_queue)
            .insert(event_loop.handle())
            .map_err(|e| anyhow::anyhow!("WaylandSource: {e}"))?;

        while !app.exit {
            event_loop
                .dispatch(std::time::Duration::from_millis(16), &mut app)
                .map_err(|e| anyhow::anyhow!("dispatch: {e}"))?;
        }

        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn main() -> anyhow::Result<()> {
    linux::main()
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("slopos-lock is only supported on Linux.");
}

#[cfg(test)]
mod tests {
    use super::lock_geometry::{bitmap_text_width, centered_bitmap_text_x, BITMAP_GLYPH_ADVANCE};

    #[test]
    fn bitmap_text_measurement_uses_rendered_character_advances() {
        assert_eq!(
            bitmap_text_width("Enter password:"),
            15 * BITMAP_GLYPH_ADVANCE
        );
        assert_eq!(bitmap_text_width("日本語"), 3 * BITMAP_GLYPH_ADVANCE);
    }

    #[test]
    fn bitmap_text_centering_uses_character_width_not_utf8_bytes() {
        let width = 800;
        assert_eq!(
            centered_bitmap_text_x(width, "Enter password:"),
            (width - 15 * BITMAP_GLYPH_ADVANCE) / 2
        );
        assert_eq!(
            centered_bitmap_text_x(width, "日本語"),
            (width - 3 * BITMAP_GLYPH_ADVANCE) / 2
        );
    }
}
