//! The X11 side of the Screenshot app. Every X request the app makes is in
//! this file. The rest of the app talks to [`Backend`], so its logic can be
//! tested without a display.

use image::RgbaImage;
use thiserror::Error;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ClientMessageEvent, ConnectionExt, EventMask, ImageFormat, ImageOrder,
};
use x11rb::rust_connection::RustConnection;

/// `WM_CLASS` of this app's windows, as passed to `WindowOptions::new`.
pub const APP_ID: &str = "slopos-screenshot";

/// ICCCM `IconicState`, requested with `WM_CHANGE_STATE`.
const ICONIC_STATE: u32 = 3;
/// `_NET_ACTIVE_WINDOW` source indication for a pager or taskbar acting for the user.
const PAGER_SOURCE: u32 = 2;
/// Enough parents to reach a frame under any sane window manager.
const MAX_FRAME_DEPTH: usize = 16;

x11rb::atom_manager! {
    Atoms: AtomsCookie {
        _NET_CLIENT_LIST,
        _NET_ACTIVE_WINDOW,
        WM_CHANGE_STATE,
    }
}

/// Where pixels and window control come from. [`X11Backend`] is the real one.
pub trait Backend {
    /// The whole screen as RGBA.
    fn grab_screen(&mut self) -> Result<RgbaImage, CaptureError>;
    /// The active top-level window, with its frame, as RGBA.
    fn grab_active_window(&mut self) -> Result<RgbaImage, CaptureError>;
    /// Hides this app's windows and returns what to show again.
    fn hide_own_windows(&mut self) -> Vec<u32>;
    /// Shows windows returned by [`Backend::hide_own_windows`] and focuses them.
    fn show_windows(&mut self, windows: &[u32]);
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CaptureError {
    #[error("the X display is unavailable ({0})")]
    Display(String),
    #[error("the screen uses a pixel format this app cannot read")]
    Format,
    #[error("no window is active")]
    NoActiveWindow,
    #[error("the Screenshot window is the active one")]
    OwnWindowActive,
    #[error("the X server failed ({0})")]
    Protocol(String),
}

fn protocol(error: impl std::fmt::Display) -> CaptureError {
    CaptureError::Protocol(error.to_string())
}

/// The real backend. Each call opens a short connection, because captures
/// are rare and a long-lived connection would only sit idle. `display` is
/// `None` for `$DISPLAY`; tests name another display to check the failure path.
#[derive(Default)]
pub struct X11Backend {
    display: Option<String>,
}

#[cfg(test)]
impl X11Backend {
    /// A backend that connects to `display` (such as `:1`) instead of `$DISPLAY`.
    pub fn on_display(display: &str) -> Self {
        Self {
            display: Some(display.to_owned()),
        }
    }
}

impl Backend for X11Backend {
    fn grab_screen(&mut self) -> Result<RgbaImage, CaptureError> {
        let session = Session::open(self.display.as_deref())?;
        session.image(session.root, 0, 0, session.width, session.height)
    }

    fn grab_active_window(&mut self) -> Result<RgbaImage, CaptureError> {
        let session = Session::open(self.display.as_deref())?;
        let client = session
            .active_client()?
            .ok_or(CaptureError::NoActiveWindow)?;
        if session.is_app_window(client)? {
            return Err(CaptureError::OwnWindowActive);
        }
        let frame = session.frame_of(client)?;
        let geometry = session
            .conn
            .get_geometry(frame)
            .map_err(protocol)?
            .reply()
            .map_err(protocol)?;
        session.image(frame, 0, 0, geometry.width, geometry.height)
    }

    fn hide_own_windows(&mut self) -> Vec<u32> {
        let Ok(session) = Session::open(self.display.as_deref()) else {
            return Vec::new();
        };
        let windows = session.app_windows();
        for &window in &windows {
            let _ = session.send_to_root(window, session.atoms.WM_CHANGE_STATE, ICONIC_STATE);
        }
        let _ = session.conn.flush();
        windows
    }

    fn show_windows(&mut self, windows: &[u32]) {
        let Ok(session) = Session::open(self.display.as_deref()) else {
            return;
        };
        for &window in windows {
            let _ = session.send_to_root(window, session.atoms._NET_ACTIVE_WINDOW, PAGER_SOURCE);
        }
        let _ = session.conn.flush();
    }
}

struct Session {
    conn: RustConnection,
    root: u32,
    width: u16,
    height: u16,
    atoms: Atoms,
}

impl Session {
    fn open(display: Option<&str>) -> Result<Self, CaptureError> {
        let (conn, screen_number) =
            x11rb::connect(display).map_err(|error| CaptureError::Display(error.to_string()))?;
        let (root, width, height) = {
            let screen = &conn.setup().roots[screen_number];
            (screen.root, screen.width_in_pixels, screen.height_in_pixels)
        };
        let atoms = Atoms::new(&conn)
            .map_err(protocol)?
            .reply()
            .map_err(protocol)?;
        Ok(Self {
            conn,
            root,
            width,
            height,
            atoms,
        })
    }

    /// Reads a rectangle of `drawable` as RGBA. Only 32-bit little-endian
    /// pixels (24-bit colour in 32-bit cells) are supported.
    fn image(
        &self,
        drawable: u32,
        x: i16,
        y: i16,
        width: u16,
        height: u16,
    ) -> Result<RgbaImage, CaptureError> {
        let reply = self
            .conn
            .get_image(
                ImageFormat::Z_PIXMAP,
                drawable,
                x,
                y,
                width,
                height,
                u32::MAX,
            )
            .map_err(protocol)?
            .reply()
            .map_err(protocol)?;
        let bits_per_pixel = self
            .conn
            .setup()
            .pixmap_formats
            .iter()
            .find(|format| format.depth == reply.depth)
            .map(|format| format.bits_per_pixel);
        let supported_depth = reply.depth == 24 || reply.depth == 32;
        if !supported_depth
            || bits_per_pixel != Some(32)
            || self.conn.setup().image_byte_order != ImageOrder::LSB_FIRST
        {
            return Err(CaptureError::Format);
        }
        to_rgba(u32::from(width), u32::from(height), &reply.data)
    }

    /// The client window the user has active, from `_NET_ACTIVE_WINDOW`.
    fn active_client(&self) -> Result<Option<u32>, CaptureError> {
        let active = self
            .cardinals(self.root, self.atoms._NET_ACTIVE_WINDOW, AtomEnum::WINDOW)?
            .into_iter()
            .find(|&window| window != 0);
        Ok(active)
    }

    /// The top-level window under the root that contains `client`: the window
    /// manager's frame when there is one.
    fn frame_of(&self, client: u32) -> Result<u32, CaptureError> {
        let mut window = client;
        for _ in 0..MAX_FRAME_DEPTH {
            let tree = self
                .conn
                .query_tree(window)
                .map_err(protocol)?
                .reply()
                .map_err(protocol)?;
            if tree.parent == 0 || tree.parent == self.root {
                return Ok(window);
            }
            window = tree.parent;
        }
        Ok(window)
    }

    fn is_app_window(&self, client: u32) -> Result<bool, CaptureError> {
        let reply = self
            .conn
            .get_property(false, client, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 256)
            .map_err(protocol)?
            .reply()
            .map_err(protocol)?;
        Ok(reply
            .value
            .split(|byte| *byte == 0)
            .any(|part| part == APP_ID.as_bytes()))
    }

    /// This app's client windows, from the window manager's list.
    fn app_windows(&self) -> Vec<u32> {
        self.cardinals(self.root, self.atoms._NET_CLIENT_LIST, AtomEnum::WINDOW)
            .unwrap_or_default()
            .into_iter()
            .filter(|&window| self.is_app_window(window).unwrap_or(false))
            .collect()
    }

    fn cardinals(
        &self,
        window: u32,
        property: u32,
        kind: impl Into<u32>,
    ) -> Result<Vec<u32>, CaptureError> {
        let reply = self
            .conn
            .get_property(false, window, property, kind.into(), 0, 4096)
            .map_err(protocol)?
            .reply()
            .map_err(protocol)?;
        Ok(reply.value32().map(Iterator::collect).unwrap_or_default())
    }

    /// Sends a client message to the root window, where the window manager
    /// listens for requests such as iconify and activate.
    fn send_to_root(&self, window: u32, message_type: u32, data0: u32) -> Result<(), CaptureError> {
        let message = ClientMessageEvent::new(32, window, message_type, [data0, 0, 0, 0, 0]);
        self.conn
            .send_event(
                false,
                self.root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                message,
            )
            .map_err(protocol)?;
        Ok(())
    }
}

/// Converts X `ZPixmap` data (BGRX, 4 bytes per pixel) to RGBA.
fn to_rgba(width: u32, height: u32, data: &[u8]) -> Result<RgbaImage, CaptureError> {
    let expected = width as usize * height as usize * 4;
    if data.len() < expected {
        return Err(CaptureError::Protocol(
            "the image is shorter than its size".into(),
        ));
    }
    let mut rgba = Vec::with_capacity(expected);
    for pixel in data[..expected].as_chunks::<4>().0 {
        rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 0xff]);
    }
    RgbaImage::from_raw(width, height, rgba)
        .ok_or_else(|| CaptureError::Protocol("the image has no pixels".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bgrx_pixels_become_opaque_rgba() {
        let data = [0x01, 0x02, 0x03, 0x00, 0x10, 0x20, 0x30, 0x00];
        let image = to_rgba(2, 1, &data).unwrap();
        assert_eq!(image.get_pixel(0, 0).0, [0x03, 0x02, 0x01, 0xff]);
        assert_eq!(image.get_pixel(1, 0).0, [0x30, 0x20, 0x10, 0xff]);
    }

    #[test]
    fn short_image_data_is_an_error_not_a_panic() {
        assert!(matches!(
            to_rgba(2, 2, &[0; 8]),
            Err(CaptureError::Protocol(_))
        ));
    }

    #[test]
    fn a_display_that_does_not_exist_is_a_display_error() {
        let mut backend = X11Backend::on_display(":65000");
        assert!(matches!(
            backend.grab_screen(),
            Err(CaptureError::Display(_))
        ));
        assert!(matches!(
            backend.grab_active_window(),
            Err(CaptureError::Display(_))
        ));
    }

    /// Needs a real X server. CI runs it under `xvfb-run -a` with `--ignored`;
    /// anywhere else it is reported as ignored, never as passed. One test covers
    /// every live case, so the cases do not race over the root window's
    /// `_NET_ACTIVE_WINDOW` property.
    #[test]
    #[ignore = "needs an X server: run under xvfb-run -a with --ignored"]
    fn live_capture_reads_the_screen_and_the_active_window() {
        use x11rb::protocol::xproto::{CreateWindowAux, PropMode, WindowClass};
        use x11rb::protocol::Event;
        use x11rb::{COPY_DEPTH_FROM_PARENT, COPY_FROM_PARENT};

        let (conn, screen_number) = x11rb::connect(None).expect("an X server (xvfb-run -a)");
        let (root, root_width, root_height) = {
            let screen = &conn.setup().roots[screen_number];
            (screen.root, screen.width_in_pixels, screen.height_in_pixels)
        };
        let active = conn
            .intern_atom(false, b"_NET_ACTIVE_WINDOW")
            .unwrap()
            .reply()
            .unwrap()
            .atom;
        let mut backend = X11Backend::default();

        // The whole screen is the size of the root window.
        let screen = backend.grab_screen().expect("screen capture");
        assert_eq!(
            screen.dimensions(),
            (u32::from(root_width), u32::from(root_height))
        );

        // Without _NET_ACTIVE_WINDOW there is no window to capture.
        conn.delete_property(root, active).unwrap();
        conn.flush().unwrap();
        assert!(matches!(
            backend.grab_active_window(),
            Err(CaptureError::NoActiveWindow)
        ));

        // A 64x48 green window that is made active is captured at its own size.
        let window = conn.generate_id().unwrap();
        conn.create_window(
            COPY_DEPTH_FROM_PARENT,
            window,
            root,
            10,
            10,
            64,
            48,
            0,
            WindowClass::INPUT_OUTPUT,
            COPY_FROM_PARENT,
            &CreateWindowAux::new()
                .background_pixel(0x00ff00)
                .event_mask(EventMask::EXPOSURE),
        )
        .unwrap();
        conn.map_window(window).unwrap();
        conn.flush().unwrap();
        // Wait for the server to paint the window before reading it.
        while !matches!(conn.wait_for_event().unwrap(), Event::Expose(_)) {}
        conn.change_property32(PropMode::REPLACE, root, active, AtomEnum::WINDOW, &[window])
            .unwrap();
        conn.flush().unwrap();
        let captured = backend.grab_active_window().expect("window capture");
        assert_eq!(captured.dimensions(), (64, 48));
        assert_eq!(captured.get_pixel(0, 0).0, [0, 255, 0, 255]);

        // A window whose class is this app is refused, not captured.
        conn.change_property8(
            PropMode::REPLACE,
            window,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            b"slopos-screenshot\0Screenshot\0",
        )
        .unwrap();
        conn.flush().unwrap();
        assert!(matches!(
            backend.grab_active_window(),
            Err(CaptureError::OwnWindowActive)
        ));
    }
}
